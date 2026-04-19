//! Comprehensive unit tests for all gust-stdlib machines.
//!
//! Tests verify parsing, AST structure, validation, and codegen (Rust, Go,
//! WASM, no_std, C FFI) for every stdlib `.gu` file.

use gust_lang::ast::{Statement, TypeExpr};
use gust_lang::{
    parse_program_with_errors, validate_program, CffiCodegen, GoCodegen, NoStdCodegen, RustCodegen,
    ValidationReport, WasmCodegen,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse(source: &str, file: &str) -> gust_lang::ast::Program {
    parse_program_with_errors(source, file).expect("parse should succeed")
}

fn validate(source: &str, file: &str) -> ValidationReport {
    let program = parse(source, file);
    validate_program(&program, file, source)
}

fn rust_codegen(source: &str, file: &str) -> String {
    let program = parse(source, file);
    RustCodegen::new().generate(&program)
}

fn go_codegen(source: &str, file: &str) -> String {
    let program = parse(source, file);
    GoCodegen::new().generate(&program, "stdlibtest")
}

fn wasm_codegen(source: &str, file: &str) -> String {
    let program = parse(source, file);
    WasmCodegen::new().generate(&program)
}

fn nostd_codegen(source: &str, file: &str) -> String {
    let program = parse(source, file);
    NoStdCodegen::new().generate(&program)
}

fn ffi_codegen(source: &str, file: &str) -> (String, String) {
    let program = parse(source, file);
    CffiCodegen::new().generate(&program)
}

/// Return the first (and only) machine in the program.
fn first_machine(source: &str, file: &str) -> gust_lang::ast::MachineDecl {
    let program = parse(source, file);
    assert_eq!(program.machines.len(), 1, "expected exactly one machine");
    program.machines.into_iter().next().unwrap()
}

// ---------------------------------------------------------------------------
// RequestResponse tests
// ---------------------------------------------------------------------------

mod request_response {
    use super::*;

    const SRC: &str = gust_stdlib::REQUEST_RESPONSE;
    const FILE: &str = "request_response.gu";

    #[test]
    fn parses_successfully() {
        let _program = parse(SRC, FILE);
    }

    #[test]
    fn machine_name_and_generics() {
        let m = first_machine(SRC, FILE);
        assert_eq!(m.name, "RequestResponse");
        assert_eq!(m.generic_params.len(), 2);
        assert_eq!(m.generic_params[0].name, "T");
        assert_eq!(m.generic_params[1].name, "R");
    }

    #[test]
    fn states() {
        let m = first_machine(SRC, FILE);
        let names: Vec<&str> = m.states.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["Pending", "Completed", "Failed", "TimedOut"]);
    }

    #[test]
    fn state_fields() {
        let m = first_machine(SRC, FILE);
        let pending = m.states.iter().find(|s| s.name == "Pending").unwrap();
        assert_eq!(pending.fields.len(), 2);
        assert_eq!(pending.fields[0].name, "request");
        assert_eq!(pending.fields[1].name, "timeout_ms");

        let completed = m.states.iter().find(|s| s.name == "Completed").unwrap();
        assert_eq!(completed.fields.len(), 1);
        assert_eq!(completed.fields[0].name, "response");
    }

    #[test]
    fn transitions() {
        let m = first_machine(SRC, FILE);
        let names: Vec<&str> = m.transitions.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["send", "receive", "timeout"]);

        let receive = m.transitions.iter().find(|t| t.name == "receive").unwrap();
        assert_eq!(receive.from, "Pending");
        assert!(receive.targets.contains(&"Completed".to_string()));
        assert!(receive.targets.contains(&"Failed".to_string()));
    }

    #[test]
    fn effects() {
        let m = first_machine(SRC, FILE);
        assert_eq!(m.effects.len(), 2);

        let wait = m
            .effects
            .iter()
            .find(|e| e.name == "wait_for_response")
            .unwrap();
        assert!(wait.is_async);
        assert_eq!(wait.params.len(), 2);

        let time = m
            .effects
            .iter()
            .find(|e| e.name == "current_time_ms")
            .unwrap();
        assert!(!time.is_async);
        assert_eq!(time.params.len(), 0);
    }

    #[test]
    fn handlers() {
        let m = first_machine(SRC, FILE);
        assert_eq!(m.handlers.len(), 3);
        let handler_names: Vec<&str> = m
            .handlers
            .iter()
            .map(|h| h.transition_name.as_str())
            .collect();
        assert!(handler_names.contains(&"send"));
        assert!(handler_names.contains(&"receive"));
        assert!(handler_names.contains(&"timeout"));

        let receive_handler = m
            .handlers
            .iter()
            .find(|h| h.transition_name == "receive")
            .unwrap();
        assert!(receive_handler.is_async);

        let send_handler = m
            .handlers
            .iter()
            .find(|h| h.transition_name == "send")
            .unwrap();
        assert!(!send_handler.is_async);
    }

    #[test]
    fn validation_passes() {
        let report = validate(SRC, FILE);
        assert!(report.is_ok(), "validation errors: {:?}", report.errors);
    }

    #[test]
    fn rust_codegen_structure() {
        let code = rust_codegen(SRC, FILE);
        assert!(code.contains("Generated by Gust compiler"));
        assert!(code.contains("RequestResponseState"));
        assert!(code.contains("Pending"));
        assert!(code.contains("Completed"));
        assert!(code.contains("Failed"));
        assert!(code.contains("TimedOut"));
        assert!(code.contains("RequestResponseEffects"));
        assert!(code.contains("fn send"));
        assert!(code.contains("fn receive"));
        assert!(code.contains("fn timeout"));
        assert!(code.contains("wait_for_response"));
        assert!(code.contains("current_time_ms"));
    }

    #[test]
    fn go_codegen_structure() {
        let code = go_codegen(SRC, FILE);
        assert!(code.contains("Code generated by Gust compiler"));
        assert!(code.contains("package stdlibtest"));
        assert!(code.contains("RequestResponse"));
        assert!(code.contains("Pending"));
        assert!(code.contains("Completed"));
        assert!(code.contains("Failed"));
        assert!(code.contains("TimedOut"));
        assert!(code.contains("WaitForResponse"));
        assert!(code.contains("CurrentTimeMs"));
    }

    #[test]
    fn wasm_codegen_produces_output() {
        let code = wasm_codegen(SRC, FILE);
        assert!(code.contains("wasm32"));
        assert!(code.contains("RequestResponse"));
    }

    #[test]
    fn nostd_codegen_produces_output() {
        let code = nostd_codegen(SRC, FILE);
        assert!(code.contains("#![no_std]"));
        assert!(code.contains("RequestResponse"));
    }

    #[test]
    fn ffi_codegen_produces_output() {
        let (rust_code, header) = ffi_codegen(SRC, FILE);
        assert!(rust_code.contains("C FFI"));
        assert!(header.contains("GUST_FFI_H"));
    }
}

