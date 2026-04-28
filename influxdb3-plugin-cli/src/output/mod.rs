//! Output-mode plumbing.
//!
//! - [`OutputMode`] — the `--output {human,json}` value type.
//! - [`Env`] — dependency-injectable env reader used by
//!   [`resolve_output_mode`]; tests pass fakes, the binary uses [`RealEnv`].
//! - [`resolve_output_mode`] — auto-detection precedence.
//!
//! Per-command rendering lives in [`human`] and [`json`].

use std::io::IsTerminal;

/// Output mode for a command. Selected by `--output <mode>` or, when the
/// flag is omitted, by [`resolve_output_mode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "lowercase")]
pub(crate) enum OutputMode {
    /// Colorized, human-readable rendering. Default on TTY.
    Human,
    /// Machine-readable JSON on stdout. Default in CI / when stdout is not a
    /// terminal.
    Json,
}

/// Dependency-injectable env reader for [`resolve_output_mode`] and
/// [`crate::color::decide_color`].
///
/// Tests pass fake implementations so they can exercise every row of the
/// precedence tables without mutating the process env (parallel-test safe).
pub(crate) trait Env {
    /// Returns the value of `name` in the environment, or `None` if unset.
    fn var(&self, name: &str) -> Option<String>;
    /// Returns whether stdout is attached to a terminal.
    fn stdout_is_terminal(&self) -> bool;
}

/// Stdlib-backed [`Env`] impl used by the binary.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct RealEnv;

impl Env for RealEnv {
    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
    fn stdout_is_terminal(&self) -> bool {
        std::io::stdout().is_terminal()
    }
}

/// Resolves the effective [`OutputMode`]:
///
/// 1. Explicit `--output <mode>` always wins.
/// 2. `!isatty(stdout)` → [`OutputMode::Json`].
/// 3. `CI` env var equal to `"true"` or `"1"` → [`OutputMode::Json`].
/// 4. Otherwise → [`OutputMode::Human`].
///
/// Detection deliberately consults only `IsTerminal` and the `CI` variable.
/// Platform-specific CI markers (`GITHUB_ACTIONS`, `GITLAB_CI`,
/// `JENKINS_URL`, `BUILDKITE`, `CIRCLECI`) are **never** read — per-platform
/// allowlists rot, and `CI=true` is the modern convention every runner sets.
pub(crate) fn resolve_output_mode(explicit: Option<OutputMode>, env: &dyn Env) -> OutputMode {
    if let Some(m) = explicit {
        return m;
    }
    if !env.stdout_is_terminal() {
        return OutputMode::Json;
    }
    if matches!(env.var("CI").as_deref(), Some("true" | "1")) {
        return OutputMode::Json;
    }
    OutputMode::Human
}

pub(crate) mod error_mapping;
pub(crate) mod human;
pub(crate) mod json;

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use std::collections::HashMap;

    #[derive(Debug, Default)]
    struct FakeEnv {
        vars: HashMap<String, String>,
        is_terminal: bool,
    }

    impl FakeEnv {
        fn new(is_terminal: bool) -> Self {
            Self {
                vars: HashMap::new(),
                is_terminal,
            }
        }
        fn with(mut self, name: &str, value: &str) -> Self {
            self.vars.insert(name.to_owned(), value.to_owned());
            self
        }
    }

    impl Env for FakeEnv {
        fn var(&self, name: &str) -> Option<String> {
            self.vars.get(name).cloned()
        }
        fn stdout_is_terminal(&self) -> bool {
            self.is_terminal
        }
    }

    #[rstest]
    #[case(OutputMode::Human, true, None)]
    #[case(OutputMode::Human, false, Some("true"))]
    #[case(OutputMode::Json, true, None)]
    #[case(OutputMode::Json, false, None)]
    fn explicit_overrides_everything(
        #[case] explicit: OutputMode,
        #[case] is_terminal: bool,
        #[case] ci: Option<&str>,
    ) {
        let mut env = FakeEnv::new(is_terminal);
        if let Some(v) = ci {
            env = env.with("CI", v);
        }
        assert_eq!(resolve_output_mode(Some(explicit), &env), explicit);
    }

    #[rstest]
    #[case(None)]
    #[case(Some("true"))]
    #[case(Some("false"))]
    #[case(Some("1"))]
    #[case(Some("0"))]
    fn not_a_tty_is_json(#[case] ci: Option<&str>) {
        let mut env = FakeEnv::new(false);
        if let Some(v) = ci {
            env = env.with("CI", v);
        }
        assert_eq!(resolve_output_mode(None, &env), OutputMode::Json);
    }

    #[rstest]
    #[case("true", OutputMode::Json)]
    #[case("1", OutputMode::Json)]
    #[case("false", OutputMode::Human)]
    #[case("0", OutputMode::Human)]
    #[case("", OutputMode::Human)]
    fn ci_var_truthy_forces_json_on_tty(#[case] value: &str, #[case] expected: OutputMode) {
        let env = FakeEnv::new(true).with("CI", value);
        assert_eq!(resolve_output_mode(None, &env), expected);
    }

    #[test]
    fn tty_with_no_ci_is_human() {
        let env = FakeEnv::new(true);
        assert_eq!(resolve_output_mode(None, &env), OutputMode::Human);
    }

    // Per-runner CI markers must not affect mode detection — only `CI=true|1`
    // counts. Locks the contract against drift toward a per-platform allowlist.
    #[rstest]
    #[case("GITHUB_ACTIONS", "true")]
    #[case("GITLAB_CI", "true")]
    #[case("JENKINS_URL", "https://jenkins.example/")]
    #[case("BUILDKITE", "true")]
    #[case("CIRCLECI", "true")]
    fn platform_ci_markers_are_ignored(#[case] var: &str, #[case] value: &str) {
        let env = FakeEnv::new(true).with(var, value);
        assert_eq!(
            resolve_output_mode(None, &env),
            OutputMode::Human,
            "{var}={value} alone must not force json mode (only CI=true|1 does)"
        );
    }
}
