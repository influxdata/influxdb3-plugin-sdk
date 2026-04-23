//! Composes the parenthesized fragment of the `--version` output per
//! Spec 2 § S2-21:
//!
//! - When git + `date` succeed → `"(<short-sha> <yyyy-mm-dd>)"`
//! - Otherwise → `"(unknown)"`
//!
//! The fragment is written to `$OUT_DIR/version_fragment.rs` as a Rust
//! string literal so `src/config.rs` can `include!` it inside a `concat!`.
//! Doing the (short-sha, date) → text composition here lets clap's
//! `version` attribute take a single `&'static str` const without the
//! library needing build-time logic at runtime.
//!
//! ## Why no `cargo::rerun-if-changed=.git/HEAD`
//!
//! Watching `.git/HEAD` would force rebuilds on every commit / branch
//! switch during active development. Release builds get a fresh build
//! anyway (CI checks out the tag and starts from an empty cache); v1
//! does not need per-build SHA precision in dev cycles.

use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    let sha = run_capture("git", &["rev-parse", "--short=7", "HEAD"]);
    let date = run_capture("date", &["-u", "+%Y-%m-%d"]);

    let fragment = match (sha, date) {
        (Some(s), Some(d)) => format!(r#""({s} {d})""#),
        _ => r#""(unknown)""#.to_owned(),
    };

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is always set"));
    fs::write(out_dir.join("version_fragment.rs"), fragment)
        .expect("writing version_fragment.rs to OUT_DIR");

    println!("cargo::rerun-if-changed=build.rs");
}

/// Runs `cmd` with `args`; returns the trimmed stdout on success, `None`
/// when the command is missing, exits non-zero, or produces non-UTF8
/// output. Used to feed the source-tarball graceful-degradation path.
fn run_capture(cmd: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(cmd).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}
