use gust_mcp::{
    handle_request, handle_tools_call, tool_build, tool_check, tool_diagram, tool_format,
    tool_parse, JsonRpcRequest,
};
use serde_json::{json, Value};
use std::io::Write;
use tempfile::NamedTempFile;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Write source to a temporary .gu file and return the handle (keeps file alive).
fn write_temp_gu(source: &str) -> NamedTempFile {
    let mut f = NamedTempFile::with_suffix(".gu").expect("create tempfile");
    f.write_all(source.as_bytes()).expect("write source");
    f.flush().expect("flush");
    f
}

/// Canonical minimal machine used across many tests.
const MINIMAL_MACHINE: &str = r#"
machine Light {
    state Off
    state On

    transition toggle: Off -> On
    transition switch: On -> Off
}
"#;

/// Machine with effects and a handler.
const MACHINE_WITH_EFFECTS: &str = r#"
machine Payments {
    state Pending
    state Done(receipt: String)

    transition charge: Pending -> Done

    effect process() -> String

    on charge() {
        let receipt = perform process();
        goto Done(receipt);
    }
}
"#;

/// Machine with multiple targets (branching transitions).
const MACHINE_WITH_BRANCHES: &str = r#"
machine OrderProcessor {
    state Pending
    state Validated
    state Failed(reason: String)

    transition validate: Pending -> Validated | Failed

    effect check_order() -> bool

    on validate() {
        let ok = perform check_order();
        if ok {
            goto Validated;
        } else {
            goto Failed("invalid");
        }
    }
}
"#;

/// Extract the text content from a tools/call response.
fn extract_tool_text(resp: &Value) -> &str {
    resp.get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
        .expect("response should contain content[0].text")
}

