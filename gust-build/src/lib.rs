//! # Gust Build
//!
//! A `build.rs` integration helper for compiling Gust (`.gu`) state machine
//! files during `cargo build`.
//!
//! This crate bridges the Gust compiler into the Cargo build pipeline so that
//! `.gu` files are automatically compiled to Rust (or other targets) whenever
//! your project is built. It handles:
//!
//! - Discovering `.gu` files in your source directory
//! - Incremental compilation (skipping files whose output is already up-to-date)
//! - Emitting `cargo:rerun-if-changed` directives for correct rebuild tracking
//! - Formatting parse errors with source snippets and caret pointers
//!
//! ## Quick start
//!
//! Add `gust-build` as a build dependency in your `Cargo.toml`:
//!
//! ```toml
//! [build-dependencies]
//! gust-build = "0.1"
//! ```
//!
//! Then create a `build.rs` at your crate root:
//!
//! ```rust,ignore
//! // In build.rs:
//! fn main() {
//!     gust_build::compile_gust_files().unwrap();
//! }
//! ```
//!
//! This will find all `.gu` files under `src/`, compile them to `.g.rs` files
//! next to each source, and set up Cargo rebuild tracking automatically.
//!
//! ## Advanced configuration
//!
//! For more control, use the [`GustBuilder`] API:
//!
//! ```rust,ignore
//! // In build.rs:
//! use gust_build::{GustBuilder, Target};
//!
//! fn main() {
//!     GustBuilder::new()
//!         .source_dir("gust_sources")
//!         .output_dir("src/generated")
//!         .target(Target::Wasm)
//!         .compile()
//!         .unwrap();
//! }
//! ```

use gust_lang::{parse_program, CffiCodegen, GoCodegen, NoStdCodegen, RustCodegen, WasmCodegen};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use walkdir::WalkDir;

/// The compilation target for generated code.
///
/// Each variant produces files with a different extension and codegen backend:
///
/// | Target | Extension | Backend |
/// |--------|-----------|---------|
/// | `Rust` | `.g.rs` | [`RustCodegen`](gust_lang::RustCodegen) |
/// | `Go` | `.g.go` | [`GoCodegen`](gust_lang::GoCodegen) |
/// | `Wasm` | `.g.wasm.rs` | [`WasmCodegen`](gust_lang::WasmCodegen) |
/// | `NoStd` | `.g.nostd.rs` | [`NoStdCodegen`](gust_lang::NoStdCodegen) |
/// | `Cffi` | `.g.ffi.rs` + `.g.h` | [`CffiCodegen`](gust_lang::CffiCodegen) |
#[derive(Debug, Clone)]
pub enum Target {
    /// Generate idiomatic Rust code (`.g.rs`). This is the default target.
    Rust,
    /// Generate Go code (`.g.go`). Requires a Go package name.
    Go {
        /// The Go package name to use in the generated `package` declaration.
        package_name: String,
    },
    /// Generate Rust code with `wasm-bindgen` annotations (`.g.wasm.rs`).
    Wasm,
    /// Generate `no_std`-compatible Rust code (`.g.nostd.rs`).
    NoStd,
    /// Generate Rust code with C FFI exports (`.g.ffi.rs`) and a C header (`.g.h`).
    Cffi,
}

/// A builder for configuring and running Gust compilation in `build.rs`.
///
/// `GustBuilder` provides a fluent API for specifying the source directory,
/// output directory, and compilation target. It defaults to scanning `src/`
/// for `.gu` files and compiling them to Rust.
///
/// # Examples
///
/// ```rust,ignore
/// use gust_build::{GustBuilder, Target};
///
/// // Compile Go code from a custom directory:
/// GustBuilder::new()
///     .source_dir("machines")
///     .output_dir("generated")
///     .target(Target::Go { package_name: "machines".into() })
///     .compile()
///     .unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct GustBuilder {
    /// The directory to scan for `.gu` files. Defaults to `"src"`.
    source_dir: PathBuf,
    /// An optional output directory. When `None`, generated files are
    /// placed next to their corresponding `.gu` sources.
    output_dir: Option<PathBuf>,
    /// The compilation target. Defaults to [`Target::Rust`].
    target: Target,
}

