use clap::{Parser, Subcommand};
use colored::Colorize;
use globset::{Glob, GlobSet, GlobSetBuilder};
use gust_lang::{
    CffiCodegen, GoCodegen, NoStdCodegen, RustCodegen, SchemaCodegen, WasmCodegen, ast::Program,
    format_program_preserving, parse_program, parse_program_with_errors, validate_program,
};
use notify::RecursiveMode;
use notify_debouncer_mini::{DebouncedEventKind, new_debouncer};
use serde::Deserialize;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
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
        /// Emit tracing instrumentation in generated Rust code (behind #[cfg(feature = "tracing")])
        #[arg(long)]
        tracing: bool,
    },
    /// Generate code from a gust.toml manifest
    Generate {
        /// Path to gust.toml. Defaults to ./gust.toml.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Generate only one target from the manifest.
        #[arg(short, long)]
        target: Option<String>,
        /// Verify generated files are current without writing them.
        #[arg(long)]
        check: bool,
        /// Permit target outputs that resolve outside the manifest and current
        /// directories.
        ///
        /// By default a manifest may only write beneath the directory holding
        /// it or the directory you ran from, so running `gust generate` inside
        /// an unfamiliar repository cannot write to arbitrary paths.
        #[arg(long)]
        allow_outside: bool,
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
    /// Check environment, toolchains, and project health
    Doctor,
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
            tracing,
        } => {
            let out_file = compile_single_file(
                &input,
                output.as_deref(),
                &target,
                package.as_deref(),
                tracing,
            )
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
        Commands::Generate {
            config,
            target,
            check,
            allow_outside,
        } => {
            generate_from_manifest(config.as_deref(), target.as_deref(), check, allow_outside)
                .unwrap_or_else(|e| {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                });
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
                // Accept a directory as well as a file path.
                //
                // `gust build -o` takes a directory, so passing one here is the
                // natural guess — and it used to fail with a bare
                // "Access is denied. (os error 5)" from the OS, which says
                // nothing about the actual mistake. A directory now behaves the
                // way `build` does: the filename derives from the input stem.
                let out = if out.is_dir() {
                    let stem = input
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("schema");
                    out.join(format!("{stem}.schema.json"))
                } else {
                    out
                };
                fs::write(&out, &schema_json).unwrap_or_else(|e| {
                    eprintln!("error: cannot write '{}': {e}", out.display());
                    std::process::exit(1);
                });
                println!("Wrote {}", out.display());
            } else {
                println!("{schema_json}");
            }
        }
        Commands::Doctor => {
            run_doctor();
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

/// The `major.minor` requirement a scaffolded project should depend on.
///
/// All workspace crates share one version, so this binary's own version is the
/// right answer for `gust-runtime` and `gust-build` too. Dropping the patch
/// component keeps the scaffold on a caret requirement rather than pinning.
fn scaffold_dependency_req() -> String {
    let version = env!("CARGO_PKG_VERSION");
    match version.split('.').take(2).collect::<Vec<_>>()[..] {
        [major, minor] => format!("{major}.{minor}"),
        _ => version.to_string(),
    }
}

fn build_init_cargo_toml(name: &str, standalone_workspace: bool) -> String {
    // Version requirements, not path dependencies.
    //
    // The scaffold previously emitted `path = "../gust-runtime"`, which resolves
    // only inside a checkout of the Gust repository itself. Anywhere else —
    // which is to say, for every actual user — `gust init` produced a project
    // that could not build:
    //
    //     error: failed to load source for dependency `gust-runtime`
    //       failed to read .../gust-runtime/Cargo.toml
    //
    // Derived from this binary's own version rather than hardcoded, so a
    // release bump cannot leave the scaffold pointing at a version that does
    // not exist yet.
    let req = scaffold_dependency_req();
    let mut cargo_toml = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[dependencies]
gust-runtime = "{req}"

[build-dependencies]
gust-build = "{req}"
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
    if report_validation(&program, input, &source).is_err() {
        return Err(1);
    }
    println!("Check passed");
    Ok(())
}

/// Read and parse a Gust source file **without** validating it.
///
/// Only for callers that have already validated the file — currently the
/// `gust generate` manifest targets, which validate every discovered source up
/// front in [`validate_manifest_sources`]. Anything that emits code from a file
/// nobody has validated must call [`report_validation`] first.
fn read_and_parse(input: &Path) -> Result<(String, Program), String> {
    let source =
        fs::read_to_string(input).map_err(|e| format!("cannot read '{}': {e}", input.display()))?;
    let program = parse_program_with_errors(&source, &input.display().to_string())
        .map_err(|e| e.render(&source))?;
    Ok((source, program))
}

/// Render every validator diagnostic for a parsed program and report whether it
/// is fit to generate code from.
///
/// Warnings are printed and do not block. They still carry real weight — an
/// unused binding is only a lint in Rust but a hard error in Go — which is why
/// they are surfaced while building rather than left for `gust check`.
///
/// Errors are printed and returned as a failure. A caller must not write output
/// after this returns `Err`: emitting a `.g.rs` for a program the validator
/// rejects hands the author broken generated code they are told never to edit,
/// and the mistake then resurfaces as a host-language compiler error far from
/// its cause.
///
/// The returned message is a short summary; the diagnostics themselves have
/// already gone to stderr, so callers should not render it a second time.
fn report_validation(program: &Program, input: &Path, source: &str) -> Result<(), String> {
    let report = validate_program(program, &input.display().to_string(), source);
    for warning in &report.warnings {
        eprintln!("{}", warning.render(source));
    }
    if report.errors.is_empty() {
        return Ok(());
    }
    for error in &report.errors {
        eprintln!("{}", error.render(source));
    }
    let count = report.errors.len();
    Err(format!(
        "validation of '{}' failed with {count} error{}",
        input.display(),
        if count == 1 { "" } else { "s" }
    ))
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

/// Validate a source file and render its JSON Schema — the `gust schema`
/// subcommand's entry point.
fn generate_json_schema(input: &Path, machine_filter: Option<&str>) -> Result<String, String> {
    let (source, program) = read_and_parse(input)?;
    report_validation(&program, input, &source)?;
    render_json_schema(&program, machine_filter)
}

/// Render JSON Schema for a program that has already been validated.
fn render_json_schema(program: &Program, machine_filter: Option<&str>) -> Result<String, String> {
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

    Ok(SchemaCodegen::generate_filtered(program, machine_filter))
}

#[derive(Debug, Deserialize)]
struct GustManifest {
    #[allow(dead_code)]
    package: Option<ManifestPackage>,
    source: ManifestSource,
    targets: ManifestTargets,
}

#[derive(Debug, Deserialize)]
struct ManifestPackage {
    #[allow(dead_code)]
    name: Option<String>,
    #[allow(dead_code)]
    version: Option<String>,
    #[allow(dead_code)]
    id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ManifestSource {
    root: PathBuf,
    include: Option<Vec<String>>,
    exclude: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct ManifestTargets {
    rust: Option<RustManifestTarget>,
    go: Option<GoManifestTarget>,
    schema: Option<SchemaManifestTarget>,
}

#[derive(Debug, Deserialize)]
struct RustManifestTarget {
    output: PathBuf,
    #[allow(dead_code)]
    module: Option<String>,
    tracing: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct GoManifestTarget {
    output: PathBuf,
    package: String,
}

#[derive(Debug, Deserialize)]
struct SchemaManifestTarget {
    output: PathBuf,
    #[allow(dead_code)]
    id: Option<String>,
}

fn generate_from_manifest(
    config: Option<&Path>,
    target: Option<&str>,
    check: bool,
    allow_outside: bool,
) -> Result<(), String> {
    let config_path = resolve_manifest_path(config)?;
    let manifest = load_manifest(&config_path)?;
    let base_dir = config_path
        .parent()
        .ok_or_else(|| format!("cannot determine parent of '{}'", config_path.display()))?;
    let files = discover_manifest_sources(base_dir, &manifest.source)?;
    if files.is_empty() {
        return Err(format!(
            "no .gu files matched manifest source '{}'",
            resolve_manifest_path_relative(base_dir, &manifest.source.root).display()
        ));
    }

    validate_manifest_sources(&files)?;

    match target {
        Some("rust") => {
            let target = manifest
                .targets
                .rust
                .as_ref()
                .ok_or_else(|| "target 'rust' is not defined in gust.toml".to_string())?;
            generate_rust_target(base_dir, target, &files, check, allow_outside)?;
        }
        Some("go") => {
            let target = manifest
                .targets
                .go
                .as_ref()
                .ok_or_else(|| "target 'go' is not defined in gust.toml".to_string())?;
            generate_go_target(base_dir, target, &files, check, allow_outside)?;
        }
        Some("schema") => {
            let target = manifest
                .targets
                .schema
                .as_ref()
                .ok_or_else(|| "target 'schema' is not defined in gust.toml".to_string())?;
            generate_schema_target(base_dir, target, &files, check, allow_outside)?;
        }
        Some(other) => {
            return Err(format!(
                "unsupported manifest target '{other}'. Use 'rust', 'go', or 'schema'"
            ));
        }
        None => {
            let mut generated_any = false;
            if let Some(target) = &manifest.targets.rust {
                generate_rust_target(base_dir, target, &files, check, allow_outside)?;
                generated_any = true;
            }
            if let Some(target) = &manifest.targets.go {
                generate_go_target(base_dir, target, &files, check, allow_outside)?;
                generated_any = true;
            }
            if let Some(target) = &manifest.targets.schema {
                generate_schema_target(base_dir, target, &files, check, allow_outside)?;
                generated_any = true;
            }
            if !generated_any {
                return Err(
                    "gust.toml must define at least one target under [targets.rust], [targets.go], or [targets.schema]"
                        .to_string(),
                );
            }
        }
    }

    Ok(())
}

/// Parse and validate every manifest source before any target writes a file.
///
/// Validating up front rather than inside each target means a source's
/// diagnostics are printed once no matter how many targets the manifest
/// declares, and a manifest holding one invalid source emits nothing at all
/// rather than writing output for whichever targets happened to run first.
fn validate_manifest_sources(files: &[PathBuf]) -> Result<(), String> {
    for input in files {
        let (source, program) = read_and_parse(input)?;
        report_validation(&program, input, &source)?;
    }
    Ok(())
}

fn resolve_manifest_path(config: Option<&Path>) -> Result<PathBuf, String> {
    let path = config
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("gust.toml"));
    let absolute = if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .map_err(|e| format!("cannot resolve current directory: {e}"))?
            .join(path)
    };
    if absolute.is_file() {
        Ok(absolute)
    } else if config.is_some() {
        Err(format!("config file '{}' not found", absolute.display()))
    } else {
        Err(format!(
            "no gust.toml found in current directory '{}'; pass --config <path>",
            absolute
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .display()
        ))
    }
}

fn load_manifest(config_path: &Path) -> Result<GustManifest, String> {
    let content = fs::read_to_string(config_path)
        .map_err(|e| format!("cannot read '{}': {e}", config_path.display()))?;
    toml::from_str(&content).map_err(|e| format!("cannot parse '{}': {e}", config_path.display()))
}

fn discover_manifest_sources(
    base_dir: &Path,
    source: &ManifestSource,
) -> Result<Vec<PathBuf>, String> {
    let source_root = resolve_manifest_path_relative(base_dir, &source.root);
    if !source_root.is_dir() {
        return Err(format!(
            "source root '{}' is not a directory",
            source_root.display()
        ));
    }

    let include = build_glob_set(source.include.as_deref(), &["**/*.gu"])?;
    let exclude = build_glob_set(source.exclude.as_deref(), &[])?;
    let mut files = Vec::new();
    for entry in WalkDir::new(&source_root)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.into_path();
        let rel = path.strip_prefix(&source_root).unwrap_or(&path);
        if include.is_match(rel) && !exclude.is_match(rel) {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn build_glob_set(patterns: Option<&[String]>, defaults: &[&str]) -> Result<GlobSet, String> {
    let mut builder = GlobSetBuilder::new();
    if let Some(patterns) = patterns {
        for pattern in patterns {
            builder.add(Glob::new(pattern).map_err(|e| format!("invalid glob '{pattern}': {e}"))?);
        }
    } else {
        for pattern in defaults {
            builder.add(Glob::new(pattern).map_err(|e| format!("invalid glob '{pattern}': {e}"))?);
        }
    }
    builder
        .build()
        .map_err(|e| format!("cannot build glob set: {e}"))
}

fn resolve_manifest_path_relative(base_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
}

/// Collapse `.` and `..` without touching the filesystem.
///
/// `canonicalize` is not usable here: output directories routinely do not
/// exist yet, and on Windows it also rewrites paths into the `\\?\` form,
/// which would make the containment comparison below inconsistent.
///
/// A leading `..` that would escape the root is retained so that such a path
/// can never silently normalize into something that looks contained.
fn normalize_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // Only pop a component that can actually be popped. Popping
                // past a prefix/root, or past a retained `..`, would turn an
                // escaping path into a contained-looking one.
                let can_pop = out
                    .components()
                    .next_back()
                    .is_some_and(|c| matches!(c, Component::Normal(_)));
                if can_pop {
                    out.pop();
                } else {
                    out.push(Component::ParentDir);
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// Resolve a manifest target's output directory, refusing paths that escape
/// both the manifest directory and the invocation directory.
///
/// Without this, an `output` of `../../..` or an absolute path lets a
/// `gust.toml` write anywhere the invoking user can — which matters because
/// running `gust generate` in a freshly cloned repository is a normal thing
/// to do.
///
/// Two roots are permitted, because both are chosen by the user rather than by
/// the manifest:
///
/// - the **manifest directory**, for `gust generate --config /elsewhere/gust.toml`
///   run from an unrelated working directory;
/// - the **current directory**, for the documented layout where the manifest
///   sits in a subdirectory and emits into sibling projects
///   (`output = "../rs-project/src/generated"`).
///
/// A path escaping both is refused.
fn resolve_manifest_output(
    base_dir: &Path,
    path: &Path,
    allow_outside: bool,
) -> Result<PathBuf, String> {
    let resolved = resolve_manifest_path_relative(base_dir, path);
    if allow_outside {
        return Ok(resolved);
    }

    let cwd = env::current_dir().map_err(|e| format!("cannot determine current directory: {e}"))?;
    // A relative base (`gust.toml` with no directory part yields an empty
    // parent) must be anchored before comparison, or every path would appear
    // contained.
    let absolute_base = if base_dir.as_os_str().is_empty() {
        cwd.clone()
    } else {
        resolve_manifest_path_relative(&cwd, base_dir)
    };
    let absolute_output = resolve_manifest_path_relative(&cwd, &resolved);

    let normalized_output = normalize_lexically(&absolute_output);
    let allowed_roots = [
        normalize_lexically(&absolute_base),
        normalize_lexically(&cwd),
    ];
    if allowed_roots
        .iter()
        .any(|root| normalized_output.starts_with(root))
    {
        return Ok(resolved);
    }

    Err(format!(
        "target output '{}' resolves to '{}', which is outside both the manifest directory '{}' \
         and the current directory '{}'.\n\
         Pass --allow-outside if this is intended.",
        path.display(),
        normalized_output.display(),
        allowed_roots[0].display(),
        allowed_roots[1].display()
    ))
}

fn generate_rust_target(
    base_dir: &Path,
    target: &RustManifestTarget,
    files: &[PathBuf],
    check: bool,
    allow_outside: bool,
) -> Result<(), String> {
    let output = resolve_manifest_output(base_dir, &target.output, allow_outside)?;
    validate_output_collisions(files, &output, "rust")?;
    for input in files {
        let out_file = write_or_check_generated_file(
            input,
            &output,
            "rust",
            None,
            target.tracing.unwrap_or(false),
            check,
        )?;
        print_generation_status(check, &out_file);
    }
    Ok(())
}

fn generate_go_target(
    base_dir: &Path,
    target: &GoManifestTarget,
    files: &[PathBuf],
    check: bool,
    allow_outside: bool,
) -> Result<(), String> {
    let output = resolve_manifest_output(base_dir, &target.output, allow_outside)?;
    validate_output_collisions(files, &output, "go")?;
    for input in files {
        let out_file = write_or_check_generated_file(
            input,
            &output,
            "go",
            Some(&target.package),
            false,
            check,
        )?;
        print_generation_status(check, &out_file);
    }
    Ok(())
}

fn generate_schema_target(
    base_dir: &Path,
    target: &SchemaManifestTarget,
    files: &[PathBuf],
    check: bool,
    allow_outside: bool,
) -> Result<(), String> {
    let output = resolve_manifest_output(base_dir, &target.output, allow_outside)?;
    validate_schema_output_collisions(files, &output)?;
    for input in files {
        // Sources were validated up front by `validate_manifest_sources`.
        let (_source, program) = read_and_parse(input)?;
        let schema_json = render_json_schema(&program, None)?;
        let out_file = generated_schema_path(input, &output)?;
        write_or_check_file(&out_file, &output, &schema_json, check)?;
        print_generation_status(check, &out_file);
    }
    Ok(())
}

fn print_generation_status(check: bool, out_file: &Path) {
    if check {
        println!("Checked {}", out_file.display());
    } else {
        println!("Generated {}", out_file.display());
    }
}

fn write_or_check_generated_file(
    input: &Path,
    output_dir: &Path,
    target: &str,
    package: Option<&str>,
    tracing: bool,
    check: bool,
) -> Result<PathBuf, String> {
    let content = render_generated_code(input, target, package, tracing)?;
    let out_file = generated_output_path(input, Some(output_dir), target)?;
    write_or_check_file(&out_file, output_dir, &content, check)?;
    Ok(out_file)
}

/// Render a manifest target's output for one source file.
///
/// Sources reaching here were validated up front by
/// [`validate_manifest_sources`].
fn render_generated_code(
    input: &Path,
    target: &str,
    package: Option<&str>,
    tracing: bool,
) -> Result<String, String> {
    let (_source, program) = read_and_parse(input)?;
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("invalid filename '{}'", input.display()))?;

    match target {
        "rust" => Ok(RustCodegen::new().with_tracing(tracing).generate(&program)),
        "go" => {
            let package_name = package
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| stem.replace(['-', ' '], "_"));
            Ok(GoCodegen::new().generate(&program, &package_name))
        }
        "wasm" => Ok(WasmCodegen::new().generate(&program)),
        "nostd" => Ok(NoStdCodegen::new().generate(&program)),
        other => Err(format!(
            "unsupported target '{other}'. Use 'rust', 'go', 'wasm', or 'nostd'"
        )),
    }
}

fn write_or_check_file(
    out_file: &Path,
    output_dir: &Path,
    content: &str,
    check: bool,
) -> Result<(), String> {
    if check {
        verify_generated_file(out_file, content)
    } else {
        fs::create_dir_all(output_dir)
            .map_err(|e| format!("cannot create output dir '{}': {e}", output_dir.display()))?;
        fs::write(out_file, content)
            .map_err(|e| format!("cannot write '{}': {e}", out_file.display()))
    }
}

fn verify_generated_file(out_file: &Path, expected: &str) -> Result<(), String> {
    match fs::read_to_string(out_file) {
        Ok(existing) if existing == expected => Ok(()),
        Ok(_) => Err(format!(
            "generated file '{}' is stale; run `gust generate`",
            out_file.display()
        )),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Err(format!(
            "generated file '{}' is missing; run `gust generate`",
            out_file.display()
        )),
        Err(err) => Err(format!("cannot read '{}': {err}", out_file.display())),
    }
}

fn validate_output_collisions(
    files: &[PathBuf],
    output_dir: &Path,
    target: &str,
) -> Result<(), String> {
    let mut outputs = HashSet::new();
    for input in files {
        let output = generated_output_path(input, Some(output_dir), target)?;
        if !outputs.insert(output.clone()) {
            return Err(format!(
                "multiple source files would generate '{}'",
                output.display()
            ));
        }
    }
    Ok(())
}

fn validate_schema_output_collisions(files: &[PathBuf], output_dir: &Path) -> Result<(), String> {
    let mut outputs = HashSet::new();
    for input in files {
        let output = generated_schema_path(input, output_dir)?;
        if !outputs.insert(output.clone()) {
            return Err(format!(
                "multiple source files would generate '{}'",
                output.display()
            ));
        }
    }
    Ok(())
}

fn generated_schema_path(input: &Path, output_dir: &Path) -> Result<PathBuf, String> {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("invalid filename '{}'", input.display()))?;
    Ok(output_dir.join(format!("{stem}.schema.json")))
}

fn watch_files(dir: &Path, target: &str, package: Option<&str>) -> Result<(), String> {
    compile_all_gu_files(dir, target, package);
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
                    match compile_single_file(&event.path, None, target, package, false) {
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

/// Compile every `.gu` file under `dir` — the initial sweep `gust watch` runs
/// before it starts watching.
///
/// A file that fails to compile is reported and skipped rather than aborting
/// the sweep. Now that validation runs here, refusing to start because one
/// source has a bad `goto` would be exactly backwards: the watcher exists to be
/// running while you fix things. This matches what the watch loop already does
/// with a failure after a file changes.
fn compile_all_gu_files(dir: &Path, target: &str, package: Option<&str>) {
    for entry in WalkDir::new(dir).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("gu") {
            continue;
        }
        match compile_single_file(path, None, target, package, false) {
            Ok(out_file) => println!("Generated {}", out_file.display()),
            Err(err) => eprintln!("error: {err}"),
        }
    }
}

/// Compile one `.gu` file to a single backend's output — the shared body of
/// `gust build` and `gust watch`.
///
/// Validation runs before any codegen, so a source the validator rejects leaves
/// no output file behind.
fn compile_single_file(
    input: &Path,
    output: Option<&Path>,
    target: &str,
    package: Option<&str>,
    tracing: bool,
) -> Result<PathBuf, String> {
    let (source, program) = read_and_parse(input)?;
    report_validation(&program, input, &source)?;
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("invalid filename '{}'", input.display()))?;

    match target {
        "rust" => {
            let rust_code = RustCodegen::new().with_tracing(tracing).generate(&program);
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
            ));
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

// ---------------------------------------------------------------------------
// gust doctor
// ---------------------------------------------------------------------------

/// Run all doctor checks and print a human-readable report.
fn run_doctor() {
    println!("{}", "Gust Doctor".bold());
    println!("{}", "===========".bold());
    println!();

    let mut warnings: u32 = 0;
    let mut errors: u32 = 0;

    // -- Toolchain checks ---------------------------------------------------
    check_rustc(&mut warnings, &mut errors);
    check_cargo(&mut warnings, &mut errors);
    check_go(&mut warnings);
    print_gust_version();
    println!();

    // -- Project detection --------------------------------------------------
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    check_project(&cwd);
    println!();

    // -- .gu file discovery and freshness -----------------------------------
    let gu_files = discover_gu_files(&cwd);
    check_generated_freshness(&gu_files, &mut warnings);
    println!();

    // -- Validation ---------------------------------------------------------
    validate_gu_files(&gu_files, &mut warnings, &mut errors);
    println!();

    // -- Summary ------------------------------------------------------------
    print_summary(warnings, errors);
}

/// Check for `rustc` on PATH and print its version.
fn check_rustc(warnings: &mut u32, errors: &mut u32) {
    match Command::new("rustc").arg("--version").output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            println!("  {} Rust: {}", "[OK]".green(), version);
        }
        _ => {
            println!(
                "  {} Rust: rustc not found — required for Rust codegen",
                "[ERR]".red()
            );
            *errors += 1;
            *warnings += 0; // explicit for clarity
        }
    }
}

/// Check for `cargo` on PATH and print its version.
fn check_cargo(warnings: &mut u32, errors: &mut u32) {
    match Command::new("cargo").arg("--version").output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            println!("  {} Cargo: {}", "[OK]".green(), version);
        }
        _ => {
            println!(
                "  {} Cargo: cargo not found — required for Rust codegen",
                "[ERR]".red()
            );
            *errors += 1;
            *warnings += 0;
        }
    }
}

