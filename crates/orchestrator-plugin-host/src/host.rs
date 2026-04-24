use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Result};
use orchestrator_plugin_protocol::{
    error_codes, HealthCheckResult, HostCapabilities, HostInfo, InitializeParams, InitializeResult, RpcError,
    RpcNotification, RpcRequest, RpcResponse, PROTOCOL_VERSION,
};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tracing::{debug, warn};

use crate::StdioTransport;

pub struct PluginHost<R = ChildStdout, W = ChildStdin> {
    pub name: String,
    child: Option<Child>,
    transport: StdioTransport<R, W>,
    next_id: u64,
}

impl PluginHost<ChildStdout, ChildStdin> {
    pub async fn spawn(binary_path: &Path, args: &[&str]) -> Result<Self> {
        let name = binary_path.file_name().and_then(|value| value.to_str()).unwrap_or("plugin").to_string();
        let mut command = tokio::process::Command::new(binary_path);
        command
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = command.spawn()?;
        let stdin = child.stdin.take().ok_or_else(|| anyhow!("failed to take plugin stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("failed to take plugin stdout"))?;
        let stderr = child.stderr.take().ok_or_else(|| anyhow!("failed to take plugin stderr"))?;

        let stderr_plugin_name = name.clone();
        tokio::spawn(async move {
            let mut lines = tokio::io::BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                warn!(plugin = %stderr_plugin_name, "{}", line);
            }
        });

        Ok(Self { name, child: Some(child), transport: StdioTransport::new(stdout, stdin), next_id: 1 })
    }
}

impl<R, W> PluginHost<R, W>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    pub fn from_streams(name: impl Into<String>, reader: R, writer: W) -> Self {
        Self { name: name.into(), child: None, transport: StdioTransport::new(reader, writer), next_id: 1 }
    }

    pub fn next_request_id(&self) -> u64 {
        self.next_id
    }

    async fn send_and_receive(&mut self, id: u64, method: &str, params: Option<Value>) -> Result<RpcResponse> {
        self.transport.write_message(&RpcRequest::new(id, method, params)).await?;
        let expected_id = serde_json::json!(id);

        loop {
            let response = self
                .transport
                .read_message::<RpcResponse>()
                .await?
                .ok_or_else(|| anyhow!("plugin closed while waiting for response to '{method}'"))?;
            if response.id.as_ref() == Some(&expected_id) {
                return Ok(response);
            }
        }
    }

    fn take_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    pub async fn handshake(&mut self) -> Result<InitializeResult> {
        let params = InitializeParams {
            protocol_version: PROTOCOL_VERSION.to_string(),
            host_info: HostInfo { name: "ao".to_string(), version: env!("CARGO_PKG_VERSION").to_string() },
            capabilities: HostCapabilities { streaming: true, progress: true, cancellation: true },
        };

        let id = self.take_id();
        let response = self.send_and_receive(id, "initialize", Some(serde_json::to_value(params)?)).await?;
        if let Some(error) = response.error {
            return Err(anyhow!("plugin initialize failed ({}): {}", error.code, error.message));
        }

        let result: InitializeResult =
            serde_json::from_value(response.result.ok_or_else(|| anyhow!("plugin initialize returned no result"))?)?;
        self.notify("initialized", None).await?;
        debug!(plugin = %self.name, plugin_name = %result.plugin_info.name, "stdio plugin initialized");
        Ok(result)
    }

    pub async fn request(&mut self, method: impl Into<String>, params: Option<Value>) -> Result<Value, RpcError> {
        let method = method.into();
        let id = self.take_id();
        let response = self.send_and_receive(id, &method, params).await.map_err(|error| RpcError {
            code: error_codes::INTERNAL_ERROR,
            message: error.to_string(),
            data: None,
        })?;

        if let Some(error) = response.error {
            return Err(error);
        }

        Ok(response.result.unwrap_or(Value::Null))
    }

    pub async fn notify(&mut self, method: impl Into<String>, params: Option<Value>) -> Result<()> {
        self.transport.write_message(&RpcNotification::new(method, params)).await
    }

    pub async fn ping(&mut self) -> Result<()> {
        let id = self.take_id();
        let response = tokio::time::timeout(Duration::from_secs(2), self.send_and_receive(id, "$/ping", None))
            .await
            .map_err(|_| anyhow!("plugin ping timed out"))??;
        if let Some(error) = response.error {
            return Err(anyhow!("plugin ping failed ({}): {}", error.code, error.message));
        }
        Ok(())
    }

    pub async fn health_check(&mut self) -> Result<HealthCheckResult> {
        let result = tokio::time::timeout(Duration::from_secs(2), self.request("health/check", None))
            .await
            .map_err(|_| anyhow!("plugin health/check timed out"))?
            .map_err(|error| anyhow!("plugin health/check failed ({}): {}", error.code, error.message))?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn shutdown(mut self) -> Result<()> {
        let _ = self.request("shutdown", None).await;
        let _ = self.notify("exit", None).await;
        if let Some(mut child) = self.child.take() {
            let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use orchestrator_plugin_protocol::{PluginCapabilities, PluginInfo, RpcRequest, RpcResponse};
    use tokio::io::{duplex, AsyncBufReadExt, AsyncWriteExt, BufReader};

    use super::*;

    #[tokio::test]
    async fn handshake_sends_initialize_and_initialized() {
        let (host_reader, mut plugin_writer) = duplex(8192);
        let (plugin_reader, host_writer) = duplex(8192);

        tokio::spawn(async move {
            let mut reader = BufReader::new(plugin_reader);
            let mut line = String::new();
            reader.read_line(&mut line).await.expect("read initialize");
            let request: RpcRequest = serde_json::from_str(line.trim()).expect("parse initialize");
            assert_eq!(request.method, "initialize");

            let response = RpcResponse::ok(
                request.id,
                serde_json::json!(InitializeResult {
                    protocol_version: PROTOCOL_VERSION.to_string(),
                    plugin_info: PluginInfo {
                        name: "test".to_string(),
                        version: "0.1.0".to_string(),
                        plugin_kind: "custom".to_string(),
                    },
                    capabilities: PluginCapabilities::default(),
                }),
            );
            let mut encoded = serde_json::to_string(&response).expect("encode response");
            encoded.push('\n');
            plugin_writer.write_all(encoded.as_bytes()).await.expect("write response");

            line.clear();
            reader.read_line(&mut line).await.expect("read initialized");
            let notification: serde_json::Value = serde_json::from_str(line.trim()).expect("parse initialized");
            assert_eq!(notification["method"], "initialized");
        });

        let mut host = PluginHost::from_streams("test", host_reader, host_writer);
        let result = host.handshake().await.expect("handshake should succeed");

        assert_eq!(result.plugin_info.name, "test");
    }
}
