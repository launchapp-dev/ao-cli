//! Disk-space checks for the scoped runtime root.
//!
//! Uses `statvfs` on unix to compute available bytes. Threshold: warn under
//! 1 GiB free, fail under 100 MiB.

use std::path::Path;

use super::check_kit::{CheckContext, CheckFix, CheckStatus, DiagnosticCheck};

const CATEGORY: &str = "disk";
const WARN_BELOW_BYTES: u64 = 1024 * 1024 * 1024; // 1 GiB
const FAIL_BELOW_BYTES: u64 = 100 * 1024 * 1024; // 100 MiB

pub(crate) fn run(ctx: &CheckContext) -> Vec<DiagnosticCheck> {
    let target = protocol::scoped_state_root(&ctx.project_root).unwrap_or_else(|| ctx.project_root.join(".animus"));

    let mut check = DiagnosticCheck::new("disk_space_scoped_root", CATEGORY, CheckStatus::Pass, "Free disk space");

    match free_bytes(&target) {
        Ok(bytes) => {
            let human = format_bytes(bytes);
            check.current = Some(format!("{human} free at {}", target.display()));
            if bytes < FAIL_BELOW_BYTES {
                check.status = CheckStatus::Fail;
                check.expected = Some(format!(">= {} free", format_bytes(WARN_BELOW_BYTES)));
                check.fixes.push(CheckFix::manual(
                    "free_disk_space",
                    "Free disk space, then re-run; the daemon writes session and artifact data here.",
                ));
            } else if bytes < WARN_BELOW_BYTES {
                check.status = CheckStatus::Warn;
                check.expected = Some(format!(">= {} free", format_bytes(WARN_BELOW_BYTES)));
                check.fixes.push(CheckFix::manual(
                    "free_disk_space",
                    "Consider freeing disk space; long-running workflows may exhaust the partition.",
                ));
            } else {
                check.details = format!("{human} free under {}", target.display());
            }
            check
        }
        Err(e) => DiagnosticCheck::new("disk_space_scoped_root", CATEGORY, CheckStatus::Warn, "Free disk space")
            .current(format!("statvfs failed: {e}"))
            .expected(format!("statvfs succeeds on {}", target.display())),
    }
    .pipe(|c| vec![c])
}

fn free_bytes(path: &Path) -> Result<u64, String> {
    // Walk up to the nearest existing ancestor; `available_space` requires
    // the path to exist.
    let target = if path.exists() {
        path.to_path_buf()
    } else {
        path.ancestors().find(|p| p.exists()).map(|p| p.to_path_buf()).unwrap_or_else(|| path.to_path_buf())
    };
    fs2::available_space(&target).map_err(|e| e.to_string())
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.2} GiB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MiB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KiB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

// Tiny inline pipe helper so the unit-returning expression chains read cleanly.
trait Pipe: Sized {
    fn pipe<U>(self, f: impl FnOnce(Self) -> U) -> U {
        f(self)
    }
}
impl<T> Pipe for T {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bytes_renders_at_each_scale() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(2048), "2.00 KiB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.00 MiB");
        assert_eq!(format_bytes(2 * 1024 * 1024 * 1024), "2.00 GiB");
    }
}
