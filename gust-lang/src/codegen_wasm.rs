use crate::ast::Program;
use crate::codegen::RustCodegen;

pub struct WasmCodegen;

impl WasmCodegen {
    pub fn new() -> Self {
        Self
    }

    pub fn generate(&self, program: &Program) -> String {
        let mut out = String::new();
        out.push_str("// Generated for wasm32 target\n");
        out.push_str("use wasm_bindgen::prelude::*;\n\n");
        out.push_str(&RustCodegen::new().generate(program));
        out
    }
}

impl Default for WasmCodegen {
    fn default() -> Self {
        Self::new()
    }
}