/// Check whether the response is marked as an error.
fn is_tool_error(resp: &Value) -> bool {
    resp.get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

// ===========================================================================
// gust_check tests
// ===========================================================================

#[test]
fn check_valid_machine_returns_no_errors() {
    let f = write_temp_gu(MINIMAL_MACHINE);
    let args = json!({ "file": f.path().to_str().unwrap() });
    let result = tool_check(&args).expect("tool_check should succeed");

    let parsed: Value = serde_json::from_str(&result).expect("result should be valid JSON");
    let errors = parsed["errors"].as_array().unwrap();
    assert!(errors.is_empty(), "expected no errors, got: {errors:?}");
}

#[test]
fn check_syntax_error_returns_error_diagnostics() {
    let f = write_temp_gu("machine { broken syntax }");
    let args = json!({ "file": f.path().to_str().unwrap() });
    let result = tool_check(&args).expect("tool_check should succeed even on bad input");

    let parsed: Value = serde_json::from_str(&result).expect("result should be valid JSON");
    let errors = parsed["errors"].as_array().unwrap();
    assert!(!errors.is_empty(), "expected parse errors");
    // Each error should have at least a message
    let first = &errors[0];
    assert!(first["message"].is_string(), "error should have a message");
}

#[test]
fn check_semantic_error_returns_validation_error() {
    // Reference an undeclared state in a transition
    let source = r#"
machine Broken {
    state A

    transition go: A -> NonExistent
}
"#;
    let f = write_temp_gu(source);
    let args = json!({ "file": f.path().to_str().unwrap() });
    let result = tool_check(&args).expect("tool_check should succeed");

    let parsed: Value = serde_json::from_str(&result).expect("valid JSON");
    let errors = parsed["errors"].as_array().unwrap();
    assert!(
        !errors.is_empty(),
        "expected validation error for undeclared state"
    );
}

#[test]
fn check_valid_machine_with_effects_returns_no_errors() {
    let f = write_temp_gu(MACHINE_WITH_EFFECTS);
    let args = json!({ "file": f.path().to_str().unwrap() });
    let result = tool_check(&args).expect("tool_check should succeed");

    let parsed: Value = serde_json::from_str(&result).expect("valid JSON");
    let errors = parsed["errors"].as_array().unwrap();
    assert!(errors.is_empty(), "expected no errors, got: {errors:?}");
}

#[test]
fn check_missing_file_returns_error() {
    let args = json!({ "file": "/nonexistent/path/to/file.gu" });
    let result = tool_check(&args);
    assert!(result.is_err(), "missing file should produce an error");
    let msg = result.unwrap_err();
    assert!(
        msg.contains("Cannot read"),
        "error should mention file read failure: {msg}"
    );
}

#[test]
fn check_missing_file_arg_returns_error() {
    let args = json!({});
    let result = tool_check(&args);
    assert!(result.is_err(), "missing arg should produce an error");
    let msg = result.unwrap_err();
    assert!(
        msg.contains("Missing required argument"),
        "error should mention missing argument: {msg}"
    );
}

#[test]
fn check_empty_file_returns_no_errors() {
    // An empty file has no machines, so no validation errors
    let f = write_temp_gu("");
    let args = json!({ "file": f.path().to_str().unwrap() });
    let result = tool_check(&args).expect("tool_check should succeed");

    let parsed: Value = serde_json::from_str(&result).expect("valid JSON");
    let errors = parsed["errors"].as_array().unwrap();
    assert!(errors.is_empty(), "empty file should produce no errors");
}

// ===========================================================================
// gust_build tests
// ===========================================================================

#[test]
fn build_rust_target_generates_valid_output() {
    let f = write_temp_gu(MINIMAL_MACHINE);
    let args = json!({ "file": f.path().to_str().unwrap(), "target": "rust" });
    let result = tool_build(&args).expect("tool_build should succeed");

    assert!(
        result.contains("enum LightState"),
        "should contain state enum"
    );
    assert!(
        result.contains("toggle"),
        "should contain transition method"
    );
}

#[test]
fn build_defaults_to_rust_target() {
    let f = write_temp_gu(MINIMAL_MACHINE);
    let args = json!({ "file": f.path().to_str().unwrap() });
    let result = tool_build(&args).expect("tool_build should succeed");

    // Should be Rust output (same as explicit rust target)
    assert!(
        result.contains("enum LightState"),
        "default target should be Rust"
    );
}

#[test]
fn build_go_target_generates_valid_output() {
    let f = write_temp_gu(MINIMAL_MACHINE);
    let args = json!({
        "file": f.path().to_str().unwrap(),
        "target": "go",
        "package": "light"
    });
    let result = tool_build(&args).expect("tool_build should succeed");

    assert!(
        result.contains("package light"),
        "should contain Go package declaration"
    );
}

#[test]
fn build_wasm_target_generates_output() {
    let f = write_temp_gu(MINIMAL_MACHINE);
    let args = json!({ "file": f.path().to_str().unwrap(), "target": "wasm" });
    let result = tool_build(&args).expect("tool_build should succeed");

    assert!(
        result.contains("wasm_bindgen"),
        "WASM output should contain wasm_bindgen"
    );
}

#[test]
fn build_nostd_target_generates_output() {
    let f = write_temp_gu(MINIMAL_MACHINE);
    let args = json!({ "file": f.path().to_str().unwrap(), "target": "nostd" });
    let result = tool_build(&args).expect("tool_build should succeed");

    assert!(
        result.contains("no_std"),
        "no_std output should contain no_std attribute"
    );
}

#[test]
fn build_ffi_target_generates_rust_and_c_header() {
    let f = write_temp_gu(MINIMAL_MACHINE);
    let args = json!({ "file": f.path().to_str().unwrap(), "target": "ffi" });
    let result = tool_build(&args).expect("tool_build should succeed");

    assert!(
        result.contains("// === Rust source ==="),
        "FFI output should contain Rust section"
    );
    assert!(
        result.contains("// === C header ==="),
        "FFI output should contain C header section"
    );
}

#[test]
fn build_unknown_target_returns_error() {
    let f = write_temp_gu(MINIMAL_MACHINE);
    let args = json!({ "file": f.path().to_str().unwrap(), "target": "python" });
    let result = tool_build(&args);

    assert!(result.is_err(), "unknown target should produce an error");
    let msg = result.unwrap_err();
    assert!(msg.contains("Unknown target"), "error: {msg}");
    assert!(
        msg.contains("python"),
        "error should mention the bad target"
    );
}

#[test]
fn build_syntax_error_returns_parse_error() {
    let f = write_temp_gu("not valid gust");
    let args = json!({ "file": f.path().to_str().unwrap(), "target": "rust" });
    let result = tool_build(&args);

    assert!(result.is_err(), "parse failure should return Err");
    let msg = result.unwrap_err();
    assert!(msg.contains("Parse error"), "error: {msg}");
}

#[test]
fn build_machine_with_effects_generates_effect_trait() {
    let f = write_temp_gu(MACHINE_WITH_EFFECTS);
    let args = json!({ "file": f.path().to_str().unwrap(), "target": "rust" });
    let result = tool_build(&args).expect("tool_build should succeed");

    assert!(
        result.contains("PaymentsEffects"),
        "should generate effect trait"
    );
    assert!(
        result.contains("fn process"),
        "should contain effect method"
    );
}

// ===========================================================================
// gust_diagram tests
// ===========================================================================

#[test]
fn diagram_produces_mermaid_state_diagram() {
    let f = write_temp_gu(MINIMAL_MACHINE);
    let args = json!({ "file": f.path().to_str().unwrap() });
    let result = tool_diagram(&args).expect("tool_diagram should succeed");

    assert!(
        result.contains("stateDiagram-v2"),
        "should contain Mermaid stateDiagram-v2 header"
    );
    assert!(
        result.contains("[*] --> Off"),
        "should have initial state arrow"
    );
    assert!(
        result.contains("Off --> On : toggle"),
        "should have toggle transition"
    );
    assert!(
        result.contains("On --> Off : switch"),
        "should have switch transition"
    );
}

#[test]
fn diagram_with_machine_filter_returns_single_machine() {
    let source = r#"
machine A {
    state S1
    state S2
    transition go: S1 -> S2
}
machine B {
    state X1
    state X2
    transition move: X1 -> X2
}
"#;
    let f = write_temp_gu(source);
    let args = json!({ "file": f.path().to_str().unwrap(), "machine": "A" });
    let result = tool_diagram(&args).expect("tool_diagram should succeed");

    assert!(
        result.contains("S1 --> S2 : go"),
        "should contain machine A transitions"
    );
    assert!(
        !result.contains("X1"),
        "should not contain machine B content"
    );
}

#[test]
fn diagram_nonexistent_machine_returns_error() {
    let f = write_temp_gu(MINIMAL_MACHINE);
    let args = json!({ "file": f.path().to_str().unwrap(), "machine": "DoesNotExist" });
    let result = tool_diagram(&args);

    assert!(result.is_err(), "nonexistent machine should error");
    let msg = result.unwrap_err();
    assert!(msg.contains("not found"), "error: {msg}");
    assert!(
        msg.contains("Light"),
        "should list available machines: {msg}"
    );
}

#[test]
fn diagram_empty_file_returns_error() {
    let f = write_temp_gu("");
    let args = json!({ "file": f.path().to_str().unwrap() });
    let result = tool_diagram(&args);

    assert!(result.is_err(), "empty file should error (no machines)");
    let msg = result.unwrap_err();
    assert!(msg.contains("No machine declarations"), "error: {msg}");
}

#[test]
fn diagram_branching_transitions_shows_all_targets() {
    let f = write_temp_gu(MACHINE_WITH_BRANCHES);
    let args = json!({ "file": f.path().to_str().unwrap() });
    let result = tool_diagram(&args).expect("tool_diagram should succeed");

    assert!(
        result.contains("Pending --> Validated : validate"),
        "should show Validated target"
    );
    assert!(
        result.contains("Pending --> Failed : validate"),
        "should show Failed target"
    );
}

#[test]
fn diagram_multiple_machines_without_filter() {
    let source = r#"
machine First {
    state A
    state B
    transition go: A -> B
}
machine Second {
    state X
    state Y
    transition move: X -> Y
}
"#;
    let f = write_temp_gu(source);
    let args = json!({ "file": f.path().to_str().unwrap() });
    let result = tool_diagram(&args).expect("tool_diagram should succeed");

    assert!(
        result.contains("%% Machine: First"),
        "should label first machine"
    );
    assert!(
        result.contains("%% Machine: Second"),
        "should label second machine"
    );
    assert!(
        result.contains("A --> B : go"),
        "should contain First transitions"
    );
    assert!(
        result.contains("X --> Y : move"),
        "should contain Second transitions"
    );
}

// ===========================================================================
// gust_format tests
// ===========================================================================

#[test]
fn format_returns_formatted_source() {
    let f = write_temp_gu(MINIMAL_MACHINE);
    let args = json!({ "file": f.path().to_str().unwrap() });
    let result = tool_format(&args).expect("tool_format should succeed");

    // Formatted output should still contain the machine
    assert!(
        result.contains("machine Light"),
        "formatted output should contain machine name"
    );
    assert!(
        result.contains("state Off"),
        "formatted output should contain states"
    );
}

#[test]
fn format_is_idempotent() {
    let f = write_temp_gu(MINIMAL_MACHINE);
    let args = json!({ "file": f.path().to_str().unwrap() });

    let first_pass = tool_format(&args).expect("first format pass");

    // Write the formatted output and format again
    let f2 = write_temp_gu(&first_pass);
    let args2 = json!({ "file": f2.path().to_str().unwrap() });
    let second_pass = tool_format(&args2).expect("second format pass");

    assert_eq!(
        first_pass, second_pass,
        "formatting should be idempotent (second pass should match first)"
    );
}

#[test]
fn format_syntax_error_returns_error() {
    let f = write_temp_gu("machine {{ bad");
    let args = json!({ "file": f.path().to_str().unwrap() });
    let result = tool_format(&args);

    assert!(result.is_err(), "syntax error should return Err");
    let msg = result.unwrap_err();
    assert!(msg.contains("Parse error"), "error: {msg}");
}

#[test]
fn format_machine_with_effects_preserves_structure() {
    let f = write_temp_gu(MACHINE_WITH_EFFECTS);
    let args = json!({ "file": f.path().to_str().unwrap() });
    let result = tool_format(&args).expect("tool_format should succeed");

    assert!(
        result.contains("effect process()"),
        "should preserve effect declaration"
    );
    assert!(result.contains("on charge()"), "should preserve handler");
    assert!(
        result.contains("perform process()"),
        "should preserve perform expression"
    );
}

// ===========================================================================
// gust_parse tests
// ===========================================================================

#[test]
fn parse_returns_valid_json_ast() {
    let f = write_temp_gu(MINIMAL_MACHINE);
    let args = json!({ "file": f.path().to_str().unwrap() });
    let result = tool_parse(&args).expect("tool_parse should succeed");

    let ast: Value = serde_json::from_str(&result).expect("result should be valid JSON");
    assert!(ast["machines"].is_array(), "AST should have machines array");

    let machines = ast["machines"].as_array().unwrap();
    assert_eq!(machines.len(), 1, "should have one machine");
    assert_eq!(machines[0]["name"], "Light", "machine name should be Light");
}

#[test]
fn parse_returns_states_and_transitions() {
    let f = write_temp_gu(MINIMAL_MACHINE);
    let args = json!({ "file": f.path().to_str().unwrap() });
    let result = tool_parse(&args).expect("tool_parse should succeed");

    let ast: Value = serde_json::from_str(&result).unwrap();
    let machine = &ast["machines"][0];

    let states = machine["states"].as_array().unwrap();
    let state_names: Vec<&str> = states.iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert_eq!(state_names, vec!["Off", "On"], "should parse all states");

    let transitions = machine["transitions"].as_array().unwrap();
    assert_eq!(transitions.len(), 2, "should parse all transitions");
    assert_eq!(transitions[0]["name"], "toggle");
    assert_eq!(transitions[1]["name"], "switch");
}

#[test]
fn parse_returns_effects_and_handlers() {
    let f = write_temp_gu(MACHINE_WITH_EFFECTS);
    let args = json!({ "file": f.path().to_str().unwrap() });
    let result = tool_parse(&args).expect("tool_parse should succeed");

    let ast: Value = serde_json::from_str(&result).unwrap();
    let machine = &ast["machines"][0];

    let effects = machine["effects"].as_array().unwrap();
    assert_eq!(effects.len(), 1, "should have one effect");
    assert_eq!(effects[0]["name"], "process");
    assert_eq!(effects[0]["return_type"], "String");

    let handlers = machine["handlers"].as_array().unwrap();
    assert_eq!(handlers.len(), 1, "should have one handler");
    assert_eq!(handlers[0]["transition"], "charge");
}

#[test]
fn parse_returns_type_declarations() {
    let source = r#"
type Order {
    id: String,
    amount: i64,
}

machine Processor {
    state Idle
    state Done
    transition go: Idle -> Done
}
"#;
    let f = write_temp_gu(source);
    let args = json!({ "file": f.path().to_str().unwrap() });
    let result = tool_parse(&args).expect("tool_parse should succeed");

    let ast: Value = serde_json::from_str(&result).unwrap();
    let types = ast["types"].as_array().unwrap();
    assert_eq!(types.len(), 1, "should have one type declaration");
    assert_eq!(types[0]["name"], "Order");
    assert_eq!(types[0]["kind"], "struct");

    let fields = types[0]["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 2, "Order should have 2 fields");
    assert_eq!(fields[0]["name"], "id");
    assert_eq!(fields[1]["name"], "amount");
}

#[test]
fn parse_syntax_error_returns_error() {
    let f = write_temp_gu("this is not valid gust code!!!");
    let args = json!({ "file": f.path().to_str().unwrap() });
    let result = tool_parse(&args);

    assert!(result.is_err(), "syntax error should return Err");
    let msg = result.unwrap_err();
    assert!(msg.contains("Parse error"), "error: {msg}");
}

#[test]
fn parse_empty_file_returns_empty_ast() {
    let f = write_temp_gu("");
    let args = json!({ "file": f.path().to_str().unwrap() });
    let result = tool_parse(&args).expect("tool_parse should succeed on empty input");

    let ast: Value = serde_json::from_str(&result).expect("result should be valid JSON");
    assert_eq!(
        ast["machines"].as_array().unwrap().len(),
        0,
        "empty file should have no machines"
    );
    assert_eq!(
        ast["types"].as_array().unwrap().len(),
        0,
        "empty file should have no types"
    );
}

#[test]
fn parse_branching_transition_returns_multiple_targets() {
    let f = write_temp_gu(MACHINE_WITH_BRANCHES);
    let args = json!({ "file": f.path().to_str().unwrap() });
    let result = tool_parse(&args).expect("tool_parse should succeed");

    let ast: Value = serde_json::from_str(&result).unwrap();
    let transitions = ast["machines"][0]["transitions"].as_array().unwrap();
    let validate = &transitions[0];

    let targets = validate["targets"].as_array().unwrap();
    assert_eq!(targets.len(), 2, "should have two targets");
    assert_eq!(targets[0], "Validated");
    assert_eq!(targets[1], "Failed");
}

// ===========================================================================
// handle_request dispatch tests
// ===========================================================================

#[test]
fn handle_initialize_returns_server_info() {
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(1)),
        method: "initialize".to_string(),
        params: json!({}),
    };

    let resp = handle_request(req).expect("initialize should return a response");
    let result = resp.result.expect("should have result");
    assert_eq!(result["serverInfo"]["name"], "gust-mcp");
    assert_eq!(result["protocolVersion"], "2024-11-05");
}

