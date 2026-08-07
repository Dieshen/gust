//! Compiles generated output with each backend's real toolchain.
//!
//! Every other codegen test asserts on strings. Strings do not tell you whether
//! `rustc` accepts the result, and the non-default backends had never once had
//! their output fed to a compiler. Most did not compile at all when first tried.
//!
//! `wasm` and `nostd` were removed in 1.0 rather than frozen into the stability
//! promise: both compiled while discarding the machine's actual semantics, which
//! is precisely the failure this table cannot catch. Compiling proves
//! well-formedness, not behaviour.
//!
//! The table below is the point: adding a fixture exercises every backend, so
//! coverage cannot drift backend-by-backend the way it did before.

use gust_lang::ast::Program;
use gust_lang::{CffiCodegen, GoCodegen, RustCodegen, parse_program};
use std::path::{Path, PathBuf};
use std::process::Command;

// ─── fixtures ───────────────────────────────────────────────────────────────

struct Fixture {
    name: &'static str,
    source: &'static str,
}

/// Exercises user types, a fieldless enum, state fields, sync and async
/// effects, ctx rewriting, and branching — the shapes that broke in practice.
const RICH: &str = r#"
enum Tier { Fast, Slow }

type Config { service_name: String, retries: i64, tier: Tier }

machine DeployPipeline {
    state Idle(config: Config)
    state Running(config: Config, attempt: i64)
    state Done(message: String)
    state Failed(reason: String)

    transition start: Idle -> Running
    transition finish: Running -> Done | Failed

    async effect deploy(name: String) -> String
    effect log(msg: String) -> bool

    async on start(ctx: StartCtx) {
        let result = perform deploy(ctx.config.service_name);
        perform log(result);
        goto Running(ctx.config, 1);
    }

    async on finish(ctx: FinishCtx) {
        if ctx.attempt > ctx.config.retries {
            goto Failed("max retries exceeded");
        } else {
            let msg = perform deploy(ctx.config.service_name);
            goto Done(msg);
        }
    }
}
"#;

/// Fieldless states only. Kept as a distinct fixture because it isolates which
/// shapes a backend can handle from which it merely happens to accept.
const FIELDLESS: &str = r#"
machine Toggle {
    state Off
    state On

    transition flip: Off -> On
    transition reset: On -> Off
}
"#;

/// A `let` whose value is never read. Go rejects an unused local outright and
/// Rust's `unused_variables` fails a consumer building with `-D warnings`, so
/// both backends have to lower this to a discard. See #100.
const UNUSED_BINDING: &str = r#"
machine Probe {
    state Idle(id: String)
    state Done(id: String)

    transition go: Idle -> Done

    effect check(a: String) -> bool

    on go(ctx: GoCtx) {
        let unread = perform check(ctx.id);
        goto Done(ctx.id);
    }
}
"#;

/// A `timeout` transition. This is one of the two forms that make the Rust
/// backend emit `tokio` into the prelude, and no fixture had either — which is
/// how a redundant `use tokio;` survived in output consumers build with
/// `-D warnings`.
const TIMEOUT: &str = r#"
machine Dispatcher {
    state Idle
    state Done(note: String)

    transition run: Idle -> Done timeout 5s

    async on run() {
        goto Done("dispatched");
    }
}
"#;

/// A channel declaration and a `send`, the other form that reaches the `tokio`
/// prelude.
const CHANNEL: &str = r#"
channel OrderEvents: String (capacity: 32, mode: broadcast)

machine Notifier {
    state Idle
    state Sent

    transition notify: Idle -> Sent

    async on notify() {
        send OrderEvents("started");
        goto Sent();
    }
}
"#;

/// The three machine-header annotations — `sends`, `receives`, `supervises` —
/// against both channel modes. No fixture had any of them, so the only backend
/// path that reads `machine.sends` was never compiled: the Rust backend emitted
/// its `send_*` helper at module scope with a `&self` receiver, which `rustc`
/// rejects outright ("`self` parameter is only allowed in associated
/// functions"). That made channels unusable on Rust for any machine declaring
/// `sends`.
///
/// `receives` and `supervises` currently lower to nothing on the Rust backend
/// and to a `SupervisionSpec` table on Go; they are here so that stays a
/// deliberate no-op rather than an untested one.
const CHANNEL_ANNOTATIONS: &str = r#"
type Job { id: String }

