use clap::{Parser, Subcommand};
use gust_lang::{
    format_program, parse_program, parse_program_with_errors, validate_program, GoCodegen,
    RustCodegen,
};
use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;
use walkdir::WalkDir;

#[derive(Parser)]
#[command(name = "gust", version, about = "The Gust programming language compiler")]
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
            let out_file = compile_single_file(&input, output.as_deref(), &target, package.as_deref())
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
                let status = Command::new("cargo").arg("build").status().expect("failed to run cargo");
                if !status.success() {
                    std::process::exit(1);
                }
            }
        }
        Commands::Watch { dir, target, package } => {
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
        Commands::Diagram { input, output } => {
            let diagram = generate_mermaid_diagram(&input).unwrap_or_else(|e| {
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
    }
}

fn init_project(name: &str) -> Result<(), String> {
    let root = PathBuf::from(name);
    if root.exists() {
        return Err(format!("directory '{}' already exists", root.display()));
    }
    fs::create_dir_all(root.join("src")).map_err(|e| format!("cannot create project dirs: {e}"))?;

    let cargo_toml = format!(
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
    fs::write(root.join("Cargo.toml"), cargo_toml).map_err(|e| format!("write Cargo.toml failed: {e}"))?;

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

fn format_file(input: &Path) -> Result<(), String> {
    let source = fs::read_to_string(input).map_err(|e| format!("cannot read '{}': {e}", input.display()))?;
    let program = parse_program_with_errors(&source, &input.display().to_string())
        .map_err(|e| e.render(&source))?;
    let formatted = format_program(&program);
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

fn generate_mermaid_diagram(input: &Path) -> Result<String, String> {
    let source = fs::read_to_string(input).map_err(|e| format!("cannot read '{}': {e}", input.display()))?;
    let program = parse_program_with_errors(&source, &input.display().to_string())
        .map_err(|e| e.render(&source))?;
    let machine = program
        .machines
        .first()
        .ok_or_else(|| "no machine declaration found".to_string())?;
    let mut out = String::from("stateDiagram-v2\n");
    if let Some(first) = machine.states.first() {
        out.push_str(&format!("    [*] --> {}\n", first.name));
    }
    for t in &machine.transitions {
        for target in &t.targets {
            out.push_str(&format!("    {} --> {} : {}\n", t.from, target, t.name));
        }
    }
    Ok(out)
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
                    if !matches!(event.kind, DebouncedEventKind::Any | DebouncedEventKind::AnyContinuous) {
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
    let source = fs::read_to_string(input).map_err(|e| format!("cannot read '{}': {e}", input.display()))?;
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
                fs::create_dir_all(output_dir)
                    .map_err(|e| format!("cannot create output dir '{}': {e}", output_dir.display()))?;
            }
            fs::write(&out_file, rust_code)
                .map_err(|e| format!("cannot write '{}': {e}", out_file.display()))?;
            Ok(out_file)
        }
        "go" => {
            let package_name = package.map(ToOwned::to_owned).unwrap_or_else(|| stem.replace(['-', ' '], "_"));
            let go_code = GoCodegen::new().generate(&program, &package_name);
            let out_file = generated_output_path(input, output, target)?;
            if let Some(output_dir) = output {
                fs::create_dir_all(output_dir)
                    .map_err(|e| format!("cannot create output dir '{}': {e}", output_dir.display()))?;
            }
            fs::write(&out_file, go_code)
                .map_err(|e| format!("cannot write '{}': {e}", out_file.display()))?;
            Ok(out_file)
        }
        other => Err(format!("unsupported target '{other}'. Use 'rust' or 'go'")),
    }
}

fn delete_generated_file(input: &Path, target: &str) -> Result<Option<PathBuf>, String> {
    let out_file = generated_output_path(input, None, target)?;
    if out_file.exists() {
        fs::remove_file(&out_file).map_err(|e| format!("cannot remove '{}': {e}", out_file.display()))?;
        Ok(Some(out_file))
    } else {
        Ok(None)
    }
}

fn generated_output_path(input: &Path, output: Option<&Path>, target: &str) -> Result<PathBuf, String> {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("invalid filename '{}'", input.display()))?;
    let filename = match target {
        "rust" => format!("{stem}.g.rs"),
        "go" => format!("{stem}.g.go"),
        other => return Err(format!("unsupported target '{other}'. Use 'rust' or 'go'")),
    };
    Ok(if let Some(output_dir) = output {
        output_dir.join(filename)
    } else {
        input.parent().unwrap_or_else(|| Path::new(".")).join(filename)
    })
}
