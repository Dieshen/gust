//! Gust Runtime Library
//!
//! This crate provides the runtime support for compiled Gust programs.
//! Generated Rust code from .gu files imports `gust_runtime::prelude::*`.

pub mod prelude {
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
}