/// Check for `go` on PATH (optional — only needed for `--target go`).
fn check_go(warnings: &mut u32) {
    match Command::new("go").arg("version").output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            println!("  {} Go: {} (optional)", "[OK]".green(), version);
        }
        _ => {
            println!(
                "  {} Go: not found (optional, needed for --target go)",
                "[WARN]".yellow()
            );
            *warnings += 1;
        }
    }
}

/// Print the gust CLI version from Cargo metadata.
fn print_gust_version() {
    let version = env!("CARGO_PKG_VERSION");
    println!("  {} Gust: {}", "[OK]".green(), version);
}

/// Detect Cargo.toml and gust-build dependency in the working directory.
fn check_project(cwd: &Path) {
    println!("Project: {}", cwd.display());

    let cargo_path = cwd.join("Cargo.toml");
    if cargo_path.is_file() {
        println!("  Cargo.toml: {}", "found".green());
        match fs::read_to_string(&cargo_path) {
            Ok(content) => {
                if content.contains("gust-build") {
                    println!("  gust-build dependency: {}", "found".green());
                } else {
                    println!("  gust-build dependency: {}", "not found".dimmed());
                }
            }
            Err(_) => {
                println!(
                    "  gust-build dependency: {}",
                    "could not read Cargo.toml".dimmed()
                );
            }
        }
    } else {
        println!("  Cargo.toml: {}", "not found".dimmed());
    }
}

