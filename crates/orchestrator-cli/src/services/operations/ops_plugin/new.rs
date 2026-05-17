use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use chrono::Datelike;
use serde::Serialize;

use crate::{invalid_input_error, print_value, PluginNewArgs};

#[cfg(test)]
#[path = "new_tests.rs"]
mod new_tests;

const KNOWN_KINDS: &[&str] = &["subject", "provider", "trigger"];

#[derive(Debug, Serialize)]
pub(super) struct PluginNewOutput {
    pub(super) name: String,
    pub(super) kind: String,
    pub(super) full_name: String,
    pub(super) out_dir: String,
    pub(super) template_repo: String,
    pub(super) template_version: String,
    pub(super) files_written: Vec<String>,
    pub(super) next_steps: Vec<String>,
}

pub(super) fn handle_plugin_new(args: PluginNewArgs, json: bool) -> Result<()> {
    let kind = args.kind.trim().to_string();
    let name = args.name.trim().to_string();
    if name.is_empty() {
        return Err(invalid_input_error("--name must not be empty"));
    }
    if !KNOWN_KINDS.contains(&kind.as_str()) {
        return Err(invalid_input_error(format!(
            "unknown --kind '{}' (expected one of: {})",
            kind,
            KNOWN_KINDS.join(", ")
        )));
    }
    if !is_valid_kebab_name(&name) {
        return Err(invalid_input_error(format!(
            "--name '{}' must be kebab-case (lowercase letters, digits, and hyphens; start with a letter, end alphanumerically)",
            name
        )));
    }

    let full_name = format!("animus-{}-{}", kind, name);
    let out_dir = args.out_dir.clone().unwrap_or_else(|| PathBuf::from(format!("./{}", full_name)));
    if out_dir.exists() {
        if args.force {
            std::fs::remove_dir_all(&out_dir)
                .with_context(|| format!("failed to overwrite existing output directory {}", out_dir.display()))?;
        } else {
            return Err(invalid_input_error(format!(
                "output directory already exists: {} (pass --force to overwrite)",
                out_dir.display()
            )));
        }
    }

    let description = args.description.clone().unwrap_or_else(|| format!("An Animus {} backend plugin", kind));
    let vars = build_substitution_vars(&name, &kind, &full_name, &description, &args.org)?;

    let _temp_guard;
    let template_root = match args.template_path.as_ref() {
        Some(path) => path.clone(),
        None => {
            let temp = TemplateClone::fetch(&args.template_repo, &args.template_version)?;
            let root = temp.root.clone();
            _temp_guard = Some(temp);
            root
        }
    };

    let kind_dir = template_root.join(&kind);
    if !kind_dir.is_dir() {
        return Err(invalid_input_error(format!(
            "template repo {} does not contain a '{}/' subdirectory",
            template_root.display(),
            kind
        )));
    }

    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create output directory {}", out_dir.display()))?;
    let mut files_written: Vec<String> = Vec::new();
    copy_with_substitution(&kind_dir, &out_dir, &vars, &mut files_written)?;
    files_written.sort();

    let next_steps = vec![
        format!("cd {}", out_dir.display()),
        "cargo build --release".to_string(),
        "cargo test".to_string(),
        format!("animus plugin install --path target/release/{}", full_name),
    ];

    let output = PluginNewOutput {
        name,
        kind,
        full_name,
        out_dir: out_dir.display().to_string(),
        template_repo: args.template_repo,
        template_version: args.template_version,
        files_written,
        next_steps: next_steps.clone(),
    };

    if !json {
        eprintln!("Scaffolded {} files into {}", output.files_written.len(), output.out_dir);
        eprintln!("Next steps:");
        for step in &next_steps {
            eprintln!("  - {}", step);
        }
    }

    print_value(output, json)
}

