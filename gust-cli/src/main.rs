use clap::{Parser, Subcommand};
use gust_lang::{parse_program, RustCodegen};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser)]
#[command(name = "gust", version, about = "The Gust programming language compiler")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile a .gu file to Rust source
    Build {
        /// Input .gu file
        #[arg(value_name = "FILE")]
        input: PathBuf,

        /// Output directory for generated Rust (default: alongside the .gu file)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Also run `cargo build` on the generated output
        #[arg(long)]
        compile: bool,
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
            compile,
        } => {
            let source = fs::read_to_string(&input).unwrap_or_else(|e| {
                eprintln!("error: cannot read '{}': {e}", input.display());
                std::process::exit(1);
            });

            let program = parse_program(&source).unwrap_or_else(|e| {
                eprintln!("error: {e}");
                std::process::exit(1);
            });

            let rust_code = RustCodegen::new().generate(&program);

            // Determine output path: alongside .gu file as .g.rs, or in specified dir
            let stem = input.file_stem().unwrap().to_string_lossy();
            let out_file = if let Some(ref output_dir) = output {
                fs::create_dir_all(output_dir).unwrap_or_else(|e| {
                    eprintln!("error: cannot create output dir: {e}");
                    std::process::exit(1);
                });
                output_dir.join(format!("{stem}.g.rs"))
            } else {
                // Default: place .g.rs alongside the .gu source file
                let parent = input.parent().unwrap_or_else(|| std::path::Path::new("."));
                parent.join(format!("{stem}.g.rs"))
            };
            fs::write(&out_file, &rust_code).unwrap_or_else(|e| {
                eprintln!("error: cannot write output: {e}");
                std::process::exit(1);
            });

            println!("✓ Generated {}", out_file.display());

            if compile {
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
