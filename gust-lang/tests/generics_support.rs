use gust_lang::{GoCodegen, RustCodegen, parse_program_with_errors};

#[test]
fn parses_machine_generic_params_with_bounds() {
    let source = r#"
machine Cache<T: Clone + Send, U> {
    state Empty
}
"#;

    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    let machine = &program.machines[0];
    assert_eq!(machine.name, "Cache");
    assert_eq!(machine.generic_params.len(), 2);
    assert_eq!(machine.generic_params[0].name, "T");
    assert_eq!(machine.generic_params[0].bounds, vec!["Clone", "Send"]);
    assert_eq!(machine.generic_params[1].name, "U");
    assert!(machine.generic_params[1].bounds.is_empty());
}

#[test]
fn rust_codegen_emits_machine_generics() {
    let source = r#"
machine Boxed<T: Clone> {
    state Ready(value: T)
}
"#;

    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    let generated = RustCodegen::new().generate(&program);
    assert!(generated.contains("pub enum BoxedState<T: Clone>"));
    assert!(generated.contains("pub struct Boxed<T: Clone>"));
    assert!(generated.contains("pub state: BoxedState<T>"));
    assert!(generated.contains("impl<T: Clone> Boxed<T>"));
}

#[test]
fn go_codegen_emits_machine_generics() {
    let source = r#"
machine Queue<T> {
    state Idle(item: T)
}
"#;

    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    let generated = GoCodegen::new().generate(&program, "testpkg");
    assert!(generated.contains("type QueueState[T any] int"));
    assert!(generated.contains("type QueueIdleData[T any] struct {"));
    assert!(generated.contains("type Queue[T any] struct {"));
    assert!(generated.contains("State QueueState[T]"));
    assert!(generated.contains("func NewQueue[T any]("));
}
