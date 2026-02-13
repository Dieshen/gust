use crate::ast::Program;

pub struct CffiCodegen;

impl CffiCodegen {
    pub fn new() -> Self {
        Self
    }

    pub fn generate(&self, program: &Program) -> (String, String) {
        let mut rust = String::new();
        let mut header = String::new();

        rust.push_str("// Generated C FFI bindings for Gust machine(s)\n");
        rust.push_str("use core::ffi::c_int;\n\n");

        header.push_str("#ifndef GUST_MACHINE_H\n");
        header.push_str("#define GUST_MACHINE_H\n\n");
        header.push_str("#ifdef __cplusplus\nextern \"C\" {\n#endif\n\n");

        for machine in &program.machines {
            let lower = machine.name.to_ascii_lowercase();
            header.push_str(&format!("typedef struct {} {} ;\n", machine.name, machine.name));
            header.push_str(&format!("{}* {}_new(void);\n", machine.name, lower));
            header.push_str(&format!("void {}_free({}* machine);\n", lower, machine.name));
            for transition in &machine.transitions {
                header.push_str(&format!("int {}_{}({}* machine);\n", lower, transition.name, machine.name));
            }
            header.push('\n');

            rust.push_str(&format!("pub struct {} {{}}\n", machine.name));
            rust.push_str("#[no_mangle]\n");
            rust.push_str(&format!("pub unsafe extern \"C\" fn {}_new() -> *mut {} {{ core::ptr::null_mut() }}\n", lower, machine.name));
            rust.push_str("#[no_mangle]\n");
            rust.push_str(&format!("pub unsafe extern \"C\" fn {}_free(_machine: *mut {}) {{}}\n", lower, machine.name));
            for transition in &machine.transitions {
                rust.push_str("#[no_mangle]\n");
                rust.push_str(&format!(
                    "pub unsafe extern \"C\" fn {}_{}(_machine: *mut {}) -> c_int {{ 0 }}\n",
                    lower, transition.name, machine.name
                ));
            }
            rust.push('\n');
        }

        header.push_str("#ifdef __cplusplus\n}\n#endif\n\n#endif\n");
        (rust, header)
    }
}

impl Default for CffiCodegen {
    fn default() -> Self {
        Self::new()
    }
}
