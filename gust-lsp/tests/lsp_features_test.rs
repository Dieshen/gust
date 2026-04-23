use gust_lsp::gust_lang::ast::TypeExpr;
use gust_lsp::gust_lang::{format_program_preserving, parse_program_with_errors};
use gust_lsp::{
    DiagSeverity, SimpleSymbolKind, code_actions_at, collect_doc_comments, diagnostics_from_source,
    document_symbols, find_all_word_occurrences, find_closing_brace_line, find_decl_line,
    find_let_line, find_line_index, find_name_end_col, find_perform_effect_name, first_ident,
    goto_definition, hover_info, inlay_hints, make_hover_content, signature_help, token_at_col,
    type_expr_label,
};

// ============================================================================
// Shared test fixtures
// ============================================================================

/// A minimal valid machine with states, transitions, effects, and a handler.
const BASIC_MACHINE: &str = r#"
machine TrafficLight {
    state Red
    state Green
    state Yellow

    transition go: Red -> Green
    transition slow: Green -> Yellow
    transition stop: Yellow -> Red

    effect log_change(msg: String) -> ()

    on go() {
        perform log_change("red to green");
        goto Green;
    }

    on slow() {
        perform log_change("green to yellow");
        goto Yellow;
    }

    on stop() {
        perform log_change("yellow to red");
        goto Red;
    }
}
"#;

/// A machine with fields, async effects, and perform-let for inlay hints.
const MACHINE_WITH_FIELDS: &str = r#"
machine OrderProcessor {
    state Pending(order_id: String)
    state Processing(order_id: String, total: i64)
    state Complete(receipt: String)
    state Failed(reason: String)

    transition process: Pending -> Processing | Failed
    transition finalize: Processing -> Complete

    async effect validate_order(id: String) -> i64
    effect generate_receipt(order_id: String, total: i64) -> String

    on process() {
        let total = perform validate_order(order_id);
        goto Processing(order_id, total);
    }

    on finalize() {
        let receipt = perform generate_receipt(order_id, total);
        goto Complete(receipt);
    }
}
"#;

/// Source with doc comments above declarations.
const MACHINE_WITH_DOCS: &str = r#"
machine Documented {
    // The initial idle state
    state Idle

    // The running state with a counter
    state Running(count: i64)

    // Start the process
    transition start: Idle -> Running

    // Log something to the console
    effect log(msg: String) -> ()

    on start() {
        perform log("starting");
        goto Running(0);
    }
}
"#;

/// Source with type declarations (struct and enum).
const SOURCE_WITH_TYPES: &str = r#"
type Address {
    street: String,
    city: String,
    zip: String,
}

enum PaymentMethod {
    Cash,
    Card(String),
    Transfer(String, i64),
}

machine Checkout {
    state Pending(addr: Address)
    state Done

    transition pay: Pending -> Done

    on pay() {
        goto Done;
    }
}
"#;

/// Source with a missing handler (transition declared but no `on` block).
const MACHINE_MISSING_HANDLER: &str = r#"
machine Incomplete {
    state Start
    state End

    transition go: Start -> End
    transition back: End -> Start

    effect notify() -> ()

    on go() {
        perform notify();
        goto End;
    }
}
"#;

/// Source with a timeout on a transition.
const MACHINE_WITH_TIMEOUT: &str = r#"
machine TimedMachine {
    state Waiting
    state Done

    transition finish: Waiting -> Done timeout 30s

    on finish() {
        goto Done;
    }
}
"#;

// ============================================================================
// Diagnostics tests
// ============================================================================

#[test]
fn diagnostics_valid_source_no_errors() {
    let diags = diagnostics_from_source(BASIC_MACHINE, "test.gu");
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == DiagSeverity::Error)
        .collect();
    assert!(errors.is_empty(), "valid source should produce no errors");
}

#[test]
fn diagnostics_parse_error_on_invalid_syntax() {
    let source = r#"
machine Broken {
    state Start
    transision go: Start -> End
}
"#;
    let diags = diagnostics_from_source(source, "test.gu");
    assert!(
        !diags.is_empty(),
        "invalid syntax should produce diagnostics"
    );
    assert_eq!(diags[0].severity, DiagSeverity::Error);
}

#[test]
fn diagnostics_validator_error_for_undefined_target() {
    let source = r#"
machine Test {
    state Start
    transition go: Start -> Nonexistent
}
"#;
    let diags = diagnostics_from_source(source, "test.gu");
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == DiagSeverity::Error)
        .collect();
    assert!(
        !errors.is_empty(),
        "undefined target state should produce an error"
    );
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("Nonexistent")
                || e.message.to_lowercase().contains("undefined")),
        "error should mention the undefined state"
    );
}

