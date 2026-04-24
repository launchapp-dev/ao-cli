use super::*;

#[tool_router(router = agent_tool_router, vis = "pub(super)")]
impl AoMcpServer {
    #[tool(
        name = "ao.agent.list",
        description = "List configured project agent profiles, including names, roles, memory, and communication flags.",
        input_schema = ao_schema_for_type::<ProjectRootInput>()
    )]
    async fn ao_agent_list(&self, params: Parameters<ProjectRootInput>) -> Result<CallToolResult, McpError> {
        self.run_tool("ao.agent.list", vec!["agent".to_string(), "list".to_string()], params.0.project_root).await
    }

    #[tool(
        name = "ao.agent.get",
        description = "Get a configured agent profile by id.",
        input_schema = ao_schema_for_type::<AgentProfileInput>()
    )]
    async fn ao_agent_get(&self, params: Parameters<AgentProfileInput>) -> Result<CallToolResult, McpError> {
        let input = params.0;
        let args = vec!["agent".to_string(), "get".to_string(), "--id".to_string(), input.id];
        self.run_tool("ao.agent.get", args, input.project_root).await
    }

    #[tool(
        name = "ao.agent.run",
        description = "Run an agent to execute work. Purpose: Launch an AI agent to perform tasks. Prerequisites: Runner must be healthy (check ao.runner.health). Example: {\"tool\": \"claude\", \"model\": \"claude-3-opus\", \"prompt\": \"Fix the bug\"}. Sequencing: Use ao.agent.status to monitor, ao.agent.control to pause/resume/terminate.",
        input_schema = ao_schema_for_type::<AgentRunInput>()
    )]
    async fn ao_agent_run(&self, params: Parameters<AgentRunInput>) -> Result<CallToolResult, McpError> {
        let input = params.0;
        let args = build_agent_run_args(&input);
        self.run_tool("ao.agent.run", args, input.project_root).await
    }

    #[tool(
        name = "ao.agent.control",
        description = "Control a running agent. Purpose: Pause, resume, or terminate an active agent run. Prerequisites: Agent must be running (use ao.agent.status to verify). Example: {\"run_id\": \"abc123\", \"action\": \"terminate\"}. Valid actions: pause, resume, terminate. Sequencing: Use ao.agent.status first to check state, ao.output.monitor to see output.",
        input_schema = ao_schema_for_type::<AgentControlInput>()
    )]
    async fn ao_agent_control(&self, params: Parameters<AgentControlInput>) -> Result<CallToolResult, McpError> {
        let input = params.0;
        let mut args = vec![
            "agent".to_string(),
            "control".to_string(),
            "--run-id".to_string(),
            input.run_id,
            "--action".to_string(),
            input.action,
        ];
        push_opt(&mut args, "--runner-scope", input.runner_scope);
        self.run_tool("ao.agent.control", args, input.project_root).await
    }

    #[tool(
        name = "ao.agent.status",
        description = "Get status of an agent run. Purpose: Check if an agent is running, completed, or failed. Prerequisites: None (run_id from ao.agent.run). Example: {\"run_id\": \"abc123\"}. Sequencing: Use after ao.agent.run to track progress, or ao.agent.control to take action.",
        input_schema = ao_schema_for_type::<AgentStatusInput>()
    )]
    async fn ao_agent_status(&self, params: Parameters<AgentStatusInput>) -> Result<CallToolResult, McpError> {
        let input = params.0;
        let mut args = vec!["agent".to_string(), "status".to_string(), "--run-id".to_string(), input.run_id];
        push_opt(&mut args, "--runner-scope", input.runner_scope);
        self.run_tool("ao.agent.status", args, input.project_root).await
    }

    #[tool(
        name = "ao.agent.memory.get",
        description = "Read project-scoped memory for a configured agent profile.",
        input_schema = ao_schema_for_type::<AgentMemoryGetInput>()
    )]
    async fn ao_agent_memory_get(&self, params: Parameters<AgentMemoryGetInput>) -> Result<CallToolResult, McpError> {
        let input = params.0;
        let args =
            vec!["agent".to_string(), "memory".to_string(), "get".to_string(), "--agent".to_string(), input.agent];
        self.run_tool("ao.agent.memory.get", args, input.project_root).await
    }

    #[tool(
        name = "ao.agent.memory.append",
        description = "Append a project-scoped memory entry for a configured agent profile.",
        input_schema = ao_schema_for_type::<AgentMemoryAppendInput>()
    )]
    async fn ao_agent_memory_append(
        &self,
        params: Parameters<AgentMemoryAppendInput>,
    ) -> Result<CallToolResult, McpError> {
        let input = params.0;
        let mut args = vec![
            "agent".to_string(),
            "memory".to_string(),
            "append".to_string(),
            "--agent".to_string(),
            input.agent,
            "--text".to_string(),
            input.text,
        ];
        push_opt(&mut args, "--source", input.source);
        self.run_tool("ao.agent.memory.append", args, input.project_root).await
    }

    #[tool(
        name = "ao.agent.memory.clear",
        description = "Clear project-scoped memory for a configured agent profile.",
        input_schema = ao_schema_for_type::<AgentMemoryGetInput>()
    )]
    async fn ao_agent_memory_clear(&self, params: Parameters<AgentMemoryGetInput>) -> Result<CallToolResult, McpError> {
        let input = params.0;
        let args =
            vec!["agent".to_string(), "memory".to_string(), "clear".to_string(), "--agent".to_string(), input.agent];
        self.run_tool("ao.agent.memory.clear", args, input.project_root).await
    }

    #[tool(
        name = "ao.agent.message.send",
        description = "Send a project-scoped message on a configured agent channel.",
        input_schema = ao_schema_for_type::<AgentMessageSendInput>()
    )]
    async fn ao_agent_message_send(
        &self,
        params: Parameters<AgentMessageSendInput>,
    ) -> Result<CallToolResult, McpError> {
        let input = params.0;
        let mut args = vec![
            "agent".to_string(),
            "message".to_string(),
            "send".to_string(),
            "--channel".to_string(),
            input.channel,
            "--from".to_string(),
            input.from,
            "--text".to_string(),
            input.text,
        ];
        push_opt(&mut args, "--to", input.to);
        push_opt(&mut args, "--workflow-id", input.workflow_id);
        push_opt(&mut args, "--phase-id", input.phase_id);
        self.run_tool("ao.agent.message.send", args, input.project_root).await
    }

    #[tool(
        name = "ao.agent.message.list",
        description = "List project-scoped agent messages, optionally filtered by channel or agent.",
        input_schema = ao_schema_for_type::<AgentMessageListInput>()
    )]
    async fn ao_agent_message_list(
        &self,
        params: Parameters<AgentMessageListInput>,
    ) -> Result<CallToolResult, McpError> {
        let input = params.0;
        let mut args = vec!["agent".to_string(), "message".to_string(), "list".to_string()];
        push_opt(&mut args, "--channel", input.channel);
        push_opt(&mut args, "--agent", input.agent);
        push_opt(&mut args, "--limit", input.limit.map(|value| value.to_string()));
        self.run_tool("ao.agent.message.list", args, input.project_root).await
    }
}
