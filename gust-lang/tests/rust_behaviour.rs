//! Runs generated Rust and asserts what it *does*, not that it compiles.
//!
//! The mirror of `go_behaviour.rs`, and it exists because the confidence was
//! asymmetric. Go got a behavioural suite when #136 showed a `goto` that did
//! not end the handler; the Rust half of the same defect (#121) was fixed
//! earlier and never got one. "The backend that had the bug we found" is not a
//! principled basis for which backend gets tested.
//!
//! Compiling proves output is well-formed, never that it implements the source
//! machine. Two backends cleared that bar for their whole existence while
//! discarding handler bodies wholesale, and were removed in 1.0 rather than
//! frozen into the stability promise.
//!
//! Each case writes the generated crate plus a driver and runs `cargo test`.

use gust_lang::{RustCodegen, parse_program_with_errors};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Generate Rust for `source`, pair it with `driver`, and run `cargo test`.
fn run_rust_behaviour(source: &str, driver: &str) -> Result<(), String> {
    let program =
        parse_program_with_errors(source, "behaviour.gu").expect("fixture source should parse");
    let generated = RustCodegen::new().generate(&program);

    let dir = tempfile::tempdir().expect("create tempdir");
    let src = dir.path().join("src");
    std::fs::create_dir(&src).expect("create src");
    // The generated code is the crate root; the driver is a `#[cfg(test)]`
    // module appended to it, so it sees private items exactly as a consumer's
    // own tests would.
    std::fs::write(src.join("lib.rs"), format!("{generated}\n\n{driver}\n")).expect("write lib.rs");

    let runtime = workspace_root()
        .join("gust-runtime")
        .display()
        .to_string()
        .replace('\\', "/");
    std::fs::write(
        dir.path().join("Cargo.toml"),
        format!(
            "[package]\nname = \"gust-behaviour\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
             [dependencies]\n\
             gust-runtime = {{ path = \"{runtime}\" }}\n\
             serde = {{ version = \"1.0\", features = [\"derive\"] }}\n\
             serde_json = \"1.0\"\n\
             thiserror = \"2.0\"\n\
             tokio = {{ version = \"1\", features = [\"full\"] }}\n\n[workspace]\n"
        ),
    )
    .expect("write Cargo.toml");

    let output = Command::new(env!("CARGO"))
        .args(["test", "--quiet"])
        .current_dir(dir.path())
        .env(
            "CARGO_TARGET_DIR",
            workspace_root().join("target/rust-behaviour"),
        )
        .output()
        .expect("cargo test should run");

    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "--- generated ---\n{generated}\n--- cargo test ---\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    ))
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("gust-lang has a parent")
        .to_path_buf()
}

/// A multi-target transition whose first `goto` sits in a bare `if`.
///
/// Deliberately the same machine as `go_behaviour.rs` uses, so the two suites
/// assert the same behaviour of the same source rather than each testing
/// whatever its author happened to think of.
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
#[cfg(test)]
mod behaviour_tests {
    use super::*;

    struct AcceptAll;
    impl LifecycleEffects for AcceptAll {
        fn evaluate(&self, _serial: &str) -> Verdict {
            Verdict { accept: true, reason: String::new() }
        }
    }

    struct RejectAll;
    impl LifecycleEffects for RejectAll {
        fn evaluate(&self, _serial: &str) -> Verdict {
            Verdict { accept: false, reason: "no".to_string() }
        }
    }

    fn at_core() -> Lifecycle {
        Lifecycle {
            state: LifecycleState::AtCore {
                piece: Piece { serial: "P-1".to_string() },
            },
        }
    }

    // The taken branch must win. If `goto` did not end the handler, the machine
    // would assign AtWax and continue into the trailing `goto`, landing in
    // Scrapped no matter what the effect returned.
    #[test]
    fn accepted_piece_lands_in_at_wax() {
        let mut m = at_core();
        m.core_to_wax(&AcceptAll).expect("transition should succeed");
        assert!(
            matches!(m.state, LifecycleState::AtWax { .. }),
            "accepted piece should land in AtWax, got {:?}",
            m.state
        );
    }

    #[test]
    fn rejected_piece_lands_in_scrapped() {
        let mut m = at_core();
        m.core_to_wax(&RejectAll).expect("transition should succeed");
        assert!(
            matches!(m.state, LifecycleState::Scrapped { .. }),
            "rejected piece should land in Scrapped, got {:?}",
            m.state
        );
    }

    // The accepted branch carries the piece through. A fall-through would have
    // overwritten it with the Scrapped payload, so this pins the *data* as well
    // as the state.
    #[test]
    fn accepted_piece_keeps_its_payload() {
        let mut m = at_core();
        m.core_to_wax(&AcceptAll).expect("transition should succeed");
        match &m.state {
            LifecycleState::AtWax { piece } => assert_eq!(piece.serial, "P-1"),
            other => panic!("expected AtWax, got {other:?}"),
        }
    }

    // A transition fired from the wrong state must be refused rather than
    // applied, and must leave the machine where it was.
    #[test]
    fn transition_from_the_wrong_state_is_refused() {
        let mut m = Lifecycle {
            state: LifecycleState::Scrapped { reason: "already".to_string() },
        };
        let result = m.core_to_wax(&AcceptAll);
        assert!(result.is_err(), "core_to_wax from Scrapped should fail");
        assert!(
            matches!(m.state, LifecycleState::Scrapped { .. }),
            "a refused transition must not move the machine"
        );
    }
}
"#;

    if let Err(diagnostics) = run_rust_behaviour(BRANCHING, driver) {
        panic!("generated Rust did not behave as the source specifies:\n{diagnostics}");
    }
}
