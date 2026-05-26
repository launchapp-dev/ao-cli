//! Cosign signature presence + verification for installed plugins.
//!
//! Surfaces both "cosign isn't installed at all" and "this plugin has no
//! matching .bundle next to its binary". We do NOT shell out to
//! `cosign verify-blob` here — that requires the trust anchors the install
//! flow uses and is too heavy for a default doctor run. Instead we point at
//! `animus plugin install <repo>@<version>` which re-downloads + re-verifies.

use std::path::PathBuf;

use orchestrator_plugin_host::discover_plugins;

use super::check_kit::{CheckContext, CheckFix, CheckStatus, DiagnosticCheck};

const CATEGORY: &str = "cosign";

pub(crate) fn run(ctx: &CheckContext) -> Vec<DiagnosticCheck> {
    let mut out = Vec::new();

    let cosign_path = which::which("cosign").ok();
    out.push(match cosign_path.as_ref() {
        Some(path) => DiagnosticCheck::new("cosign_installed", CATEGORY, CheckStatus::Pass, "cosign on PATH")
            .details(format!("cosign found at {}", path.display())),
        None => DiagnosticCheck::new("cosign_installed", CATEGORY, CheckStatus::Warn, "cosign on PATH")
            .current("cosign not found".to_string())
            .expected("cosign resolvable via `which cosign`".to_string())
            .fix(CheckFix::command(
                "install_cosign",
                "Install Sigstore cosign to verify plugin signatures.",
                "brew install cosign",
            )),
    });

    let discovered = match discover_plugins(ctx.project_root.clone()) {
        Ok(list) => list,
        Err(_) => return out,
    };

    for plugin in &discovered {
        let bundle_path: PathBuf = plugin.path.with_extension("bundle");
        let id = format!("cosign_bundle_present.{}", sanitize(&plugin.name));
        if bundle_path.exists() {
            out.push(
                DiagnosticCheck::new(id, CATEGORY, CheckStatus::Pass, format!("Signature bundle: {}", plugin.name))
                    .details(format!("bundle at {}", bundle_path.display())),
            );
        } else {
            out.push(
                DiagnosticCheck::new(id, CATEGORY, CheckStatus::Warn, format!("Signature bundle: {}", plugin.name))
                    .current(format!("no .bundle at {}", bundle_path.display()))
                    .expected("matching .bundle file next to plugin binary".to_string())
                    .fix(CheckFix::command(
                        "reinstall_plugin_signed",
                        &format!("Reinstall {} so the install flow re-downloads its signature bundle.", plugin.name),
                        &format!("animus plugin install {}@latest", plugin.name),
                    )),
            );
        }
    }

    out
}

fn sanitize(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}
