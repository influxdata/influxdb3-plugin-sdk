//! ANSI palette for human-mode renderers.
//!
//! [`Palette::for_stream`] consults [`crate::color::decide_color`]; when
//! color is off, every style field is a no-op `Style::new()` so callers
//! can write `palette.error.render()` unconditionally and get plain text
//! on monochrome environments.

use crate::color::{Stream, decide_color};
use crate::output::{Env, OutputMode};
use anstyle::{AnsiColor, Color, Effects, Style};

/// Styles used by the human renderers.
#[derive(Debug, Clone, Copy, Default)]
pub struct Palette {
    /// Red + bold. Used for `"validation failed: ..."` header lines and
    /// data-tool failure summaries.
    pub error: Style,
    /// Green + bold. Used for `"validation passed: ..."` and
    /// `"Packaged X@Y"` success headers.
    pub success: Style,
    /// Yellow. Used for `yanked=true` / `yanked=false` indicators and
    /// `"already in desired state"` informational notices.
    pub warn: Style,
    /// Dim. Used for per-line ordinal prefixes `"  1."`, etc.
    pub dim: Style,
    /// Cyan. Used for diagnostic variant tags `"[SchemaReported]"` and
    /// field paths `"plugin.name"`.
    pub tag: Style,
}

impl Palette {
    /// Builds a palette for `stream` given `mode`, `env`, and the stream's
    /// own `is_terminal` status. When `decide_color` says "no color,"
    /// returns the default (all-noop) palette.
    pub(crate) fn for_stream(
        stream: Stream,
        mode: OutputMode,
        env: &dyn Env,
        is_terminal: bool,
    ) -> Self {
        if !decide_color(stream, mode, env, is_terminal) {
            return Self::default();
        }
        Self {
            error: Style::new()
                .fg_color(Some(Color::Ansi(AnsiColor::Red)))
                .effects(Effects::BOLD),
            success: Style::new()
                .fg_color(Some(Color::Ansi(AnsiColor::Green)))
                .effects(Effects::BOLD),
            warn: Style::new().fg_color(Some(Color::Ansi(AnsiColor::Yellow))),
            dim: Style::new().effects(Effects::DIMMED),
            tag: Style::new().fg_color(Some(Color::Ansi(AnsiColor::Cyan))),
        }
    }
}

/// Builds the human-mode error palette for stderr, respecting the real
/// environment's NO_COLOR / FORCE_COLOR / isatty rules.
pub fn stderr_error_palette() -> Palette {
    use crate::output::RealEnv;
    use std::io::IsTerminal;
    Palette::for_stream(
        Stream::Stderr,
        OutputMode::Human,
        &RealEnv,
        std::io::stderr().is_terminal(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Debug, Default)]
    struct FakeEnv {
        vars: HashMap<String, String>,
    }
    impl FakeEnv {
        fn with(mut self, k: &str, v: &str) -> Self {
            self.vars.insert(k.into(), v.into());
            self
        }
    }
    impl Env for FakeEnv {
        fn var(&self, name: &str) -> Option<String> {
            self.vars.get(name).cloned()
        }
        fn stdout_is_terminal(&self) -> bool {
            true
        }
        fn stderr_is_terminal(&self) -> bool {
            true
        }
    }

    /// Absolute rule: JSON + stdout → every style field is a no-op.
    #[test]
    fn json_stdout_yields_empty_palette() {
        let env = FakeEnv::default().with("FORCE_COLOR", "1");
        let p = Palette::for_stream(Stream::Stdout, OutputMode::Json, &env, true);
        assert_eq!(p.error, Style::new());
        assert_eq!(p.success, Style::new());
        assert_eq!(p.warn, Style::new());
        assert_eq!(p.dim, Style::new());
        assert_eq!(p.tag, Style::new());
    }

    /// Default is a no-op palette (convenience for monochrome paths).
    #[test]
    fn default_palette_is_all_noop() {
        let p = Palette::default();
        assert_eq!(p.error, Style::new());
    }

    /// Human-mode TTY produces a non-empty palette.
    #[test]
    fn human_tty_palette_is_populated() {
        let env = FakeEnv::default();
        let p = Palette::for_stream(Stream::Stderr, OutputMode::Human, &env, true);
        assert_ne!(p.error, Style::new());
        assert_ne!(p.success, Style::new());
    }

    /// `NO_COLOR` collapses back to empty.
    #[test]
    fn no_color_collapses() {
        let env = FakeEnv::default().with("NO_COLOR", "1");
        let p = Palette::for_stream(Stream::Stderr, OutputMode::Human, &env, true);
        assert_eq!(p.error, Style::new());
    }
}
