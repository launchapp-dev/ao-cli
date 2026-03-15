use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use semver::Version;

use crate::agent_runtime_config::{AgentRuntimeOverlay, CliToolConfig, PhaseExecutionDefinition};
use crate::pack_config::{load_pack_manifest, LoadedPackManifest};
use crate::workflow_config::{compile_yaml_sources_with_base, WorkflowConfig};

pub const PROJECT_PACKS_DIR_NAME: &str = "plugins";
pub const MACHINE_PACKS_DIR_NAME: &str = "packs";
pub const BUNDLED_BUILTIN_PACK_ID: &str = "builtin";
pub const BUNDLED_BUILTIN_PACK_VERSION: &str = "builtin";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackRegistrySource {
    Bundled,
    Installed,
    ProjectOverride,
}

impl PackRegistrySource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bundled => "bundled",
            Self::Installed => "installed",
            Self::ProjectOverride => "project_override",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedPackRegistryEntry {
    pub pack_id: String,
    pub version: String,
    pub source: PackRegistrySource,
    pub pack_root: Option<PathBuf>,
    pub manifest_path: Option<PathBuf>,
    loaded_manifest: Option<LoadedPackManifest>,
}

impl ResolvedPackRegistryEntry {
    fn bundled_builtin() -> Self {
        Self {
            pack_id: BUNDLED_BUILTIN_PACK_ID.to_string(),
            version: BUNDLED_BUILTIN_PACK_VERSION.to_string(),
            source: PackRegistrySource::Bundled,
            pack_root: None,
            manifest_path: None,
            loaded_manifest: None,
        }
    }

    fn from_manifest(source: PackRegistrySource, loaded_manifest: LoadedPackManifest) -> Self {
        Self {
            pack_id: loaded_manifest.manifest.id.clone(),
            version: loaded_manifest.manifest.version.clone(),
            source,
            pack_root: Some(loaded_manifest.pack_root.clone()),
            manifest_path: Some(loaded_manifest.manifest_path.clone()),
            loaded_manifest: Some(loaded_manifest),
        }
    }

    pub fn loaded_manifest(&self) -> Option<&LoadedPackManifest> {
        self.loaded_manifest.as_ref()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResolvedPackRegistry {
    pub entries: Vec<ResolvedPackRegistryEntry>,
}

impl ResolvedPackRegistry {
    pub fn has_external_packs(&self) -> bool {
        self.entries.iter().any(|entry| entry.source != PackRegistrySource::Bundled)
    }

    pub fn resolve(&self, pack_id: &str) -> Option<&ResolvedPackRegistryEntry> {
        self.entries.iter().find(|entry| entry.pack_id.eq_ignore_ascii_case(pack_id))
    }

    pub fn entries_for_source(
        &self,
        source: PackRegistrySource,
    ) -> impl Iterator<Item = &ResolvedPackRegistryEntry> + '_ {
        self.entries.iter().filter(move |entry| entry.source == source)
    }
}

pub fn project_pack_overrides_dir(project_root: &Path) -> PathBuf {
    project_root.join(".ao").join(PROJECT_PACKS_DIR_NAME)
}

pub fn machine_installed_packs_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".ao").join(MACHINE_PACKS_DIR_NAME)
}

pub fn resolve_pack_registry(project_root: &Path) -> Result<ResolvedPackRegistry> {
    let mut entries = vec![ResolvedPackRegistryEntry::bundled_builtin()];
    let installed = discover_installed_packs()?;
    let project_overrides = discover_project_override_packs(project_root)?;

    let mut resolved = BTreeMap::new();
    for pack in installed {
        resolved.insert(
            pack.manifest.id.to_ascii_lowercase(),
            ResolvedPackRegistryEntry::from_manifest(PackRegistrySource::Installed, pack),
        );
    }
    for pack in project_overrides {
        let key = pack.manifest.id.to_ascii_lowercase();
        let entry = ResolvedPackRegistryEntry::from_manifest(PackRegistrySource::ProjectOverride, pack);
        if resolved
            .insert(key.clone(), entry)
            .is_some_and(|previous| previous.source == PackRegistrySource::ProjectOverride)
        {
            return Err(anyhow!("duplicate project override pack id '{}'", key));
        }
    }

    entries.extend(resolved.into_values());
    Ok(ResolvedPackRegistry { entries })
}

