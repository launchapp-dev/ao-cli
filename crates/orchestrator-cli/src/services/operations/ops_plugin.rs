mod control_routing;
mod marketplace;
mod new;
mod signing;

pub(crate) use control_routing::build_plugin_routing;
pub(crate) use marketplace::{
    run_plugin_browse, run_plugin_search, run_plugin_update, PluginBrowseRequest, PluginSearchRequest,
    PluginUpdateRequest,
};
#[allow(unused_imports)]
pub(crate) use signing::{
    cosign_available, load_trusted_signers, resolve_trusted_signers_path, verify_with_cosign, SignatureStatus,
    GITHUB_OIDC_ISSUER,
};

use std::collections::BTreeSet;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use animus_plugin_protocol::PluginManifest;
use anyhow::{anyhow, Context, Result};
use cli_wrapper::is_reserved_provider_tool;
use orchestrator_plugin_host::{
    discover_plugins, legacy_plugins_registry_path, plugin_install_dir, plugins_registry_path,
    registered_skip_manifest_check_at_install, DiscoveredPlugin, DiscoverySource, DiscoveryWarning, PluginDiscovery,
    PluginHost,
};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    invalid_input_error, not_found_error, print_value, PluginCallArgs, PluginCommand, PluginInfoArgs,
    PluginInstallArgs, PluginListArgs, PluginPingArgs, PluginUninstallArgs,
};

#[derive(Debug, Serialize)]
pub(crate) struct DiscoveredPluginRow {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) plugin_kind: String,
    pub(crate) description: String,
    pub(crate) protocol_version: String,
    pub(crate) capabilities: Vec<String>,
    pub(crate) source: &'static str,
    pub(crate) path: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct PluginWarningRow {
    pub(crate) name: String,
    pub(crate) source: &'static str,
    pub(crate) path: String,
    pub(crate) reason: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct PluginListOutput {
    pub(crate) plugins: Vec<DiscoveredPluginRow>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) warnings: Vec<PluginWarningRow>,
}

#[derive(Debug, Serialize)]
pub(crate) struct PluginInfoOutput {
    pub(crate) name: String,
    pub(crate) source: &'static str,
    pub(crate) path: String,
    pub(crate) manifest: PluginManifest,
    pub(crate) initialize: Value,
    /// Audit-trail field: `true` when the plugin was installed with
    /// `--skip-manifest-check`. Surfaced so operators can see why discovery
    /// silently tolerates manifest probe failures for this plugin.
    pub(crate) skip_manifest_check_at_install: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct PluginCallOutput {
    pub(crate) name: String,
    pub(crate) method: String,
    pub(crate) result: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct PluginPingOutput {
    pub(crate) name: String,
    pub(crate) ok: bool,
    pub(crate) plugin_info: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct PluginInstallOutput {
    pub(crate) name: String,
    pub(crate) installed_path: String,
    pub(crate) sha256: String,
    pub(crate) manifest: Option<PluginManifest>,
    pub(crate) plugins_yaml: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) source_kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) origin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) release_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) asset_name: Option<String>,
    pub(crate) sha256_verified: bool,
    /// Cosign signature verification outcome. Stable strings:
    /// `verified` | `unsigned` | `invalid` | `untrusted_signer` | `skipped`.
    /// See `docs/architecture/plugin-signing.md`.
    pub(crate) signature_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) signature_detail: Option<SignatureStatus>,
}

#[derive(Debug, Serialize)]
pub(crate) struct PluginUninstallOutput {
    pub(crate) name: String,
    pub(crate) removed_path: Option<String>,
    pub(crate) plugins_yaml: String,
}

// ===== Typed request structs (shared between CLI and MCP) =====

