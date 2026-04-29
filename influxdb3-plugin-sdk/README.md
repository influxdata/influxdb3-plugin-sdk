# influxdb3-plugin-sdk

Author-side packaging library for InfluxDB 3 plugins.

Provides the library functions that [`influxdb3-plugin-cli`](../influxdb3-plugin-cli/)
wraps in user-facing subcommands.

## Public modules

- **`scaffold`** — generate a plugin directory or index directory from a
  built-in template (`process_writes`, `process_scheduled_call`,
  `process_request`, `index`).
- **`validate`** — structural + cross-file checks against a plugin directory:
  manifest well-formedness, required-file presence, and (via
  `tree-sitter-python`) top-level sync-def implementation of every declared
  trigger.
- **`archive`** — canonical tar.gz construction per Spec 2 Reproducibility.
  Byte-deterministic across machines given identical inputs.
- **`hash`** — SHA-256 of archive bytes in the canonical
  `sha256:<64 lowercase hex chars>` form.
- **`mutate_index`** — add, yank, and unyank entries in an existing index.
  Enforces Spec 1 S1-4 / Spec 2 S2-2 immutability on add.
- **`package`** — composes validate → archive → hash → mutate_index into a
  single pipeline.

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
