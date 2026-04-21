use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

/// A minimal valid Gust program used as a test fixture.
const VALID_GU: &str = r#"machine Light {
    state Off()
    state On()
    transition toggle: Off -> On
    transition turn_off: On -> Off
    on toggle(ctx: Off) {
        goto On();
    }
    on turn_off(ctx: On) {
        goto Off();
    }
}
"#;

/// A syntactically invalid Gust program.
const INVALID_GU: &str = r#"machine Broken {
    state Off(
}
"#;

/// A semantically invalid Gust program (references nonexistent state).
const SEMANTIC_ERROR_GU: &str = r#"machine Bad {
    state Off()
    transition go: Off -> Nowhere
    on go(ctx: Off) {
        goto Nowhere();
    }
}
"#;

/// Helper: create a temp directory with a .gu file and return (dir, file_path).
fn write_fixture(content: &str, filename: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempdir().expect("create tempdir");
    let path = dir.path().join(filename);
    fs::write(&path, content).expect("write fixture file");
    (dir, path)
}

fn gust_cmd() -> Command {
    Command::cargo_bin("gust").expect("binary 'gust' should be built")
}

// ─── build subcommand ────────────────────────────────────────────────────────

#[test]
fn build_rust_produces_g_rs_file() {
    let (_dir, gu_path) = write_fixture(VALID_GU, "light.gu");

    gust_cmd()
        .args(["build", gu_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains(".g.rs"));

    let generated = gu_path.with_extension("g.rs");
    assert!(generated.exists(), "expected {generated:?} to exist");
    let content = fs::read_to_string(&generated).unwrap();
    assert!(
        content.contains("Light"),
        "generated Rust code should reference the machine name"
    );
}

#[test]
fn build_rust_with_output_dir() {
    let (_dir, gu_path) = write_fixture(VALID_GU, "light.gu");
    let out_dir = _dir.path().join("out");

    gust_cmd()
        .args([
            "build",
            gu_path.to_str().unwrap(),
            "--output",
            out_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    let generated = out_dir.join("light.g.rs");
    assert!(generated.exists(), "expected {generated:?} in output dir");
}

#[test]
fn build_go_produces_g_go_file() {
    let (_dir, gu_path) = write_fixture(VALID_GU, "light.gu");

    gust_cmd()
        .args([
            "build",
            gu_path.to_str().unwrap(),
            "--target",
            "go",
            "--package",
            "mypkg",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(".g.go"));

    let generated = gu_path.with_extension("g.go");
    assert!(generated.exists(), "expected {generated:?} to exist");
    let content = fs::read_to_string(&generated).unwrap();
    assert!(
        content.contains("mypkg"),
        "generated Go code should contain the package name"
    );
}

#[test]
fn build_wasm_produces_g_wasm_rs_file() {
    let (_dir, gu_path) = write_fixture(VALID_GU, "light.gu");

    gust_cmd()
        .args(["build", gu_path.to_str().unwrap(), "--target", "wasm"])
        .assert()
        .success()
        .stdout(predicate::str::contains(".g.wasm.rs"));

    let generated = gu_path.parent().unwrap().join("light.g.wasm.rs");
    assert!(generated.exists(), "expected {generated:?} to exist");
}

#[test]
fn build_nostd_produces_g_nostd_rs_file() {
    let (_dir, gu_path) = write_fixture(VALID_GU, "light.gu");

    gust_cmd()
        .args(["build", gu_path.to_str().unwrap(), "--target", "nostd"])
        .assert()
        .success()
        .stdout(predicate::str::contains(".g.nostd.rs"));

    let generated = gu_path.parent().unwrap().join("light.g.nostd.rs");
    assert!(generated.exists(), "expected {generated:?} to exist");
}

#[test]
fn build_ffi_produces_rs_and_header() {
    let (_dir, gu_path) = write_fixture(VALID_GU, "light.gu");

    gust_cmd()
        .args(["build", gu_path.to_str().unwrap(), "--target", "ffi"])
        .assert()
        .success()
        .stdout(predicate::str::contains(".g.ffi.rs"));

    let rs_file = gu_path.parent().unwrap().join("light.g.ffi.rs");
    let h_file = gu_path.parent().unwrap().join("light.g.h");
    assert!(rs_file.exists(), "expected FFI .rs file");
    assert!(h_file.exists(), "expected FFI .h header file");
}

#[test]
fn build_invalid_target_fails() {
    let (_dir, gu_path) = write_fixture(VALID_GU, "light.gu");

    gust_cmd()
        .args(["build", gu_path.to_str().unwrap(), "--target", "java"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported target"));
}

#[test]
fn build_missing_file_fails() {
    gust_cmd()
        .args(["build", "/nonexistent/path/foo.gu"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot read"));
}

#[test]
fn build_invalid_syntax_fails() {
    let (_dir, gu_path) = write_fixture(INVALID_GU, "broken.gu");

    gust_cmd()
        .args(["build", gu_path.to_str().unwrap()])
        .assert()
        .failure();
}

// ─── check subcommand ────────────────────────────────────────────────────────

#[test]
fn check_valid_file_succeeds() {
    let (_dir, gu_path) = write_fixture(VALID_GU, "light.gu");

    gust_cmd()
        .args(["check", gu_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Check passed"));
}

#[test]
fn check_invalid_syntax_fails() {
    let (_dir, gu_path) = write_fixture(INVALID_GU, "broken.gu");

    gust_cmd()
        .args(["check", gu_path.to_str().unwrap()])
        .assert()
        .failure();
}

#[test]
fn check_semantic_error_shows_diagnostics() {
    let (_dir, gu_path) = write_fixture(SEMANTIC_ERROR_GU, "bad.gu");

    gust_cmd()
        .args(["check", gu_path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Nowhere"));
}

#[test]
fn check_missing_file_fails() {
    gust_cmd()
        .args(["check", "/nonexistent/path/foo.gu"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot read"));
}

// ─── fmt subcommand ──────────────────────────────────────────────────────────

#[test]
fn fmt_formats_a_valid_file() {
    // Use poorly formatted source to verify formatting occurs
    let messy_gu = "machine Light {\nstate Off()\n  state On()\ntransition toggle: Off -> On\ntransition turn_off: On -> Off\non toggle(ctx: Off) {\ngoto On();\n}\non turn_off(ctx: On) {\ngoto Off();\n}\n}\n";
    let (_dir, gu_path) = write_fixture(messy_gu, "light.gu");

    gust_cmd()
        .args(["fmt", gu_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Formatted"));

    let formatted = fs::read_to_string(&gu_path).unwrap();
    // After formatting, indentation should be consistent (4 spaces)
    assert!(
        formatted.contains("    state Off"),
        "expected formatted output to have consistent indentation"
    );
}

#[test]
fn fmt_missing_file_fails() {
    gust_cmd()
        .args(["fmt", "/nonexistent/path/foo.gu"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot read"));
}

// ─── parse subcommand ────────────────────────────────────────────────────────

#[test]
fn parse_outputs_ast_debug() {
    let (_dir, gu_path) = write_fixture(VALID_GU, "light.gu");

    gust_cmd()
        .args(["parse", gu_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Light"))
        .stdout(predicate::str::contains("Off"))
        .stdout(predicate::str::contains("On"))
        .stdout(predicate::str::contains("toggle"));
}

#[test]
fn parse_invalid_syntax_fails() {
    let (_dir, gu_path) = write_fixture(INVALID_GU, "broken.gu");

    gust_cmd()
        .args(["parse", gu_path.to_str().unwrap()])
        .assert()
        .failure();
}

#[test]
fn parse_missing_file_fails() {
    gust_cmd()
        .args(["parse", "/nonexistent/path/foo.gu"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot read"));
}

// ─── diagram subcommand ──────────────────────────────────────────────────────

#[test]
fn diagram_outputs_mermaid_to_stdout() {
    let (_dir, gu_path) = write_fixture(VALID_GU, "light.gu");

    gust_cmd()
        .args(["diagram", gu_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("stateDiagram-v2"))
        .stdout(predicate::str::contains("Off"))
        .stdout(predicate::str::contains("On"))
        .stdout(predicate::str::contains("toggle"));
}

#[test]
fn diagram_writes_to_output_file() {
    let (_dir, gu_path) = write_fixture(VALID_GU, "light.gu");
    let out_file = _dir.path().join("diagram.md");

    gust_cmd()
        .args([
            "diagram",
            gu_path.to_str().unwrap(),
            "--output",
            out_file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Wrote"));

    let content = fs::read_to_string(&out_file).unwrap();
    assert!(
        content.contains("stateDiagram-v2"),
        "output file should contain Mermaid diagram"
    );
}

#[test]
fn diagram_filters_by_machine_name() {
    let (_dir, gu_path) = write_fixture(VALID_GU, "light.gu");

    gust_cmd()
        .args(["diagram", gu_path.to_str().unwrap(), "--machine", "Light"])
        .assert()
        .success()
        .stdout(predicate::str::contains("stateDiagram-v2"));
}

#[test]
fn diagram_unknown_machine_fails() {
    let (_dir, gu_path) = write_fixture(VALID_GU, "light.gu");

    gust_cmd()
        .args([
            "diagram",
            gu_path.to_str().unwrap(),
            "--machine",
            "NonExistent",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn diagram_missing_file_fails() {
    gust_cmd()
        .args(["diagram", "/nonexistent/path/foo.gu"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot read"));
}

// ─── init subcommand ─────────────────────────────────────────────────────────

#[test]
fn init_creates_project_scaffold() {
    let dir = tempdir().expect("create tempdir");
    let project_name = "test_project";

    gust_cmd()
        .current_dir(dir.path())
        .args(["init", project_name])
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized"));

    let project_dir = dir.path().join(project_name);
    assert!(project_dir.join("Cargo.toml").exists(), "Cargo.toml");
    assert!(project_dir.join("build.rs").exists(), "build.rs");
    assert!(project_dir.join("src/main.rs").exists(), "src/main.rs");
    assert!(
        project_dir.join("src/payment.gu").exists(),
        "src/payment.gu"
    );
    assert!(project_dir.join("README.md").exists(), "README.md");
}

#[test]
fn init_fails_if_directory_exists() {
    let dir = tempdir().expect("create tempdir");
    let project_name = "existing_dir";
    fs::create_dir(dir.path().join(project_name)).expect("create dir");

    gust_cmd()
        .current_dir(dir.path())
        .args(["init", project_name])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn init_rejects_invalid_project_name() {
    let dir = tempdir().expect("create tempdir");

    gust_cmd()
        .current_dir(dir.path())
        .args(["init", "bad name"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Cargo compatibility"));
}

// ─── general CLI behavior ────────────────────────────────────────────────────

#[test]
fn no_args_shows_help() {
    gust_cmd()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}

#[test]
fn version_flag_shows_version() {
    gust_cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("gust"));
}

#[test]
fn help_flag_shows_help() {
    gust_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Gust"))
        .stdout(predicate::str::contains("build"))
        .stdout(predicate::str::contains("check"))
        .stdout(predicate::str::contains("fmt"))
        .stdout(predicate::str::contains("parse"))
        .stdout(predicate::str::contains("diagram"))
        .stdout(predicate::str::contains("init"));
}

// ─── doctor subcommand ──────────────────────────────────────────────────────

#[test]
fn doctor_prints_all_sections_in_empty_dir() {
    let dir = tempdir().expect("create tempdir");

    gust_cmd()
        .current_dir(dir.path())
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("Gust Doctor"))
        .stdout(predicate::str::contains("Rust"))
        .stdout(predicate::str::contains("Cargo"))
        .stdout(predicate::str::contains("Gust"))
        .stdout(predicate::str::contains("Project"))
        .stdout(predicate::str::contains("Cargo.toml"));
}

#[test]
fn doctor_detects_cargo_toml_and_gust_build_dep() {
    let dir = tempdir().expect("create tempdir");
    let cargo_toml = r#"[package]
name = "demo"
version = "0.1.0"
edition = "2021"

[build-dependencies]
gust-build = "0.1"
"#;
    fs::write(dir.path().join("Cargo.toml"), cargo_toml).expect("write Cargo.toml");

    gust_cmd()
        .current_dir(dir.path())
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("Cargo.toml"))
        .stdout(predicate::str::contains("gust-build dependency"))
        .stdout(predicate::str::contains("found"));
}

#[test]
fn doctor_validates_gu_files_in_cwd() {
    let (dir, _) = write_fixture(VALID_GU, "light.gu");

    gust_cmd()
        .current_dir(dir.path())
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("light.gu"));
}

#[test]
fn doctor_reports_semantic_errors_in_gu_files() {
    let (dir, _) = write_fixture(SEMANTIC_ERROR_GU, "bad.gu");

    gust_cmd()
        .current_dir(dir.path())
        .arg("doctor")
        .assert()
        // doctor does not itself fail — it only reports. Exit status is success.
        .success()
        .stdout(predicate::str::contains("bad.gu"));
}

// ─── schema subcommand ──────────────────────────────────────────────────────

#[test]
fn schema_emits_json_to_stdout() {
    let (_dir, gu_path) = write_fixture(VALID_GU, "light.gu");

    gust_cmd()
        .args(["schema", gu_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"$schema\""))
        .stdout(predicate::str::contains("Light"));
}

#[test]
fn schema_writes_to_output_file() {
    let (dir, gu_path) = write_fixture(VALID_GU, "light.gu");
    let out = dir.path().join("schema.json");

    gust_cmd()
        .args([
            "schema",
            gu_path.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
        ])
        .assert()
        .success();

    let content = fs::read_to_string(&out).expect("read schema");
    assert!(content.contains("\"$schema\""));
}

#[test]
fn schema_missing_file_fails() {
    gust_cmd()
        .args(["schema", "/nonexistent/foo.gu"])
        .assert()
        .failure();
}

// ─── build flag variants ────────────────────────────────────────────────────

#[test]
fn build_with_tracing_flag_emits_tracing_imports() {
    let (dir, gu_path) = write_fixture(VALID_GU, "light.gu");

    gust_cmd()
        .args(["build", gu_path.to_str().unwrap(), "--tracing"])
        .assert()
        .success();

    let out_path = dir.path().join("light.g.rs");
    let content = fs::read_to_string(&out_path).expect("read generated");
    assert!(
        content.contains("tracing"),
        "tracing-enabled codegen should mention tracing in output"
    );
}

#[test]
fn build_go_defaults_package_to_file_stem_when_omitted() {
    let (dir, gu_path) = write_fixture(VALID_GU, "light.gu");

    // Go codegen falls back to the file stem as the package name when
    // `--package` is not supplied. Verify the generated .g.go contains it.
    gust_cmd()
        .args(["build", gu_path.to_str().unwrap(), "--target", "go"])
        .assert()
        .success();

    let out = dir.path().join("light.g.go");
    let content = fs::read_to_string(&out).expect("read generated");
    assert!(
        content.contains("package light"),
        "expected fallback package name to match file stem, got:\n{content}"
    );
}

#[test]
fn build_go_respects_explicit_package_flag() {
    let (dir, gu_path) = write_fixture(VALID_GU, "light.gu");

    gust_cmd()
        .args([
            "build",
            gu_path.to_str().unwrap(),
            "--target",
            "go",
            "--package",
            "customsvc",
        ])
        .assert()
        .success();

    let out = dir.path().join("light.g.go");
    let content = fs::read_to_string(&out).expect("read generated");
    assert!(content.contains("package customsvc"));
}

#[test]
fn build_rust_rebuild_overwrites_existing_output() {
    let (dir, gu_path) = write_fixture(VALID_GU, "light.gu");

    // First build
    gust_cmd()
        .args(["build", gu_path.to_str().unwrap()])
        .assert()
        .success();

    let out = dir.path().join("light.g.rs");
    assert!(out.exists());

    // Touch output to a known state, then rebuild — must regenerate.
    fs::write(&out, "// placeholder\n").expect("write placeholder");
    gust_cmd()
        .args(["build", gu_path.to_str().unwrap()])
        .assert()
        .success();

    let content = fs::read_to_string(&out).expect("read regenerated");
    assert!(
        !content.starts_with("// placeholder"),
        "build must overwrite prior generated file"
    );
}

// ─── init edge cases ────────────────────────────────────────────────────────

#[test]
fn init_rejects_empty_project_name() {
    let dir = tempdir().expect("create tempdir");

    gust_cmd()
        .current_dir(dir.path())
        .args(["init", ""])
        .assert()
        .failure()
        .stderr(predicate::str::contains("empty"));
}

#[test]
fn init_rejects_name_with_path_separator() {
    let dir = tempdir().expect("create tempdir");

    gust_cmd()
        .current_dir(dir.path())
        .args(["init", "foo/bar"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("path separators"));
}

#[test]
fn init_creates_valid_cargo_toml_in_standalone_dir() {
    let dir = tempdir().expect("create tempdir");

    gust_cmd()
        .current_dir(dir.path())
        .args(["init", "demo_proj"])
        .assert()
        .success();

    let cargo_toml =
        fs::read_to_string(dir.path().join("demo_proj").join("Cargo.toml")).expect("read");
    assert!(cargo_toml.contains("name = \"demo_proj\""));
    assert!(cargo_toml.contains("gust-build"));
    assert!(cargo_toml.contains("gust-runtime"));
    // When there IS no parent workspace, the init output omits [workspace].
    // When there IS a parent workspace (detected via walking up), [workspace]
    // is added to detach the new project. Both states are valid — we only
    // assert that the file is non-empty and references the expected crates.
}

#[test]
fn init_scaffold_produces_expected_files() {
    let dir = tempdir().expect("create tempdir");

    gust_cmd()
        .current_dir(dir.path())
        .args(["init", "proj2"])
        .assert()
        .success();

    let proj = dir.path().join("proj2");
    assert!(proj.join("Cargo.toml").exists());
    assert!(proj.join("build.rs").exists());
    assert!(proj.join("src/main.rs").exists());
    assert!(proj.join("src/payment.gu").exists());
    assert!(proj.join("README.md").exists());
}

// ─── check exit codes ───────────────────────────────────────────────────────

#[test]
fn check_semantic_error_returns_nonzero_exit() {
    let (_dir, gu_path) = write_fixture(SEMANTIC_ERROR_GU, "bad.gu");

    gust_cmd()
        .args(["check", gu_path.to_str().unwrap()])
        .assert()
        .code(predicate::ne(0));
}

#[test]
fn check_on_valid_file_returns_zero_exit() {
    let (_dir, gu_path) = write_fixture(VALID_GU, "light.gu");

    gust_cmd()
        .args(["check", gu_path.to_str().unwrap()])
        .assert()
        .code(0);
}

// ─── fmt edge cases ─────────────────────────────────────────────────────────

#[test]
fn fmt_rejects_malformed_source() {
    let (_dir, gu_path) = write_fixture(INVALID_GU, "broken.gu");

    gust_cmd()
        .args(["fmt", gu_path.to_str().unwrap()])
        .assert()
        .failure();
}

#[test]
fn fmt_idempotent_over_two_runs() {
    let (_dir, gu_path) = write_fixture(VALID_GU, "light.gu");

    gust_cmd()
        .args(["fmt", gu_path.to_str().unwrap()])
        .assert()
        .success();
    let first = fs::read_to_string(&gu_path).expect("read formatted");

    gust_cmd()
        .args(["fmt", gu_path.to_str().unwrap()])
        .assert()
        .success();
    let second = fs::read_to_string(&gu_path).expect("read re-formatted");

    assert_eq!(first, second, "formatter must be idempotent");
}
