use anyhow::{bail, Result};
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use rand::Rng;
use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use super::types::*;

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CircuitState {
    /// Circuit is closed, requests flow normally
    Closed,
    /// Circuit is open, requests are rejected immediately
    Open,
    /// Circuit is half-open, allowing a test request through
    HalfOpen,
}

/// Classification of API errors for per-error retry policies
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorKind {
    /// Rate limited (429)
    RateLimit,
    /// Server error (5xx)
    ServerError,
    /// Transient connection error
    Transient,
    /// Permanent error (should not retry)
    Permanent,
}

/// Retry configuration - all values configurable
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Number of consecutive failures before opening circuit
    pub circuit_threshold: u32,
    /// Seconds to wait in open state before transitioning to half-open
    pub circuit_cooldown_secs: u64,
    /// Seconds to wait in half-open state before closing or reopening
    pub half_open_test_secs: u64,
    /// Maximum number of retries for a single request
    pub max_retries: u32,
    /// Base delay in milliseconds for exponential backoff
    pub base_delay_ms: u64,
    /// Maximum delay in milliseconds (caps the exponential growth)
    pub max_delay_ms: u64,
    /// Jitter factor (0.0 to 1.0), applied as ±(delay * jitter_factor)
    pub jitter_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            circuit_threshold: 5,
            circuit_cooldown_secs: 60,
            half_open_test_secs: 30,
            max_retries: 3,
            base_delay_ms: 500,
            max_delay_ms: 30000,
            jitter_factor: 0.3,
        }
    }
}

static PROVIDER_STATES: RwLock<Option<HashMap<String, Arc<ProviderState>>>> = RwLock::new(None);

struct ProviderState {
    consecutive_failures: AtomicU32,
    circuit_open_until: AtomicU64,
    circuit_half_open_until: AtomicU64,
    #[cfg(test)]
    test_mode: bool,
}

impl ProviderState {
    fn new() -> Self {
        Self {
            consecutive_failures: AtomicU32::new(0),
            circuit_open_until: AtomicU64::new(0),
            circuit_half_open_until: AtomicU64::new(0),
            #[cfg(test)]
            test_mode: false,
        }
    }
}

fn get_provider_state(api_base: &str) -> Arc<ProviderState> {
    {
        let read = PROVIDER_STATES.read().unwrap();
        if let Some(map) = read.as_ref() {
            if let Some(state) = map.get(api_base) {
                return state.clone();
            }
        }
    }

    let mut write = PROVIDER_STATES.write().unwrap();
    let map = write.get_or_insert_with(HashMap::new);
    map.entry(api_base.to_string())
        .or_insert_with(|| Arc::new(ProviderState::new()))
        .clone()
}

/// Get the current circuit state for a provider
fn get_circuit_state(state: &ProviderState, config: &RetryConfig) -> CircuitState {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Check half-open state first
    let half_open_until = state.circuit_half_open_until.load(Ordering::Relaxed);
    if half_open_until > 0 {
        if now < half_open_until {
            return CircuitState::HalfOpen;
        }
        // Half-open test window expired without a request - stay open
        return CircuitState::Open;
    }

    // Check open state
    let open_until = state.circuit_open_until.load(Ordering::Relaxed);
    if open_until > 0 {
        if now < open_until {
            return CircuitState::Open;
        }
        // Cooldown expired - transition to half-open
        state
            .circuit_half_open_until
            .store(now + config.half_open_test_secs, Ordering::Relaxed);
        state.circuit_open_until.store(0, Ordering::Relaxed);
        return CircuitState::HalfOpen;
    }

    CircuitState::Closed
}

fn classify_error(err_str: &str) -> ErrorKind {
    let lower = err_str.to_lowercase();
    if lower.contains("429") {
        ErrorKind::RateLimit
    } else if lower.contains(" 5") || lower.starts_with('5') {
        ErrorKind::ServerError
    } else if lower.contains("eof")
        || lower.contains("connection closed")
        || lower.contains("broken pipe")
        || lower.contains("reset by peer")
        || lower.contains("timeout")
        || lower.contains("timed out")
    {
        ErrorKind::Transient
    } else {
        ErrorKind::Permanent
    }
}

fn record_success(state: &ProviderState) {
    state.consecutive_failures.store(0, Ordering::Relaxed);
    state.circuit_open_until.store(0, Ordering::Relaxed);
    state.circuit_half_open_until.store(0, Ordering::Relaxed);
}

fn record_failure(state: &ProviderState, api_base: &str, config: &RetryConfig) {
    let count = state.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;

    if count >= config.circuit_threshold {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        state
            .circuit_open_until
            .store(now + config.circuit_cooldown_secs, Ordering::Relaxed);
        state.circuit_half_open_until.store(0, Ordering::Relaxed);
        eprintln!(
            "[oai-runner] Circuit breaker OPEN for {} after {} consecutive failures. Cooling down for {}s.",
            api_base, count, config.circuit_cooldown_secs
        );
    }
}

