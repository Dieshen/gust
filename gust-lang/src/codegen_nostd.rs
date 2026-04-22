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
        let mut out = String::new();
        out.push_str("#![no_std]\n");
        out.push_str("extern crate alloc;\n");
        out.push_str("use heapless::{String as HString, Vec as HVec};\n\n");

        for machine in &program.machines {
            self.emit_machine(&mut out, machine);
            out.push('\n');
        }

        out
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
            out.push_str(&format!(
                "    pub fn {}(&mut self) -> Result<(), &'static str> {{\n",
                transition.name
            ));
            out.push_str("        match &self.state {\n");
            out.push_str(&format!("            {from_pattern} => {{\n"));
            out.push_str(&format!(
                "                self.state = {state_name}::{to};\n"
            ));
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
