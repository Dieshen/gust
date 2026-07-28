//! Compiles generated output with each backend's real toolchain.
//!
//! Every other codegen test asserts on strings. Strings do not tell you whether
//! `rustc` accepts the result, and three backends — wasm, no_std, and ffi — had
//! never once had their output fed to a compiler. Two of the three did not
//! compile at all when first tried.
//!
//! The table below is the point: adding a fixture exercises every backend, so
//! coverage cannot drift backend-by-backend the way it did before.

use gust_lang::ast::Program;
use gust_lang::{CffiCodegen, GoCodegen, NoStdCodegen, RustCodegen, WasmCodegen, parse_program};
use std::path::{Path, PathBuf};
use std::process::Command;

// ─── fixtures ───────────────────────────────────────────────────────────────

struct Fixture {
    name: &'static str,
    source: &'static str,
}

/// Exercises user types, a fieldless enum, state fields, sync and async
/// effects, ctx rewriting, and branching — the shapes that broke in practice.
const RICH: &str = r#"
enum Tier { Fast, Slow }

type Config { service_name: String, retries: i64, tier: Tier }

machine DeployPipeline {
    state Idle(config: Config)
    state Running(config: Config, attempt: i64)
    state Done(message: String)
    state Failed(reason: String)

    transition start: Idle -> Running
    transition finish: Running -> Done | Failed

    async effect deploy(name: String) -> String
    effect log(msg: String) -> bool

    async on start(ctx: StartCtx) {
        let result = perform deploy(ctx.config.service_name);
        perform log(result);
        goto Running(ctx.config, 1);
    }

    async on finish(ctx: FinishCtx) {
        if ctx.attempt > ctx.config.retries {
            goto Failed("max retries exceeded");
        } else {
            let msg = perform deploy(ctx.config.service_name);
            goto Done(msg);
        }
    }
}
"#;

/// Fieldless states only. This is the shape the no_std backend supports, and
/// keeping it separate isolates which fixtures a backend can handle.
const FIELDLESS: &str = r#"
machine Toggle {
    state Off
    state On

    transition flip: Off -> On
    transition reset: On -> Off
}
"#;

/// A `let` whose value is never read. Go rejects an unused local outright and
/// Rust's `unused_variables` fails a consumer building with `-D warnings`, so
/// both backends have to lower this to a discard. See #100.
const UNUSED_BINDING: &str = r#"
machine Probe {
    state Idle(id: String)
    state Done(id: String)

    transition go: Idle -> Done

    effect check(a: String) -> bool

    on go(ctx: GoCtx) {
        let unread = perform check(ctx.id);
        goto Done(ctx.id);
    }
}
"#;

fn fixtures() -> Vec<Fixture> {
    vec![
        Fixture {
            name: "rich",
            source: RICH,
        },
        Fixture {
            name: "fieldless",
            source: FIELDLESS,
        },
        Fixture {
            name: "unused-binding",
            source: UNUSED_BINDING,
        },
    ]
}

// ─── backends ───────────────────────────────────────────────────────────────

/// How a backend's output is proven to be real code.
enum Verify {
    /// Build a crate and run cargo against it.
    Cargo {
        /// Extra `[dependencies]` lines for the generated crate.
        deps: &'static str,
        /// rustup target triple, when the backend does not build for host.
        target: Option<&'static str>,
        /// Deny all warnings, not just errors.
        deny_warnings: bool,
    },
    /// Write a Go package and run `go vet`.
    GoVet,
}

struct Backend {
    name: &'static str,
    generate: fn(&Program) -> String,
    verify: Verify,
    /// Fixtures this backend cannot handle yet, each with the tracking issue.
    /// Listed explicitly rather than silently skipped so the gap stays visible.
    unsupported: &'static [(&'static str, &'static str)],
}

fn backends() -> Vec<Backend> {
    vec![
        Backend {
            name: "rust",
            generate: |p| RustCodegen::new().generate(p),
            verify: Verify::Cargo {
                deps: "gust-runtime = { path = \"GUST_RUNTIME_PATH\" }\n\
                       serde = { version = \"1.0\", features = [\"derive\"] }\n\
                       tokio = { version = \"1\", features = [\"full\"] }\n\
                       thiserror = \"2.0\"",
                target: None,
                // Consumers build with -D warnings; output that merely compiles
                // still breaks them.
                deny_warnings: true,
            },
            unsupported: &[],
        },
        Backend {
            name: "wasm",
            generate: |p| WasmCodegen::new().generate(p),
            verify: Verify::Cargo {
                deps: "wasm-bindgen = \"0.2\"\n\
                       wasm-bindgen-futures = \"0.4\"\n\
                       js-sys = \"0.3\"",
                target: Some("wasm32-unknown-unknown"),
                // wasm_bindgen's own expansion emits warnings we do not control.
                deny_warnings: false,
            },
            unsupported: &[],
        },
        Backend {
            name: "ffi",
            // The second element is the C header. Verifying that would need a C
            // toolchain in CI, so only the Rust half is compiled here.
            generate: |p| CffiCodegen::new().generate(p).0,
            verify: Verify::Cargo {
                deps: "",
                target: None,
                deny_warnings: false,
            },
            unsupported: &[],
        },
        Backend {
            name: "nostd",
            generate: |p| NoStdCodegen::new().generate(p),
            verify: Verify::Cargo {
                deps: "heapless = \"0.8\"",
                target: None,
                deny_warnings: false,
            },
            unsupported: &[],
        },
        Backend {
            name: "go",
            generate: |p| GoCodegen::new().generate(p, "main"),
            verify: Verify::GoVet,
            unsupported: &[],
        },
    ]
}