#[test]
fn handle_tools_list_returns_five_tools() {
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(2)),
        method: "tools/list".to_string(),
        params: json!({}),
    };

    let resp = handle_request(req).expect("tools/list should return a response");
    let result = resp.result.expect("should have result");
    let tools = result["tools"].as_array().expect("tools should be array");
    assert_eq!(tools.len(), 5, "should expose 5 tools");

    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"gust_check"), "should have gust_check");
    assert!(names.contains(&"gust_build"), "should have gust_build");
    assert!(names.contains(&"gust_diagram"), "should have gust_diagram");
    assert!(names.contains(&"gust_format"), "should have gust_format");
    assert!(names.contains(&"gust_parse"), "should have gust_parse");
}

#[test]
fn handle_tools_call_dispatches_to_check() {
    let f = write_temp_gu(MINIMAL_MACHINE);
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(3)),
        method: "tools/call".to_string(),
        params: json!({
            "name": "gust_check",
            "arguments": { "file": f.path().to_str().unwrap() }
        }),
    };

    let resp = handle_request(req).expect("tools/call should return a response");
    let result = resp.result.expect("should have result");
    let text = extract_tool_text(&result);

    let diagnostics: Value = serde_json::from_str(text).expect("check output should be JSON");
    assert!(
        diagnostics["errors"].as_array().unwrap().is_empty(),
        "valid machine should have no errors"
    );
}

