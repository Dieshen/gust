use gust_lang::{RustCodegen, parse_program};

#[test]
fn async_effect_and_handler_generate_async_rust() {
    let source = r#"
machine Payments {
    state Pending
    state Done(receipt: String)

    transition charge: Pending -> Done

    async effect process() -> String

    async on charge() {
        let receipt = perform process();
        goto Done(receipt);
    }
}
"#;

    let program = parse_program(source).expect("source should parse");
    let generated = RustCodegen::new().generate(&program);

    // Desugared to RPITIT rather than `async fn`, which would trip the
    // `async_fn_in_trait` lint in the consumer's crate. See #99.
    assert!(
        generated
            .contains("fn process(&self) -> impl ::core::future::Future<Output = String> + Send;")
    );
    assert!(!generated.contains("async fn process"));
    assert!(generated.contains("pub async fn charge("));
    assert!(generated.contains("effects.process().await"));
}

#[test]
fn async_effect_returning_unit_desugars_to_future_of_unit() {
    let source = r#"
machine Notifier {
    state Idle
    state Sent

    transition notify: Idle -> Sent

    async effect emit() -> ()

    async on notify() {
        perform emit();
        goto Sent;
    }
}
"#;

    let program = parse_program(source).expect("source should parse");
    let generated = RustCodegen::new().generate(&program);

    assert!(
        generated.contains("fn emit(&self) -> impl ::core::future::Future<Output = ()> + Send;")
    );
}

#[test]
fn sync_effect_keeps_plain_signature() {
    let source = r#"
machine Calc {
    state Idle
    state Done(total: i64)

    transition run: Idle -> Done

    effect total() -> i64
    effect log_it() -> ()

    on run() {
        let t = perform total();
        goto Done(t);
    }
}
"#;

    let program = parse_program(source).expect("source should parse");
    let generated = RustCodegen::new().generate(&program);

    assert!(generated.contains("fn total(&self) -> i64;"));
    assert!(generated.contains("fn log_it(&self);"));
    assert!(!generated.contains("Future"));
}

/// A fieldless enum read out of a struct field is a partial move unless it is
/// Copy, which made the generated code fail to compile. See #99.
#[test]
fn fieldless_enum_derives_copy() {
    let source = r#"
enum Bucket {
    Fast,
    Slow,
}

machine Router {
    state Idle
    state Done

    transition go: Idle -> Done
}
"#;

    let program = parse_program(source).expect("source should parse");
    let generated = RustCodegen::new().generate(&program);

    assert!(
        generated
            .contains("#[derive(Debug, Clone, Copy, Serialize, Deserialize)]\npub enum Bucket {")
    );
}

/// Copy must NOT be derived when any variant carries a payload, since the
/// payload types (String, Vec, ...) are generally not Copy.
#[test]
fn enum_with_payload_does_not_derive_copy() {
    let source = r#"
enum Status {
    Pending,
    Done(String),
}

machine Tracker {
    state Idle
    state Finished

    transition go: Idle -> Finished
}
"#;

    let program = parse_program(source).expect("source should parse");
    let generated = RustCodegen::new().generate(&program);

    assert!(
        generated.contains("#[derive(Debug, Clone, Serialize, Deserialize)]\npub enum Status {")
    );
    assert!(!generated.contains("Copy"));
}

/// Transitions bind state fields by reference and take owned copies at arm
/// entry. A field whose type is a fieldless (and therefore `Copy`) enum must be
/// dereferenced, not cloned — `.clone()` on a Copy type trips
/// clippy::clone_on_copy, which fails consumers building with -D warnings.
/// `is_copy_type` only knows primitives, so user enums need separate tracking.
#[test]
fn copy_state_fields_are_dereferenced_not_cloned() {
    let source = r#"
enum Tier { Fast, Slow }

machine Router {
    state Idle(tier: Tier, attempt: i64, label: String)
    state Done(tier: Tier, attempt: i64, label: String)

    transition go: Idle -> Done

    on go() {
        goto Done(tier, attempt, label);
    }
}
"#;

    let program = parse_program(source).expect("source should parse");
    let generated = RustCodegen::new().generate(&program);

    // Fieldless user enum: Copy, so deref.
    assert!(
        generated.contains("let tier = *tier;"),
        "fieldless enum field should be dereferenced, got:\n{generated}"
    );
    // Primitive: Copy, so deref.
    assert!(
        generated.contains("let attempt = *attempt;"),
        "primitive field should be dereferenced, got:\n{generated}"
    );
    // Non-Copy: must still clone.
    assert!(
        generated.contains("let label = label.clone();"),
        "String field should be cloned, got:\n{generated}"
    );
}