impl GustBuilder {
    /// Creates a new builder with default settings.
    ///
    /// Defaults:
    /// - Source directory: `src/`
    /// - Output directory: same as source (no separate output dir)
    /// - Target: [`Target::Rust`]
    pub fn new() -> Self {
        Self {
            source_dir: PathBuf::from("src"),
            output_dir: None,
            target: Target::Rust,
        }
    }

    /// Sets the directory to scan for `.gu` files.
    ///
    /// The path is relative to the crate root (where `Cargo.toml` lives).
    /// Subdirectories are scanned recursively.
    pub fn source_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.source_dir = path.into();
        self
    }

    /// Sets a separate output directory for generated files.
    ///
    /// When set, all generated files are written to this directory instead
    /// of being placed next to their `.gu` sources. The directory is created
    /// automatically if it does not exist.
    pub fn output_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.output_dir = Some(path.into());
        self
    }

    /// Sets the compilation target.
    ///
    /// See [`Target`] for the available backends and their output formats.
    pub fn target(mut self, target: Target) -> Self {
        self.target = target;
        self
    }

    /// Runs the compilation and returns the list of written output files.
    ///
    /// Files whose output is already newer than their `.gu` source are
    /// skipped (incremental compilation). A `cargo:rerun-if-changed`
    /// directive is emitted for each `.gu` file discovered.
    ///
    /// # Errors
    ///
    /// Returns an error string if any `.gu` file fails to parse or if
    /// file I/O fails.
    pub fn compile(&self) -> Result<Vec<PathBuf>, String> {
        compile_with_config(
            &self.source_dir,
            self.output_dir.as_deref(),
            self.target.clone(),
        )
    }
}

impl Default for GustBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function that compiles all `.gu` files under `src/` to Rust.
///
/// This is equivalent to `GustBuilder::new().compile()` and is the simplest
/// way to integrate Gust into a `build.rs` script.
///
/// # Examples
///
/// ```rust,ignore
/// // In build.rs:
/// fn main() {
///     gust_build::compile_gust_files().unwrap();
/// }
/// ```
///
/// # Errors
///
/// Returns an error string if any `.gu` file fails to parse or if file I/O
/// fails.
pub fn compile_gust_files() -> Result<Vec<PathBuf>, String> {
    GustBuilder::new().compile()
}

fn compile_with_config(
    source_dir: &Path,
    output_dir: Option<&Path>,
    target: Target,
) -> Result<Vec<PathBuf>, String> {
    if !source_dir.exists() {
        return Ok(Vec::new());
    }

    let mut written_files = Vec::new();
    for entry in WalkDir::new(source_dir).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if !entry.file_type().is_file() || path.extension().and_then(|s| s.to_str()) != Some("gu") {
            continue;
        }

        println!("cargo:rerun-if-changed={}", path.display());

        let out_path = output_path(path, output_dir, &target)?;
        // For Cffi, also check if the header file needs regeneration
        let needs_regen = if matches!(target, Target::Cffi) {
            let h_path = header_output_path(path, output_dir)?;
            should_regenerate(path, &out_path)? || should_regenerate(path, &h_path)?
        } else {
            should_regenerate(path, &out_path)?
        };
        if !needs_regen {
            continue;
        }

        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create '{}': {e}", parent.display()))?;
        }

        let source = fs::read_to_string(path)
            .map_err(|e| format!("failed to read '{}': {e}", path.display()))?;
        let program =
            parse_program(&source).map_err(|msg| format_parse_error(path, &source, &msg))?;
        match target {
            Target::Cffi => {
                let (rust_code, header_code) = CffiCodegen::new().generate(&program);
                fs::write(&out_path, rust_code)
                    .map_err(|e| format!("failed to write '{}': {e}", out_path.display()))?;
                written_files.push(out_path.clone());

                let header_path = header_output_path(path, output_dir)?;
                if let Some(parent) = header_path.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("failed to create '{}': {e}", parent.display()))?;
                }
                fs::write(&header_path, header_code)
                    .map_err(|e| format!("failed to write '{}': {e}", header_path.display()))?;
                written_files.push(header_path);
            }
            _ => {
                let generated = match target {
                    Target::Rust => RustCodegen::new().generate(&program),
                    Target::Go { ref package_name } => {
                        GoCodegen::new().generate(&program, package_name)
                    }
                    Target::Wasm => WasmCodegen::new().generate(&program),
                    Target::NoStd => NoStdCodegen::new().generate(&program),
                    Target::Cffi => unreachable!(),
                };
                fs::write(&out_path, generated)
                    .map_err(|e| format!("failed to write '{}': {e}", out_path.display()))?;
                written_files.push(out_path);
            }
        }
    }

    Ok(written_files)
}