fn record_half_open_success(state: &ProviderState) {
    // Successful test request - close the circuit
    state.consecutive_failures.store(0, Ordering::Relaxed);
    state.circuit_open_until.store(0, Ordering::Relaxed);
    state.circuit_half_open_until.store(0, Ordering::Relaxed);
    eprintln!("[oai-runner] Circuit breaker CLOSED for provider after successful half-open test.");
}

fn record_half_open_failure(state: &ProviderState, api_base: &str, config: &RetryConfig) {
    // Failed test request - reopen the circuit with fresh cooldown
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    state
        .circuit_open_until
        .store(now + config.circuit_cooldown_secs, Ordering::Relaxed);
    state.circuit_half_open_until.store(0, Ordering::Relaxed);
    eprintln!(
        "[oai-runner] Circuit breaker re-OPENED for {} after half-open test failure. Cooling down for {}s.",
        api_base, config.circuit_cooldown_secs
    );
}

/// Calculate exponential backoff delay with jitter
fn calculate_backoff_delay(attempt: u32, config: &RetryConfig) -> Duration {
    let base = config.base_delay_ms;
    let max_delay = config.max_delay_ms;

    // Exponential backoff: base * 2^attempt
    let exponential_delay = base * 2u64.pow(attempt as u32);

    // Cap at max_delay
    let capped_delay = exponential_delay.min(max_delay);

    // Apply jitter: ±(capped_delay * jitter_factor)
    let jitter_range = (capped_delay as f64 * config.jitter_factor) as i64;
    let mut rng = rand::thread_rng();
    let jitter: i64 = rng.gen_range(-jitter_range..=jitter_range);

    // Ensure delay is at least 0
    let final_delay = (capped_delay as i64 + jitter).max(0) as u64;
    Duration::from_millis(final_delay)
}

pub struct ApiClient {
    http: reqwest::Client,
    api_base: String,
    api_key: String,
    retry_config: RetryConfig,
}