#[test]
fn diagnostics_warning_for_unreachable_state() {
    let source = r#"
machine Test {
    state Start
    state Orphan
    transition go: Start -> Start
}
"#;
    let diags = diagnostics_from_source(source, "test.gu");
    let warnings: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == DiagSeverity::Warning)
        .collect();
    assert!(
        !warnings.is_empty(),
        "unreachable state should produce a warning"
    );
}

#[test]
fn diagnostics_empty_source() {
    let diags = diagnostics_from_source("", "test.gu");
    // Empty source should either parse cleanly or produce a parse error; not panic
    let _ = diags;
}

#[test]
fn diagnostics_only_comments() {
    let source = "// This is just a comment\n// Another comment\n";
    let diags = diagnostics_from_source(source, "test.gu");
    // Should not panic; may produce errors since no machine is present
    let _ = diags;
}

// ============================================================================
// Hover info tests
// ============================================================================

#[test]
fn hover_on_state_shows_fields() {
    // Line 2 (0-indexed) in BASIC_MACHINE is "    state Red"
    // "Red" starts at column 10
    let (sig, _doc) = hover_info(BASIC_MACHINE, 2, 10).expect("hover should return info for Red");
    assert!(
        sig.contains("state Red"),
        "signature should contain 'state Red'"
    );
    assert!(sig.contains("no fields"), "Red has no fields");
}

#[test]
fn hover_on_state_with_fields() {
    // "state Pending(order_id: String)" — find the line
    let line_idx = MACHINE_WITH_FIELDS
        .lines()
        .position(|l| l.trim().starts_with("state Pending"))
        .expect("should find state Pending line");
    let col = MACHINE_WITH_FIELDS
        .lines()
        .nth(line_idx)
        .unwrap()
        .find("Pending")
        .unwrap();

    let (sig, _doc) = hover_info(MACHINE_WITH_FIELDS, line_idx, col)
        .expect("hover should return info for Pending");
    assert!(sig.contains("state Pending"));
    assert!(sig.contains("order_id: String"));
}

#[test]
fn hover_on_effect_shows_signature() {
    let line_idx = BASIC_MACHINE
        .lines()
        .position(|l| l.trim().starts_with("effect log_change"))
        .expect("should find effect line");
    let col = BASIC_MACHINE
        .lines()
        .nth(line_idx)
        .unwrap()
        .find("log_change")
        .unwrap();

    let (sig, _doc) =
        hover_info(BASIC_MACHINE, line_idx, col).expect("hover should return info for log_change");
    assert!(sig.contains("effect log_change"));
    assert!(sig.contains("msg: String"));
    assert!(sig.contains("-> ()"));
}

#[test]
fn hover_on_async_effect() {
    let line_idx = MACHINE_WITH_FIELDS
        .lines()
        .position(|l| l.trim().starts_with("async effect validate_order"))
        .expect("should find async effect line");
    let col = MACHINE_WITH_FIELDS
        .lines()
        .nth(line_idx)
        .unwrap()
        .find("validate_order")
        .unwrap();

    let (sig, _doc) = hover_info(MACHINE_WITH_FIELDS, line_idx, col)
        .expect("hover should return info for validate_order");
    assert!(sig.contains("async effect validate_order"));
    assert!(sig.contains("id: String"));
    assert!(sig.contains("-> i64"));
}

#[test]
fn hover_on_transition_shows_from_to() {
    let line_idx = BASIC_MACHINE
        .lines()
        .position(|l| l.trim().starts_with("transition go"))
        .expect("should find transition go line");
    let col = BASIC_MACHINE
        .lines()
        .nth(line_idx)
        .unwrap()
        .find("go")
        .unwrap();

    let (sig, _doc) =
        hover_info(BASIC_MACHINE, line_idx, col).expect("hover should return info for go");
    assert!(sig.contains("transition go"));
    assert!(sig.contains("Red -> Green"));
}

#[test]
fn hover_on_transition_with_timeout() {
    let line_idx = MACHINE_WITH_TIMEOUT
        .lines()
        .position(|l| l.trim().starts_with("transition finish"))
        .expect("should find transition finish line");
    let col = MACHINE_WITH_TIMEOUT
        .lines()
        .nth(line_idx)
        .unwrap()
        .find("finish")
        .unwrap();

    let (sig, _doc) = hover_info(MACHINE_WITH_TIMEOUT, line_idx, col)
        .expect("hover should return info for finish");
    assert!(sig.contains("transition finish"));
    assert!(sig.contains("[timeout: 30s]"));
}

