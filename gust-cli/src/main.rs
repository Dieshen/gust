use clap::{Parser, Subcommand};
use gust_lang::{
    format_program_preserving, parse_program, parse_program_with_errors, validate_program,
    CffiCodegen, GoCodegen, NoStdCodegen, RustCodegen, SchemaCodegen, WasmCodegen,
};
use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;
use walkdir::WalkDir;

#[derive(Parser)]
#[command(
    name = "gust",
    version,
    about = "The Gust programming language compiler"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile a .gu file to Rust or Go source
    Build {
        #[arg(value_name = "FILE")]
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(short, long, default_value = "rust")]
        target: String,
        #[arg(short, long)]
        package: Option<String>,
        #[arg(long)]
        compile: bool,
    },
    /// Watch a directory and recompile .gu files on changes
    Watch {
        #[arg(value_name = "DIR", default_value = ".")]
        dir: PathBuf,
        #[arg(short, long, default_value = "rust")]
        target: String,
        #[arg(short, long)]
        package: Option<String>,
    },
    /// Parse a .gu file and print the AST (for debugging)
    Parse {
        #[arg(value_name = "FILE")]
        input: PathBuf,
    },
    /// Scaffold a new Gust-enabled Rust project
    Init {
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Format a Gust source file in-place
    Fmt {
        #[arg(value_name = "FILE")]
        input: PathBuf,
    },
    /// Parse + validate a Gust source file without codegen
    Check {
        #[arg(value_name = "FILE")]
        input: PathBuf,
    },
    /// Generate Mermaid state diagram
    Diagram {
        #[arg(value_name = "FILE")]
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(short, long, value_name = "NAME")]
        machine: Option<String>,
    },
    /// Generate JSON Schema from Gust types and machine states
    Schema {
        #[arg(value_name = "FILE")]
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Only generate schema for a specific machine
        #[arg(short, long, value_name = "NAME")]
        machine: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Build {
            input,
            output,
            target,
            package,
            compile,
        } => {
            let out_file =
                compile_single_file(&input, output.as_deref(), &target, package.as_deref())
                    .unwrap_or_else(|e| {
                        eprintln!("error: {e}");
                        std::process::exit(1);
                    });
            println!("Generated {}", out_file.display());
            if compile {
                if target != "rust" {
                    eprintln!("warning: --compile is only supported for Rust target");
                    return;
                }
                if let Err(err) = run_rust_compile("cargo", &out_file) {
                    eprintln!("error: {err}");
                    std::process::exit(1);
                }
            }
        }
        Commands::Watch {
            dir,
            target,
            package,
        } => {
            watch_files(&dir, &target, package.as_deref()).unwrap_or_else(|e| {
                eprintln!("error: {e}");
                std::process::exit(1);
            });
        }
        Commands::Parse { input } => {
            let source = fs::read_to_string(&input).unwrap_or_else(|e| {
                eprintln!("error: cannot read '{}': {e}", input.display());
                std::process::exit(1);
            });
            let program = parse_program(&source).unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(1);
            });
            println!("{program:#?}");
        }
        Commands::Init { name } => {
            init_project(&name).unwrap_or_else(|e| {
                eprintln!("error: {e}");
                std::process::exit(1);
            });
            println!("Initialized project '{name}'");
        }
        Commands::Fmt { input } => {
            format_file(&input).unwrap_or_else(|e| {
                eprintln!("error: {e}");
                std::process::exit(1);
            });
            println!("Formatted {}", input.display());
        }
        Commands::Check { input } => {
            if let Err(code) = check_file(&input) {
                std::process::exit(code);
            }
        }
        Commands::Diagram {
            input,
            output,
            machine,
        } => {
            let diagram =
                generate_mermaid_diagram(&input, machine.as_deref()).unwrap_or_else(|e| {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                });
            if let Some(out) = output {
                fs::write(&out, diagram).unwrap_or_else(|e| {
                    eprintln!("error: cannot write '{}': {e}", out.display());
                    std::process::exit(1);
                });
                println!("Wrote {}", out.display());
            } else {
                println!("{diagram}");
            }
        }
        Commands::Schema {
            input,
            output,
            machine,
        } => {
            let schema_json =
                generate_json_schema(&input, machine.as_deref()).unwrap_or_else(|e| {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                });
            if let Some(out) = output {
                fs::write(&out, &schema_json).unwrap_or_else(|e| {
                    eprintln!("error: cannot write '{}': {e}", out.display());
                    std::process::exit(1);
                });
                println!("Wrote {}", out.display());
            } else {
                println!("{schema_json}");
            }
        }
    }
}

