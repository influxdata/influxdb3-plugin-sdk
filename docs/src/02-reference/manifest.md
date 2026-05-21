# Manifest Format

A plugin manifest describes one plugin version. It lives at the root of the plugin directory as `manifest.toml`, travels inside the packaged artifact, and is authored by the plugin repository maintainer or plugin author.

The SDK validates manifests before packaging. It does not generate or rewrite them.

## File Format

Manifest files are TOML.

The current manifest schema version is `1.1`. Consumers accept schema major version `1` and reject unsupported majors.

## Minimal Example

```toml
manifest_schema_version = "1.1"

[plugin]
name = "downsampler"
version = "1.2.0"
description = "Notify an HTTP endpoint on every WAL commit."
triggers = ["process_writes"]

[dependencies]
database_version = ">=3.2.0,<4.0.0"
```

## Complete Example

```toml
manifest_schema_version = "1.1"

[plugin]
name = "downsampler"
version = "1.2.0"
description = "Notify an HTTP endpoint on every WAL commit."
triggers = ["process_writes", "process_scheduled_call"]
homepage = "https://influxdata.com"
repository = "https://github.com/influxdata/plugin-downsampler"
documentation = "https://github.com/influxdata/plugin-downsampler/readme.md"

[dependencies]
database_version = ">=3.2.0,<4.0.0"
python = ["requests>=2.31,<3", "pydantic~=2.0"]
```

## Top-Level Fields

| Field | Type | Required | Description |
|---|---|---:|---|
| `manifest_schema_version` | string | Yes | Manifest schema version in `<major>.<minor>` form. Parsed before field-level validation. |
| `plugin` | table | Yes | Plugin metadata. |
| `dependencies` | table | Yes | Runtime compatibility and Python package requirements. |

Unknown fields are ignored within a supported schema major.

## `plugin` Fields

| Field | Type | Required | Description |
|---|---|---:|---|
| `name` | string | Yes | Plugin name. Forms the name component of plugin identity. |
| `version` | string | Yes | Plugin version. Must be valid SemVer 2.0.0. |
| `description` | string | Yes | One-line human-readable description. |
| `triggers` | array of strings | Yes | Trigger types implemented by the plugin. Must be non-empty. |
| `homepage` | string | No | HTTP or HTTPS URL for the plugin or project homepage. |
| `repository` | string | No | HTTP or HTTPS URL for the plugin source repository. |
| `documentation` | string | No | HTTP or HTTPS URL for plugin documentation. |

### `plugin.name`

Names are stored case-preserving, but registry collision checks use a canonical form: lowercase, with `-` replaced by `_`.

Validation rules:

- 1 to 64 ASCII characters.
- Starts with an ASCII letter.
- Remaining characters are ASCII letters, ASCII digits, `_`, or `-`.
- Windows reserved device names are rejected case-insensitively: `con`, `prn`, `aux`, `nul`, `com0` through `com9`, and `lpt0` through `lpt9`.

Examples of valid names:

- `downsampler`
- `my-plugin`
- `MyPlugin`
- `process_writes_v2`

Examples of invalid names:

- `123plugin`
- `my plugin`
- `plugin.example`
- any name containing non-ASCII characters

### `plugin.description`

Descriptions must be non-empty, single-line strings no longer than 200 characters. Newline characters are rejected.

### `plugin.triggers`

The trigger array must contain at least one value. Supported trigger values are:

- `process_writes`
- `process_scheduled_call`
- `process_request`

## `dependencies` Fields

| Field | Type | Required | Description |
|---|---|---:|---|
| `database_version` | string | Yes | SemVer version requirement for compatible InfluxDB 3 database versions. |
| `python` | array of strings | No | PEP 508 Python package requirement strings. Omitted or empty means no Python dependencies. |

`database_version` uses Rust `semver` version requirement syntax, for example `>=3.2.0,<4.0.0`.

Each `python` entry must parse as a PEP 508 requirement, for example `requests>=2.31,<3`.

## Validation

Manifest parsing has two phases:

1. TOML structure and required fields are parsed.
2. Field-level validation checks names, versions, descriptions, triggers, URLs, dependency ranges, and Python requirements.

If `manifest_schema_version` is malformed or uses an unsupported major, parsing stops with that schema-version error. Otherwise, the parser reports all field-level validation errors it can find in one pass.

## Schema Versioning

`manifest_schema_version` uses `<major>.<minor>` form.

Within a supported major version, fields may be added and unknown fields are ignored. Breaking changes require a new major version. Consumers reject unsupported majors instead of guessing.

Back to [Reference](./).

Next: [Index format](./registry-index.md).
