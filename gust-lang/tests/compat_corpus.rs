//! The compatibility corpus and the golden output snapshots.
//!
//! Two properties, over one frozen set of sources.
//!
//! **Compatibility.** Every `.gu` under `tests/compat/<release>/` must keep
//! parsing and validating. The directory name is the release that introduced
//! those sources, and the files are **never edited afterwards** — that is what
//! makes the promise meaningful. Rewriting a corpus file to satisfy a new
//! validator rule silently converts "1.0 source still compiles" into "source we
//! were willing to change still compiles", which is not a promise at all.
//!
//! **Goldens.** Generated Rust and Go for each source is recorded next to it. A
//! diff is not a failure to be silenced; it is a decision that needs a CHANGELOG
//! entry. Codegen is deterministic, so this is cheap.
//!
//! Both 0.4.0 breaking changes would have been caught here. `goto` beginning to
//! end the handler (#121) changes every generated transition body, and the
//! supervision contract (#120) adds a trait and a strategy table.
//!
//! # Adding to the corpus
//!
//! Drop a `.gu` into the newest release directory and run with
//! `UPDATE_GOLDENS=1` to record its output. Real projects are welcome and
//! wanted: the corpus is only as good as the shapes in it, and the fixtures in
//! `codegen_backends.rs` are deliberately small.
//!
//! # Updating goldens
//!
//! ```text
//! UPDATE_GOLDENS=1 cargo test -p gust-lang --test compat_corpus
//! ```
//!
//! Then read the diff. If it was not intended, that is the bug. If it was,
//! the CHANGELOG entry is part of the change.

use gust_lang::{GoCodegen, RustCodegen, parse_program_with_errors, validate_program};
use std::path::{Path, PathBuf};

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("compat")
}

fn updating() -> bool {
    std::env::var_os("UPDATE_GOLDENS").is_some()
}

struct Case {
    /// e.g. `v1.0/retry.gu`, used in failure messages.
    label: String,
    source_path: PathBuf,
    rust_golden: PathBuf,
    go_golden: PathBuf,
    package: String,
}

fn cases() -> Vec<Case> {
    let root = corpus_root();
    let mut out = Vec::new();

    let mut release_dirs: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("corpus root {} unreadable: {e}", root.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    release_dirs.sort();

    for dir in release_dirs {
        let release = dir
            .file_name()
            .and_then(|s| s.to_str())
            .expect("release dir name")
            .to_string();

        let mut sources: Vec<PathBuf> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("corpus dir {} unreadable: {e}", dir.display()))
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("gu"))
            .collect();
        sources.sort();

        for source_path in sources {
            let stem = source_path
                .file_stem()
                .and_then(|s| s.to_str())
                .expect("source stem")
                .to_string();
            out.push(Case {
                label: format!("{release}/{stem}.gu"),
                rust_golden: dir.join(format!("{stem}.g.rs")),
                go_golden: dir.join(format!("{stem}.g.go")),
                // Go package names cannot contain `-`.
                package: stem.replace('-', "_"),
                source_path,
            });
        }
    }

    out
}

/// Compare against the recorded golden, or rewrite it under `UPDATE_GOLDENS`.
fn check_golden(path: &Path, generated: &str, label: &str, failures: &mut Vec<String>) {
    if updating() {
        std::fs::write(path, generated)
            .unwrap_or_else(|e| panic!("cannot write golden {}: {e}", path.display()));
        return;
    }

    let recorded = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(_) => {
            failures.push(format!(
                "{label}: no golden at {}\n  \
                 run with UPDATE_GOLDENS=1 to record it",
                path.display()
            ));
            return;
        }
    };

    // Normalise line endings: goldens are checked out with whatever the
    // platform's git config produces, and codegen always emits `\n`.
    if recorded.replace("\r\n", "\n") == generated.replace("\r\n", "\n") {
        return;
    }

    let (line, rec_line, gen_line) = first_difference(&recorded, generated);
    failures.push(format!(
        "{label}: generated output differs from {}\n  \
         first difference at line {line}:\n    \
         recorded:  {rec_line}\n    \
         generated: {gen_line}\n  \
         If this change was intended it needs a CHANGELOG entry; then re-record \
         with UPDATE_GOLDENS=1.",
        path.display()
    ));
}

/// Line number and text of the first differing line, for a readable failure.
///
/// A full diff would be nicer, but the first divergence is almost always enough
/// to recognise *which* change caused it, and this avoids a dependency.
fn first_difference(a: &str, b: &str) -> (usize, String, String) {
    let a = a.replace("\r\n", "\n");
    let b = b.replace("\r\n", "\n");

    let mut a_lines = a.lines();
    let mut b_lines = b.lines();
    let mut line = 0usize;

    loop {
        line += 1;
        match (a_lines.next(), b_lines.next()) {
            (None, None) => return (0, String::new(), String::new()),
            (x, y) => {
                let x = x.unwrap_or("<end of file>");
                let y = y.unwrap_or("<end of file>");
                if x != y {
                    return (line, x.to_string(), y.to_string());
                }
            }
        }
    }
}

/// Every corpus source still parses and validates.
///
/// A source that stops validating is a compatibility break, whether or not the
/// new diagnostic is a good idea. Fix the compiler, or make the break
/// deliberately and record it — do not edit the corpus file.
#[test]
fn every_corpus_source_still_compiles() {
    let cases = cases();
    assert!(
        !cases.is_empty(),
        "the corpus is empty — check tests/compat/ layout"
    );

    let mut failures = Vec::new();

    for case in &cases {
        let source = std::fs::read_to_string(&case.source_path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", case.source_path.display()));

        let program = match parse_program_with_errors(&source, &case.label) {
            Ok(program) => program,
            Err(err) => {
                failures.push(format!(
                    "{}: no longer parses\n{}",
                    case.label,
                    err.render(&source)
                ));
                continue;
            }
        };

        let report = validate_program(&program, &case.label, &source);
        if !report.errors.is_empty() {
            let rendered: Vec<String> = report.errors.iter().map(|e| e.render(&source)).collect();
            failures.push(format!(
                "{}: no longer validates\n{}",
                case.label,
                rendered.join("\n")
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "\n{} corpus source(s) broke compatibility:\n\n{}\n\n\
         These files are frozen. Editing one to satisfy a new rule turns \
         \"1.0 source still compiles\" into \"source we were willing to change \
         still compiles\".",
        failures.len(),
        failures.join("\n\n")
    );
}

/// Generated output for every corpus source matches its recorded golden.
#[test]
fn generated_output_matches_the_goldens() {
    let cases = cases();
    let mut failures = Vec::new();

    for case in &cases {
        let source = std::fs::read_to_string(&case.source_path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", case.source_path.display()));

        let Ok(program) = parse_program_with_errors(&source, &case.label) else {
            // Reported by the compatibility test; nothing to compare here.
            continue;
        };

        let rust = RustCodegen::new().generate(&program);
        check_golden(
            &case.rust_golden,
            &rust,
            &format!("{} [rust]", case.label),
            &mut failures,
        );

        let go = GoCodegen::new().generate(&program, &case.package);
        check_golden(
            &case.go_golden,
            &go,
            &format!("{} [go]", case.label),
            &mut failures,
        );
    }

    assert!(
        failures.is_empty(),
        "\n{} golden mismatch(es):\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