channel Jobs: Job (capacity: 8, mode: mpsc)
channel Audit: String (capacity: 4, mode: broadcast)

machine Worker(receives Jobs) {
    state Waiting
    state Busy

    transition accept: Waiting -> Busy

    on accept() {
        goto Busy;
    }
}

machine Producer(sends Jobs, sends Audit, supervises Worker(one_for_one)) {
    state Idle
    state Sent

    transition emit: Idle -> Sent

    on emit(job: Job) {
        send Jobs(job);
        send Audit("queued");
        goto Sent;
    }
}
"#;

/// A handler reading source-state fields by bare name rather than through a ctx
/// parameter. The Rust backend gets these from destructuring its match arm; the
/// Go backend had nothing in scope and emitted `undefined: tokens`.
const SOURCE_STATE_FIELDS: &str = r#"
machine Bucket {
    state Available(tokens: i64, max_tokens: i64)
    state Exhausted(max_tokens: i64)

    transition acquire: Available -> Available | Exhausted
    transition refill: Exhausted -> Available

    on acquire() {
        if tokens > 0 {
            goto Available(tokens - 1, max_tokens);
        } else {
            goto Exhausted(max_tokens);
        }
    }

    on refill() {
        goto Available(max_tokens, max_tokens);
    }
}
"#;

/// A `Result`-returning effect destructured with `Ok`/`Err`, plus an `async`
/// effect returning `()`. Go has no `Result`, so both the effect signature and
/// the match have to lower to the `(T, error)` idiom; previously the match
/// emitted a `switch` over `undefined: Ok`, and the `()` effect's `perform`
/// bound two values from a one-value call.
const RESULT_MATCH: &str = r#"
machine Fetcher {
    state Start
    state Done(body: String)
    state Failed(reason: String)

    transition run: Start -> Done | Failed

    async effect fetch() -> Result<String, String>
    async effect audit() -> ()

    async on run() {
        perform audit();
        let outcome = perform fetch();
        match outcome {
            Ok(body) => {
                goto Done(body);
            }
            Err(reason) => {
                goto Failed(reason);
            }
        }
    }
}
"#;

/// A generic machine whose handler takes a parameter typed by the machine's own
/// type parameter. Two failures met here: ctx detection treated `T` as an
/// unknown type and swallowed the parameter (in the Rust backend too), and Go
/// referenced the generated state-data struct without its type arguments.
const GENERIC_MACHINE: &str = r#"
machine Holder<T> {
    state Empty
    state Full(value: T, revision: i64)

    transition put: Empty -> Full
    transition clear: Full -> Empty

    on put(value: T) {
        goto Full(value, 1);
    }

    on clear() {
        goto Empty();
    }
}
"#;

/// An early `goto` inside a bare `if`, plus a `goto` whose later argument
/// borrows a value an earlier argument moves.
///
/// `goto` used to emit a bare state assignment with no `return`, so the first
/// shape fell through — the machine ended in whichever state the *last*
/// assignment named, and any moved value left the rest of the handler using
/// it (`E0382`). The second shape is why `saga.gu` did not compile: struct
/// fields evaluate in order, so `goto Compensating(completed, len(completed))`
/// moved `completed` and then borrowed it.
///
/// `gust check` reported nothing for either.
const EARLY_GOTO: &str = r#"
machine Router {
    state Start(items: Vec<String>, n: i64)
    state Early(items: Vec<String>)
    state Late(items: Vec<String>, remaining: i64)

    transition route: Start -> Early | Late

    effect count(items: Vec<String>) -> i64

    on route(ctx: RouteCtx) {
        if ctx.n > 0 {
            goto Early(ctx.items);
        }
        goto Late(ctx.items, perform count(ctx.items) - 1);
    }
}
"#;

/// Supervision where the child's first state carries **no** fields, so its
/// constructor takes no arguments and `spawn` must pass none.
///
/// The `supervision` fixture below happens to use a child whose first state has
/// exactly one field, matched by a one-argument `spawn` — so arity agreed by
/// luck and the mismatch stayed invisible. A child with a fieldless first state
/// generated `Worker::new(first.clone())` against `fn new() -> Self`, which is
/// `E0061`, while `gust check` reported "Check passed". Shipped in 0.4.0.
const SUPERVISION_NO_ARGS: &str = r#"
machine Worker {
    state Idle
    state Busy(job: String)

    transition start: Idle -> Busy

    on start(job: String) {
        goto Busy(job);
    }
}