#[test]
fn handle_tools_call_unknown_tool_returns_error_content() {
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(4)),
        method: "tools/call".to_string(),
        params: json!({
            "name": "nonexistent_tool",
            "arguments": {}
        }),
    };

    let resp = handle_request(req).expect("tools/call should return a response");
    let result = resp.result.expect("should have result");
    assert!(
        is_tool_error(&result),
        "unknown tool should be marked as error"
    );
    let text = extract_tool_text(&result);
    assert!(
        text.contains("Unknown tool"),
        "should mention unknown tool: {text}"
    );
}

#[test]
fn handle_tools_call_missing_name_returns_jsonrpc_error() {
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(5)),
        method: "tools/call".to_string(),
        params: json!({ "arguments": {} }),
    };

    let resp = handle_request(req).expect("tools/call should return a response");
    assert!(
        resp.error.is_some(),
        "missing name should be a JSON-RPC error"
    );
    assert_eq!(resp.error.as_ref().unwrap().code, -32602);
}

#[test]
fn handle_unknown_method_returns_method_not_found() {
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(6)),
        method: "unknown/method".to_string(),
        params: json!({}),
    };

    let resp = handle_request(req).expect("unknown method should return a response");
    assert!(resp.error.is_some(), "should be an error");
    assert_eq!(resp.error.as_ref().unwrap().code, -32601);
}

