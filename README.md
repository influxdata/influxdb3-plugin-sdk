# influxdb3-plugin-sdk

Rust workspace for the InfluxDB 3 plugin SDK. Provides tooling for authoring,
validating, packaging, and publishing plugins to InfluxDB 3's Processing Engine.

## Crates

- **`influxdb3-plugin-schemas`** — Canonical schema types (`Manifest`, `Index`,
  `IndexEntry`, `PluginId`). Published semver-stable; consumed by the SDK and
  by the future database runtime.
- **`influxdb3-plugin-sdk`** — Author-side packaging library (`scaffold`,
  `validate`, `package`, `yank`, `mutate_index`). Internal; consumed through
  the CLI crate's public API.
- **`influxdb3-plugin-cli`** — The `influxdb3-plugin` binary plus the embeddable
  `PluginConfig` type. Published semver-stable for future embedding into
  `influxdb_pro`.

## Development

Requires Rust 1.94 (pinned via `rust-toolchain.toml`).

```shell
cargo check --workspace        # verify all crates compile
cargo nextest run --workspace  # run tests (install: cargo install cargo-nextest)
cargo deny check               # audit dependency graph
cargo clippy --workspace -- -D warnings
```

See `Processing Engine - Plugin Version Management.md` (external design doc)
for the full specification of the plugin lifecycle.