machine Boss(supervises Worker(one_for_one)) {
    state Ready(first: String)
    state Running(current: String)

    transition go: Ready -> Running

    on go(ctx: GoCtx) {
        spawn Worker();
        goto Running(ctx.first);
    }
}
"#;

/// A machine that supervises a child and actually spawns one.
///
/// Nothing exercised this before: `supervises` emitted nothing at all in the
/// Rust backend, `spawn` emitted a future that discarded its arguments and
/// returned `Ok(())` in both backends, and the one example in the repo that
/// declares `supervises` never calls `spawn`. So the whole feature compiled
/// and did nothing, on every target, undetected.
///
/// Covers: the generated supervision contract, child construction from the
/// spawn arguments, and a non-`Copy` argument that the handler still needs
/// afterwards (`spawn StepRunner(first)` followed by `goto Running(first)`),
/// which must be cloned rather than moved into the child.
const SUPERVISION: &str = r#"
machine StepRunner {
    state Idle(step: String)
    state Done(result: String)

    transition complete: Idle -> Done

    effect run_step(step: String) -> String

    on complete(ctx: CompleteCtx) {
        let result = perform run_step(ctx.step);
        goto Done(result);
    }
}

machine Engine(supervises StepRunner(one_for_one)) {
    state Ready(first: String)
    state Running(current: String)

    transition start: Ready -> Running

    on start(ctx: StartCtx) {
        spawn StepRunner(ctx.first);
        goto Running(ctx.first);
    }
}
"#;

/// `gust-stdlib/health_check.gu` verbatim: a generic machine that **reads** a
/// generic-typed state field.
///
/// `STDLIB_RETRY` below is generic too and passed the whole time, because its
/// `T` only ever arrives owned from an effect — it never reads a `T`-typed
/// field out of the source state. That distinction is the entire bug: the
/// borrow strategy hoists `let status = status.clone();`, and on a bare type
/// parameter with no `Clone` bound that yields `&T`, so the following `goto`
/// fails with `expected type parameter T, found &T`.
///
/// Five of six stdlib machines failed to compile as Rust on account of this
/// and of unused type parameters, while the one fixture covering the stdlib
/// was the single machine that worked. Hence a fixture for the shape that
/// breaks, not just the one that passes.
const STDLIB_HEALTH_CHECK: &str = r#"
machine HealthCheck<T> {
    state Healthy(status: T)
    state Degraded(status: T, failures: i64)
    state Unhealthy(reason: String)

    transition probe: Healthy -> Healthy | Degraded | Unhealthy
    transition recover: Degraded -> Healthy | Unhealthy

    async effect run_probe() -> Result<T, String>

    async on probe() {
        let result = perform run_probe();
        match result {
            Ok(next_status) => {
                goto Healthy(next_status);
            }
            Err(err) => {
                goto Degraded(status, 1);
            }
        }
    }

    async on recover() {
        let result = perform run_probe();
        match result {
            Ok(next_status) => {
                goto Healthy(next_status);
            }
            Err(err) => {
                goto Unhealthy(err);
            }
        }
    }
}
"#;

/// `gust-stdlib/retry.gu` verbatim: a generic machine that reads source-state
/// fields by bare name and destructures a `Result`. Every defect above at once,
/// which is what made the whole standard library Rust-only.
const STDLIB_RETRY: &str = r#"
machine Retry<T> {
    state Ready(max_attempts: i64, base_delay_ms: i64, max_delay_ms: i64, jitter_pct: i64)
    state Attempting(attempt: i64, max_attempts: i64, base_delay_ms: i64, max_delay_ms: i64, jitter_pct: i64)
    state Waiting(attempt: i64, delay_ms: i64, max_attempts: i64, base_delay_ms: i64, max_delay_ms: i64, jitter_pct: i64)
    state Succeeded(value: T, attempts: i64)
    state Failed(error: String, attempts: i64)

    transition begin: Ready -> Attempting
    transition run: Attempting -> Waiting | Succeeded | Failed
    transition wait_complete: Waiting -> Attempting

    async effect execute_operation() -> Result<T, String>
    async effect sleep_ms(duration_ms: i64) -> i64
    effect compute_backoff(base_delay_ms: i64, attempt: i64, max_delay_ms: i64, jitter_pct: i64) -> i64

    on begin() {
        goto Attempting(1, max_attempts, base_delay_ms, max_delay_ms, jitter_pct);
    }

    async on run() {
        let result = perform execute_operation();
        match result {
            Ok(value) => {
                goto Succeeded(value, attempt);
            }
            Err(err) => {
                if attempt >= max_attempts {
                    goto Failed(err, attempt);
                } else {
                    let delay = perform compute_backoff(base_delay_ms, attempt, max_delay_ms, jitter_pct);
                    goto Waiting(attempt, delay, max_attempts, base_delay_ms, max_delay_ms, jitter_pct);
                }
            }
        }
    }

    async on wait_complete() {
        perform sleep_ms(delay_ms);
        goto Attempting(attempt + 1, max_attempts, base_delay_ms, max_delay_ms, jitter_pct);
    }
}
"#;