fn init_project(name: &str) -> Result<(), String> {
    validate_project_name(name)?;
    let root = PathBuf::from(name);
    if root.exists() {
        return Err(format!("directory '{}' already exists", root.display()));
    }
    let root_abs = absolute_project_path(&root)?;
    let parent_workspace_manifest = find_parent_workspace_manifest(&root_abs)?;
    fs::create_dir_all(root.join("src")).map_err(|e| format!("cannot create project dirs: {e}"))?;

    let cargo_toml = build_init_cargo_toml(name, parent_workspace_manifest.is_some());
    fs::write(root.join("Cargo.toml"), cargo_toml)
        .map_err(|e| format!("write Cargo.toml failed: {e}"))?;

    if let Some(manifest) = parent_workspace_manifest {
        println!(
            "note: detected parent Cargo workspace at '{}'; added [workspace] to generated Cargo.toml",
            manifest.display()
        );
    }

    fs::write(
        root.join("build.rs"),
        r#"fn main() {
    if let Err(err) = gust_build::compile_gust_files() {
        panic!("gust build failed: {err}");
    }
}
"#,
    )
    .map_err(|e| format!("write build.rs failed: {e}"))?;

    fs::write(
        root.join("src/main.rs"),
        "fn main() {\n    println!(\"hello from gust project\");\n}\n",
    )
    .map_err(|e| format!("write main.rs failed: {e}"))?;

    fs::write(
        root.join("src/payment.gu"),
        "machine Payment {\n    state Pending\n    state Done\n\n    transition finish: Pending -> Done\n\n    on finish() {\n        goto Done();\n    }\n}\n",
    )
    .map_err(|e| format!("write payment.gu failed: {e}"))?;

    fs::write(
        root.join("README.md"),
        format!("# {name}\n\nGenerated by `gust init`.\n"),
    )
    .map_err(|e| format!("write README failed: {e}"))?;

    Ok(())
}

fn validate_project_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("project name cannot be empty".to_string());
    }
    if name.contains(['\\', '/']) {
        return Err("project name must not contain path separators".to_string());
    }
    if name
        .chars()
        .any(|c| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
    {
        return Err(
            "project name must use only letters, numbers, '-' or '_' for Cargo compatibility"
                .to_string(),
        );
    }
    Ok(())
}

fn build_init_cargo_toml(name: &str, standalone_workspace: bool) -> String {
    let mut cargo_toml = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[dependencies]
gust-runtime = {{ path = "../gust-runtime" }}

[build-dependencies]
gust-build = {{ path = "../gust-build" }}
"#
    );
    if standalone_workspace {
        cargo_toml.push_str("\n[workspace]\n");
    }
    cargo_toml
}

fn absolute_project_path(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    env::current_dir()
        .map(|cwd| cwd.join(path))
        .map_err(|e| format!("cannot resolve current directory: {e}"))
}

fn find_parent_workspace_manifest(project_root: &Path) -> Result<Option<PathBuf>, String> {
    let mut current = project_root.parent();
    while let Some(dir) = current {
        let manifest = dir.join("Cargo.toml");
        if manifest.is_file() {
            let content = fs::read_to_string(&manifest)
                .map_err(|e| format!("cannot read '{}': {e}", manifest.display()))?;
            if cargo_manifest_declares_workspace(&content) {
                return Ok(Some(manifest));
            }
        }
        current = dir.parent();
    }
    Ok(None)
}

