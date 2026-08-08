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
//! - Code generators: [`RustCodegen`], [`GoCodegen`], [`SchemaCodegen`],
//!   and [`CffiCodegen`] (unstable — see below).
//!
//! ## Stable backends
//!
//! `rust`, `go`, and `schema` are covered by the 1.0 stability promise.
//! [`CffiCodegen`] is **not**: it emits a `.g.h` header that no CI job
//! compiles, so its output shape may change within 1.x. It is gated behind
//! `--unstable-ffi` in the CLI.
//!
//! The `wasm` and `nostd` backends were removed in 1.0. Both emitted output
//! that compiled without implementing the source machine — `wasm` in
//! particular discarded state payloads, handler bodies, and every effect. To
//! target WebAssembly, compile the **Rust** backend's output to `wasm32` and
//! implement the generated effects trait in the host.

/// The abstract syntax tree produced by the parser.
pub mod ast;
/// Rust (default) code generator.
pub mod codegen;
/// Shared helpers used by multiple code generators (e.g. Mermaid diagram
/// rendering, expression analysis).
pub mod codegen_common;
/// C FFI code generator (emits Rust `#[no_mangle]` exports + a companion
/// `.g.h` header).
///
/// **Unstable.** The header is generated from the same AST as the Rust half
/// but is not machine-checked, so this backend sits outside the 1.0
/// stability promise.
pub mod codegen_ffi;
/// Go code generator.
pub mod codegen_go;
/// JSON Schema code generator (emits a schema describing types and
/// machine states).
pub mod codegen_schema;
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
pub use codegen_schema::SchemaCodegen;
pub use format::{format_program, format_program_preserving};
pub use parser::{parse_program, parse_program_with_errors};
pub use validator::{ValidationReport, validate_go_target, validate_program};
