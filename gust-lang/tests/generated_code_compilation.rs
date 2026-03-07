use gust_lang::{parse_program, GoCodegen, RustCodegen};

/// A simple Gust machine that exercises common codegen patterns:
/// state fields, effects (sync + async), ctx rewrite, goto, if/else.
fn fixture_source() -> &'static str {
    r#"
type Config { service_name: String, retries: i64 }

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
"#
}

fn multiline_string_fixture() -> &'static str {
    r#"
machine Escaping {
    state Start
    state Done(msg: String)

    transition finish: Start -> Done

    on finish() {
        goto Done("line1
line2\path");
    }
}
"#
}

#[test]
fn generated_go_passes_vet() {
    let program = parse_program(fixture_source()).expect("fixture should parse");
    let generated = GoCodegen::new().generate(&program, "main");

    let dir = tempfile::tempdir().expect("create tempdir");
    let go_file = dir.path().join("pipeline.go");
    std::fs::write(&go_file, &generated).expect("write go file");

    // Create go.mod
    let go_mod = dir.path().join("go.mod");
    std::fs::write(&go_mod, "module testpkg\n\ngo 1.21\n").expect("write go.mod");

    let output = std::process::Command::new("go")
        .args(["vet", "./..."])
        .current_dir(dir.path())
        .output()
        .expect("go vet should run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "go vet failed:\n--- generated code ---\n{generated}\n--- stderr ---\n{stderr}"
    );
}

#[test]
fn generated_go_escapes_multiline_strings() {
    let program = parse_program(multiline_string_fixture()).expect("fixture should parse");
    let generated = GoCodegen::new().generate(&program, "main");
    assert!(generated.contains("\"line1\\nline2\\\\path\""));

    let dir = tempfile::tempdir().expect("create tempdir");
    let go_file = dir.path().join("escaping.go");
    std::fs::write(&go_file, &generated).expect("write go file");
    std::fs::write(dir.path().join("go.mod"), "module testpkg\n\ngo 1.21\n").expect("write go.mod");

    let output = std::process::Command::new("go")
        .args(["vet", "./..."])
        .current_dir(dir.path())
        .output()
        .expect("go vet should run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "go vet failed:\n--- generated code ---\n{generated}\n--- stderr ---\n{stderr}"
    );
}

#[test]
fn generated_rust_escapes_multiline_strings() {
    let program = parse_program(multiline_string_fixture()).expect("fixture should parse");
    let generated = RustCodegen::new().generate(&program);
    assert!(generated.contains("\"line1\\nline2\\\\path\".to_string()"));
}

#[test]
#[ignore] // Slower — run with `cargo test -- --ignored`
fn generated_rust_passes_cargo_check() {
    let program = parse_program(fixture_source()).expect("fixture should parse");
    let generated = RustCodegen::new().generate(&program);

    let dir = tempfile::tempdir().expect("create tempdir");

    // Write generated source as src/lib.rs
    let src_dir = dir.path().join("src");
    std::fs::create_dir(&src_dir).expect("create src dir");
    std::fs::write(src_dir.join("lib.rs"), &generated).expect("write lib.rs");

    // Absolute path to gust-runtime for path dependency
    let runtime_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("gust-runtime");

    let cargo_toml = format!(
        r#"[package]
name = "gust-compilation-test"
version = "0.1.0"
edition = "2021"

[dependencies]
gust-runtime = {{ path = "{}" }}
serde = {{ version = "1.0", features = ["derive"] }}
tokio = {{ version = "1", features = ["full"] }}
thiserror = "2.0"
"#,
        runtime_path.display()
    );
    std::fs::write(dir.path().join("Cargo.toml"), &cargo_toml).expect("write Cargo.toml");

    let output = std::process::Command::new("cargo")
        .args(["check"])
        .current_dir(dir.path())
        .output()
        .expect("cargo check should run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "cargo check failed:\n--- generated code ---\n{generated}\n--- stderr ---\n{stderr}"
    );
}
