use gust_runtime::prelude::*;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Mock types implementing the runtime traits
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum LightState {
    Off,
    On,
    Dimmed(u8),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct LightMachine {
    state: LightState,
    name: String,
}

impl Machine for LightMachine {
    type State = LightState;

    fn current_state(&self) -> &Self::State {
        &self.state
    }
}

impl LightMachine {
    fn new(name: &str) -> Self {
        Self {
            state: LightState::Off,
            name: name.to_string(),
        }
    }

    fn turn_on(&mut self) {
        self.state = LightState::On;
    }

    fn dim(&mut self, level: u8) {
        self.state = LightState::Dimmed(level);
    }
}

// A minimal machine with unit-like state for edge cases
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum EmptyState {
    Init,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct MinimalMachine {
    state: EmptyState,
}

impl Machine for MinimalMachine {
    type State = EmptyState;

    fn current_state(&self) -> &Self::State {
        &self.state
    }
}

// Supervisor mock
#[derive(Debug)]
struct TestSupervisor {
    restart_count: usize,
}

impl Supervisor for TestSupervisor {
    type Error = String;

    fn on_child_failure(&mut self, _child_id: &str, _error: &Self::Error) -> SupervisorAction {
        self.restart_count += 1;
        if self.restart_count > 3 {
            SupervisorAction::Escalate
        } else {
            SupervisorAction::Restart
        }
    }
}

// ---------------------------------------------------------------------------
// Machine trait tests
// ---------------------------------------------------------------------------

#[test]
fn machine_current_state_returns_initial_state() {
    let m = LightMachine::new("desk");
    assert_eq!(*m.current_state(), LightState::Off);
}

#[test]
fn machine_current_state_reflects_transitions() {
    let mut m = LightMachine::new("desk");
    m.turn_on();
    assert_eq!(*m.current_state(), LightState::On);
    m.dim(50);
    assert_eq!(*m.current_state(), LightState::Dimmed(50));
}

#[test]
fn machine_to_json_produces_valid_json() {
    let m = LightMachine::new("lamp");
    let json = m.to_json().expect("serialization should succeed");
    assert!(json.contains("\"name\": \"lamp\""));
    assert!(json.contains("\"state\": \"Off\""));
}

#[test]
fn machine_from_json_round_trip() {
    let original = LightMachine::new("lamp");
    let json = original.to_json().unwrap();
    let restored = LightMachine::from_json(&json).unwrap();
    assert_eq!(original, restored);
}

#[test]
fn machine_round_trip_with_complex_state() {
    let mut m = LightMachine::new("rgb");
    m.dim(128);
    let json = m.to_json().unwrap();
    let restored = LightMachine::from_json(&json).unwrap();
    assert_eq!(m, restored);
    assert_eq!(*restored.current_state(), LightState::Dimmed(128));
}

#[test]
fn machine_from_json_rejects_invalid_input() {
    let result = LightMachine::from_json("not valid json");
    assert!(result.is_err());
}

#[test]
fn machine_from_json_rejects_wrong_shape() {
    let result = LightMachine::from_json(r#"{"wrong": "shape"}"#);
    assert!(result.is_err());
}

#[test]
fn minimal_machine_round_trip() {
    let m = MinimalMachine {
        state: EmptyState::Init,
    };
    let json = m.to_json().unwrap();
    let restored = MinimalMachine::from_json(&json).unwrap();
    assert_eq!(m, restored);
}

// ---------------------------------------------------------------------------
// Supervisor trait tests
// ---------------------------------------------------------------------------

#[test]
fn supervisor_returns_restart_within_threshold() {
    let mut sup = TestSupervisor { restart_count: 0 };
    let action = sup.on_child_failure("child-1", &"timeout".to_string());
    assert!(matches!(action, SupervisorAction::Restart));
}

#[test]
fn supervisor_escalates_after_repeated_failures() {
    let mut sup = TestSupervisor { restart_count: 0 };
    for _ in 0..3 {
        sup.on_child_failure("child-1", &"error".to_string());
    }
    // Fourth failure should escalate
    let action = sup.on_child_failure("child-1", &"error".to_string());
    assert!(matches!(action, SupervisorAction::Escalate));
}

#[test]
fn supervisor_action_debug_format() {
    let actions = vec![
        SupervisorAction::Restart,
        SupervisorAction::Escalate,
        SupervisorAction::Ignore,
    ];
    for a in &actions {
        // Debug should not panic and should produce non-empty output
        let debug = format!("{:?}", a);
        assert!(!debug.is_empty());
    }
}

#[test]
fn supervisor_action_clone() {
    let original = SupervisorAction::Restart;
    let cloned = original.clone();
    assert!(matches!(cloned, SupervisorAction::Restart));
}

// ---------------------------------------------------------------------------
// Envelope tests
// ---------------------------------------------------------------------------

#[test]
fn envelope_new_sets_fields() {
    let env = Envelope::new("source-machine", "target-machine", 42);
    assert_eq!(env.source, "source-machine");
    assert_eq!(env.target, "target-machine");
    assert_eq!(env.payload, 42);
    assert!(env.correlation_id.is_none());
}

#[test]
fn envelope_with_correlation_id() {
    let env = Envelope::new("a", "b", "hello").with_correlation("req-123");
    assert_eq!(env.correlation_id, Some("req-123".to_string()));
}

#[test]
fn envelope_builder_chain() {
    let env = Envelope::new("src", "tgt", vec![1, 2, 3]).with_correlation("corr-456");
    assert_eq!(env.source, "src");
    assert_eq!(env.target, "tgt");
    assert_eq!(env.payload, vec![1, 2, 3]);
    assert_eq!(env.correlation_id.as_deref(), Some("corr-456"));
}

#[test]
fn envelope_serialization_round_trip() {
    let env = Envelope::new("src", "tgt", "payload-data").with_correlation("c-1");
    let json = serde_json::to_string(&env).unwrap();
    let restored: Envelope<String> = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.source, "src");
    assert_eq!(restored.target, "tgt");
    assert_eq!(restored.payload, "payload-data");
    assert_eq!(restored.correlation_id.as_deref(), Some("c-1"));
}

#[test]
fn envelope_serialization_without_correlation() {
    let env = Envelope::new("a", "b", 99u32);
    let json = serde_json::to_string(&env).unwrap();
    let restored: Envelope<u32> = serde_json::from_str(&json).unwrap();
    assert!(restored.correlation_id.is_none());
    assert_eq!(restored.payload, 99);
}

#[test]
fn envelope_debug_format() {
    let env = Envelope::new("src", "tgt", "msg");
    let debug = format!("{:?}", env);
    assert!(debug.contains("src"));
    assert!(debug.contains("tgt"));
    assert!(debug.contains("msg"));
}

#[test]
fn envelope_clone() {
    let env = Envelope::new("a", "b", 42).with_correlation("id");
    let cloned = env.clone();
    assert_eq!(cloned.source, env.source);
    assert_eq!(cloned.target, env.target);
    assert_eq!(cloned.payload, env.payload);
    assert_eq!(cloned.correlation_id, env.correlation_id);
}

#[test]
fn envelope_accepts_string_types_for_source_and_target() {
    // &str
    let _e1 = Envelope::new("a", "b", 1);
    // String
    let _e2 = Envelope::new(String::from("a"), String::from("b"), 1);
    // Mixed
    let _e3 = Envelope::new("a", String::from("b"), 1);
}

#[test]
fn envelope_with_complex_payload() {
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct Command {
        action: String,
        value: i32,
    }

    let cmd = Command {
        action: "set".to_string(),
        value: 100,
    };
    let env = Envelope::new("controller", "actuator", cmd.clone());
    let json = serde_json::to_string(&env).unwrap();
    let restored: Envelope<Command> = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.payload, cmd);
}

// ---------------------------------------------------------------------------
// ChildHandle tests
// ---------------------------------------------------------------------------

#[test]
fn child_handle_stores_id() {
    let handle = ChildHandle {
        id: "worker-1".to_string(),
    };
    assert_eq!(handle.id, "worker-1");
}

#[test]
fn child_handle_debug_format() {
    let handle = ChildHandle {
        id: "w".to_string(),
    };
    let debug = format!("{:?}", handle);
    assert!(debug.contains("w"));
}

#[test]
fn child_handle_clone() {
    let handle = ChildHandle {
        id: "task-1".to_string(),
    };
    let cloned = handle.clone();
    assert_eq!(cloned.id, handle.id);
}

// ---------------------------------------------------------------------------
// RestartStrategy tests
// ---------------------------------------------------------------------------

#[test]
fn restart_strategy_default_is_one_for_one() {
    let strategy: RestartStrategy = Default::default();
    assert!(matches!(strategy, RestartStrategy::OneForOne));
}

#[test]
fn restart_strategy_debug_format() {
    let strategies = [
        RestartStrategy::OneForOne,
        RestartStrategy::OneForAll,
        RestartStrategy::RestForOne,
    ];
    for s in &strategies {
        let debug = format!("{:?}", s);
        assert!(!debug.is_empty());
    }
}

#[test]
fn restart_strategy_clone_and_copy() {
    let s = RestartStrategy::OneForAll;
    let copied = s; // Copy — s is still usable because Copy
    let also_copied = s;
    assert!(matches!(copied, RestartStrategy::OneForAll));
    assert!(matches!(also_copied, RestartStrategy::OneForAll));
}

// ---------------------------------------------------------------------------
// SupervisorRuntime tests
// ---------------------------------------------------------------------------

#[test]
fn supervisor_runtime_default_uses_one_for_one() {
    let rt = SupervisorRuntime::default();
    assert!(matches!(rt.strategy(), RestartStrategy::OneForOne));
}

#[test]
fn supervisor_runtime_new_uses_one_for_one() {
    let rt = SupervisorRuntime::new();
    assert!(matches!(rt.strategy(), RestartStrategy::OneForOne));
}

#[test]
fn supervisor_runtime_with_strategy_sets_strategy() {
    let rt = SupervisorRuntime::with_strategy(RestartStrategy::OneForAll);
    assert!(matches!(rt.strategy(), RestartStrategy::OneForAll));

    let rt = SupervisorRuntime::with_strategy(RestartStrategy::RestForOne);
    assert!(matches!(rt.strategy(), RestartStrategy::RestForOne));
}

// restart_scope tests

#[test]
fn restart_scope_one_for_one_only_restarts_failed_child() {
    let rt = SupervisorRuntime::with_strategy(RestartStrategy::OneForOne);
    assert_eq!(rt.restart_scope(0, 5), 0..1);
    assert_eq!(rt.restart_scope(2, 5), 2..3);
    assert_eq!(rt.restart_scope(4, 5), 4..5);
}

#[test]
fn restart_scope_one_for_all_restarts_everything() {
    let rt = SupervisorRuntime::with_strategy(RestartStrategy::OneForAll);
    assert_eq!(rt.restart_scope(0, 5), 0..5);
    assert_eq!(rt.restart_scope(3, 5), 0..5);
}

#[test]
fn restart_scope_rest_for_one_restarts_from_failed_onward() {
    let rt = SupervisorRuntime::with_strategy(RestartStrategy::RestForOne);
    assert_eq!(rt.restart_scope(0, 5), 0..5);
    assert_eq!(rt.restart_scope(2, 5), 2..5);
    assert_eq!(rt.restart_scope(4, 5), 4..5);
}

#[test]
fn restart_scope_single_child() {
    let one = SupervisorRuntime::with_strategy(RestartStrategy::OneForOne);
    assert_eq!(one.restart_scope(0, 1), 0..1);

    let all = SupervisorRuntime::with_strategy(RestartStrategy::OneForAll);
    assert_eq!(all.restart_scope(0, 1), 0..1);

    let rest = SupervisorRuntime::with_strategy(RestartStrategy::RestForOne);
    assert_eq!(rest.restart_scope(0, 1), 0..1);
}

#[test]
fn restart_scope_boundary_last_child() {
    let rt = SupervisorRuntime::with_strategy(RestartStrategy::OneForOne);
    // Last child in a group of 10
    assert_eq!(rt.restart_scope(9, 10), 9..10);
}

#[test]
fn restart_scope_one_for_one_saturating_add_no_overflow() {
    let rt = SupervisorRuntime::with_strategy(RestartStrategy::OneForOne);
    // usize::MAX should not overflow thanks to saturating_add
    let scope = rt.restart_scope(usize::MAX, usize::MAX);
    assert_eq!(scope, usize::MAX..usize::MAX);
}

// Async tests for spawn and join

#[tokio::test]
async fn spawn_named_returns_child_handle_with_correct_id() {
    let rt = SupervisorRuntime::new();
    let handle = rt.spawn_named("my-worker", async { Ok(()) });
    assert_eq!(handle.id, "my-worker");
}

#[tokio::test]
async fn spawn_named_accepts_string_id() {
    let rt = SupervisorRuntime::new();
    let handle = rt.spawn_named(String::from("worker"), async { Ok(()) });
    assert_eq!(handle.id, "worker");
}

#[tokio::test]
async fn join_next_returns_ok_for_successful_task() {
    let rt = SupervisorRuntime::new();
    rt.spawn_named("ok-task", async { Ok(()) });

    let result = tokio::time::timeout(Duration::from_secs(2), rt.join_next())
        .await
        .expect("should not timeout");
    assert!(matches!(result, Some(Ok(()))));
}

#[tokio::test]
async fn join_next_returns_err_for_failed_task() {
    let rt = SupervisorRuntime::new();
    rt.spawn_named("fail-task", async { Err("something broke".to_string()) });

    let result = tokio::time::timeout(Duration::from_secs(2), rt.join_next())
        .await
        .expect("should not timeout");
    assert!(matches!(result, Some(Err(_))));
    if let Some(Err(msg)) = result {
        assert_eq!(msg, "something broke");
    }
}

#[tokio::test]
async fn join_next_returns_none_when_no_tasks() {
    let rt = SupervisorRuntime::new();
    let result = tokio::time::timeout(Duration::from_secs(1), rt.join_next())
        .await
        .expect("should not timeout");
    assert!(result.is_none());
}

#[tokio::test]
async fn join_next_drains_multiple_tasks() {
    let rt = SupervisorRuntime::new();
    rt.spawn_named("t1", async { Ok(()) });
    rt.spawn_named("t2", async { Ok(()) });
    rt.spawn_named("t3", async { Err("fail".to_string()) });

    let mut ok_count = 0;
    let mut err_count = 0;

    for _ in 0..3 {
        let result = tokio::time::timeout(Duration::from_secs(2), rt.join_next())
            .await
            .expect("should not timeout");
        match result {
            Some(Ok(())) => ok_count += 1,
            Some(Err(_)) => err_count += 1,
            None => panic!("expected a result, got None"),
        }
    }
    assert_eq!(ok_count, 2);
    assert_eq!(err_count, 1);

    // After all tasks drained, join_next returns None
    let final_result = tokio::time::timeout(Duration::from_secs(1), rt.join_next())
        .await
        .expect("should not timeout");
    assert!(final_result.is_none());
}

#[tokio::test]
async fn spawn_named_task_that_does_work() {
    let rt = SupervisorRuntime::new();
    let shared = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let shared_clone = shared.clone();

    rt.spawn_named("adder", async move {
        shared_clone.fetch_add(42, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    });

    let result = tokio::time::timeout(Duration::from_secs(2), rt.join_next())
        .await
        .expect("should not timeout");
    assert!(matches!(result, Some(Ok(()))));
    assert_eq!(shared.load(std::sync::atomic::Ordering::SeqCst), 42);
}

// ---------------------------------------------------------------------------
// Re-export tests (serde, serde_json, thiserror available via prelude)
// ---------------------------------------------------------------------------

#[test]
fn prelude_reexports_serde_json() {
    // Verify serde_json is accessible through the prelude
    let val: serde_json::Value = serde_json::json!({"key": "value"});
    assert_eq!(val["key"], "value");
}

#[test]
fn prelude_reexports_serde_derive_macros() {
    // The Serialize/Deserialize derives on our mock types above
    // prove that the re-exports work. This test simply confirms
    // a round-trip through serde_json using the prelude re-exports.
    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Ping {
        seq: u32,
    }
    let p = Ping { seq: 7 };
    let json = serde_json::to_string(&p).unwrap();
    let restored: Ping = serde_json::from_str(&json).unwrap();
    assert_eq!(p, restored);
}