// ---------------------------------------------------------------------------
// CircuitBreaker tests
// ---------------------------------------------------------------------------

mod circuit_breaker {
    use super::*;

    const SRC: &str = gust_stdlib::CIRCUIT_BREAKER;
    const FILE: &str = "circuit_breaker.gu";

    #[test]
    fn parses_successfully() {
        let _program = parse(SRC, FILE);
    }

    #[test]
    fn machine_name_and_generics() {
        let m = first_machine(SRC, FILE);
        assert_eq!(m.name, "CircuitBreaker");
        assert_eq!(m.generic_params.len(), 1);
        assert_eq!(m.generic_params[0].name, "T");
    }

    #[test]
    fn states() {
        let m = first_machine(SRC, FILE);
        let names: Vec<&str> = m.states.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["Closed", "Open", "HalfOpen"]);
    }

    #[test]
    fn closed_state_fields() {
        let m = first_machine(SRC, FILE);
        let closed = m.states.iter().find(|s| s.name == "Closed").unwrap();
        assert_eq!(closed.fields.len(), 2);
        assert_eq!(closed.fields[0].name, "failures");
        assert_eq!(closed.fields[1].name, "threshold");
    }

    #[test]
    fn open_state_fields() {
        let m = first_machine(SRC, FILE);
        let open = m.states.iter().find(|s| s.name == "Open").unwrap();
        assert_eq!(open.fields.len(), 2);
        assert_eq!(open.fields[0].name, "opened_at");
        assert_eq!(open.fields[1].name, "timeout_ms");
    }

    #[test]
    fn half_open_state_fields() {
        let m = first_machine(SRC, FILE);
        let ho = m.states.iter().find(|s| s.name == "HalfOpen").unwrap();
        assert_eq!(ho.fields.len(), 2);
        assert_eq!(ho.fields[0].name, "successes");
        assert_eq!(ho.fields[1].name, "needed");
    }

    #[test]
    fn transitions() {
        let m = first_machine(SRC, FILE);
        let names: Vec<&str> = m.transitions.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["fail", "check_open", "succeed_half"]);

        let fail = m.transitions.iter().find(|t| t.name == "fail").unwrap();
        assert_eq!(fail.from, "Closed");
        assert!(fail.targets.contains(&"Closed".to_string()));
        assert!(fail.targets.contains(&"Open".to_string()));

        let check = m
            .transitions
            .iter()
            .find(|t| t.name == "check_open")
            .unwrap();
        assert_eq!(check.from, "Open");
        assert!(check.targets.contains(&"Open".to_string()));
        assert!(check.targets.contains(&"HalfOpen".to_string()));
    }

    #[test]
    fn effects() {
        let m = first_machine(SRC, FILE);
        assert_eq!(m.effects.len(), 1);
        assert_eq!(m.effects[0].name, "current_time_ms");
        assert!(!m.effects[0].is_async);
    }

    #[test]
    fn all_handlers_sync() {
        let m = first_machine(SRC, FILE);
        for h in &m.handlers {
            assert!(!h.is_async, "handler {} should be sync", h.transition_name);
        }
    }

    #[test]
    fn fail_handler_has_conditional() {
        let m = first_machine(SRC, FILE);
        let fail_h = m
            .handlers
            .iter()
            .find(|h| h.transition_name == "fail")
            .unwrap();
        let has_if = fail_h
            .body
            .statements
            .iter()
            .any(|s| matches!(s, Statement::If { .. }));
        assert!(has_if, "fail handler should contain an if statement");
    }

    #[test]
    fn validation_passes() {
        let report = validate(SRC, FILE);
        assert!(report.is_ok(), "validation errors: {:?}", report.errors);
    }

    #[test]
    fn rust_codegen_structure() {
        let code = rust_codegen(SRC, FILE);
        assert!(code.contains("CircuitBreakerState"));
        assert!(code.contains("Closed"));
        assert!(code.contains("Open"));
        assert!(code.contains("HalfOpen"));
        assert!(code.contains("CircuitBreakerEffects"));
        assert!(code.contains("fn fail"));
        assert!(code.contains("fn check_open"));
        assert!(code.contains("fn succeed_half"));
    }

    #[test]
    fn go_codegen_structure() {
        let code = go_codegen(SRC, FILE);
        assert!(code.contains("package stdlibtest"));
        assert!(code.contains("CircuitBreaker"));
        assert!(code.contains("Closed"));
        assert!(code.contains("Open"));
        assert!(code.contains("HalfOpen"));
        assert!(code.contains("CurrentTimeMs"));
    }

    #[test]
    fn wasm_codegen_produces_output() {
        let code = wasm_codegen(SRC, FILE);
        assert!(code.contains("CircuitBreaker"));
    }

    #[test]
    fn nostd_codegen_produces_output() {
        let code = nostd_codegen(SRC, FILE);
        assert!(code.contains("CircuitBreaker"));
    }

    #[test]
    fn ffi_codegen_produces_output() {
        let (rust_code, header) = ffi_codegen(SRC, FILE);
        assert!(rust_code.contains("C FFI"));
        assert!(header.contains("GUST_FFI_H"));
    }
}