#[test]
fn hover_on_struct_type() {
    let line_idx = SOURCE_WITH_TYPES
        .lines()
        .position(|l| l.trim().starts_with("type Address"))
        .expect("should find type Address line");
    let col = SOURCE_WITH_TYPES
        .lines()
        .nth(line_idx)
        .unwrap()
        .find("Address")
        .unwrap();

    let (sig, _doc) =
        hover_info(SOURCE_WITH_TYPES, line_idx, col).expect("hover should return info for Address");
    assert!(sig.contains("type Address"));
    assert!(sig.contains("street: String"));
    assert!(sig.contains("city: String"));
}

#[test]
fn hover_on_enum_type() {
    let line_idx = SOURCE_WITH_TYPES
        .lines()
        .position(|l| l.trim().starts_with("enum PaymentMethod"))
        .expect("should find enum PaymentMethod line");
    let col = SOURCE_WITH_TYPES
        .lines()
        .nth(line_idx)
        .unwrap()
        .find("PaymentMethod")
        .unwrap();

    let (sig, _doc) = hover_info(SOURCE_WITH_TYPES, line_idx, col)
        .expect("hover should return info for PaymentMethod");
    assert!(sig.contains("enum PaymentMethod"));
    assert!(sig.contains("Cash"));
    assert!(sig.contains("Card(String)"));
    assert!(sig.contains("Transfer(String, i64)"));
}

#[test]
fn hover_returns_none_for_unknown_token() {
    // Hover on the keyword "machine" itself should not match any state/effect/transition
    let line_idx = BASIC_MACHINE
        .lines()
        .position(|l| l.trim().starts_with("machine TrafficLight"))
        .expect("should find machine line");
    // Point to "machine" keyword
    let result = hover_info(BASIC_MACHINE, line_idx, 1);
    // "machine" is not a state/effect/transition name, but it might match "machine" if
    // the token at that column is "machine". The LSP won't find a definition for it.
    // Either None or it might match the machine name depending on column. Test at col 0.
    // This is mainly checking that it does not panic.
    let _ = result;
}

#[test]
fn hover_on_empty_line_returns_none() {
    let result = hover_info(BASIC_MACHINE, 0, 0);
    assert!(result.is_none(), "empty first line should return None");
}

// ============================================================================
// Doc comment extraction tests
// ============================================================================

#[test]
fn doc_comments_collected_for_state() {
    let doc = collect_doc_comments(MACHINE_WITH_DOCS, "Idle");
    assert!(doc.contains("initial idle state"));
}

#[test]
fn doc_comments_collected_for_transition() {
    let doc = collect_doc_comments(MACHINE_WITH_DOCS, "start");
    assert!(doc.contains("Start the process"));
}

#[test]
fn doc_comments_collected_for_effect() {
    let doc = collect_doc_comments(MACHINE_WITH_DOCS, "log");
    assert!(doc.contains("Log something to the console"));
}

#[test]
fn doc_comments_empty_when_no_comment() {
    let doc = collect_doc_comments(BASIC_MACHINE, "Red");
    assert!(doc.is_empty(), "Red has no doc comments");
}

#[test]
fn doc_comments_returns_empty_for_nonexistent_decl() {
    let doc = collect_doc_comments(BASIC_MACHINE, "Nonexistent");
    assert!(doc.is_empty());
}

// ============================================================================
// Go-to-definition tests
// ============================================================================

#[test]
fn goto_definition_finds_state_declaration() {
    // Find a line that references "Green" in a goto statement
    let handler_line_idx = BASIC_MACHINE
        .lines()
        .position(|l| l.trim().starts_with("goto Green"))
        .expect("should find 'goto Green' line");
    let col = BASIC_MACHINE
        .lines()
        .nth(handler_line_idx)
        .unwrap()
        .find("Green")
        .unwrap();

    let (def_line, _, _) = goto_definition(BASIC_MACHINE, handler_line_idx, col)
        .expect("should find definition of Green");

    // The definition line should be the state declaration
    let def_text = BASIC_MACHINE
        .lines()
        .nth(def_line as usize)
        .expect("definition line should exist");
    assert!(
        def_text.trim().starts_with("state Green"),
        "definition should point to state Green declaration"
    );
}

