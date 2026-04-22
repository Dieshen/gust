use std::fs;
use std::path::Path;

fn main() {
    // Copy engine_failure.gu from gust-stdlib into src/ so that
    // compile_gust_files() generates engine_failure.g.rs alongside
    // workflow.g.rs. Both files end up in the same module, which is how
    // the EngineFailure type becomes visible to workflow.g.rs. The
    // `use std::EngineFailure;` declaration in workflow.gu is stripped
    // at codegen time since Gust 0.2 (see #66/#67), so no post-processing
    // of the generated .g.rs is needed here.
    let src_dir = Path::new("src");
    let engine_failure_gu = src_dir.join("engine_failure.gu");

    let stdlib_source = gust_stdlib::ENGINE_FAILURE;
    let needs_write = fs::read_to_string(&engine_failure_gu)
        .map(|existing| existing != stdlib_source)
        .unwrap_or(true);
    if needs_write {
        fs::write(&engine_failure_gu, stdlib_source)
            .expect("failed to write engine_failure.gu from stdlib");
    }
    println!("cargo:rerun-if-changed=src/engine_failure.gu");

    if let Err(err) = gust_build::compile_gust_files() {
        panic!("gust build failed: {err}");
    }
}
