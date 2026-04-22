#![warn(missing_docs)]
//! # Gust Language — compiler library
//!
//! The core compiler for the Gust state-machine language. This crate
//! provides the parser, validator, and code generators consumed by
//! `gust-cli`, `gust-lsp`, `gust-mcp`, and `gust-build`.
//!
//! ## Pipeline
//!
//! ```text
//! source.gu → Parser (pest PEG) → AST → Validator → Codegen → .g.rs / .g.go
//! ```
//!
//! ## Public surface
//!
//! - [`parse_program`] / [`parse_program_with_errors`] — parse a `.gu`
//!   source string into [`ast::Program`].
//! - [`validate_program`] — semantic validation returning
//!   [`ValidationReport`].
//! - [`format_program`] / [`format_program_preserving`] — reformat a
//!   parsed program back to `.gu` source.
//! - Code generators: [`RustCodegen`], [`GoCodegen`], [`WasmCodegen`],
//!   [`NoStdCodegen`], [`CffiCodegen`], [`SchemaCodegen`].

/// The abstract syntax tree produced by the parser.
pub mod ast;
/// Rust (default) code generator.
pub mod codegen;
/// Shared helpers used by multiple code generators (e.g. Mermaid diagram
/// rendering, expression analysis).
pub mod codegen_common;
/// C FFI code generator (emits Rust `#[no_mangle]` exports + a companion
/// `.g.h` header).
pub mod codegen_ffi;
/// Go code generator.
pub mod codegen_go;
/// `no_std` code generator (emits `heapless`-based Rust for embedded).
pub mod codegen_nostd;
/// JSON Schema code generator (emits a schema describing types and
/// machine states).
pub mod codegen_schema;
/// WebAssembly code generator (emits `wasm-bindgen`-annotated Rust).
pub mod codegen_wasm;
/// Diagnostic error and warning types with source-annotated rendering.
pub mod error;
/// Comment-preserving Gust source formatter.
pub mod format;
/// pest-based parser converting source text into [`ast::Program`].
pub(crate) mod parser;
/// Semantic validation producing a [`ValidationReport`] with rich
/// diagnostics (undefined references, type mismatches, handler-safety
/// warnings for `action` declarations, etc.).
pub mod validator;

pub use codegen::RustCodegen;
pub use codegen_ffi::CffiCodegen;
pub use codegen_go::GoCodegen;
pub use codegen_nostd::NoStdCodegen;
pub use codegen_schema::SchemaCodegen;
pub use codegen_wasm::WasmCodegen;
pub use format::{format_program, format_program_preserving};
pub use parser::{parse_program, parse_program_with_errors};
pub use validator::{validate_program, ValidationReport};
