//! C FFI code generator for Gust state machines.
//!
//! Produces a pair of outputs: Rust source with `#[no_mangle] extern "C"`
//! functions, and a C header file (`.h`) with the corresponding declarations.
//! This allows Gust machines to be used from C, C++, or any language with
//! C FFI support.

use crate::ast::Program;

/// C FFI code generator.
///
/// Produces both Rust FFI glue and a C header file. Each machine generates:
///
/// - A `#[repr(C)]` state enum and handle struct.
/// - `extern "C"` functions for creation, destruction, state query, and
///   each transition.
/// - A matching `.h` header with `typedef` enums, opaque handle types,
///   and function prototypes.
pub struct CffiCodegen;

impl CffiCodegen {
    /// Create a new C FFI code generator.
    pub fn new() -> Self {
        Self
    }

    /// Generate C FFI bindings from a [`Program`] AST.
    ///
    /// Returns a tuple `(rust_source, c_header)` where:
    /// - `rust_source` contains `#[no_mangle] extern "C"` functions.
    /// - `c_header` contains the corresponding C header declarations.
    pub fn generate(&self, program: &Program) -> (String, String) {
        let mut rust = String::new();
        let mut header = String::new();

        rust.push_str("// Generated C FFI bindings for Gust machine(s)\n");
        rust.push_str("use core::ffi::c_int;\n\n");

        header.push_str("#ifndef GUST_FFI_H\n");
        header.push_str("#define GUST_FFI_H\n\n");
        header.push_str("#include <stdint.h>\n\n");
        header.push_str("#ifdef __cplusplus\nextern \"C\" {\n#endif\n\n");

        for machine in &program.machines {
            let lower = machine.name.to_ascii_lowercase();
            let state_enum = format!("{}State", machine.name);
            let handle = format!("{}Handle", machine.name);

            header.push_str(&format!("typedef enum {} {{\n", state_enum));
            for (idx, state) in machine.states.iter().enumerate() {
                header.push_str(&format!(
                    "    {}_STATE_{} = {},\n",
                    lower.to_ascii_uppercase(),
                    state.name.to_ascii_uppercase(),
                    idx
                ));
            }
            header.push_str(&format!("}} {};\n\n", state_enum));

            header.push_str(&format!("typedef struct {} {};\n", handle, handle));
            header.push_str(&format!("{}* {}_new(void);\n", handle, lower));
            header.push_str(&format!("void {}_free({}* handle);\n", lower, handle));
            header.push_str(&format!(
                "{} {}_state(const {}* handle);\n",
                state_enum, lower, handle
            ));
            for transition in &machine.transitions {
                header.push_str(&format!(
                    "int {}_{}({}* handle);\n",
                    lower, transition.name, handle
                ));
            }
            header.push('\n');

            rust.push_str("#[repr(C)]\n");
            rust.push_str("#[derive(Clone, Copy, Debug, PartialEq, Eq)]\n");
            rust.push_str(&format!("pub enum {state_enum} {{\n"));
            for state in &machine.states {
                rust.push_str(&format!("    {},\n", state.name));
            }
            rust.push_str("}\n\n");

            rust.push_str("#[repr(C)]\n");
            rust.push_str(&format!("pub struct {handle} {{\n"));
            rust.push_str(&format!("    state: {state_enum},\n"));
            rust.push_str("}\n\n");

            let initial = machine
                .states
                .first()
                .map(|s| s.name.as_str())
                .unwrap_or("__Invalid");
            rust.push_str("#[no_mangle]\n");
            rust.push_str(&format!(
                "pub unsafe extern \"C\" fn {lower}_new() -> *mut {handle} {{\n"
            ));
            rust.push_str(&format!(
                "    Box::into_raw(Box::new({handle} {{ state: {state_enum}::{initial} }}))\n"
            ));
            rust.push_str("}\n\n");

            rust.push_str("#[no_mangle]\n");
            rust.push_str(&format!(
                "pub unsafe extern \"C\" fn {lower}_free(handle: *mut {handle}) {{\n"
            ));
            rust.push_str("    if !handle.is_null() {\n");
            rust.push_str("        drop(Box::from_raw(handle));\n");
            rust.push_str("    }\n");
            rust.push_str("}\n\n");

            rust.push_str("#[no_mangle]\n");
            rust.push_str(&format!("pub unsafe extern \"C\" fn {lower}_state(handle: *const {handle}) -> {state_enum} {{\n"));
            rust.push_str("    if handle.is_null() {\n");
            rust.push_str(&format!("        return {state_enum}::{initial};\n"));
            rust.push_str("    }\n");
            rust.push_str("    (*handle).state\n");
            rust.push_str("}\n\n");

            for transition in &machine.transitions {
                let target = transition
                    .targets
                    .first()
                    .cloned()
                    .unwrap_or_else(|| transition.from.clone());
                rust.push_str("#[no_mangle]\n");
                rust.push_str(&format!(
                    "pub unsafe extern \"C\" fn {lower}_{}(handle: *mut {handle}) -> c_int {{\n",
                    transition.name
                ));
                rust.push_str("    if handle.is_null() {\n");
                rust.push_str("        return -1;\n");
                rust.push_str("    }\n");
                rust.push_str(&format!(
                    "    if (*handle).state != {state_enum}::{} {{\n",
                    transition.from
                ));
                rust.push_str("        return -2;\n");
                rust.push_str("    }\n");
                rust.push_str(&format!("    (*handle).state = {state_enum}::{target};\n"));
                rust.push_str("    0\n");
                rust.push_str("}\n\n");
            }
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
