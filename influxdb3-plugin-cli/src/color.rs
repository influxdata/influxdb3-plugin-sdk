//! Color decision per Spec 2 § S2-17.
//!
//! [`decide_color`] is the single decision point each renderer consults
//! before emitting ANSI escape sequences.
//!
//! # Absolute rule
//!
//! In [`OutputMode::Json`](crate::output::OutputMode::Json) on
//! [`Stream::Stdout`], color is **never** emitted regardless of any env var.
//! JSON on stdout must be byte-stable and parseable; ANSI escapes break
//! `jq` and every other JSON consumer.

use crate::output::{Env, OutputMode};

/// One of the two output streams the SDK writes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // consumed by per-stream renderers in D30-D33
pub(crate) enum Stream {
    Stdout,
    Stderr,
}

/// Returns whether ANSI color should be rendered on `stream`.
///
/// Rules, evaluated in order:
///
/// 1. **Absolute rule** — `mode == Json && stream == Stdout` → always `false`.
/// 2. `NO_COLOR` set to any non-empty value → `false` on every stream
///    ([no-color.org](https://no-color.org/) convention; overrides
///    `FORCE_COLOR`).
/// 3. `TERM == "dumb"` → `false` on every stream.
/// 4. `FORCE_COLOR` set to any non-empty value → `true` on every stream
///    regardless of `is_terminal`.
/// 5. Otherwise → `is_terminal`.
///
/// `is_terminal` is the per-stream isatty result; the caller resolves it
/// because [`Env`] only exposes `stdout_is_terminal`.
#[allow(dead_code)] // wired into renderers in D30-D33
pub(crate) fn decide_color(
    stream: Stream,
    mode: OutputMode,
    env: &dyn Env,
    is_terminal: bool,
) -> bool {
    // Absolute rule: JSON on stdout must be byte-stable.
    if mode == OutputMode::Json && stream == Stream::Stdout {
        return false;
    }
    if env.var("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        return false;
    }
    if matches!(env.var("TERM").as_deref(), Some("dumb")) {
        return false;
    }
    if env.var("FORCE_COLOR").is_some_and(|v| !v.is_empty()) {
        return true;
    }
    is_terminal
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use std::collections::HashMap;

    #[derive(Debug, Default)]
    struct FakeEnv {
        vars: HashMap<String, String>,
    }

    impl FakeEnv {
        fn new() -> Self {
            Self::default()
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
            // `decide_color` does not consult `stdout_is_terminal` — it takes
            // `is_terminal` as a per-stream argument. Returning `false` here
            // documents the intent.
            false
        }
    }

    /// Absolute rule: JSON mode + stdout → no color, regardless of any env
    /// or isatty signal. Locks the byte-stability invariant for piped JSON.
    #[rstest]
    #[case(true, FakeEnv::new())]
    #[case(false, FakeEnv::new())]
    #[case(true, FakeEnv::new().with("FORCE_COLOR", "1"))]
    #[case(false, FakeEnv::new().with("FORCE_COLOR", "1"))]
    fn json_stdout_never_emits_color(#[case] is_terminal: bool, #[case] env: FakeEnv) {
        assert!(!decide_color(
            Stream::Stdout,
            OutputMode::Json,
            &env,
            is_terminal
        ));
    }

    /// JSON mode permits color on stderr — the absolute rule is stdout-only.
    #[test]
    fn json_stderr_follows_normal_rules() {
        let env = FakeEnv::new();
        assert!(decide_color(Stream::Stderr, OutputMode::Json, &env, true));
    }

    /// `NO_COLOR` (any non-empty value) disables color on every stream and
    /// overrides `FORCE_COLOR`.
    #[rstest]
    #[case(Stream::Stdout, OutputMode::Human)]
    #[case(Stream::Stderr, OutputMode::Human)]
    #[case(Stream::Stdout, OutputMode::Json)]
    #[case(Stream::Stderr, OutputMode::Json)]
    fn no_color_disables_everywhere(#[case] stream: Stream, #[case] mode: OutputMode) {
        let env = FakeEnv::new()
            .with("NO_COLOR", "1")
            .with("FORCE_COLOR", "1");
        assert!(!decide_color(stream, mode, &env, true));
    }

    /// `NO_COLOR=""` (empty) does NOT disable — only non-empty values do.
    #[test]
    fn no_color_empty_value_does_not_disable() {
        let env = FakeEnv::new().with("NO_COLOR", "");
        assert!(decide_color(Stream::Stderr, OutputMode::Human, &env, true));
    }

    #[test]
    fn term_dumb_disables() {
        let env = FakeEnv::new().with("TERM", "dumb");
        assert!(!decide_color(Stream::Stderr, OutputMode::Human, &env, true));
    }

    /// `FORCE_COLOR` enables color on every stream regardless of isatty —
    /// but does NOT bypass the JSON-stdout absolute rule (covered above).
    #[rstest]
    #[case(Stream::Stdout, OutputMode::Human, false)]
    #[case(Stream::Stderr, OutputMode::Human, false)]
    #[case(Stream::Stderr, OutputMode::Json, false)]
    fn force_color_enables_when_not_blocked(
        #[case] stream: Stream,
        #[case] mode: OutputMode,
        #[case] is_terminal: bool,
    ) {
        let env = FakeEnv::new().with("FORCE_COLOR", "1");
        assert!(decide_color(stream, mode, &env, is_terminal));
    }

    /// Default path: `is_terminal` decides when no env override fires.
    #[rstest]
    #[case(Stream::Stdout, OutputMode::Human, true, true)]
    #[case(Stream::Stdout, OutputMode::Human, false, false)]
    #[case(Stream::Stderr, OutputMode::Human, true, true)]
    #[case(Stream::Stderr, OutputMode::Human, false, false)]
    #[case(Stream::Stderr, OutputMode::Json, true, true)]
    #[case(Stream::Stderr, OutputMode::Json, false, false)]
    fn isatty_decides_in_default_case(
        #[case] stream: Stream,
        #[case] mode: OutputMode,
        #[case] is_terminal: bool,
        #[case] expected: bool,
    ) {
        let env = FakeEnv::new();
        assert_eq!(decide_color(stream, mode, &env, is_terminal), expected);
    }
}