impl ApiClient {
    pub fn new(api_base: String, api_key: String, timeout_secs: u64) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .expect("failed to build HTTP client");
        Self {
            http,
            api_base,
            api_key,
            retry_config: RetryConfig::default(),
        }
    }

    /// Create a new ApiClient with custom retry configuration
    pub fn with_retry_config(
        api_base: String,
        api_key: String,
        timeout_secs: u64,
        retry_config: RetryConfig,
    ) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .expect("failed to build HTTP client");
        Self {
            http,
            api_base,
            api_key,
            retry_config,
        }
    }

    pub async fn stream_chat(
        &self,
        request: &ChatRequest,
        on_text_chunk: &mut dyn FnMut(&str),
    ) -> Result<(ChatMessage, Option<UsageInfo>)> {
        let state = get_provider_state(&self.api_base);
        let config = &self.retry_config;

        // Check circuit breaker
        let circuit_state = get_circuit_state(&state, config);
        if circuit_state == CircuitState::Open {
            bail!("Circuit breaker is open for {} — too many consecutive API failures. Waiting for cooldown.", self.api_base);
        }

        let url = format!("{}/chat/completions", self.api_base);

        let mut last_err = None;
        let mut emitted_any = false;

        for attempt in 0..=config.max_retries {
            if attempt > 0 {
                if emitted_any {
                    // If we already sent chunks to the user, retrying the whole request
                    // will lead to duplicate output. Better to fail or implement resume.
                    // OpenAI-style APIs usually don't support resume mid-stream.
                    return Err(last_err.unwrap_or_else(|| {
                        anyhow::anyhow!("Stream interrupted after emitting content")
                    }));
                }

                let delay = calculate_backoff_delay(attempt, config);
                eprintln!(
                    "[oai-runner] Retry {}/{}: waiting {:?}",
                    attempt, config.max_retries, delay
                );
                tokio::time::sleep(delay).await;
            }

            // In half-open state, only allow one test request
            if attempt > 0 && circuit_state == CircuitState::HalfOpen {
                bail!("Circuit breaker is in half-open state — waiting for next cooldown period to retry.");
            }

            let mut chunk_interceptor = |chunk: &str| {
                emitted_any = true;
                on_text_chunk(chunk);
            };

            match self.do_stream(&url, request, &mut chunk_interceptor).await {
                Ok(result) => {
                    // Success - update circuit state based on previous state
                    if circuit_state == CircuitState::HalfOpen {
                        record_half_open_success(&state);
                    } else {
                        record_success(&state);
                    }
                    return Ok(result);
                }
                Err(e) => {
                    let err_str = e.to_string();
                    let error_kind = classify_error(&err_str);

                    // Decide if we should retry based on error kind
                    let should_retry = match error_kind {
                        ErrorKind::RateLimit => true,
                        ErrorKind::ServerError => attempt < config.max_retries,
                        ErrorKind::Transient => attempt < config.max_retries,
                        ErrorKind::Permanent => false,
                    };

                    if should_retry {
                        // Record failure for circuit breaker
                        if circuit_state == CircuitState::HalfOpen {
                            record_half_open_failure(&state, &self.api_base, config);
                            return Err(e);
                        } else {
                            record_failure(&state, &self.api_base, config);
                        }

                        let reason = match error_kind {
                            ErrorKind::RateLimit => "rate limited (429)",
                            ErrorKind::ServerError => "server error (5xx)",
                            ErrorKind::Transient => "transient connection error",
                            ErrorKind::Permanent => unreachable!(),
                        };
                        eprintln!(
                            "[oai-runner] Retry {}/{} ({}): {}",
                            attempt + 1,
                            config.max_retries,
                            config.max_retries - attempt,
                            reason
                        );
                        last_err = Some(e);
                        continue;
                    }

                    // Permanent error - record and return
                    if circuit_state == CircuitState::HalfOpen {
                        record_half_open_failure(&state, &self.api_base, config);
                    } else {
                        record_failure(&state, &self.api_base, config);
                    }
                    return Err(e);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| {
            anyhow::anyhow!("stream_chat failed after {} retries", config.max_retries)
        }))
    }

    async fn do_stream(
        &self,
        url: &str,
        request: &ChatRequest,
        on_text_chunk: &mut dyn FnMut(&str),
    ) -> Result<(ChatMessage, Option<UsageInfo>)> {
        let resp = self
            .http
            .post(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            if status.as_u16() == 429 {
                if let Some(retry_after) = resp.headers().get("retry-after") {
                    if let Ok(secs) = retry_after.to_str().unwrap_or("0").parse::<u64>() {
                        let wait = secs.min(120);
                        eprintln!("[oai-runner] Rate limited. Retry-After: {}s", wait);
                        tokio::time::sleep(Duration::from_secs(wait)).await;
                    }
                }
            }
            let body = resp.text().await.unwrap_or_default();
            bail!(
                "API returned {} {}: {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or("Unknown"),
                body
            );
        }

        let mut content = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut usage: Option<UsageInfo> = None;

        let mut stream = resp.bytes_stream().eventsource();

        while let Some(event_result) = stream.next().await {
            let event = match event_result {
                Ok(event) => event,
                Err(e) => {
                    // If we fail mid-stream, we should probably return an error so stream_chat can decide to retry
                    bail!("SSE stream error: {}", e);
                }
            };

            if event.data == "[DONE]" {
                std::io::stdout().flush().ok();
                let msg = ChatMessage {
                    role: "assistant".to_string(),
                    content: if content.is_empty() {
                        None
                    } else {
                        Some(content)
                    },
                    tool_calls: if tool_calls.is_empty() {
                        None
                    } else {
                        Some(tool_calls)
                    },
                    tool_call_id: None,
                };
                return Ok((msg, usage));
            }

            let parsed: StreamChunk = match serde_json::from_str(&event.data) {
                Ok(c) => c,
                Err(_) => continue,
            };

            if let Some(u) = parsed.usage {
                usage = Some(u);
            }

            // Only care about the first choice for agent loop
            if let Some(choice) = parsed.choices.first() {
                if let Some(text) = &choice.delta.content {
                    content.push_str(text);
                    on_text_chunk(text);
                }

                if let Some(tc_deltas) = &choice.delta.tool_calls {
                    for tc_delta in tc_deltas {
                        let idx = tc_delta.index;

                        while tool_calls.len() <= idx {
                            tool_calls.push(ToolCall {
                                id: String::new(),
                                type_: "function".to_string(),
                                function: FunctionCall {
                                    name: String::new(),
                                    arguments: String::new(),
                                },
                            });
                        }

                        if let Some(id) = &tc_delta.id {
                            tool_calls[idx].id = id.clone();
                        }
                        if let Some(fc) = &tc_delta.function {
                            if let Some(name) = &fc.name {
                                tool_calls[idx].function.name = name.clone();
                            }
                            if let Some(args) = &fc.arguments {
                                tool_calls[idx].function.arguments.push_str(args);
                            }
                        }
                    }
                }
            }
        }

        std::io::stdout().flush().ok();
        let msg = ChatMessage {
            role: "assistant".to_string(),
            content: if content.is_empty() {
                None
            } else {
                Some(content)
            },
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
            tool_call_id: None,
        };
        Ok((msg, usage))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_state_transitions() {
        let state = Arc::new(ProviderState::new());
        let config = RetryConfig::default();

        // Initially closed
        assert_eq!(get_circuit_state(&state, &config), CircuitState::Closed);

        // Simulate failures to open circuit
        for _ in 0..config.circuit_threshold {
            let count = state.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
            assert!(count <= config.circuit_threshold);
        }
        record_failure(&state, "test", &config);

        // Should now be open
        let circuit_state = get_circuit_state(&state, &config);
        assert!(
            circuit_state == CircuitState::Open || circuit_state == CircuitState::HalfOpen,
            "Expected Open or HalfOpen, got {:?}",
            circuit_state
        );
    }

    #[test]
    fn test_error_classification() {
        assert_eq!(classify_error("429 Too Many Requests"), ErrorKind::RateLimit);
        assert_eq!(classify_error("500 Internal Server Error"), ErrorKind::ServerError);
        assert_eq!(classify_error("502 Bad Gateway"), ErrorKind::ServerError);
        assert_eq!(classify_error("Connection reset by peer"), ErrorKind::Transient);
        assert_eq!(classify_error("EOF during negotiation"), ErrorKind::Transient);
        assert_eq!(classify_error("connection closed"), ErrorKind::Transient);
        assert_eq!(classify_error("broken pipe"), ErrorKind::Transient);
        assert_eq!(classify_error("Timeout was reached"), ErrorKind::Transient);
        assert_eq!(
            classify_error("401 Unauthorized"),
            ErrorKind::Permanent
        );
        assert_eq!(classify_error("400 Bad Request"), ErrorKind::Permanent);
    }

    #[test]
    fn test_backoff_calculation() {
        let config = RetryConfig {
            base_delay_ms: 500,
            max_delay_ms: 30000,
            jitter_factor: 0.0, // Disable jitter for deterministic test
            ..Default::default()
        };

        // Attempt 0: 500 * 2^0 = 500
        let delay0 = calculate_backoff_delay(0, &config);
        assert_eq!(delay0, Duration::from_millis(500));

        // Attempt 1: 500 * 2^1 = 1000
        let delay1 = calculate_backoff_delay(1, &config);
        assert_eq!(delay1, Duration::from_millis(1000));

        // Attempt 2: 500 * 2^2 = 2000
        let delay2 = calculate_backoff_delay(2, &config);
        assert_eq!(delay2, Duration::from_millis(2000));

        // Attempt 10: Would be 512000, but capped at 30000
        let delay10 = calculate_backoff_delay(10, &config);
        assert_eq!(delay10, Duration::from_millis(30000));
    }

    #[test]
    fn test_backoff_with_jitter() {
        let config = RetryConfig {
            base_delay_ms: 1000,
            max_delay_ms: 10000,
            jitter_factor: 0.1, // 10% jitter
            ..Default::default()
        };

        // Run multiple times and check range
        let delays: Vec<Duration> = (0..10)
            .map(|i| calculate_backoff_delay(i, &config))
            .collect();

        // Jitter should cause variation
        let unique_delays: std::collections::HashSet<_> = delays.iter().collect();
        assert!(
            unique_delays.len() > 1,
            "Expected variation with jitter, got same delays: {:?}",
            delays
        );

        // All delays should be within expected range
        for delay in &delays {
            let ms = delay.as_millis() as i64;
            assert!(
                ms >= 0 && ms <= 11000,
                "Delay {}ms out of expected range [0, 11000]",
                ms
            );
        }
    }

    #[test]
    fn test_retry_config_defaults() {
        let config = RetryConfig::default();
        assert_eq!(config.circuit_threshold, 5);
        assert_eq!(config.circuit_cooldown_secs, 60);
        assert_eq!(config.half_open_test_secs, 30);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_delay_ms, 500);
        assert_eq!(config.jitter_factor, 0.3);
    }

    #[test]
    fn test_half_open_test_secs_configurable() {
        let config = RetryConfig {
            half_open_test_secs: 45,
            ..Default::default()
        };

        // Verify the config value is correctly set
        assert_eq!(config.half_open_test_secs, 45);

        // The half_open_test_secs is used in get_circuit_state when transitioning
        // from Open to HalfOpen. We can verify the logic by checking that
        // after record_failure, the open_until timestamp is calculated correctly.
        let state = Arc::new(ProviderState::new());

        // Simulate enough failures to open the circuit
        for _ in 0..config.circuit_threshold {
            state.consecutive_failures.fetch_add(1, Ordering::Relaxed);
        }
        record_failure(&state, "test", &config);

        // After record_failure, the circuit should be in Open state (cooling down)
        // half_open_test_secs will be used when transitioning to HalfOpen
        let circuit_state = get_circuit_state(&state, &config);
        assert!(
            circuit_state == CircuitState::Open || circuit_state == CircuitState::HalfOpen,
            "Expected Open or HalfOpen after failure record, got {:?}",
            circuit_state
        );
    }
}
