fn main() {
    for file in [
        "request_response.gu",
        "circuit_breaker.gu",
        "saga.gu",
        "retry.gu",
        "rate_limiter.gu",
        "health_check.gu",
    ] {
        println!("cargo:rerun-if-changed={}", file);
    }
}
