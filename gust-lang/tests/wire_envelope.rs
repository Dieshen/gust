//! The persisted state envelope, asserted by running both backends.
//!
//! One `.gu` is compiled to Rust and to Go, each is built and executed, and the
//! JSON they print is compared — to each other, and to a literal written out
//! here. Both halves matter. Comparing the backends to each other alone would
//! pass if they drifted together; comparing each to a literal alone would let
//! them drift apart while both matched a stale expectation.
//!
//! Why this is worth the build cost. Before 1.0 the two backends did not agree,
//! and nothing said so. Rust emitted serde's externally-tagged default,
//! `{"Idle":{…}}`. Go emitted the `iota` ordinal in a sibling field,
//! `{"state":0,"idle_data":{…}}`. `gust schema` described the Rust shape, so a
//! Go document failed validation against the schema generated from its own
//! source. Every one of those compiled, vetted, and passed clippy: the
//! disagreement lived entirely in behaviour.
//!
//! The ordinal was the sharper edge. A state's identity was its declaration
//! order, so swapping two `state` lines — an edit that reads as pure
//! formatting, and which no diagnostic flagged — silently changed the meaning
//! of every stored document.
//!
//! Runs are skipped, loudly, when a toolchain is missing.

use gust_lang::{GoCodegen, RustCodegen, parse_program_with_errors};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Covers the three shapes that serialise differently: a multi-field state, a
/// state with no fields at all, and a single-field state.
const SOURCE: &str = r#"
machine Doc {
    state Draft(title: String, revision: i64)
    state Archived
    state Live(title: String)

    transition publish: Draft -> Live
    transition retire: Live -> Archived

    on publish(ctx) {
        goto Live(ctx.title);
    }

    on retire(ctx) {
        goto Archived();
    }
}
"#;

/// The canonical envelope, one document per line.
///
/// A fieldless state carries no `data` key — not `"data":null`, not `"data":{}`
/// — matching serde's treatment of a unit variant and Go's `omitempty`.
/// The last line is the re-serialisation of a document carrying an unknown
/// envelope key. Both backends must ignore the key rather than reject the
/// document — that tolerance is what lets 1.x add a schema-version key which
/// 1.0 binaries can still read, and it is a promise rather than an inherited
/// default of serde and `encoding/json`.
const EXPECTED: &[&str] = &[
    r#"{"state":"Draft","data":{"title":"a","revision":2}}"#,
    r#"{"state":"Archived"}"#,
    r#"{"state":"Live","data":{"title":"b"}}"#,
    r#"{"state":"Archived"}"#,
];

// `r##` because the driver itself contains an `r#"…"#` literal.
const RUST_DRIVER: &str = r##"
mod machine;
use machine::*;