fn is_valid_kebab_name(name: &str) -> bool {
    if name.len() < 2 {
        return false;
    }
    let bytes = name.as_bytes();
    let first_ok = bytes[0].is_ascii_lowercase();
    let last = bytes[bytes.len() - 1];
    let last_ok = last.is_ascii_lowercase() || last.is_ascii_digit();
    if !first_ok || !last_ok {
        return false;
    }
    name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn build_substitution_vars(
    name: &str,
    kind: &str,
    full_name: &str,
    description: &str,
    org: &str,
) -> Result<BTreeMap<String, String>> {
    let mut vars = BTreeMap::new();
    vars.insert("name".to_string(), name.to_string());
    vars.insert("NAME_UPPER".to_string(), to_upper_snake(name));
    vars.insert("NAME_PASCAL".to_string(), to_pascal(name));
    vars.insert("name_snake".to_string(), to_snake(name));
    vars.insert("kind".to_string(), kind.to_string());
    vars.insert("full_name".to_string(), full_name.to_string());
    vars.insert("description".to_string(), description.to_string());
    vars.insert("org".to_string(), org.to_string());
    vars.insert("year".to_string(), chrono::Utc::now().year().to_string());
    vars.insert("author".to_string(), git_config("user.name").unwrap_or_else(|| "Unknown".to_string()));
    vars.insert(
        "author_email".to_string(),
        git_config("user.email").unwrap_or_else(|| "unknown@example.com".to_string()),
    );
    Ok(vars)
}

fn git_config(key: &str) -> Option<String> {
    let output = Command::new("git").arg("config").arg("--get").arg(key).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn to_upper_snake(name: &str) -> String {
    name.replace('-', "_").to_ascii_uppercase()
}

fn to_snake(name: &str) -> String {
    name.replace('-', "_").to_ascii_lowercase()
}

fn to_pascal(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper_next = true;
    for ch in name.chars() {
        if ch == '-' || ch == '_' {
            upper_next = true;
            continue;
        }
        if upper_next {
            for upper in ch.to_uppercase() {
                out.push(upper);
            }
            upper_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

pub(super) fn substitute(template: &str, vars: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'{' {
            if let Some(close) = find_close(&bytes[i + 2..]) {
                let raw = &template[i + 2..i + 2 + close];
                let key = raw.trim();
                if let Some(value) = vars.get(key) {
                    out.push_str(value);
                    i = i + 2 + close + 2;
                    continue;
                }
            }
        }
        out.push(template[i..].chars().next().expect("non-empty"));
        i += template[i..].chars().next().expect("non-empty").len_utf8();
    }
    out
}

fn find_close(slice: &[u8]) -> Option<usize> {
    let mut j = 0;
    while j + 1 < slice.len() {
        if slice[j] == b'}' && slice[j + 1] == b'}' {
            return Some(j);
        }
        j += 1;
    }
    None
}

pub(super) fn copy_with_substitution(
    src: &Path,
    dst: &Path,
    vars: &BTreeMap<String, String>,
    files_written: &mut Vec<String>,
) -> Result<()> {
    for entry in std::fs::read_dir(src).with_context(|| format!("failed to read {}", src.display()))? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy().to_string();
        if file_type.is_dir() {
            let dst_sub = dst.join(&file_name_str);
            std::fs::create_dir_all(&dst_sub).with_context(|| format!("failed to create {}", dst_sub.display()))?;
            copy_with_substitution(&src_path, &dst_sub, vars, files_written)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let (dst_name, should_substitute) = if let Some(stripped) = file_name_str.strip_suffix(".tmpl") {
            (stripped.to_string(), true)
        } else {
            (file_name_str.clone(), false)
        };
        let dst_path = dst.join(&dst_name);
        if should_substitute {
            let raw = std::fs::read_to_string(&src_path)
                .with_context(|| format!("failed to read template {}", src_path.display()))?;
            let rendered = substitute(&raw, vars);
            std::fs::write(&dst_path, rendered).with_context(|| format!("failed to write {}", dst_path.display()))?;
        } else {
            std::fs::copy(&src_path, &dst_path)
                .with_context(|| format!("failed to copy {} -> {}", src_path.display(), dst_path.display()))?;
        }
        files_written.push(dst_path.display().to_string());
    }
    Ok(())
}

struct TemplateClone {
    root: PathBuf,
    _scratch: tempfile::TempDir,
}

impl TemplateClone {
    fn fetch(repo: &str, git_ref: &str) -> Result<Self> {
        if Command::new("git").arg("--version").output().is_err() {
            return Err(anyhow!(
                "`git` was not found on PATH; install git or pass --template-path to use a local checkout"
            ));
        }
        let scratch = tempfile::TempDir::new().context("failed to create scratch directory for template clone")?;
        let clone_dir = scratch.path().join("template");
        let status = Command::new("git")
            .arg("clone")
            .arg("--depth")
            .arg("1")
            .arg("--branch")
            .arg(git_ref)
            .arg(repo)
            .arg(&clone_dir)
            .status()
            .with_context(|| format!("failed to invoke `git clone` for {}", repo))?;
        if !status.success() {
            return Err(anyhow!("git clone {} (--branch {}) failed with status {:?}", repo, git_ref, status.code()));
        }
        Ok(Self { root: clone_dir, _scratch: scratch })
    }
}