/// Typed request for `plugin list`. Both CLI and MCP build one of these and
/// call [`run_plugin_list`]. The CLI handler additionally streams warnings to
/// stderr when in text mode; MCP returns warnings inside the structured payload.
#[derive(Debug, Clone, Default)]
pub(crate) struct PluginListRequest {
    pub(crate) project_root: String,
    pub(crate) include_system_path: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct PluginInfoRequest {
    pub(crate) project_root: String,
    pub(crate) name: String,
    pub(crate) include_system_path: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct PluginPingRequest {
    pub(crate) project_root: String,
    pub(crate) name: String,
    pub(crate) include_system_path: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct PluginCallRequest {
    pub(crate) project_root: String,
    pub(crate) name: String,
    pub(crate) method: String,
    pub(crate) params: Option<Value>,
    pub(crate) include_system_path: bool,
}

/// Typed request for `plugin install`. Mirrors the CLI arg surface so MCP can
/// invoke the same install pipeline. Exactly one of `source` / `path` / `url`
/// must be supplied. When `url` is set, `sha256` is required. The `source`
/// (owner/repo[@tag]) input is forwarded to the CLI install pipeline; if the
/// underlying handler does not yet implement public-repo installs, a clear
/// error is returned.
#[derive(Debug, Clone, Default)]
pub(crate) struct PluginInstallRequest {
    pub(crate) source: Option<String>,
    pub(crate) path: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) tag: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) sha256: Option<String>,
    pub(crate) force: bool,
    pub(crate) skip_manifest_check: bool,
    /// Override the plugin install directory. Takes precedence over
    /// `$ANIMUS_PLUGIN_DIR`. When `None`, falls back to env / default.
    pub(crate) plugin_dir: Option<String>,
    /// Refuse install when no cosign bundle is present or when verification
    /// fails. Default `false` — verify-if-present.
    pub(crate) require_signature: bool,
    /// Skip cosign verification entirely (escape hatch). Mutually exclusive
    /// with `require_signature` (enforced at the CLI layer).
    pub(crate) skip_signature: bool,
    /// Optional path to the trusted-signers YAML (overrides default
    /// `~/.animus/trusted-signers.yaml`).
    pub(crate) trusted_signers: Option<PathBuf>,
    /// Permit installs whose provider_tool collides with an in-tree backend.
    /// Required for plugins that legitimately replace claude / codex / gemini
    /// / opencode / oai-runner dispatch.
    pub(crate) allow_shadow_builtin: bool,
    /// Owners to pre-trust before this install (TOFU). Appended to
    /// `~/.animus/trusted-orgs.yaml` after a successful install.
    pub(crate) allow_org: Vec<String>,
    /// Auto-confirm the TOFU prompt for unknown orgs.
    pub(crate) yes: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct PluginUninstallRequest {
    pub(crate) name: String,
    pub(crate) plugin_dir: Option<String>,
}

pub(crate) async fn handle_plugin(command: PluginCommand, project_root: &str, json: bool) -> Result<()> {
    match command {
        PluginCommand::List(args) => handle_plugin_list(args, project_root, json).await,
        PluginCommand::Info(args) => handle_plugin_info(args, project_root, json).await,
        PluginCommand::Call(args) => handle_plugin_call(args, project_root, json).await,
        PluginCommand::Ping(args) => handle_plugin_ping(args, project_root, json).await,
        // Install and uninstall stay strictly local — they have heavy
        // filesystem side-effects (binary copy, registry yaml write,
        // signature checks) that don't benefit from a wire round-trip.
        // The daemon-side `plugin/install` handler exists so MCP/WebAPI
        // (C7/C8) can call it via the wire, but the CLI's user-facing
        // path is intentionally direct.
        PluginCommand::Install(args) => handle_plugin_install(args, json).await,
        PluginCommand::Uninstall(args) => handle_plugin_uninstall(args, json),
        PluginCommand::New(args) => new::handle_plugin_new(args, json),
        PluginCommand::Search(args) => marketplace::handle_plugin_search(args).await,
        PluginCommand::Browse(args) => marketplace::handle_plugin_browse(args).await,
        PluginCommand::Update(args) => marketplace::handle_plugin_update(args).await,
    }
}

// ===== Reusable typed entry points (shared between CLI and MCP) =====

/// List discovered plugins. Identical surface as `animus plugin list`.
pub(crate) fn run_plugin_list(req: PluginListRequest) -> Result<PluginListOutput> {
    let (discovered, warnings) = discover_with_warnings(&req.project_root, req.include_system_path)?;
    let rows: Vec<DiscoveredPluginRow> = discovered
        .into_iter()
        .map(|plugin| DiscoveredPluginRow {
            name: plugin.name,
            version: plugin.manifest.version,
            plugin_kind: plugin.manifest.plugin_kind,
            description: plugin.manifest.description,
            protocol_version: plugin.manifest.protocol_version,
            capabilities: plugin.manifest.capabilities,
            source: source_label(plugin.source),
            path: plugin.path.display().to_string(),
        })
        .collect();
    let warning_rows: Vec<PluginWarningRow> = warnings
        .into_iter()
        .map(|warning| PluginWarningRow {
            name: warning.name,
            source: source_label(warning.source),
            path: warning.path.display().to_string(),
            reason: warning.reason,
        })
        .collect();
    Ok(PluginListOutput { plugins: rows, warnings: warning_rows })
}

/// Spawn the named plugin, complete the handshake, and return manifest +
/// initialize-time capabilities.
pub(crate) async fn run_plugin_info(req: PluginInfoRequest) -> Result<PluginInfoOutput> {
    let discovered = find_plugin(&req.project_root, &req.name, req.include_system_path)?;
    let host = PluginHost::spawn(&discovered.path, &[]).await.context("failed to spawn plugin")?;
    let initialize = host.handshake().await.context("plugin initialize failed")?;
    let _ = host.shutdown().await;
    let skip_flag = registered_skip_manifest_check_at_install(&discovered.name);
    Ok(PluginInfoOutput {
        name: discovered.name,
        source: source_label(discovered.source),
        path: discovered.path.display().to_string(),
        manifest: discovered.manifest,
        initialize: serde_json::to_value(initialize)?,
        skip_manifest_check_at_install: skip_flag,
    })
}

/// Health-check a plugin by spawning it, completing the handshake, and
/// dispatching `$/ping`.
pub(crate) async fn run_plugin_ping(req: PluginPingRequest) -> Result<PluginPingOutput> {
    let discovered = find_plugin(&req.project_root, &req.name, req.include_system_path)?;
    let host = PluginHost::spawn(&discovered.path, &[]).await.context("failed to spawn plugin")?;
    let initialize = host.handshake().await.context("plugin initialize failed")?;
    host.ping().await.context("plugin ping failed")?;
    let _ = host.shutdown().await;
    Ok(PluginPingOutput { name: discovered.name, ok: true, plugin_info: serde_json::to_value(initialize.plugin_info)? })
}

/// Send a JSON-RPC request to a discovered plugin and return its response.
pub(crate) async fn run_plugin_call(req: PluginCallRequest) -> Result<PluginCallOutput> {
    let method = req.method.trim().to_string();
    if method.is_empty() {
        return Err(invalid_input_error("method must not be empty"));
    }
    let discovered = find_plugin(&req.project_root, &req.name, req.include_system_path)?;
    let host = PluginHost::spawn(&discovered.path, &[]).await.context("failed to spawn plugin")?;
    let _ = host.handshake().await.context("plugin initialize failed")?;
    let result = host
        .request(method.clone(), req.params)
        .await
        .map_err(|err| anyhow!("plugin call failed ({}): {}", err.code, err.message))?;
    let _ = host.shutdown().await;
    Ok(PluginCallOutput { name: discovered.name, method, result })
}

/// Uninstall a plugin from the install dir and registry yaml.
pub(crate) fn run_plugin_uninstall(req: PluginUninstallRequest) -> Result<PluginUninstallOutput> {
    let plugin_name = req.name.trim().to_string();
    if plugin_name.is_empty() {
        return Err(invalid_input_error("name must not be empty"));
    }

    let yaml_path = plugins_yaml_path()?;
    let mut config = load_plugins_yaml(&yaml_path)?;
    let key = serde_yaml::Value::String(plugin_name.clone());
    let removed_in_yaml = config.plugins.remove(&key).is_some() || config.providers.remove(&key).is_some();
    if removed_in_yaml {
        save_plugins_yaml(&yaml_path, &config)?;
    }

    let install_dir = install_root(req.plugin_dir.as_deref())?;
    let installed_path = install_dir.join(&plugin_name);
    let removed = if installed_path.exists() {
        std::fs::remove_file(&installed_path)
            .with_context(|| format!("failed to remove {}", installed_path.display()))?;
        Some(installed_path.to_string_lossy().to_string())
    } else {
        None
    };

    if !removed_in_yaml && removed.is_none() {
        return Err(not_found_error(format!("plugin '{plugin_name}' is not installed")));
    }

    Ok(PluginUninstallOutput {
        name: plugin_name,
        removed_path: removed,
        plugins_yaml: yaml_path.to_string_lossy().to_string(),
    })
}

/// Install a plugin binary from a public GitHub release (`owner/repo[@tag]`),
/// a local path, or an HTTPS URL. Wired into both the CLI
/// (`handle_plugin_install`) and the MCP install tool.
pub(crate) async fn run_plugin_install(req: PluginInstallRequest) -> Result<PluginInstallOutput> {
    let provided = [req.source.is_some(), req.path.is_some(), req.url.is_some()].iter().filter(|b| **b).count();
    if provided == 0 {
        return Err(invalid_input_error("one of `source` (owner/repo[@tag]), `path`, or `url` must be provided"));
    }
    if provided > 1 {
        return Err(invalid_input_error("`source`, `path`, and `url` are mutually exclusive"));
    }
    if req.tag.is_some() && req.source.is_none() {
        return Err(invalid_input_error("`tag` only applies when installing from a public repo (`source`)"));
    }
    if req.url.is_some() && req.sha256.is_none() {
        return Err(invalid_input_error(
            "--sha256 is required when installing from a URL; compute via `shasum -a 256 <plugin>`",
        ));
    }
    if req.require_signature && req.skip_signature {
        return Err(invalid_input_error("--require-signature and --skip-signature are mutually exclusive"));
    }

    // `_install_temp` keeps the install-staging directory alive for the
    // remainder of this function. It drops at the end (RAII) so the
    // tempdir is reliably cleaned up whether install succeeds, errors,
    // or returns early — closing the GBs-of-`animus-plugin-install-*`
    // accumulation the old `std::env::temp_dir().join(uuid)` left behind.
    let (source_path, default_name, provenance, _install_temp): (
        PathBuf,
        String,
        InstallProvenance,
        Option<tempfile::TempDir>,
    ) = if let Some(slug) = req.source.as_deref() {
        let spec = parse_repo_spec(slug)?;
        let release = resolve_release_install(spec, req.tag.clone()).await?;
        let provenance = InstallProvenance {
            source_kind: Some("release"),
            origin: Some(release.origin.clone()),
            release_tag: Some(release.release_tag.clone()),
            asset_name: Some(release.asset_name.clone()),
            sha256_verified: Some(release.sha256_verified),
            asset_archive_path: release.asset_archive_path.clone(),
            bundle_path: release.bundle_path.clone(),
            owner: Some(release.owner.clone()),
            repo: Some(release.repo.clone()),
        };
        (release.binary_path, release.plugin_name_hint, provenance, Some(release._temp_dir))
    } else if let Some(p) = req.path.as_deref() {
        (PathBuf::from(p), String::new(), InstallProvenance { source_kind: Some("path"), ..Default::default() }, None)
    } else if let Some(u) = req.url.as_deref() {
        let expected = req
            .sha256
            .as_deref()
            .ok_or_else(|| invalid_input_error("sha256 is required when installing from a URL"))?;
        let (path, temp_dir) = fetch_url_to_temp(u, expected).await?;
        let provenance = InstallProvenance {
            source_kind: Some("url"),
            origin: Some(u.to_string()),
            sha256_verified: Some(true),
            ..Default::default()
        };
        (path, String::new(), provenance, Some(temp_dir))
    } else {
        unreachable!("validated above");
    };

    if !source_path.exists() {
        return Err(not_found_error(format!("plugin source not found: {}", source_path.display())));
    }
    if !source_path.is_file() {
        return Err(invalid_input_error(format!("plugin source is not a file: {}", source_path.display())));
    }

    let computed_sha = sha256_of_file(&source_path)?;
    if let Some(expected) = req.sha256.as_deref() {
        if !expected.eq_ignore_ascii_case(&computed_sha) {
            return Err(invalid_input_error(format!("sha256 mismatch: expected {expected}, computed {computed_sha}")));
        }
    }

    // Manifest probe runs against the source binary BEFORE copying into the
    // install dir so we can refuse name-spoofed installs, reserved-name
    // shadows, and untrusted-org installs without leaving stale files behind.
    let source_manifest = if req.skip_manifest_check {
        None
    } else {
        // Only chmod the source for sources we downloaded ourselves (the
        // tarball-extracted release binary, or a `--url` blob in our temp
        // dir). For `--path` we leave the user's original file alone — the
        // post-copy `ensure_executable(&installed_path)` covers the install
        // location.
        if matches!(provenance.source_kind, Some("release") | Some("url")) {
            ensure_executable(&source_path)?;
        }
        Some(probe_manifest(&source_path)?)
    };

    if let Some(manifest_for_check) = source_manifest.as_ref() {
        enforce_provider_tool_policy(manifest_for_check, req.allow_shadow_builtin)?;
        if let (Some(owner), Some(repo)) = (provenance.owner.as_deref(), provenance.repo.as_deref()) {
            enforce_manifest_name_matches_repo(manifest_for_check, owner, repo, req.force)?;
        }
    }

    if provenance.source_kind == Some("release") {
        if let Some(owner) = provenance.owner.as_deref() {
            enforce_org_trust(owner, &req)?;
        }
    }

    let signature_detail = resolve_signature_status(&req, &provenance)?;
    match &signature_detail {
        SignatureStatus::Invalid { message, .. } => {
            return Err(invalid_input_error(format!(
                "cosign signature verification FAILED; refusing install: {message}"
            )));
        }
        SignatureStatus::UntrustedSigner { identity_pattern } => {
            return Err(invalid_input_error(format!(
                "signature is valid but the signer is not in trusted-signers.yaml (identity pattern: {identity_pattern})"
            )));
        }
        SignatureStatus::Unsigned { reason } if req.require_signature => {
            return Err(invalid_input_error(format!(
                "--require-signature is set but no cosign signature could be verified: {reason}"
            )));
        }
        _ => {}
    }

    let plugin_name = match req.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        Some(name) => name.to_string(),
        None => {
            if !default_name.is_empty() {
                default_name
            } else {
                source_path
                    .file_name()
                    .and_then(|f| f.to_str())
                    .ok_or_else(|| invalid_input_error("could not derive plugin name from source path"))?
                    .to_string()
            }
        }
    };

    let install_dir = install_root(req.plugin_dir.as_deref())?;
    let installed_path = install_dir.join(&plugin_name);
    if installed_path.exists() && !req.force {
        return Err(invalid_input_error(format!(
            "plugin '{plugin_name}' already installed at {} (pass force=true to overwrite)",
            installed_path.display()
        )));
    }

    std::fs::copy(&source_path, &installed_path)
        .with_context(|| format!("failed to copy {} → {}", source_path.display(), installed_path.display()))?;
    ensure_executable(&installed_path)?;

    // Manifest was probed against the source binary above; nothing to do here.
    let manifest = source_manifest;

    let yaml_path = plugins_yaml_path()?;
    let mut config = load_plugins_yaml(&yaml_path)?;
    let entry: serde_yaml::Mapping = {
        let mut map = serde_yaml::Mapping::new();
        map.insert(
            serde_yaml::Value::String("binary".to_string()),
            serde_yaml::Value::String(installed_path.to_string_lossy().to_string()),
        );
        if let Some(m) = manifest.as_ref() {
            map.insert(serde_yaml::Value::String("name".to_string()), serde_yaml::Value::String(m.name.clone()));
        }
        if let Some(kind) = provenance.source_kind {
            map.insert(
                serde_yaml::Value::String("source_kind".to_string()),
                serde_yaml::Value::String(kind.to_string()),
            );
        }
        if let Some(origin) = provenance.origin.as_ref() {
            map.insert(serde_yaml::Value::String("origin".to_string()), serde_yaml::Value::String(origin.clone()));
        }
        if let Some(tag) = provenance.release_tag.as_ref() {
            map.insert(serde_yaml::Value::String("release_tag".to_string()), serde_yaml::Value::String(tag.clone()));
        }
        if let Some(asset) = provenance.asset_name.as_ref() {
            map.insert(serde_yaml::Value::String("asset".to_string()), serde_yaml::Value::String(asset.clone()));
        }
        map.insert(serde_yaml::Value::String("sha256".to_string()), serde_yaml::Value::String(computed_sha.clone()));
        map.insert(
            serde_yaml::Value::String("installed_at".to_string()),
            serde_yaml::Value::String(chrono::Utc::now().to_rfc3339()),
        );
        // Persist an audit trail when the operator bypassed the manifest
        // probe at install time. Discovery emits a warn! on every subsequent
        // probe for plugins flagged this way so the silent tolerance of
        // probe failures stays visible. We only write the field when set to
        // keep the registry quiet for the common case.
        if req.skip_manifest_check {
            map.insert(
                serde_yaml::Value::String("skip_manifest_check_at_install".to_string()),
                serde_yaml::Value::Bool(true),
            );
        }
        map.insert(
            serde_yaml::Value::String("signature_status".to_string()),
            serde_yaml::Value::String(signature_detail.label().to_string()),
        );
        if let SignatureStatus::Verified { identity, bundle_path } = &signature_detail {
            map.insert(
                serde_yaml::Value::String("signature_identity".to_string()),
                serde_yaml::Value::String(identity.clone()),
            );
            map.insert(
                serde_yaml::Value::String("signature_bundle".to_string()),
                serde_yaml::Value::String(bundle_path.clone()),
            );
        }
        map
    };
    let table = match manifest.as_ref().map(|m| m.plugin_kind.as_str()) {
        Some("provider") => &mut config.providers,
        _ => &mut config.plugins,
    };
    table.insert(serde_yaml::Value::String(plugin_name.clone()), serde_yaml::Value::Mapping(entry));
    save_plugins_yaml(&yaml_path, &config)?;

    // TOFU: persist trust for the org we just installed from. Pre-trusted orgs
    // and orgs the user explicitly listed via `--allow-org` get written to
    // `~/.animus/trusted-orgs.yaml` so a follow-up install skips the prompt.
    if let Some(owner) = provenance.owner.as_deref() {
        if let Err(error) = add_trusted_org(owner) {
            tracing::warn!(owner, %error, "failed to persist trusted org after install");
        }
    }
    for explicit in &req.allow_org {
        if let Err(error) = add_trusted_org(explicit) {
            tracing::warn!(org = %explicit, %error, "failed to persist --allow-org");
        }
    }

    let sha256_verified = match provenance.sha256_verified {
        Some(verified) => verified,
        None => req.sha256.is_some(),
    };

    let signature_status = signature_detail.label().to_string();
    let signature_detail = Some(signature_detail);

    Ok(PluginInstallOutput {
        name: plugin_name,
        installed_path: installed_path.to_string_lossy().to_string(),
        sha256: computed_sha,
        manifest,
        plugins_yaml: yaml_path.to_string_lossy().to_string(),
        source_kind: provenance.source_kind,
        origin: provenance.origin,
        release_tag: provenance.release_tag,
        asset_name: provenance.asset_name,
        sha256_verified,
        signature_status,
        signature_detail,
    })
}

fn discover(project_root: &str, include_system_path: bool) -> Result<Vec<DiscoveredPlugin>> {
    PluginDiscovery::new()
        .with_project_root(Path::new(project_root))
        .include_system_path(include_system_path)
        .discover()
        .context("plugin discovery failed")
}

fn discover_with_warnings(
    project_root: &str,
    include_system_path: bool,
) -> Result<(Vec<DiscoveredPlugin>, Vec<DiscoveryWarning>)> {
    PluginDiscovery::new()
        .with_project_root(Path::new(project_root))
        .include_system_path(include_system_path)
        .discover_with_warnings()
        .context("plugin discovery failed")
}

fn source_label(source: DiscoverySource) -> &'static str {
    match source {
        DiscoverySource::ExplicitConfig => "explicit_config",
        DiscoverySource::ProjectLocal => "project_local",
        DiscoverySource::PluginPath => "plugin_path",
        DiscoverySource::SystemPath => "system_path",
    }
}

