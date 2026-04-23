use gust_lang::{GoCodegen, RustCodegen, parse_program, parse_program_with_errors};

// ---------------------------------------------------------------------------
// 1. Minimal machine (Rust) — single state, no transitions
// ---------------------------------------------------------------------------
#[test]
fn minimal_machine_rust_generates_enum_and_struct() {
    let source = r#"
machine Minimal {
    state Only
}
"#;
    let program = parse_program(source).expect("should parse");
    let generated = RustCodegen::new().generate(&program);

    assert!(
        generated.contains("pub enum MinimalState"),
        "should generate state enum"
    );
    assert!(
        generated.contains("Only,"),
        "enum should contain Only variant"
    );
    assert!(
        generated.contains("pub struct Minimal"),
        "should generate machine struct"
    );
}

// ---------------------------------------------------------------------------
// 2. Minimal machine (Go) — single state, no transitions
// ---------------------------------------------------------------------------
#[test]
fn minimal_machine_go_generates_struct_and_constants() {
    let source = r#"
machine Minimal {
    state Only
}
"#;
    let program = parse_program(source).expect("should parse");
    let generated = GoCodegen::new().generate(&program, "edge");

    assert!(
        generated.contains("package edge"),
        "Go output should use the given package name"
    );
    assert!(
        generated.contains("Minimal"),
        "Go output should contain machine name"
    );
    assert!(
        generated.contains("Only"),
        "Go output should contain state name"
    );
}

// ---------------------------------------------------------------------------
// 3. Multiple target states — A -> B | C | D
// ---------------------------------------------------------------------------
#[test]
fn multiple_target_states_generates_all_branches_rust() {
    let source = r#"
machine Router {
    state Incoming
    state RouteA
    state RouteB
    state RouteC

    transition dispatch: Incoming -> RouteA | RouteB | RouteC

    on dispatch() {
        goto RouteA();
    }
}
"#;
    let program = parse_program(source).expect("should parse");
    let generated = RustCodegen::new().generate(&program);

    assert!(
        generated.contains("RouteA,"),
        "should contain RouteA variant"
    );
    assert!(
        generated.contains("RouteB,"),
        "should contain RouteB variant"
    );
    assert!(
        generated.contains("RouteC,"),
        "should contain RouteC variant"
    );
    assert!(
        generated.contains("pub fn dispatch("),
        "should generate dispatch method"
    );
}

// ---------------------------------------------------------------------------
// 4. Effect with complex return type
// ---------------------------------------------------------------------------
#[test]
fn effect_with_complex_return_type_rust() {
    let source = r#"
machine Fetcher {
    state Idle
    state Done(data: Vec<String>)

    transition fetch: Idle -> Done

    effect load_data() -> Vec<String>

    on fetch() {
        let data = perform load_data();
        goto Done(data);
    }
}
"#;
    let program = parse_program(source).expect("should parse");
    let generated = RustCodegen::new().generate(&program);

    assert!(
        generated.contains("fn load_data(&self) -> Vec<String>"),
        "effect trait should have Vec<String> return type"
    );
    assert!(
        generated.contains("pub trait FetcherEffects"),
        "should generate effects trait"
    );
}

// ---------------------------------------------------------------------------
// 5. Multiple effects per machine
// ---------------------------------------------------------------------------
#[test]
fn multiple_effects_generates_complete_trait() {
    let source = r#"
machine Worker {
    state Idle(url: String)
    state Done(result: String)

    transition run: Idle -> Done

    effect fetch_url(url: String) -> String
    effect validate(data: String) -> bool
    effect store(key: String, value: String) -> bool

    on run(ctx: RunCtx) {
        let raw = perform fetch_url(ctx.url);
        let ok = perform validate(raw);
        goto Done(raw);
    }
}
"#;
    let program = parse_program(source).expect("should parse");
    let generated = RustCodegen::new().generate(&program);

    assert!(
        generated.contains("pub trait WorkerEffects"),
        "should generate effects trait"
    );
    assert!(
        generated.contains("fn fetch_url("),
        "trait should include fetch_url"
    );
    assert!(
        generated.contains("fn validate("),
        "trait should include validate"
    );
    assert!(
        generated.contains("fn store("),
        "trait should include store"
    );
}

// ---------------------------------------------------------------------------
// 6. State with many fields
// ---------------------------------------------------------------------------
#[test]
fn state_with_many_fields_generates_correct_struct() {
    let source = r#"
machine Record {
    state Draft(
        title: String,
        author: String,
        version: i64,
        tags: Vec<String>,
        published: bool
    )
    state Saved
    transition save: Draft -> Saved
    on save() {
        goto Saved();
    }
}
"#;
    let program = parse_program(source).expect("should parse");
    let generated = RustCodegen::new().generate(&program);

    assert!(generated.contains("title:"), "should contain title field");
    assert!(generated.contains("author:"), "should contain author field");
    assert!(
        generated.contains("version:"),
        "should contain version field"
    );
    assert!(generated.contains("tags:"), "should contain tags field");
    assert!(
        generated.contains("published:"),
        "should contain published field"
    );
}

