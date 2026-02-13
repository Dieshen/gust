use crate::ast::Program;
use crate::codegen::RustCodegen;

pub struct NoStdCodegen;

impl NoStdCodegen {
    pub fn new() -> Self {
        Self
    }

    pub fn generate(&self, program: &Program) -> String {
        let mut out = String::new();
        out.push_str("#![no_std]\n");
        out.push_str("extern crate alloc;\n\n");
        out.push_str(&RustCodegen::new().generate(program));
        out
    }
}

impl Default for NoStdCodegen {
    fn default() -> Self {
        Self::new()
    }
}