// ---------------------------------------------------------------------------
// Saga tests
// ---------------------------------------------------------------------------

mod saga {
    use super::*;

    const SRC: &str = gust_stdlib::SAGA;
    const FILE: &str = "saga.gu";

    #[test]
    fn parses_successfully() {
        let _program = parse(SRC, FILE);
    }

    #[test]
    fn machine_name_and_generics() {
        let m = first_machine(SRC, FILE);
        assert_eq!(m.name, "Saga");
        assert_eq!(m.generic_params.len(), 1);
        assert_eq!(m.generic_params[0].name, "S");
    }

    #[test]
    fn states() {
        let m = first_machine(SRC, FILE);
        let names: Vec<&str> = m.states.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "Planning",
                "Executing",
                "Compensating",
                "Committed",
                "Aborted"
            ]
        );
    }

    #[test]
    fn planning_state_fields() {
        let m = first_machine(SRC, FILE);
        let planning = m.states.iter().find(|s| s.name == "Planning").unwrap();
        assert_eq!(planning.fields.len(), 1);
        assert_eq!(planning.fields[0].name, "steps");
        // Vec<S> is a generic type
        assert!(
            matches!(&planning.fields[0].ty, TypeExpr::Generic(name, args) if name == "Vec" && args.len() == 1)
        );
    }

    #[test]
    fn executing_state_fields() {
        let m = first_machine(SRC, FILE);
        let exec = m.states.iter().find(|s| s.name == "Executing").unwrap();
        assert_eq!(exec.fields.len(), 3);
        assert_eq!(exec.fields[0].name, "steps");
        assert_eq!(exec.fields[1].name, "index");
        assert_eq!(exec.fields[2].name, "completed");
    }

    #[test]
    fn compensating_state_fields() {
        let m = first_machine(SRC, FILE);
        let comp = m.states.iter().find(|s| s.name == "Compensating").unwrap();
        assert_eq!(comp.fields.len(), 3);
        assert_eq!(comp.fields[0].name, "completed");
        assert_eq!(comp.fields[1].name, "index");
        assert_eq!(comp.fields[2].name, "reason");
    }

    #[test]
    fn transitions() {
        let m = first_machine(SRC, FILE);
        let names: Vec<&str> = m.transitions.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["begin", "execute_next", "compensate_next"]);

        let exec_next = m
            .transitions
            .iter()
            .find(|t| t.name == "execute_next")
            .unwrap();
        assert_eq!(exec_next.from, "Executing");
        assert_eq!(exec_next.targets.len(), 3);
        assert!(exec_next.targets.contains(&"Executing".to_string()));
        assert!(exec_next.targets.contains(&"Compensating".to_string()));
        assert!(exec_next.targets.contains(&"Committed".to_string()));
    }

    #[test]
    fn effects() {
        let m = first_machine(SRC, FILE);
        let effect_names: Vec<&str> = m.effects.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            effect_names,
            vec![
                "execute_forward",
                "execute_compensate",
                "len",
                "get_step",
                "push_step",
                "empty_steps",
            ]
        );

        let fwd = m
            .effects
            .iter()
            .find(|e| e.name == "execute_forward")
            .unwrap();
        assert!(fwd.is_async);

        let len = m.effects.iter().find(|e| e.name == "len").unwrap();
        assert!(!len.is_async);
    }

    #[test]
    fn async_handlers() {
        let m = first_machine(SRC, FILE);
        let begin_h = m
            .handlers
            .iter()
            .find(|h| h.transition_name == "begin")
            .unwrap();
        assert!(!begin_h.is_async);

        let exec_h = m
            .handlers
            .iter()
            .find(|h| h.transition_name == "execute_next")
            .unwrap();
        assert!(exec_h.is_async);

        let comp_h = m
            .handlers
            .iter()
            .find(|h| h.transition_name == "compensate_next")
            .unwrap();
        assert!(comp_h.is_async);
    }

    #[test]
    fn validation_passes() {
        let report = validate(SRC, FILE);
        assert!(report.is_ok(), "validation errors: {:?}", report.errors);
    }

    #[test]
    fn rust_codegen_structure() {
        let code = rust_codegen(SRC, FILE);
        assert!(code.contains("SagaState"));
        assert!(code.contains("Planning"));
        assert!(code.contains("Executing"));
        assert!(code.contains("Compensating"));
        assert!(code.contains("Committed"));
        assert!(code.contains("Aborted"));
        assert!(code.contains("SagaEffects"));
        assert!(code.contains("execute_forward"));
        assert!(code.contains("execute_compensate"));
    }

    #[test]
    fn go_codegen_structure() {
        let code = go_codegen(SRC, FILE);
        assert!(code.contains("package stdlibtest"));
        assert!(code.contains("Saga"));
        assert!(code.contains("Planning"));
        assert!(code.contains("Compensating"));
        assert!(code.contains("ExecuteForward"));
        assert!(code.contains("ExecuteCompensate"));
    }

    #[test]
    fn wasm_codegen_produces_output() {
        let code = wasm_codegen(SRC, FILE);
        assert!(code.contains("Saga"));
    }

    #[test]
    fn nostd_codegen_produces_output() {
        let code = nostd_codegen(SRC, FILE);
        assert!(code.contains("Saga"));
    }

    #[test]
    fn ffi_codegen_produces_output() {
        let (rust_code, header) = ffi_codegen(SRC, FILE);
        assert!(rust_code.contains("C FFI"));
        assert!(header.contains("GUST_FFI_H"));
    }
}

// ---------------------------------------------------------------------------
// Retry tests
// ---------------------------------------------------------------------------

