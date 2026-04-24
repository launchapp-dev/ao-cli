use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};

use super::types::{
    LoadedProjectTemplate, ProjectTemplateFile, ProjectTemplateManifest, ProjectTemplateSourceKind,
    ProjectTemplateSummary, PROJECT_TEMPLATE_MANIFEST_FILE_NAME, PROJECT_TEMPLATE_MANIFEST_SCHEMA_ID,
};

pub const DEFAULT_PROJECT_TEMPLATE_REGISTRY_ID: &str = "launchapp";
pub const DEFAULT_PROJECT_TEMPLATE_REGISTRY_URL: &str = "https://github.com/launchapp-dev/animus-project-templates.git";
pub const PROJECT_TEMPLATE_REGISTRY_URL_ENV: &str = "ANIMUS_TEMPLATE_REGISTRY_URL";

const PROJECT_TEMPLATE_REGISTRY_CACHE_DIR: &str = "template-registries";
const PROJECT_TEMPLATE_REGISTRY_TEMPLATES_DIR: &str = "templates";

pub fn parse_project_template_manifest(raw_toml: &str) -> Result<ProjectTemplateManifest> {
    let manifest: ProjectTemplateManifest =
        toml::from_str(raw_toml).context("failed to parse project template manifest TOML")?;
    validate_project_template_manifest(&manifest)?;
    Ok(manifest)
}

pub fn load_project_template_from_dir(template_root: &Path) -> Result<LoadedProjectTemplate> {
    load_project_template_from_dir_with_kind(template_root, ProjectTemplateSourceKind::Local)
}

pub fn load_project_template_from_file(manifest_path: &Path) -> Result<LoadedProjectTemplate> {
    load_project_template_from_file_with_kind(manifest_path, ProjectTemplateSourceKind::Local)
}

pub fn default_project_template_registry_url() -> String {
    env::var(PROJECT_TEMPLATE_REGISTRY_URL_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_PROJECT_TEMPLATE_REGISTRY_URL.to_string())
}

pub fn sync_default_project_template_registry() -> Result<PathBuf> {
    sync_project_template_registry(DEFAULT_PROJECT_TEMPLATE_REGISTRY_ID, &default_project_template_registry_url())
}

pub fn list_project_templates_from_default_registry() -> Result<Vec<ProjectTemplateSummary>> {
    let registry_root = sync_default_project_template_registry()?;
    list_project_templates_from_registry_root(&registry_root)
}

pub fn load_project_template_from_default_registry(template_id: &str) -> Result<LoadedProjectTemplate> {
    let registry_root = sync_default_project_template_registry()?;
    load_project_template_from_registry_root(&registry_root, template_id).with_context(|| {
        format!(
            "failed to load template '{template_id}' from default registry '{}'",
            default_project_template_registry_url()
        )
    })
}

pub fn list_project_templates_from_registry_root(registry_root: &Path) -> Result<Vec<ProjectTemplateSummary>> {
    let mut templates = collect_registry_template_dirs(registry_root)?
        .into_iter()
        .map(|template_root| {
            load_project_template_summary_from_dir(&template_root, ProjectTemplateSourceKind::Registry)
        })
        .collect::<Result<Vec<_>>>()?;
    templates.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(templates)
}

pub fn load_project_template_from_registry_root(
    registry_root: &Path,
    template_id: &str,
) -> Result<LoadedProjectTemplate> {
    for template_root in collect_registry_template_dirs(registry_root)? {
        let template = load_project_template_from_dir_with_kind(&template_root, ProjectTemplateSourceKind::Registry)?;
        if template.manifest.id.eq_ignore_ascii_case(template_id) {
            return Ok(template);
        }
    }

    Err(anyhow!("project template '{}' not found in registry at {}", template_id, registry_root.display()))
}

fn load_project_template_summary_from_dir(
    template_root: &Path,
    source_kind: ProjectTemplateSourceKind,
) -> Result<ProjectTemplateSummary> {
    let manifest_path = template_root.join(PROJECT_TEMPLATE_MANIFEST_FILE_NAME);
    let raw = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read project template manifest at {}", manifest_path.display()))?;
    let manifest = parse_project_template_manifest(&raw)?;
    Ok(ProjectTemplateSummary {
        id: manifest.id,
        version: manifest.version,
        title: manifest.title,
        description: manifest.description,
        pattern: manifest.pattern,
        source_kind,
    })
}

fn load_project_template_from_dir_with_kind(
    template_root: &Path,
    source_kind: ProjectTemplateSourceKind,
) -> Result<LoadedProjectTemplate> {
    let manifest_path = template_root.join(PROJECT_TEMPLATE_MANIFEST_FILE_NAME);
    load_project_template_from_file_with_kind(&manifest_path, source_kind)
}

