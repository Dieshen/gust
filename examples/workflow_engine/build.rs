use std::fs;
use std::path::Path;

fn main() {
    // Write engine_failure.gu from gust-stdlib into src/ so that
    // compile_gust_files() generates engine_failure.g.rs alongside workflow.g.rs.
    // The EngineFailure enum definition lives in the generated engine_failure.g.rs;
    // the `use std::EngineFailure;` declaration in workflow.gu is a Gust-level
    // import that the checker recognises but that the Rust codegen emits literally.
    // We strip that literal `use std::EngineFailure;` line from the generated
    // workflow.g.rs so the Rust compiler sees the type defined in engine_failure.g.rs
    // without a broken import.
    let src_dir = Path::new("src");
    let engine_failure_gu = src_dir.join("engine_failure.gu");

    // Only overwrite if the content changed to avoid spurious rebuilds.
    let stdlib_source = gust_stdlib::ENGINE_FAILURE;
    let needs_write = fs::read_to_string(&engine_failure_gu)
        .map(|existing| existing != stdlib_source)
        .unwrap_or(true);
    if needs_write {
        fs::write(&engine_failure_gu, stdlib_source)
            .expect("failed to write engine_failure.gu from stdlib");
    }
    println!("cargo:rerun-if-changed=src/engine_failure.gu");

    // Compile all .gu files (engine_failure.gu + workflow.gu) to .g.rs files.
    if let Err(err) = gust_build::compile_gust_files() {
        panic!("gust build failed: {err}");
    }

    // workflow.g.rs contains `use std::EngineFailure;` because workflow.gu
    // declares `use std::EngineFailure;`. That line is valid in Gust but
    // not in Rust (EngineFailure is not in std::). Strip it so that the Rust
    // compiler uses the EngineFailure definition emitted by engine_failure.g.rs.
    let workflow_g_rs = src_dir.join("workflow.g.rs");
    if workflow_g_rs.exists() {
        let content = fs::read_to_string(&workflow_g_rs)
            .expect("failed to read workflow.g.rs");
        let patched = content
            .lines()
            .filter(|line| line.trim() != "use std::EngineFailure;")
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        if patched != content {
            fs::write(&workflow_g_rs, patched)
                .expect("failed to patch workflow.g.rs");
        }
    }
}