async fn handle_plugin_list(args: PluginListArgs, project_root: &str, json: bool) -> Result<()> {
    // C6: prefer the control wire when the daemon is running so the
    // daemon's view of installed plugins is authoritative. For CLI text
    // output we still drive the local in-process render path because it
    // pulls richer rows from the on-disk install index. JSON mode
    // round-trips through the wire's PluginListResponse shape (which is
    // the same shape MCP/WebAPI will surface in C7/C8).
    use animus_control_protocol::types::PluginListRequest as WirePluginListRequest;
    use orchestrator_daemon_runtime::control::ControlClient;

    if json {
        let project_root_path = std::path::Path::new(project_root);
        if let Some(client) = ControlClient::try_connect(project_root_path).await? {
            let request = WirePluginListRequest { include_warnings: true, kind: None };
            match client.plugin_list(request).await {
                Ok(response) => return print_value(response, true),
                Err(err) if orchestrator_daemon_runtime::control::is_method_unavailable(&err) => {
                    tracing::debug!(error = %err, "plugin/list wire returned unavailable; falling back to local");
                }
                Err(err) => return Err(err),
            }
        }
    }

    let output = run_plugin_list(PluginListRequest {
        project_root: project_root.to_string(),
        include_system_path: args.include_system_path,
    })?;

    if json {
        return print_value(output, true);
    }

    for warning in &output.warnings {
        eprintln!(
            "warning: plugin '{}' was discovered ({}) but could not be loaded: {} ({})",
            warning.name, warning.source, warning.reason, warning.path
        );
    }

    print_plugin_list_table(&output)
}

/// Render `plugin list` results as a table with source-of-truth columns:
/// `NAME  KIND  VERSION  SOURCE  INSTALLED  PATH`.
fn print_plugin_list_table(output: &PluginListOutput) -> Result<()> {
    if output.plugins.is_empty() {
        println!("no plugins discovered");
        return Ok(());
    }
    let installed = marketplace::read_installed_index().unwrap_or_default();
    struct Row {
        name: String,
        kind: String,
        version: String,
        source: String,
        installed: String,
        path: String,
    }
    let rows: Vec<Row> = output
        .plugins
        .iter()
        .map(|p| {
            let installed_entry = installed.get(&p.name);
            let source = installed_entry.map(marketplace::format_installed_source).unwrap_or_else(|| "--".to_string());
            let installed_at = installed_entry
                .and_then(|e| e.installed_at.as_deref())
                .map(|s| s.split('T').next().unwrap_or(s).to_string())
                .unwrap_or_else(|| "--".to_string());
            Row {
                name: p.name.clone(),
                kind: p.plugin_kind.clone(),
                version: if p.version.is_empty() { "--".to_string() } else { p.version.clone() },
                source,
                installed: installed_at,
                path: p.path.clone(),
            }
        })
        .collect();
    let widths = [
        rows.iter().map(|r| r.name.len()).max().unwrap_or(4).max(4),
        rows.iter().map(|r| r.kind.len()).max().unwrap_or(4).max(4),
        rows.iter().map(|r| r.version.len()).max().unwrap_or(7).max(7),
        rows.iter().map(|r| r.source.len()).max().unwrap_or(6).max(6),
        rows.iter().map(|r| r.installed.len()).max().unwrap_or(9).max(9),
    ];
    println!(
        "{:<w0$}  {:<w1$}  {:<w2$}  {:<w3$}  {:<w4$}  PATH",
        "NAME",
        "KIND",
        "VERSION",
        "SOURCE",
        "INSTALLED",
        w0 = widths[0],
        w1 = widths[1],
        w2 = widths[2],
        w3 = widths[3],
        w4 = widths[4],
    );
    for row in &rows {
        println!(
            "{:<w0$}  {:<w1$}  {:<w2$}  {:<w3$}  {:<w4$}  {}",
            row.name,
            row.kind,
            row.version,
            row.source,
            row.installed,
            row.path,
            w0 = widths[0],
            w1 = widths[1],
            w2 = widths[2],
            w3 = widths[3],
            w4 = widths[4],
        );
    }
    Ok(())
}

fn find_plugin(project_root: &str, name: &str, include_system_path: bool) -> Result<DiscoveredPlugin> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(invalid_input_error("--name must not be empty"));
    }
    let mut matches =
        if include_system_path { discover(project_root, true)? } else { discover_plugins(Path::new(project_root))? };
    matches.retain(|plugin| plugin.name == trimmed);
    matches.pop().ok_or_else(|| not_found_error(format!("plugin not found: {trimmed}")))
}

async fn handle_plugin_info(args: PluginInfoArgs, project_root: &str, json: bool) -> Result<()> {
    use animus_control_protocol::types::PluginInfoRequest as WirePluginInfoRequest;
    use orchestrator_daemon_runtime::control::ControlClient;

    if json {
        let project_root_path = std::path::Path::new(project_root);
        if let Some(client) = ControlClient::try_connect(project_root_path).await? {
            let request = WirePluginInfoRequest { name: args.name.clone() };
            match client.plugin_info(request).await {
                Ok(response) => return print_value(response, true),
                Err(err) if orchestrator_daemon_runtime::control::is_method_unavailable(&err) => {
                    tracing::debug!(error = %err, "plugin/info wire returned unavailable; falling back to local");
                }
                Err(err) => return Err(err),
            }
        }
    }

    let output = run_plugin_info(PluginInfoRequest {
        project_root: project_root.to_string(),
        name: args.name,
        include_system_path: args.include_system_path,
    })
    .await?;
    // Surface the audit flag in human-readable mode so operators see at a
    // glance that the manifest probe was skipped at install time. JSON mode
    // carries the same signal via `skip_manifest_check_at_install`.
    if !json && output.skip_manifest_check_at_install {
        println!("SKIP_MANIFEST_CHECK: true");
    }
    print_value(output, json)
}

async fn handle_plugin_call(args: PluginCallArgs, project_root: &str, json: bool) -> Result<()> {
    use animus_control_protocol::types::PluginCallRequest as WirePluginCallRequest;
    use orchestrator_daemon_runtime::control::ControlClient;

    let params = match args.params {
        Some(raw) => Some(serde_json::from_str::<Value>(&raw).context("--params must be valid JSON")?),
        None => None,
    };

    if json {
        let project_root_path = std::path::Path::new(project_root);
        if let Some(client) = ControlClient::try_connect(project_root_path).await? {
            let request = WirePluginCallRequest {
                name: args.name.clone(),
                method: args.method.clone(),
                params: params.clone().unwrap_or(Value::Null),
            };
            match client.plugin_call(request).await {
                Ok(response) => return print_value(response, true),
                Err(err) if orchestrator_daemon_runtime::control::is_method_unavailable(&err) => {
                    tracing::debug!(error = %err, "plugin/call wire returned unavailable; falling back to local");
                }
                Err(err) => return Err(err),
            }
        }
    }

    let output = run_plugin_call(PluginCallRequest {
        project_root: project_root.to_string(),
        name: args.name,
        method: args.method,
        params,
        include_system_path: args.include_system_path,
    })
    .await?;
    print_value(output, json)
}

async fn handle_plugin_ping(args: PluginPingArgs, project_root: &str, json: bool) -> Result<()> {
    use animus_control_protocol::types::PluginPingRequest as WirePluginPingRequest;
    use orchestrator_daemon_runtime::control::ControlClient;

    if json {
        let project_root_path = std::path::Path::new(project_root);
        if let Some(client) = ControlClient::try_connect(project_root_path).await? {
            let request = WirePluginPingRequest { name: args.name.clone() };
            match client.plugin_ping(request).await {
                Ok(response) => return print_value(response, true),
                Err(err) if orchestrator_daemon_runtime::control::is_method_unavailable(&err) => {
                    tracing::debug!(error = %err, "plugin/ping wire returned unavailable; falling back to local");
                }
                Err(err) => return Err(err),
            }
        }
    }

    let output = run_plugin_ping(PluginPingRequest {
        project_root: project_root.to_string(),
        name: args.name,
        include_system_path: args.include_system_path,
    })
    .await?;
    print_value(output, json)
}

/// Resolves the plugin install directory.
///
/// Resolution order:
/// 1. `--plugin-dir <path>` CLI arg (when provided)
/// 2. `$ANIMUS_PLUGIN_DIR` env var (via [`plugin_install_dir`])
/// 3. Default `~/.animus/plugins/`
fn install_root(cli_override: Option<&str>) -> Result<PathBuf> {
    let dir = match cli_override.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => PathBuf::from(value),
        None => plugin_install_dir(),
    };
    std::fs::create_dir_all(&dir).with_context(|| format!("failed to create install dir {}", dir.display()))?;
    Ok(dir)
}

/// Resolves the plugin registry yaml path, performing a one-shot migration from
/// the legacy `~/.config/animus/plugins.yaml` location when needed.
fn plugins_yaml_path() -> Result<PathBuf> {
    let canonical = plugins_registry_path();
    if let Some(parent) = canonical.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("failed to create config dir {}", parent.display()))?;
    }

    if !canonical.exists() {
        let legacy = legacy_plugins_registry_path();
        if legacy.exists() {
            std::fs::copy(&legacy, &canonical).with_context(|| {
                format!("failed to migrate plugin registry from {} to {}", legacy.display(), canonical.display())
            })?;
            tracing::info!(
                from = %legacy.display(),
                to = %canonical.display(),
                "migrated plugin registry to canonical location",
            );
        }
    }

    Ok(canonical)
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
struct PluginsYamlConfig {
    #[serde(default)]
    plugins: serde_yaml::Mapping,
    #[serde(default)]
    providers: serde_yaml::Mapping,
}

fn load_plugins_yaml(path: &Path) -> Result<PluginsYamlConfig> {
    if !path.exists() {
        return Ok(PluginsYamlConfig::default());
    }
    let contents = std::fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_yaml::from_str(&contents).with_context(|| format!("failed to parse {}", path.display()))
}

fn save_plugins_yaml(path: &Path, config: &PluginsYamlConfig) -> Result<()> {
    let serialized = serde_yaml::to_string(config).context("failed to serialize plugins.yaml")?;
    std::fs::write(path, serialized).with_context(|| format!("failed to write {}", path.display()))
}

fn sha256_of_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(unix)]
fn ensure_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(perms.mode() | 0o111);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn ensure_executable(_path: &Path) -> Result<()> {
    Ok(())
}

/// Create a fresh staging directory for a plugin install under
/// `$TMPDIR/animus-plugin-install-<random>`.
///
/// Returns a [`tempfile::TempDir`] guard — when it drops, the directory
/// is removed recursively via RAII. The pre-fix code used
/// `std::env::temp_dir().join(uuid)` and never cleaned up, accumulating
/// GBs of orphaned install dirs over time on power-user machines.
fn create_install_staging_dir() -> Result<tempfile::TempDir> {
    tempfile::Builder::new()
        .prefix("animus-plugin-install-")
        .tempdir()
        .context("failed to create plugin install staging dir")
}

/// Download a plugin asset from `url` to a freshly created staging
/// directory and verify its sha256.
///
/// Returns the on-disk path of the downloaded file **and** the
/// [`tempfile::TempDir`] guard that owns the staging directory. The
/// caller MUST keep the `TempDir` alive until it has copied the binary
/// to its final home — when the guard drops, the staging dir (and the
/// returned path inside it) are deleted via RAII. This closes the GB-of-
/// orphaned-`animus-plugin-install-*` dirs leak that accumulated under
/// `$TMPDIR` over time.
async fn fetch_url_to_temp(url: &str, expected_sha256: &str) -> Result<(PathBuf, tempfile::TempDir)> {
    if !url.starts_with("https://") {
        return Err(invalid_input_error("--url must use https://"));
    }
    let response = reqwest::get(url)
        .await
        .with_context(|| format!("failed to download {url}"))?
        .error_for_status()
        .with_context(|| format!("download from {url} returned non-success status"))?;
    let bytes = response.bytes().await.with_context(|| format!("failed to read body from {url}"))?;

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let computed_sha = format!("{:x}", hasher.finalize());
    if !expected_sha256.eq_ignore_ascii_case(&computed_sha) {
        return Err(invalid_input_error(format!(
            "sha256 mismatch for {url}: expected {expected_sha256}, computed {computed_sha}"
        )));
    }

    let temp_dir = create_install_staging_dir()?;
    let filename = url.rsplit('/').next().unwrap_or("plugin");
    let dest = temp_dir.path().join(filename);
    std::fs::write(&dest, &bytes)
        .with_context(|| format!("failed to write downloaded plugin to {}", dest.display()))?;
    Ok((dest, temp_dir))
}

