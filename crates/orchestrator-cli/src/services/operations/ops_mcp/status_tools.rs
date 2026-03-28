use super::*;

#[tool_router(router = status_tool_router, vis = "pub(super)")]
impl AoMcpServer {
    #[tool(
        name = "ao.status",
        description = "Get unified project status. Purpose: Returns the full unified status payload including daemon status, active agents, task summary, recent completions, recent failures, and CI status. This provides a comprehensive \"what should I do now\" view for agents and UI consumers. Prerequisites: None. Example: {}. Sequencing: Use before ao.task.next or ao.daemon.health for a complete status snapshot.",
        input_schema = ao_schema_for_type::<ProjectRootInput>()
    )]
    async fn ao_status(&self, params: Parameters<ProjectRootInput>) -> Result<CallToolResult, McpError> {
        self.run_tool("ao.status", vec!["status".to_string()], params.0.project_root).await
    }
}
