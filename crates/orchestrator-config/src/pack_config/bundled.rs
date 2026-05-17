use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context, Result};

use super::loading::{load_pack_manifest, parse_pack_manifest, LoadedPackManifest};
use crate::machine_installed_packs_dir;

#[derive(Clone, Copy)]
struct BundledPackFile {
    relative_path: &'static str,
    contents: &'static [u8],
}

#[derive(Clone, Copy)]
struct BundledPackDescriptor {
    pack_id: &'static str,
    manifest_toml: &'static str,
    files: &'static [BundledPackFile],
}

const ANIMUS_REQUIREMENT_FILES: &[BundledPackFile] = &[
    BundledPackFile {
        relative_path: "pack.toml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.requirement/pack.toml"
        )),
    },
    BundledPackFile {
        relative_path: "runtime/agent-runtime.overlay.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.requirement/runtime/agent-runtime.overlay.yaml"
        )),
    },
    BundledPackFile {
        relative_path: "workflows/requirement-pack.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.requirement/workflows/requirement-pack.yaml"
        )),
    },
];

const ANIMUS_REVIEW_FILES: &[BundledPackFile] = &[
    BundledPackFile {
        relative_path: "pack.toml",
        contents: include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/config/bundled-packs/animus.review/pack.toml")),
    },
    BundledPackFile {
        relative_path: "runtime/agent-runtime.overlay.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.review/runtime/agent-runtime.overlay.yaml"
        )),
    },
    BundledPackFile {
        relative_path: "workflows/review-pack.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.review/workflows/review-pack.yaml"
        )),
    },
];

const ANIMUS_TASK_FILES: &[BundledPackFile] = &[
    BundledPackFile {
        relative_path: "pack.toml",
        contents: include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/config/bundled-packs/animus.task/pack.toml")),
    },
    BundledPackFile {
        relative_path: "runtime/agent-runtime.overlay.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.task/runtime/agent-runtime.overlay.yaml"
        )),
    },
    BundledPackFile {
        relative_path: "workflows/task-pack.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.task/workflows/task-pack.yaml"
        )),
    },
];

const ANIMUS_CORE_SKILLS_FILES: &[BundledPackFile] = &[
    BundledPackFile {
        relative_path: "pack.toml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.core-skills/pack.toml"
        )),
    },
    BundledPackFile {
        relative_path: "skills/api-documentation.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.core-skills/skills/api-documentation.yaml"
        )),
    },
    BundledPackFile {
        relative_path: "skills/architecture-review.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.core-skills/skills/architecture-review.yaml"
        )),
    },
    BundledPackFile {
        relative_path: "skills/changelog-generation.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.core-skills/skills/changelog-generation.yaml"
        )),
    },
    BundledPackFile {
        relative_path: "skills/ci-cd-authoring.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.core-skills/skills/ci-cd-authoring.yaml"
        )),
    },
    BundledPackFile {
        relative_path: "skills/code-analysis.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.core-skills/skills/code-analysis.yaml"
        )),
    },
    BundledPackFile {
        relative_path: "skills/code-review.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.core-skills/skills/code-review.yaml"
        )),
    },
    BundledPackFile {
        relative_path: "skills/debugging.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.core-skills/skills/debugging.yaml"
        )),
    },
    BundledPackFile {
        relative_path: "skills/deep-search.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.core-skills/skills/deep-search.yaml"
        )),
    },
    BundledPackFile {
        relative_path: "skills/impact-analysis.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.core-skills/skills/impact-analysis.yaml"
        )),
    },
    BundledPackFile {
        relative_path: "skills/implementation.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.core-skills/skills/implementation.yaml"
        )),
    },
    BundledPackFile {
        relative_path: "skills/incident-response.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.core-skills/skills/incident-response.yaml"
        )),
    },
    BundledPackFile {
        relative_path: "skills/pr-summary.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.core-skills/skills/pr-summary.yaml"
        )),
    },
    BundledPackFile {
        relative_path: "skills/prioritization.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.core-skills/skills/prioritization.yaml"
        )),
    },
    BundledPackFile {
        relative_path: "skills/refactoring.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.core-skills/skills/refactoring.yaml"
        )),
    },
    BundledPackFile {
        relative_path: "skills/release-management.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.core-skills/skills/release-management.yaml"
        )),
    },
    BundledPackFile {
        relative_path: "skills/security-audit.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.core-skills/skills/security-audit.yaml"
        )),
    },
    BundledPackFile {
        relative_path: "skills/task-decomposition.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.core-skills/skills/task-decomposition.yaml"
        )),
    },
    BundledPackFile {
        relative_path: "skills/technical-writing.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.core-skills/skills/technical-writing.yaml"
        )),
    },
    BundledPackFile {
        relative_path: "skills/unit-testing.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.core-skills/skills/unit-testing.yaml"
        )),
    },
];

const BUNDLED_PACKS: &[BundledPackDescriptor] = &[
    BundledPackDescriptor {
        pack_id: "animus.core-skills",
        manifest_toml: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.core-skills/pack.toml"
        )),
        files: ANIMUS_CORE_SKILLS_FILES,
    },
    BundledPackDescriptor {
        pack_id: "animus.requirement",
        manifest_toml: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.requirement/pack.toml"
        )),
        files: ANIMUS_REQUIREMENT_FILES,
    },
    BundledPackDescriptor {
        pack_id: "animus.review",
        manifest_toml: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/animus.review/pack.toml"
        )),
        files: ANIMUS_REVIEW_FILES,
    },
    BundledPackDescriptor {
        pack_id: "animus.task",
        manifest_toml: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/config/bundled-packs/animus.task/pack.toml")),
        files: ANIMUS_TASK_FILES,
    },
];

pub fn has_bundled_pack(pack_id: &str) -> bool {
    bundled_pack_descriptor(pack_id).is_some()
}

pub fn ensure_bundled_pack_installed(pack_id: &str) -> Result<LoadedPackManifest> {
    let descriptor =
        bundled_pack_descriptor(pack_id).ok_or_else(|| anyhow!("bundled pack '{}' is not available", pack_id))?;
    let manifest = parse_pack_manifest(descriptor.manifest_toml)?;

    for dependency in &manifest.dependencies {
        if dependency.optional || !has_bundled_pack(&dependency.id) {
            continue;
        }
        let _ = ensure_bundled_pack_installed(&dependency.id)?;
    }

    let target_root = machine_installed_packs_dir().join(&manifest.id).join(&manifest.version);
    if target_root.exists() {
        if let Ok(loaded) = load_pack_manifest(&target_root) {
            return Ok(loaded);
        }
        fs::remove_dir_all(&target_root)
            .with_context(|| format!("failed to remove invalid bundled pack install at {}", target_root.display()))?;
    }

    write_bundled_pack(&target_root, descriptor)?;
    load_pack_manifest(&target_root)
}

fn bundled_pack_descriptor(pack_id: &str) -> Option<&'static BundledPackDescriptor> {
    BUNDLED_PACKS.iter().find(|descriptor| descriptor.pack_id.eq_ignore_ascii_case(pack_id))
}

fn write_bundled_pack(target_root: &Path, descriptor: &BundledPackDescriptor) -> Result<()> {
    for file in descriptor.files {
        let path = target_root.join(file.relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&path, file.contents).with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(())
}
