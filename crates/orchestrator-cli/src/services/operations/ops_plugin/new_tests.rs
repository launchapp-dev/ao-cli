use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::{handle_plugin_new, substitute};
use crate::PluginNewArgs;

/// Build a minimal `subject/` template fixture under `root`. Mirrors the
/// real `animus-plugin-template` layout closely enough that the scaffold
/// engine exercises every code path: nested directories, `.tmpl` files,
/// verbatim files, and `{{var}}` substitution markers.
fn write_subject_fixture(root: &Path) {
    let subject = root.join("subject");
    std::fs::create_dir_all(subject.join("src")).unwrap();
    std::fs::create_dir_all(subject.join("tests")).unwrap();

    std::fs::write(
        subject.join("Cargo.toml.tmpl"),
        "[package]\nname = \"{{full_name}}\"\ndescription = \"{{description}}\"\nauthors = [\"{{author}}\"]\n",
    )
    .unwrap();
    std::fs::write(
        subject.join("plugin.toml.tmpl"),
        "name = \"{{full_name}}\"\nplugin_kind = \"{{kind}}_backend\"\n[[env]]\nname = \"{{NAME_UPPER}}_API_TOKEN\"\n",
    )
    .unwrap();
    std::fs::write(
        subject.join("src/main.rs.tmpl"),
        "fn main() {{ let _ = \"{{name_snake}}\"; println!(\"{{NAME_PASCAL}}\"); }}\n",
    )
    .unwrap();
    std::fs::write(subject.join("src/lib.rs.tmpl"), "//! {{full_name}}\npub mod backend;\n").unwrap();
    std::fs::write(subject.join("tests/contract.rs.tmpl"), "// contract test for {{full_name}}\n").unwrap();
    std::fs::write(subject.join("README.md.tmpl"), "# {{full_name}}\n").unwrap();
    // Verbatim — no `.tmpl` suffix.
    std::fs::write(subject.join(".gitignore"), "target/\n").unwrap();
}

fn write_provider_fixture(root: &Path) {
    let provider = root.join("provider");
    std::fs::create_dir_all(provider.join("src")).unwrap();
    std::fs::write(provider.join("Cargo.toml.tmpl"), "[package]\nname = \"{{full_name}}\"\n").unwrap();
    std::fs::write(provider.join("src/main.rs.tmpl"), "// {{NAME_PASCAL}}\nfn main() {{}}\n").unwrap();
}

fn make_args(template_root: &Path, kind: &str, name: &str, out_dir: PathBuf) -> PluginNewArgs {
    PluginNewArgs {
        kind: kind.to_string(),
        name: name.to_string(),
        org: "launchapp-dev".to_string(),
        description: Some("Test plugin".to_string()),
        out_dir: Some(out_dir),
        template_version: "main".to_string(),
        template_repo: "https://example.invalid/unused".to_string(),
        template_path: Some(template_root.to_path_buf()),
        force: false,
    }
}

#[test]
fn scaffold_subject_substitutes_and_strips_tmpl() {
    let template_dir = TempDir::new().unwrap();
    write_subject_fixture(template_dir.path());

    let out_root = TempDir::new().unwrap();
    let out_dir = out_root.path().join("animus-subject-jira");
    let args = make_args(template_dir.path(), "subject", "jira", out_dir.clone());

    handle_plugin_new(args, true).expect("scaffold should succeed");

    let cargo = std::fs::read_to_string(out_dir.join("Cargo.toml")).unwrap();
    assert!(cargo.contains("name = \"animus-subject-jira\""), "got: {cargo}");
    assert!(cargo.contains("description = \"Test plugin\""), "got: {cargo}");

    let plugin = std::fs::read_to_string(out_dir.join("plugin.toml")).unwrap();
    assert!(plugin.contains("plugin_kind = \"subject_backend\""), "got: {plugin}");
    assert!(plugin.contains("JIRA_API_TOKEN"), "expected NAME_UPPER substitution: {plugin}");

    let main = std::fs::read_to_string(out_dir.join("src/main.rs")).unwrap();
    assert!(main.contains("\"jira\""), "expected name_snake substitution: {main}");
    assert!(main.contains("Jira"), "expected NAME_PASCAL substitution: {main}");

    assert!(out_dir.join("src/lib.rs").exists(), "lib.rs should be created from lib.rs.tmpl");
    assert!(out_dir.join("tests/contract.rs").exists(), "tests/contract.rs should be created");
    assert!(!out_dir.join("Cargo.toml.tmpl").exists(), ".tmpl suffix should be stripped");
    let gitignore = std::fs::read_to_string(out_dir.join(".gitignore")).unwrap();
    assert_eq!(gitignore, "target/\n", "verbatim files should not be substituted");
}