#[test]
fn handle_notification_returns_none() {
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: None,
        method: "notifications/initialized".to_string(),
        params: json!({}),
    };

    let resp = handle_request(req);
    assert!(
        resp.is_none(),
        "notifications should not produce a response"
    );
}

// ===========================================================================
// tools/call integration via handle_tools_call
// ===========================================================================

#[test]
fn tools_call_build_rust_via_dispatch() {
    let f = write_temp_gu(MINIMAL_MACHINE);
    let resp = handle_tools_call(
        json!(1),
        json!({
            "name": "gust_build",
            "arguments": {
                "file": f.path().to_str().unwrap(),
                "target": "rust"
            }
        }),
    );

    let result = resp.result.expect("should have result");
    assert!(!is_tool_error(&result), "should not be an error");
    let text = extract_tool_text(&result);
    assert!(
        text.contains("enum LightState"),
        "should contain Rust codegen output"
    );
}

#[test]
fn tools_call_build_go_via_dispatch() {
    let f = write_temp_gu(MINIMAL_MACHINE);
    let resp = handle_tools_call(
        json!(1),
        json!({
            "name": "gust_build",
            "arguments": {
                "file": f.path().to_str().unwrap(),
                "target": "go",
                "package": "light"
            }
        }),
    );

    let result = resp.result.expect("should have result");
    assert!(!is_tool_error(&result), "should not be an error");
    let text = extract_tool_text(&result);
    assert!(
        text.contains("package light"),
        "should contain Go package: {text}"
    );
}

