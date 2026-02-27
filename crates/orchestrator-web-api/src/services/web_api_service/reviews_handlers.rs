use orchestrator_core::AgentHandoffRequestInput;
use serde_json::{json, Value};

use super::{
    parsing::{parse_handoff_target_role, parse_json_body},
    requests::ReviewHandoffRequest,
    WebApiError, WebApiService,
};

impl WebApiService {
    pub async fn reviews_handoff(&self, body: Value) -> Result<Value, WebApiError> {
        let request: ReviewHandoffRequest = parse_json_body(body)?;
        let target_role = parse_handoff_target_role(&request.target_role)?;
        let input = AgentHandoffRequestInput {
            handoff_id: request.handoff_id,
            run_id: request.run_id,
            target_role,
            question: request.question,
            context: request.context,
        };

        let result = self.context.hub.review().request_handoff(input).await?;
        self.publish_event(
            "review-handoff",
            json!({
                "handoff_id": result.handoff_id,
                "run_id": result.run_id,
                "target_role": result.target_role.as_str(),
                "status": result.status,
            }),
        );
        Ok(json!(result))
    }
}
