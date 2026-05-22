# The Manifest Format

A plugin manifest describes one plugin version. It lives at the root of the plugin directory as `manifest.toml`, travels inside the packaged artifact, and is authored by the plugin repository maintainer or plugin author.

Scaffolding a plugin with `influxdb3-plugin new <template>` writes an initial `manifest.toml` alongside the template's source files. Packaging and validation commands read the manifest, validate it, and preserve the author-written source file.

## Minimal Example

```toml
manifest_schema_version = "1.1"

[plugin]
name = "downsampler"
version = "1.2.0"
description = "Downsample data on every WAL write."
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
description = "Downsample data on every WAL write."
triggers = ["process_writes", "process_scheduled_call"]
homepage = "https://influxdata.com"
repository = "https://github.com/influxdata/plugin-downsampler"
documentation = "https://github.com/influxdata/plugin-downsampler/readme.md"

[dependencies]
database_version = ">=3.2.0,<4.0.0"
python = ["requests>=2.31,<3", "pydantic~=2.0"]
```

## Manifest Structure

Every manifest file consists of these fields and sections:

- `manifest_schema_version` - Root-level manifest schema version.
- `[plugin]` - Plugin metadata.
  - `name` - Plugin name.
  - `version` - Plugin version.
  - `description` - One-line description.
  - `triggers` - Trigger types implemented by the plugin.
  - `homepage` - Optional project homepage URL.
  - `repository` - Optional source repository URL.
  - `documentation` - Optional documentation URL.
- `[dependencies]` - Runtime compatibility and Python package requirements.
  - `database_version` - Compatible InfluxDB 3 database version range.
  - `python` - Optional Python package requirements.

Unknown fields are ignored within a supported schema major. Do not use unknown fields for durable custom metadata: a future schema version may define them. The key `dependencies.plugins` is reserved for a future inter-plugin dependency format.

## Top-Level Entries

| Entry | TOML type | Required | Description |
|---|---|---:|---|
| `manifest_schema_version` | string | Yes | Manifest schema version in `<major>.<minor>` form. Parsed before field-level validation. |
| `[plugin]` | table | Yes | Plugin metadata. |
| `[dependencies]` | table | Yes | Runtime compatibility and Python package requirements. |

### `manifest_schema_version`

`manifest_schema_version` must be a root-level string before any table header:

```toml
manifest_schema_version = "1.1"
```

The value uses `<major>.<minor>` form. Consumers accept known major version `1`, including newer minor versions such as `1.2`, and reject unsupported majors instead of guessing.

If `manifest_schema_version` is malformed or uses an unsupported major, parsing stops with that schema-version error before field-level validation.

## The `[plugin]` Section

The `[plugin]` section defines the plugin version.

```toml
[plugin]
name = "downsampler"
version = "1.2.0"
description = "Downsample data on every WAL write."
triggers = ["process_writes"]
```

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

The plugin name is an identifier used to refer to the plugin. It is used in registry entries, search and info output, artifact names, and as the name component of plugin identity.

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

Within one registry, two plugin names that share a canonical form are treated as the same plugin. For example, `foo-bar`, `foo_bar`, and `FOO-BAR` cannot be published as separate plugins in one registry.

### `plugin.version`

The `version` field is formatted according to the SemVer 2.0.0 specification:

```toml
version = "1.2.0"
```

Versions must have three numeric parts: major, minor, and patch. A pre-release part can be added after a dash, for example `1.2.0-rc.1`. Build metadata can be added after a plus, for example `1.2.0+build.7`.

Invalid examples include `1`, `1.2`, and `latest`.

The SDK preserves the full version string. Registry ordering uses SemVer precedence. Plugin versions are immutable once published to a registry; to publish changed plugin contents, bump `plugin.version`.

### `plugin.description`

The `description` field is a short, plain-text blurb about the plugin. Registries display it with the plugin in browse and discovery output. Use plain text, not Markdown.

```toml
description = "Downsample data on every WAL write."
```

Descriptions must be non-empty, single-line strings no longer than 200 characters. Newline (`\n`) and carriage return (`\r`) characters are rejected.

### `plugin.triggers`

`triggers` lists the trigger functions the plugin implements:

```toml
triggers = ["process_writes", "process_scheduled_call"]
```

The array must contain at least one value. Supported trigger values are:

- `process_writes`
- `process_scheduled_call`
- `process_request`

Each value must correspond to a supported trigger entry point in the plugin source. Unknown trigger strings are rejected.

### `plugin.homepage`

The `homepage` field should be a URL to a site that is the home page for the plugin:

```toml
homepage = "https://influxdata.com"
```

Set `homepage` only when the plugin has a dedicated website other than the source repository or API documentation. Do not make `homepage` redundant with `documentation` or `repository`.

When present, the URL must parse and use the `http` or `https` scheme.

### `plugin.repository`

The `repository` field should be a URL to the source repository for the plugin:

```toml
repository = "https://github.com/influxdata/plugin-downsampler"
```

When present, the URL must parse and use the `http` or `https` scheme.

### `plugin.documentation`

The `documentation` field specifies a URL to a website hosting the plugin's documentation:

```toml
documentation = "https://github.com/influxdata/plugin-downsampler/readme.md"
```

When present, the URL must parse and use the `http` or `https` scheme.

## The `[dependencies]` Section

The `[dependencies]` section lists requirements needed to run the plugin and filter compatible database versions.

```toml
[dependencies]
database_version = ">=3.2.0,<4.0.0"
python = ["requests>=2.31,<3", "pydantic~=2.0"]
```

| Field | Type | Required | Description |
|---|---|---:|---|
| `database_version` | string | Yes | SemVer version requirement for compatible InfluxDB 3 database versions. |
| `python` | array of strings | No | PEP 508 Python package requirement strings. Omitted or empty means no Python dependencies. |

### `dependencies.database_version`

The `database_version` field is a Rust `semver` version requirement for compatible InfluxDB 3 database versions:

```toml
database_version = ">=3.2.0,<4.0.0"
```

Use a range that reflects the database versions the plugin supports. The SDK validates the requirement syntax and registry consumers can use it for compatibility filtering.

### `dependencies.python`

The `python` field is an optional array of PEP 508 requirement strings for Python packages the plugin needs at runtime:

```toml
python = ["requests>=2.31,<3", "pydantic~=2.0"]
```

Omitting `python` or setting it to an empty array means the plugin has no declared Python package dependencies. Each entry must parse as a PEP 508 requirement. The SDK preserves the original requirement strings.

## Validation

Manifest parsing has two phases:

1. TOML structure and required fields are parsed.
2. Field-level validation checks names, versions, descriptions, triggers, URLs, dependency ranges, and Python requirements.

Syntax errors, missing required fields, or wrong TOML container shape are reported as root-level TOML parse errors. If `manifest_schema_version` is malformed or uses an unsupported major, parsing stops with that schema-version error. Otherwise, the parser reports all field-level validation errors it can find in one pass.

## Schema Versioning

`manifest_schema_version` uses `<major>.<minor>` form.

Within a supported major version, fields may be added and unknown fields are ignored. Breaking changes require a new major version. Consumers reject unsupported majors instead of guessing.

Back to [Reference](./).

Next: [The Registry Index Format](./registry-index.md).
