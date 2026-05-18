//! Shell-style environment variable interpolation for workflow YAML.
//!
//! Substitution happens against the raw file contents before YAML parsing so every
//! string field (subject configs, provider tokens, env override blocks, workflow
//! metadata, etc.) accepts the same syntax uniformly.
//!
//! Supported syntax (modeled after docker-compose / POSIX shell):
//!
//! | Form              | Meaning                                        |
//! | ----------------- | ---------------------------------------------- |
//! | `${VAR}`          | Required. Errors if `VAR` is unset.            |
//! | `${VAR:-default}` | Optional. Falls back to `default` if unset.    |
//! | `${VAR:?message}` | Required with a custom error message.          |
//! | `$$`              | Literal `$`.                                   |
//!
//! Errors include the YAML file path and 1-based line number of the offending
//! reference for fast diagnosis.

use std::env;

use anyhow::{anyhow, Result};

/// Resolve a single `${...}` reference against the process environment.
///
/// This is factored out so tests can stub the environment via `EnvVarGuard`
/// without needing to plumb a custom resolver through the call sites.
fn lookup_env(key: &str) -> Option<String> {
    env::var(key).ok()
}

/// Interpolate shell-style `${VAR}` references in `content`.
///
/// `source_label` is included in error messages — pass the YAML file path
/// (or any human-readable identifier) so users can locate the offending file.
pub fn interpolate_env(content: &str, source_label: &str) -> Result<String> {
    interpolate_env_with(content, source_label, lookup_env)
}

/// Implementation seam used by unit tests to inject a hermetic env lookup.
pub(crate) fn interpolate_env_with<F>(content: &str, source_label: &str, resolver: F) -> Result<String>
where
    F: Fn(&str) -> Option<String>,
{
    // Walk byte-wise but push str slices so multi-byte UTF-8 sequences are
    // preserved intact. `$` is always ASCII (0x24), so it cannot appear inside
    // a multi-byte UTF-8 sequence — splitting on `$` boundaries is safe.
    let bytes = content.as_bytes();
    let mut out = String::with_capacity(content.len());
    let mut i = 0usize;
    let mut copy_from = 0usize;

    while i < bytes.len() {
        if bytes[i] != b'$' {
            i += 1;
            continue;
        }

        // Flush everything since the last `$` (or start) as a single str slice.
        out.push_str(&content[copy_from..i]);

        // `$$` escapes a literal `$`.
        if i + 1 < bytes.len() && bytes[i + 1] == b'$' {
            out.push('$');
            i += 2;
            copy_from = i;
            continue;
        }

        // `${...}` reference.
        if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            let start = i;
            let body_start = i + 2;
            let Some(close_off) = find_matching_close(&bytes[body_start..]) else {
                let line = line_number_for_offset(content, start);
                return Err(anyhow!(
                    "workflow YAML at {} line {} contains an unterminated `${{` env-var reference",
                    source_label,
                    line
                ));
            };
            let body = &content[body_start..body_start + close_off];
            let resolved = resolve_reference(body, source_label, &resolver, || line_number_for_offset(content, start))?;
            out.push_str(&resolved);
            i = body_start + close_off + 1; // skip past `}`
            copy_from = i;
            continue;
        }

        // Lone `$` not followed by `{` or `$` passes through literally so YAML
        // strings like `cost $5` aren't disturbed.
        out.push('$');
        i += 1;
        copy_from = i;
    }

    out.push_str(&content[copy_from..]);
    Ok(out)
}

/// Scan `bytes` for the first unmatched `}`. Tracks brace depth so nested
/// `${VAR:-${OTHER}}` would still be parsed coherently if we choose to support
/// nesting later. For now we don't recurse — but balancing keeps us honest.
fn find_matching_close(bytes: &[u8]) -> Option<usize> {
    let mut depth = 0i32;
    for (idx, &b) in bytes.iter().enumerate() {
        if b == b'{' {
            depth += 1;
        } else if b == b'}' {
            if depth == 0 {
                return Some(idx);
            }
            depth -= 1;
        }
    }
    None
}