mod retry {
    use super::*;

    const SRC: &str = gust_stdlib::RETRY;
    const FILE: &str = "retry.gu";

    #[test]
    fn parses_successfully() {
        let _program = parse(SRC, FILE);
    }

    #[test]
    fn machine_name_and_generics() {
        let m = first_machine(SRC, FILE);
        assert_eq!(m.name, "Retry");
        assert_eq!(m.generic_params.len(), 1);
        assert_eq!(m.generic_params[0].name, "T");
    }

    #[test]
    fn states() {
        let m = first_machine(SRC, FILE);
        let names: Vec<&str> = m.states.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["Ready", "Attempting", "Waiting", "Succeeded", "Failed"]
        );
    }

    #[test]
    fn ready_state_has_config_fields() {
        let m = first_machine(SRC, FILE);
        let ready = m.states.iter().find(|s| s.name == "Ready").unwrap();
        let field_names: Vec<&str> = ready.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            field_names,
            vec![
                "max_attempts",
                "base_delay_ms",
                "max_delay_ms",
                "jitter_pct"
            ]
        );
    }

    #[test]
    fn waiting_state_has_delay_field() {
        let m = first_machine(SRC, FILE);
        let waiting = m.states.iter().find(|s| s.name == "Waiting").unwrap();
        let field_names: Vec<&str> = waiting.fields.iter().map(|f| f.name.as_str()).collect();
        assert!(field_names.contains(&"delay_ms"));
        assert!(field_names.contains(&"attempt"));
    }

    #[test]
    fn succeeded_state_has_value_and_attempts() {
        let m = first_machine(SRC, FILE);
        let succ = m.states.iter().find(|s| s.name == "Succeeded").unwrap();
        assert_eq!(succ.fields.len(), 2);
        assert_eq!(succ.fields[0].name, "value");
        assert_eq!(succ.fields[1].name, "attempts");
    }

    #[test]
    fn transitions() {
        let m = first_machine(SRC, FILE);
        let names: Vec<&str> = m.transitions.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["begin", "run", "wait_complete"]);

        let run = m.transitions.iter().find(|t| t.name == "run").unwrap();
        assert_eq!(run.from, "Attempting");
        assert_eq!(run.targets.len(), 3);
    }

    #[test]
    fn effects() {
        let m = first_machine(SRC, FILE);
        let effect_names: Vec<&str> = m.effects.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            effect_names,
            vec!["execute_operation", "sleep_ms", "compute_backoff"]
        );

        let exec = m
            .effects
            .iter()
            .find(|e| e.name == "execute_operation")
            .unwrap();
        assert!(exec.is_async);

        let sleep = m.effects.iter().find(|e| e.name == "sleep_ms").unwrap();
        assert!(sleep.is_async);
        assert_eq!(sleep.params.len(), 1);

        let backoff = m
            .effects
            .iter()
            .find(|e| e.name == "compute_backoff")
            .unwrap();
        assert!(!backoff.is_async);
        assert_eq!(backoff.params.len(), 4);
    }

    #[test]
    fn handler_async_flags() {
        let m = first_machine(SRC, FILE);
        let begin_h = m
            .handlers
            .iter()
            .find(|h| h.transition_name == "begin")
            .unwrap();
        assert!(!begin_h.is_async);

        let run_h = m
            .handlers
            .iter()
            .find(|h| h.transition_name == "run")
            .unwrap();
        assert!(run_h.is_async);

        let wait_h = m
            .handlers
            .iter()
            .find(|h| h.transition_name == "wait_complete")
            .unwrap();
        assert!(wait_h.is_async);
    }

    #[test]
    fn run_handler_contains_match() {
        let m = first_machine(SRC, FILE);
        let run_h = m
            .handlers
            .iter()
            .find(|h| h.transition_name == "run")
            .unwrap();
        let has_match = run_h
            .body
            .statements
            .iter()
            .any(|s| matches!(s, Statement::Match { .. }));
        assert!(has_match, "run handler should contain a match statement");
    }

    #[test]
    fn validation_passes() {
        let report = validate(SRC, FILE);
        assert!(report.is_ok(), "validation errors: {:?}", report.errors);
    }

    #[test]
    fn rust_codegen_structure() {
        let code = rust_codegen(SRC, FILE);
        assert!(code.contains("RetryState"));
        assert!(code.contains("Ready"));
        assert!(code.contains("Attempting"));
        assert!(code.contains("Waiting"));
        assert!(code.contains("Succeeded"));
        assert!(code.contains("Failed"));
        assert!(code.contains("RetryEffects"));
        assert!(code.contains("execute_operation"));
        assert!(code.contains("sleep_ms"));
        assert!(code.contains("compute_backoff"));
    }

    #[test]
    fn go_codegen_structure() {
        let code = go_codegen(SRC, FILE);
        assert!(code.contains("package stdlibtest"));
        assert!(code.contains("Retry"));
        assert!(code.contains("Ready"));
        assert!(code.contains("Waiting"));
        assert!(code.contains("ExecuteOperation"));
        assert!(code.contains("SleepMs"));
        assert!(code.contains("ComputeBackoff"));
    }

    #[test]
    fn wasm_codegen_produces_output() {
        let code = wasm_codegen(SRC, FILE);
        assert!(code.contains("Retry"));
    }

    #[test]
    fn nostd_codegen_produces_output() {
        let code = nostd_codegen(SRC, FILE);
        assert!(code.contains("Retry"));
    }

    #[test]
    fn ffi_codegen_produces_output() {
        let (rust_code, header) = ffi_codegen(SRC, FILE);
        assert!(rust_code.contains("C FFI"));
        assert!(header.contains("GUST_FFI_H"));
    }
}

// ---------------------------------------------------------------------------
// RateLimiter tests
// ---------------------------------------------------------------------------

mod rate_limiter {
    use super::*;