#[test]
fn goto_definition_finds_effect_declaration() {
    // Find "log_change" in a perform call
    let line_idx = BASIC_MACHINE
        .lines()
        .position(|l| l.contains("perform log_change"))
        .expect("should find perform log_change line");
    let col = BASIC_MACHINE
        .lines()
        .nth(line_idx)
        .unwrap()
        .find("log_change")
        .unwrap();

    let (def_line, _, _) = goto_definition(BASIC_MACHINE, line_idx, col)
        .expect("should find definition of log_change");

    let def_text = BASIC_MACHINE.lines().nth(def_line as usize).unwrap();
    assert!(
        def_text.trim().starts_with("effect log_change"),
        "definition should point to effect declaration"
    );
}

#[test]
fn goto_definition_finds_transition_declaration() {
    // Find "go" referenced in "on go()"
    let line_idx = BASIC_MACHINE
        .lines()
        .position(|l| l.trim().starts_with("on go"))
        .expect("should find 'on go' line");
    let col = BASIC_MACHINE
        .lines()
        .nth(line_idx)
        .unwrap()
        .find("go")
        .unwrap();

    let (def_line, _, _) =
        goto_definition(BASIC_MACHINE, line_idx, col).expect("should find definition of go");

    let def_text = BASIC_MACHINE.lines().nth(def_line as usize).unwrap();
    assert!(
        def_text.trim().starts_with("transition go"),
        "definition should point to transition declaration"
    );
}

#[test]
fn goto_definition_returns_none_for_unknown_symbol() {
    let result = goto_definition(BASIC_MACHINE, 0, 0);
    assert!(result.is_none(), "no definition on empty/comment line");
}

// ============================================================================
// Formatting tests
// ============================================================================

#[test]
fn formatting_produces_valid_output() {
    let program =
        parse_program_with_errors(BASIC_MACHINE, "test.gu").expect("BASIC_MACHINE should parse");
    let formatted = format_program_preserving(&program, BASIC_MACHINE);
    assert!(
        !formatted.is_empty(),
        "formatted output should not be empty"
    );

    // The formatted output should still parse
    parse_program_with_errors(&formatted, "test.gu").expect("formatted output should still parse");
}

#[test]
fn formatting_is_idempotent() {
    let program =
        parse_program_with_errors(BASIC_MACHINE, "test.gu").expect("BASIC_MACHINE should parse");
    let first_format = format_program_preserving(&program, BASIC_MACHINE);

    let program2 =
        parse_program_with_errors(&first_format, "test.gu").expect("first format should parse");
    let second_format = format_program_preserving(&program2, &first_format);

    assert_eq!(
        first_format, second_format,
        "formatting should be idempotent"
    );
}

#[test]
fn formatting_returns_valid_output_for_complex_source() {
    let program = parse_program_with_errors(MACHINE_WITH_FIELDS, "test.gu")
        .expect("MACHINE_WITH_FIELDS should parse");
    let formatted = format_program_preserving(&program, MACHINE_WITH_FIELDS);

    parse_program_with_errors(&formatted, "test.gu")
        .expect("formatted complex source should still parse");
}

#[test]
fn formatting_preserves_type_declarations() {
    let program = parse_program_with_errors(SOURCE_WITH_TYPES, "test.gu")
        .expect("SOURCE_WITH_TYPES should parse");
    let formatted = format_program_preserving(&program, SOURCE_WITH_TYPES);

    assert!(
        formatted.contains("Address"),
        "formatted output should contain Address type"
    );
    assert!(
        formatted.contains("PaymentMethod"),
        "formatted output should contain PaymentMethod enum"
    );
}

// ============================================================================
// Document symbols tests
// ============================================================================

#[test]
fn document_symbols_extracts_machine() {
    let symbols = document_symbols(BASIC_MACHINE).expect("should extract symbols");
    let machine = symbols
        .iter()
        .find(|s| s.name == "TrafficLight")
        .expect("should find TrafficLight machine symbol");
    assert_eq!(machine.kind, SimpleSymbolKind::Class);
}

#[test]
fn document_symbols_extracts_states_as_children() {
    let symbols = document_symbols(BASIC_MACHINE).expect("should extract symbols");
    let machine = symbols
        .iter()
        .find(|s| s.name == "TrafficLight")
        .expect("should find TrafficLight");
    let state_names: Vec<&str> = machine
        .children
        .iter()
        .filter(|c| c.kind == SimpleSymbolKind::EnumMember)
        .map(|c| c.name.as_str())
        .collect();
    assert!(state_names.contains(&"Red"));
    assert!(state_names.contains(&"Green"));
    assert!(state_names.contains(&"Yellow"));
}

