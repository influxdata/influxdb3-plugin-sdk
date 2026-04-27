//! Captures the git commit SHA used in the `--version` output.
//!
//! Precedence (first non-empty wins; final fallback is the literal
//! `"unknown"`):
//!
//! 1. `GIT_HASH` env var — CI override; release pipelines bake the
//!    authoritative SHA from CI's checkout.
//! 2. `.cargo_vcs_info.json` at `$CARGO_MANIFEST_DIR` — written by
//!    `cargo publish` into the published `.crate` tarball; recovers
//!    the published commit's SHA without a `.git` directory (the
//!    dominant `cargo install` case).
//! 3. `git rev-parse HEAD` — local-development case.
//! 4. The literal `"unknown"` — uncontrolled rebuilds (Homebrew, Nix,
//!    hand-unpacked tarballs without `cargo install`).
//!
//! The captured SHA is written to `$OUT_DIR/version_fragment.rs` as a
//! Rust string literal so `src/config.rs` can `include!` it inside a
//! `concat!`.
//!
//! ## Why no `cargo::rerun-if-changed=.git/HEAD`
//!
//! Watching `.git/HEAD` would force rebuilds on every commit / branch
//! switch during active development. Release builds get a fresh build
//! anyway (CI checks out the tag from a clean cache, or reads
//! `GIT_HASH`/`.cargo_vcs_info.json` for the authoritative SHA), so
//! per-build SHA precision isn't needed in dev cycles.

use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    let sha = from_env_var()
        .or_else(from_cargo_vcs_info)
        .or_else(from_git_rev_parse)
        .unwrap_or_else(|| "unknown".to_owned());

    // `sha` is constrained to `[a-f0-9]{40}` or `"unknown"` by the
    // `from_*` validators and the hardcoded fallback, so no escape
    // handling is needed when emitting as a Rust string literal.
    let fragment = format!(r#""{sha}""#);

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is always set"));
    fs::write(out_dir.join("version_fragment.rs"), fragment)
        .expect("writing version_fragment.rs to OUT_DIR");

    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=.cargo_vcs_info.json");
    println!("cargo::rerun-if-env-changed=GIT_HASH");
}

fn from_env_var() -> Option<String> {
    let s = env::var("GIT_HASH").ok()?;
    if is_valid_sha(&s) { Some(s) } else { None }
}

/// Reads `.cargo_vcs_info.json` at `$CARGO_MANIFEST_DIR` and extracts
/// `git.sha1`. Cargo writes this file at `cargo publish` time; it ships
/// inside the published `.crate` tarball.
fn from_cargo_vcs_info() -> Option<String> {
    let dir = env::var_os("CARGO_MANIFEST_DIR")?;
    let path = PathBuf::from(&dir).join(".cargo_vcs_info.json");
    let content = fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    let s = v.get("git")?.get("sha1")?.as_str()?;
    if is_valid_sha(s) { Some(s.to_owned()) } else { None }
}

fn from_git_rev_parse() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    if is_valid_sha(&s) { Some(s) } else { None }
}

/// True iff `s` is a 40-character lowercase-hex git SHA. Rejects
/// uppercase, short SHAs, non-hex content, and anything that could
/// inject syntax into the generated `version_fragment.rs` (quotes,
/// backslashes, newlines — none of which are valid hex).
fn is_valid_sha(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