pub fn load_pack_workflow_overlay(pack: &LoadedPackManifest, base: &WorkflowConfig) -> Result<Option<WorkflowConfig>> {
    let yaml_sources = collect_pack_workflow_yaml_sources(pack)?;
    if yaml_sources.is_empty() {
        return Ok(None);
    }

    let mut overlay = compile_yaml_sources_with_base(base, &yaml_sources)?;
    if let Some(overlay) = overlay.as_mut() {
        resolve_pack_workflow_assets(pack, overlay)?;
    }
    Ok(overlay)
}

pub fn load_pack_agent_runtime_overlay(pack: &LoadedPackManifest) -> Result<Option<AgentRuntimeOverlay>> {
    let Some(overlay_path) = pack.manifest.runtime.agent_overlay.as_deref() else {
        return Ok(None);
    };

    let path = pack.pack_root.join(overlay_path);
    let raw = fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut overlay: AgentRuntimeOverlay =
        serde_yaml::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))?;
    resolve_pack_agent_runtime_assets(pack, &mut overlay)?;
    Ok(Some(overlay))
}

fn discover_project_override_packs(project_root: &Path) -> Result<Vec<LoadedPackManifest>> {
    let overrides_dir = project_pack_overrides_dir(project_root);
    if !overrides_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut pack_roots = read_child_directories(&overrides_dir)?;
    pack_roots.sort();

    let mut loaded = Vec::new();
    let mut seen_ids = BTreeMap::<String, PathBuf>::new();
    for pack_root in pack_roots {
        if !pack_root.join(crate::PACK_MANIFEST_FILE_NAME).exists() {
            continue;
        }

        let pack = load_pack_manifest(&pack_root)
            .with_context(|| format!("failed to load project override pack at {}", pack_root.display()))?;
        let id_key = pack.manifest.id.to_ascii_lowercase();
        if let Some(previous) = seen_ids.insert(id_key.clone(), pack_root.clone()) {
            return Err(anyhow!(
                "duplicate project override pack '{}' in '{}' and '{}'",
                pack.manifest.id,
                previous.display(),
                pack_root.display()
            ));
        }
        loaded.push(pack);
    }

    loaded.sort_by(|left, right| left.manifest.id.cmp(&right.manifest.id));
    Ok(loaded)
}

fn discover_installed_packs() -> Result<Vec<LoadedPackManifest>> {
    let installed_root = machine_installed_packs_dir();
    if !installed_root.is_dir() {
        return Ok(Vec::new());
    }

    let mut selected = BTreeMap::<String, (Version, LoadedPackManifest)>::new();
    let mut pack_roots = read_child_directories(&installed_root)?;
    pack_roots.sort();

    for pack_dir in pack_roots {
        let mut version_dirs = read_child_directories(&pack_dir)?;
        version_dirs.sort();
        for version_dir in version_dirs {
            if !version_dir.join(crate::PACK_MANIFEST_FILE_NAME).exists() {
                continue;
            }

            let pack = load_pack_manifest(&version_dir)
                .with_context(|| format!("failed to load installed pack at {}", version_dir.display()))?;
            let version = Version::parse(&pack.manifest.version)
                .with_context(|| format!("invalid installed pack version '{}'", pack.manifest.version))?;
            let key = pack.manifest.id.to_ascii_lowercase();

            let replace = selected
                .get(&key)
                .map(|(current_version, current_pack)| {
                    version > *current_version
                        || (version == *current_version && pack.pack_root < current_pack.pack_root)
                })
                .unwrap_or(true);
            if replace {
                selected.insert(key, (version, pack));
            }
        }
    }

    Ok(selected.into_values().map(|(_, pack)| pack).collect())
}

