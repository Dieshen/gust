use crate::ast::*;

/// `no_std` code generator. Emits Rust backed by `heapless` collections
/// for embedded and resource-constrained targets.
pub struct NoStdCodegen;

impl NoStdCodegen {
    /// Construct a new no_std code generator.
    pub fn new() -> Self {
        Self
    }

    /// Generate the full `.g.nostd.rs` source for `program`.
    pub fn generate(&self, program: &Program) -> String {
        // Body first, prelude second: which heapless aliases are needed cannot
        // be known until the body exists. Importing both unconditionally left
        // an `unused import: Vec as HVec` in any program that used only one —
        // a hard error for a consumer building with `-D warnings`, in a file
        // they are told never to edit. Same shape as the `use tokio;` defect.
        let mut body = String::new();

        // State enums reference user-declared types by name, so the types have
        // to be emitted too. They were not, which meant any machine whose
        // states carried a `type` field produced output that could not compile.
        for ty in &program.types {
            self.emit_type_decl(&mut body, ty);
            body.push('\n');
        }

        for machine in &program.machines {
            self.emit_machine(&mut body, machine);
            body.push('\n');
        }

        let mut out = String::new();
        out.push_str("#![no_std]\n");
        out.push_str("extern crate alloc;\n");
        let mut aliases = Vec::new();
        if body.contains("HString") {
            aliases.push("String as HString");
        }
        if body.contains("HVec") {
            aliases.push("Vec as HVec");
        }
        match aliases.len() {
            0 => out.push('\n'),
            1 => out.push_str(&format!("use heapless::{};\n\n", aliases[0])),
            _ => out.push_str(&format!("use heapless::{{{}}};\n\n", aliases.join(", "))),
        }
        out.push_str(&body);

        out
    }

    fn emit_type_decl(&self, out: &mut String, decl: &TypeDecl) {
        match decl {
            TypeDecl::Struct { name, fields, .. } => {
                out.push_str(&format!("pub struct {name} {{\n"));
                for field in fields {
                    out.push_str(&format!(
                        "    pub {}: {},\n",
                        field.name,
                        self.nostd_type(&field.ty)
                    ));
                }
                out.push_str("}\n");
            }
            TypeDecl::Enum { name, variants, .. } => {
                out.push_str(&format!("pub enum {name} {{\n"));
                for variant in variants {
                    if variant.payload.is_empty() {
                        out.push_str(&format!("    {},\n", variant.name));
                    } else {
                        let payload = variant
                            .payload
                            .iter()
                            .map(|t| self.nostd_type(t))
                            .collect::<Vec<_>>()
                            .join(", ");
                        out.push_str(&format!("    {}({payload}),\n", variant.name));
                    }
                }
                out.push_str("}\n");
            }
        }
    }

    fn emit_machine(&self, out: &mut String, machine: &MachineDecl) {
        let generic_decl = nostd_generic_decl(&machine.generic_params);
        let generic_use = nostd_generic_use(&machine.generic_params);
        let state_name = format!("{}State", machine.name);

        out.push_str(&format!("pub enum {state_name}{generic_decl} {{\n"));
        for state in &machine.states {
            if state.fields.is_empty() {
                out.push_str(&format!("    {},\n", state.name));
            } else {
                out.push_str(&format!("    {} {{\n", state.name));
                for field in &state.fields {
                    out.push_str(&format!(
                        "        {}: {},\n",
                        field.name,
                        self.nostd_type(&field.ty)
                    ));
                }
                out.push_str("    },\n");
            }
        }
        out.push_str("}\n\n");

        out.push_str(&format!("pub struct {}{generic_decl} {{\n", machine.name));
        out.push_str(&format!("    pub state: {state_name}{generic_use},\n"));
        out.push_str("}\n\n");

        out.push_str(&format!(
            "impl{generic_decl} {}{generic_use} {{\n",
            machine.name
        ));
        if let Some(first) = machine.states.first() {
            if first.fields.is_empty() {
                out.push_str("    pub fn new() -> Self {\n");
                out.push_str(&format!(
                    "        Self {{ state: {state_name}::{} }}\n",
                    first.name
                ));
                out.push_str("    }\n\n");
            } else {
                let params = first
                    .fields
                    .iter()
                    .map(|f| format!("{}: {}", f.name, self.nostd_type(&f.ty)))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&format!("    pub fn new({params}) -> Self {{\n"));
                let field_names = first
                    .fields
                    .iter()
                    .map(|f| f.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&format!(
                    "        Self {{ state: {state_name}::{} {{ {} }} }}\n",
                    first.name, field_names
                ));
                out.push_str("    }\n\n");
            }
        }