fn cargo_manifest_declares_workspace(content: &str) -> bool {
    content.lines().any(|line| line.trim() == "[workspace]")
}

fn format_file(input: &Path) -> Result<(), String> {
    let source =
        fs::read_to_string(input).map_err(|e| format!("cannot read '{}': {e}", input.display()))?;
    let program = parse_program_with_errors(&source, &input.display().to_string())
        .map_err(|e| e.render(&source))?;
    let formatted = format_program_preserving(&program, &source);
    fs::write(input, formatted).map_err(|e| format!("cannot write '{}': {e}", input.display()))
}

fn check_file(input: &Path) -> Result<(), i32> {
    let source = match fs::read_to_string(input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read '{}': {e}", input.display());
            return Err(1);
        }
    };
    let program = match parse_program_with_errors(&source, &input.display().to_string()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}", e.render(&source));
            return Err(1);
        }
    };
    let report = validate_program(&program, &input.display().to_string(), &source);
    for warning in &report.warnings {
        eprintln!("{}", warning.render(&source));
    }
    for error in &report.errors {
        eprintln!("{}", error.render(&source));
    }
    if report.errors.is_empty() {
        println!("Check passed");
        Ok(())
    } else {
        Err(1)
    }
}

fn render_machine_diagram(machine: &gust_lang::ast::MachineDecl) -> String {
    let mut out = String::from("stateDiagram-v2\n");
    if let Some(first) = machine.states.first() {
        out.push_str(&format!("    [*] --> {}\n", first.name));
    }
    for t in &machine.transitions {
        for target in &t.targets {
            out.push_str(&format!("    {} --> {} : {}\n", t.from, target, t.name));
        }
    }
    out
}

fn generate_mermaid_diagram(input: &Path, machine_filter: Option<&str>) -> Result<String, String> {
    let source =
        fs::read_to_string(input).map_err(|e| format!("cannot read '{}': {e}", input.display()))?;
    let program = parse_program_with_errors(&source, &input.display().to_string())
        .map_err(|e| e.render(&source))?;

    if program.machines.is_empty() {
        return Err("no machine declaration found".to_string());
    }

    match machine_filter {
        Some(name) => {
            let machine = program
                .machines
                .iter()
                .find(|m| m.name == name)
                .ok_or_else(|| {
                    let available: Vec<&str> =
                        program.machines.iter().map(|m| m.name.as_str()).collect();
                    format!(
                        "machine '{}' not found. Available: {}",
                        name,
                        available.join(", ")
                    )
                })?;
            Ok(render_machine_diagram(machine))
        }
        None => {
            let parts: Vec<String> = program
                .machines
                .iter()
                .map(|m| format!("%% Machine: {}\n{}", m.name, render_machine_diagram(m)))
                .collect();
            Ok(parts.join("\n"))
        }
    }
}

fn generate_json_schema(input: &Path, machine_filter: Option<&str>) -> Result<String, String> {
    let source =
        fs::read_to_string(input).map_err(|e| format!("cannot read '{}': {e}", input.display()))?;
    let program = parse_program_with_errors(&source, &input.display().to_string())
        .map_err(|e| e.render(&source))?;

    let report = validate_program(&program, &input.display().to_string(), &source);
    for warning in &report.warnings {
        eprintln!("{}", warning.render(&source));
    }
    if !report.errors.is_empty() {
        for error in &report.errors {
            eprintln!("{}", error.render(&source));
        }
        return Err("validation failed".to_string());
    }

    if let Some(name) = machine_filter {
        if !program.machines.iter().any(|m| m.name == name) {
            let available: Vec<&str> = program.machines.iter().map(|m| m.name.as_str()).collect();
            return Err(format!(
                "machine '{}' not found. Available: {}",
                name,
                available.join(", ")
            ));
        }
    }

    Ok(SchemaCodegen::generate_filtered(&program, machine_filter))
}