fn resolve_reference<F, L>(body: &str, source_label: &str, resolver: &F, line_of: L) -> Result<String>
where
    F: Fn(&str) -> Option<String>,
    L: Fn() -> usize,
{
    // Split on the first ':-' or ':?' modifier.
    if let Some(idx) = body.find(":-") {
        let name = body[..idx].trim();
        validate_name(name, source_label, &line_of)?;
        let default = &body[idx + 2..];
        return Ok(resolver(name).unwrap_or_else(|| default.to_string()));
    }
    if let Some(idx) = body.find(":?") {
        let name = body[..idx].trim();
        validate_name(name, source_label, &line_of)?;
        let message = body[idx + 2..].trim();
        return match resolver(name) {
            Some(value) => Ok(value),
            None => Err(anyhow!(
                "workflow YAML at {} line {} requires env var {}: {}",
                source_label,
                line_of(),
                name,
                if message.is_empty() { "value is unset" } else { message }
            )),
        };
    }

    let name = body.trim();
    validate_name(name, source_label, &line_of)?;
    match resolver(name) {
        Some(value) => Ok(value),
        None => Err(anyhow!("workflow YAML at {} line {} references unset env var {}.", source_label, line_of(), name)),
    }
}

fn validate_name<L>(name: &str, source_label: &str, line_of: &L) -> Result<()>
where
    L: Fn() -> usize,
{
    if name.is_empty() {
        return Err(anyhow!(
            "workflow YAML at {} line {} has an empty `${{}}` env-var reference",
            source_label,
            line_of()
        ));
    }
    if !name.chars().next().map(|c| c == '_' || c.is_ascii_alphabetic()).unwrap_or(false) {
        return Err(anyhow!(
            "workflow YAML at {} line {} env var name `{}` must start with a letter or underscore",
            source_label,
            line_of(),
            name
        ));
    }
    if !name.chars().all(|c| c == '_' || c.is_ascii_alphanumeric()) {
        return Err(anyhow!(
            "workflow YAML at {} line {} env var name `{}` may only contain letters, digits, and underscores",
            source_label,
            line_of(),
            name
        ));
    }
    Ok(())
}

