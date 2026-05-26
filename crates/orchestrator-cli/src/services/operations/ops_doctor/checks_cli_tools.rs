//! CLI-tool-on-PATH checks for installed provider plugins.
//!
//! Each provider plugin shells out to an external CLI (`claude`, `codex`,
//! `gemini`, …); if that binary is missing the daemon will still start but
//! every dispatch will fail. We surface that here with copy-pasteable
//! install hints.

use orchestrator_core::plugin_preflight::summarize_discovered_plugins;
use orchestrator_plugin_host::discover_plugins;

use super::check_kit::{CheckContext, CheckFix, CheckStatus, DiagnosticCheck};

const CATEGORY: &str = "cli_tools";

/// Maps the conventional provider plugin name → (CLI binary, install hint).
fn provider_cli_for(plugin_name: &str) -> Option<(&'static str, &'static str)> {
    let lc = plugin_name.to_ascii_lowercase();
    if lc.contains("claude") {
        Some(("claude", "npm install -g @anthropic-ai/claude-code"))
    } else if lc.contains("codex") {
        Some(("codex", "npm install -g @openai/codex"))
    } else if lc.contains("gemini") {
        Some(("gemini", "npm install -g @google/gemini-cli"))
    } else if lc.contains("opencode") {
        Some(("opencode", "npm install -g opencode-ai"))
    } else if lc.contains("oai") {
        // The `animus-provider-oai` plugin talks to the OpenAI API directly via
        // HTTP, no local CLI required. Surface as informational.
        None
    } else {
        None
    }
}

pub(crate) fn run(ctx: &CheckContext) -> Vec<DiagnosticCheck> {
    let mut out = Vec::new();
    let discovered = match discover_plugins(ctx.project_root.clone()) {
        Ok(list) => list,
        Err(_) => return out, // plugin check module already reports this
    };
    let summaries = summarize_discovered_plugins(&discovered);

    let mut any_provider = false;
    for summary in summaries.iter().filter(|s| s.is_provider()) {
        any_provider = true;
        let Some((binary, install_hint)) = provider_cli_for(&summary.name) else {
            out.push(
                DiagnosticCheck::new(
                    format!("cli_tool_present.{}", sanitize(&summary.name)),
                    CATEGORY,
                    CheckStatus::Skipped,
                    format!("CLI tool for {}", summary.name),
                )
                .details("provider does not require a local CLI".to_string()),
            );
            continue;
        };

        let found = which::which(binary).ok();
        let check = if let Some(path) = found {
            DiagnosticCheck::new(
                format!("cli_tool_present.{}", sanitize(&summary.name)),
                CATEGORY,
                CheckStatus::Pass,
                format!("CLI tool for {}", summary.name),
            )
            .details(format!("{binary} found at {}", path.display()))
        } else {
            DiagnosticCheck::new(
                format!("cli_tool_present.{}", sanitize(&summary.name)),
                CATEGORY,
                CheckStatus::Fail,
                format!("CLI tool for {}", summary.name),
            )
            .current(format!("{binary} not on PATH"))
            .expected(format!("{binary} resolvable via `which {binary}`"))
            .fix(CheckFix::command(
                "install_provider_cli",
                &format!("Install the {binary} CLI."),
                install_hint,
            ))
        };
        out.push(check);
    }

    if !any_provider {
        out.push(
            DiagnosticCheck::new("cli_tool_inventory", CATEGORY, CheckStatus::Skipped, "Provider CLIs on PATH")
                .details("no provider plugins installed; skipping per-provider CLI checks".to_string()),
        );
    }

    out
}

fn sanitize(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_cli_resolves_claude_and_codex() {
        assert_eq!(provider_cli_for("animus-provider-claude").map(|(b, _)| b), Some("claude"));
        assert_eq!(provider_cli_for("animus-provider-codex").map(|(b, _)| b), Some("codex"));
        assert_eq!(provider_cli_for("animus-provider-gemini").map(|(b, _)| b), Some("gemini"));
        assert_eq!(provider_cli_for("animus-provider-opencode").map(|(b, _)| b), Some("opencode"));
    }

    #[test]
    fn provider_cli_returns_none_for_oai_and_unknown() {
        assert!(provider_cli_for("animus-provider-oai").is_none());
        assert!(provider_cli_for("animus-provider-unknown").is_none());
    }
}
