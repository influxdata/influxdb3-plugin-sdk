//! Path shortening for human-mode CLI output.
//!
//! Absolute paths in success output (`/Users/<name>/.../build/foo.tar.gz`)
//! are noisy in terminals, demos, screenshots, and CI logs. This module
//! shortens a path for display when it is a descendant of the current
//! working directory; otherwise it falls back to the absolute form so
//! the output is never ambiguous or polluted with `../../..` traversal.
//!
//! JSON-mode output is unaffected by this helper — programmatic
//! consumers continue to receive the canonical absolute paths from the
//! command payload.

use std::path::Path;

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
