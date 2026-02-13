pub const REQUEST_RESPONSE: &str = include_str!("../request_response.gu");
pub const CIRCUIT_BREAKER: &str = include_str!("../circuit_breaker.gu");
pub const SAGA: &str = include_str!("../saga.gu");
pub const RETRY: &str = include_str!("../retry.gu");
pub const RATE_LIMITER: &str = include_str!("../rate_limiter.gu");
pub const HEALTH_CHECK: &str = include_str!("../health_check.gu");

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
