//! # Gust Standard Library
//!
//! A collection of reusable, production-ready Gust state machines that solve
//! common distributed systems and application patterns.
//!
//! Each machine is shipped as a `.gu` source string embedded at compile time.
//! You can feed these sources directly to the Gust compiler or include them
//! alongside your own `.gu` files.
//!
//! ## Available machines
//!
//! | Machine | Constant | Description |
//! |---------|----------|-------------|
//! | **CircuitBreaker** | [`CIRCUIT_BREAKER`] | Protects calls to external services by tracking failures and opening the circuit when a threshold is reached. Transitions through `Closed`, `Open`, and `HalfOpen` states. |
//! | **Retry** | [`RETRY`] | Implements retry-with-backoff logic. Tracks attempt counts, computes exponential delays with jitter, and caps at a maximum delay. |
//! | **Saga** | [`SAGA`] | Orchestrates a sequence of distributed steps with automatic compensation (rollback) on failure, following the saga pattern. |
//! | **RateLimiter** | [`RATE_LIMITER`] | Token-bucket rate limiter that transitions between `Available` and `Exhausted` states based on remaining tokens. |
//! | **HealthCheck** | [`HEALTH_CHECK`] | Models service health with `Healthy`, `Degraded`, and `Unhealthy` states, tracking consecutive failures before transitioning. |
//! | **RequestResponse** | [`REQUEST_RESPONSE`] | Models an async request lifecycle with `Pending`, `Completed`, `Failed`, and `TimedOut` states. |
//!
//! ## Usage
//!
//! Use [`all_sources`] to iterate over every machine source for bulk
//! compilation, or access individual constants directly:
//!
//! ```rust
//! let sources = gust_stdlib::all_sources();
//! assert_eq!(sources.len(), 6);
//!
//! // Each entry is a (filename, source_code) pair
//! for (filename, source) in &sources {
//!     println!("{filename}: {} bytes", source.len());
//! }
//! ```

/// The Gust source for the **RequestResponse** machine.
///
/// Models an async request lifecycle through `Pending`, `Completed`,
/// `Failed`, and `TimedOut` states. Generic over the request type `T`
/// and response type `R`.
pub const REQUEST_RESPONSE: &str = include_str!("../request_response.gu");

/// The Gust source for the **CircuitBreaker** machine.
///
/// Implements the circuit breaker pattern with three states:
/// - **Closed** -- requests pass through; failures are counted against a threshold.
/// - **Open** -- requests are blocked; a timeout controls when to probe again.
/// - **HalfOpen** -- a limited number of probe requests are allowed to test recovery.
///
/// Generic over `T` for the protected call's context type.
pub const CIRCUIT_BREAKER: &str = include_str!("../circuit_breaker.gu");

/// The Gust source for the **Saga** machine.
///
/// Orchestrates a multi-step workflow with compensation. States include
/// `Planning`, `Executing`, `Compensating`, and `Committed`. If any step
/// fails during execution, the machine transitions to `Compensating` and
/// rolls back previously completed steps in reverse order.
///
/// Generic over `S`, the type representing individual saga steps.
pub const SAGA: &str = include_str!("../saga.gu");

/// The Gust source for the **Retry** machine.
///
/// Provides configurable retry logic with exponential backoff and jitter.
/// States include `Ready`, `Attempting`, `Waiting`, `Succeeded`, and
/// `Failed`. Tracks attempt count, base and max delay, and jitter
/// percentage.
///
/// Generic over `T` for the value type returned on success.
pub const RETRY: &str = include_str!("../retry.gu");

/// The Gust source for the **RateLimiter** machine.
///
/// A token-bucket rate limiter with two states:
/// - **Available** -- tokens remain; requests can proceed.
/// - **Exhausted** -- no tokens left; a `retry_after_ms` value indicates
///   when tokens will be replenished.
///
/// Generic over `K` for the rate-limit key type.
pub const RATE_LIMITER: &str = include_str!("../rate_limiter.gu");

/// The Gust source for the **HealthCheck** machine.
///
/// Models service health monitoring with three states:
/// - **Healthy** -- the service is operating normally.
/// - **Degraded** -- some health checks are failing but the service is
///   partially functional; tracks a failure count.
/// - **Unhealthy** -- the service is down, with a reason string.
///
/// Generic over `T` for the health status payload type.
pub const HEALTH_CHECK: &str = include_str!("../health_check.gu");

/// Returns all standard library machine sources as an array of
/// `(filename, source_code)` pairs.
///
/// This is useful for bulk-compiling the entire standard library or
/// for tooling that needs to enumerate available machines.
///
/// # Examples
///
/// ```rust
/// let sources = gust_stdlib::all_sources();
/// assert_eq!(sources.len(), 6);
///
/// // Find a specific machine by filename
/// let circuit_breaker = sources.iter()
///     .find(|(name, _)| *name == "circuit_breaker.gu")
///     .expect("circuit_breaker.gu should exist");
/// assert!(circuit_breaker.1.contains("machine CircuitBreaker"));
/// ```
pub fn all_sources() -> [(&'static str, &'static str); 6] {
    [
        ("request_response.gu", REQUEST_RESPONSE),
        ("circuit_breaker.gu", CIRCUIT_BREAKER),
        ("saga.gu", SAGA),
        ("retry.gu", RETRY),
        ("rate_limiter.gu", RATE_LIMITER),
        ("health_check.gu", HEALTH_CHECK),
    ]
}