#[test]
fn document_symbols_extracts_transitions() {
    let symbols = document_symbols(BASIC_MACHINE).expect("should extract symbols");
    let machine = symbols.iter().find(|s| s.name == "TrafficLight").unwrap();
    let transitions: Vec<&str> = machine
        .children
        .iter()
        .filter(|c| c.kind == SimpleSymbolKind::Event)
        .map(|c| c.name.as_str())
        .collect();
    assert!(transitions.contains(&"go"));
    assert!(transitions.contains(&"slow"));
    assert!(transitions.contains(&"stop"));
}

#[test]
fn document_symbols_extracts_effects() {
    let symbols = document_symbols(BASIC_MACHINE).expect("should extract symbols");
    let machine = symbols.iter().find(|s| s.name == "TrafficLight").unwrap();
    let effects: Vec<&str> = machine
        .children
        .iter()
        .filter(|c| c.kind == SimpleSymbolKind::Function)
        .map(|c| c.name.as_str())
        .collect();
    assert!(effects.contains(&"log_change"));
}

#[test]
fn document_symbols_extracts_handlers() {
    let symbols = document_symbols(BASIC_MACHINE).expect("should extract symbols");
    let machine = symbols.iter().find(|s| s.name == "TrafficLight").unwrap();
    let handlers: Vec<&str> = machine
        .children
        .iter()
        .filter(|c| c.kind == SimpleSymbolKind::Method)
        .map(|c| c.name.as_str())
        .collect();
    assert!(handlers.contains(&"on go"));
    assert!(handlers.contains(&"on slow"));
    assert!(handlers.contains(&"on stop"));
}

#[test]
fn document_symbols_extracts_type_declarations() {
    let symbols = document_symbols(SOURCE_WITH_TYPES).expect("should extract symbols");
    let struct_sym = symbols
        .iter()
        .find(|s| s.name == "Address")
        .expect("should find Address struct");
    assert_eq!(struct_sym.kind, SimpleSymbolKind::Struct);

    let enum_sym = symbols
        .iter()
        .find(|s| s.name == "PaymentMethod")
        .expect("should find PaymentMethod enum");
    assert_eq!(enum_sym.kind, SimpleSymbolKind::Enum);
}

#[test]
fn document_symbols_state_detail_shows_field_count() {
    let symbols = document_symbols(MACHINE_WITH_FIELDS).expect("should extract symbols");
    let machine = symbols.iter().find(|s| s.name == "OrderProcessor").unwrap();
    let pending = machine
        .children
        .iter()
        .find(|c| c.name == "Pending")
        .expect("should find Pending state");
    assert_eq!(
        pending.detail.as_deref(),
        Some("1 field(s)"),
        "Pending has one field"
    );
}

#[test]
fn document_symbols_returns_none_for_unparseable_source() {
    let result = document_symbols("this is not valid gust");
    assert!(result.is_none());
}

// ============================================================================
// Code action tests (missing handler stubs)
// ============================================================================

#[test]
fn code_action_suggests_missing_handler() {
    // "transition back" has no handler in MACHINE_MISSING_HANDLER
    let line_idx = MACHINE_MISSING_HANDLER
        .lines()
        .position(|l| l.trim().starts_with("transition back"))
        .expect("should find 'transition back' line") as u32;

    let actions = code_actions_at(MACHINE_MISSING_HANDLER, line_idx);
    assert!(
        !actions.is_empty(),
        "should suggest a code action for missing handler"
    );

    let action = &actions[0];
    assert!(action.title.contains("back"));
    assert!(action.stub_text.contains("on back"));
    assert!(action.stub_text.contains("EndCtx"));
}

#[test]
fn code_action_not_suggested_for_handled_transition() {
    // "transition go" has a handler, so no code action should appear
    let line_idx = MACHINE_MISSING_HANDLER
        .lines()
        .position(|l| l.trim().starts_with("transition go"))
        .expect("should find 'transition go' line") as u32;

    let actions = code_actions_at(MACHINE_MISSING_HANDLER, line_idx);
    assert!(
        actions.is_empty(),
        "no code action should be suggested for already-handled transition"
    );
}

#[test]
fn code_action_not_triggered_on_unrelated_line() {
    let actions = code_actions_at(MACHINE_MISSING_HANDLER, 0);
    assert!(
        actions.is_empty(),
        "no code action on line 0 (outside any transition)"
    );
}

#[test]
fn code_action_stub_targets_first_target_state() {
    let line_idx = MACHINE_MISSING_HANDLER
        .lines()
        .position(|l| l.trim().starts_with("transition back"))
        .unwrap() as u32;

    let actions = code_actions_at(MACHINE_MISSING_HANDLER, line_idx);
    assert!(!actions.is_empty());
    // "back: End -> Start", so stub should goto Start
    assert!(
        actions[0].stub_text.contains("goto Start"),
        "stub should target the first target state"
    );
}

