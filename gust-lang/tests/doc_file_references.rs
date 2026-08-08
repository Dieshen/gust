//! Every source file the docs and skills name must exist.
//!
//! Prose cannot be compiled, so most documentation drift is caught by review or
//! not at all. This is the slice that does not need judgement: a backticked
//! path either resolves or it does not.
//!
//! It is also the slice that went wrong worst. When the skills were brought to
//! 1.0 they pointed at `codegen_wasm.rs`, `codegen_nostd.rs`,
//! `wasm_codegen_coverage.rs`, and `nostd_codegen_coverage.rs` — four files
//! deleted a release earlier. Anyone following those references went looking
//! for code that was not there. This test would have failed the moment they
//! were removed.
//!
//! What it deliberately does **not** try to check: whether a claim about
//! behaviour is still true. That needs a compiler, and the honest answer is to
//! verify against one when editing. Naming a file that does not exist is a
//! different, cheaper kind of wrong.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Extensions worth resolving. Deliberately narrow: these name real files in
/// this repository, whereas `.json` or `.md` in prose is as likely to be a
/// user's file as one of ours.
const EXTENSIONS: &[&str] = &[".rs", ".pest", ".gu", ".toml"];

/// Backticked names that are deliberately not files in this repository.
///
/// Two kinds. Most are files a *reader* would create, or that Gust generates
/// for them. The rest are files this repository **used** to contain and now
/// names on purpose, to say they are gone — a doc explaining that
/// `codegen_wasm.rs` was deleted has to be allowed to write `codegen_wasm.rs`.
///
/// Listing them explicitly beats loosening the extension rules, which would
/// quietly stop checking the names that matter.
const NOT_OURS: &[&str] = &[
    // Deleted in 1.0. Named only in prose recording their removal; if either
    // ever comes back, delete its line here so references are checked again.
    "codegen_wasm.rs",
    "codegen_nostd.rs",
    "wasm_codegen_coverage.rs",
    "nostd_codegen_coverage.rs",
    // A consumer's formatter config.
    "rustfmt.toml",
    // Generated output, named in examples of what Gust produces.
    "machine.g.rs",
    "machine.g.go",
    "upload.gu",
    "pipeline.gu",
    "slow.g.rs",
    "order.g.rs",
    "deploy.gu",
    "resmatch.g.go",
    // The tutorial's project, which the reader builds as they follow along.
    "upload.g.rs",
    "upload.rs",
    // Cookbook pages tell the reader to save these; they are not in the repo.
    "breaker.gu",
    "service_health.gu",
    "api_call.gu",
    "upload_retry.gu",
    "booking_saga.gu",
    "gate.g.ffi.rs",
    // Files in a consumer's project, not ours.
    "build.rs",
    "Cargo.toml",
    "gust.toml",
    "mutants.toml",
    "main.rs",
    "lib.rs",
    "go.mod",
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("gust-lang has a parent")
        .to_path_buf()
}

fn markdown_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            // `node_modules` and build output contain other projects' docs.
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if matches!(name, "node_modules" | "target" | "site" | ".git") {
                continue;
            }
            markdown_under(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("md") {
            out.push(path);
        }
    }
}

/// Pull backticked spans out of markdown and keep the ones that look like paths.
fn referenced_paths(content: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut in_fence = false;

    for line in content.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        // Fenced code is sample source, not a claim about this repository.
        if in_fence {
            continue;
        }

        for span in line.split('`').skip(1).step_by(2) {
            let token = span.trim().trim_end_matches(&[',', '.', ')', ':'][..]);
            if !EXTENSIONS.iter().any(|ext| token.ends_with(ext)) {
                continue;
            }
            // A span with whitespace is prose that happens to contain a name,
            // e.g. "the `wasm` and `nostd` backends".
            if token.contains(char::is_whitespace) {
                continue;
            }
            // A bare extension (`.gu`, `.g.rs`) names a *kind* of file, and a
            // template (`<stem>.g.rs`, `{name}.go`) names a naming scheme.
            // Neither is a reference to a particular file.
            if token.starts_with('.') || token.contains(['<', '>', '{', '}', '*']) {
                continue;
            }
            found.insert(token.to_string());
        }
    }
    found
}

/// Whether `token` names a file that exists, by path or by basename.
///
/// Docs refer to files both ways — `gust-lang/src/validator.rs` and plain
/// `validator.rs` — and both should resolve.
fn resolves(root: &Path, token: &str, all_files: &BTreeSet<String>) -> bool {
    if token.contains('/') {
        return root.join(token).exists();
    }
    all_files.contains(token)
}

fn collect_basenames(dir: &Path, out: &mut BTreeSet<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if path.is_dir() {
            if matches!(name, "node_modules" | "target" | ".git" | "site") {
                continue;
            }
            collect_basenames(&path, out);
        } else {
            out.insert(name.to_string());
        }
    }
}

#[test]
fn every_file_the_docs_name_exists() {
    let root = repo_root();

    let mut basenames = BTreeSet::new();
    collect_basenames(&root, &mut basenames);

    let mut pages = Vec::new();
    markdown_under(&root.join("docs").join("content"), &mut pages);
    markdown_under(&root.join("skills"), &mut pages);
    assert!(!pages.is_empty(), "no markdown found — check the paths");

    let skip: BTreeSet<&str> = NOT_OURS.iter().copied().collect();
    let mut dangling = Vec::new();
    let mut checked = 0usize;

    for page in &pages {
        let content = std::fs::read_to_string(page).expect("markdown should be readable");
        for token in referenced_paths(&content) {
            let basename = token.rsplit('/').next().unwrap_or(&token);
            if skip.contains(basename) {
                continue;
            }
            checked += 1;
            if !resolves(&root, &token, &basenames) {
                dangling.push(format!(
                    "{}: `{token}`",
                    page.strip_prefix(&root).unwrap_or(page).display()
                ));
            }
        }
    }

    dangling.sort();
    dangling.dedup();

    assert!(
        dangling.is_empty(),
        "\n{} documented file(s) do not exist:\n\n  {}\n\n\
         Either the reference is stale or the file moved. If the name is an \
         example rather than a reference into this repository, add it to \
         NOT_OURS with a reason.",
        dangling.len(),
        dangling.join("\n  ")
    );
    assert!(
        checked > 0,
        "no file references were found — check the parser"
    );
}