#[test]
fn enum_and_match_generate_rust_enum_and_match() {
    let source = r#"
enum Status {
    Pending,
    Done(String),
}

machine Tracker {
    state Idle(status: Status)
    state Finished(msg: String)

    transition finish: Idle -> Finished

    on finish() {
        match status {
            Status::Done(msg) => { goto Finished(msg); }
            _ => { goto Finished("unknown"); }
        }
    }
}
"#;

    let program = parse_program(source).expect("source should parse");
    let generated = RustCodegen::new().generate(&program);

    assert!(generated.contains("pub enum Status {"));
    assert!(generated.contains("Pending,"));
    assert!(generated.contains("Done(String),"));
    assert!(generated.contains("match status {"));
}

#[test]
fn test_ctx_field_rewrite_and_borrows() {
    let source = r#"
type Order {
    id: String,
    items: Vec<String>,
}
type Money {
    cents: i64,
}
machine Processor {
    state Pending(order: Order)
    state Done(order: Order, total: Money)
    state Failed(reason: String)

    transition process: Pending -> Done | Failed

    effect calculate_total(order: Order) -> Money

    on process(ctx: ProcessCtx) {
        let total = perform calculate_total(ctx.order);
        if total.cents > 0 {
            goto Done(ctx.order, total);
        } else {
            goto Failed("bad total");
        }
    }
}
"#;
    let program = parse_program(source).expect("should parse");
    let generated = RustCodegen::new().generate(&program);

    // Bug 1: ctx.field must be rewritten to direct field access
    assert!(
        !generated.contains("ctx.order"),
        "ctx.field should be rewritten to field"
    );
    assert!(
        !generated.contains("ctx: ProcessCtx"),
        "ctx param should not appear in method sig"
    );

    // Matches by reference and clones only the fields the handler uses, rather
    // than deep-copying the whole state before the from-state check.
    assert!(
        generated.contains("match &self.state"),
        "should match state by reference"
    );
    assert!(
        !generated.contains("self.state.clone()"),
        "should not deep-copy the whole state per transition"
    );
    assert!(
        generated.contains("let order = order.clone();"),
        "should clone only the referenced field, got:\n{generated}"
    );

    // Bug 5: perform args must be passed by reference
    assert!(
        generated.contains("effects.calculate_total(&"),
        "perform args should be references"
    );

    // Bug 4: no unnecessary parens in if condition
    assert!(
        !generated.contains("if (total"),
        "if condition should not have outer parens"
    );
    assert!(
        generated.contains("if total.cents > 0"),
        "if condition should be bare"
    );
}

#[test]
fn test_enum_path_in_expression() {
    let source = r#"
enum Stage { Build, Test, Deploy }
machine Pipeline {
    state Waiting(stage: Stage)
    state Running(stage: Stage)
    transition advance: Waiting -> Running
    on advance(ctx: AdvanceCtx) {
        goto Running(Stage::Build);
    }
}
"#;
    let program = parse_program(source).expect("should parse with enum path in expression");
    let generated = RustCodegen::new().generate(&program);
    assert!(
        generated.contains("Stage::Build"),
        "enum path should appear in generated Rust"
    );
}

#[test]
fn test_generated_rust_structural_validity() {
    let source = r#"
type Item { name: String, price: i64 }
machine Cart {
    state Empty
    state HasItems(items: Vec<Item>, total: i64)
    state CheckedOut(receipt: String)

    transition add_item: Empty -> HasItems
    transition checkout: HasItems -> CheckedOut

    effect compute_receipt(items: Vec<Item>) -> String

    on checkout(ctx: CheckoutCtx) {
        let receipt = perform compute_receipt(ctx.items);
        goto CheckedOut(receipt);
    }
}
"#;
    let program = parse_program(source).expect("should parse");
    let generated = RustCodegen::new().generate(&program);

    // Structural checks that would cause compilation failures:
    // 1. No undefined variables (ctx should be rewritten)
    assert!(!generated.contains("ctx."), "no ctx references");
    // 2. No type-unknown params in signatures
    assert!(
        !generated.contains("CheckoutCtx"),
        "no phantom types in sigs"
    );
    // 3. Proper match form — borrowed, not a whole-state deep copy
    assert!(generated.contains("match &self.state"), "borrowed match");
    // 4. State enum has all variants
    assert!(generated.contains("Empty,"));
    assert!(generated.contains("HasItems {"));
    assert!(generated.contains("CheckedOut {"));
    // 5. Transition method exists with correct signature
    assert!(generated.contains("pub fn checkout(&mut self"));
    // 6. Effect trait exists
    assert!(generated.contains("pub trait CartEffects"));
    assert!(generated.contains("fn compute_receipt(&self, items: &[Item]) -> String"));
    // 7. Effects are called with references
    assert!(generated.contains("effects.compute_receipt(&"));
}

#[test]
fn tuple_types_parse_and_codegen() {
    let source = r#"
type PairHolder {
    pair: (String, i64),
}
"#;

    let program = parse_program(source).expect("source should parse");
    let generated = RustCodegen::new().generate(&program);

    assert!(generated.contains("pub pair: (String, i64),"));
}