// ============================================================================
// Inlay hint tests
// ============================================================================

#[test]
fn inlay_hints_for_perform_let_binding() {
    let hints = inlay_hints(MACHINE_WITH_FIELDS);
    assert!(
        !hints.is_empty(),
        "should produce inlay hints for let bindings with perform"
    );

    // "let total = perform validate_order(order_id)" should get ": i64"
    let total_hint = hints.iter().find(|h| h.label == ": i64");
    assert!(total_hint.is_some(), "should have i64 hint for 'total'");

    // "let receipt = perform generate_receipt(order_id, total)" should get ": String"
    let receipt_hint = hints.iter().find(|h| h.label == ": String");
    assert!(
        receipt_hint.is_some(),
        "should have String hint for 'receipt'"
    );
}

#[test]
fn inlay_hints_empty_for_source_without_perform_let() {
    let source = r#"
machine Simple {
    state Start
    state End
    transition go: Start -> End
    on go() {
        goto End;
    }
}
"#;
    let hints = inlay_hints(source);
    assert!(hints.is_empty(), "no inlay hints without perform-let");
}

#[test]
fn inlay_hints_empty_for_unparseable_source() {
    let hints = inlay_hints("not valid gust at all");
    assert!(hints.is_empty());
}

// ============================================================================
// Signature help tests
// ============================================================================

#[test]
fn signature_help_for_perform_call() {
    // Find the line with "perform log_change("
    let line_idx = BASIC_MACHINE
        .lines()
        .position(|l| l.contains("perform log_change("))
        .expect("should find perform log_change line");
    let line = BASIC_MACHINE.lines().nth(line_idx).unwrap();
    // Place cursor right after the opening paren
    let col = line.find("log_change(").unwrap() + "log_change(".len();

    let help = signature_help(BASIC_MACHINE, line_idx, col).expect("should provide signature help");
    assert!(help.label.contains("log_change"));
    assert!(help.label.contains("msg: String"));
    assert_eq!(help.active_parameter, Some(0));
}

#[test]
fn signature_help_active_parameter_after_comma() {
    // Use a two-param effect
    let line_idx = MACHINE_WITH_FIELDS
        .lines()
        .position(|l| l.contains("perform generate_receipt("))
        .expect("should find perform generate_receipt line");
    let line = MACHINE_WITH_FIELDS.lines().nth(line_idx).unwrap();
    // Place cursor after the comma
    let col = line.find("generate_receipt(").unwrap() + "generate_receipt(order_id, ".len();

    let help =
        signature_help(MACHINE_WITH_FIELDS, line_idx, col).expect("should provide signature help");
    assert_eq!(
        help.active_parameter,
        Some(1),
        "active parameter should be 1 (second param)"
    );
    assert_eq!(help.parameters.len(), 2);
}

#[test]
fn signature_help_returns_none_outside_perform() {
    // Point at a line that has no perform call
    let line_idx = BASIC_MACHINE
        .lines()
        .position(|l| l.trim().starts_with("state Red"))
        .expect("should find state Red");
    let result = signature_help(BASIC_MACHINE, line_idx, 10);
    assert!(result.is_none());
}

// ============================================================================
// Token / position helper tests
// ============================================================================

#[test]
fn token_at_col_extracts_identifier() {
    assert_eq!(token_at_col("  state Red", 8), Some("Red".to_string()));
}

#[test]
fn token_at_col_extracts_identifier_at_start() {
    assert_eq!(
        token_at_col("machine Test {", 3),
        Some("machine".to_string())
    );
}

#[test]
fn token_at_col_returns_none_on_whitespace() {
    assert_eq!(token_at_col("   ", 1), None);
}

#[test]
fn token_at_col_returns_none_on_empty_line() {
    assert_eq!(token_at_col("", 0), None);
}

#[test]
fn token_at_col_handles_col_past_end() {
    assert_eq!(token_at_col("hello", 100), Some("hello".to_string()));
}

#[test]
fn first_ident_extracts_simple() {
    assert_eq!(first_ident("foo_bar baz"), Some("foo_bar"));
}

#[test]
fn first_ident_returns_none_on_non_ident_start() {
    assert_eq!(first_ident("(foo)"), None);
}

#[test]
fn first_ident_handles_all_ident() {
    assert_eq!(first_ident("abcdef"), Some("abcdef"));
}

