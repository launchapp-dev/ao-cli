use std::collections::HashMap;

use animus_plugin_protocol::RpcError;
use anyhow::{anyhow, Result};
use serde_json::Value;

use crate::PluginHost;

/// Subject-kind registration parsed from a plugin's declared
/// `subject_kinds`. A pattern ending in `.*` matches any kind whose dotted
/// prefix matches everything before the trailing `*`.
#[derive(Debug, Clone)]
struct KindPattern {
    /// Raw pattern as declared by the plugin (e.g. `"task"`, `"task.tracked"`,
    /// or `"task.*"`).
    raw: String,
    /// Pattern prefix excluding any trailing `*` (e.g. `"task."` for the glob
    /// `"task.*"`, or the full string for exact matches).
    prefix: String,
    /// Whether the pattern is a glob (`true`) or an exact match (`false`).
    is_glob: bool,
}

impl KindPattern {
    fn parse(raw: &str) -> Self {
        if let Some(stem) = raw.strip_suffix(".*") {
            KindPattern { raw: raw.to_string(), prefix: format!("{stem}."), is_glob: true }
        } else {
            KindPattern { raw: raw.to_string(), prefix: raw.to_string(), is_glob: false }
        }
    }

    fn matches(&self, kind: &str) -> bool {
        if self.is_glob {
            kind.starts_with(&self.prefix) && kind.len() > self.prefix.len()
        } else {
            self.prefix == kind
        }
    }
}

pub struct SubjectRouter {
    /// Exact-kind registrations keyed by the declared kind string.
    exact_kinds: HashMap<String, String>,
    /// Glob registrations stored as (pattern, plugin_name) pairs.
    glob_kinds: Vec<(KindPattern, String)>,
    hosts: HashMap<String, PluginHost>,
}

impl SubjectRouter {
    pub async fn from_initialized_hosts(hosts: HashMap<String, PluginHost>) -> Result<Self> {
        let mut exact_kinds: HashMap<String, String> = HashMap::new();
        let mut glob_kinds: Vec<(KindPattern, String)> = Vec::new();
        let names = hosts.keys().cloned().collect::<Vec<_>>();

        for name in names {
            let host = hosts.get(&name).ok_or_else(|| anyhow!("plugin host disappeared during routing setup"))?;
            let result = host.handshake().await?;
            for raw_kind in result.capabilities.subject_kinds {
                let pattern = KindPattern::parse(&raw_kind);
                if pattern.is_glob {
                    // Reject duplicate glob registrations on the same prefix
                    // to keep precedence deterministic. Different prefix
                    // lengths are fine — longest wins at resolve time.
                    if let Some((existing_pattern, existing_name)) =
                        glob_kinds.iter().find(|(p, _)| p.prefix == pattern.prefix && p.is_glob)
                    {
                        return Err(anyhow!(
                            "duplicate subject kind glob '{}' claimed by '{}' and '{}'",
                            existing_pattern.raw,
                            existing_name,
                            name
                        ));
                    }
                    glob_kinds.push((pattern, name.clone()));
                } else if let Some(existing) = exact_kinds.get(&pattern.raw) {
                    return Err(anyhow!(
                        "duplicate subject kind '{}' claimed by '{}' and '{}'",
                        pattern.raw,
                        existing,
                        name
                    ));
                } else {
                    exact_kinds.insert(pattern.raw, name.clone());
                }
            }
        }

        Ok(Self { exact_kinds, glob_kinds, hosts })
    }

    /// Resolve the plugin name responsible for `kind`.
    ///
    /// Precedence rules:
    ///
    /// 1. Exact-match registration (e.g. `task.tracked` beats `task.*`).
    /// 2. Longest matching glob prefix wins (`task.tracked.*` beats `task.*`
    ///    when resolving `task.tracked.foo`).
    /// 3. If two globs of equal prefix length both match, the resolution is
    ///    ambiguous and `None` is returned. (Equal-prefix duplicates are
    ///    already rejected at registration time, so this is defensive.)
    pub fn plugin_for_kind(&self, kind: &str) -> Option<&str> {
        if let Some(name) = self.exact_kinds.get(kind) {
            return Some(name.as_str());
        }
        let mut best: Option<(usize, &str)> = None;
        let mut ambiguous = false;
        for (pattern, plugin) in &self.glob_kinds {
            if !pattern.matches(kind) {
                continue;
            }
            let len = pattern.prefix.len();
            match best {
                None => best = Some((len, plugin.as_str())),
                Some((cur_len, _cur_plugin)) => {
                    if len > cur_len {
                        best = Some((len, plugin.as_str()));
                        ambiguous = false;
                    } else if len == cur_len {
                        ambiguous = true;
                    }
                }
            }
        }
        if ambiguous {
            None
        } else {
            best.map(|(_, plugin)| plugin)
        }
    }

    pub fn is_subject_method(&self, method: &str) -> bool {
        method.split('/').next().is_some_and(|kind| self.plugin_for_kind(kind).is_some())
    }

    pub async fn route_call(&self, method: &str, params: Option<Value>) -> Result<Value, RpcError> {
        let kind = method.split('/').next().unwrap_or_default();
        let Some(plugin_name) = self.plugin_for_kind(kind) else {
            return Err(RpcError {
                code: animus_plugin_protocol::error_codes::METHOD_NOT_FOUND,
                message: format!("no subject backend registered for kind '{kind}'"),
                data: None,
            });
        };
        let Some(host) = self.hosts.get(plugin_name) else {
            return Err(RpcError {
                code: animus_plugin_protocol::error_codes::INTERNAL_ERROR,
                message: format!("subject backend '{plugin_name}' is not available"),
                data: None,
            });
        };

        host.request(method, params).await
    }