// ===== Public-repo (GitHub release) install support =====

/// Parsed `owner/repo[@tag]` positional source.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RepoSpec {
    owner: String,
    repo: String,
    tag: Option<String>,
}

/// Parse an `owner/repo` or `owner/repo@tag` slug. Whitespace is trimmed.
fn parse_repo_spec(raw: &str) -> Result<RepoSpec> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(invalid_input_error("repo source must not be empty"));
    }
    let (slug, tag) = match trimmed.split_once('@') {
        Some((slug, tag)) => {
            let tag = tag.trim();
            if tag.is_empty() {
                return Err(invalid_input_error(format!("repo source '{trimmed}' has an empty tag after '@'")));
            }
            (slug.trim(), Some(tag.to_string()))
        }
        None => (trimmed, None),
    };
    let (owner, repo) = slug.split_once('/').ok_or_else(|| {
        invalid_input_error(format!("repo source '{trimmed}' must be in the form 'owner/repo[@tag]'"))
    })?;
    let owner = owner.trim();
    let repo = repo.trim();
    if owner.is_empty() || repo.is_empty() {
        return Err(invalid_input_error(format!("repo source '{trimmed}' must be in the form 'owner/repo[@tag]'")));
    }
    Ok(RepoSpec { owner: owner.to_string(), repo: repo.to_string(), tag })
}

/// Returns the list of platform-target substrings to match against asset names,
/// in priority order. The first asset whose name contains any of these
/// substrings (case-insensitive) is selected.
fn current_platform_tokens() -> &'static [&'static str] {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        &["aarch64-apple-darwin", "macos-aarch64", "darwin-arm64", "darwin-aarch64", "macos-arm64"]
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        &["x86_64-apple-darwin", "macos-x86_64", "darwin-amd64", "darwin-x86_64", "macos-amd64"]
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        &["x86_64-unknown-linux-gnu", "x86_64-unknown-linux-musl", "linux-x86_64", "linux-amd64"]
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        &["aarch64-unknown-linux-gnu", "aarch64-unknown-linux-musl", "linux-aarch64", "linux-arm64"]
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        &["x86_64-pc-windows-msvc", "x86_64-pc-windows-gnu", "windows-x86_64", "windows-amd64"]
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64"),
    )))]
    {
        &[]
    }
}

/// Human-readable label for the current build target.
fn current_platform_label() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

#[derive(Debug, Clone, serde::Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    assets: Vec<GithubReleaseAsset>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct GithubReleaseAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

/// Pure asset-selection helper. Picks the first asset whose name contains any
/// of `platform_tokens` (case-insensitive). Sidecar `.sha256` assets are
/// excluded from candidate matching.
fn pick_release_asset<'a>(
    assets: &'a [GithubReleaseAsset],
    platform_tokens: &[&str],
) -> Option<&'a GithubReleaseAsset> {
    for token in platform_tokens {
        let needle = token.to_ascii_lowercase();
        for asset in assets {
            let lower = asset.name.to_ascii_lowercase();
            if lower.ends_with(".sha256") || lower.ends_with(".sha256sum") {
                continue;
            }
            if lower.contains(&needle) {
                return Some(asset);
            }
        }
    }
    None
}

/// Look up the sidecar `<asset_name>.sha256` in the same release, if present.
fn find_sha256_sidecar<'a>(assets: &'a [GithubReleaseAsset], asset_name: &str) -> Option<&'a GithubReleaseAsset> {
    let sidecar = format!("{asset_name}.sha256");
    assets.iter().find(|a| a.name.eq_ignore_ascii_case(&sidecar))
}

/// Build the GitHub releases API URL for either `latest` or a specific tag.
fn github_release_api_url(owner: &str, repo: &str, tag: Option<&str>) -> String {
    match tag {
        Some(tag) => format!("https://api.github.com/repos/{owner}/{repo}/releases/tags/{tag}"),
        None => format!("https://api.github.com/repos/{owner}/{repo}/releases/latest"),
    }
}

fn release_user_agent() -> String {
    format!("animus-cli/{}", env!("CARGO_PKG_VERSION"))
}

async fn fetch_github_release(owner: &str, repo: &str, tag: Option<&str>) -> Result<GithubRelease> {
    let url = github_release_api_url(owner, repo, tag);
    let client =
        reqwest::Client::builder().user_agent(release_user_agent()).build().context("failed to build HTTP client")?;
    let response = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .with_context(|| format!("failed to GET {url}"))?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(not_found_error(format!(
            "no release found at {url} (check the repo slug, tag, or whether a release has been published yet)"
        )));
    }
    let response = response.error_for_status().with_context(|| format!("GET {url} returned non-success status"))?;
    let release: GithubRelease =
        response.json().await.with_context(|| format!("failed to parse GitHub release JSON from {url}"))?;
    Ok(release)
}

/// Parse an `algo:hex` digest string (as returned by the GitHub API's `digest`
/// field on release assets), returning the lowercased hex if the algorithm is
/// `sha256`. Returns `None` for unsupported algorithms.
fn parse_release_digest(digest: &str) -> Option<String> {
    let trimmed = digest.trim();
    let (algo, hex) = trimmed.split_once(':')?;
    if !algo.eq_ignore_ascii_case("sha256") {
        return None;
    }
    let hex = hex.trim();
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(hex.to_ascii_lowercase())
}

/// Parse a sidecar file body (commonly `<hex>  <filename>\n`), returning the
/// leading hex digest if present.
fn parse_sha256_sidecar(body: &str) -> Option<String> {
    let line = body.lines().next()?.trim();
    let token = line.split_whitespace().next()?;
    if token.len() == 64 && token.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(token.to_ascii_lowercase())
    } else {
        None
    }
}

async fn download_to_path(url: &str, dest: &Path) -> Result<()> {
    let client =
        reqwest::Client::builder().user_agent(release_user_agent()).build().context("failed to build HTTP client")?;
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("failed to download {url}"))?
        .error_for_status()
        .with_context(|| format!("download from {url} returned non-success status"))?;
    let bytes = response.bytes().await.with_context(|| format!("failed to read body from {url}"))?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create download dir {}", parent.display()))?;
    }
    std::fs::write(dest, &bytes).with_context(|| format!("failed to write {}", dest.display()))?;
    Ok(())
}

async fn download_text(url: &str) -> Result<String> {
    let client =
        reqwest::Client::builder().user_agent(release_user_agent()).build().context("failed to build HTTP client")?;
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("failed to download {url}"))?
        .error_for_status()
        .with_context(|| format!("download from {url} returned non-success status"))?;
    response.text().await.with_context(|| format!("failed to read body from {url}"))
}

/// Extract a `.tar.gz` archive into `dest_dir` and pick the plugin binary
/// out of the extracted tree.
///
/// Selection priority (deterministic — `first_file()` was order-dependent
/// and silently installed READMEs):
///
/// 1. A regular file whose basename matches `expected_name` exactly (with
///    or without a `.exe` suffix).
/// 2. If exactly one extracted file has any execute bit set, use it.
/// 3. Otherwise, error with a list of every extracted file so the operator
///    can see why the install was rejected.
///
/// `expected_name` is the plugin name derived from the install source —
/// typically the GitHub repo basename (`animus-provider-claude`).
fn extract_tarball(archive: &Path, dest_dir: &Path, expected_name: &str) -> Result<PathBuf> {
    let file = std::fs::File::open(archive).with_context(|| format!("failed to open {}", archive.display()))?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut tar_reader = tar::Archive::new(gz);
    std::fs::create_dir_all(dest_dir)
        .with_context(|| format!("failed to create extract dir {}", dest_dir.display()))?;
    tar_reader
        .unpack(dest_dir)
        .with_context(|| format!("failed to extract {} into {}", archive.display(), dest_dir.display()))?;

    fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = entry.metadata()?;
            if metadata.is_file() {
                out.push(path);
            } else if metadata.is_dir() {
                collect_files(&path, out)?;
            }
        }
        Ok(())
    }

    let mut files = Vec::new();
    collect_files(dest_dir, &mut files)?;
    if files.is_empty() {
        return Err(anyhow!("tarball {} contained no regular files", archive.display()));
    }

    // 1. Exact basename match (with or without `.exe`).
    if let Some(matched) = files.iter().find(|path| {
        let Some(base) = path.file_name().and_then(|n| n.to_str()) else {
            return false;
        };
        base.eq_ignore_ascii_case(expected_name) || base.eq_ignore_ascii_case(&format!("{expected_name}.exe"))
    }) {
        return Ok(matched.clone());
    }

    // 2. Sole executable.
    let executables: Vec<&PathBuf> = files.iter().filter(|p| is_executable_file(p)).collect();
    if executables.len() == 1 {
        return Ok(executables[0].clone());
    }

    // 3. Ambiguous — list every file we extracted so operators can see why
    //    we refused to guess.
    let names: Vec<String> = files
        .iter()
        .filter_map(|p| p.strip_prefix(dest_dir).ok().and_then(|rel| rel.to_str()).map(str::to_string))
        .collect();
    Err(anyhow!(
        "could not determine which file is the plugin binary in {}; expected one named '{}'. Extracted files: [{}]",
        archive.display(),
        expected_name,
        names.join(", ")
    ))
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).map(|m| m.is_file() && (m.permissions().mode() & 0o111) != 0).unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    // On non-unix targets the executable bit isn't a stable signal — fall
    // back to existence + regular-file. Selection then relies on the
    // basename-match path (which is the common case for our releases).
    std::fs::metadata(path).map(|m| m.is_file()).unwrap_or(false)
}

/// Result of resolving a public-repo install source.
#[derive(Debug)]
struct ReleaseInstall {
    binary_path: PathBuf,
    plugin_name_hint: String,
    asset_name: String,
    release_tag: String,
    origin: String,
    sha256_verified: bool,
    /// Downloaded asset archive (`.tar.gz` etc.) — what cosign signed.
    asset_archive_path: Option<PathBuf>,
    /// Local path of the `.bundle` sidecar, when one was published alongside the asset.
    bundle_path: Option<PathBuf>,
    /// `<owner>` of the GitHub repo, for identity matching.
    owner: String,
    /// `<repo>` of the GitHub repo, for identity matching.
    repo: String,
    /// RAII guard for the staging directory the asset was downloaded into.
    /// All paths above point inside this guard's directory; the caller
    /// must keep `_temp_dir` alive until the binary has been copied to
    /// its final home. Dropping the guard recursively removes the staging
    /// dir — closes the `$TMPDIR/animus-plugin-install-*` leak.
    _temp_dir: tempfile::TempDir,
}

