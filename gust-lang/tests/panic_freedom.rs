//! The compiler must reject bad input, never panic on it.
//!
//! A panic is a bad failure mode for every consumer Gust has. `gust check`
//! prints a backtrace instead of a diagnostic; `gust-lsp` takes down the
//! editor's language server mid-keystroke, which is *exactly* when it sees
//! half-written source; `gust-mcp` returns a broken JSON-RPC response; and
//! `gust-build` fails a `cargo build` with something the author cannot act on.
//! The LSP case is the sharpest, because incomplete source is its normal input
//! rather than an edge case.
//!
//! `parser_property_tests.rs` covers generated integer literals. This is the
//! complementary direction: take real sources and damage them, which reaches
//! shapes a generator will not stumble into — a `machine` whose body stops
//! mid-handler, a truncated string literal, a lone `<` where generics start.
//!
//! Every stage is exercised, not just the parser. A panic in the validator or a
//! backend is just as fatal, and those run on input the parser has *accepted*,
//! which is the harder thing to get right.

use gust_lang::{
    GoCodegen, RustCodegen, SchemaCodegen, format_program, parse_program_with_errors,
    validate_program,
};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

fn corpus_sources() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("compat");
    let mut out = Vec::new();
    collect(&root, &mut out);
    out.sort();
    out
}

fn collect(dir: &Path, out: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path: PathBuf = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("gu") {
            if let Ok(text) = std::fs::read_to_string(&path) {
                out.push((path.display().to_string(), text));
            }
        }
    }
}

/// Push one input through every stage, returning a description if any panicked.
///
/// Stages after the parser only run when parsing succeeded, which is the point:
/// they are reached with *accepted-but-strange* programs, not with garbage.
fn stages_survive(label: &str, source: &str) -> Option<String> {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let Ok(program) = parse_program_with_errors(source, "fuzz.gu") else {
            return; // a rejected input is a correct outcome
        };
        // Diagnostics must be renderable too — `render` indexes into the source
        // by span, and a span pointing past a truncated buffer would panic
        // there rather than in the check that produced it.
        let report = validate_program(&program, "fuzz.gu", source);
        for error in &report.errors {
            let _ = error.render(source);
        }
        for warning in &report.warnings {
            let _ = warning.render(source);
        }
        let _ = RustCodegen::new().generate(&program);
        let _ = GoCodegen::new().generate(&program, "fuzz");
        let _ = SchemaCodegen::generate(&program);
        let _ = format_program(&program);
    }));

    outcome.err().map(|payload| {
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "<non-string panic payload>".to_string());
        format!("{label}: panicked — {message}\n--- input ---\n{source}")
    })
}

/// Truncating a real source at every stage boundary must never panic.
///
/// This is the language server's normal input: a file that is correct up to
/// the cursor and unfinished after it.
#[test]
fn truncated_sources_do_not_panic() {
    let sources = corpus_sources();
    assert!(!sources.is_empty(), "no corpus sources found");

    let mut failures = Vec::new();
    let mut checked = 0usize;

    for (path, text) in &sources {
        // Every 7 bytes: dense enough to land mid-token, mid-string, and
        // mid-block without running the whole suite for minutes. Stepping by a
        // prime avoids syncing up with any repeating indentation width.
        let mut offset = 0usize;
        while offset < text.len() {
            if text.is_char_boundary(offset) {
                checked += 1;
                if let Some(failure) =
                    stages_survive(&format!("{path} truncated at {offset}"), &text[..offset])
                {
                    failures.push(failure);
                }
            }
            offset += 7;
        }
    }

    assert!(
        failures.is_empty(),
        "\n{} truncated input(s) panicked out of {checked}:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    assert!(checked > 0, "no truncations were checked");
}

/// Deleting a single byte from a real source must never panic.
///
/// Truncation only ever produces a valid *prefix*. Deleting a byte from the
/// middle produces something structurally broken but still complete-looking —
/// an unbalanced brace, a `-` where `->` was — which reaches different paths.
#[test]
fn single_byte_deletions_do_not_panic() {
    let mut failures = Vec::new();
    let mut checked = 0usize;

    for (path, text) in &corpus_sources() {
        let mut offset = 0usize;
        while offset < text.len() {
            if text.is_char_boundary(offset) {
                let mut damaged = String::with_capacity(text.len());
                damaged.push_str(&text[..offset]);
                let rest = &text[offset..];
                let skip = rest.chars().next().map(char::len_utf8).unwrap_or(0);
                damaged.push_str(&rest[skip..]);

                checked += 1;
                if let Some(failure) =
                    stages_survive(&format!("{path} byte {offset} deleted"), &damaged)
                {
                    failures.push(failure);
                }
            }
            offset += 13;
        }
    }

    assert!(
        failures.is_empty(),
        "\n{} damaged input(s) panicked out of {checked}:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    assert!(checked > 0, "no deletions were checked");
}

/// Hand-picked shapes that a truncation or deletion will not produce.
#[test]
fn hostile_inputs_do_not_panic() {
    let cases: Vec<(&str, String)> = vec![
        ("empty", String::new()),
        ("only whitespace", "   \n\t\n  ".to_string()),
        ("lone brace", "}".to_string()),
        ("unterminated string", r#"machine M { state A("#.to_string()),
        (
            "unterminated comment",
            "machine M { /* state A".to_string(),
        ),
        ("bare bom", "\u{feff}machine M {}".to_string()),
        (
            "nul and control bytes",
            "machine \u{0}M\u{1} {}".to_string(),
        ),
        (
            "non-ascii identifiers",
            "machine Ünïcødé { state Ätat }".to_string(),
        ),
        (
            "emoji in a string literal",
            r#"machine M { state A(s: String) transition t: A -> A on t(ctx) { goto A("🦀🦊"); } }"#
                .to_string(),
        ),
        (
            "integer past i64",
            "machine M { state A transition t: A -> A on t() { let n = 99999999999999999999999; goto A(); } }"
                .to_string(),
        ),
        (
            "very deep nesting",
            format!(
                "machine M {{ state A transition t: A -> A on t() {{ {} goto A(); {} }} }}",
                "if true { ".repeat(200),
                "}".repeat(200)
            ),
        ),
        (
            "very long identifier",
            format!("machine {} {{ state A }}", "M".repeat(100_000)),
        ),
        (
            "many states",
            format!(
                "machine M {{ {} }}",
                (0..2000)
                    .map(|i| format!("state S{i} "))
                    .collect::<String>()
            ),
        ),
    ];

    let mut failures = Vec::new();
    for (label, source) in &cases {
        if let Some(failure) = stages_survive(label, source) {
            failures.push(failure);
        }
    }

    assert!(
        failures.is_empty(),
        "\n{} hostile input(s) panicked:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
