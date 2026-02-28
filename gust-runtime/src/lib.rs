//! Gust Runtime Library
//!
//! This crate provides the runtime support for compiled Gust programs.
//! Generated Rust code from .gu files imports `gust_runtime::prelude::*`.

pub mod prelude {
    use std::future::Future;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use tokio::task::JoinSet;

    pub use serde::{Deserialize, Serialize};
    pub use serde_json;
    pub use thiserror;

    /// Trait for all Gust state machines.
    /// Provides common functionality like serialization and state inspection.
    pub trait Machine: Serialize + for<'de> Deserialize<'de> {
        type State: std::fmt::Debug + Clone + Serialize + for<'de> Deserialize<'de>;

        /// Get the current state
        fn current_state(&self) -> &Self::State;

        /// Serialize the machine to JSON
        fn to_json(&self) -> Result<String, serde_json::Error> {
            serde_json::to_string_pretty(self)
        }

        /// Deserialize a machine from JSON
        fn from_json(json: &str) -> Result<Self, serde_json::Error>
        where
            Self: Sized,
        {
            serde_json::from_str(json)
        }
    }

    /// Trait for supervised machine groups (structured concurrency)
    pub trait Supervisor {
        type Error: std::fmt::Debug;

        /// Called when a child machine enters a failure state
        fn on_child_failure(&mut self, child_id: &str, error: &Self::Error) -> SupervisorAction;
    }

    /// What a supervisor does when a child fails
    #[derive(Debug, Clone)]
    pub enum SupervisorAction {
        /// Restart the child machine from its initial state
        Restart,
        /// Stop the child and propagate the error up
        Escalate,
        /// Ignore the failure and continue
        Ignore,
    }

    /// Message envelope for cross-boundary communication
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Envelope<T: Serialize> {
        pub source: String,
        pub target: String,
        pub payload: T,
        pub correlation_id: Option<String>,
    }

    impl<T: Serialize> Envelope<T> {
        pub fn new(source: impl Into<String>, target: impl Into<String>, payload: T) -> Self {
            Self {
                source: source.into(),
                target: target.into(),
                payload,
                correlation_id: None,
            }
        }

        pub fn with_correlation(mut self, id: impl Into<String>) -> Self {
            self.correlation_id = Some(id.into());
            self
        }
    }

    #[derive(Debug, Clone)]
    pub struct ChildHandle {
        pub id: String,
    }

    #[derive(Debug, Clone, Copy, Default)]
    pub enum RestartStrategy {
        #[default]
        OneForOne,
        OneForAll,
        RestForOne,
    }

    #[derive(Debug, Clone)]
    pub struct SupervisorRuntime {
        tasks: Arc<Mutex<JoinSet<Result<(), String>>>>,
        pending_spawns: Arc<AtomicUsize>,
        strategy: RestartStrategy,
    }

    impl Default for SupervisorRuntime {
        fn default() -> Self {
            Self::new()
        }
    }

    impl SupervisorRuntime {
        pub fn new() -> Self {
            Self::with_strategy(RestartStrategy::OneForOne)
        }

        pub fn with_strategy(strategy: RestartStrategy) -> Self {
            Self {
                tasks: Arc::new(Mutex::new(JoinSet::new())),
                pending_spawns: Arc::new(AtomicUsize::new(0)),
                strategy,
            }
        }

        pub fn spawn_named<F>(&self, id: impl Into<String>, fut: F) -> ChildHandle
        where
            F: Future<Output = Result<(), String>> + Send + 'static,
        {
            let id = id.into();
            let task_id = id.clone();
            if let Ok(mut tasks) = self.tasks.try_lock() {
                tasks.spawn(fut);
            } else {
                self.pending_spawns.fetch_add(1, Ordering::SeqCst);
                let tasks = self.tasks.clone();
                let pending_spawns = self.pending_spawns.clone();
                tokio::spawn(async move {
                    tasks.lock().await.spawn(fut);
                    pending_spawns.fetch_sub(1, Ordering::SeqCst);
                });
            }
            ChildHandle { id: task_id }
        }

        pub async fn join_next(&self) -> Option<Result<(), String>> {
            loop {
                let next = self.tasks.lock().await.join_next().await;
                match next {
                    Some(Ok(inner)) => return Some(inner),
                    Some(Err(join_err)) => {
                        return Some(Err(format!("task join error: {join_err}")))
                    }
                    None if self.pending_spawns.load(Ordering::SeqCst) == 0 => return None,
                    None => tokio::task::yield_now().await,
                }
            }
        }

        pub fn strategy(&self) -> RestartStrategy {
            self.strategy
        }

        pub fn restart_scope(
            &self,
            failed_child_index: usize,
            child_count: usize,
        ) -> std::ops::Range<usize> {
            match self.strategy {
                RestartStrategy::OneForOne => {
                    failed_child_index..failed_child_index.saturating_add(1)
                }
                RestartStrategy::OneForAll => 0..child_count,
                RestartStrategy::RestForOne => failed_child_index..child_count,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::prelude::{RestartStrategy, SupervisorRuntime};
    use std::time::Duration;

    #[test]
    fn restart_scope_matches_strategy() {
        let one_for_one = SupervisorRuntime::with_strategy(RestartStrategy::OneForOne);
        assert_eq!(one_for_one.restart_scope(2, 5), 2..3);

        let one_for_all = SupervisorRuntime::with_strategy(RestartStrategy::OneForAll);
        assert_eq!(one_for_all.restart_scope(2, 5), 0..5);

        let rest_for_one = SupervisorRuntime::with_strategy(RestartStrategy::RestForOne);
        assert_eq!(rest_for_one.restart_scope(2, 5), 2..5);
    }

    #[tokio::test]
    async fn join_next_observes_immediately_spawned_task() {
        let runtime = SupervisorRuntime::new();
        runtime.spawn_named("worker-1", async { Ok::<(), String>(()) });

        let joined = tokio::time::timeout(Duration::from_secs(1), runtime.join_next())
            .await
            .expect("join_next should not hang");

        assert!(matches!(joined, Some(Ok(()))));
    }
}