async fn resolve_release_install(spec: RepoSpec, explicit_tag: Option<String>) -> Result<ReleaseInstall> {
    let tag = match (spec.tag.clone(), explicit_tag) {
        (Some(spec_tag), Some(flag_tag)) => {
            if spec_tag != flag_tag {
                return Err(invalid_input_error(format!(
                    "conflicting tag: positional says '{spec_tag}', --tag says '{flag_tag}'"
                )));
            }
            Some(spec_tag)
        }
        (Some(tag), None) | (None, Some(tag)) => Some(tag),
        (None, None) => None,
    };

    let release = fetch_github_release(&spec.owner, &spec.repo, tag.as_deref()).await?;
    let platform_tokens = current_platform_tokens();
    if platform_tokens.is_empty() {
        return Err(invalid_input_error(format!(
            "current platform '{}' is not supported by `animus plugin install` (no asset selectors registered)",
            current_platform_label()
        )));
    }

    let asset = pick_release_asset(&release.assets, platform_tokens).ok_or_else(|| {
        let available: Vec<String> = release.assets.iter().map(|a| a.name.clone()).collect();
        invalid_input_error(format!(
            "no release asset matched current platform '{}' (looked for any of: {}). Available assets in {}: [{}]",
            current_platform_label(),
            platform_tokens.join(", "),
            release.tag_name,
            available.join(", ")
        ))
    })?;

    // RAII staging dir — drops when `ReleaseInstall` drops. Replaces the
    // pre-fix `std::env::temp_dir().join(uuid)` that was created and
    // never cleaned up, accumulating GBs of orphaned staging dirs under
    // `$TMPDIR` over time.
    let temp_dir = create_install_staging_dir()?;
    let temp_path = temp_dir.path().to_path_buf();

    let asset_path = temp_path.join(&asset.name);
    download_to_path(&asset.browser_download_url, &asset_path).await?;

    // Resolve expected SHA256: sidecar asset > release `digest` field.
    let mut expected_sha: Option<String> = None;
    if let Some(sidecar_asset) = find_sha256_sidecar(&release.assets, &asset.name) {
        match download_text(&sidecar_asset.browser_download_url).await {
            Ok(body) => {
                if let Some(hex) = parse_sha256_sidecar(&body) {
                    expected_sha = Some(hex);
                } else {
                    eprintln!(
                        "warning: sha256 sidecar '{}' had unexpected format; skipping verification",
                        sidecar_asset.name
                    );
                }
            }
            Err(err) => {
                eprintln!("warning: failed to download sha256 sidecar '{}': {}", sidecar_asset.name, err);
            }
        }
    }
    if expected_sha.is_none() {
        if let Some(digest) = asset.digest.as_deref() {
            if let Some(hex) = parse_release_digest(digest) {
                expected_sha = Some(hex);
            }
        }
    }

    let mut sha256_verified = false;
    let computed = sha256_of_file(&asset_path)?;
    if let Some(expected) = expected_sha.as_ref() {
        if !expected.eq_ignore_ascii_case(&computed) {
            return Err(invalid_input_error(format!(
                "sha256 mismatch for asset '{}': expected {expected}, computed {computed}",
                asset.name
            )));
        }
        sha256_verified = true;
    } else {
        eprintln!(
            "warning: no sha256 sidecar or digest for asset '{}'; install proceeding without checksum verification",
            asset.name
        );
    }

    // Extract if tarball; otherwise treat as a bare binary.
    // The expected plugin binary basename is the GitHub repo name —
    // releases publish `animus-provider-foo-<target>.tar.gz` containing a
    // single binary named `animus-provider-foo`. Passing the repo name
    // lets `extract_tarball` deterministically reject tarballs that ship
    // README/LICENSE alongside the binary instead of installing whatever
    // happened to come back first in walk order.
    let lower = asset.name.to_ascii_lowercase();
    let binary_path = if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        let extract_dir = temp_path.join("extracted");
        extract_tarball(&asset_path, &extract_dir, &spec.repo)?
    } else {
        asset_path.clone()
    };

    // Download the cosign signature bundle if one is published. Bundles match
    // the original archive (not the extracted binary).
    let bundle_path = match find_bundle_sidecar(&release.assets, &asset.name) {
        Some(bundle_asset) => {
            let local = temp_path.join(&bundle_asset.name);
            match download_to_path(&bundle_asset.browser_download_url, &local).await {
                Ok(()) => Some(local),
                Err(err) => {
                    eprintln!("warning: failed to download cosign bundle '{}': {}", bundle_asset.name, err);
                    None
                }
            }
        }
        None => None,
    };

    let plugin_name_hint = binary_path
        .file_name()
        .and_then(|f| f.to_str())
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| spec.repo.clone());

    Ok(ReleaseInstall {
        binary_path,
        plugin_name_hint,
        asset_name: asset.name.clone(),
        release_tag: release.tag_name.clone(),
        origin: format!("{}/{}@{}", spec.owner, spec.repo, release.tag_name),
        sha256_verified,
        asset_archive_path: Some(asset_path.clone()),
        bundle_path,
        owner: spec.owner.clone(),
        repo: spec.repo.clone(),
        _temp_dir: temp_dir,
    })
}

/// Look up the cosign signature bundle (`<asset>.bundle`) in the release
/// assets, if present.
fn find_bundle_sidecar<'a>(assets: &'a [GithubReleaseAsset], asset_name: &str) -> Option<&'a GithubReleaseAsset> {
    let bundle_name = format!("{asset_name}.bundle");
    assets.iter().find(|a| a.name.eq_ignore_ascii_case(&bundle_name))
}

/// Verify the cosign signature for the install source (if any), apply the
/// trusted-signers policy, and return the resulting [`SignatureStatus`]. The
/// caller is responsible for turning hard-fail statuses (`Invalid`,
/// `UntrustedSigner`, `Unsigned` under `--require-signature`) into install
/// errors.
fn resolve_signature_status(req: &PluginInstallRequest, provenance: &InstallProvenance) -> Result<SignatureStatus> {
    if req.skip_signature {
        return Ok(SignatureStatus::Skipped);
    }

    let (Some(asset_archive), Some(bundle_path)) =
        (provenance.asset_archive_path.as_deref(), provenance.bundle_path.as_deref())
    else {
        return Ok(SignatureStatus::Unsigned {
            reason: match provenance.source_kind {
                Some("release") => "no cosign signature bundle published in release".to_string(),
                Some("path") => "local --path install; cosign signatures only apply to release assets".to_string(),
                Some("url") => "direct --url install; cosign signatures only apply to release assets".to_string(),
                _ => "no signature context available for this install source".to_string(),
            },
        });
    };

    let signers_path = resolve_trusted_signers_path(req.trusted_signers.as_deref());
    let trusted = load_trusted_signers(&signers_path)?;
    let identity_regex = if let (Some(owner), Some(repo)) = (provenance.owner.as_deref(), provenance.repo.as_deref()) {
        let cfg = trusted.clone().unwrap_or_default();
        Some(cfg.identity_regexp_for(owner, repo))
    } else {
        None
    };

    if !cosign_available() {
        return Ok(SignatureStatus::Unsigned {
            reason: "cosign binary not found on PATH; install cosign from https://github.com/sigstore/cosign to enable signature verification".to_string(),
        });
    }

    let status = verify_with_cosign(asset_archive, bundle_path, identity_regex.as_deref(), GITHUB_OIDC_ISSUER)?;
    if let SignatureStatus::Verified { .. } = &status {
        if let Some(cfg) = trusted.as_ref() {
            if let (Some(owner), Some(repo)) = (provenance.owner.as_deref(), provenance.repo.as_deref()) {
                let slug = format!("{owner}/{repo}");
                if !cfg.matches_repo(&slug) {
                    return Ok(SignatureStatus::UntrustedSigner {
                        identity_pattern: identity_regex.unwrap_or_else(|| ".*".to_string()),
                    });
                }
            }
        }
    }
    Ok(status)
}

/// Provenance attached to a resolved install source. Recorded in the registry
/// and surfaced in the install output.
#[derive(Debug, Default)]
struct InstallProvenance {
    source_kind: Option<&'static str>,
    origin: Option<String>,
    release_tag: Option<String>,
    asset_name: Option<String>,
    /// `Some(true)` if checksum verification ran and passed during resolution.
    sha256_verified: Option<bool>,
    /// Path to the archive that cosign signs (the `.tar.gz`, not the extracted binary).
    asset_archive_path: Option<PathBuf>,
    /// Local path to the cosign `.bundle`, when published.
    bundle_path: Option<PathBuf>,
    /// `<owner>` for identity-regex construction.
    owner: Option<String>,
    /// `<repo>` for identity-regex construction.
    repo: Option<String>,
}

