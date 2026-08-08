use gust_lang::{GoCodegen, RustCodegen, parse_program};

/// A simple Gust machine that exercises common codegen patterns:
/// state fields, effects (sync + async), ctx rewrite, goto, if/else.
fn fixture_source() -> &'static str {
    r#"
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
    effect pick(tier: Tier) -> bool

    async on start(ctx) {
        let tier = ctx.config.tier;
        perform pick(tier);
        let result = perform deploy(ctx.config.service_name);
        perform log(result);
        goto Running(ctx.config, 1);
    }

    async on finish(ctx) {
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

/// Hand-written consumer code appended to the generated module. This is the
/// half that regressed in the field: gust only ever compiled the code it
/// emits, never code that *implements* a generated effect trait. Both the
/// partial-move bug and the `async_fn_in_trait` bug (both #99) were invisible
/// until a real project wrote this.
const CONSUMER_SOURCE: &str = r#"
struct TestEffects;

// Implementors must be able to write a plain `async fn` against the desugared
// RPITIT signature that the effect trait declares.
impl DeployPipelineEffects for TestEffects {
    async fn deploy(&self, name: &str) -> String {
        format!("deployed {name}")
    }
    fn log(&self, _msg: &str) -> bool {
        true
    }
    fn pick(&self, tier: &Tier) -> bool {
        matches!(tier, Tier::Fast)
    }
}

// The machine future must be Send: callers hold the machine across an await
// inside a spawned task, which is the reason the effect trait carries `+ Send`.
pub fn machine_future_is_send() {
    fn requires_send<T: Send>(_: T) {}
    requires_send(async {
        let config = Config {
            service_name: "svc".to_string(),
            retries: 3,
            tier: Tier::Fast,
        };
        let mut machine = DeployPipeline::new(config);
        machine.start(&TestEffects).await.unwrap();
    });
}
"#;

/// Lints the generated Rust *together with* code that implements the generated
/// effect trait. Runs on edition 2021 on purpose: consumers are not all on
/// 2024, and the effect trait's RPITIT return type has to be valid on both.
///
/// Uses `clippy -D warnings` rather than `cargo check` because that is what a
/// consumer's CI runs — gust's own CI included — and generated code that only
/// *compiles* still breaks those builds. `redundant_field_names`,
/// `cmp_owned`, and `new_without_default` all reached consumers this way.
///
/// This test builds a real crate, so it uses a dedicated target directory
/// under the workspace `target/` to keep dependency compilation cached between
/// runs instead of paying for it on every invocation.
#[test]
fn generated_rust_is_clippy_clean_with_a_trait_implementor() {
    let program = parse_program(fixture_source()).expect("fixture should parse");
    let generated = RustCodegen::new().generate(&program);

    let dir = tempfile::tempdir().expect("create tempdir");

    let src_dir = dir.path().join("src");
    std::fs::create_dir(&src_dir).expect("create src dir");
    // `async_fn_in_trait` is warn-by-default, so it has to be denied here or a
    // regression would compile cleanly and the test would pass.
    let lib_rs = format!("#![deny(async_fn_in_trait)]\n{generated}\n{CONSUMER_SOURCE}");
    std::fs::write(src_dir.join("lib.rs"), &lib_rs).expect("write lib.rs");

    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let runtime_path = workspace_root.join("gust-runtime");

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

[workspace]
"#,
        runtime_path.display().to_string().replace('\\', "/")
    );
    std::fs::write(dir.path().join("Cargo.toml"), &cargo_toml).expect("write Cargo.toml");

    let output = std::process::Command::new(env!("CARGO"))
        .args(["clippy", "--quiet", "--", "-D", "warnings"])
        .current_dir(dir.path())
        .env(
            "CARGO_TARGET_DIR",
            workspace_root.join("target/codegen-compile-test"),
        )
        .output()
        .expect("cargo clippy should run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "cargo clippy -D warnings failed:\n--- generated code ---\n{lib_rs}\n--- stderr ---\n{stderr}"
    );
}