fn watch_files(dir: &Path, target: &str, package: Option<&str>) -> Result<(), String> {
    compile_all_gu_files(dir, target, package)?;
    println!("Watching {} for .gu changes...", dir.display());

    let (tx, rx) = mpsc::channel();
    let mut debouncer = new_debouncer(Duration::from_millis(100), tx)
        .map_err(|e| format!("failed to create file watcher: {e}"))?;
    debouncer
        .watcher()
        .watch(dir, RecursiveMode::Recursive)
        .map_err(|e| format!("failed to watch '{}': {e}", dir.display()))?;

    loop {
        match rx.recv() {
            Ok(Ok(events)) => {
                for event in events {
                    if !matches!(
                        event.kind,
                        DebouncedEventKind::Any | DebouncedEventKind::AnyContinuous
                    ) {
                        continue;
                    }
                    if event.path.extension().and_then(|e| e.to_str()) != Some("gu") {
                        continue;
                    }
                    if !event.path.exists() {
                        match delete_generated_file(&event.path, target) {
                            Ok(Some(path)) => println!("Deleted {}", path.display()),
                            Ok(None) => {}
                            Err(err) => eprintln!("error: {err}"),
                        }
                        continue;
                    }
                    match compile_single_file(&event.path, None, target, package) {
                        Ok(out_file) => println!("Recompiled {}", out_file.display()),
                        Err(err) => eprintln!("error: {err}"),
                    }
                }
            }
            Ok(Err(e)) => eprintln!("watch error: {e}"),
            Err(e) => return Err(format!("watch channel failed: {e}")),
        }
    }
}

fn compile_all_gu_files(dir: &Path, target: &str, package: Option<&str>) -> Result<(), String> {
    for entry in WalkDir::new(dir).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("gu") {
            continue;
        }
        let out_file = compile_single_file(path, None, target, package)?;
        println!("Generated {}", out_file.display());
    }
    Ok(())
}

fn compile_single_file(
    input: &Path,
    output: Option<&Path>,
    target: &str,
    package: Option<&str>,
) -> Result<PathBuf, String> {
    let source =
        fs::read_to_string(input).map_err(|e| format!("cannot read '{}': {e}", input.display()))?;
    let program = parse_program_with_errors(&source, &input.display().to_string())
        .map_err(|e| e.render(&source))?;
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("invalid filename '{}'", input.display()))?;

    match target {
        "rust" => {
            let rust_code = RustCodegen::new().generate(&program);
            let out_file = generated_output_path(input, output, target)?;
            if let Some(output_dir) = output {
                fs::create_dir_all(output_dir).map_err(|e| {
                    format!("cannot create output dir '{}': {e}", output_dir.display())
                })?;
            }
            fs::write(&out_file, rust_code)
                .map_err(|e| format!("cannot write '{}': {e}", out_file.display()))?;
            Ok(out_file)
        }
        "go" => {
            let package_name = package
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| stem.replace(['-', ' '], "_"));
            let go_code = GoCodegen::new().generate(&program, &package_name);
            let out_file = generated_output_path(input, output, target)?;
            if let Some(output_dir) = output {
                fs::create_dir_all(output_dir).map_err(|e| {
                    format!("cannot create output dir '{}': {e}", output_dir.display())
                })?;
            }
            fs::write(&out_file, go_code)
                .map_err(|e| format!("cannot write '{}': {e}", out_file.display()))?;
            Ok(out_file)
        }
        "wasm" => {
            let code = WasmCodegen::new().generate(&program);
            let out_file = generated_output_path(input, output, target)?;
            if let Some(output_dir) = output {
                fs::create_dir_all(output_dir).map_err(|e| {
                    format!("cannot create output dir '{}': {e}", output_dir.display())
                })?;
            }
            fs::write(&out_file, code)
                .map_err(|e| format!("cannot write '{}': {e}", out_file.display()))?;
            Ok(out_file)
        }
        "nostd" => {
            let code = NoStdCodegen::new().generate(&program);
            let out_file = generated_output_path(input, output, target)?;
            if let Some(output_dir) = output {
                fs::create_dir_all(output_dir).map_err(|e| {
                    format!("cannot create output dir '{}': {e}", output_dir.display())
                })?;
            }
            fs::write(&out_file, code)
                .map_err(|e| format!("cannot write '{}': {e}", out_file.display()))?;
            Ok(out_file)
        }
        "ffi" => {
            let (rust_code, header_code) = CffiCodegen::new().generate(&program);
            let out_file = generated_output_path(input, output, target)?;
            let header_file = generated_header_path(input, output, target)?;
            if let Some(output_dir) = output {
                fs::create_dir_all(output_dir).map_err(|e| {
                    format!("cannot create output dir '{}': {e}", output_dir.display())
                })?;
            }
            fs::write(&out_file, rust_code)
                .map_err(|e| format!("cannot write '{}': {e}", out_file.display()))?;
            fs::write(&header_file, header_code)
                .map_err(|e| format!("cannot write '{}': {e}", header_file.display()))?;
            Ok(out_file)
        }
        other => Err(format!(
            "unsupported target '{other}'. Use 'rust', 'go', 'wasm', 'nostd', or 'ffi'"
        )),
    }
}

