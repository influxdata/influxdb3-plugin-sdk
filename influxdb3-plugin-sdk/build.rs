//! Resolves `INFLUXDB3_PLUGIN_SDK_KNOWN_LATEST_DB` at build time.
//!
//! The value becomes the version string `scaffold.rs` prepends `>=` to when
//! populating a scaffolded manifest's `dependencies.database_version`.
//!
//! - When the env var is set, validate that `>={value}` parses via
//!   `semver::VersionReq`. Fail the build on invalid input — callers who
//!   supplied a value must have supplied a working one; no silent fallback.
//! - When the env var is unset or empty, fall back to `3.0.0` and emit a
//!   `cargo::warning` so developers see that the scaffolded default is the
//!   permissive floor, not a release-pinned value. Also set the
//!   `sdk_known_db_is_fallback` cfg flag so tests can gate on the fallback
//!   branch having fired at build time (rather than guessing from
//!   process env at test time, which can disagree with the bake).

const FALLBACK: &str = "3.0.0";
const ENV_VAR: &str = "INFLUXDB3_PLUGIN_SDK_KNOWN_LATEST_DB";

fn main() {
    let (raw, is_fallback) = match std::env::var(ENV_VAR) {
        Ok(v) if !v.is_empty() => (v, false),
        _ => (FALLBACK.to_owned(), true),
    };

    let probe = format!(">={raw}");
    if let Err(e) = semver::VersionReq::parse(&probe) {
        panic!("{ENV_VAR}={raw:?} does not produce a valid SemVer range {probe:?}: {e}");
    }

    println!("cargo::rustc-env={ENV_VAR}={raw}");
    println!("cargo::rerun-if-env-changed={ENV_VAR}");
    println!("cargo::rustc-check-cfg=cfg(sdk_known_db_is_fallback)");

    if is_fallback {
        println!("cargo::rustc-cfg=sdk_known_db_is_fallback");
        println!(
            "cargo::warning={ENV_VAR} not set; scaffold default will be `>={FALLBACK}`. \
             Release builds should supply a pinned version."
        );
    }
}
