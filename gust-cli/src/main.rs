use clap::{Parser, Subcommand};
use gust_lang::{parse_program, GoCodegen, RustCodegen};
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
        /// Input .gu file
        #[arg(value_name = "FILE")]
        input: PathBuf,

        /// Output directory for generated code (default: alongside the .gu file)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Target language (rust or go)
        #[arg(short, long, default_value = "rust")]
        target: String,

        /// Package name for Go output (default: derived from filename)
        #[arg(short, long)]
        package: Option<String>,

        /// Also run `cargo build` on the generated output (Rust only)
        #[arg(long)]
        compile: bool,
    },
    /// Watch a directory and recompile .gu files on changes
    Watch {
        /// Directory to watch recursively
        #[arg(value_name = "DIR", default_value = ".")]
        dir: PathBuf,

        /// Target language (rust or go)
        #[arg(short, long, default_value = "rust")]
        target: String,

        /// Package name for Go output (default: derived from filename)
        #[arg(short, long)]
        package: Option<String>,
    },
    /// Parse a .gu file and print the AST (for debugging)
    Parse {
        /// Input .gu file
        #[arg(value_name = "FILE")]
        input: PathBuf,
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

            println!("✓ Generated {}", out_file.display());

            if compile {
                if target != "rust" {
                    eprintln!("warning: --compile flag is only supported for Rust target");
                    return;
                }
                println!("→ Running cargo build...");
                let status = Command::new("cargo")
                    .arg("build")
                    .status()
                    .expect("failed to run cargo");
                if !status.success() {
                    std::process::exit(1);
                }
                println!("✓ Build successful");
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
                eprintln!("error: {e}");
                std::process::exit(1);
            });

            println!("{program:#?}");
        }
    }
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
                        DebouncedEventKind::Any
                            | DebouncedEventKind::AnyContinuous
                    ) {
                        continue;
                    }
                    if event.path.extension().and_then(|e| e.to_str()) != Some("gu") {
                        continue;
                    }
                    if !event.path.exists() {
                        match delete_generated_file(&event.path, target) {
                            Ok(Some(path)) => println!("✓ Deleted {}", path.display()),
                            Ok(None) => {}
                            Err(err) => eprintln!("error: {err}"),
                        }
                        continue;
                    }
                    match compile_single_file(&event.path, None, target, package) {
                        Ok(out_file) => println!("✓ Recompiled {}", out_file.display()),
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
        println!("✓ Generated {}", out_file.display());
    }
    Ok(())
}

fn compile_single_file(
    input: &Path,
    output: Option<&Path>,
    target: &str,
    package: Option<&str>,
) -> Result<PathBuf, String> {
    let source = fs::read_to_string(input)
        .map_err(|e| format!("cannot read '{}': {e}", input.display()))?;
    let program = parse_program(&source).map_err(|e| format!("parse failed '{}': {e}", input.display()))?;
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
            let package_name = package
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| stem.replace(['-', ' '], "_"));
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
        fs::remove_file(&out_file)
            .map_err(|e| format!("cannot remove '{}': {e}", out_file.display()))?;
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