    const SRC: &str = gust_stdlib::RATE_LIMITER;
    const FILE: &str = "rate_limiter.gu";

    #[test]
    fn parses_successfully() {
        let _program = parse(SRC, FILE);
    }

    #[test]
    fn machine_name_and_generics() {
        let m = first_machine(SRC, FILE);
        assert_eq!(m.name, "RateLimiter");
        assert_eq!(m.generic_params.len(), 1);
        assert_eq!(m.generic_params[0].name, "K");
    }

    #[test]
    fn states() {
        let m = first_machine(SRC, FILE);
        let names: Vec<&str> = m.states.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["Available", "Exhausted"]);
    }

    #[test]
    fn available_state_fields() {
        let m = first_machine(SRC, FILE);
        let avail = m.states.iter().find(|s| s.name == "Available").unwrap();
        assert_eq!(avail.fields.len(), 2);
        assert_eq!(avail.fields[0].name, "tokens");
        assert_eq!(avail.fields[1].name, "max_tokens");
    }

    #[test]
    fn exhausted_state_fields() {
        let m = first_machine(SRC, FILE);
        let exh = m.states.iter().find(|s| s.name == "Exhausted").unwrap();
        assert_eq!(exh.fields.len(), 2);
        assert_eq!(exh.fields[0].name, "retry_after_ms");
        assert_eq!(exh.fields[1].name, "max_tokens");
    }

    #[test]
    fn transitions() {
        let m = first_machine(SRC, FILE);
        let names: Vec<&str> = m.transitions.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["acquire", "refill"]);

        let acquire = m.transitions.iter().find(|t| t.name == "acquire").unwrap();
        assert_eq!(acquire.from, "Available");
        assert!(acquire.targets.contains(&"Available".to_string()));
        assert!(acquire.targets.contains(&"Exhausted".to_string()));

        let refill = m.transitions.iter().find(|t| t.name == "refill").unwrap();
        assert_eq!(refill.from, "Exhausted");
        assert_eq!(refill.targets, vec!["Available".to_string()]);
    }

    #[test]
    fn effects() {
        let m = first_machine(SRC, FILE);
        assert_eq!(m.effects.len(), 1);
        assert_eq!(m.effects[0].name, "now_ms");
        assert!(!m.effects[0].is_async);
        assert_eq!(m.effects[0].params.len(), 0);
    }

    #[test]
    fn all_handlers_sync() {
        let m = first_machine(SRC, FILE);
        for h in &m.handlers {
            assert!(!h.is_async, "handler {} should be sync", h.transition_name);
        }
    }

    #[test]
    fn acquire_handler_has_conditional() {
        let m = first_machine(SRC, FILE);
        let acq_h = m
            .handlers
            .iter()
            .find(|h| h.transition_name == "acquire")
            .unwrap();
        let has_if = acq_h
            .body
            .statements
            .iter()
            .any(|s| matches!(s, Statement::If { .. }));
        assert!(has_if, "acquire handler should contain an if statement");
    }

    #[test]
    fn validation_passes() {
        let report = validate(SRC, FILE);
        assert!(report.is_ok(), "validation errors: {:?}", report.errors);
    }

    #[test]
    fn rust_codegen_structure() {
        let code = rust_codegen(SRC, FILE);
        assert!(code.contains("RateLimiterState"));
        assert!(code.contains("Available"));
        assert!(code.contains("Exhausted"));
        assert!(code.contains("RateLimiterEffects"));
        assert!(code.contains("fn acquire"));
        assert!(code.contains("fn refill"));
        assert!(code.contains("now_ms"));
    }

    #[test]
    fn go_codegen_structure() {
        let code = go_codegen(SRC, FILE);
        assert!(code.contains("package stdlibtest"));
        assert!(code.contains("RateLimiter"));
        assert!(code.contains("Available"));
        assert!(code.contains("Exhausted"));
        assert!(code.contains("NowMs"));
    }

    #[test]
    fn wasm_codegen_produces_output() {
        let code = wasm_codegen(SRC, FILE);
        assert!(code.contains("RateLimiter"));
    }

    #[test]
    fn nostd_codegen_produces_output() {
        let code = nostd_codegen(SRC, FILE);
        assert!(code.contains("RateLimiter"));
    }

    #[test]
    fn ffi_codegen_produces_output() {
        let (rust_code, header) = ffi_codegen(SRC, FILE);
        assert!(rust_code.contains("C FFI"));
        assert!(header.contains("GUST_FFI_H"));
    }
}

// ---------------------------------------------------------------------------
// HealthCheck tests
// ---------------------------------------------------------------------------

mod health_check {
    use super::*;

    const SRC: &str = gust_stdlib::HEALTH_CHECK;
    const FILE: &str = "health_check.gu";

    #[test]
    fn parses_successfully() {
        let _program = parse(SRC, FILE);
    }

    #[test]
    fn machine_name_and_generics() {
        let m = first_machine(SRC, FILE);
        assert_eq!(m.name, "HealthCheck");
        assert_eq!(m.generic_params.len(), 1);
        assert_eq!(m.generic_params[0].name, "T");
    }

