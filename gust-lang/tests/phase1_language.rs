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
