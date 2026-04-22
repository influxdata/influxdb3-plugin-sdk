# influxdb3-plugin-schemas

Schema types for InfluxDB 3 plugin manifests and registry indexes.

This crate is consumed by:
- [`influxdb3-plugin-sdk`](../influxdb3-plugin-sdk/) — author-side packaging
- [`influxdb3-plugin-cli`](../influxdb3-plugin-cli/) — the `influxdb3-plugin` binary
- the InfluxDB 3 Processing Engine runtime — for install-time manifest parsing

## Overview

The crate exposes three core types plus their supporting newtypes:

- `Manifest` — parsed `manifest.toml` with `PluginMetadata` and `Dependencies`
- `Index` / `IndexEntry` — parsed `index.json` with canonical serialization
- `PluginId` — the `(source, name, version)` identity tuple

All parsing is fail-fast; multi-error collection is handled by
`influxdb3-plugin-sdk`.

## Stability

Per the plugin SDK design, this crate targets a semver-stable public API.
Schema formats evolve independently via `manifest_schema_version` and
`index_schema_version` fields; this crate exposes those version types as
first-class.

The crate is currently unpublished pending the project's license decision.
The stability commitment applies to the types defined here and will be
anchored at first publish.
