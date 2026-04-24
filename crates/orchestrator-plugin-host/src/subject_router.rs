use std::collections::HashMap;

use anyhow::{anyhow, Result};
use orchestrator_plugin_protocol::RpcError;
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::process::{ChildStdin, ChildStdout};

use crate::PluginHost;

pub struct SubjectRouter<R = ChildStdout, W = ChildStdin> {
    kind_to_plugin: HashMap<String, String>,
    hosts: HashMap<String, PluginHost<R, W>>,
}

impl<R, W> SubjectRouter<R, W>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    pub async fn from_initialized_hosts(mut hosts: HashMap<String, PluginHost<R, W>>) -> Result<Self> {
        let mut kind_to_plugin = HashMap::new();
        let names = hosts.keys().cloned().collect::<Vec<_>>();

        for name in names {
            let host = hosts.get_mut(&name).ok_or_else(|| anyhow!("plugin host disappeared during routing setup"))?;
            let result = host.handshake().await?;
            for kind in result.capabilities.subject_kinds {
                if let Some(existing) = kind_to_plugin.get(&kind) {
                    return Err(anyhow!("duplicate subject kind '{kind}' claimed by '{existing}' and '{name}'"));
                }
                kind_to_plugin.insert(kind, name.clone());
            }
        }

        Ok(Self { kind_to_plugin, hosts })
    }

    pub fn plugin_for_kind(&self, kind: &str) -> Option<&str> {
        self.kind_to_plugin.get(kind).map(String::as_str)
    }

    pub fn is_subject_method(&self, method: &str) -> bool {
        method.split('/').next().is_some_and(|kind| self.kind_to_plugin.contains_key(kind))
    }

    pub async fn route_call(&mut self, method: &str, params: Option<Value>) -> Result<Value, RpcError> {
        let kind = method.split('/').next().unwrap_or_default();
        let Some(plugin_name) = self.kind_to_plugin.get(kind) else {
            return Err(RpcError {
                code: orchestrator_plugin_protocol::error_codes::METHOD_NOT_FOUND,
                message: format!("no subject backend registered for kind '{kind}'"),
                data: None,
            });
        };
        let Some(host) = self.hosts.get_mut(plugin_name) else {
            return Err(RpcError {
                code: orchestrator_plugin_protocol::error_codes::INTERNAL_ERROR,
                message: format!("subject backend '{plugin_name}' is not available"),
                data: None,
            });
        };

        host.request(method, params).await
    }

    pub async fn resolve_subject(&mut self, subject_kind: &str, subject_id: &str) -> Result<Value, RpcError> {
        self.route_call(&format!("{subject_kind}/get"), Some(serde_json::json!({ "id": subject_id }))).await
    }
}

#[cfg(test)]
mod tests {
    use orchestrator_plugin_protocol::{InitializeResult, PluginCapabilities, PluginInfo, RpcRequest, RpcResponse};
    use tokio::io::{duplex, AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};

    use super::*;

    async fn subject_host(name: &str, subject_kinds: Vec<&str>) -> PluginHost<DuplexStream, DuplexStream> {
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
        let mut router = SubjectRouter::from_initialized_hosts(hosts).await.expect("router");

        let result = router.route_call("task/get", Some(serde_json::json!({ "id": "TASK-1" }))).await.expect("route");

        assert_eq!(result["method"], "task/get");
        assert_eq!(router.plugin_for_kind("task"), Some("tasks"));
    }
}