/// Walk the directory tree and collect all `.gu` file paths.
fn discover_gu_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        if entry.path().extension().and_then(|e| e.to_str()) == Some("gu") {
            files.push(entry.into_path());
        }
    }
    files.sort();
    files
}

/// For each `.gu` file, check whether a generated `.g.rs` or `.g.go` file
/// exists and whether it is older than the source (stale).
fn check_generated_freshness(gu_files: &[PathBuf], warnings: &mut u32) {
    println!(".gu files: {} found", gu_files.len());
    if gu_files.is_empty() {
        return;
    }
    for gu in gu_files {
        let stem = gu.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown");
        let parent = gu.parent().unwrap_or_else(|| Path::new("."));
        let display_gu = gu.display();

        // Check for all possible generated extensions
        let candidates: Vec<(&str, PathBuf)> = vec![
            (".g.rs", parent.join(format!("{stem}.g.rs"))),
            (".g.go", parent.join(format!("{stem}.g.go"))),
            (".g.wasm.rs", parent.join(format!("{stem}.g.wasm.rs"))),
            (".g.nostd.rs", parent.join(format!("{stem}.g.nostd.rs"))),
            (".g.ffi.rs", parent.join(format!("{stem}.g.ffi.rs"))),
        ];

        let mut found_any = false;
        for (ext, gen_path) in &candidates {
            if gen_path.is_file() {
                found_any = true;
                let gen_display = format!("{stem}{ext}");
                match (gu.metadata(), gen_path.metadata()) {
                    (Ok(src_meta), Ok(gen_meta)) => {
                        let src_time = src_meta.modified().ok();
                        let gen_time = gen_meta.modified().ok();
                        match (src_time, gen_time) {
                            (Some(src_t), Some(gen_t)) if gen_t < src_t => {
                                println!(
                                    "  {} {} -> {} (stale, regenerate)",
                                    "[WARN]".yellow(),
                                    display_gu,
                                    gen_display
                                );
                                *warnings += 1;
                            }
                            _ => {
                                println!(
                                    "  {} {} -> {} (up to date)",
                                    "[OK]".green(),
                                    display_gu,
                                    gen_display
                                );
                            }
                        }
                    }
                    _ => {
                        println!(
                            "  {} {} -> {} (could not read metadata)",
                            "[WARN]".yellow(),
                            display_gu,
                            gen_display
                        );
                        *warnings += 1;
                    }
                }
            }
        }
        if !found_any {
            println!("  {} {} (no generated file)", "[OK]".green(), display_gu);
        }
    }
}