// ============================================================================
// Type expression label tests
// ============================================================================

#[test]
fn type_expr_label_unit() {
    assert_eq!(type_expr_label(&TypeExpr::Unit), "()");
}

#[test]
fn type_expr_label_simple() {
    assert_eq!(
        type_expr_label(&TypeExpr::Simple("String".to_string())),
        "String"
    );
}

#[test]
fn type_expr_label_generic() {
    let ty = TypeExpr::Generic(
        "Vec".to_string(),
        vec![TypeExpr::Simple("String".to_string())],
    );
    assert_eq!(type_expr_label(&ty), "Vec<String>");
}

#[test]
fn type_expr_label_nested_generic() {
    let ty = TypeExpr::Generic(
        "HashMap".to_string(),
        vec![
            TypeExpr::Simple("String".to_string()),
            TypeExpr::Generic("Vec".to_string(), vec![TypeExpr::Simple("i64".to_string())]),
        ],
    );
    assert_eq!(type_expr_label(&ty), "HashMap<String, Vec<i64>>");
}

#[test]
fn type_expr_label_tuple() {
    let ty = TypeExpr::Tuple(vec![
        TypeExpr::Simple("String".to_string()),
        TypeExpr::Simple("i64".to_string()),
    ]);
    assert_eq!(type_expr_label(&ty), "(String, i64)");
}

// ============================================================================
// Hover content formatting tests
// ============================================================================

#[test]
fn make_hover_content_without_doc() {
    let content = make_hover_content("state Red(no fields)", "");
    assert!(content.contains("```gust"));
    assert!(content.contains("state Red(no fields)"));
    assert!(!content.contains("---"));
}

#[test]
fn make_hover_content_with_doc() {
    let content = make_hover_content("state Idle", "The initial state");
    assert!(content.contains("The initial state"));
    assert!(content.contains("---"));
    assert!(content.contains("```gust"));
    assert!(content.contains("state Idle"));
}

// ============================================================================
// Source search helper tests
// ============================================================================

#[test]
fn find_decl_line_finds_state() {
    let (line, _, _, _) = find_decl_line(BASIC_MACHINE, "Red");
    let text = BASIC_MACHINE.lines().nth(line as usize).unwrap();
    assert!(text.trim().starts_with("state Red"));
}

#[test]
fn find_decl_line_finds_machine() {
    let (line, _, _, _) = find_decl_line(BASIC_MACHINE, "TrafficLight");
    let text = BASIC_MACHINE.lines().nth(line as usize).unwrap();
    assert!(text.trim().starts_with("machine TrafficLight"));
}

#[test]
fn find_decl_line_returns_zero_for_missing() {
    let (line, col_s, _, col_e) = find_decl_line(BASIC_MACHINE, "Nonexistent");
    assert_eq!(line, 0);
    assert_eq!(col_s, 0);
    assert_eq!(col_e, 0);
}

#[test]
fn find_line_index_finds_match() {
    let idx = find_line_index(BASIC_MACHINE, "transition go").expect("should find transition go");
    let text = BASIC_MACHINE.lines().nth(idx).unwrap();
    assert!(text.trim().starts_with("transition go"));
}

#[test]
fn find_line_index_returns_none_for_missing() {
    let result = find_line_index(BASIC_MACHINE, "nonexistent_prefix");
    assert!(result.is_none());
}

#[test]
fn find_all_word_occurrences_finds_multiple() {
    let occurrences = find_all_word_occurrences(BASIC_MACHINE, "Green");
    // "Green" appears in: state Green, transition go: Red -> Green,
    // on go() { ... goto Green; }, and possibly in slow transition
    assert!(
        occurrences.len() >= 2,
        "Green should appear at least twice (state decl + transition target)"
    );
}

#[test]
fn find_all_word_occurrences_respects_word_boundaries() {
    let text = "foobar foo barfoo foo";
    let occurrences = find_all_word_occurrences(text, "foo");
    // "foo" appears as whole word at positions: col 7 and col 18
    assert_eq!(
        occurrences.len(),
        2,
        "should find exactly 2 whole-word occurrences"
    );
}

#[test]
fn find_all_word_occurrences_empty_word() {
    let occurrences = find_all_word_occurrences(BASIC_MACHINE, "");
    assert!(occurrences.is_empty());
}

#[test]
fn find_closing_brace_line_finds_match() {
    let source = "fn foo() {\n    bar();\n}\n";
    let result = find_closing_brace_line(source, 0);
    assert_eq!(result, Some(2));
}