#[test]
fn tools_call_diagram_via_dispatch() {
    let f = write_temp_gu(MINIMAL_MACHINE);
    let resp = handle_tools_call(
        json!(1),
        json!({
            "name": "gust_diagram",
            "arguments": { "file": f.path().to_str().unwrap() }
        }),
    );

    let result = resp.result.expect("should have result");
    assert!(!is_tool_error(&result), "should not be an error");
    let text = extract_tool_text(&result);
    assert!(
        text.contains("stateDiagram-v2"),
        "should contain Mermaid output"
    );
}

#[test]
fn tools_call_format_via_dispatch() {
    let f = write_temp_gu(MINIMAL_MACHINE);
    let resp = handle_tools_call(
        json!(1),
        json!({
            "name": "gust_format",
            "arguments": { "file": f.path().to_str().unwrap() }
        }),
    );

    let result = resp.result.expect("should have result");
    assert!(!is_tool_error(&result), "should not be an error");
    let text = extract_tool_text(&result);
    assert!(
        text.contains("machine Light"),
        "formatted output should contain machine"
    );
}

#[test]
fn tools_call_parse_via_dispatch() {
    let f = write_temp_gu(MINIMAL_MACHINE);
    let resp = handle_tools_call(
        json!(1),
        json!({
            "name": "gust_parse",
            "arguments": { "file": f.path().to_str().unwrap() }
        }),
    );

    let result = resp.result.expect("should have result");
    assert!(!is_tool_error(&result), "should not be an error");
    let text = extract_tool_text(&result);
    let ast: Value = serde_json::from_str(text).expect("parse output should be valid JSON");
    assert!(ast["machines"].is_array(), "AST should have machines");
}