/// Parse and validate every discovered `.gu` file, reporting results.
fn validate_gu_files(gu_files: &[PathBuf], warnings: &mut u32, errors: &mut u32) {
    if gu_files.is_empty() {
        println!("Validation: no .gu files to validate");
        return;
    }
    println!("Validation:");
    for gu in gu_files {
        let source = match fs::read_to_string(gu) {
            Ok(s) => s,
            Err(e) => {
                println!(
                    "  {} {}: could not read file: {e}",
                    "[ERR]".red(),
                    gu.display()
                );
                *errors += 1;
                continue;
            }
        };

        let program = match parse_program_with_errors(&source, &gu.display().to_string()) {
            Ok(p) => p,
            Err(e) => {
                println!(
                    "  {} {}: parse error: {}",
                    "[ERR]".red(),
                    gu.display(),
                    e.render(&source)
                );
                *errors += 1;
                continue;
            }
        };

        let report = validate_program(&program, &gu.display().to_string(), &source);
        let n_err = report.errors.len();
        let n_warn = report.warnings.len();

        if n_err == 0 && n_warn == 0 {
            println!("  {} {}: valid", "[OK]".green(), gu.display());
        } else {
            let mut parts = Vec::new();
            if n_err > 0 {
                parts.push(format!(
                    "{} error{}",
                    n_err,
                    if n_err == 1 { "" } else { "s" }
                ));
            }
            if n_warn > 0 {
                parts.push(format!(
                    "{} warning{}",
                    n_warn,
                    if n_warn == 1 { "" } else { "s" }
                ));
            }
            let label = if n_err > 0 {
                "[ERR]".red().to_string()
            } else {
                "[WARN]".yellow().to_string()
            };
            println!("  {} {}: {}", label, gu.display(), parts.join(", "));
            *errors += n_err as u32;
            *warnings += n_warn as u32;
        }
    }
}

/// Print a summary line with counts.
fn print_summary(warnings: u32, errors: u32) {
    if warnings == 0 && errors == 0 {
        println!(
            "{}",
            "Summary: no issues found. Environment looks good!".green()
        );
    } else {
        let mut parts = Vec::new();
        if warnings > 0 {
            parts.push(format!(
                "{} warning{}",
                warnings,
                if warnings == 1 { "" } else { "s" }
            ));
        }
        if errors > 0 {
            parts.push(format!(
                "{} error{}",
                errors,
                if errors == 1 { "" } else { "s" }
            ));
        }
        let msg = format!("Summary: {} found.", parts.join(", "));
        if errors > 0 {
            print!("{}", msg.red());
        } else {
            print!("{}", msg.yellow());
        }
        println!(" Run `gust build` to regenerate stale files.");
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
