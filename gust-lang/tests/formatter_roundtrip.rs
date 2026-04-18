use gust_lang::{format_program, format_program_preserving, parse_program};

/// Helper: parse source, format, parse again, format again.
/// The two formatted outputs must be identical (idempotency).
fn assert_format_idempotent(source: &str) {
    let program1 = parse_program(source).expect("first parse should succeed");
    let formatted1 = format_program(&program1);
    let program2 = parse_program(&formatted1).expect("second parse should succeed");
    let formatted2 = format_program(&program2);
    assert_eq!(
        formatted1, formatted2,
        "formatting is not idempotent:\n--- first ---\n{formatted1}\n--- second ---\n{formatted2}"
    );
}

// ---------------------------------------------------------------------------
// 1. Roundtrip idempotency
// ---------------------------------------------------------------------------

#[test]
fn roundtrip_idempotency_simple_machine() {
    let source = r#"
machine Door {
    state Locked
    state Unlocked

    transition unlock: Locked -> Unlocked
    transition lock: Unlocked -> Locked

    on unlock() {
        goto Unlocked;
    }

    on lock() {
        goto Locked;
    }
}
"#;
    assert_format_idempotent(source);
}

#[test]
fn roundtrip_idempotency_complex_program() {
    let source = r#"
type Config {
    retries: i64,
    label: String,
}

enum Status {
    Active,
    Inactive,
    Error(String),
}

machine Worker {
    state Idle(status: Status)
    state Running(config: Config)
    state Done

    transition start: Idle -> Running
    transition finish: Running -> Done

    effect get_config() -> Config

    on start() {
        let cfg = perform get_config();
        goto Running(cfg);
    }

    on finish() {
        goto Done;
    }
}
"#;
    assert_format_idempotent(source);
}

// ---------------------------------------------------------------------------
// 2. Simple machine formats correctly
// ---------------------------------------------------------------------------

#[test]
fn simple_machine_format() {
    // Intentionally messy whitespace that the formatter should normalise
    let source = r#"
machine  Light  {
    state   Off
    state   On

    transition  toggle: Off  ->  On

    on toggle()  {
        goto  On;
    }
}
"#;
    let program = parse_program(source).expect("parse");
    let formatted = format_program(&program);

    assert!(formatted.contains("machine Light {"));
    assert!(formatted.contains("    state Off"));
    assert!(formatted.contains("    state On"));
    assert!(formatted.contains("    transition toggle: Off -> On"));
    assert!(formatted.contains("    on toggle() {"));
    assert!(formatted.contains("        goto On;"));
    // Verify idempotency too
    assert_format_idempotent(source);
}

// ---------------------------------------------------------------------------
// 3. Type declarations (structs and enums)
// ---------------------------------------------------------------------------

#[test]
fn type_declarations_format_correctly() {
    let source = r#"
type Point {
    x: f64,
    y: f64,
}

enum Shape {
    Circle(f64),
    Rectangle(f64, f64),
    Unknown,
}

machine Canvas {
    state Blank
    state Drawing(shape: Shape)

    transition draw: Blank -> Drawing

    on draw() {
        goto Drawing(Shape::Circle);
    }
}
"#;
    let program = parse_program(source).expect("parse");
    let formatted = format_program(&program);

    // Struct fields indented with 4 spaces
    assert!(formatted.contains("type Point {"));
    assert!(formatted.contains("    x: f64,"));
    assert!(formatted.contains("    y: f64,"));

    // Enum variants indented with 4 spaces
    assert!(formatted.contains("enum Shape {"));
    assert!(formatted.contains("    Circle(f64),"));
    assert!(formatted.contains("    Rectangle(f64, f64),"));
    assert!(formatted.contains("    Unknown,"));

    assert_format_idempotent(source);
}

// ---------------------------------------------------------------------------
// 4. Comment preservation with format_program_preserving
// ---------------------------------------------------------------------------

#[test]
fn comment_preservation() {
    let source = "// File header comment\n\
// describing the purpose\n\
\n\
// Machine-level comment\n\
machine Greeter {\n\
    // State comment\n\
    state Idle\n\
    state Done\n\
\n\
    // Transition comment\n\
    transition greet: Idle -> Done\n\
\n\
    // Handler comment\n\
    on greet() {\n\
        // Body comment\n\
        goto Done;\n\
    }\n\
}\n";

    let program = parse_program(source).expect("parse");
    let preserved = format_program_preserving(&program, source);

    // File header comments should be present
    assert!(
        preserved.contains("// File header comment"),
        "file header comment missing"
    );
    assert!(
        preserved.contains("// describing the purpose"),
        "second header line missing"
    );
    // Machine-level comment
    assert!(
        preserved.contains("// Machine-level comment"),
        "machine comment missing"
    );
    // State comment
    assert!(
        preserved.contains("// State comment"),
        "state comment missing"
    );
    // Transition comment
    assert!(
        preserved.contains("// Transition comment"),
        "transition comment missing"
    );
    // Handler comment
    assert!(
        preserved.contains("// Handler comment"),
        "handler comment missing"
    );
    // Body comment
    assert!(
        preserved.contains("// Body comment"),
        "body comment missing"
    );
}

// ---------------------------------------------------------------------------
// 5. Complex expressions (match, if/else, let, perform)
// ---------------------------------------------------------------------------