#[test]
fn test_implicit_ctx_rewrite() {
    // Implicit ctx: handler body uses ctx.field without declaring ctx as a parameter
    let source = r#"
type Config {
    name: String,
    count: i64,
}
machine Pipeline {
    state Waiting(config: Config)
    state Running(config: Config)
    state Done(name: String)

    transition start: Waiting -> Running
    transition finish: Running -> Done

    effect log_start(name: String) -> bool

    on start() {
        goto Running(ctx.config);
    }

    on finish() {
        perform log_start(ctx.config.name);
        goto Done(ctx.config.name);
    }
}
"#;
    let program = parse_program(source).expect("should parse");
    let generated = RustCodegen::new().generate(&program);

    // ctx.field must be rewritten even without explicit ctx param
    assert!(
        !generated.contains("ctx."),
        "implicit ctx references should be rewritten"
    );

    // Single-level: ctx.config → config, emitted as field-init shorthand
    // rather than `config: config`, which trips clippy::redundant_field_names.
    assert!(
        generated.contains("PipelineState::Running { config }"),
        "ctx.config in goto should become shorthand `config`, got:\n{generated}"
    );

    // Nested: ctx.config.name → config.name
    assert!(
        generated.contains("config.name"),
        "ctx.config.name should become config.name"
    );

    // No ctx parameter in method signatures
    assert!(
        !generated.contains("ctx:"),
        "no ctx param in method signatures"
    );
}

#[test]
fn test_effect_string_param_generates_str_ref() {
    let source = r#"
machine Notifier {
    state Idle
    state Done
    transition notify: Idle -> Done
    effect send_email(to: String, subject: String) -> bool
    async effect send_sms(number: String) -> bool
    effect log_count(count: i64) -> bool
    on notify() {
        perform send_email("a", "b");
        goto Done;
    }
}
"#;
    let program = parse_program(source).expect("should parse");
    let generated = RustCodegen::new().generate(&program);

    // String params in effect trait should generate &str, not &String
    assert!(
        generated.contains("to: &str"),
        "String param should be &str"
    );
    assert!(
        generated.contains("subject: &str"),
        "String param should be &str"
    );
    assert!(
        generated.contains("number: &str"),
        "async String param should be &str"
    );

    // Copy types should be passed by value, not reference
    assert!(
        generated.contains("count: i64"),
        "i64 param should be by value"
    );
    assert!(
        !generated.contains("count: &i64"),
        "i64 should not be by reference"
    );

    // No &String in effect trait signatures
    assert!(
        !generated.contains("&String"),
        "should not have &String in effect trait"
    );
}

#[test]
fn test_unused_state_fields_prefixed_with_underscore() {
    let source = r#"
type Config { name: String, retries: i64 }
machine Pipeline {
    state Running(config: Config, attempt: i64, tag: String)
    state Done(msg: String)
    transition finish: Running -> Done
    on finish(ctx: FinishCtx) {
        goto Done(ctx.config.name);
    }
}
"#;
    let program = parse_program(source).expect("should parse");
    let generated = RustCodegen::new().generate(&program);

    // config is used (ctx.config.name → config.name), so no underscore
    assert!(
        generated.contains("Running { config,"),
        "used field should not have underscore prefix: {generated}"
    );
    // attempt and tag are unused, so should be prefixed
    assert!(
        generated.contains("_attempt"),
        "unused field 'attempt' should have underscore prefix: {generated}"
    );
    assert!(
        generated.contains("_tag"),
        "unused field 'tag' should have underscore prefix: {generated}"
    );
}

// ─── mutation-testing survivors ────────────────────────────────────────────
//
// With indent-bookkeeping noise excluded (.cargo/mutants.toml), a shard of
// codegen.rs left these as the only survivors. Three are lookup predicates —
// `find(|x| x.name == wanted)` — which substring assertions cannot catch,
// because the emitted output still contains the expected text; it is just
// resolved against the wrong declaration. Each test below is built so the
// *wrong* declaration would be found first.

/// codegen.rs: `if uses_effects && !effects.is_empty()`.
///
/// Mutated to `||`, a machine that declares an effect no handler ever performs
/// gains an `effects:` parameter nothing uses — an unused-variable warning in
/// any consumer building with -D warnings.
#[test]
fn declared_but_unperformed_effect_adds_no_effects_param() {
    let source = r#"
machine Quiet {
    state Idle
    state Done

    transition go: Idle -> Done

    effect never_called(a: String) -> bool

    on go() {
        goto Done;
    }
}
"#;
    let program = parse_program(source).expect("source should parse");
    let generated = RustCodegen::new().generate(&program);

    assert!(
        generated.contains("pub fn go(&mut self) -> Result<(), QuietError>"),
        "handler that performs nothing must take no effects param, got:\n{generated}"
    );
}