    #[test]
    fn states() {
        let m = first_machine(SRC, FILE);
        let names: Vec<&str> = m.states.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["Healthy", "Degraded", "Unhealthy"]);
    }

    #[test]
    fn healthy_state_fields() {
        let m = first_machine(SRC, FILE);
        let healthy = m.states.iter().find(|s| s.name == "Healthy").unwrap();
        assert_eq!(healthy.fields.len(), 1);
        assert_eq!(healthy.fields[0].name, "status");
    }

    #[test]
    fn degraded_state_fields() {
        let m = first_machine(SRC, FILE);
        let degraded = m.states.iter().find(|s| s.name == "Degraded").unwrap();
        assert_eq!(degraded.fields.len(), 2);
        assert_eq!(degraded.fields[0].name, "status");
        assert_eq!(degraded.fields[1].name, "failures");
    }

    #[test]
    fn unhealthy_state_fields() {
        let m = first_machine(SRC, FILE);
        let unhealthy = m.states.iter().find(|s| s.name == "Unhealthy").unwrap();
        assert_eq!(unhealthy.fields.len(), 1);
        assert_eq!(unhealthy.fields[0].name, "reason");
    }

    #[test]
    fn transitions() {
        let m = first_machine(SRC, FILE);
        let names: Vec<&str> = m.transitions.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["probe", "recover"]);

        let probe = m.transitions.iter().find(|t| t.name == "probe").unwrap();
        assert_eq!(probe.from, "Healthy");
        assert_eq!(probe.targets.len(), 3);
        assert!(probe.targets.contains(&"Healthy".to_string()));
        assert!(probe.targets.contains(&"Degraded".to_string()));
        assert!(probe.targets.contains(&"Unhealthy".to_string()));

        let recover = m.transitions.iter().find(|t| t.name == "recover").unwrap();
        assert_eq!(recover.from, "Degraded");
        assert_eq!(recover.targets.len(), 2);
    }

    #[test]
    fn effects() {
        let m = first_machine(SRC, FILE);
        assert_eq!(m.effects.len(), 1);
        assert_eq!(m.effects[0].name, "run_probe");
        assert!(m.effects[0].is_async);
        assert_eq!(m.effects[0].params.len(), 0);
    }

    #[test]
    fn all_handlers_async() {
        let m = first_machine(SRC, FILE);
        for h in &m.handlers {
            assert!(h.is_async, "handler {} should be async", h.transition_name);
        }
    }

    #[test]
    fn probe_handler_has_match() {
        let m = first_machine(SRC, FILE);
        let probe_h = m
            .handlers
            .iter()
            .find(|h| h.transition_name == "probe")
            .unwrap();
        let has_match = probe_h
            .body
            .statements
            .iter()
            .any(|s| matches!(s, Statement::Match { .. }));
        assert!(has_match, "probe handler should contain a match statement");
    }

    #[test]
    fn validation_passes() {
        let report = validate(SRC, FILE);
        assert!(report.is_ok(), "validation errors: {:?}", report.errors);
    }

    #[test]
    fn rust_codegen_structure() {
        let code = rust_codegen(SRC, FILE);
        assert!(code.contains("HealthCheckState"));
        assert!(code.contains("Healthy"));
        assert!(code.contains("Degraded"));
        assert!(code.contains("Unhealthy"));
        assert!(code.contains("HealthCheckEffects"));
        assert!(code.contains("run_probe"));
        assert!(code.contains("fn probe"));
        assert!(code.contains("fn recover"));
    }

    #[test]
    fn go_codegen_structure() {
        let code = go_codegen(SRC, FILE);
        assert!(code.contains("package stdlibtest"));
        assert!(code.contains("HealthCheck"));
        assert!(code.contains("Healthy"));
        assert!(code.contains("Degraded"));
        assert!(code.contains("Unhealthy"));
        assert!(code.contains("RunProbe"));
    }

    #[test]
    fn wasm_codegen_produces_output() {
        let code = wasm_codegen(SRC, FILE);
        assert!(code.contains("HealthCheck"));
    }

    #[test]
    fn nostd_codegen_produces_output() {
        let code = nostd_codegen(SRC, FILE);
        assert!(code.contains("HealthCheck"));
    }

    #[test]
    fn ffi_codegen_produces_output() {
        let (rust_code, header) = ffi_codegen(SRC, FILE);
        assert!(rust_code.contains("C FFI"));
        assert!(header.contains("GUST_FFI_H"));
    }
}

// ---------------------------------------------------------------------------
// Cross-cutting / all_sources() tests
// ---------------------------------------------------------------------------

mod cross_cutting {
    use super::*;

    /// All sources that are expected to contain exactly one machine.
    /// Excludes type-only sources like `engine_failure.gu`.
    fn machine_sources() -> Vec<(&'static str, &'static str)> {
        gust_stdlib::all_sources()
            .into_iter()
            .filter(|(name, _)| *name != "engine_failure.gu")
            .collect()
    }

    #[test]
    fn all_sources_returns_seven_entries() {
        let sources = gust_stdlib::all_sources();
        assert_eq!(sources.len(), 7);
    }

