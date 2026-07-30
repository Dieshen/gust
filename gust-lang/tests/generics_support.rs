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
    // The impl carries an extra `Debug` bound the type declarations do not: the
    // invalid-transition arm formats the state with `{:?}`, and the derived
    // `Debug` on a generic state enum only applies when `T: Debug`. Bounding the
    // impl rather than the struct keeps `Serialize`/`Deserialize` unconstrained.
    assert!(generated.contains("impl<T: Clone + core::fmt::Debug> Boxed<T>"));
}

#[test]
fn generic_type_parameter_is_not_mistaken_for_a_ctx_accessor() {
    // A handler parameter typed by the machine's own type parameter used to look
    // like an unknown type, which is how ctx accessors are detected. The
    // parameter was dropped from the generated signature and every reference to
    // it was left undefined — in both backends.
    let source = r#"
machine Holder<T> {
    state Empty
    state Full(value: T)

    transition put: Empty -> Full

    on put(value: T) {
        goto Full(value);
    }
}
"#;

    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");

    let rust = RustCodegen::new().generate(&program);
    assert!(
        rust.contains("pub fn put(&mut self, value: T)"),
        "handler param should survive into the Rust signature:\n{rust}"
    );

    let go = GoCodegen::new().generate(&program, "testpkg");
    assert!(
        go.contains("func (m *Holder[T]) Put(value T) error"),
        "handler param should survive into the Go signature:\n{go}"
    );
    // The state-data struct is generic, so the literal needs type arguments.
    assert!(
        go.contains("m.FullData = &HolderFullData[T]{"),
        "generic state data must be instantiated:\n{go}"
    );
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