// ---------------------------------------------------------------------------
// 7. Enum type usage in state fields
// ---------------------------------------------------------------------------
#[test]
fn enum_type_in_state_field_rust() {
    let source = r#"
enum Priority {
    Low,
    Medium,
    High,
}

machine Task {
    state Pending(priority: Priority)
    state Complete

    transition finish: Pending -> Complete
    on finish() {
        goto Complete();
    }
}
"#;
    let program = parse_program(source).expect("should parse");
    let generated = RustCodegen::new().generate(&program);

    assert!(
        generated.contains("pub enum Priority"),
        "should generate Priority enum"
    );
    assert!(generated.contains("Low,"), "enum should contain Low");
    assert!(generated.contains("Medium,"), "enum should contain Medium");
    assert!(generated.contains("High,"), "enum should contain High");
    assert!(
        generated.contains("priority: Priority"),
        "state field should reference custom enum type"
    );
}

// ---------------------------------------------------------------------------
// 8. Multiple machines in one program
// ---------------------------------------------------------------------------
#[test]
fn multiple_machines_generate_independent_code() {
    let source = r#"
machine Alpha {
    state On
    state Off
    transition toggle: On -> Off
    on toggle() {
        goto Off();
    }
}

machine Beta {
    state Running
    state Stopped
    transition stop: Running -> Stopped
    on stop() {
        goto Stopped();
    }
}
"#;
    let program = parse_program(source).expect("should parse");
    let generated = RustCodegen::new().generate(&program);

    assert!(
        generated.contains("pub struct Alpha"),
        "should generate Alpha struct"
    );
    assert!(
        generated.contains("pub enum AlphaState"),
        "should generate AlphaState enum"
    );
    assert!(
        generated.contains("pub struct Beta"),
        "should generate Beta struct"
    );
    assert!(
        generated.contains("pub enum BetaState"),
        "should generate BetaState enum"
    );
}

// ---------------------------------------------------------------------------
// 9. Go package naming
// ---------------------------------------------------------------------------
#[test]
fn go_package_name_appears_in_output() {
    let source = r#"
machine Simple {
    state Ready
}
"#;
    let program = parse_program(source).expect("should parse");

    let out_a = GoCodegen::new().generate(&program, "myservice");
    assert!(
        out_a.contains("package myservice"),
        "should use 'myservice' as package"
    );

    let out_b = GoCodegen::new().generate(&program, "another_pkg");
    assert!(
        out_b.contains("package another_pkg"),
        "should use 'another_pkg' as package"
    );
}

// ---------------------------------------------------------------------------
// 10. Serde derives on Rust state types
// ---------------------------------------------------------------------------
#[test]
fn rust_codegen_derives_serde_on_state_types() {
    let source = r#"
machine Ledger {
    state Open(balance: i64)
    state Closed
}
"#;
    let program = parse_program(source).expect("should parse");
    let generated = RustCodegen::new().generate(&program);

    assert!(
        generated.contains("use serde::{Serialize, Deserialize};"),
        "should import serde"
    );
    assert!(
        generated.contains("Serialize, Deserialize"),
        "should derive serde traits"
    );
}

// ---------------------------------------------------------------------------
// 11. JSON struct tags in Go output
// ---------------------------------------------------------------------------
#[test]
fn go_codegen_emits_json_struct_tags() {
    let source = r#"
machine Inventory {
    state Stocked(item_name: String, quantity: i64)
    state Empty
}
"#;
    let program = parse_program(source).expect("should parse");
    let generated = GoCodegen::new().generate(&program, "warehouse");

    assert!(
        generated.contains("`json:\""),
        "Go output should contain json struct tags"
    );
}

// ---------------------------------------------------------------------------
// 12. Guard conditions — if/else in handlers
// ---------------------------------------------------------------------------
#[test]
fn guard_condition_generates_if_else_rust() {
    let source = r#"
machine Gate {
    state Pending(value: i64)
    state Accepted
    state Rejected

    transition check: Pending -> Accepted | Rejected

    on check(ctx: CheckCtx) {
        if ctx.value > 10 {
            goto Accepted();
        } else {
            goto Rejected();
        }
    }
}
"#;
    let program = parse_program(source).expect("should parse");
    let generated = RustCodegen::new().generate(&program);

    assert!(
        generated.contains("if value > 10"),
        "should generate if condition (ctx rewritten)"
    );
    assert!(
        generated.contains("Accepted"),
        "should contain Accepted state"
    );
    assert!(
        generated.contains("Rejected"),
        "should contain Rejected state"
    );
}

// ---------------------------------------------------------------------------
// 13. Multiple machines in Go — both appear with correct package
// ---------------------------------------------------------------------------
#[test]
fn multiple_machines_go_codegen() {
    let source = r#"
machine First {
    state A
}

machine Second {
    state B
}
"#;
    let program = parse_program(source).expect("should parse");
    let generated = GoCodegen::new().generate(&program, "multi");

    assert!(
        generated.contains("package multi"),
        "should use correct package"
    );
    assert!(generated.contains("First"), "should contain First machine");
    assert!(
        generated.contains("Second"),
        "should contain Second machine"
    );
}

// ---------------------------------------------------------------------------
// 14. parse_program_with_errors also works for codegen
// ---------------------------------------------------------------------------
#[test]
fn parse_with_errors_feeds_codegen_correctly() {
    let source = r#"
machine Beacon {
    state On
    state Off
    transition toggle: On -> Off
    on toggle() {
        goto Off();
    }
}
"#;
    let program =
        parse_program_with_errors(source, "edge_test.gu").expect("should parse with errors API");
    let rust = RustCodegen::new().generate(&program);
    let go = GoCodegen::new().generate(&program, "beacon");

    assert!(
        rust.contains("Generated by Gust compiler"),
        "Rust output should have header"
    );
    assert!(
        go.contains("Code generated by Gust compiler"),
        "Go output should have header"
    );
}
