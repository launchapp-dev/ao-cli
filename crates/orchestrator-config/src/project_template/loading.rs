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
const PROJECT_TEMPLATE_REGISTRY_COMMIT_FILE: &str = ".commit";
const PROJECT_TEMPLATE_REGISTRY_DEFAULT_BRANCH: &str = "main";

#[derive(Debug, Clone, Copy, Default)]
pub struct RegistrySyncOptions {
    pub update: bool,
}

impl RegistrySyncOptions {
    pub fn pinned() -> Self {
        Self { update: false }
    }

    pub fn update() -> Self {
        Self { update: true }
    }
}

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
    sync_default_project_template_registry_with_options(RegistrySyncOptions::default())
}

pub fn sync_default_project_template_registry_with_options(options: RegistrySyncOptions) -> Result<PathBuf> {
    sync_project_template_registry(
        DEFAULT_PROJECT_TEMPLATE_REGISTRY_ID,
        &default_project_template_registry_url(),
        options,
    )
}

pub fn list_project_templates_from_default_registry() -> Result<Vec<ProjectTemplateSummary>> {
    list_project_templates_from_default_registry_with_options(RegistrySyncOptions::default())
}

pub fn list_project_templates_from_default_registry_with_options(
    options: RegistrySyncOptions,
) -> Result<Vec<ProjectTemplateSummary>> {
    let registry_root = sync_default_project_template_registry_with_options(options)?;
    list_project_templates_from_registry_root(&registry_root)
}

pub fn load_project_template_from_default_registry(template_id: &str) -> Result<LoadedProjectTemplate> {
    load_project_template_from_default_registry_with_options(template_id, RegistrySyncOptions::default())
}

pub fn load_project_template_from_default_registry_with_options(
    template_id: &str,
    options: RegistrySyncOptions,
) -> Result<LoadedProjectTemplate> {
    let registry_root = sync_default_project_template_registry_with_options(options)?;
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

fn sync_project_template_registry(registry_id: &str, url: &str, options: RegistrySyncOptions) -> Result<PathBuf> {
    let cache_dir = project_template_registry_cache_dir();
    fs::create_dir_all(&cache_dir).with_context(|| format!("failed to create {}", cache_dir.display()))?;
    let target = cache_dir.join(registry_id);
    let commit_file = target.join(PROJECT_TEMPLATE_REGISTRY_COMMIT_FILE);

    if target.exists() {
        if !target.is_dir() {
            return Err(anyhow!("project template registry cache target '{}' is not a directory", target.display()));
        }

        if options.update {
            git_fetch_and_reset(&target, PROJECT_TEMPLATE_REGISTRY_DEFAULT_BRANCH)?;
            let head = git_rev_parse_head(&target)?;
            write_pinned_commit(&commit_file, &head)?;
            return Ok(target);
        }

        let pinned = read_pinned_commit(&commit_file).with_context(|| {
            format!(
                "failed to read pinned commit metadata at {}; re-pin the registry with --update-registry",
                commit_file.display()
            )
        })?;
        let head = git_rev_parse_head(&target)?;
        if head != pinned {
            return Err(anyhow!(
                "project template registry at '{}' has diverged from the pinned commit\n  pinned:   {}\n  current:  {}\nThe registry checkout no longer matches the commit captured on first clone. \
                 This can happen if the cache was modified out-of-band or the upstream history was rewritten. \
                 Re-run with --update-registry to fetch the latest commit and re-pin, or restore the cache to {}",
                target.display(),
                pinned,
                head,
                pinned
            ));
        }
        return Ok(target);
    }

    git_clone(url, &target)?;
    let head = git_rev_parse_head(&target)
        .with_context(|| format!("failed to capture HEAD commit for newly cloned registry at {}", target.display()))?;
    write_pinned_commit(&commit_file, &head)?;
    Ok(target)
}

fn project_template_registry_cache_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".animus").join(PROJECT_TEMPLATE_REGISTRY_CACHE_DIR)
}

fn git_clone(url: &str, target: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["clone", url, &target.display().to_string()])
        .output()
        .with_context(|| format!("failed to run git clone for {}", url))?;
    if !output.status.success() {
        return Err(anyhow!(
            "git clone failed: {}\n  stdout: {}\n  stderr: {}",
            describe_exit_status(&output.status),
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

fn git_fetch_and_reset(target: &Path, branch: &str) -> Result<()> {
    let fetch = Command::new("git")
        .args(["fetch", "origin", branch])
        .current_dir(target)
        .output()
        .with_context(|| format!("failed to run git fetch in {}", target.display()))?;
    if !fetch.status.success() {
        return Err(anyhow!(
            "git fetch failed: {}\n  stdout: {}\n  stderr: {}",
            describe_exit_status(&fetch.status),
            String::from_utf8_lossy(&fetch.stdout).trim(),
            String::from_utf8_lossy(&fetch.stderr).trim()
        ));
    }

    let reset = Command::new("git")
        .args(["reset", "--hard", &format!("origin/{branch}")])
        .current_dir(target)
        .output()
        .with_context(|| format!("failed to run git reset in {}", target.display()))?;
    if !reset.status.success() {
        return Err(anyhow!(
            "git reset failed: {}\n  stdout: {}\n  stderr: {}",
            describe_exit_status(&reset.status),
            String::from_utf8_lossy(&reset.stdout).trim(),
            String::from_utf8_lossy(&reset.stderr).trim()
        ));
    }
    Ok(())
}

fn git_rev_parse_head(target: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(target)
        .output()
        .with_context(|| format!("failed to run git rev-parse in {}", target.display()))?;
    if !output.status.success() {
        return Err(anyhow!(
            "git rev-parse HEAD failed: {}\n  stdout: {}\n  stderr: {}",
            describe_exit_status(&output.status),
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if sha.is_empty() {
        return Err(anyhow!("git rev-parse HEAD returned empty output for {}", target.display()));
    }
    Ok(sha)
}

fn read_pinned_commit(commit_file: &Path) -> Result<String> {
    let raw = fs::read_to_string(commit_file)
        .with_context(|| format!("failed to read pinned commit file {}", commit_file.display()))?;
    let trimmed = raw.trim().to_string();
    if trimmed.is_empty() {
        return Err(anyhow!("pinned commit file {} is empty", commit_file.display()));
    }
    Ok(trimmed)
}

fn write_pinned_commit(commit_file: &Path, sha: &str) -> Result<()> {
    if let Some(parent) = commit_file.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create pinned commit parent directory {}", parent.display()))?;
    }
    fs::write(commit_file, format!("{}\n", sha))
        .with_context(|| format!("failed to write pinned commit file {}", commit_file.display()))?;
    Ok(())
}

fn describe_exit_status(status: &std::process::ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("exit code {code}"),
        None => "terminated by signal".to_string(),
    }
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
