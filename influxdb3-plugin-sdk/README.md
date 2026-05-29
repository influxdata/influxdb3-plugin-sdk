# influxdb3-plugin-sdk

Author-side packaging library for InfluxDB 3 plugins.

Provides the library functions that [`influxdb3-plugin-cli`](../influxdb3-plugin-cli/)
wraps in user-facing subcommands.

## Public modules

- **`scaffold`** — generate a plugin directory or index directory from a
  built-in template (`process_writes`, `process_scheduled_call`,
  `process_request`, `index`).
- **`validate`** — the filesystem + Python-parser mechanism for validating a
  plugin directory. `plugin_dir` walks the directory, reads the entry point,
  and feeds the results into the pure contract in
  `influxdb3_plugin_schemas::validate`, returning a `ValidatedPlugin`
  (parsed manifest + classified `EntryPoint`) on success or a
  `ValidationFailure` on error. Supports multi-file plugins (with `__init__.py`)
  and single-file plugins (a sole `.py` file at the top level). The reference
  `tree-sitter-python` extractor `extract_top_level_defs` is exposed publicly
  so other consumers (e.g. the runtime) can reuse it; it is drift-checked
  against `schemas::validate::TOP_LEVEL_DEF_CORPUS`.
- **`archive`** — canonical tar.gz construction per Spec 2 Reproducibility.
  Byte-deterministic across machines given identical inputs.
- **`hash`** — SHA-256 of archive bytes in the canonical
  `sha256:<64 lowercase hex chars>` form.
- **`mutate_index`** — add, yank, and unyank entries in an existing index.
  Enforces Spec 1 S1-4 / Spec 2 S2-2 immutability on add and preserves
  original publication timestamps when yanking or unyanking.
- **`package`** — composes validate → archive → hash → mutate_index into a
  single pipeline, assigning current UTC `published_at` to newly packaged
  plugin versions.

## Stability

**Internal crate.** Per the plugin SDK's Spec 2 Stability policy, this
crate has no semver commitment — consumers go through
`influxdb3-plugin-cli`'s public API. Refactoring freedom in the `sdk`
crate is the goal; the stable boundary is `cli`.

## Dependencies

- `influxdb3-plugin-schemas` — canonical schema types (re-exported via
  function signatures).
- `tree-sitter` + `tree-sitter-python` — static Python analysis at author
  time. Linked as C code inside the binary; no runtime `python3`
  dependency.
- `tar` + `flate2` (with `rust_backend` / `miniz_oxide` for deterministic
  gzip bytes) — archive construction.
- `sha2` — artifact hashing.
- `walkdir` — plugin directory traversal.