fn collect_pack_workflow_yaml_sources(pack: &LoadedPackManifest) -> Result<Vec<(PathBuf, String)>> {
    let mut sources = Vec::new();
    let workflows_root = pack.pack_root.join(&pack.manifest.workflows.root);
    if workflows_root.is_dir() {
        let mut entries = fs::read_dir(&workflows_root)
            .with_context(|| format!("failed to read pack workflows directory {}", workflows_root.display()))?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().map(|ext| ext == "yaml" || ext == "yml").unwrap_or(false))
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.path());

        for entry in entries {
            let path = entry.path();
            let content = fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
            sources.push((path, content));
        }
    }

    if let Some(overlay_path) = pack.manifest.runtime.workflow_overlay.as_deref() {
        let path = pack.pack_root.join(overlay_path);
        let content = fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
        sources.push((path, content));
    }

    Ok(sources)
}

fn resolve_pack_workflow_assets(pack: &LoadedPackManifest, workflow: &mut WorkflowConfig) -> Result<()> {
    for (phase_id, definition) in &mut workflow.phase_definitions {
        resolve_phase_command_assets(pack, phase_id, definition)?;
    }
    for (tool_id, definition) in &mut workflow.tools {
        definition.executable =
            resolve_pack_asset_path(pack, &definition.executable, &format!("tools['{}'].executable", tool_id))?;
    }
    Ok(())
}

fn resolve_pack_agent_runtime_assets(pack: &LoadedPackManifest, overlay: &mut AgentRuntimeOverlay) -> Result<()> {
    for (phase_id, definition) in &mut overlay.phases {
        resolve_phase_command_assets(pack, phase_id, definition)?;
    }
    for (tool_id, definition) in &mut overlay.cli_tools {
        resolve_cli_tool_assets(pack, tool_id, definition)?;
    }
    Ok(())
}

fn resolve_phase_command_assets(
    pack: &LoadedPackManifest,
    phase_id: &str,
    definition: &mut PhaseExecutionDefinition,
) -> Result<()> {
    let Some(command) = definition.command.as_mut() else {
        return Ok(());
    };

    command.program =
        resolve_pack_asset_path(pack, &command.program, &format!("phases['{}'].command.program", phase_id))?;
    Ok(())
}

fn resolve_cli_tool_assets(pack: &LoadedPackManifest, tool_id: &str, definition: &mut CliToolConfig) -> Result<()> {
    let Some(executable) = definition.executable.as_deref() else {
        return Ok(());
    };

    definition.executable =
        Some(resolve_pack_asset_path(pack, executable, &format!("cli_tools['{}'].executable", tool_id))?);
    Ok(())
}

fn resolve_pack_asset_path(pack: &LoadedPackManifest, raw: &str, field: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || !looks_like_pack_relative_asset(trimmed) {
        return Ok(raw.to_string());
    }

    let relative = Path::new(trimmed);
    if relative.components().any(|component| matches!(component, std::path::Component::ParentDir)) {
        return Err(anyhow!("{} path '{}' cannot escape pack root", field, raw));
    }

    let resolved = pack.pack_root.join(relative);
    if !resolved.exists() {
        return Err(anyhow!("{} path '{}' is missing from pack '{}'", field, raw, pack.manifest.id));
    }

    let canonical_pack_root = fs::canonicalize(&pack.pack_root).unwrap_or_else(|_| pack.pack_root.clone());
    let canonical = fs::canonicalize(&resolved).unwrap_or_else(|_| resolved.clone());
    if !canonical.starts_with(&canonical_pack_root) {
        return Err(anyhow!("{} path '{}' escapes pack root", field, raw));
    }

    Ok(canonical.display().to_string())
}

fn looks_like_pack_relative_asset(raw: &str) -> bool {
    let path = Path::new(raw);
    if path.is_absolute() {
        return false;
    }

    raw.starts_with('.') || path.components().count() > 1
}

