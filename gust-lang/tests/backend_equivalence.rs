//! One `.gu`, two backends, driven through the same transitions — do the
//! machines *behave* the same?
//!
//! This is the claim Gust actually makes, and until now nothing tested it.
//! `codegen_backends.rs` proves both backends emit code their toolchain
//! accepts. `wire_envelope.rs` proves they agree on the persisted form of a
//! state someone constructed by hand. Neither runs a machine.
//!
//! The gap is not hypothetical. #121 and #136 were exactly this: `goto` did not
//! end the handler in Go, so a multi-target transition silently collapsed to
//! its **last** declared target regardless of the condition. The source
//! validated, both backends compiled, `go vet` was quiet, and the two machines
//! disagreed about what state they were in. Only running them shows that.
//!
//! # How it works
//!
//! Each backend gets a driver implementing the same effects and firing the same
//! transitions, printing the machine's JSON envelope after every step. Because
//! the envelope is byte-identical across backends by construction, the two
//! transcripts can be compared directly — the 1.0 envelope work is what makes
//! this test cheap.
//!
//! Both transcripts are also compared to a literal. Comparing them only to each
//! other would pass if both backends drifted the same way.

use gust_lang::{GoCodegen, RustCodegen, parse_program_with_errors};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Exercises the shapes that have actually diverged between backends.
///
/// `triage` is the important one, and its awkwardness is deliberate: an early
/// `goto` inside a **bare `if`**, with another `goto` after it. That is the
/// exact shape #136 broke, and it is not interchangeable with a `match` whose
/// arms each end in `goto` — those sit in tail position, where the defect never
/// showed. An earlier version of this fixture used only the `match` form and
/// passed with the bug reintroduced.
///
/// `finish` keeps the `Result` match, which is its own historical divergence.
const SOURCE: &str = r#"
machine Job {
    state Queued(id: String, attempts: i64)
    state Running(id: String, attempts: i64)
    state Done(result: String)
    state Failed(id: String, reason: String)

    transition start: Queued -> Running
    transition triage: Running -> Done | Failed
    transition finish: Running -> Done | Failed

    effect run(id: String) -> Result<String, String>

    on start(ctx) {
        goto Running(ctx.id, ctx.attempts + 1);
    }

    on triage(ctx) {
        if ctx.attempts > 3 {
            goto Failed(ctx.id, "too many attempts");
        }
        goto Done(ctx.id);
    }

    on finish(ctx) {
        let outcome = perform run(ctx.id);
        match outcome {
            Ok(result) => {
                goto Done(result);
            }
            Err(reason) => {
                goto Failed(ctx.id, reason);
            }
        }
    }
}
"#;

/// The transcript both backends must produce.
///
/// The two `finish` runs differ only in what the effect returns, so the pair
/// pins the branch: a backend that ignores the condition and always lands on
/// the last declared target produces `Failed` twice, which is precisely the
/// #136 failure and is visible here as a diff rather than as silence.
const EXPECTED: &[&str] = &[
    // triage: attempts stays low, so the bare `if` must NOT be taken.
    r#"{"state":"Queued","data":{"id":"t1","attempts":0}}"#,
    r#"{"state":"Running","data":{"id":"t1","attempts":1}}"#,
    r#"{"state":"Done","data":{"result":"t1"}}"#,
    // triage: attempts is high, so the early `goto` inside the bare `if` runs
    // and must end the handler. A backend that falls through lands on `Done`.
    r#"{"state":"Queued","data":{"id":"t2","attempts":9}}"#,
    r#"{"state":"Running","data":{"id":"t2","attempts":10}}"#,
    r#"{"state":"Failed","data":{"id":"t2","reason":"too many attempts"}}"#,
    // finish: the Result match, both arms.
    r#"{"state":"Queued","data":{"id":"j1","attempts":0}}"#,
    r#"{"state":"Running","data":{"id":"j1","attempts":1}}"#,
    r#"{"state":"Done","data":{"result":"ok:j1"}}"#,
    r#"{"state":"Queued","data":{"id":"j2","attempts":7}}"#,
    r#"{"state":"Running","data":{"id":"j2","attempts":8}}"#,
    r#"{"state":"Failed","data":{"id":"j2","reason":"boom:j2"}}"#,
];

const RUST_DRIVER: &str = r#"
mod machine;
use machine::*;

struct Succeeds;
impl JobEffects for Succeeds {
    fn run(&self, id: &str) -> Result<String, String> {
        Ok(format!("ok:{id}"))
    }
}

struct Fails;
impl JobEffects for Fails {
    fn run(&self, id: &str) -> Result<String, String> {
        Err(format!("boom:{id}"))
    }
}