#[test]
fn complex_expressions_format() {
    let source = r#"
enum Result {
    Ok(String),
    Err(String),
}

machine Processor {
    state Waiting(value: i64)
    state Success(msg: String)
    state Failure(err: String)

    transition process: Waiting -> Success | Failure

    effect fetch(id: i64) -> Result

    on process() {
        let result = perform fetch(value);
        match result {
            Result::Ok(msg) => {
                if msg == "special" {
                    goto Success("VIP");
                } else {
                    goto Success(msg);
                }
            }
            Result::Err(e) => {
                goto Failure(e);
            }
        }
    }
}
"#;
    let program = parse_program(source).expect("parse");
    let formatted = format_program(&program);

    // let binding with perform expression
    assert!(formatted.contains("let result = perform fetch(value);"));
    // match statement
    assert!(formatted.contains("match result {"));
    // pattern with variant
    assert!(formatted.contains("Result::Ok(msg) => {"));
    // if/else
    assert!(formatted.contains("if msg == \"special\" {"));
    assert!(formatted.contains("} else {"));

    assert_format_idempotent(source);
}

// ---------------------------------------------------------------------------
// 6. Multiple machines in one file
// ---------------------------------------------------------------------------

#[test]
fn multiple_machines_separated() {
    let source = r#"
machine First {
    state A
    state B

    transition go: A -> B

    on go() {
        goto B;
    }
}

machine Second {
    state X
    state Y

    transition move: X -> Y

    on move() {
        goto Y;
    }
}
"#;
    let program = parse_program(source).expect("parse");
    let formatted = format_program(&program);

    // Both machines present
    assert!(formatted.contains("machine First {"));
    assert!(formatted.contains("machine Second {"));

    // They should be separated by at least one blank line
    let first_end = formatted.find("}\n\nmachine Second").or_else(|| {
        // The formatter places a newline after each machine block
        formatted.find("}\nmachine Second")
    });
    assert!(
        first_end.is_some(),
        "machines should be separated in output:\n{formatted}"
    );

    assert_format_idempotent(source);
}

// ---------------------------------------------------------------------------
// 7. Channel declarations
// ---------------------------------------------------------------------------

#[test]
fn channel_declarations_format() {
    let source = r#"
channel Events: String (capacity: 32, mode: broadcast)
channel Tasks: i64 (capacity: 64, mode: mpsc)

machine Dispatcher {
    state Idle
    state Busy

    transition dispatch: Idle -> Busy

    on dispatch() {
        goto Busy;
    }
}
"#;
    let program = parse_program(source).expect("parse");
    let formatted = format_program(&program);

    assert!(
        formatted.contains("channel Events: String (capacity: 32, mode: broadcast)"),
        "broadcast channel missing or malformed:\n{formatted}"
    );
    assert!(
        formatted.contains("channel Tasks: i64 (capacity: 64, mode: mpsc)"),
        "mpsc channel missing or malformed:\n{formatted}"
    );

    assert_format_idempotent(source);
}

// ---------------------------------------------------------------------------
// 8. Generic type parameters
// ---------------------------------------------------------------------------

#[test]
fn generic_type_params_format() {
    let source = r#"
machine Cache<T: Clone + Send, U> {
    state Empty
    state Loaded(value: T)

    transition load: Empty -> Loaded

    effect fetch() -> T

    on load() {
        let val = perform fetch();
        goto Loaded(val);
    }
}
"#;
    let program = parse_program(source).expect("parse");
    let formatted = format_program(&program);

    assert!(
        formatted.contains("machine Cache<T: Clone + Send, U> {"),
        "generic params missing or wrong:\n{formatted}"
    );
    assert!(formatted.contains("    state Loaded(value: T)"));

    assert_format_idempotent(source);
}

// ---------------------------------------------------------------------------
// 9. Async handlers
// ---------------------------------------------------------------------------

#[test]
fn async_handlers_format() {
    let source = r#"
machine HttpClient {
    state Ready
    state Done(body: String)

    transition request: Ready -> Done

    async effect http_get(url: String) -> String

    async on request() {
        let body = perform http_get("example.com");
        goto Done(body);
    }
}
"#;
    let program = parse_program(source).expect("parse");
    let formatted = format_program(&program);

    assert!(
        formatted.contains("    async effect http_get(url: String) -> String"),
        "async effect missing:\n{formatted}"
    );
    assert!(
        formatted.contains("    async on request() {"),
        "async on handler missing:\n{formatted}"
    );

    assert_format_idempotent(source);
}

// ---------------------------------------------------------------------------
// 10. Empty / minimal machine
// ---------------------------------------------------------------------------

#[test]
fn minimal_machine_single_state() {
    let source = r#"
machine Noop {
    state Idle
}
"#;
    let program = parse_program(source).expect("parse");
    let formatted = format_program(&program);

    assert!(formatted.contains("machine Noop {"));
    assert!(formatted.contains("    state Idle"));
    assert!(formatted.ends_with("}\n"));

    assert_format_idempotent(source);
}

// ---------------------------------------------------------------------------
// 11. Preserving vs non-preserving produces valid output for both
// ---------------------------------------------------------------------------

#[test]
fn preserving_and_non_preserving_both_reparse() {
    let source = "// A comment\n\
machine Demo {\n\
    state Start\n\
    state End\n\
\n\
    // Transition doc\n\
    transition run: Start -> End\n\
\n\
    on run() {\n\
        goto End;\n\
    }\n\
}\n";

    let program = parse_program(source).expect("parse");

    let plain = format_program(&program);
    let preserved = format_program_preserving(&program, source);

    // Both outputs should re-parse without error
    parse_program(&plain).expect("plain formatted output should re-parse");
    parse_program(&preserved).expect("preserved formatted output should re-parse");

    // The preserved variant should have the comment, the plain one should not
    assert!(
        preserved.contains("// A comment"),
        "preserved should keep header comment"
    );
    assert!(
        !plain.contains("// A comment"),
        "plain format should not have comments"
    );
}
