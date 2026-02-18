use gust_lang::{parse_program, RustCodegen};

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

    assert!(generated.contains("async fn process(&self) -> String;"));
    assert!(generated.contains("pub async fn charge("));
    assert!(generated.contains("effects.process().await"));
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
    assert!(!generated.contains("ctx.order"), "ctx.field should be rewritten to field");
    assert!(!generated.contains("ctx: ProcessCtx"), "ctx param should not appear in method sig");

    // Should use clone match for owned destructuring
    assert!(generated.contains("match self.state.clone()"), "should clone state for owned access");

    // Bug 5: perform args must be passed by reference
    assert!(generated.contains("effects.calculate_total(&"), "perform args should be references");

    // Bug 4: no unnecessary parens in if condition
    assert!(!generated.contains("if (total"), "if condition should not have outer parens");
    assert!(generated.contains("if total.cents > 0"), "if condition should be bare");
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
    assert!(generated.contains("Stage::Build"), "enum path should appear in generated Rust");
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
    assert!(!generated.contains("CheckoutCtx"), "no phantom types in sigs");
    // 3. Proper match form
    assert!(generated.contains("match self.state.clone()"), "owned match");
    // 4. State enum has all variants
    assert!(generated.contains("Empty,"));
    assert!(generated.contains("HasItems {"));
    assert!(generated.contains("CheckedOut {"));
    // 5. Transition method exists with correct signature
    assert!(generated.contains("pub fn checkout(&mut self"));
    // 6. Effect trait exists
    assert!(generated.contains("pub trait CartEffects"));
    assert!(generated.contains("fn compute_receipt(&self, items: &Vec<Item>) -> String"));
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
