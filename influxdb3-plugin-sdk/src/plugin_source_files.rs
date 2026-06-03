//! Shared plugin source-file selection.
//!
//! Walks a plugin directory and applies the manifest `[plugin].exclude`
//! gitignore-style patterns *relative to the plugin root*, returning the
//! selected files in deterministic order. Packaging ([`crate::archive`]) and
//! validation ([`crate::validate`]) both call [`select`] so their selected
//! file sets are identical for the same plugin and exclude list.
//!
//! # Matcher choice
//!
//! Uses [`ignore::gitignore::GitignoreBuilder`] / [`Gitignore`] — NOT
//! `ignore::WalkBuilder`. `WalkBuilder`'s defaults read `.ignore`,
//! `.gitignore`, git excludes, the global gitignore, and hidden-file rules,
//! all of which are explicit non-goals: only manifest `exclude` controls
//! selection. The walk is `walkdir` with no ignore-file awareness.
//!
//! # No directory pruning
//!
//! Excluded directories are **not** pruned during traversal. Selection matches
//! files after discovery so a later `!` negation can re-include a file beneath
//! a directory removed by an earlier pattern. Trade-off: excluded directories
//! are still traversed and walk errors inside them may still surface.

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::path::{Component, Path, PathBuf};

/// A selected plugin source file.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SelectedFile {
    /// Absolute path on disk (canonicalized root joined with `relative`).
    pub absolute: PathBuf,
    /// Path relative to the plugin root.
    pub relative: PathBuf,
    /// `relative` rendered with `/` separators — the deterministic sort key
    /// and the archive-path suffix.
    pub normalized: String,
    /// Whether the file carries the Unix executable bit.
    pub is_exec: bool,
}

/// Failure modes of source-file selection.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SelectError {
    /// A manifest `[plugin].exclude` entry is not a valid gitignore pattern.
    /// Names the exact offending pattern.
    #[error("invalid exclude pattern {pattern:?}: {message}")]
    InvalidExcludePattern { pattern: String, message: String },
    /// An I/O error canonicalizing the root, walking, or stat-ing a file.
    #[error("I/O error{}", .path.as_ref().map(|p| format!(" at {}", p.display())).unwrap_or_default())]
    Io {
        #[source]
        source: std::io::Error,
        path: Option<PathBuf>,
    },
    /// A walkdir traversal error with no underlying I/O error.
    #[error("walk error: {message}")]
    Walk { message: String },
}

/// Walks `plugin_dir` and returns the files selected after applying `exclude`.
///
/// Skips directory entries, symlinks, and other non-regular files (preserving
/// historical packaging behavior). Patterns are gitignore-style and resolved
/// relative to the plugin root. The result is sorted lexicographically by the
/// normalized (`/`-separated) relative path for cross-platform determinism.
/// `normalized` is built from `to_string_lossy`, so the sort is over UTF-8
/// code units. For the ASCII paths plugins use in practice this is the natural
/// byte order; non-ASCII names are sorted by their lossy UTF-8 form.
pub fn select(plugin_dir: &Path, exclude: &[String]) -> Result<Vec<SelectedFile>, SelectError> {
    let root = std::fs::canonicalize(plugin_dir).map_err(|source| SelectError::Io {
        source,
        path: Some(plugin_dir.to_path_buf()),
    })?;

    let matcher = build_matcher(&root, exclude)?;

    let mut out = Vec::new();
    let walk = walkdir::WalkDir::new(&root).follow_links(false);
    for result in walk {
        let entry = result.map_err(|e| {
            if e.io_error().is_some() {
                let path = e.path().map(|p| p.to_path_buf());
                SelectError::Io {
                    source: e.into_io_error().expect("io_error present"),
                    path,
                }
            } else {
                SelectError::Walk {
                    message: e.to_string(),
                }
            }
        })?;

        if entry.file_type().is_dir() || !entry.file_type().is_file() {
            // Dirs are not entries; symlinks/sockets/etc. are non-regular.
            continue;
        }
        let absolute = entry.path().to_path_buf();
        let relative = absolute
            .strip_prefix(&root)
            .map_err(|e| SelectError::Walk {
                message: format!("path outside plugin_dir: {e}"),
            })?
            .to_path_buf();

        // Ancestor-aware match so directory patterns (e.g. `__pycache__/`)
        // exclude files beneath them. Bare `matched` would miss ancestors.
        if matcher
            .matched_path_or_any_parents(&absolute, false)
            .is_ignore()
        {
            continue;
        }

        let is_exec = is_executable(&absolute).map_err(|source| SelectError::Io {
            source,
            path: Some(absolute.clone()),
        })?;
        let normalized = to_normalized(&relative);
        out.push(SelectedFile {
            absolute,
            relative,
            normalized,
            is_exec,
        });
    }

    out.sort_by(|a, b| a.normalized.cmp(&b.normalized));
    Ok(out)
}