#[test]
fn tools_call_error_sets_is_error_flag() {
    let resp = handle_tools_call(
        json!(1),
        json!({
            "name": "gust_build",
            "arguments": { "file": "/nonexistent.gu" }
        }),
    );

    let result = resp.result.expect("should have result");
    assert!(
        is_tool_error(&result),
        "file-not-found should set isError flag"
    );
}

// ===========================================================================
// Content-Length framing tests
// ===========================================================================

#[test]
fn read_message_parses_content_length_frame() {
    let body = r#"{"hello":true}"#;
    let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
    let mut reader = std::io::Cursor::new(frame.into_bytes());

    let msg = gust_mcp::read_message(&mut reader).expect("should read message");
    assert_eq!(msg, body);
}

#[test]
fn write_message_produces_correct_frame() {
    let mut buf: Vec<u8> = Vec::new();
    gust_mcp::write_message(&mut buf, "{\"ok\":1}").expect("write should succeed");
    let output = String::from_utf8(buf).unwrap();

    assert!(
        output.starts_with("Content-Length: 8\r\n\r\n"),
        "should start with correct Content-Length header: {output}"
    );
    assert!(output.ends_with("{\"ok\":1}"), "should end with JSON body");
}

#[test]
fn read_message_returns_none_on_eof() {
    let mut reader = std::io::Cursor::new(b"" as &[u8]);
    let msg = gust_mcp::read_message(&mut reader);
    assert!(msg.is_none(), "EOF should return None");
}

// ===========================================================================
// Parse enum type declarations
// ===========================================================================

#[test]
fn parse_enum_type_returns_variants() {
    let source = r#"
enum Color {
    Red,
    Green,
    Blue,
}

machine Painter {
    state Idle
    state Painting(color: Color)
    transition paint: Idle -> Painting
}
"#;
    let f = write_temp_gu(source);
    let args = json!({ "file": f.path().to_str().unwrap() });
    let result = tool_parse(&args).expect("tool_parse should succeed");

    let ast: Value = serde_json::from_str(&result).unwrap();
    let types = ast["types"].as_array().unwrap();
    assert_eq!(types.len(), 1);
    assert_eq!(types[0]["kind"], "enum");
    assert_eq!(types[0]["name"], "Color");

    let variants = types[0]["variants"].as_array().unwrap();
    assert_eq!(variants.len(), 3);
    let variant_names: Vec<&str> = variants
        .iter()
        .map(|v| v["name"].as_str().unwrap())
        .collect();
    assert_eq!(variant_names, vec!["Red", "Green", "Blue"]);
}

// ===========================================================================
// Build with complex machine (handler body, effects, branching)
// ===========================================================================

#[test]
fn build_complex_machine_generates_complete_rust() {
    let f = write_temp_gu(MACHINE_WITH_BRANCHES);
    let args = json!({ "file": f.path().to_str().unwrap(), "target": "rust" });
    let result = tool_build(&args).expect("tool_build should succeed");

    assert!(
        result.contains("OrderProcessorEffects"),
        "should generate effects trait"
    );
    assert!(
        result.contains("fn validate"),
        "should generate transition method"
    );
    assert!(
        result.contains("Validated"),
        "should reference target state"
    );
    assert!(result.contains("Failed"), "should reference error state");
}
