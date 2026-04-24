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

const AO_REQUIREMENT_FILES: &[BundledPackFile] = &[
    BundledPackFile {
        relative_path: "pack.toml",
        contents: include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/config/bundled-packs/ao.requirement/pack.toml")),
    },
    BundledPackFile {
        relative_path: "runtime/agent-runtime.overlay.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/ao.requirement/runtime/agent-runtime.overlay.yaml"
        )),
    },
    BundledPackFile {
        relative_path: "workflows/requirement-pack.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/ao.requirement/workflows/requirement-pack.yaml"
        )),
    },
];

const AO_REVIEW_FILES: &[BundledPackFile] = &[
    BundledPackFile {
        relative_path: "pack.toml",
        contents: include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/config/bundled-packs/ao.review/pack.toml")),
    },
    BundledPackFile {
        relative_path: "runtime/agent-runtime.overlay.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/ao.review/runtime/agent-runtime.overlay.yaml"
        )),
    },
    BundledPackFile {
        relative_path: "workflows/review-pack.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/ao.review/workflows/review-pack.yaml"
        )),
    },
];

const AO_TASK_FILES: &[BundledPackFile] = &[
    BundledPackFile {
        relative_path: "pack.toml",
        contents: include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/config/bundled-packs/ao.task/pack.toml")),
    },
    BundledPackFile {
        relative_path: "runtime/agent-runtime.overlay.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/ao.task/runtime/agent-runtime.overlay.yaml"
        )),
    },
    BundledPackFile {
        relative_path: "workflows/task-pack.yaml",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/ao.task/workflows/task-pack.yaml"
        )),
    },
];

const BUNDLED_PACKS: &[BundledPackDescriptor] = &[
    BundledPackDescriptor {
        pack_id: "ao.requirement",
        manifest_toml: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config/bundled-packs/ao.requirement/pack.toml"
        )),
        files: AO_REQUIREMENT_FILES,
    },
    BundledPackDescriptor {
        pack_id: "ao.review",
        manifest_toml: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/config/bundled-packs/ao.review/pack.toml")),
        files: AO_REVIEW_FILES,
    },
    BundledPackDescriptor {
        pack_id: "ao.task",
        manifest_toml: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/config/bundled-packs/ao.task/pack.toml")),
        files: AO_TASK_FILES,
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