fn main() {
    let cases = vec![
        DocState::Draft { title: "a".to_string(), revision: 2 },
        DocState::Archived,
        DocState::Live { title: "b".to_string() },
    ];
    for state in cases {
        let doc = Doc { state };
        let json = serde_json::to_string(&doc).expect("serialize");
        let back: Doc = serde_json::from_str(&json).expect("deserialize");
        let again = serde_json::to_string(&back).expect("re-serialize");
        assert_eq!(json, again, "round-trip changed the document");
        println!("{json}");
    }
    // A document naming a state this machine does not have must be rejected,
    // not silently coerced into whichever state happens to be first.
    assert!(
        serde_json::from_str::<Doc>(r#"{"state":"Nope"}"#).is_err(),
        "unknown state was accepted"
    );

    // An *unknown envelope key* must be ignored. This is what lets 1.x add a
    // schema-version key that 1.0 binaries can still read.
    let forward: Doc = serde_json::from_str(r#"{"state":"Archived","v":2}"#)
        .expect("an unknown envelope key must be tolerated");
    println!("{}", serde_json::to_string(&forward).expect("re-serialize"));
}
"##;

const GO_DRIVER: &str = r#"
package main

import (
	"encoding/json"
	"fmt"
	"os"
)

func main() {
	cases := []Doc{
		{State: DocStateDraft, DraftData: &DocDraftData{Title: "a", Revision: 2}},
		{State: DocStateArchived},
		{State: DocStateLive, LiveData: &DocLiveData{Title: "b"}},
	}
	for _, doc := range cases {
		encoded, err := json.Marshal(doc)
		if err != nil {
			fmt.Fprintln(os.Stderr, "marshal:", err)
			os.Exit(1)
		}
		var back Doc
		if err := json.Unmarshal(encoded, &back); err != nil {
			fmt.Fprintln(os.Stderr, "unmarshal:", err)
			os.Exit(1)
		}
		again, err := json.Marshal(back)
		if err != nil {
			fmt.Fprintln(os.Stderr, "re-marshal:", err)
			os.Exit(1)
		}
		if string(encoded) != string(again) {
			fmt.Fprintln(os.Stderr, "round-trip changed the document")
			os.Exit(1)
		}
		fmt.Println(string(encoded))
	}
	var unknown Doc
	if err := json.Unmarshal([]byte(`{"state":"Nope"}`), &unknown); err == nil {
		fmt.Fprintln(os.Stderr, "unknown state was accepted")
		os.Exit(1)
	}

	// An *unknown envelope key* must be ignored, so that 1.x can add a
	// schema-version key 1.0 binaries can still read.
	var forward Doc
	if err := json.Unmarshal([]byte(`{"state":"Archived","v":2}`), &forward); err != nil {
		fmt.Fprintln(os.Stderr, "unknown envelope key was rejected:", err)
		os.Exit(1)
	}
	encoded, err := json.Marshal(forward)
	if err != nil {
		fmt.Fprintln(os.Stderr, "re-marshal:", err)
		os.Exit(1)
	}
	fmt.Println(string(encoded))
}
"#;

#[test]
fn both_backends_emit_the_same_state_envelope() {
    let program = parse_program_with_errors(SOURCE, "envelope.gu").expect("fixture should parse");

    let rust = run_rust(&RustCodegen::new().generate(&program));
    let go = run_go(&GoCodegen::new().generate(&program, "main"));

    match (rust, go) {
        (Some(rust), Some(go)) => {
            assert_eq!(
                rust, EXPECTED,
                "the Rust backend no longer emits the canonical envelope"
            );
            assert_eq!(
                go, EXPECTED,
                "the Go backend no longer emits the canonical envelope"
            );
            // Redundant given the two above, and kept because it is the
            // property the change exists to guarantee: a Go API and a Rust
            // worker can read each other's persisted machines.
            assert_eq!(rust, go, "the backends disagree about the persisted form");
        }
        (rust, go) => {
            eprintln!(
                "skipped: envelope comparison needs both toolchains \
                 (rust ran: {}, go ran: {})",
                rust.is_some(),
                go.is_some()
            );
        }
    }
}

/// Build and run the generated Rust, returning its stdout lines.
///
/// `None` means the toolchain was unavailable, not that the test passed.
fn run_rust(generated: &str) -> Option<Vec<String>> {
    let dir = tempfile::tempdir().expect("create tempdir");
    let src = dir.path().join("src");
    std::fs::create_dir(&src).expect("create src");
    std::fs::write(src.join("machine.rs"), generated).expect("write machine.rs");
    std::fs::write(src.join("main.rs"), RUST_DRIVER).expect("write main.rs");

    let runtime = toml_path(&workspace_root().join("gust-runtime"));
    std::fs::write(
        dir.path().join("Cargo.toml"),
        format!(
            "[package]\nname = \"gust-envelope\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
             [dependencies]\n\
             gust-runtime = {{ path = \"{runtime}\" }}\n\
             serde = {{ version = \"1.0\", features = [\"derive\"] }}\n\
             serde_json = \"1.0\"\n\
             thiserror = \"2.0\"\n\n[workspace]\n"
        ),
    )
    .expect("write Cargo.toml");

    let output = Command::new(env!("CARGO"))
        .args(["run", "--quiet"])
        .current_dir(dir.path())
        .env(
            "CARGO_TARGET_DIR",
            workspace_root().join("target/wire-envelope"),
        )
        .output()
        .ok()?;

    assert!(
        output.status.success(),
        "generated Rust failed to build or run:\n{}\n--- generated ---\n{generated}",
        String::from_utf8_lossy(&output.stderr)
    );
    Some(lines(&output.stdout))
}

/// Build and run the generated Go, returning its stdout lines.
fn run_go(generated: &str) -> Option<Vec<String>> {
    let dir = tempfile::tempdir().expect("create tempdir");
    std::fs::write(dir.path().join("machine.go"), generated).expect("write machine.go");
    std::fs::write(dir.path().join("main.go"), GO_DRIVER).expect("write main.go");
    std::fs::write(dir.path().join("go.mod"), "module envelope\n\ngo 1.21\n")
        .expect("write go.mod");

    let output = Command::new("go")
        .args(["run", "."])
        .current_dir(dir.path())
        .output()
        .ok()?;

    assert!(
        output.status.success(),
        "generated Go failed to build or run:\n{}\n--- generated ---\n{generated}",
        String::from_utf8_lossy(&output.stderr)
    );
    Some(lines(&output.stdout))
}

fn lines(raw: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(raw)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("gust-lang has a parent")
        .to_path_buf()
}

fn toml_path(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}
