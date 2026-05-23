use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct SupervisorConfig {
    pub max_restarts_per_window: u32,
    pub window_duration: Duration,
    pub disable_cooldown: Duration,
}

impl Default for SupervisorConfig {
    // WHY 3-per-60s + 5min cooldown: a healthy plugin should never restart at
    // all; a single crash + retry is acceptable; three crashes within a minute
    // indicates a structurally broken plugin (bad binary, env issue, OOM loop)
    // that will only get worse if we keep respawning it. The 5-minute cooldown
    // matches the typical CI/dev iteration cycle so an operator who fixes the
    // root cause doesn't have to restart the daemon to re-enable the plugin.
    fn default() -> Self {
        Self {
            max_restarts_per_window: 3,
            window_duration: Duration::from_secs(60),
            disable_cooldown: Duration::from_secs(300),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    #[error("plugin '{plugin}' exceeded restart budget ({count} restarts in {window:?}); marked disabled")]
    TooManyRestarts { plugin: String, count: u32, window: Duration },
    #[error("plugin '{plugin}' is currently disabled; retry after {retry_after:?}")]
    PluginDisabled { plugin: String, retry_after: Duration },
}

pub struct PluginSupervisor {
    plugin_name: String,
    restart_count: AtomicU32,
    window_start: Mutex<Instant>,
    disabled_until: Mutex<Option<Instant>>,
    config: SupervisorConfig,
}

impl PluginSupervisor {
    pub fn new(plugin_name: impl Into<String>, config: SupervisorConfig) -> Self {
        Self {
            plugin_name: plugin_name.into(),
            restart_count: AtomicU32::new(0),
            window_start: Mutex::new(Instant::now()),
            disabled_until: Mutex::new(None),
            config,
        }
    }

    pub fn with_defaults(plugin_name: impl Into<String>) -> Self {
        Self::new(plugin_name, SupervisorConfig::default())
    }

    pub fn plugin_name(&self) -> &str {
        &self.plugin_name
    }

    pub fn config(&self) -> &SupervisorConfig {
        &self.config
    }

    pub fn is_disabled(&self) -> bool {
        let mut guard = self.disabled_until.lock().expect("disabled_until mutex poisoned");
        match *guard {
            Some(deadline) if Instant::now() < deadline => true,
            Some(_) => {
                *guard = None;
                self.restart_count.store(0, Ordering::SeqCst);
                *self.window_start.lock().expect("window_start mutex poisoned") = Instant::now();
                false
            }
            None => false,
        }
    }

    pub fn disabled_remaining(&self) -> Option<Duration> {
        let guard = self.disabled_until.lock().expect("disabled_until mutex poisoned");
        match *guard {
            Some(deadline) => {
                let now = Instant::now();
                if now < deadline {
                    Some(deadline - now)
                } else {
                    None
                }
            }
            None => None,
        }
    }

    pub fn record_restart(&self) -> Result<(), SupervisorError> {
        if let Some(retry_after) = self.disabled_remaining() {
            if !self.is_disabled() {
                // cooldown elapsed between calls; fall through.
            } else {
                return Err(SupervisorError::PluginDisabled { plugin: self.plugin_name.clone(), retry_after });
            }
        }

        let mut window_start = self.window_start.lock().expect("window_start mutex poisoned");
        let now = Instant::now();
        if now.duration_since(*window_start) > self.config.window_duration {
            *window_start = now;
            self.restart_count.store(0, Ordering::SeqCst);
        }
        drop(window_start);

        let new_count = self.restart_count.fetch_add(1, Ordering::SeqCst) + 1;
        if new_count > self.config.max_restarts_per_window {
            let mut disabled = self.disabled_until.lock().expect("disabled_until mutex poisoned");
            *disabled = Some(Instant::now() + self.config.disable_cooldown);
            return Err(SupervisorError::TooManyRestarts {
                plugin: self.plugin_name.clone(),
                count: new_count,
                window: self.config.window_duration,
            });
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn restart_count_for_test(&self) -> u32 {
        self.restart_count.load(Ordering::SeqCst)
    }

    #[doc(hidden)]
    pub fn restart_count_for_test_public(&self) -> u32 {
        self.restart_count.load(Ordering::SeqCst)
    }

    #[doc(hidden)]
    pub fn force_disable_for_test_public(&self, cooldown: Duration) {
        let mut disabled = self.disabled_until.lock().expect("disabled_until mutex poisoned");
        *disabled = Some(Instant::now() + cooldown);
    }
}

/// Classify an `RpcError` returned from a plugin request. Codes in the
/// well-known JSON-RPC range (-32700..-32600) are structured errors emitted
/// BY THE PLUGIN; the plugin process is still alive and a retry would just
/// re-elicit the same error. Anything else (typically our internal
/// `INTERNAL_ERROR` wrapping a `ConnectionLost` / I/O error) means the
/// process died mid-call and a fresh spawn could succeed.
pub fn is_structured_jsonrpc_error(code: i32) -> bool {
    (-32700..=-32600).contains(&code)
}

/// True when an `RpcError` from the plugin host looks like the plugin process
/// died or the transport broke mid-call. Pairs with
/// `is_structured_jsonrpc_error`: the wrapping host returns `INTERNAL_ERROR`
/// for both genuine plugin-side internal errors AND for our own
/// `HostError::ConnectionLost`/`HostError::Timeout`, so we have to inspect
/// the message to tell them apart. Used by the dispatcher to decide whether
/// a retry-once is safe.
pub fn is_death_like_error(code: i32, message: &str) -> bool {
    if !is_structured_jsonrpc_error(code) {
        return true;
    }
    let lower = message.to_ascii_lowercase();
    lower.contains("connection lost") || lower.contains("connection closed") || lower.contains("broken pipe")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> SupervisorConfig {
        SupervisorConfig {
            max_restarts_per_window: 3,
            window_duration: Duration::from_secs(60),
            disable_cooldown: Duration::from_millis(200),
        }
    }

    #[test]
    fn supervisor_records_restart_within_window() {
        let sup = PluginSupervisor::new("plug", test_config());
        sup.record_restart().expect("first restart ok");
        sup.record_restart().expect("second restart ok");
        sup.record_restart().expect("third restart ok");
        assert_eq!(sup.restart_count_for_test(), 3);
        assert!(!sup.is_disabled());
    }

    #[test]
    fn supervisor_disables_plugin_after_3_restarts_in_60s() {
        let sup = PluginSupervisor::new("plug", test_config());
        sup.record_restart().expect("1");
        sup.record_restart().expect("2");
        sup.record_restart().expect("3");
        let err = sup.record_restart().expect_err("4th restart must fail");
        match err {
            SupervisorError::TooManyRestarts { plugin, count, .. } => {
                assert_eq!(plugin, "plug");
                assert_eq!(count, 4);
            }
            SupervisorError::PluginDisabled { .. } => panic!("expected TooManyRestarts, got PluginDisabled"),
        }
        assert!(sup.is_disabled(), "supervisor must be disabled after exceeding budget");
    }

    #[test]
    fn supervisor_re_enables_after_cooldown() {
        let sup = PluginSupervisor::new("plug", test_config());
        sup.record_restart().expect("1");
        sup.record_restart().expect("2");
        sup.record_restart().expect("3");
        let _ = sup.record_restart().expect_err("4th must trip disable");
        assert!(sup.is_disabled());
        std::thread::sleep(Duration::from_millis(250));
        assert!(!sup.is_disabled(), "cooldown should clear the disabled flag");
        assert_eq!(sup.restart_count_for_test(), 0, "counter resets on re-enable");
        sup.record_restart().expect("first restart in new window ok");
    }

    #[test]
    fn classify_death_like_errors() {
        // Internal error code with a connection-lost message => death-like.
        assert!(is_death_like_error(-32603, "plugin connection lost"));
        assert!(is_death_like_error(-32603, "broken pipe writing to plugin"));
        // Internal error code with a real plugin internal error => structured (no retry).
        assert!(!is_death_like_error(-32603, "plugin handler raised: KeyError"));
        // Codes outside well-known range => always death-like.
        assert!(is_death_like_error(-1, "anything"));
        // Other JSON-RPC well-known codes with plugin-side messages => structured.
        assert!(!is_death_like_error(-32602, "bad params"));
    }

    #[test]
    fn classify_jsonrpc_errors() {
        assert!(is_structured_jsonrpc_error(-32700));
        assert!(is_structured_jsonrpc_error(-32600));
        assert!(is_structured_jsonrpc_error(-32603));
        assert!(!is_structured_jsonrpc_error(-32003));
        assert!(!is_structured_jsonrpc_error(-32000));
        assert!(!is_structured_jsonrpc_error(0));
    }
}