    #[test]
    fn all_sources_names_match_files() {
        let sources = gust_stdlib::all_sources();
        let names: Vec<&str> = sources.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            names,
            vec![
                "request_response.gu",
                "circuit_breaker.gu",
                "saga.gu",
                "retry.gu",
                "rate_limiter.gu",
                "health_check.gu",
                "engine_failure.gu",
            ]
        );
    }

    #[test]
    fn all_sources_parse_and_validate() {
        for (file, source) in &gust_stdlib::all_sources() {
            let program = parse_program_with_errors(source, file)
                .unwrap_or_else(|_| panic!("{file} should parse"));
            let report = validate_program(&program, file, source);
            assert!(
                report.is_ok(),
                "{file} validation failed: {:?}",
                report.errors
            );
        }
    }

    #[test]
    fn machine_sources_have_exactly_one_machine() {
        // engine_failure.gu is a type-only source; skip it here.
        for (file, source) in gust_stdlib::all_sources()
            .iter()
            .filter(|(name, _)| *name != "engine_failure.gu")
        {
            let program = parse(source, file);
            assert_eq!(
                program.machines.len(),
                1,
                "{file} should contain exactly one machine"
            );
        }
    }

    #[test]
    fn machine_sources_have_no_type_declarations() {
        // Type-only sources (engine_failure.gu) are handled separately.
        for (file, source) in machine_sources() {
            let program = parse(source, file);
            assert!(
                program.types.is_empty(),
                "{file} should not have standalone type declarations"
            );
        }
    }

    #[test]
    fn all_sources_have_no_channels() {
        for (file, source) in &gust_stdlib::all_sources() {
            let program = parse(source, file);
            assert!(
                program.channels.is_empty(),
                "{file} should not have channel declarations"
            );
        }
    }

    #[test]
    fn all_sources_have_no_use_paths() {
        for (file, source) in &gust_stdlib::all_sources() {
            let program = parse(source, file);
            assert!(
                program.uses.is_empty(),
                "{file} should not have use declarations"
            );
        }
    }

    #[test]
    fn all_machines_have_generic_params() {
        for (file, source) in machine_sources() {
            let m = first_machine(source, file);
            assert!(
                !m.generic_params.is_empty(),
                "{file} machine should have generic parameters"
            );
        }
    }

    #[test]
    fn all_machines_have_at_least_two_states() {
        for (file, source) in machine_sources() {
            let m = first_machine(source, file);
            assert!(
                m.states.len() >= 2,
                "{file} machine should have at least two states, found {}",
                m.states.len()
            );
        }
    }

    #[test]
    fn all_machines_have_at_least_one_effect() {
        for (file, source) in machine_sources() {
            let m = first_machine(source, file);
            assert!(
                !m.effects.is_empty(),
                "{file} machine should have at least one effect"
            );
        }
    }

    #[test]
    fn all_machines_have_matching_handler_count() {
        for (file, source) in machine_sources() {
            let m = first_machine(source, file);
            assert_eq!(
                m.transitions.len(),
                m.handlers.len(),
                "{file}: transition count should match handler count"
            );
        }
    }

    #[test]
    fn handler_names_match_transition_names() {
        for (file, source) in machine_sources() {
            let m = first_machine(source, file);
            let transition_names: Vec<&str> =
                m.transitions.iter().map(|t| t.name.as_str()).collect();
            for handler in &m.handlers {
                assert!(
                    transition_names.contains(&handler.transition_name.as_str()),
                    "{file}: handler '{}' has no matching transition",
                    handler.transition_name
                );
            }
        }
    }

    #[test]
    fn every_handler_body_has_at_least_one_goto() {
        for (file, source) in machine_sources() {
            let m = first_machine(source, file);
            for handler in &m.handlers {
                let has_goto = has_goto_in_block(&handler.body);
                assert!(
                    has_goto,
                    "{file}: handler '{}' should contain at least one goto",
                    handler.transition_name
                );
            }
        }
    }

    /// Recursively search for Goto statements in a block.
    fn has_goto_in_block(block: &gust_lang::ast::Block) -> bool {
        for stmt in &block.statements {
            match stmt {
                Statement::Goto { .. } => return true,
                Statement::If {
                    then_block,
                    else_block,
                    ..
                } => {
                    if has_goto_in_block(then_block) {
                        return true;
                    }
                    if let Some(eb) = else_block {
                        if has_goto_in_block(eb) {
                            return true;
                        }
                    }
                }
                Statement::Match { arms, .. } => {
                    for arm in arms {
                        if has_goto_in_block(&arm.body) {
                            return true;
                        }
                    }
                }
                _ => {}
            }
        }
        false
    }

    #[test]
    fn rust_codegen_all_contain_serde_derives() {
        for (file, source) in &gust_stdlib::all_sources() {
            let code = rust_codegen(source, file);
            assert!(
                code.contains("Serialize") && code.contains("Deserialize"),
                "{file}: Rust codegen should include serde derives"
            );
        }
    }

    #[test]
    fn machine_go_codegen_contain_json_tags() {
        // json tags are on state struct fields; type-only sources have none.
        for (file, source) in machine_sources() {
            let code = go_codegen(source, file);
            assert!(
                code.contains("json:"),
                "{file}: Go codegen should include json struct tags"
            );
        }
    }

    #[test]
    fn rust_and_go_codegen_preserve_state_names() {
        for (file, source) in machine_sources() {
            let m = first_machine(source, file);
            let rust_code = rust_codegen(source, file);
            let go_code = go_codegen(source, file);

            for state in &m.states {
                assert!(
                    rust_code.contains(&state.name),
                    "{file}: Rust codegen missing state '{}'",
                    state.name
                );
                assert!(
                    go_code.contains(&state.name),
                    "{file}: Go codegen missing state '{}'",
                    state.name
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// EngineFailure (type-only source) tests
// ---------------------------------------------------------------------------

mod engine_failure {
    use super::*;

    const FILE: &str = "engine_failure.gu";
    const SRC: &str = gust_stdlib::ENGINE_FAILURE;

    #[test]
    fn parses_successfully() {
        let _ = parse(SRC, FILE);
    }

    #[test]
    fn validation_passes() {
        let program = parse_program_with_errors(SRC, FILE).expect("should parse");
        let report = validate_program(&program, FILE, SRC);
        assert!(report.is_ok(), "validation failed: {:?}", report.errors);
    }

    #[test]
    fn declares_exactly_one_enum() {
        let program = parse(SRC, FILE);
        assert_eq!(program.types.len(), 1, "should declare exactly one type");
        assert!(program.machines.is_empty(), "should declare no machines");
        match &program.types[0] {
            gust_lang::ast::TypeDecl::Enum { name, .. } => {
                assert_eq!(name, "EngineFailure");
            }
            other => panic!("expected Enum, got {:?}", other),
        }
    }

    #[test]
    fn has_all_five_variants() {
        let program = parse(SRC, FILE);
        let gust_lang::ast::TypeDecl::Enum { variants, .. } = &program.types[0] else {
            panic!("expected enum");
        };
        let names: Vec<&str> = variants.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "UserError",
                "SystemError",
                "IntegrationError",
                "Timeout",
                "Cancelled",
            ]
        );
    }

    #[test]
    fn variant_payload_arity_matches_spec() {
        let program = parse(SRC, FILE);
        let gust_lang::ast::TypeDecl::Enum { variants, .. } = &program.types[0] else {
            panic!("expected enum");
        };
        let by_name: std::collections::HashMap<&str, usize> = variants
            .iter()
            .map(|v| (v.name.as_str(), v.payload.len()))
            .collect();
        assert_eq!(by_name["UserError"], 1, "UserError(reason)");
        assert_eq!(by_name["SystemError"], 2, "SystemError(reason, attempt)");
        assert_eq!(
            by_name["IntegrationError"], 3,
            "IntegrationError(service, status_code, body)"
        );
        assert_eq!(by_name["Timeout"], 1, "Timeout(wall_clock_ms)");
        assert_eq!(by_name["Cancelled"], 1, "Cancelled(requested_by)");
    }

    #[test]
    fn rust_codegen_emits_serde_derived_enum() {
        let code = rust_codegen(SRC, FILE);
        assert!(code.contains("pub enum EngineFailure"));
        assert!(code.contains("UserError(String)"));
        assert!(code.contains("SystemError(String, i64)"));
        assert!(code.contains("IntegrationError(String, i64, String)"));
        assert!(code.contains("Timeout(i64)"));
        assert!(code.contains("Cancelled(String)"));
        assert!(code.contains("Serialize") && code.contains("Deserialize"));
    }

    #[test]
    fn go_codegen_emits_type_and_constants() {
        let code = go_codegen(SRC, FILE);
        assert!(code.contains("type EngineFailure"));
        for variant in [
            "EngineFailureUserError",
            "EngineFailureSystemError",
            "EngineFailureIntegrationError",
            "EngineFailureTimeout",
            "EngineFailureCancelled",
        ] {
            assert!(code.contains(variant), "Go codegen missing {variant}");
        }
    }

    #[test]
    fn usable_as_state_field_type_in_downstream_machine() {
        // Smoke test: a downstream machine can declare a state whose field
        // is typed `EngineFailure` alongside the enum declaration.
        // (Constructing variants with payloads is a separate grammar feature.)
        let combined = format!(
            "{}\n\nmachine Demo {{\n    state Running\n    state Failed(failure: EngineFailure)\n\n    transition fail: Running -> Failed\n\n    effect produce_failure() -> EngineFailure\n\n    on fail() {{\n        let f: EngineFailure = perform produce_failure();\n        goto Failed(f);\n    }}\n}}\n",
            SRC
        );
        let program = parse_program_with_errors(&combined, "demo.gu").expect("should parse");
        let report = validate_program(&program, "demo.gu", &combined);
        assert!(
            report.is_ok(),
            "downstream use should validate: {:?}",
            report.errors
        );
    }
}

// ---------------------------------------------------------------------------
// Negative / edge-case tests
// ---------------------------------------------------------------------------

mod negative_tests {
    use super::*;

    #[test]
    fn malformed_source_fails_to_parse() {
        let bad_source = "machine Broken { state A( }";
        let result = parse_program_with_errors(bad_source, "broken.gu");
        assert!(result.is_err(), "malformed source should fail to parse");
    }

    #[test]
    fn duplicate_state_triggers_validation_error() {
        let source = r#"
machine DupState<T> {
    state Foo(x: i64)
    state Foo(y: i64)

    transition go: Foo -> Foo

    effect noop() -> i64

    on go() {
        goto Foo(x);
    }
}
"#;
        let program = parse(source, "dup.gu");
        let report = validate_program(&program, "dup.gu", source);
        assert!(
            !report.is_ok(),
            "duplicate state should produce validation error"
        );
        let msgs: Vec<&str> = report.errors.iter().map(|e| e.message.as_str()).collect();
        assert!(
            msgs.iter().any(|m| m.contains("duplicate state")),
            "error should mention 'duplicate state', got: {:?}",
            msgs
        );
    }

    #[test]
    fn undefined_state_in_transition_triggers_error() {
        let source = r#"
machine BadTrans<T> {
    state A(x: i64)

    transition go: A -> NonExistent

    effect noop() -> i64

    on go() {
        goto NonExistent(0);
    }
}
"#;
        let program = parse(source, "bad_trans.gu");
        let report = validate_program(&program, "bad_trans.gu", source);
        assert!(
            !report.is_ok(),
            "undefined target state should produce validation error"
        );
        let msgs: Vec<&str> = report.errors.iter().map(|e| e.message.as_str()).collect();
        assert!(
            msgs.iter().any(|m| m.contains("undefined state")),
            "error should mention 'undefined state', got: {:?}",
            msgs
        );
    }

    #[test]
    fn duplicate_transition_triggers_error() {
        let source = r#"
machine DupTrans<T> {
    state A(x: i64)
    state B(y: i64)

    transition go: A -> B
    transition go: B -> A

    effect noop() -> i64

    on go() {
        goto B(0);
    }
}
"#;
        let program = parse(source, "dup_trans.gu");
        let report = validate_program(&program, "dup_trans.gu", source);
        assert!(
            !report.is_ok(),
            "duplicate transition should produce validation error"
        );
    }

    #[test]
    fn empty_machine_still_parses() {
        let source = r#"
machine Empty<T> {
    state Start(x: i64)
    state End(y: i64)

    transition go: Start -> End

    effect noop() -> i64

    on go() {
        goto End(x);
    }
}
"#;
        let program = parse(source, "empty.gu");
        let report = validate_program(&program, "empty.gu", source);
        assert!(
            report.is_ok(),
            "minimal machine should validate: {:?}",
            report.errors
        );
    }

    #[test]
    fn constants_are_accessible_and_non_empty() {
        assert!(!gust_stdlib::REQUEST_RESPONSE.is_empty());
        assert!(!gust_stdlib::CIRCUIT_BREAKER.is_empty());
        assert!(!gust_stdlib::SAGA.is_empty());
        assert!(!gust_stdlib::RETRY.is_empty());
        assert!(!gust_stdlib::RATE_LIMITER.is_empty());
        assert!(!gust_stdlib::HEALTH_CHECK.is_empty());
    }
}
