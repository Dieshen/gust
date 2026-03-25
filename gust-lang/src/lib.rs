//! # gust-lang
//!
//! Core compiler library for the Gust state machine language. Gust is a
//! type-safe DSL that compiles `.gu` source files into idiomatic Rust and Go
//! code, with additional backends for WebAssembly, `no_std`, and C FFI.
//!
//! ## Compiler Pipeline
//!
//! ```text
//! source.gu → Parser (PEG) → AST → Validator → Codegen → .g.rs / .g.go
//! ```
//!
//! 1. **Parse** -- [`parse_program`] turns a `.gu` source string into a
//!    [`ast::Program`] AST, or returns a human-readable error.
//! 2. **Validate** -- [`validate_program`] performs semantic analysis on the
//!    AST, producing a [`ValidationReport`] with errors and warnings.
//! 3. **Generate** -- A codegen backend ([`RustCodegen`], [`GoCodegen`],
//!    [`WasmCodegen`], [`NoStdCodegen`], or [`CffiCodegen`]) transforms the
//!    validated AST into target-language source code.
//!
//! ## Quick Start
//!
//! ```rust
//! use gust_lang::{parse_program, validate_program, RustCodegen};
//!
//! let source = r#"
//!     machine Toggle {
//!         state Off
//!         state On
//!         transition flip: Off -> On
//!         transition reset: On -> Off
//!         on flip(ctx: Ctx) { goto On; }
//!         on reset(ctx: Ctx) { goto Off; }
//!     }
//! "#;
//!
//! let ast = parse_program(source).expect("parse error");
//! let report = validate_program(&ast, "toggle.gu", source);
//! assert!(report.is_ok());
//!
//! let rust_code = RustCodegen::new().generate(&ast);
//! assert!(rust_code.contains("pub enum ToggleState"));
//! ```
//!
//! ## Formatting
//!
//! The [`format_program`] function pretty-prints a parsed AST back into
//! canonical Gust syntax, while [`format_program_preserving`] retains
//! comments from the original source.

pub mod ast;
pub mod codegen;
pub mod codegen_common;
pub mod codegen_ffi;
pub mod codegen_go;
pub mod codegen_nostd;
pub mod codegen_wasm;
pub mod error;
pub mod format;
pub mod parser;
pub mod validator;

pub use codegen::RustCodegen;
pub use codegen_ffi::CffiCodegen;
pub use codegen_go::GoCodegen;
pub use codegen_nostd::NoStdCodegen;
pub use codegen_wasm::WasmCodegen;
pub use format::{format_program, format_program_preserving};
pub use parser::{parse_program, parse_program_with_errors};
pub use validator::{validate_program, ValidationReport};