fn output_path(
    input: &Path,
    output_dir: Option<&Path>,
    target: &Target,
) -> Result<PathBuf, String> {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("invalid input filename '{}'", input.display()))?;
    let ext = match target {
        Target::Rust => "g.rs",
        Target::Go { .. } => "g.go",
        Target::Wasm => "g.wasm.rs",
        Target::NoStd => "g.nostd.rs",
        Target::Cffi => "g.ffi.rs",
    };

    Ok(if let Some(dir) = output_dir {
        dir.join(format!("{stem}.{ext}"))
    } else {
        input.with_file_name(format!("{stem}.{ext}"))
    })
}

fn header_output_path(input: &Path, output_dir: Option<&Path>) -> Result<PathBuf, String> {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("invalid input filename '{}'", input.display()))?;
    let filename = format!("{stem}.g.h");
    Ok(if let Some(dir) = output_dir {
        dir.join(filename)
    } else {
        input.with_file_name(filename)
    })
}

fn should_regenerate(input: &Path, output: &Path) -> Result<bool, String> {
    if !output.exists() {
        return Ok(true);
    }
    let input_time = modified_time(input)?;
    let output_time = modified_time(output)?;
    Ok(input_time > output_time)
}

fn modified_time(path: &Path) -> Result<SystemTime, String> {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .map_err(|e| format!("failed to read mtime '{}': {e}", path.display()))
}

fn format_parse_error(path: &Path, source: &str, parse_error: &str) -> String {
    let (line, col) = extract_line_col(parse_error);
    if line == 0 || col == 0 {
        return format!("{}: {}", path.display(), parse_error);
    }

    let lines: Vec<&str> = source.lines().collect();
    let snippet = lines.get(line.saturating_sub(1)).copied().unwrap_or("");
    let caret = format!("{}^", " ".repeat(col.saturating_sub(1)));

    format!(
        "{}:{}:{}: {}\n{}\n{}",
        path.display(),
        line,
        col,
        parse_error,
        snippet,
        caret
    )
}

fn extract_line_col(parse_error: &str) -> (usize, usize) {
    let marker = "-->";
    let Some(start) = parse_error.find(marker) else {
        return (0, 0);
    };
    let tail = &parse_error[start + marker.len()..];
    let mut digits = String::new();
    for ch in tail.chars() {
        if ch.is_ascii_digit() || ch == ':' {
            digits.push(ch);
        } else if !digits.is_empty() {
            break;
        }
    }
    let mut parts = digits.split(':');
    let line = parts
        .next()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);
    let col = parts
        .next()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn compiles_rust_files_from_source_dir() {
        let dir = tempdir().unwrap();
        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(
            src_dir.join("flow.gu"),
            "machine Flow { state A transition go: A -> A on go() { goto A(); } }",
        )
        .unwrap();

        let written = compile_with_config(&src_dir, None, Target::Rust).unwrap();
        assert_eq!(written.len(), 1);
        assert!(src_dir.join("flow.g.rs").exists());
    }

    #[test]
    fn skips_when_output_is_newer() {
        let dir = tempdir().unwrap();
        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();
        let gu = src_dir.join("flow.gu");
        let out = src_dir.join("flow.g.rs");
        fs::write(
            &gu,
            "machine Flow { state A transition go: A -> A on go() { goto A(); } }",
        )
        .unwrap();
        fs::write(&out, "// pre-existing output").unwrap();

        let written = compile_with_config(&src_dir, None, Target::Rust).unwrap();
        assert!(written.is_empty());
    }
}
