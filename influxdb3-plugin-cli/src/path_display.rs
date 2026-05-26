//! Path helpers for CLI output.
//!
//! - Human mode: [`display_relative_to_cwd`] shortens an absolute path
//!   to CWD-relative form when the path is a descendant of the working
//!   directory; otherwise it falls back to the absolute form so the
//!   output is never ambiguous or polluted with `../../..` traversal.
//! - JSON mode: [`absolutize_for_json`] returns the lexical absolute
//!   form of a path (via [`std::path::absolute`]). No FS access, no
//!   symlink resolution — the emitted path mirrors the caller-supplied
//!   structure. Failure surfaces as a structured [`CliError`] rather
//!   than a silent fallback that could leak a relative path.

use std::path::{Path, PathBuf};

use crate::cli_error::CliError;
use crate::output::json::JsonError;

/// Lexical absolute form of `path` for emission as a JSON path field.
///
/// **Policy:** lexical display, not canonical identity.
/// - Joins relative input onto the process CWD.
/// - Does not resolve symlinks (`/repo/link/foo` stays
///   `/repo/link/foo`, never collapses to the link target).
/// - Does not collapse `..` components (on Unix; see
///   [`std::path::absolute`] for platform notes). A caller-supplied
///   `..` segment is preserved so OS-level path resolution at write
///   time matches the user's intent in the presence of symlinks.
/// - Does not touch the filesystem; safe to call before any write.
///
/// The only failure mode is an unreadable process CWD; that surfaces
/// as a structured [`CliError::Runtime`] so the JSON contract
/// ("`target_dir` / `index_path` / `artifact_path` are absolute")
/// cannot quietly degrade to a relative path.
pub(crate) fn absolutize_for_json(path: &Path) -> Result<PathBuf, anyhow::Error> {
    std::path::absolute(path).map_err(|source| {
        CliError::runtime(JsonError {
            code: "path::resolution_failed".into(),
            message: format!("could not resolve path {path:?}: {source}"),
            field: None,
            details: None,
            diagnostics: vec![],
            cause: vec![],
        })
    })
}

/// Returns a display string for `path` shortened to a CWD-relative form
/// when possible. Reads the process CWD via [`std::env::current_dir`].
pub(crate) fn display_relative_to_cwd(path: &Path) -> String {
    let Ok(cwd) = std::env::current_dir() else {
        return path.display().to_string();
    };
    display_relative_to(path, &cwd)
}

/// CWD-injectable variant: returns `path` relative to `cwd` when `path`
/// is a descendant of `cwd`; falls back to the absolute form otherwise.
/// Returns `"."` when `path == cwd`.
fn display_relative_to(path: &Path, cwd: &Path) -> String {
    match path.strip_prefix(cwd) {
        Ok(rel) if rel.as_os_str().is_empty() => ".".to_string(),
        Ok(rel) => rel.display().to_string(),
        Err(_) => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn descendant_path_is_shortened() {
        let cwd = PathBuf::from("/home/u/proj");
        let nested = cwd.join("build").join("foo.tar.gz");
        assert_eq!(
            display_relative_to(&nested, &cwd),
            PathBuf::from("build")
                .join("foo.tar.gz")
                .display()
                .to_string()
        );
    }

    #[test]
    fn cwd_itself_renders_as_dot() {
        let cwd = PathBuf::from("/home/u/proj");
        assert_eq!(display_relative_to(&cwd, &cwd), ".");
    }

    #[test]
    fn non_descendant_path_falls_back_to_absolute() {
        let cwd = PathBuf::from("/home/u/proj");
        let other = PathBuf::from("/var/lib/something/file");
        assert_eq!(
            display_relative_to(&other, &cwd),
            other.display().to_string()
        );
    }

    #[test]
    fn sibling_path_falls_back_to_absolute() {
        let cwd = PathBuf::from("/home/u/proj");
        let sibling = PathBuf::from("/home/u/other/file");
        assert_eq!(
            display_relative_to(&sibling, &cwd),
            sibling.display().to_string()
        );
    }
}