/// Builds the gitignore matcher rooted at `root`. An empty pattern list yields
/// an empty matcher (a no-op). Each pattern is added with `add_line`; the first
/// invalid pattern returns [`SelectError::InvalidExcludePattern`] naming it.
fn build_matcher(root: &Path, exclude: &[String]) -> Result<Gitignore, SelectError> {
    if exclude.is_empty() {
        return Ok(Gitignore::empty());
    }
    let mut builder = GitignoreBuilder::new(root);
    for pattern in exclude {
        builder
            .add_line(None, pattern)
            .map_err(|e| SelectError::InvalidExcludePattern {
                pattern: pattern.clone(),
                message: e.to_string(),
            })?;
    }
    builder
        .build()
        .map_err(|e| SelectError::InvalidExcludePattern {
            pattern: exclude.join(", "),
            message: e.to_string(),
        })
}

/// Joins a relative path's normal components with `/` (tar/archive canonical).
fn to_normalized(relative: &Path) -> String {
    relative
        .components()
        .filter_map(|c| match c {
            Component::Normal(os) => Some(os.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(unix)]
fn is_executable(path: &Path) -> std::io::Result<bool> {
    use std::os::unix::fs::PermissionsExt;
    Ok(std::fs::metadata(path)?.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> std::io::Result<bool> {
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(dir: &std::path::Path, rel: &str, body: &str) {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, body).unwrap();
    }

    #[test]
    fn empty_exclude_is_a_no_op_selecting_all_regular_files() {
        let td = tempfile::tempdir().unwrap();
        write(td.path(), "manifest.toml", "x");
        write(td.path(), "__init__.py", "y");
        write(td.path(), "pkg/helper.py", "z");
        let got: Vec<String> = select(td.path(), &[])
            .unwrap()
            .into_iter()
            .map(|f| f.normalized)
            .collect();
        assert_eq!(got, vec!["__init__.py", "manifest.toml", "pkg/helper.py"]);
    }

    #[test]
    fn directory_pattern_excludes_nested_files_at_any_depth() {
        let td = tempfile::tempdir().unwrap();
        write(td.path(), "__init__.py", "y");
        write(td.path(), "__pycache__/a/b/c.pyc", "junk");
        let got: Vec<String> = select(td.path(), &["__pycache__/".to_string()])
            .unwrap()
            .into_iter()
            .map(|f| f.normalized)
            .collect();
        assert!(
            !got.iter().any(|p| p.contains("__pycache__")),
            "got {got:?}"
        );
        assert!(got.contains(&"__init__.py".to_string()));
    }

    #[test]
    fn invalid_pattern_names_the_offender() {
        let td = tempfile::tempdir().unwrap();
        write(td.path(), "__init__.py", "y");
        // The `ignore` 0.4.25 globset is lenient: `a/**b` and unterminated
        // character classes like `[` are both accepted without error. A
        // character-class with an inverted range (`[z-a]`) is reliably
        // rejected at `add_line` time, so the offending `pattern` field is the
        // single bad string verbatim (not the joined list).
        let bad = "[z-a]".to_string();
        let err = select(td.path(), std::slice::from_ref(&bad)).unwrap_err();
        match err {
            SelectError::InvalidExcludePattern { pattern, .. } => assert_eq!(pattern, bad),
            other => panic!("expected InvalidExcludePattern, got {other:?}"),
        }
    }

    #[test]
    fn output_is_normalized_forward_slash_and_sorted() {
        let td = tempfile::tempdir().unwrap();
        write(td.path(), "z.py", "");
        write(td.path(), "a.py", "");
        write(td.path(), "pkg/m.py", "");
        let got: Vec<String> = select(td.path(), &[])
            .unwrap()
            .into_iter()
            .map(|f| f.normalized)
            .collect();
        assert_eq!(got, vec!["a.py", "pkg/m.py", "z.py"]); // sorted by normalized key
        assert!(got.iter().all(|p| !p.contains('\\')));
    }

    #[test]
    fn negation_re_includes_a_file_beneath_an_excluded_dir() {
        // Proves no early dir-pruning + ancestor-aware matching.
        let td = tempfile::tempdir().unwrap();
        write(td.path(), "__init__.py", "y");
        write(td.path(), "__pycache__/keep.pyc", "keep");
        write(td.path(), "__pycache__/drop.pyc", "drop");
        let got: Vec<String> = select(
            td.path(),
            &[
                "__pycache__/".to_string(),
                "!__pycache__/keep.pyc".to_string(),
            ],
        )
        .unwrap()
        .into_iter()
        .map(|f| f.normalized)
        .collect();
        assert!(
            got.contains(&"__pycache__/keep.pyc".to_string()),
            "got {got:?}"
        );
        assert!(
            !got.contains(&"__pycache__/drop.pyc".to_string()),
            "got {got:?}"
        );
    }

    /// Even a path component containing a literal backslash byte (which is a
    /// valid filename byte on Unix) must produce a forward-slash-separated
    /// normalized path. Unix-only: Windows parses `\` as a path separator, so
    /// the same input would split into different components.
    #[cfg(unix)]
    #[test]
    fn backslash_byte_in_component_does_not_leak_into_normalized_path() {
        // Single component whose name contains a literal `\` byte.
        let p = PathBuf::from("sub\\leaf");
        let result = to_normalized(&p);
        assert_eq!(result, "sub\\leaf", "single component preserved verbatim");
        assert!(
            !result.contains('/'),
            "single component must not introduce `/`"
        );

        // Multi-component path with a backslash in one component: the component
        // separator is `/`, component content is preserved byte-for-byte. On
        // Unix, `PathBuf::from_iter` treats each string as a single component
        // and backslashes are ordinary filename bytes, so the middle
        // component's name is literally `sub\leaf` (8 bytes including the
        // backslash). `to_normalized` joins the three components with `/`,
        // yielding the bytes `a/sub\leaf/c`.
        let p: PathBuf = ["a", "sub\\leaf", "c"].iter().collect();
        let result = to_normalized(&p);
        assert_eq!(
            result, "a/sub\\leaf/c",
            "backslash byte inside a component is preserved; `/` only appears as component separator"
        );
    }

    #[test]
    fn anchored_pattern_is_relative_to_plugin_root_not_nested_dirs() {
        // A pattern with an internal separator (e.g. `tests/**`) is anchored to
        // the plugin root, so it excludes `<root>/tests/...` but NOT a `tests/`
        // directory nested deeper. This proves selection is relative to the
        // plugin root, independent of where the matched files sit.
        let td = tempfile::tempdir().unwrap();
        write(td.path(), "__init__.py", "y");
        write(td.path(), "tests/a.py", "root-level tests, excluded");
        write(td.path(), "pkg/tests/b.py", "nested tests, kept");
        let got: Vec<String> = select(td.path(), &["tests/**".to_string()])
            .unwrap()
            .into_iter()
            .map(|f| f.normalized)
            .collect();
        assert!(
            !got.iter().any(|p| p == "tests/a.py"),
            "root tests/ must be excluded: {got:?}"
        );
        assert!(
            got.iter().any(|p| p == "pkg/tests/b.py"),
            "nested pkg/tests/ must be kept (pattern anchored to root): {got:?}"
        );
        assert!(got.iter().any(|p| p == "__init__.py"), "got {got:?}");
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_and_dirs_are_excluded_from_selection() {
        let td = tempfile::tempdir().unwrap();
        write(td.path(), "real.py", "");
        std::fs::create_dir(td.path().join("subdir")).unwrap();
        std::os::unix::fs::symlink(td.path().join("real.py"), td.path().join("link.py")).unwrap();
        let got: Vec<String> = select(td.path(), &[])
            .unwrap()
            .into_iter()
            .map(|f| f.normalized)
            .collect();
        assert_eq!(
            got,
            vec!["real.py"],
            "symlink + dir must be excluded; got {got:?}"
        );
    }
}