fn fixtures() -> Vec<Fixture> {
    vec![
        Fixture {
            name: "rich",
            source: RICH,
        },
        Fixture {
            name: "timeout",
            source: TIMEOUT,
        },
        Fixture {
            name: "channel",
            source: CHANNEL,
        },
        Fixture {
            name: "channel-annotations",
            source: CHANNEL_ANNOTATIONS,
        },
        Fixture {
            name: "fieldless",
            source: FIELDLESS,
        },
        Fixture {
            name: "unused-binding",
            source: UNUSED_BINDING,
        },
        Fixture {
            name: "source-state-fields",
            source: SOURCE_STATE_FIELDS,
        },
        Fixture {
            name: "result-match",
            source: RESULT_MATCH,
        },
        Fixture {
            name: "generic-machine",
            source: GENERIC_MACHINE,
        },
        Fixture {
            name: "stdlib-retry",
            source: STDLIB_RETRY,
        },
        Fixture {
            name: "stdlib-health-check",
            source: STDLIB_HEALTH_CHECK,
        },
        Fixture {
            name: "supervision",
            source: SUPERVISION,
        },
        Fixture {
            name: "supervision-no-args",
            source: SUPERVISION_NO_ARGS,
        },
        Fixture {
            name: "early-goto",
            source: EARLY_GOTO,
        },
    ]
}

// ─── backends ───────────────────────────────────────────────────────────────

/// How a backend's output is proven to be real code.
enum Verify {
    /// Build a crate and run cargo against it.
    Cargo {
        /// Extra `[dependencies]` lines for the generated crate.
        deps: &'static str,
        /// rustup target triple, when the backend does not build for host.
        target: Option<&'static str>,
        /// Deny all warnings, not just errors.
        deny_warnings: bool,
    },
    /// Write a Go package and run `go vet`.
    GoVet,
}

struct Backend {
    name: &'static str,
    generate: fn(&Program) -> String,
    verify: Verify,
    /// Fixtures this backend cannot handle yet, each with the tracking issue.
    /// Listed explicitly rather than silently skipped so the gap stays visible.
    unsupported: &'static [(&'static str, &'static str)],
}

fn backends() -> Vec<Backend> {
    vec![
        Backend {
            name: "rust",
            generate: |p| RustCodegen::new().generate(p),
            verify: Verify::Cargo {
                deps: "gust-runtime = { path = \"GUST_RUNTIME_PATH\" }\n\
                       serde = { version = \"1.0\", features = [\"derive\"] }\n\
                       tokio = { version = \"1\", features = [\"full\"] }\n\
                       thiserror = \"2.0\"",
                target: None,
                // Consumers build with -D warnings; output that merely compiles
                // still breaks them.
                deny_warnings: true,
            },
            unsupported: &[],
        },
        Backend {
            name: "ffi",
            // The second element is the C header. Verifying that would need a C
            // toolchain in CI, so only the Rust half is compiled here.
            generate: |p| CffiCodegen::new().generate(p).0,
            verify: Verify::Cargo {
                deps: "",
                target: None,
                deny_warnings: false,
            },
            unsupported: &[],
        },
        Backend {
            name: "go",
            generate: |p| GoCodegen::new().generate(p, "main"),
            verify: Verify::GoVet,
            unsupported: &[],
        },
    ]
}

// ─── harness ────────────────────────────────────────────────────────────────

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("gust-lang has a workspace parent")
        .to_path_buf()
}