fn show(job: &Job) {
    println!("{}", serde_json::to_string(job).expect("serialize"));
}

fn main() {
    let mut low = Job::new("t1".to_string(), 0);
    show(&low);
    low.start().expect("start");
    show(&low);
    low.triage().expect("triage");
    show(&low);

    let mut high = Job::new("t2".to_string(), 9);
    show(&high);
    high.start().expect("start");
    show(&high);
    high.triage().expect("triage");
    show(&high);

    let mut ok = Job::new("j1".to_string(), 0);
    show(&ok);
    ok.start().expect("start");
    show(&ok);
    ok.finish(&Succeeds).expect("finish");
    show(&ok);

    let mut bad = Job::new("j2".to_string(), 7);
    show(&bad);
    bad.start().expect("start");
    show(&bad);
    bad.finish(&Fails).expect("finish");
    show(&bad);

    // An out-of-order transition must be refused, not silently applied.
    let mut stuck = Job::new("j3".to_string(), 0);
    assert!(stuck.finish(&Succeeds).is_err(), "finish from Queued should fail");
}
"#;

const GO_DRIVER: &str = r#"
package main

import (
	"encoding/json"
	"fmt"
	"os"
)

type succeeds struct{}

func (succeeds) Run(id string) (string, error) { return "ok:" + id, nil }

type fails struct{}

func (fails) Run(id string) (string, error) { return "", fmt.Errorf("boom:%s", id) }

func show(job *Job) {
	encoded, err := json.Marshal(job)
	if err != nil {
		fmt.Fprintln(os.Stderr, "marshal:", err)
		os.Exit(1)
	}
	fmt.Println(string(encoded))
}

func must(err error) {
	if err != nil {
		fmt.Fprintln(os.Stderr, "transition:", err)
		os.Exit(1)
	}
}

func main() {
	low := NewJob("t1", 0)
	show(low)
	must(low.Start())
	show(low)
	must(low.Triage())
	show(low)

	high := NewJob("t2", 9)
	show(high)
	must(high.Start())
	show(high)
	must(high.Triage())
	show(high)

	ok := NewJob("j1", 0)
	show(ok)
	must(ok.Start())
	show(ok)
	must(ok.Finish(succeeds{}))
	show(ok)

	bad := NewJob("j2", 7)
	show(bad)
	must(bad.Start())
	show(bad)
	must(bad.Finish(fails{}))
	show(bad)

	// An out-of-order transition must be refused, not silently applied.
	stuck := NewJob("j3", 0)
	if err := stuck.Finish(succeeds{}); err == nil {
		fmt.Fprintln(os.Stderr, "finish from Queued should fail")
		os.Exit(1)
	}
}
"#;

#[test]
fn both_backends_run_the_same_machine_the_same_way() {
    let program =
        parse_program_with_errors(SOURCE, "equivalence.gu").expect("fixture should parse");

    let rust = run_rust(&RustCodegen::new().generate(&program));
    let go = run_go(&GoCodegen::new().generate(&program, "main"));

    match (rust, go) {
        (Some(rust), Some(go)) => {
            assert_eq!(rust, EXPECTED, "the Rust machine took a different path");
            assert_eq!(go, EXPECTED, "the Go machine took a different path");
            assert_eq!(rust, go, "the two backends disagree about behaviour");
        }
        (rust, go) => {
            eprintln!(
                "skipped: equivalence needs both toolchains (rust ran: {}, go ran: {})",
                rust.is_some(),
                go.is_some()
            );
        }
    }
}

fn run_rust(generated: &str) -> Option<Vec<String>> {
    let dir = tempfile::tempdir().expect("create tempdir");
    let src = dir.path().join("src");
    std::fs::create_dir(&src).expect("create src");
    std::fs::write(src.join("machine.rs"), generated).expect("write machine.rs");
    std::fs::write(src.join("main.rs"), RUST_DRIVER).expect("write main.rs");

    let runtime = workspace_root()
        .join("gust-runtime")
        .display()
        .to_string()
        .replace('\\', "/");
    std::fs::write(
        dir.path().join("Cargo.toml"),
        format!(
            "[package]\nname = \"gust-equivalence\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
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

fn run_go(generated: &str) -> Option<Vec<String>> {
    let dir = tempfile::tempdir().expect("create tempdir");
    std::fs::write(dir.path().join("machine.go"), generated).expect("write machine.go");
    std::fs::write(dir.path().join("main.go"), GO_DRIVER).expect("write main.go");
    std::fs::write(dir.path().join("go.mod"), "module equivalence\n\ngo 1.21\n")
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