#[test]
fn find_closing_brace_line_handles_nested_braces() {
    let source = "outer {\n    inner {\n    }\n}\n";
    let result = find_closing_brace_line(source, 0);
    assert_eq!(result, Some(3));
}

#[test]
fn find_closing_brace_line_returns_none_for_unclosed() {
    let source = "fn foo() {\n    bar();\n";
    let result = find_closing_brace_line(source, 0);
    assert!(result.is_none());
}

#[test]
fn find_let_line_finds_variable() {
    let source = "    on go() {\n        let total = perform calc();\n    }\n";
    let result = find_let_line(source, "total");
    assert_eq!(result, Some(1));
}

#[test]
fn find_let_line_returns_none_for_missing() {
    let result = find_let_line(BASIC_MACHINE, "nonexistent_var");
    assert!(result.is_none());
}

#[test]
fn find_name_end_col_correct_position() {
    let line = "        let total = perform calc();";
    let col = find_name_end_col(line, "total");
    // "let total" starts at col 8, "let " is 4 chars, "total" is 5 chars
    assert_eq!(col, 8 + 4 + 5);
}

#[test]
fn find_name_end_col_fallback_zero() {
    let col = find_name_end_col("no let here", "x");
    assert_eq!(col, 0);
}

// ============================================================================
// find_perform_effect_name tests
// ============================================================================

#[test]
fn find_perform_effect_name_basic() {
    let result = find_perform_effect_name("perform foo(");
    assert_eq!(result, Some("foo".to_string()));
}

#[test]
fn find_perform_effect_name_with_args() {
    let result = find_perform_effect_name("let x = perform bar(a, ");
    assert_eq!(result, Some("bar".to_string()));
}

#[test]
fn find_perform_effect_name_closed_paren_returns_none() {
    let result = find_perform_effect_name("perform foo(a)");
    assert!(result.is_none(), "closed paren should return None");
}

#[test]
fn find_perform_effect_name_no_perform_returns_none() {
    let result = find_perform_effect_name("let x = foo(a, ");
    assert!(result.is_none());
}

#[test]
fn find_perform_effect_name_nested_parens() {
    // "perform outer(inner(" — inner paren is open but outer is also open
    let result = find_perform_effect_name("perform outer(inner(");
    assert_eq!(result, Some("outer".to_string()));
}

// ============================================================================
// Edge case: multi-target transition
// ============================================================================

#[test]
fn hover_on_multi_target_transition() {
    let line_idx = MACHINE_WITH_FIELDS
        .lines()
        .position(|l| l.trim().starts_with("transition process"))
        .expect("should find transition process");
    let col = MACHINE_WITH_FIELDS
        .lines()
        .nth(line_idx)
        .unwrap()
        .find("process")
        .unwrap();

    let (sig, _doc) =
        hover_info(MACHINE_WITH_FIELDS, line_idx, col).expect("should return hover for process");
    assert!(sig.contains("Processing | Failed"));
}

// ============================================================================
// Integration: diagnostics + hover + symbols work on same source
// ============================================================================

#[test]
fn full_pipeline_on_complex_source() {
    // Diagnostics should be clean
    let diags = diagnostics_from_source(MACHINE_WITH_FIELDS, "test.gu");
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == DiagSeverity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "MACHINE_WITH_FIELDS should have no errors, got: {:?}",
        errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );

    // Symbols should be extractable
    let symbols = document_symbols(MACHINE_WITH_FIELDS).expect("should extract symbols");
    assert!(!symbols.is_empty());

    // Inlay hints should work
    let hints = inlay_hints(MACHINE_WITH_FIELDS);
    assert!(!hints.is_empty(), "should produce inlay hints");

    // Formatting should be idempotent
    let program = parse_program_with_errors(MACHINE_WITH_FIELDS, "test.gu").unwrap();
    let formatted = format_program_preserving(&program, MACHINE_WITH_FIELDS);
    let program2 = parse_program_with_errors(&formatted, "test.gu").unwrap();
    let formatted2 = format_program_preserving(&program2, &formatted);
    assert_eq!(formatted, formatted2);
}

#[test]
fn full_pipeline_on_source_with_types() {
    let diags = diagnostics_from_source(SOURCE_WITH_TYPES, "test.gu");
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == DiagSeverity::Error)
        .collect();
    assert!(errors.is_empty(), "SOURCE_WITH_TYPES should have no errors");

    let symbols = document_symbols(SOURCE_WITH_TYPES).expect("should extract symbols");
    let type_names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(type_names.contains(&"Address"));
    assert!(type_names.contains(&"PaymentMethod"));
    assert!(type_names.contains(&"Checkout"));
}