fn load_project_template_from_file_with_kind(
    manifest_path: &Path,
    source_kind: ProjectTemplateSourceKind,
) -> Result<LoadedProjectTemplate> {
    let raw = fs::read_to_string(manifest_path)
        .with_context(|| format!("failed to read project template manifest at {}", manifest_path.display()))?;
    let manifest = parse_project_template_manifest(&raw)?;
    let template_root = manifest_path
        .parent()
        .ok_or_else(|| anyhow!("template manifest path '{}' has no parent directory", manifest_path.display()))?;
    let source_root = template_root.join(&manifest.source.root);
    if !source_root.is_dir() {
        return Err(anyhow!("project template source root '{}' is not a directory", source_root.display()));
    }
    let mut files = Vec::new();
    collect_template_files(&source_root, Path::new(""), &mut files)?;
    Ok(LoadedProjectTemplate { source_kind, template_root: Some(template_root.to_path_buf()), manifest, files })
}

fn collect_template_files(root: &Path, relative: &Path, files: &mut Vec<ProjectTemplateFile>) -> Result<()> {
    let dir = root.join(relative);
    let mut entries = fs::read_dir(&dir)
        .with_context(|| format!("failed to read template source directory {}", dir.display()))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("failed to enumerate template source directory {}", dir.display()))?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let file_name = entry.file_name();
        let relative_path = relative.join(&file_name);
        let path = entry.path();
        if path.is_dir() {
            collect_template_files(root, &relative_path, files)?;
            continue;
        }

        let contents = fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
        files.push(ProjectTemplateFile { relative_path, contents });
    }

    Ok(())
}

fn sync_project_template_registry(registry_id: &str, url: &str) -> Result<PathBuf> {
    let cache_dir = project_template_registry_cache_dir();
    fs::create_dir_all(&cache_dir).with_context(|| format!("failed to create {}", cache_dir.display()))?;
    let target = cache_dir.join(registry_id);

    if target.exists() {
        if !target.is_dir() {
            return Err(anyhow!("project template registry cache target '{}' is not a directory", target.display()));
        }
        let _ = git_pull_ff_only(&target);
        return Ok(target);
    }

    git_clone(url, &target)?;
    Ok(target)
}

fn project_template_registry_cache_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".ao").join(PROJECT_TEMPLATE_REGISTRY_CACHE_DIR)
}

fn git_pull_ff_only(target: &Path) -> bool {
    Command::new("git")
        .args(["pull", "--ff-only"])
        .current_dir(target)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn git_clone(url: &str, target: &Path) -> Result<()> {
    let status = Command::new("git")
        .args(["clone", "--depth", "1", url, &target.display().to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .with_context(|| format!("failed to run git clone for {}", url))?;
    if !status.success() {
        return Err(anyhow!("git clone failed for {}", url));
    }
    Ok(())
}

fn collect_registry_template_dirs(registry_root: &Path) -> Result<Vec<PathBuf>> {
    let mut template_dirs = BTreeSet::new();
    for root in [registry_root.join(PROJECT_TEMPLATE_REGISTRY_TEMPLATES_DIR), registry_root.to_path_buf()] {
        if !root.is_dir() {
            continue;
        }

        let mut entries = fs::read_dir(&root)
            .with_context(|| format!("failed to read template registry directory {}", root.display()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .with_context(|| format!("failed to enumerate template registry directory {}", root.display()))?;
        entries.sort_by_key(|entry| entry.file_name());

        for entry in entries {
            let file_name = entry.file_name();
            if file_name.to_string_lossy().starts_with('.') {
                continue;
            }
            let path = entry.path();
            if path.is_dir() && path.join(PROJECT_TEMPLATE_MANIFEST_FILE_NAME).is_file() {
                template_dirs.insert(path);
            }
        }
    }
    Ok(template_dirs.into_iter().collect())
}

fn validate_project_template_manifest(manifest: &ProjectTemplateManifest) -> Result<()> {
    if manifest.schema.trim() != PROJECT_TEMPLATE_MANIFEST_SCHEMA_ID {
        return Err(anyhow!(
            "project template schema must be '{}' (got '{}')",
            PROJECT_TEMPLATE_MANIFEST_SCHEMA_ID,
            manifest.schema
        ));
    }
    if manifest.id.trim().is_empty() {
        return Err(anyhow!("project template id must not be empty"));
    }
    if manifest.version.trim().is_empty() {
        return Err(anyhow!("project template version must not be empty"));
    }
    if manifest.title.trim().is_empty() {
        return Err(anyhow!("project template title must not be empty"));
    }
    if manifest.pattern.trim().is_empty() {
        return Err(anyhow!("project template pattern must not be empty"));
    }
    if manifest.source.root.trim().is_empty() {
        return Err(anyhow!("project template source.root must not be empty"));
    }
    if Path::new(&manifest.source.root).is_absolute() {
        return Err(anyhow!("project template source.root must be relative"));
    }
    for pack in &manifest.packs {
        if pack.id.trim().is_empty() {
            return Err(anyhow!("project template pack id must not be empty"));
        }
        if let Some(version) = pack.version.as_deref() {
            if version.trim().is_empty() {
                return Err(anyhow!("project template pack version must not be empty when provided"));
            }
        }
    }
    for step in &manifest.next_steps {
        if step.trim().is_empty() {
            return Err(anyhow!("project template next_steps entries must not be empty"));
        }
    }
    Ok(())
}
