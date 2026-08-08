//! The VS Code extension's snippets must be valid 1.0 Gust.
//!
//! They were not. Four of them taught `on transition(ctx: Context)`, the
//! pre-1.0 spelling that 1.0 rejects, so anyone tab-completing a handler got
//! source the compiler refuses. The snippets live in this repository and were
//! still missed, because nothing read them — `docs_snippets.rs` walks
//! `docs/content`, and a `.json` file of editor snippets is not markdown.
//!
//! Parsing alone would not have caught it either. `on go(ctx: Context)` parses
//! cleanly; `param = { ident ~ (":" ~ type_expr)? }` accepts an annotation. The
//! rejection comes from the validator, so this test **validates**.
//!
//! # Placeholders
//!
//! Snippet bodies are LSP snippet syntax: `${1:name}`, `${2:field}`, `$0`.
//! Stripping each placeholder to its default text yields ordinary Gust, which
//! is why the defaults in `gust.json` are real type names — `${3:String}`, not
//! `${3:Type}`. That is worth keeping: a default that does not compile is a
//! worse suggestion for the person tab-completing, as well as untestable.

use gust_lang::{parse_program_with_errors, validate_program};
use std::path::{Path, PathBuf};

/// How to make a snippet into a standalone program.
///
/// Most snippets are fragments — a `state` line is not a program. Wrapping is
/// explicit per snippet rather than guessed, so a snippet that changes shape
/// fails here instead of silently falling into a wrapper that still parses.
enum Wrap {
    /// Already a complete program.
    None,
    /// Goes inside a machine that declares what the fragments refer to.
    MachineBody,
    /// Not checkable standalone; the reason is printed, not swallowed.
    Skip(&'static str),
}

/// Declarations the machine-body fragments refer to but do not declare.
///
/// The `Transition` snippet names `From` and `To`; the handler snippets name a
/// transition called `transition`. Supplying exactly those, and nothing more,
/// keeps the test honest: a snippet that starts referring to something else
/// fails here rather than being absorbed by a permissive scaffold.
const MACHINE_SCAFFOLD: &str = "    state From\n    \
                                state To\n    \
                                transition transition: From -> To";

fn plan(name: &str) -> Wrap {
    match name {
        "Machine" | "Enum" | "Type" | "Corsac Node" => Wrap::None,
        "State" | "Transition" | "Effect" | "Async Effect" => Wrap::MachineBody,
        "Handler" | "Async Handler" => Wrap::MachineBody,
        // A bare `match` arm needs a matchable scrutinee, which means an effect
        // returning `Result` and patterns that agree with it. Wrapping it would
        // test the wrapper.
        "Match" => Wrap::Skip("a match arm needs a scrutinee to be meaningful"),
        other => panic!(
            "snippet {other:?} has no entry in `plan` — add one rather than \
             letting a new snippet go unchecked"
        ),
    }
}

/// Replace `${1:default}` with `default`, and drop `${1}` / `$0` entirely.
fn strip_placeholders(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;

    while let Some(start) = rest.find('$') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];

        if let Some(inner_end) = after.strip_prefix('{').and_then(|b| b.find('}')) {
            let inner = &after[1..inner_end + 1];
            // `1:default` keeps `default`; a bare `1` contributes nothing.
            if let Some((_, default)) = inner.split_once(':') {
                out.push_str(default);
            }
            rest = &after[inner_end + 2..];
        } else {
            // `$0` and friends.
            let digits = after.len() - after.trim_start_matches(|c: char| c.is_ascii_digit()).len();
            rest = &after[digits..];
        }
    }
    out.push_str(rest);
    out
}

fn snippets_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("editors")
        .join("vscode")
        .join("snippets")
        .join("gust.json")
}

#[test]
fn every_vscode_snippet_is_valid_gust() {
    let path = snippets_path();
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let parsed: serde_json::Value =
        serde_json::from_str(&raw).expect("the snippets file should be valid JSON");
    let table = parsed
        .as_object()
        .expect("snippets file should be an object");

    assert!(!table.is_empty(), "no snippets found — check the path");

    let mut checked = 0usize;
    let mut skipped = Vec::new();
    let mut failures = Vec::new();

    for (name, snippet) in table {
        let body = snippet["body"]
            .as_array()
            .unwrap_or_else(|| panic!("snippet {name:?} has no body array"))
            .iter()
            .map(|line| line.as_str().expect("snippet body line should be a string"))
            .collect::<Vec<_>>()
            .join("\n");

        let source = match plan(name) {
            Wrap::None => strip_placeholders(&body),
            Wrap::MachineBody => format!(
                "machine SnippetHost {{\n{MACHINE_SCAFFOLD}\n{}\n}}",
                strip_placeholders(&body)
            ),
            Wrap::Skip(why) => {
                skipped.push(format!("{name} — {why}"));
                continue;
            }
        };

        checked += 1;

        let program = match parse_program_with_errors(&source, name) {
            Ok(program) => program,
            Err(err) => {
                failures.push(format!(
                    "{name}: does not parse\n{}\n--- source ---\n{source}",
                    err.render(&source)
                ));
                continue;
            }
        };

        let report = validate_program(&program, name, &source);
        if !report.errors.is_empty() {
            let rendered: Vec<String> = report.errors.iter().map(|e| e.render(&source)).collect();
            failures.push(format!(
                "{name}: does not validate\n{}\n--- source ---\n{source}",
                rendered.join("\n")
            ));
        }
    }

    // Printed rather than silent: a skipped snippet is not a checked one.
    for entry in &skipped {
        eprintln!("skipped: {entry}");
    }

    assert!(
        failures.is_empty(),
        "\n{} VS Code snippet(s) are not valid 1.0 Gust — \
         tab-completing one would produce source the compiler rejects:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    assert!(checked > 0, "no snippets were checked");
}