    pub async fn resolve_subject(&self, subject_kind: &str, subject_id: &str) -> Result<Value, RpcError> {
        self.route_call(&format!("{subject_kind}/get"), Some(serde_json::json!({ "id": subject_id }))).await
    }
}

#[cfg(test)]
mod tests {
    use animus_plugin_protocol::{InitializeResult, PluginCapabilities, PluginInfo, RpcRequest, RpcResponse};
    use tokio::io::{duplex, AsyncBufReadExt, AsyncWriteExt, BufReader};

    use super::*;

    async fn subject_host(name: &str, subject_kinds: Vec<&str>) -> PluginHost {
        let (host_reader, mut plugin_writer) = duplex(8192);
        let (plugin_reader, host_writer) = duplex(8192);
        let name_for_task = name.to_string();
        let kinds = subject_kinds.into_iter().map(ToOwned::to_owned).collect::<Vec<_>>();

        tokio::spawn(async move {
            let mut reader = BufReader::new(plugin_reader);
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).await.expect("read line") == 0 {
                    break;
                }
                let request: RpcRequest = serde_json::from_str(line.trim()).expect("parse request");
                let response = match request.method.as_str() {
                    "initialize" => RpcResponse::ok(
                        request.id,
                        serde_json::json!(InitializeResult {
                            protocol_version: "1.0.0".to_string(),
                            plugin_info: PluginInfo {
                                name: name_for_task.clone(),
                                version: "0.1.0".to_string(),
                                plugin_kind: "subject_backend".to_string(),
                                description: None,
                            },
                            capabilities: PluginCapabilities {
                                subject_kinds: kinds.clone(),
                                methods: kinds.iter().map(|kind| format!("{kind}/get")).collect(),
                                ..PluginCapabilities::default()
                            },
                        }),
                    ),
                    "initialized" => continue,
                    method => RpcResponse::ok(request.id, serde_json::json!({ "method": method })),
                };
                let mut encoded = serde_json::to_string(&response).expect("encode response");
                encoded.push('\n');
                plugin_writer.write_all(encoded.as_bytes()).await.expect("write response");
            }
        });

        PluginHost::from_streams(name, host_reader, host_writer)
    }

    #[tokio::test]
    async fn routes_by_subject_kind_prefix() {
        let mut hosts = HashMap::new();
        hosts.insert("tasks".to_string(), subject_host("tasks", vec!["task"]).await);
        let router = SubjectRouter::from_initialized_hosts(hosts).await.expect("router");

        let result = router.route_call("task/get", Some(serde_json::json!({ "id": "TASK-1" }))).await.expect("route");

        assert_eq!(result["method"], "task/get");
        assert_eq!(router.plugin_for_kind("task"), Some("tasks"));
    }

    #[tokio::test]
    async fn glob_kind_matches_dotted_subkinds() {
        let mut hosts = HashMap::new();
        hosts.insert("all-tasks".to_string(), subject_host("all-tasks", vec!["task.*"]).await);
        let router = SubjectRouter::from_initialized_hosts(hosts).await.expect("router");

        // Glob matches both kinds.
        assert_eq!(router.plugin_for_kind("task.tracked"), Some("all-tasks"));
        assert_eq!(router.plugin_for_kind("task.untracked"), Some("all-tasks"));
        // The glob does not match the bare prefix itself.
        assert_eq!(router.plugin_for_kind("task"), None);
        // And the route_call path also accepts the dotted method.
        let result = router.route_call("task.tracked/list", Some(serde_json::json!({}))).await.expect("route");
        assert_eq!(result["method"], "task.tracked/list");
    }

    #[tokio::test]
    async fn exact_match_beats_glob() {
        let mut hosts = HashMap::new();
        hosts.insert("any-task".to_string(), subject_host("any-task", vec!["task.*"]).await);
        hosts.insert("tracked".to_string(), subject_host("tracked", vec!["task.tracked"]).await);
        let router = SubjectRouter::from_initialized_hosts(hosts).await.expect("router");

        assert_eq!(router.plugin_for_kind("task.tracked"), Some("tracked"));
        assert_eq!(router.plugin_for_kind("task.untracked"), Some("any-task"));
    }

    #[tokio::test]
    async fn longest_glob_prefix_wins() {
        let mut hosts = HashMap::new();
        hosts.insert("any-task".to_string(), subject_host("any-task", vec!["task.*"]).await);
        hosts.insert("nested".to_string(), subject_host("nested", vec!["task.tracked.*"]).await);
        let router = SubjectRouter::from_initialized_hosts(hosts).await.expect("router");

        assert_eq!(router.plugin_for_kind("task.tracked.high"), Some("nested"));
        assert_eq!(router.plugin_for_kind("task.untracked.low"), Some("any-task"));
    }

    #[tokio::test]
    async fn duplicate_glob_kinds_are_rejected_at_registration() {
        let mut hosts = HashMap::new();
        hosts.insert("a".to_string(), subject_host("a", vec!["task.*"]).await);
        hosts.insert("b".to_string(), subject_host("b", vec!["task.*"]).await);

        let outcome = SubjectRouter::from_initialized_hosts(hosts).await;
        let err = match outcome {
            Err(e) => e,
            Ok(_) => panic!("router should reject duplicate glob kinds"),
        };
        assert!(format!("{err:?}").contains("duplicate subject kind glob"));
    }
}
