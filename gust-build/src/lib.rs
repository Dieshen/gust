use gust_lang::{parse_program, CffiCodegen, GoCodegen, NoStdCodegen, RustCodegen, WasmCodegen};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub enum Target {
    Rust,
    Go { package_name: String },
    Wasm,
    NoStd,
    Cffi,
}

#[derive(Debug, Clone)]
pub struct GustBuilder {
    source_dir: PathBuf,
    output_dir: Option<PathBuf>,
    target: Target,
}

impl GustBuilder {
    pub fn new() -> Self {
        Self {
            source_dir: PathBuf::from("src"),
            output_dir: None,
            target: Target::Rust,
        }
    }

    pub fn source_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.source_dir = path.into();
        self
    }

    pub fn output_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.output_dir = Some(path.into());
        self
    }

    pub fn target(mut self, target: Target) -> Self {
        self.target = target;
        self
    }

    pub fn compile(&self) -> Result<Vec<PathBuf>, String> {
        compile_with_config(&self.source_dir, self.output_dir.as_deref(), self.target.clone())
    }
}

impl Default for GustBuilder {
    fn default() -> Self {
        Self::new()
    }
}

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
        if !entry.file_type().is_file() || path.extension().and_then(|s| s.to_str()) != Some("gu")
        {
            continue;
        }

        println!("cargo:rerun-if-changed={}", path.display());

        let out_path = output_path(path, output_dir, &target)?;
        if !should_regenerate(path, &out_path)? {
            continue;
        }

        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("failed to create '{}': {e}", parent.display()))?;
        }

        let source = fs::read_to_string(path)
            .map_err(|e| format!("failed to read '{}': {e}", path.display()))?;
        let program = parse_program(&source)
            .map_err(|msg| format_parse_error(path, &source, &msg))?;
        let generated = match target {
            Target::Rust => RustCodegen::new().generate(&program),
            Target::Go { ref package_name } => GoCodegen::new().generate(&program, package_name),
            Target::Wasm => WasmCodegen::new().generate(&program),
            Target::NoStd => NoStdCodegen::new().generate(&program),
            Target::Cffi => CffiCodegen::new().generate(&program).0,
        };

        fs::write(&out_path, generated)
            .map_err(|e| format!("failed to write '{}': {e}", out_path.display()))?;
        written_files.push(out_path);
    }

    Ok(written_files)
}

fn output_path(input: &Path, output_dir: Option<&Path>, target: &Target) -> Result<PathBuf, String> {
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
    let snippet = lines
        .get(line.saturating_sub(1))
        .copied()
        .unwrap_or("");
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
    let line = parts.next().and_then(|v| v.trim().parse().ok()).unwrap_or(0);
    let col = parts.next().and_then(|v| v.trim().parse().ok()).unwrap_or(0);
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
        fs::write(&gu, "machine Flow { state A transition go: A -> A on go() { goto A(); } }")
            .unwrap();
        fs::write(&out, "// pre-existing output").unwrap();

        let written = compile_with_config(&src_dir, None, Target::Rust).unwrap();
        assert!(written.is_empty());
    }
}