fn delete_generated_file(input: &Path, target: &str) -> Result<Option<PathBuf>, String> {
    let out_file = generated_output_path(input, None, target)?;
    if target == "ffi" {
        let header = generated_header_path(input, None, target)?;
        if header.exists() {
            fs::remove_file(&header)
                .map_err(|e| format!("cannot remove '{}': {e}", header.display()))?;
        }
    }
    if out_file.exists() {
        fs::remove_file(&out_file)
            .map_err(|e| format!("cannot remove '{}': {e}", out_file.display()))?;
        Ok(Some(out_file))
    } else {
        Ok(None)
    }
}

fn generated_output_path(
    input: &Path,
    output: Option<&Path>,
    target: &str,
) -> Result<PathBuf, String> {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("invalid filename '{}'", input.display()))?;
    let filename = match target {
        "rust" => format!("{stem}.g.rs"),
        "go" => format!("{stem}.g.go"),
        "wasm" => format!("{stem}.g.wasm.rs"),
        "nostd" => format!("{stem}.g.nostd.rs"),
        "ffi" => format!("{stem}.g.ffi.rs"),
        other => {
            return Err(format!(
                "unsupported target '{other}'. Use 'rust', 'go', 'wasm', 'nostd', or 'ffi'"
            ))
        }
    };
    Ok(if let Some(output_dir) = output {
        output_dir.join(filename)
    } else {
        input
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(filename)
    })
}

fn generated_header_path(
    input: &Path,
    output: Option<&Path>,
    target: &str,
) -> Result<PathBuf, String> {
    if target != "ffi" {
        return Err("header path is only valid for ffi target".to_string());
    }
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("invalid filename '{}'", input.display()))?;
    let filename = format!("{stem}.g.h");
    Ok(if let Some(output_dir) = output {
        output_dir.join(filename)
    } else {
        input
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(filename)
    })
}

fn find_crate_root(start: &Path) -> Result<PathBuf, String> {
    // Canonicalize to resolve relative paths before walking up
    let absolute = if start.is_absolute() {
        start.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| format!("cannot determine current directory: {e}"))?
            .join(start)
    };
    let mut dir = if absolute.is_file() {
        absolute
            .parent()
            .ok_or_else(|| format!("cannot determine parent of '{}'", absolute.display()))?
            .to_path_buf()
    } else {
        absolute
    };
    loop {
        if dir.join("Cargo.toml").is_file() {
            return Ok(dir);
        }
        let parent = dir
            .parent()
            .ok_or_else(|| "no Cargo.toml found in any parent directory".to_string())?
            .to_path_buf();
        if parent == dir {
            return Err("no Cargo.toml found in any parent directory".to_string());
        }
        dir = parent;
    }
}