/// codegen.rs: `states.iter().find(|s| &s.name == first_target)` on the
/// no-handler default-transition path.
///
/// Mutated to `!=`, the first *non*-target state is found. Here that is the
/// fieldless `Idle`, so the emitter would think the target needs no fields and
/// emit a bare variant assignment for a state that actually carries two.
#[test]
fn handlerless_transition_to_state_with_fields_is_not_auto_initialized() {
    let source = r#"
machine Auto {
    state Idle
    state Done(a: String, b: i64)

    transition go: Idle -> Done
}
"#;
    let program = parse_program(source).expect("source should parse");
    let generated = RustCodegen::new().generate(&program);

    assert!(
        generated.contains("// Cannot auto-transition to Done - requires fields"),
        "target state's own fields must decide this, not another state's, got:\n{generated}"
    );
    assert!(
        !generated.contains("self.state = AutoState::Done;"),
        "must not emit a fieldless assignment for a state carrying fields, got:\n{generated}"
    );
}

/// codegen.rs: `cond.starts_with('(') && cond.ends_with(')')` strips redundant
/// outer parens from an if-condition.
///
/// Mutated to `||` this strips when only one side matches. A binop condition
/// renders as `(..)` so both hold and the mutant is invisible — but a
/// call-shaped condition ends with `)` without starting with `(`, and would
/// have its first and last characters shaved off into malformed Rust.
#[test]
fn call_shaped_if_condition_keeps_its_parens() {
    let source = r#"
machine Gate {
    state Idle
    state Open
    state Shut

    transition check: Idle -> Open | Shut

    effect is_ready() -> bool

    on check() {
        if perform is_ready() {
            goto Open;
        } else {
            goto Shut;
        }
    }
}
"#;
    let program = parse_program(source).expect("source should parse");
    let generated = RustCodegen::new().generate(&program);

    assert!(
        generated.contains("if effects.is_ready() {"),
        "a call-shaped condition must survive paren stripping intact, got:\n{generated}"
    );
}

/// codegen.rs: `effects.iter().find(|e| e.name == *effect)` decides whether
/// each argument is passed by value or by reference.
///
/// Mutated to `!=`, the first *other* effect is found. `takes_copy` is declared
/// first and takes an i64, so a wrong lookup would treat `takes_owned`'s String
/// argument as Copy and drop the `&`.
#[test]
fn perform_arg_borrowing_uses_the_matching_effect_declaration() {
    let source = r#"
machine Args {
    state Idle(label: String)
    state Done(out: String)

    transition go: Idle -> Done

    effect takes_copy(n: i64) -> String
    effect takes_owned(s: String) -> String

    on go(ctx: GoCtx) {
        perform takes_owned(ctx.label);
        goto Done(ctx.label);
    }
}
"#;
    let program = parse_program(source).expect("source should parse");
    let generated = RustCodegen::new().generate(&program);

    // Deliberately a *bare* perform: the borrowing decision for a let-bound
    // perform is made in `expr_to_rust`, a different path that this lookup
    // does not feed.
    assert!(
        generated.contains("effects.takes_owned(&label);"),
        "a String arg must be borrowed, resolved against its own effect decl, got:\n{generated}"
    );
}

/// codegen.rs: `channels.iter().find(|c| c.name == *channel)` selects the
/// channel's mode, which picks `send` vs `try_send`.
///
/// Mutated to `!=`, the first *other* channel is found. `Broadcast` is declared
/// first, so a wrong lookup would emit a broadcast `send` for an mpsc channel.
#[test]
fn send_uses_the_matching_channels_mode() {
    let source = r#"
channel Broadcast: String (capacity: 8, mode: broadcast)
channel Queue: String (capacity: 8, mode: mpsc)

machine Emitter(sends Queue) {
    state Idle
    state Done

    transition go: Idle -> Done

    on go() {
        send Queue("x");
        goto Done;
    }
}
"#;
    let program = parse_program(source).expect("source should parse");
    let generated = RustCodegen::new().generate(&program);

    // Pinned to the send *statement* by its literal argument. A bare
    // `queue_tx.try_send(` also appears in the generated `send_queue` helper,
    // which a different code path emits — asserting on that substring alone
    // passes even when the statement itself is emitted wrongly.
    assert!(
        generated.contains(r#"queue_tx.try_send("x".to_string());"#),
        "an mpsc channel must use try_send, resolved against its own decl, got:\n{generated}"
    );
    // The same lookup, separately, picks the transition method's channel
    // parameter type.
    assert!(
        generated.contains("pub fn go(&mut self, queue_tx: &tokio::sync::mpsc::Sender<String>)"),
        "the channel param type must come from the matching channel, got:\n{generated}"
    );
}