fn read_child_directories(root: &Path) -> Result<Vec<PathBuf>> {
    Ok(fs::read_dir(root)
        .with_context(|| format!("failed to read directory {}", root.display()))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect())
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::sync::{Mutex, OnceLock};

    use super::*;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct EnvVarGuard {
        key: &'static str,
        original: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &std::path::Path) -> Self {
            let original = env::var(key).ok();
            env::set_var(key, value);
            Self { key, original }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match self.original.as_deref() {
                Some(value) => env::set_var(self.key, value),
                None => env::remove_var(self.key),
            }
        }
    }

    fn write_pack_fixture(root: &Path, pack_id: &str, version: &str, description: &str, extra_workflow: &str) {
        fs::create_dir_all(root.join("workflows")).expect("create workflows");
        fs::create_dir_all(root.join("runtime")).expect("create runtime");

        let manifest = format!(
            r#"
schema = "ao.pack.v1"
id = "{pack_id}"
version = "{version}"
kind = "domain-pack"
title = "Pack {pack_id}"
description = "Fixture"

[ownership]
mode = "bundled"

[compatibility]
ao_core = ">=0.1.0"
workflow_schema = "v2"
subject_schema = "v2"

[subjects]
kinds = ["ao.task"]
default_kind = "ao.task"

[workflows]
root = "workflows"
exports = ["{pack_id}/standard"]

[runtime]
workflow_overlay = "runtime/workflow-runtime.overlay.yaml"
"#
        );
        fs::write(root.join(crate::PACK_MANIFEST_FILE_NAME), manifest).expect("write manifest");
        fs::write(
            root.join("runtime/workflow-runtime.overlay.yaml"),
            format!(
                r#"
workflows:
  - id: standard
    name: Standard
    description: "{description}"
    phases:
      - requirements
      - implementation
  - id: {extra_workflow}
    name: "{extra_workflow}"
    phases:
      - requirements
      - testing
"#
            ),
        )
        .expect("write workflow overlay");
    }

    fn write_pack_with_command_assets(root: &Path, pack_id: &str, version: &str) {
        fs::create_dir_all(root.join("workflows")).expect("create workflows");
        fs::create_dir_all(root.join("runtime")).expect("create runtime");
        fs::create_dir_all(root.join("assets")).expect("create assets");
        fs::write(root.join("assets/review-helper.sh"), "#!/bin/sh\nexit 0\n").expect("write helper");

        fs::write(
            root.join(crate::PACK_MANIFEST_FILE_NAME),
            format!(
                r#"
schema = "ao.pack.v1"
id = "{pack_id}"
version = "{version}"
kind = "domain-pack"
title = "Pack {pack_id}"
description = "Fixture"

[ownership]
mode = "bundled"

[compatibility]
ao_core = ">=0.1.0"
workflow_schema = "v2"
subject_schema = "v2"

[subjects]
kinds = ["ao.task"]
default_kind = "ao.task"

[workflows]
root = "workflows"
exports = ["{pack_id}/review-pack"]

[runtime]
workflow_overlay = "runtime/workflow-runtime.overlay.yaml"
"#
            ),
        )
        .expect("write manifest");
        fs::write(
            root.join("runtime/workflow-runtime.overlay.yaml"),
            r#"
phases:
  pack-review:
    mode: command
    command:
      program: ./assets/review-helper.sh
      cwd_mode: path
      cwd_path: workspace/scripts
tools:
  pack-tool:
    executable: ./assets/review-helper.sh
    supports_mcp: false
    supports_write: false
workflows:
  - id: review-pack
    name: "review-pack"
    phases:
      - pack-review
"#,
        )
        .expect("write workflow overlay");
    }

    #[test]
    fn resolve_pack_registry_prefers_project_overrides_and_latest_installed_version() {
        let _lock = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let home = tempfile::tempdir().expect("home tempdir");
        let project = tempfile::tempdir().expect("project tempdir");
        let _home_guard = EnvVarGuard::set("HOME", home.path());

        write_pack_fixture(
            &machine_installed_packs_dir().join("ao.review").join("0.1.0"),
            "ao.review",
            "0.1.0",
            "old installed",
            "review-old",
        );
        write_pack_fixture(
            &machine_installed_packs_dir().join("ao.review").join("0.2.0"),
            "ao.review",
            "0.2.0",
            "new installed",
            "review-new",
        );
        write_pack_fixture(
            &machine_installed_packs_dir().join("ao.task").join("1.0.0"),
            "ao.task",
            "1.0.0",
            "installed task",
            "task-installed",
        );
        write_pack_fixture(
            &project_pack_overrides_dir(project.path()).join("ao-task"),
            "ao.task",
            "9.0.0",
            "project override task",
            "task-override",
        );

        let registry = resolve_pack_registry(project.path()).expect("resolve registry");
        assert_eq!(registry.entries[0].source, PackRegistrySource::Bundled);

        let review = registry.resolve("ao.review").expect("ao.review should resolve");
        assert_eq!(review.source, PackRegistrySource::Installed);
        assert_eq!(review.version, "0.2.0");

        let task = registry.resolve("ao.task").expect("ao.task should resolve");
        assert_eq!(task.source, PackRegistrySource::ProjectOverride);
        assert_eq!(task.version, "9.0.0");
    }

    #[test]
    fn workflow_loading_uses_installed_then_ad_hoc_then_project_override_precedence() {
        let _lock = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let home = tempfile::tempdir().expect("home tempdir");
        let project = tempfile::tempdir().expect("project tempdir");
        let _home_guard = EnvVarGuard::set("HOME", home.path());

        write_pack_fixture(
            &machine_installed_packs_dir().join("ao.review").join("0.2.0"),
            "ao.review",
            "0.2.0",
            "installed review",
            "review-installed",
        );
        write_pack_fixture(
            &machine_installed_packs_dir().join("ao.task").join("1.0.0"),
            "ao.task",
            "1.0.0",
            "installed task",
            "task-installed",
        );
        write_pack_fixture(
            &project_pack_overrides_dir(project.path()).join("ao-task"),
            "ao.task",
            "9.0.0",
            "project override task",
            "task-override",
        );

        fs::create_dir_all(project.path().join(".ao")).expect("create project .ao");
        fs::write(
            project.path().join(".ao").join("workflows.yaml"),
            r#"
workflows:
  - id: standard
    name: Standard
    description: "project ad hoc"
    phases:
      - requirements
      - testing
  - id: adhoc-only
    name: "adhoc-only"
    phases:
      - requirements
      - code-review
"#,
        )
        .expect("write project workflows");

        let loaded = crate::load_workflow_config_with_metadata(project.path()).expect("load effective workflow config");
        let workflows = loaded.config.workflows.iter().map(|workflow| workflow.id.as_str()).collect::<Vec<_>>();
        let standard = loaded
            .config
            .workflows
            .iter()
            .find(|workflow| workflow.id == "standard")
            .expect("standard workflow should exist");

        assert_eq!(standard.description, "project override task");
        assert!(workflows.contains(&"review-installed"));
        assert!(workflows.contains(&"task-override"));
        assert!(workflows.contains(&"adhoc-only"));
        assert!(!workflows.contains(&"task-installed"));
    }

    #[test]
    fn load_pack_workflow_overlay_resolves_pack_relative_command_assets() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_pack_with_command_assets(temp.path(), "ao.review", "0.2.0");

        let pack = load_pack_manifest(temp.path()).expect("load pack");
        let overlay = load_pack_workflow_overlay(&pack, &crate::workflow_config::builtin_workflow_config())
            .expect("load overlay");
        let overlay = overlay.expect("overlay should be present");
        let command = overlay
            .phase_definitions
            .get("pack-review")
            .and_then(|definition| definition.command.as_ref())
            .expect("pack-review command");
        let tool = overlay.tools.get("pack-tool").expect("pack tool");

        assert!(command.program.ends_with("assets/review-helper.sh"));
        assert_eq!(command.cwd_path.as_deref(), Some("workspace/scripts"));
        assert!(tool.executable.ends_with("assets/review-helper.sh"));
    }
}
