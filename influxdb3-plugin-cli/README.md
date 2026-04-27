# influxdb3-plugin-cli

Author-side CLI for the InfluxDB 3 Processing Engine plugin system —
the `influxdb3-plugin` binary plus the embeddable [`PluginConfig`]
type.

This crate is consumed by:

- end-user plugin authors invoking the standalone `influxdb3-plugin`
  binary
- the future phase-2 `influxdb_pro` integration mounting
  [`PluginConfig`] as a variant of the host's top-level command enum
  (Spec 2 § Phase-2 Embedding)

[`PluginConfig`]: ./src/config.rs

## Commands

All commands accept `--output {human, json}`. The default is
auto-detected from stdout's TTY status and the `CI` env var per Spec
2 § S2-14.

### `new <template> [path]`

Scaffold a new plugin or registry from a built-in template. Plugin
templates: `process_writes`, `process_scheduled_call`,
`process_request`. Registry template: `registry`.

```bash
influxdb3-plugin new process_writes ./my-plugin
influxdb3-plugin new registry ./my-registry --artifacts-url https://plugins.example.com/artifacts
```

### `validate [plugin-dir]`

Run the manifest + cross-file checks. Emits a `{ "diagnostics": [...] }`
JSON document on stdout in `--output json` mode regardless of pass /
fail (Spec 2 § S2-15 validator idiom). Optional `--index <path>`
adds the `(name, version)` uniqueness check (Spec 2 § S2-2).

```bash
influxdb3-plugin validate ./my-plugin
influxdb3-plugin validate ./my-plugin --index ./registry/index.json
```

### `package [plugin-dir]`

Validate, archive, hash, and emit a derived index entry. Writes
`<out>/<name>-<version>.tar.gz` and `<out>/index.json`. The input
`--index` is read-only (Spec 2 § S2-11); `--out` must NOT resolve to
the directory containing `--index` (S2-12).

```bash
influxdb3-plugin package ./my-plugin --index ./registry/index.json --out ./build
```

### `yank <name>@<version>`

Toggle the `yanked` flag on an existing index entry. Idempotent per
Spec 2: re-yanking already-yanked (or `--undo`-ing not-yanked) is a
successful no-op with an informational marker.

```bash
influxdb3-plugin yank downsampler@1.2.0 --index ./registry/index.json --out ./build
influxdb3-plugin yank downsampler@1.2.0 --undo --index ./registry/index.json --out ./build
```

### `--version`

Top-level flag (Spec 2 § S2-21). Always emits one line of plain
text regardless of `--output`:

```text
influxdb3-plugin <version>, revision <sha>
```

The format matches the `influxdb3` binary's `build_version_string`
(`{product}, {version}, revision {sha}`), so when the SDK is embedded
as `influxdb3 plugin --version`, the output is visually consistent
with the host's top-level `--version`.

`<sha>` is the 40-character git commit hash from which the binary was
built, sourced from (in precedence) the `GIT_HASH` env var,
`.cargo_vcs_info.json` at the crate root, or `git rev-parse HEAD`. It
degrades to the literal `unknown` only for uncontrolled rebuilds
outside CI and outside `cargo install`.

## Exit codes (Spec 2 § S2-18)

| Code | Meaning |
|------|---------|
| `0`  | Success. |
| `1`  | Runtime failure (validation, I/O, immutability collision, parse error, internal invariant). |
| `2`  | Usage error. clap emits this for unknown flags, missing required args, invalid `--output` values. |

Codes `3` through `255` are reserved for additive future semantic
codes; consumers reading only `0` / non-zero continue to work.

## Embedding contract

`PluginConfig` is a clap-derived, semver-stable type. The phase-2
embedding shape is:

```rust,no_run
use clap::Parser;
use influxdb3_plugin_cli::PluginConfig;

# fn _example(host_argv: Vec<String>) -> anyhow::Result<()> {
let config = PluginConfig::try_parse_from(host_argv)?;
let runtime = tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()?;
runtime.block_on(config.run())?;
# Ok(())
# }
```

Schema-type re-exports route through this crate so phase-2 consumers
depend only on `influxdb3-plugin-cli`, satisfying Spec 2 § S2-10 and
preventing parser drift from a parallel direct dependency on
`influxdb3-plugin-schemas`.

## Stability

Per Spec 2 § Stability, the public API of this crate — `PluginConfig`,
its subcommand enum, `pub async fn run(self) -> anyhow::Result<()>`,
clap attribute surface (arg names, env-var bindings, version
declaration), schema-type re-exports, and the JSON output schema
emitted in `--output json` mode — is covered by semver. Adding fields
to a JSON output schema is a minor bump; renaming, removing,
repurposing, or narrowing the type of an existing field is a major
bump.

The crate is licensed `MIT OR Apache-2.0`. It is currently
unpublished pending the SDK's go-public timing.

## Dependency summary

Runtime: `anyhow`, `clap` (derive + env), `tokio` (current_thread +
macros), `serde`, `serde_json`, `semver`,
`influxdb3-plugin-schemas`, `influxdb3-plugin-sdk`.

Dev / test: `assert_cmd`, `predicates`, `insta`, `rstest`,
`tempfile`, `toml`.

A `build.rs` script captures the full 40-char git commit SHA for the
`--version` output (Spec 2 § S2-21), reading from `GIT_HASH` env when
set, then `.cargo_vcs_info.json` at the crate root (Cargo's
publish-time SHA capture), then `git rev-parse HEAD`. On full
fallback the SHA degrades to the literal `unknown` rather than
failing the build.