#[test]
fn scaffold_provider_writes_to_default_outdir() {
    let template_dir = TempDir::new().unwrap();
    write_provider_fixture(template_dir.path());

    let work_root = TempDir::new().unwrap();
    let expected = work_root.path().join("animus-provider-foo");
    let args = PluginNewArgs {
        kind: "provider".to_string(),
        name: "foo".to_string(),
        org: "launchapp-dev".to_string(),
        description: None,
        out_dir: Some(expected.clone()),
        template_version: "main".to_string(),
        template_repo: "https://example.invalid/unused".to_string(),
        template_path: Some(template_dir.path().to_path_buf()),
        force: false,
    };
    handle_plugin_new(args, true).expect("scaffold should succeed");
    assert!(expected.join("Cargo.toml").exists(), "default convention animus-<kind>-<name> should be honored");
    let cargo = std::fs::read_to_string(expected.join("Cargo.toml")).unwrap();
    assert!(cargo.contains("animus-provider-foo"), "got: {cargo}");
}

#[test]
fn scaffold_fails_when_kind_unknown() {
    let template_dir = TempDir::new().unwrap();
    write_subject_fixture(template_dir.path());
    let out_root = TempDir::new().unwrap();
    let args = make_args(template_dir.path(), "made-up", "jira", out_root.path().join("out"));

    let err = handle_plugin_new(args, true).expect_err("unknown kind should fail");
    let message = format!("{err:#}");
    assert!(message.contains("unknown --kind"), "got: {message}");
    assert!(message.contains("made-up"), "got: {message}");
}

#[test]
fn scaffold_fails_when_output_already_exists() {
    let template_dir = TempDir::new().unwrap();
    write_subject_fixture(template_dir.path());
    let out_root = TempDir::new().unwrap();
    let out_dir = out_root.path().join("already-here");
    std::fs::create_dir_all(&out_dir).unwrap();
    let args = make_args(template_dir.path(), "subject", "jira", out_dir.clone());

    let err = handle_plugin_new(args, true).expect_err("existing output should fail without --force");
    let message = format!("{err:#}");
    assert!(message.contains("already exists"), "got: {message}");

    let mut args_force = make_args(template_dir.path(), "subject", "jira", out_dir.clone());
    args_force.force = true;
    handle_plugin_new(args_force, true).expect("--force should overwrite");
    assert!(out_dir.join("Cargo.toml").exists(), "overwrite should produce scaffold");
}

#[test]
fn scaffold_uses_template_path_when_provided() {
    // Point at an offline template fixture; template_repo is an invalid URL on
    // purpose. If the scaffold tried to clone, the test would fail with a
    // network/clone error instead of succeeding.
    let template_dir = TempDir::new().unwrap();
    write_subject_fixture(template_dir.path());
    let out_root = TempDir::new().unwrap();
    let out_dir = out_root.path().join("animus-subject-jira");
    let args = PluginNewArgs {
        kind: "subject".to_string(),
        name: "jira".to_string(),
        org: "launchapp-dev".to_string(),
        description: Some("offline".to_string()),
        out_dir: Some(out_dir.clone()),
        template_version: "main".to_string(),
        template_repo: "https://does-not-resolve.invalid/repo.git".to_string(),
        template_path: Some(template_dir.path().to_path_buf()),
        force: false,
    };
    handle_plugin_new(args, true).expect("template_path should bypass git clone");
    assert!(out_dir.join("Cargo.toml").exists());
}

#[test]
fn substitute_handles_missing_keys_by_leaving_marker() {
    let mut vars = std::collections::BTreeMap::new();
    vars.insert("name".to_string(), "jira".to_string());
    let rendered = substitute("hello {{name}} and {{missing}}", &vars);
    assert_eq!(rendered, "hello jira and {{missing}}");
}

#[test]
fn substitute_handles_whitespace_inside_markers() {
    let mut vars = std::collections::BTreeMap::new();
    vars.insert("full_name".to_string(), "animus-subject-jira".to_string());
    let rendered = substitute("{{ full_name }}", &vars);
    assert_eq!(rendered, "animus-subject-jira");
}