/// Probe a plugin binary's `--manifest` output without touching the install
/// directory. Used to validate identity (name vs repo) and policy
/// (reserved-provider-tool) before the install commits.
fn probe_manifest(binary_path: &Path) -> Result<PluginManifest> {
    let output = std::process::Command::new(binary_path)
        .arg("--manifest")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to run {} --manifest", binary_path.display()))?;
    if !output.status.success() {
        return Err(anyhow!(
            "binary failed --manifest probe (exit={:?}): {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice::<PluginManifest>(&output.stdout)
        .with_context(|| format!("plugin {} returned malformed --manifest JSON", binary_path.display()))
}

/// Refuse provider plugins whose manifest name (or `animus-provider-*` suffix)
/// claims one of the in-tree `RESERVED_PROVIDER_TOOLS`. A misconfigured or
/// malicious plugin can otherwise replace the entire `claude` / `codex` /
/// `gemini` / `opencode` / `oai-runner` dispatch path without warning.
fn enforce_provider_tool_policy(manifest: &PluginManifest, allow_shadow_builtin: bool) -> Result<()> {
    if manifest.plugin_kind != animus_plugin_protocol::PLUGIN_KIND_PROVIDER {
        return Ok(());
    }
    let derived_tool = manifest.name.strip_prefix("animus-provider-").unwrap_or(manifest.name.as_str());
    if !is_reserved_provider_tool(derived_tool) {
        return Ok(());
    }
    if allow_shadow_builtin {
        tracing::warn!(
            plugin = %manifest.name,
            tool = %derived_tool,
            "installing plugin that shadows the in-tree '{}' backend (--allow-shadow-builtin)",
            derived_tool,
        );
        return Ok(());
    }
    Err(invalid_input_error(format!(
        "plugin '{}' resolves to provider_tool '{}', which is a reserved in-tree backend \
         (claude / codex / gemini / opencode / oai-runner). Installing it would silently \
         override the built-in dispatch for that tool. Pass --allow-shadow-builtin to proceed.",
        manifest.name, derived_tool
    )))
}

/// Refuse installs whose published manifest name disagrees with the GitHub
/// repo basename it was downloaded from. This is the most common shape of a
/// supply-chain typosquat (`evil-org/animus-provider-claude` shipping a binary
/// whose manifest is `animus-provider-claude` from `launchapp-dev`).
fn enforce_manifest_name_matches_repo(manifest: &PluginManifest, _owner: &str, repo: &str, force: bool) -> Result<()> {
    if manifest.name == repo {
        return Ok(());
    }
    let message = format!(
        "manifest name '{}' does not match repo basename '{}' — this may be a typosquat or supply-chain attack. \
         Pass --force to install anyway.",
        manifest.name, repo
    );
    if force {
        tracing::warn!(
            manifest_name = %manifest.name,
            repo = %repo,
            "installing plugin with manifest/repo basename mismatch (--force)"
        );
        return Ok(());
    }
    Err(invalid_input_error(message))
}

/// Path to the trusted-orgs allowlist used by `animus plugin install`.
///
/// Honors `$ANIMUS_TRUSTED_ORGS` first, then falls back to
/// `<animus_home>/trusted-orgs.yaml`.
fn trusted_orgs_path() -> PathBuf {
    if let Ok(value) = std::env::var("ANIMUS_TRUSTED_ORGS") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let base = match std::env::var("ANIMUS_CONFIG_DIR") {
        Ok(value) if !value.trim().is_empty() => PathBuf::from(value),
        _ => home.join(".animus"),
    };
    base.join("trusted-orgs.yaml")
}

/// Built-in trusted orgs. Pre-populated with `launchapp-dev` so a fresh
/// install gets a safe default for the canonical animus plugins.
const BUILTIN_TRUSTED_ORGS: &[&str] = &["launchapp-dev"];

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct TrustedOrgsConfig {
    #[serde(default)]
    trusted_orgs: Vec<String>,
}

fn load_trusted_orgs() -> Result<TrustedOrgsConfig> {
    let path = trusted_orgs_path();
    if !path.exists() {
        return Ok(TrustedOrgsConfig::default());
    }
    let contents = std::fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let parsed: TrustedOrgsConfig = serde_yaml::from_str(&contents)
        .with_context(|| format!("failed to parse {} as TrustedOrgsConfig", path.display()))?;
    Ok(parsed)
}

fn save_trusted_orgs(config: &TrustedOrgsConfig) -> Result<()> {
    let path = trusted_orgs_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create trusted-orgs dir {}", parent.display()))?;
    }
    let serialized = serde_yaml::to_string(config).context("failed to serialize trusted-orgs.yaml")?;
    std::fs::write(&path, serialized).with_context(|| format!("failed to write {}", path.display()))
}

/// Append `owner` to the trusted-orgs allowlist on disk. Idempotent.
fn add_trusted_org(owner: &str) -> Result<()> {
    let trimmed = owner.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let mut config = load_trusted_orgs()?;
    let already_known: BTreeSet<String> = config.trusted_orgs.iter().map(|o| o.to_ascii_lowercase()).collect();
    if already_known.contains(&trimmed.to_ascii_lowercase()) {
        return Ok(());
    }
    if BUILTIN_TRUSTED_ORGS.iter().any(|o| o.eq_ignore_ascii_case(trimmed)) {
        return Ok(());
    }
    config.trusted_orgs.push(trimmed.to_string());
    save_trusted_orgs(&config)
}

fn org_is_trusted(owner: &str) -> Result<bool> {
    if BUILTIN_TRUSTED_ORGS.iter().any(|o| o.eq_ignore_ascii_case(owner)) {
        return Ok(true);
    }
    let config = load_trusted_orgs()?;
    Ok(config.trusted_orgs.iter().any(|o| o.eq_ignore_ascii_case(owner)))
}

/// Implements the trust-on-first-use prompt for installs from public-repo
/// sources. Pre-trusted orgs (`launchapp-dev` plus anything in
/// `~/.animus/trusted-orgs.yaml`) skip the prompt entirely. Operators can
/// pre-trust additional orgs via `--allow-org`, or auto-confirm via `--yes`.
fn enforce_org_trust(owner: &str, req: &PluginInstallRequest) -> Result<()> {
    if req.allow_org.iter().any(|o| o.eq_ignore_ascii_case(owner)) {
        return Ok(());
    }
    if org_is_trusted(owner)? {
        return Ok(());
    }
    if req.yes || req.force {
        tracing::warn!(owner, "installing plugin from untrusted org (--yes / --force); recording trust on first use");
        return Ok(());
    }
    if !std::io::stdin().is_terminal() {
        return Err(invalid_input_error(format!(
            "installing plugin from untrusted org '{owner}'. Pass --allow-org {owner} (or --yes) to confirm \
             non-interactively. trusted-orgs.yaml lives at {}.",
            trusted_orgs_path().display()
        )));
    }
    eprintln!(
        "warning: you are installing a plugin from `{owner}`, which is not a trusted organization.\n\
         Verify this is the intended publisher before continuing. Type 'yes' to trust this org \
         for future installs, anything else to abort."
    );
    eprint!("> ");
    let _ = std::io::stderr().flush();
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer).with_context(|| "failed to read TOFU response from stdin")?;
    let normalized = answer.trim().to_ascii_lowercase();
    if normalized == "yes" || normalized == "y" {
        Ok(())
    } else {
        Err(invalid_input_error(format!("user declined to trust org '{owner}'; aborting install")))
    }
}

async fn handle_plugin_install(args: PluginInstallArgs, json: bool) -> Result<()> {
    if args.latest && args.tag.is_some() {
        return Err(invalid_input_error("--latest and --tag are mutually exclusive"));
    }
    if (args.tag.is_some() || args.latest) && args.source.is_none() {
        return Err(invalid_input_error(
            "--tag and --latest only apply when installing from a public repo (positional OWNER/REPO[@TAG])",
        ));
    }
    let output = run_plugin_install(PluginInstallRequest {
        source: args.source,
        path: args.path,
        url: args.url,
        tag: args.tag,
        name: args.name,
        sha256: args.sha256,
        force: args.force,
        skip_manifest_check: args.skip_manifest_check,
        plugin_dir: args.plugin_dir,
        require_signature: args.require_signature,
        skip_signature: args.skip_signature,
        trusted_signers: args.trusted_signers,
        allow_shadow_builtin: args.allow_shadow_builtin,
        allow_org: args.allow_org,
        yes: args.yes,
    })
    .await?;
    print_value(output, json)
}

fn handle_plugin_uninstall(args: PluginUninstallArgs, json: bool) -> Result<()> {
    let output = run_plugin_uninstall(PluginUninstallRequest { name: args.name, plugin_dir: args.plugin_dir })?;
    print_value(output, json)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> GithubReleaseAsset {
        GithubReleaseAsset {
            name: name.to_string(),
            browser_download_url: format!("https://example.test/{name}"),
            digest: None,
        }
    }

    #[test]
    fn parses_repo_at_tag_syntax() {
        let spec = parse_repo_spec("launchapp-dev/animus-provider-claude@v0.1.0").unwrap();
        assert_eq!(spec.owner, "launchapp-dev");
        assert_eq!(spec.repo, "animus-provider-claude");
        assert_eq!(spec.tag.as_deref(), Some("v0.1.0"));
    }

    #[test]
    fn parses_repo_without_tag() {
        let spec = parse_repo_spec("launchapp-dev/animus-provider-claude").unwrap();
        assert_eq!(spec.owner, "launchapp-dev");
        assert_eq!(spec.repo, "animus-provider-claude");
        assert!(spec.tag.is_none());
    }

    #[test]
    fn parse_repo_spec_trims_whitespace() {
        let spec = parse_repo_spec("  launchapp-dev/foo @ v1.2.3  ").unwrap();
        assert_eq!(spec.owner, "launchapp-dev");
        assert_eq!(spec.repo, "foo");
        assert_eq!(spec.tag.as_deref(), Some("v1.2.3"));
    }

    #[test]
    fn parse_repo_spec_rejects_missing_slash() {
        let err = parse_repo_spec("animus-provider-claude").unwrap_err();
        assert!(format!("{err}").contains("owner/repo"));
    }

    #[test]
    fn parse_repo_spec_rejects_empty_tag() {
        let err = parse_repo_spec("launchapp-dev/foo@").unwrap_err();
        assert!(format!("{err}").contains("empty tag"));
    }

    #[test]
    fn parse_repo_spec_rejects_empty_owner_or_repo() {
        assert!(parse_repo_spec("/foo").is_err());
        assert!(parse_repo_spec("foo/").is_err());
        assert!(parse_repo_spec("").is_err());
    }

    #[test]
    fn selects_aarch64_apple_darwin_asset() {
        let assets = vec![
            asset("animus-provider-oai-aarch64-apple-darwin.tar.gz"),
            asset("animus-provider-oai-x86_64-apple-darwin.tar.gz"),
            asset("animus-provider-oai-x86_64-unknown-linux-gnu.tar.gz"),
        ];
        let tokens: &[&str] = &["aarch64-apple-darwin", "macos-aarch64", "darwin-arm64"];
        let picked = pick_release_asset(&assets, tokens).expect("expected an asset to match");
        assert_eq!(picked.name, "animus-provider-oai-aarch64-apple-darwin.tar.gz");
    }

    #[test]
    fn selects_x86_64_linux_asset() {
        let assets = vec![
            asset("animus-provider-oai-aarch64-apple-darwin.tar.gz"),
            asset("animus-provider-oai-x86_64-apple-darwin.tar.gz"),
            asset("animus-provider-oai-x86_64-unknown-linux-gnu.tar.gz"),
        ];
        let tokens: &[&str] = &["x86_64-unknown-linux-gnu", "linux-x86_64", "linux-amd64"];
        let picked = pick_release_asset(&assets, tokens).expect("expected an asset to match");
        assert_eq!(picked.name, "animus-provider-oai-x86_64-unknown-linux-gnu.tar.gz");
    }

    #[test]
    fn errors_clearly_when_no_matching_asset() {
        let assets = vec![asset("animus-provider-oai-x86_64-unknown-linux-gnu.tar.gz")];
        let tokens: &[&str] = &["aarch64-apple-darwin", "macos-aarch64"];
        assert!(pick_release_asset(&assets, tokens).is_none());
    }

    #[test]
    fn current_platform_has_known_tokens() {
        let tokens = current_platform_tokens();
        assert!(!tokens.is_empty(), "no platform tokens registered for {}", current_platform_label());
    }

    #[test]
    fn excludes_sha256_sidecars_from_asset_picking() {
        let assets = vec![
            asset("animus-provider-oai-aarch64-apple-darwin.tar.gz.sha256"),
            asset("animus-provider-oai-aarch64-apple-darwin.tar.gz"),
        ];
        let tokens: &[&str] = &["aarch64-apple-darwin"];
        let picked = pick_release_asset(&assets, tokens).expect("expected the binary asset to win");
        assert_eq!(picked.name, "animus-provider-oai-aarch64-apple-darwin.tar.gz");
    }

    #[test]
    fn verifies_sha256_sidecar_when_present() {
        let assets = vec![
            asset("animus-provider-oai-aarch64-apple-darwin.tar.gz"),
            asset("animus-provider-oai-aarch64-apple-darwin.tar.gz.sha256"),
        ];
        let sidecar = find_sha256_sidecar(&assets, "animus-provider-oai-aarch64-apple-darwin.tar.gz")
            .expect("expected sidecar to be found");
        assert_eq!(sidecar.name, "animus-provider-oai-aarch64-apple-darwin.tar.gz.sha256");

        let body =
            "a10a3a505ca102bc4249d4e660f0622278abd319054a2e033b72988783ea7a48  animus-provider-oai-aarch64-apple-darwin.tar.gz\n";
        let hex = parse_sha256_sidecar(body).unwrap();
        assert_eq!(hex, "a10a3a505ca102bc4249d4e660f0622278abd319054a2e033b72988783ea7a48");
    }

    #[test]
    fn returns_none_when_no_sha256_sidecar() {
        let assets = vec![asset("animus-provider-oai-aarch64-apple-darwin.tar.gz")];
        assert!(find_sha256_sidecar(&assets, "animus-provider-oai-aarch64-apple-darwin.tar.gz").is_none());
    }

    #[test]
    fn parses_release_digest_field() {
        let hex =
            parse_release_digest("sha256:a10a3a505ca102bc4249d4e660f0622278abd319054a2e033b72988783ea7a48").unwrap();
        assert_eq!(hex, "a10a3a505ca102bc4249d4e660f0622278abd319054a2e033b72988783ea7a48");
        assert!(parse_release_digest("md5:deadbeef").is_none());
        assert!(parse_release_digest("sha256:").is_none());
        assert!(parse_release_digest("not-a-digest").is_none());
        assert!(parse_release_digest("sha256:deadbeef").is_none()); // too short
    }

    #[test]
    fn parses_hex_only_sidecar_body() {
        let body = "A10A3A505CA102BC4249D4E660F0622278ABD319054A2E033B72988783EA7A48\n";
        let hex = parse_sha256_sidecar(body).unwrap();
        assert_eq!(hex, "a10a3a505ca102bc4249d4e660f0622278abd319054a2e033b72988783ea7a48");
    }

    #[test]
    fn rejects_malformed_sidecar_body() {
        assert!(parse_sha256_sidecar("").is_none());
        assert!(parse_sha256_sidecar("not-a-hex-digest").is_none());
        assert!(parse_sha256_sidecar("deadbeef").is_none()); // too short
    }

    #[test]
    fn github_release_api_url_is_correct() {
        assert_eq!(
            github_release_api_url("launchapp-dev", "animus-provider-oai", None),
            "https://api.github.com/repos/launchapp-dev/animus-provider-oai/releases/latest"
        );
        assert_eq!(
            github_release_api_url("launchapp-dev", "animus-provider-oai", Some("v0.1.0")),
            "https://api.github.com/repos/launchapp-dev/animus-provider-oai/releases/tags/v0.1.0"
        );
    }

    #[test]
    fn extract_tarball_returns_named_binary() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("plugin.tar.gz");
        let bin_path = dir.path().join("animus-provider-foo");
        std::fs::File::create(&bin_path).unwrap().write_all(b"#!/bin/sh\necho ok\n").unwrap();
        {
            let file = std::fs::File::create(&archive_path).unwrap();
            let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut builder = tar::Builder::new(enc);
            builder.append_path_with_name(&bin_path, "animus-provider-foo").unwrap();
            let enc = builder.into_inner().unwrap();
            enc.finish().unwrap();
        }

        let extract_dir = dir.path().join("extracted");
        let extracted = extract_tarball(&archive_path, &extract_dir, "animus-provider-foo").unwrap();
        assert_eq!(extracted.file_name().and_then(|n| n.to_str()), Some("animus-provider-foo"));
        assert!(extracted.exists());
    }

    /// Tarball containing README + LICENSE alongside the binary. Pre-fix
    /// behavior installed the README. With basename matching the binary
    /// always wins.
    #[test]
    fn extract_tarball_prefers_matching_basename() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("plugin.tar.gz");

        // Write three files into a staging dir, then archive all three.
        let staging = dir.path().join("staging");
        std::fs::create_dir_all(&staging).unwrap();
        let readme = staging.join("README.md");
        let license = staging.join("LICENSE");
        let binary = staging.join("animus-provider-foo");
        std::fs::File::create(&readme).unwrap().write_all(b"# Foo Plugin\n").unwrap();
        std::fs::File::create(&license).unwrap().write_all(b"MIT\n").unwrap();
        std::fs::File::create(&binary).unwrap().write_all(b"#!/bin/sh\necho ok\n").unwrap();

        let file = std::fs::File::create(&archive_path).unwrap();
        let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(enc);
        builder.append_path_with_name(&readme, "README.md").unwrap();
        builder.append_path_with_name(&license, "LICENSE").unwrap();
        builder.append_path_with_name(&binary, "animus-provider-foo").unwrap();
        let enc = builder.into_inner().unwrap();
        enc.finish().unwrap();

        let extract_dir = dir.path().join("extracted");
        let extracted = extract_tarball(&archive_path, &extract_dir, "animus-provider-foo").unwrap();
        assert_eq!(
            extracted.file_name().and_then(|n| n.to_str()),
            Some("animus-provider-foo"),
            "basename match must win even when README/LICENSE come first in walk order"
        );
    }

    /// No file name-matches the expected plugin name, but exactly one is
    /// executable. The executable wins. Mirrors releases that publish
    /// `<plugin>-<version>` style binaries.
    #[cfg(unix)]
    #[test]
    fn extract_tarball_picks_only_executable_when_no_name_match() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("plugin.tar.gz");

        let staging = dir.path().join("staging");
        std::fs::create_dir_all(&staging).unwrap();
        let readme = staging.join("README.md");
        let binary = staging.join("animus-provider-foo-v0.1.0");
        std::fs::File::create(&readme).unwrap().write_all(b"# readme\n").unwrap();
        std::fs::File::create(&binary).unwrap().write_all(b"#!/bin/sh\necho ok\n").unwrap();
        // Mark the binary executable; README stays mode 0o644.
        let mut perms = std::fs::metadata(&binary).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&binary, perms).unwrap();

        let file = std::fs::File::create(&archive_path).unwrap();
        let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(enc);
        builder.append_path_with_name(&readme, "README.md").unwrap();
        builder.append_path_with_name(&binary, "animus-provider-foo-v0.1.0").unwrap();
        let enc = builder.into_inner().unwrap();
        enc.finish().unwrap();

        let extract_dir = dir.path().join("extracted");
        let extracted = extract_tarball(&archive_path, &extract_dir, "animus-provider-foo").unwrap();
        assert_eq!(
            extracted.file_name().and_then(|n| n.to_str()),
            Some("animus-provider-foo-v0.1.0"),
            "the sole executable must win when no basename matches"
        );
    }

    // =================== Tempdir cleanup tests (gap #13) ===================
    //
    // The pre-fix code created `std::env::temp_dir().join(uuid)` and
    // never removed it; the fix wraps creation in a [`tempfile::TempDir`]
    // RAII guard. These tests track the *specific* staging dir created
    // by `create_install_staging_dir()` (uuid-suffixed, can't collide
    // with parallel tests' tempdirs) and assert it disappears once the
    // guard drops — both on the happy path and when a downstream step
    // errors before the binary is copied to its final home.

    /// On success: the staging dir lives until the `TempDir` guard is
    /// dropped, then disappears. Mirrors the install pipeline's
    /// contract: caller holds the guard while copying out the binary,
    /// then drops it.
    #[test]
    fn install_staging_dir_cleaned_up_on_success() {
        let staging_path: PathBuf;
        {
            let staging = create_install_staging_dir().expect("create staging");
            staging_path = staging.path().to_path_buf();
            assert!(staging_path.exists(), "staging dir should exist while guard is held");
            // Sanity: the dir lives in the platform temp dir and uses
            // the documented `animus-plugin-install-` prefix so logs and
            // cleanup scripts can find it.
            let basename = staging_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            assert!(
                basename.starts_with("animus-plugin-install-"),
                "staging dir basename must start with `animus-plugin-install-`, got '{basename}'"
            );
            assert!(
                staging_path.starts_with(std::env::temp_dir()),
                "staging dir must live under the platform temp dir, got {staging_path:?}"
            );
        } // drop

        // After: RAII removed the staging dir.
        assert!(!staging_path.exists(), "staging dir must be removed when TempDir drops; leaked: {staging_path:?}");
    }

    /// On failure: even when the caller errors after creating the
    /// staging dir, the RAII guard drop still cleans it up. Simulates
    /// "download then sha256 mismatch" / "extraction failed" /
    /// "manifest probe rejected" paths.
    #[test]
    fn install_staging_dir_cleaned_up_on_failure() {
        let staging_path: PathBuf = (|| -> Result<PathBuf> {
            let staging = create_install_staging_dir()?;
            let path = staging.path().to_path_buf();
            // Mirror the real install pipeline: write a "download" into
            // the staging dir, then fail before copying it out.
            std::fs::write(path.join("downloaded.tar.gz"), b"bytes")?;
            assert!(path.exists());
            // Return the path — `staging` drops here as it leaves the
            // closure, simulating the install function returning Err.
            Ok(path)
        })()
        .expect("setup must not fail");

        assert!(
            !staging_path.exists(),
            "staging dir must be removed even when the install path errored before copy; leaked: {staging_path:?}"
        );
    }

    /// Tarball with two non-matching, non-executable files. We must error
    /// loudly and list every file rather than silently install whichever
    /// came back first.
    #[test]
    fn extract_tarball_errors_clearly_on_ambiguous_content() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("plugin.tar.gz");

        let staging = dir.path().join("staging");
        std::fs::create_dir_all(&staging).unwrap();
        let readme = staging.join("README.md");
        let license = staging.join("LICENSE");
        std::fs::File::create(&readme).unwrap().write_all(b"# readme\n").unwrap();
        std::fs::File::create(&license).unwrap().write_all(b"MIT\n").unwrap();

        let file = std::fs::File::create(&archive_path).unwrap();
        let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(enc);
        builder.append_path_with_name(&readme, "README.md").unwrap();
        builder.append_path_with_name(&license, "LICENSE").unwrap();
        let enc = builder.into_inner().unwrap();
        enc.finish().unwrap();

        let extract_dir = dir.path().join("extracted");
        let err = extract_tarball(&archive_path, &extract_dir, "animus-provider-foo")
            .expect_err("ambiguous tarball must not silently install");
        let reason = format!("{err:#}");
        assert!(reason.contains("animus-provider-foo"), "error must name expected plugin, got: {reason}");
        assert!(reason.contains("README.md"), "error must list README.md, got: {reason}");
        assert!(reason.contains("LICENSE"), "error must list LICENSE, got: {reason}");
    }

    // =================== Signature verification tests ===================

    fn release_provenance_with_bundle(bundle: Option<std::path::PathBuf>) -> InstallProvenance {
        InstallProvenance {
            source_kind: Some("release"),
            origin: Some("launchapp-dev/animus-provider-claude@v0.1.2".to_string()),
            release_tag: Some("v0.1.2".to_string()),
            asset_name: Some("animus-provider-claude-aarch64-apple-darwin.tar.gz".to_string()),
            sha256_verified: Some(true),
            asset_archive_path: Some(std::path::PathBuf::from("/tmp/example.tar.gz")),
            bundle_path: bundle,
            owner: Some("launchapp-dev".to_string()),
            repo: Some("animus-provider-claude".to_string()),
        }
    }

    #[test]
    fn marks_unsigned_when_no_bundle_in_release() {
        let req = PluginInstallRequest::default();
        let prov = release_provenance_with_bundle(None);
        let status = resolve_signature_status(&req, &prov).expect("status should resolve");
        assert_eq!(status.label(), "unsigned");
    }

    #[test]
    fn refuses_install_when_require_signature_and_no_bundle() {
        let status = SignatureStatus::Unsigned { reason: "no bundle".to_string() };
        let require_signature = true;
        let blocked = matches!(&status, SignatureStatus::Unsigned { .. }) && require_signature;
        assert!(blocked);
    }

    #[test]
    fn skips_verification_when_skip_signature_flag() {
        let req = PluginInstallRequest { skip_signature: true, ..Default::default() };
        let prov = release_provenance_with_bundle(Some(std::path::PathBuf::from("/tmp/x.bundle")));
        let status = resolve_signature_status(&req, &prov).expect("status should resolve");
        assert_eq!(status, SignatureStatus::Skipped);
    }

    #[test]
    fn falls_back_to_unsigned_when_cosign_not_in_path() {
        let req = PluginInstallRequest::default();
        let tmp = tempfile::tempdir().unwrap();
        let fake_bundle = tmp.path().join("fake.bundle");
        std::fs::write(&fake_bundle, b"not a real bundle").unwrap();
        let prov = release_provenance_with_bundle(Some(fake_bundle));
        let status = resolve_signature_status(&req, &prov).expect("status should resolve");
        // Without cosign on PATH: Unsigned. With cosign: Invalid (fake bytes).
        assert!(matches!(&status, SignatureStatus::Unsigned { .. } | SignatureStatus::Invalid { .. }));
    }

    #[test]
    fn verifies_signature_when_bundle_present_and_cosign_works() {
        let verified = SignatureStatus::Verified {
            identity: "^https://github\\.com/launchapp-dev/animus-provider-claude/.+".to_string(),
            bundle_path: "/tmp/x.bundle".to_string(),
        };
        assert_eq!(verified.label(), "verified");
        let blocked = matches!(&verified, SignatureStatus::Unsigned { .. }) && true;
        assert!(!blocked, "Verified must never refuse install");
    }

    fn provider_manifest(name: &str) -> PluginManifest {
        PluginManifest {
            name: name.to_string(),
            version: "0.1.0".to_string(),
            plugin_kind: animus_plugin_protocol::PLUGIN_KIND_PROVIDER.to_string(),
            description: "test".to_string(),
            protocol_version: "1.0.0".to_string(),
            capabilities: vec!["agent/run".to_string()],
            env_required: Vec::new(),
            notification_buffer_size: None,
        }
    }

    #[test]
    fn install_refuses_reserved_provider_tool_without_flag() {
        let manifest = provider_manifest("animus-provider-claude");
        let err = enforce_provider_tool_policy(&manifest, false).expect_err("must refuse claude provider plugin");
        assert!(format!("{err}").contains("reserved in-tree backend"));
    }

    #[test]
    fn install_allows_reserved_with_explicit_flag() {
        let manifest = provider_manifest("animus-provider-codex");
        let ok = enforce_provider_tool_policy(&manifest, true);
        assert!(ok.is_ok(), "--allow-shadow-builtin must let install through");
    }

    #[test]
    fn install_allows_non_reserved_provider_plugin() {
        let manifest = provider_manifest("animus-provider-mock");
        let ok = enforce_provider_tool_policy(&manifest, false);
        assert!(ok.is_ok(), "non-reserved provider tools must install without the override");
    }

    #[test]
    fn install_skips_provider_check_for_non_provider_plugins() {
        let mut manifest = provider_manifest("animus-provider-claude");
        manifest.plugin_kind = animus_plugin_protocol::PLUGIN_KIND_SUBJECT_BACKEND.to_string();
        let ok = enforce_provider_tool_policy(&manifest, false);
        assert!(ok.is_ok(), "subject backends are never gated by reserved provider tools");
    }

    #[test]
    fn install_rejects_manifest_name_repo_mismatch() {
        let manifest = provider_manifest("animus-provider-claude");
        let err = enforce_manifest_name_matches_repo(&manifest, "evil-org", "animus-provider-oai", false)
            .expect_err("name vs repo basename mismatch must fail");
        let msg = format!("{err}");
        assert!(msg.contains("typosquat") || msg.contains("does not match"), "unexpected message: {msg}");
    }

    #[test]
    fn install_allows_manifest_name_repo_mismatch_with_force() {
        let manifest = provider_manifest("animus-provider-claude");
        let ok = enforce_manifest_name_matches_repo(&manifest, "evil-org", "animus-provider-oai", true);
        assert!(ok.is_ok(), "--force should bypass the manifest-name check");
    }

    #[test]
    fn install_accepts_matching_manifest_name() {
        let manifest = provider_manifest("animus-provider-mock");
        let ok = enforce_manifest_name_matches_repo(&manifest, "launchapp-dev", "animus-provider-mock", false);
        assert!(ok.is_ok(), "exact match must pass");
    }

    #[test]
    fn launchapp_dev_is_builtin_trusted() {
        // Don't read disk in this test — only the built-in list.
        assert!(BUILTIN_TRUSTED_ORGS.contains(&"launchapp-dev"));
    }

    /// v0.4.10: serializes the trusted-orgs tests below that all mutate the
    /// process-global `ANIMUS_TRUSTED_ORGS` env var. Cargo test runs in
    /// parallel by default; concurrent `set_var`/`remove_var` calls were
    /// the root cause of the documented `install_succeeds_after_org_added_to_trusted`
    /// flake. Held alongside [`ScopedEnv`] so the env var is restored on
    /// drop even when an assertion panics.
    static TRUSTED_ORGS_ENV_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// RAII wrapper around a single env var. On drop, restores the prior
    /// value (or removes it if it wasn't set). Prevents tests that panic
    /// mid-flow from leaking state into siblings.
    struct ScopedEnv {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    impl ScopedEnv {
        fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
            let previous = std::env::var_os(key);
            std::env::set_var(key, value);
            Self { key, previous }
        }
    }

    impl Drop for ScopedEnv {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(prev) => std::env::set_var(self.key, prev),
                None => std::env::remove_var(self.key),
            }
        }
    }

    #[test]
    fn install_warns_on_untrusted_org_first_time() {
        let _guard = TRUSTED_ORGS_ENV_GUARD.lock().unwrap_or_else(|p| p.into_inner());
        // Direct unit test of the trusted-org policy.
        let temp = tempfile::tempdir().unwrap();
        let trusted_orgs_yaml = temp.path().join("trusted-orgs.yaml");
        let _env = ScopedEnv::set("ANIMUS_TRUSTED_ORGS", &trusted_orgs_yaml);
        // Untrusted, non-interactive, no --yes -> must error.
        let req = PluginInstallRequest::default();
        let err = enforce_org_trust("evil-org", &req).expect_err("untrusted org without --yes must fail");
        assert!(format!("{err}").contains("untrusted org"), "unexpected: {err}");
    }

    #[test]
    fn install_succeeds_after_org_added_to_trusted() {
        let _guard = TRUSTED_ORGS_ENV_GUARD.lock().unwrap_or_else(|p| p.into_inner());
        let temp = tempfile::tempdir().unwrap();
        let trusted_orgs_yaml = temp.path().join("trusted-orgs.yaml");
        let _env = ScopedEnv::set("ANIMUS_TRUSTED_ORGS", &trusted_orgs_yaml);
        // Pre-populate with someone-elses-org.
        std::fs::write(&trusted_orgs_yaml, "trusted_orgs:\n  - someone-elses-org\n").unwrap();
        let req = PluginInstallRequest::default();
        let ok = enforce_org_trust("someone-elses-org", &req);
        assert!(ok.is_ok(), "previously-trusted org must skip the TOFU prompt");
    }

    #[test]
    fn install_succeeds_when_org_passed_via_allow_org() {
        let _guard = TRUSTED_ORGS_ENV_GUARD.lock().unwrap_or_else(|p| p.into_inner());
        let temp = tempfile::tempdir().unwrap();
        let trusted_orgs_yaml = temp.path().join("trusted-orgs.yaml");
        let _env = ScopedEnv::set("ANIMUS_TRUSTED_ORGS", &trusted_orgs_yaml);
        let req = PluginInstallRequest { allow_org: vec!["new-friend-org".to_string()], ..Default::default() };
        let ok = enforce_org_trust("new-friend-org", &req);
        assert!(ok.is_ok(), "--allow-org should pre-trust the org for this install");
    }

    #[test]
    fn launchapp_dev_skips_tofu_prompt() {
        let _guard = TRUSTED_ORGS_ENV_GUARD.lock().unwrap_or_else(|p| p.into_inner());
        let temp = tempfile::tempdir().unwrap();
        let trusted_orgs_yaml = temp.path().join("trusted-orgs.yaml");
        let _env = ScopedEnv::set("ANIMUS_TRUSTED_ORGS", &trusted_orgs_yaml);
        let req = PluginInstallRequest::default();
        let ok = enforce_org_trust("launchapp-dev", &req);
        assert!(ok.is_ok(), "launchapp-dev is pre-trusted and must never trip TOFU");
    }

    #[test]
    fn add_trusted_org_persists_and_is_idempotent() {
        let _guard = TRUSTED_ORGS_ENV_GUARD.lock().unwrap_or_else(|p| p.into_inner());
        let temp = tempfile::tempdir().unwrap();
        let trusted_orgs_yaml = temp.path().join("trusted-orgs.yaml");
        let _env = ScopedEnv::set("ANIMUS_TRUSTED_ORGS", &trusted_orgs_yaml);
        add_trusted_org("first-org").expect("add 1st");
        add_trusted_org("first-org").expect("idempotent 2nd");
        add_trusted_org("second-org").expect("add 2nd");
        let cfg = load_trusted_orgs().expect("load");
        assert_eq!(cfg.trusted_orgs.len(), 2);
        assert!(cfg.trusted_orgs.contains(&"first-org".to_string()));
        assert!(cfg.trusted_orgs.contains(&"second-org".to_string()));
        // Pre-trusted built-ins never get written.
        add_trusted_org("launchapp-dev").expect("builtin add is no-op");
        let cfg2 = load_trusted_orgs().expect("reload");
        assert_eq!(cfg2.trusted_orgs.len(), 2, "launchapp-dev must not be appended to trusted-orgs.yaml");
    }

    #[test]
    fn trusted_signers_yaml_matches_launchapp_dev() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("trusted.yaml");
        std::fs::write(
            &p,
            r#"
trusted_signers:
  - identity: "launchapp-dev/animus-*"
    issuer: "https://token.actions.githubusercontent.com"
"#,
        )
        .unwrap();
        let cfg = signing::load_trusted_signers(&p).unwrap().expect("config should load");
        assert!(cfg.matches_repo("launchapp-dev/animus-provider-claude"));
        assert!(!cfg.matches_repo("evil-org/animus-provider-claude"));
    }

    // ---- Gap #11: --skip-manifest-check audit trail ----------------------
    //
    // These tests drive the full `run_plugin_install` pipeline with a local
    // `--path` source, then read back the canonical plugins.yaml registry to
    // verify the `skip_manifest_check_at_install` field is persisted when the
    // flag is set, and absent otherwise.

    /// Mutex to serialize install-pipeline tests that mutate process-global
    /// env vars (ANIMUS_CONFIG_DIR, ANIMUS_PLUGIN_DIR, ANIMUS_TRUSTED_ORGS).
    /// Cargo runs tests on multiple threads; sharing these env vars across
    /// concurrent tests would otherwise race.
    static INSTALL_ENV_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[cfg(unix)]
    fn write_fake_plugin_binary(path: &std::path::Path, manifest_name: &str, plugin_kind: &str) {
        use std::os::unix::fs::PermissionsExt;
        let manifest = serde_json::json!({
            "name": manifest_name,
            "version": "0.1.0",
            "plugin_kind": plugin_kind,
            "description": "fake plugin for install-pipeline tests",
            "protocol_version": "1.0.0",
            "capabilities": [],
        });
        // The probe runs the binary with `--manifest`. A POSIX shell script that
        // prints the manifest JSON when `--manifest` is the first arg is enough
        // to satisfy the probe.
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = \"--manifest\" ]; then\n  printf '%s\\n' '{manifest}'\nfi\n",
            manifest = manifest
        );
        std::fs::write(path, script).expect("write fake plugin binary");
        let mut perms = std::fs::metadata(path).expect("metadata").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).expect("chmod");
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    #[allow(clippy::await_holding_lock)] // intentional: guards process-global env mutation across the install await
    async fn install_with_skip_manifest_check_persists_flag() {
        let _guard = INSTALL_ENV_GUARD.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempfile::tempdir().expect("tempdir");
        let config_dir = tmp.path().join("config");
        let install_dir = tmp.path().join("install");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::create_dir_all(&install_dir).unwrap();
        // `--path` install does not exercise the TOFU org-trust pipeline
        // (that only runs for `--source` release installs), so we deliberately
        // leave ANIMUS_TRUSTED_ORGS alone to avoid racing the existing
        // `install_succeeds_after_org_added_to_trusted` test which serialises
        // its own use of that variable.
        std::env::set_var("ANIMUS_CONFIG_DIR", &config_dir);
        std::env::set_var("ANIMUS_PLUGIN_DIR", &install_dir);

        let source = tmp.path().join("animus-provider-skipped");
        write_fake_plugin_binary(&source, "animus-provider-skipped", "subject_backend");

        let req = PluginInstallRequest {
            path: Some(source.to_string_lossy().to_string()),
            skip_manifest_check: true,
            skip_signature: true,
            yes: true,
            ..Default::default()
        };

        let result = run_plugin_install(req).await;

        std::env::remove_var("ANIMUS_CONFIG_DIR");
        std::env::remove_var("ANIMUS_PLUGIN_DIR");

        let output = result.expect("install must succeed with --skip-manifest-check");
        let yaml_path = std::path::PathBuf::from(&output.plugins_yaml);
        let yaml = std::fs::read_to_string(&yaml_path).expect("read plugins.yaml");
        let parsed: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("parse plugins.yaml");
        // No manifest was probed, so the plugin lands in the generic `plugins`
        // table (not `providers`) under its file-name-derived key.
        let entry =
            parsed.get("plugins").and_then(|p| p.get(&output.name)).expect("registry entry for installed plugin");
        let flag = entry
            .get("skip_manifest_check_at_install")
            .and_then(|v| v.as_bool())
            .expect("skip_manifest_check_at_install field must be persisted when flag is set");
        assert!(flag, "skip_manifest_check_at_install must be `true` when the install flag is set");
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    #[allow(clippy::await_holding_lock)] // intentional: guards process-global env mutation across the install await
    async fn install_without_skip_manifest_check_omits_flag() {
        let _guard = INSTALL_ENV_GUARD.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempfile::tempdir().expect("tempdir");
        let config_dir = tmp.path().join("config");
        let install_dir = tmp.path().join("install");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::create_dir_all(&install_dir).unwrap();
        // `--path` install does not exercise the TOFU org-trust pipeline
        // (that only runs for `--source` release installs), so we deliberately
        // leave ANIMUS_TRUSTED_ORGS alone to avoid racing the existing
        // `install_succeeds_after_org_added_to_trusted` test which serialises
        // its own use of that variable.
        std::env::set_var("ANIMUS_CONFIG_DIR", &config_dir);
        std::env::set_var("ANIMUS_PLUGIN_DIR", &install_dir);

        // Note: when the manifest probe runs, the install pipeline insists the
        // manifest name match the install file basename for the `--path`
        // shape. Using the same basename here keeps the test focused on the
        // audit-flag persistence behavior rather than the unrelated name check.
        let source = tmp.path().join("animus-plugin-honest");
        write_fake_plugin_binary(&source, "honest", "subject_backend");

        let req = PluginInstallRequest {
            path: Some(source.to_string_lossy().to_string()),
            skip_manifest_check: false,
            skip_signature: true,
            yes: true,
            ..Default::default()
        };

        let result = run_plugin_install(req).await;

        std::env::remove_var("ANIMUS_CONFIG_DIR");
        std::env::remove_var("ANIMUS_PLUGIN_DIR");

        let output = result.expect("install must succeed without --skip-manifest-check");
        let yaml_path = std::path::PathBuf::from(&output.plugins_yaml);
        let yaml = std::fs::read_to_string(&yaml_path).expect("read plugins.yaml");
        let parsed: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("parse plugins.yaml");
        // The manifest was probed and accepted, so the entry lands under
        // `plugins` (the manifest's plugin_kind is `subject_backend`, not
        // `provider`).
        let entry =
            parsed.get("plugins").and_then(|p| p.get(&output.name)).expect("registry entry for installed plugin");
        let flag = entry.get("skip_manifest_check_at_install").and_then(|v| v.as_bool());
        assert!(
            flag.is_none() || flag == Some(false),
            "skip_manifest_check_at_install must be absent (or `false`) when the flag is not set; got {flag:?}"
        );
    }
}