// ─── harness ────────────────────────────────────────────────────────────────

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("gust-lang has a workspace parent")
        .to_path_buf()
}

fn toml_path(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

fn installed_targets() -> String {
    Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// Builds a crate around `generated` and runs cargo over it. Returns the
/// combined diagnostics on failure.
fn verify_with_cargo(
    generated: &str,
    label: &str,
    deps: &str,
    target: Option<&str>,
    deny_warnings: bool,
) -> Result<(), String> {
    let root = workspace_root();
    let dir = tempfile::tempdir().expect("create tempdir");
    let src = dir.path().join("src");
    std::fs::create_dir(&src).expect("create src");
    std::fs::write(src.join("lib.rs"), generated).expect("write lib.rs");

    let deps = deps.replace("GUST_RUNTIME_PATH", &toml_path(&root.join("gust-runtime")));
    // Edition 2021, not 2024: consumers are not all on the newer edition, and
    // generated code has to be valid on both.
    std::fs::write(
        dir.path().join("Cargo.toml"),
        format!(
            "[package]\nname = \"gust-backend-{label}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
             [dependencies]\n{deps}\n\n[workspace]\n"
        ),
    )
    .expect("write Cargo.toml");

    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg(if deny_warnings { "clippy" } else { "check" })
        .arg("--quiet");
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    if deny_warnings {
        cmd.args(["--", "-D", "warnings"]);
    }
    // A shared target dir keeps dependency builds cached across the whole
    // table instead of recompiling per cell.
    let output = cmd
        .current_dir(dir.path())
        .env("CARGO_TARGET_DIR", root.join("target/codegen-backends"))
        .output()
        .expect("cargo should run");

    if output.status.success() {
        return Ok(());
    }
    Err(String::from_utf8_lossy(&output.stderr).into_owned())
}

fn verify_with_go_vet(generated: &str) -> Result<(), String> {
    let dir = tempfile::tempdir().expect("create tempdir");
    std::fs::write(dir.path().join("machine.go"), generated).expect("write go file");
    std::fs::write(dir.path().join("go.mod"), "module testpkg\n\ngo 1.21\n").expect("write go.mod");

    let output = Command::new("go")
        .args(["vet", "./..."])
        .current_dir(dir.path())
        .output()
        .expect("go vet should run");

    if output.status.success() {
        return Ok(());
    }
    Err(String::from_utf8_lossy(&output.stderr).into_owned())
}

/// The whole table in one test. A single test rather than one per cell so the
/// report lists every failing combination at once instead of stopping at the
/// first — when a codegen change breaks three backends, you want to see three.
#[test]
fn every_backend_produces_code_its_toolchain_accepts() {
    let targets = installed_targets();
    let mut failures: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut checked = 0usize;

    for fixture in fixtures() {
        let program = parse_program(fixture.source)
            .unwrap_or_else(|e| panic!("fixture '{}' should parse: {e}", fixture.name));

        for backend in backends() {
            let cell = format!("{}/{}", backend.name, fixture.name);

            if let Some((_, why)) = backend.unsupported.iter().find(|(f, _)| *f == fixture.name) {
                skipped.push(format!("{cell} — unsupported, {why}"));
                continue;
            }

            let generated = (backend.generate)(&program);

            let result = match &backend.verify {
                Verify::Cargo {
                    deps,
                    target,
                    deny_warnings,
                } => {
                    if let Some(t) = target {
                        if !targets.contains(t) {
                            skipped.push(format!("{cell} — rustup target '{t}' not installed"));
                            continue;
                        }
                    }
                    verify_with_cargo(
                        &generated,
                        &cell.replace('/', "-"),
                        deps,
                        *target,
                        *deny_warnings,
                    )
                }
                Verify::GoVet => verify_with_go_vet(&generated),
            };

            checked += 1;
            if let Err(diagnostics) = result {
                failures.push(format!(
                    "\n=== {cell} ===\n--- generated ---\n{generated}\n--- diagnostics ---\n{diagnostics}"
                ));
            }
        }
    }

    // Printed rather than silent: a skipped cell is not a passing cell, and the
    // reason it was skipped is the thing worth noticing.
    for entry in &skipped {
        eprintln!("skipped: {entry}");
    }

    assert!(
        failures.is_empty(),
        "{} of {checked} backend/fixture combinations produced code their toolchain rejected:{}",
        failures.len(),
        failures.join("")
    );
    assert!(checked > 0, "harness verified nothing");
}
