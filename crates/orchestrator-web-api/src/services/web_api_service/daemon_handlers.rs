use serde_json::{json, Value};

use super::{WebApiError, WebApiService};

impl WebApiService {
    pub async fn daemon_status(&self) -> Result<Value, WebApiError> {
        Ok(json!(self.context.hub.daemon().status().await?))
    }

    pub async fn daemon_health(&self) -> Result<Value, WebApiError> {
        Ok(json!(self.context.hub.daemon().health().await?))
    }

    pub async fn daemon_logs(&self, limit: Option<usize>) -> Result<Value, WebApiError> {
        Ok(json!(self.context.hub.daemon().logs(limit).await?))
    }

    pub async fn daemon_start(&self) -> Result<Value, WebApiError> {
        self.context.hub.daemon().start().await?;
        self.publish_event("daemon-start", json!({ "message": "daemon started" }));
        Ok(json!({ "message": "daemon started" }))
    }

    pub async fn daemon_stop(&self) -> Result<Value, WebApiError> {
        self.context.hub.daemon().stop().await?;
        self.publish_event("daemon-stop", json!({ "message": "daemon stopped" }));
        Ok(json!({ "message": "daemon stopped" }))
    }

    pub async fn daemon_pause(&self) -> Result<Value, WebApiError> {
        self.context.hub.daemon().pause().await?;
        self.publish_event("daemon-pause", json!({ "message": "daemon paused" }));
        Ok(json!({ "message": "daemon paused" }))
    }

    pub async fn daemon_resume(&self) -> Result<Value, WebApiError> {
        self.context.hub.daemon().resume().await?;
        self.publish_event("daemon-resume", json!({ "message": "daemon resumed" }));
        Ok(json!({ "message": "daemon resumed" }))
    }

    pub async fn daemon_clear_logs(&self) -> Result<Value, WebApiError> {
        self.context.hub.daemon().clear_logs().await?;
        self.publish_event(
            "daemon-clear-logs",
            json!({ "message": "daemon logs cleared" }),
        );
        Ok(json!({ "message": "daemon logs cleared" }))
    }

    pub async fn daemon_agents(&self) -> Result<Value, WebApiError> {
        let active_agents = self.context.hub.daemon().active_agents().await?;
        Ok(json!({ "active_agents": active_agents }))
    }
}