fn line_number_for_offset(content: &str, offset: usize) -> usize {
    let clamped = offset.min(content.len());
    content[..clamped].bytes().filter(|b| *b == b'\n').count() + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{env_lock, EnvVarGuard};

    const KEY: &str = "ANIMUS_TEST_ENV_INTERP_VALUE";
    const OTHER_KEY: &str = "ANIMUS_TEST_ENV_INTERP_OTHER";

    #[test]
    fn expands_required_var() {
        let _g = env_lock().lock().unwrap();
        let _v = EnvVarGuard::set(KEY, "secret-token");
        let out = interpolate_env(&format!("api_token: ${{{}}}\n", KEY), "test.yaml").unwrap();
        assert_eq!(out, "api_token: secret-token\n");
    }

    #[test]
    fn errors_clearly_when_required_var_unset() {
        let _g = env_lock().lock().unwrap();
        let _v = EnvVarGuard::unset(KEY);
        let src = format!("a: 1\nb: 2\napi_token: ${{{}}}\n", KEY);
        let err = interpolate_env(&src, ".animus/workflows/agents.yaml").unwrap_err();
        let msg = format!("{:#}", err);
        assert!(msg.contains("line 3"), "missing line number: {msg}");
        assert!(msg.contains(KEY), "missing var name: {msg}");
        assert!(msg.contains(".animus/workflows/agents.yaml"), "missing source label: {msg}");
    }

    #[test]
    fn uses_default_when_var_unset_with_default_syntax() {
        let _g = env_lock().lock().unwrap();
        let _v = EnvVarGuard::unset(KEY);
        let out = interpolate_env(&format!("api_url: ${{{}:-https://api.example.com}}\n", KEY), "test.yaml").unwrap();
        assert_eq!(out, "api_url: https://api.example.com\n");
    }

    #[test]
    fn prefers_set_var_over_default() {
        let _g = env_lock().lock().unwrap();
        let _v = EnvVarGuard::set(KEY, "https://real.example.com");
        let out =
            interpolate_env(&format!("api_url: ${{{}:-https://fallback.example.com}}\n", KEY), "test.yaml").unwrap();
        assert_eq!(out, "api_url: https://real.example.com\n");
    }

    #[test]
    fn handles_multiple_vars_in_one_line() {
        let _g = env_lock().lock().unwrap();
        let _v1 = EnvVarGuard::set(KEY, "alpha");
        let _v2 = EnvVarGuard::set(OTHER_KEY, "beta");
        let out = interpolate_env(&format!("combo: \"${{{}}}-${{{}}}\"\n", KEY, OTHER_KEY), "test.yaml").unwrap();
        assert_eq!(out, "combo: \"alpha-beta\"\n");
    }

    #[test]
    fn escapes_literal_dollar_with_double_dollar() {
        let _g = env_lock().lock().unwrap();
        let _v = EnvVarGuard::unset(KEY);
        let out = interpolate_env("price: $$5.00 raw\n", "test.yaml").unwrap();
        assert_eq!(out, "price: $5.00 raw\n");
    }

    #[test]
    fn required_with_custom_message() {
        let _g = env_lock().lock().unwrap();
        let _v = EnvVarGuard::unset(KEY);
        let src = format!("a: ${{{}:?set this in your shell}}\n", KEY);
        let err = interpolate_env(&src, "test.yaml").unwrap_err();
        let msg = format!("{:#}", err);
        assert!(msg.contains("set this in your shell"), "missing custom message: {msg}");
        assert!(msg.contains(KEY));
    }

    #[test]
    fn lone_dollar_passes_through() {
        let _g = env_lock().lock().unwrap();
        let out = interpolate_env("note: this costs $5 in total\n", "test.yaml").unwrap();
        assert_eq!(out, "note: this costs $5 in total\n");
    }

    #[test]
    fn unterminated_reference_errors_with_line() {
        let _g = env_lock().lock().unwrap();
        let src = "ok: yes\nbroken: ${MISSING_BRACE\n";
        let err = interpolate_env(src, "test.yaml").unwrap_err();
        let msg = format!("{:#}", err);
        assert!(msg.contains("line 2"), "missing line: {msg}");
        assert!(msg.contains("unterminated"));
    }

    #[test]
    fn rejects_empty_name() {
        let _g = env_lock().lock().unwrap();
        let err = interpolate_env("a: ${}\n", "test.yaml").unwrap_err();
        let msg = format!("{:#}", err);
        assert!(msg.contains("empty"));
    }

    #[test]
    fn preserves_multibyte_utf8_around_substitution() {
        // Em-dash (U+2014) is 3 bytes in UTF-8 and previously triggered control-character
        // YAML parse errors when the interpolator walked byte-by-byte.
        let _g = env_lock().lock().unwrap();
        let _v = EnvVarGuard::set(KEY, "expanded");
        let src = format!("note: a — b — ${{{}}}\nemoji: 🚀 — done\n", KEY);
        let out = interpolate_env(&src, "test.yaml").unwrap();
        assert_eq!(out, "note: a — b — expanded\nemoji: 🚀 — done\n");
    }

    #[test]
    fn rejects_invalid_name() {
        let _g = env_lock().lock().unwrap();
        let err = interpolate_env("a: ${1BAD}\n", "test.yaml").unwrap_err();
        let msg = format!("{:#}", err);
        assert!(msg.contains("must start with"));
    }
}