fn toml_path(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

fn installed_targets() -> String {
    Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// Builds a crate around `generated` and runs cargo over it. Returns the
/// combined diagnostics on failure.
fn verify_with_cargo(
    generated: &str,
    label: &str,
    deps: &str,
    target: Option<&str>,
    deny_warnings: bool,
) -> Result<(), String> {
    let root = workspace_root();
    let dir = tempfile::tempdir().expect("create tempdir");
    let src = dir.path().join("src");
    std::fs::create_dir(&src).expect("create src");
    std::fs::write(src.join("lib.rs"), generated).expect("write lib.rs");

    let deps = deps.replace("GUST_RUNTIME_PATH", &toml_path(&root.join("gust-runtime")));
    // Edition 2021, not 2024: consumers are not all on the newer edition, and
    // generated code has to be valid on both.
    std::fs::write(
        dir.path().join("Cargo.toml"),
        format!(
            "[package]\nname = \"gust-backend-{label}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
             [dependencies]\n{deps}\n\n[workspace]\n"
        ),
    )
    .expect("write Cargo.toml");

    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg(if deny_warnings { "clippy" } else { "check" })
        .arg("--quiet");
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    if deny_warnings {
        cmd.args(["--", "-D", "warnings"]);
    }
    // A shared target dir keeps dependency builds cached across the whole
    // table instead of recompiling per cell.
    let output = cmd
        .current_dir(dir.path())
        .env("CARGO_TARGET_DIR", root.join("target/codegen-backends"))
        .output()
        .expect("cargo should run");

    if output.status.success() {
        return Ok(());
    }
    Err(String::from_utf8_lossy(&output.stderr).into_owned())
}

fn verify_with_go_vet(generated: &str) -> Result<(), String> {
    let dir = tempfile::tempdir().expect("create tempdir");
    std::fs::write(dir.path().join("machine.go"), generated).expect("write go file");
    std::fs::write(dir.path().join("go.mod"), "module testpkg\n\ngo 1.21\n").expect("write go.mod");

    let output = Command::new("go")
        .args(["vet", "./..."])
        .current_dir(dir.path())
        .output()
        .expect("go vet should run");

    if output.status.success() {
        return Ok(());
    }
    Err(String::from_utf8_lossy(&output.stderr).into_owned())
}

/// The whole table in one test. A single test rather than one per cell so the
/// report lists every failing combination at once instead of stopping at the
/// first — when a codegen change breaks three backends, you want to see three.
#[test]
fn every_backend_produces_code_its_toolchain_accepts() {
    let targets = installed_targets();
    let mut failures: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut checked = 0usize;

    for fixture in fixtures() {
        let program = parse_program(fixture.source)
            .unwrap_or_else(|e| panic!("fixture '{}' should parse: {e}", fixture.name));

        for backend in backends() {
            let cell = format!("{}/{}", backend.name, fixture.name);

            if let Some((_, why)) = backend.unsupported.iter().find(|(f, _)| *f == fixture.name) {
                skipped.push(format!("{cell} — unsupported, {why}"));
                continue;
            }

            let generated = (backend.generate)(&program);

            let result = match &backend.verify {
                Verify::Cargo {
                    deps,
                    target,
                    deny_warnings,
                } => {
                    if let Some(t) = target {
                        if !targets.contains(t) {
                            skipped.push(format!("{cell} — rustup target '{t}' not installed"));
                            continue;
                        }
                    }
                    verify_with_cargo(
                        &generated,
                        &cell.replace('/', "-"),
                        deps,
                        *target,
                        *deny_warnings,
                    )
                }
                Verify::GoVet => verify_with_go_vet(&generated),
            };

            checked += 1;
            if let Err(diagnostics) = result {
                failures.push(format!(
                    "\n=== {cell} ===\n--- generated ---\n{generated}\n--- diagnostics ---\n{diagnostics}"
                ));
            }
        }
    }

    // Printed rather than silent: a skipped cell is not a passing cell, and the
    // reason it was skipped is the thing worth noticing.
    for entry in &skipped {
        eprintln!("skipped: {entry}");
    }

    assert!(
        failures.is_empty(),
        "{} of {checked} backend/fixture combinations produced code their toolchain rejected:{}",
        failures.len(),
        failures.join("")
    );
    assert!(checked > 0, "harness verified nothing");
}
