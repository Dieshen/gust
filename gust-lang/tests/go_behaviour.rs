//! Runs generated Go and asserts what it *does*, not that it compiles.
//!
//! `codegen_backends.rs` compiles every fixture's output with its real
//! toolchain. That is a strong check, and it is not this one: compiling proves
//! output is well-formed, never that it implements the source machine. Two
//! backends cleared that bar for their entire existence while discarding
//! handler bodies wholesale, and were removed in 1.0 rather than frozen into
//! the stability promise.
//!
//! #136 is the same class of defect surviving in a backend that is *not*
//! removable: the Go emitter lowered `goto` to a bare state assignment and kept
//! going, so an early `goto` inside a bare `if` fell through into every later
//! branch. `gust check` passed, the output compiled, `go vet` was quiet, and
//! the machine silently landed in the wrong state.
//!
//! Each case here writes the generated package plus a Go driver and runs
//! `go test`. Slower than string assertions, and the only thing that would have
//! caught this.

use gust_lang::{GoCodegen, parse_program_with_errors};
use std::path::Path;
use std::process::Command;

/// Generate Go for `source`, pair it with `driver`, and run `go test`.
fn run_go_behaviour(source: &str, driver: &str) -> Result<(), String> {
    let program =
        parse_program_with_errors(source, "behaviour.gu").expect("fixture source should parse");
    let generated = GoCodegen::new().generate(&program, "behaviour");

    let dir = tempfile::tempdir().expect("create tempdir");
    write(dir.path(), "machine.go", &generated);
    write(dir.path(), "machine_test.go", driver);
    write(dir.path(), "go.mod", "module behaviour\n\ngo 1.21\n");

    let output = Command::new("go")
        .args(["test", "./..."])
        .current_dir(dir.path())
        .output()
        .expect("go test should run");

    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "--- generated ---\n{generated}\n--- go test ---\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    ))
}

fn write(dir: &Path, name: &str, contents: &str) {
    std::fs::write(dir.join(name), contents).unwrap_or_else(|e| panic!("write {name}: {e}"));
}

/// A multi-target transition whose first `goto` sits in a bare `if`.
///
/// The condition decides the destination, so a fall-through is observable as
/// the machine landing in the *last* declared target no matter what the effect
/// returned.
const BRANCHING: &str = r#"
type Piece { serial: String }
type Verdict { accept: bool, reason: String }

machine Lifecycle {
    state AtCore(piece: Piece)
    state AtWax(piece: Piece)
    state Scrapped(reason: String)

    transition core_to_wax: AtCore -> AtWax | Scrapped

    effect evaluate(serial: String) -> Verdict

    on core_to_wax(ctx) {
        let verdict = perform evaluate(ctx.piece.serial);
        if verdict.accept {
            goto AtWax(ctx.piece);
        }
        goto Scrapped("rejected");
    }
}
"#;

#[test]
fn goto_in_a_bare_if_does_not_fall_through() {
    let driver = r#"
package behaviour

import "testing"

type acceptAll struct{}

func (acceptAll) Evaluate(serial string) Verdict { return Verdict{Accept: true} }

type rejectAll struct{}

func (rejectAll) Evaluate(serial string) Verdict { return Verdict{Accept: false} }

// The taken branch must win. Before #136 this landed in Scrapped, because the
// `goto` assigned AtWax and execution continued into the trailing `goto`.
func TestAcceptedPieceLandsInAtWax(t *testing.T) {
	m := &Lifecycle{
		State:      LifecycleStateAtCore,
		AtCoreData: &LifecycleAtCoreData{Piece: Piece{Serial: "P-1"}},
	}
	if err := m.CoreToWax(acceptAll{}); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if m.State != LifecycleStateAtWax {
		t.Fatalf("accepted piece should land in AtWax, got %v", m.State)
	}
	if m.AtWaxData == nil || m.AtWaxData.Piece.Serial != "P-1" {
		t.Fatalf("state payload did not survive the transition")
	}
}

// The untaken branch must still reach the trailing goto.
func TestRejectedPieceLandsInScrapped(t *testing.T) {
	m := &Lifecycle{
		State:      LifecycleStateAtCore,
		AtCoreData: &LifecycleAtCoreData{Piece: Piece{Serial: "P-2"}},
	}
	if err := m.CoreToWax(rejectAll{}); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if m.State != LifecycleStateScrapped {
		t.Fatalf("rejected piece should land in Scrapped, got %v", m.State)
	}
}
"#;

    if let Err(diagnostics) = run_go_behaviour(BRANCHING, driver) {
        panic!("generated Go behaved incorrectly:\n{diagnostics}");
    }
}

/// The fall-through's second failure mode: a later `goto` that reads a
/// source-state field dereferences the pointer the taken branch already nulled
/// via `clearStateData()`. Distinct from landing in the wrong state, because it
/// panics rather than silently succeeding.
const FALLTHROUGH_READS_SOURCE: &str = r#"
type Verdict { accept: bool }

machine Router {
    state Pending(id: String, attempts: i64)
    state Accepted(id: String)
    state Retried(id: String, attempts: i64)

    transition route: Pending -> Accepted | Retried

    effect judge(id: String) -> Verdict

    on route(ctx) {
        let verdict = perform judge(ctx.id);
        if verdict.accept {
            goto Accepted(ctx.id);
        }
        goto Retried(ctx.id, ctx.attempts);
    }
}
"#;

#[test]
fn goto_fallthrough_does_not_dereference_cleared_state() {
    let driver = r#"
package behaviour

import "testing"

type accept struct{}

func (accept) Judge(id string) Verdict { return Verdict{Accept: true} }

// Before #136 this panicked: the taken branch called clearStateData(), then the
// trailing goto read PendingData.Attempts off the nulled pointer.
func TestAcceptDoesNotPanicOnClearedSourceState(t *testing.T) {
	m := &Router{
		State:       RouterStatePending,
		PendingData: &RouterPendingData{Id: "R-1", Attempts: 2},
	}
	if err := m.Route(accept{}); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if m.State != RouterStateAccepted {
		t.Fatalf("expected Accepted, got %v", m.State)
	}
}
"#;

    if let Err(diagnostics) = run_go_behaviour(FALLTHROUGH_READS_SOURCE, driver) {
        panic!("generated Go behaved incorrectly:\n{diagnostics}");
    }
}

/// A `goto` in tail position must *not* emit its own `return`, or `go vet`
/// reports unreachable code before the method's trailing `return nil`.
///
/// This is the constraint that makes the fix non-trivial: the Rust backend
/// avoids `clippy::needless_return` for the same reason, and the two rules have
/// to agree about what counts as tail position.
const TAIL_GOTO: &str = r#"
machine Simple {
    state Start(n: i64)
    state Done(n: i64)

    transition go: Start -> Done

    on go(ctx) {
        goto Done(ctx.n);
    }
}
"#;

#[test]
fn tail_goto_leaves_no_unreachable_return() {
    let driver = r#"
package behaviour

import "testing"

// A machine declaring no effects generates a transition method with no
// parameters — there is no effects interface to pass.
func TestTailGotoTransitions(t *testing.T) {
	m := &Simple{State: SimpleStateStart, StartData: &SimpleStartData{N: 7}}
	if err := m.Go(); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if m.State != SimpleStateDone {
		t.Fatalf("expected Done, got %v", m.State)
	}
	if m.DoneData == nil || m.DoneData.N != 7 {
		t.Fatalf("payload did not survive")
	}
}
"#;

    if let Err(diagnostics) = run_go_behaviour(TAIL_GOTO, driver) {
        panic!("generated Go behaved incorrectly:\n{diagnostics}");
    }
}
