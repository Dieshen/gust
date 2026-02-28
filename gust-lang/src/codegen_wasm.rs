use crate::ast::*;

pub struct WasmCodegen;

impl WasmCodegen {
    pub fn new() -> Self {
        Self
    }

    pub fn generate(&self, program: &Program) -> String {
        let mut out = String::new();
        out.push_str("// Generated for wasm32 target\n");
        out.push_str("use wasm_bindgen::prelude::*;\n");
        out.push_str("use wasm_bindgen_futures::future_to_promise;\n");
        out.push_str("use js_sys::{Array, Promise};\n\n");
        out.push_str("pub trait GustWasmEffectAdapter {\n");
        out.push_str("    fn call_effect(&self, name: &str, args: Array) -> Promise;\n");
        out.push_str("}\n\n");

        for ty in &program.types {
            self.emit_type_decl(&mut out, ty);
            out.push('\n');
        }
        for machine in &program.machines {
            self.emit_machine(&mut out, machine);
            out.push('\n');
        }

        out
    }

    fn emit_type_decl(&self, out: &mut String, decl: &TypeDecl) {
        match decl {
            TypeDecl::Struct { name, fields } => {
                out.push_str("#[wasm_bindgen]\n");
                out.push_str(&format!("pub struct {name} {{\n"));
                for field in fields {
                    out.push_str(&format!(
                        "    pub {}: {},\n",
                        field.name,
                        self.wasm_type(&field.ty)
                    ));
                }
                out.push_str("}\n");
            }
            TypeDecl::Enum { name, variants } => {
                out.push_str("#[wasm_bindgen]\n");
                out.push_str("#[repr(u32)]\n");
                out.push_str(&format!("pub enum {name} {{\n"));
                for (idx, variant) in variants.iter().enumerate() {
                    let _ = &variant.payload;
                    out.push_str(&format!("    {} = {},\n", variant.name, idx));
                }
                out.push_str("}\n");
            }
        }
    }

    fn emit_machine(&self, out: &mut String, machine: &MachineDecl) {
        let generic_decl = wasm_generic_decl(&machine.generic_params);
        let generic_use = wasm_generic_use(&machine.generic_params);
        let state_name = format!("{}State", machine.name);

        out.push_str("#[wasm_bindgen]\n");
        out.push_str("#[repr(u32)]\n");
        out.push_str(&format!("pub enum {state_name}{generic_decl} {{\n"));
        for (idx, state) in machine.states.iter().enumerate() {
            out.push_str(&format!("    {} = {},\n", state.name, idx));
        }
        out.push_str("}\n\n");

        out.push_str("#[wasm_bindgen]\n");
        out.push_str(&format!("pub struct {}{generic_decl} {{\n", machine.name));
        out.push_str(&format!("    state: {state_name}{generic_use},\n"));
        out.push_str("}\n\n");

        out.push_str("#[wasm_bindgen]\n");
        out.push_str(&format!("impl {}{generic_decl} {{\n", machine.name));
        if let Some(first) = machine.states.first() {
            out.push_str("    #[wasm_bindgen(constructor)]\n");
            out.push_str(&format!(
                "    pub fn new() -> {}{generic_use} {{\n",
                machine.name
            ));
            out.push_str(&format!(
                "        Self {{ state: {state_name}::{} }}\n",
                first.name
            ));
            out.push_str("    }\n\n");
        }
        out.push_str("    #[wasm_bindgen(js_name = state)]\n");
        out.push_str("    pub fn state(&self) -> u32 {\n");
        out.push_str("        self.state as u32\n");
        out.push_str("    }\n\n");

        for transition in &machine.transitions {
            let method = &transition.name;
            let from = &transition.from;
            let to = transition
                .targets
                .first()
                .cloned()
                .unwrap_or_else(|| from.clone());

            if transition.timeout.is_some() {
                out.push_str(&format!("    #[wasm_bindgen(js_name = {}Async)]\n", method));
                out.push_str(&format!(
                    "    pub fn {method}_async(&mut self) -> Promise {{\n"
                ));
                out.push_str(&format!(
                    "        if self.state as u32 != {state_name}::{from} as u32 {{\n"
                ));
                out.push_str("            return Promise::reject(&JsValue::from_str(\"invalid transition\"));\n");
                out.push_str("        }\n");
                out.push_str(&format!("        self.state = {state_name}::{to};\n"));
                out.push_str("        future_to_promise(async move { Ok(JsValue::UNDEFINED) })\n");
                out.push_str("    }\n\n");
            } else {
                out.push_str(&format!("    #[wasm_bindgen(js_name = {method})]\n"));
                out.push_str(&format!(
                    "    pub fn {method}(&mut self) -> Result<(), JsValue> {{\n"
                ));
                out.push_str(&format!(
                    "        if self.state as u32 != {state_name}::{from} as u32 {{\n"
                ));
                out.push_str(
                    "            return Err(JsValue::from_str(\"invalid transition\"));\n",
                );
                out.push_str("        }\n");
                out.push_str(&format!("        self.state = {state_name}::{to};\n"));
                out.push_str("        Ok(())\n");
                out.push_str("    }\n\n");
            }
        }

        out.push_str("}\n");
    }

    fn wasm_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Simple(name) => match name.as_str() {
                "String" => "String".to_string(),
                "i64" => "i64".to_string(),
                "i32" => "i32".to_string(),
                "u64" => "u64".to_string(),
                "u32" => "u32".to_string(),
                "f64" => "f64".to_string(),
                "f32" => "f32".to_string(),
                "bool" => "bool".to_string(),
                _ => "JsValue".to_string(),
            },
            TypeExpr::Generic(_, _) | TypeExpr::Tuple(_) => "JsValue".to_string(),
        }
    }
}

impl Default for WasmCodegen {
    fn default() -> Self {
        Self::new()
    }
}

fn wasm_generic_decl(params: &[GenericParam]) -> String {
    if params.is_empty() {
        return String::new();
    }
    let joined = params
        .iter()
        .map(|p| format!("{}: Into<JsValue> + Clone", p.name))
        .collect::<Vec<_>>()
        .join(", ");
    format!("<{joined}>")
}

fn wasm_generic_use(params: &[GenericParam]) -> String {
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