fn run_rust_compile(cargo_bin: &str, generated_file: &Path) -> Result<(), String> {
    let crate_root = find_crate_root(generated_file)?;
    let status = Command::new(cargo_bin)
        .arg("build")
        .current_dir(&crate_root)
        .status()
        .map_err(|e| format!("failed to run cargo: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("cargo build failed".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_init_cargo_toml, cargo_manifest_declares_workspace, find_crate_root,
        find_parent_workspace_manifest, run_rust_compile, validate_project_name,
    };
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn compile_step_returns_error_when_cargo_binary_is_missing() {
        let dir = tempdir().expect("create tempdir");
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname=\"x\"\n")
            .expect("write Cargo.toml");
        let fake_file = dir.path().join("src").join("main.g.rs");
        let err = run_rust_compile("__gust_nonexistent_cargo_bin__", &fake_file)
            .expect_err("missing binary should return an error");
        assert!(err.contains("failed to run cargo"));
    }

    #[test]
    fn find_crate_root_walks_up_to_cargo_toml() {
        let dir = tempdir().expect("create tempdir");
        let sub = dir.path().join("src").join("nested");
        fs::create_dir_all(&sub).expect("create dirs");
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname=\"x\"\n")
            .expect("write Cargo.toml");
        let file = sub.join("foo.g.rs");
        let root = find_crate_root(&file).expect("should find crate root");
        assert_eq!(root, dir.path());
    }

    #[test]
    fn find_crate_root_errors_without_cargo_toml() {
        let dir = tempdir().expect("create tempdir");
        let file = dir.path().join("foo.g.rs");
        let err = find_crate_root(&file).expect_err("should error without Cargo.toml");
        assert!(err.contains("no Cargo.toml"));
    }

    #[test]
    fn cargo_toml_includes_workspace_when_requested() {
        let cargo_toml = build_init_cargo_toml("demo", true);
        assert!(cargo_toml.contains("[workspace]"));
    }

    #[test]
    fn cargo_toml_omits_workspace_when_not_requested() {
        let cargo_toml = build_init_cargo_toml("demo", false);
        assert!(!cargo_toml.contains("[workspace]"));
    }

    #[test]
    fn workspace_detection_finds_parent_workspace_manifest() {
        let dir = tempdir().expect("create tempdir");
        let workspace_root = dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).expect("create workspace root");
        fs::write(
            workspace_root.join("Cargo.toml"),
            "[workspace]\nmembers = []\n",
        )
        .expect("write workspace Cargo.toml");

        let project_root = workspace_root.join("apps").join("new_project");
        let found = find_parent_workspace_manifest(&project_root).expect("workspace detection");
        assert_eq!(found, Some(workspace_root.join("Cargo.toml")));
    }

    #[test]
    fn workspace_detection_returns_none_without_parent_workspace() {
        let dir = tempdir().expect("create tempdir");
        let project_root = dir.path().join("standalone").join("new_project");
        let found = find_parent_workspace_manifest(&project_root).expect("workspace detection");
        assert_eq!(found, None);
    }

    #[test]
    fn workspace_parser_detects_workspace_table() {
        assert!(cargo_manifest_declares_workspace(
            "[workspace]\nmembers=[]\n"
        ));
        assert!(!cargo_manifest_declares_workspace(
            "[package]\nname=\"x\"\n"
        ));
    }

    #[test]
    fn project_name_validation_rejects_spaces() {
        let err = validate_project_name("bad name").expect_err("name with space should fail");
        assert!(err.contains("Cargo compatibility"));
    }

    #[test]
    fn project_name_validation_allows_common_cargo_names() {
        validate_project_name("my-app_01").expect("valid name should pass");
    }
}