        for transition in &machine.transitions {
            let to = transition
                .targets
                .first()
                .cloned()
                .unwrap_or_else(|| transition.from.clone());
            let from_state = machine.states.iter().find(|s| s.name == transition.from);
            let from_pattern = if from_state.map(|s| s.fields.is_empty()).unwrap_or(true) {
                format!("{state_name}::{}", transition.from)
            } else {
                format!("{state_name}::{} {{ .. }}", transition.from)
            };

            // A target state that carries fields is a struct variant, so it
            // cannot be named as a bare value. This backend emits no handler
            // bodies and has no effects trait, so the field values have to come
            // from the caller — the same shape the constructor already uses for
            // the initial state. Previously this emitted
            // `self.state = State::Variant;`, which does not compile. See #103.
            let to_state = machine.states.iter().find(|s| s.name == to);
            let to_fields = to_state.map(|s| s.fields.as_slice()).unwrap_or(&[]);
            let params = to_fields
                .iter()
                .map(|f| format!(", {}: {}", f.name, self.nostd_type(&f.ty)))
                .collect::<Vec<_>>()
                .join("");
            let construction = if to_fields.is_empty() {
                format!("{state_name}::{to}")
            } else {
                let names = to_fields
                    .iter()
                    .map(|f| f.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{state_name}::{to} {{ {names} }}")
            };

            out.push_str(&format!(
                "    pub fn {}(&mut self{params}) -> Result<(), &'static str> {{\n",
                transition.name
            ));
            out.push_str("        match &self.state {\n");
            out.push_str(&format!("            {from_pattern} => {{\n"));
            out.push_str(&format!("                self.state = {construction};\n"));
            out.push_str("                Ok(())\n");
            out.push_str("            }\n");
            out.push_str("            _ => Err(\"invalid transition\"),\n");
            out.push_str("        }\n");
            out.push_str("    }\n\n");
        }

        out.push_str("}\n");
    }

    fn nostd_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Unit => "()".to_string(),
            TypeExpr::Simple(name) => match name.as_str() {
                "String" => "HString<64>".to_string(),
                "i64" | "i32" | "u64" | "u32" | "f64" | "f32" | "bool" => name.clone(),
                other => other.to_string(),
            },
            TypeExpr::Generic(name, args) => match name.as_str() {
                "Vec" => {
                    let inner = args
                        .first()
                        .map(|a| self.nostd_type(a))
                        .unwrap_or_else(|| "u8".to_string());
                    format!("HVec<{inner}, 16>")
                }
                "Option" => {
                    let inner = args
                        .first()
                        .map(|a| self.nostd_type(a))
                        .unwrap_or_else(|| "u8".to_string());
                    format!("Option<{inner}>")
                }
                "Result" => {
                    let ok = args
                        .first()
                        .map(|a| self.nostd_type(a))
                        .unwrap_or_else(|| "u8".to_string());
                    let err = args
                        .get(1)
                        .map(|a| self.nostd_type(a))
                        .unwrap_or_else(|| "u8".to_string());
                    format!("Result<{ok}, {err}>")
                }
                other => {
                    let mapped = args
                        .iter()
                        .map(|a| self.nostd_type(a))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("{other}<{mapped}>")
                }
            },
            TypeExpr::Tuple(types) => {
                let inner = types
                    .iter()
                    .map(|t| self.nostd_type(t))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("({inner})")
            }
        }
    }
}

impl Default for NoStdCodegen {
    fn default() -> Self {
        Self::new()
    }
}

fn nostd_generic_decl(params: &[GenericParam]) -> String {
    if params.is_empty() {
        return String::new();
    }
    let joined = params
        .iter()
        .map(|p| format!("{}: Clone", p.name))
        .collect::<Vec<_>>()
        .join(", ");
    format!("<{joined}>")
}

fn nostd_generic_use(params: &[GenericParam]) -> String {
    if params.is_empty() {
        return String::new();
    }
    let joined = params
        .iter()
        .map(|p| p.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!("<{joined}>")
}
