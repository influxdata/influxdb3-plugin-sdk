# The Registry Index Format

A registry index describes the plugin versions published by one registry. It
- is a single `index.json` file
- is generated and updated by the SDK from validated manifests and packaged artifacts
- contains every published plugin version
- is consumed by tools that browse, resolve, and install plugins. 
- should never be edited manually; edits should only be made by the SDK tooling.

This page specifies the on-disk format of `index.json`: required fields, validation rules, identity, and canonical serialization. It does not specify how the index file is fetched, cached, or authenticated. Transport concerns (URL schemes for the index location, HTTP cache headers, redirect handling, private-registry credentials, missing-file responses) are the responsibility of the registry consumer and are out of scope for this document. Credentials for private registries are supplied via consumer-side registry configuration and applied at fetch time; they are never embedded in the index.

## Minimal Example

```json
{
  "index_schema_version": "2.0",
  "artifacts_url": "https://plugins.example.com/artifacts",
  "plugins": []
}
```

## Complete Example

```json
{
  "index_schema_version": "2.0",
  "artifacts_url": "https://plugins.example.com/artifacts",
  "plugins": [
    {
      "name": "downsampler",
      "version": "1.2.0",
      "published_at": "2026-04-29T18:45:12Z",
      "description": "Notify an HTTP endpoint on every WAL commit.",
      "triggers": ["process_writes", "process_scheduled_call"],
      "homepage": "https://influxdata.com",
      "repository": "https://github.com/influxdata/plugin-downsampler",
      "documentation": "https://github.com/influxdata/plugin-downsampler/readme.md",
      "dependencies": {
        "database_version": ">=3.2.0,<4.0.0",
        "python": ["requests>=2.31,<3", "pydantic~=2.0"]
      },
      "hash": "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
    }
  ]
}
```

## Index Structure

Every index file consists of these fields and sections:

- `index_schema_version` - Root-level index schema version.
- `artifacts_url` - Base URL where flat artifact files are hosted.
- `plugins` - Array of per-version plugin entries.
  - `name` - Plugin name.
  - `version` - Plugin version.
  - `published_at` - Original publication timestamp.
  - `description` - One-line description.
  - `triggers` - Trigger types implemented by the plugin.
  - `homepage` - Optional project homepage URL.
  - `repository` - Optional source repository URL.
  - `documentation` - Optional documentation URL.
  - `dependencies` - Runtime compatibility and Python package requirements.
  - `hash` - SHA-256 hash of the published archive.
  - `yanked` - Optional flag marking a version unavailable for new resolution.

Unknown fields are ignored within a supported schema major. Do not use unknown fields for durable custom metadata: a future schema version may define them. The key `dependencies.plugins` is reserved for a future inter-plugin dependency format.

## Top-Level Entries

| Entry | Type | Required | Description |
|---|---|---:|---|
| `index_schema_version` | string | Yes | Index schema version in `<major>.<minor>` form. Parsed before field-level validation. |
| `artifacts_url` | string | Yes | Base URL where flat artifact files are hosted. |
| `plugins` | array | Yes | Per-version plugin entries. Empty registries use an empty array. |

### `index_schema_version`

`index_schema_version` must be a root-level string:

```json
"index_schema_version": "2.0"
```

The value uses `<major>.<minor>` form. Consumers accept known major version `2`, including newer minor versions such as `2.1`, and reject unsupported majors instead of guessing.

If `index_schema_version` is malformed or uses an unsupported major, parsing stops with that schema-version error before field-level validation.

### `artifacts_url`

`artifacts_url` is the base URL under which artifact files are hosted. Artifacts are addressed with this flat naming convention:

```text
{artifacts_url}/{name}-{version}.tar.gz
```

The artifact URL shape is fixed. There are no templating markers in `artifacts_url` and no per-entry artifact URL override; the path is always `{name}-{version}.tar.gz` directly under the base. Consumers can compute the URL for any entry from `(artifacts_url, name, version)` alone.

Supported schemes:

| Scheme | Use |
|---|---|
| `https://` | Recommended default for public and private registries. |
| `http://` | Local development or trusted internal networks. |
| `file://` | Offline, air-gapped, or appliance-style deployments. |

Unsupported schemes are rejected, including `oci://`, `s3://`, `git://`, `git+https://`, `git+ssh://`, `ftp://`, and `sftp://`.

Use an object store's HTTPS endpoint rather than a native storage URI such as `s3://`.

## Plugin Entries

Each object in `plugins[]` represents one published plugin version.

| Field | Type | Required | Description |
|---|---|---:|---|
| `name` | string | Yes | Plugin name copied from the manifest. |
| `version` | string | Yes | Plugin version copied from the manifest. Must be valid SemVer 2.0.0. |
| `published_at` | string | Yes | Original publication timestamp for this exact version. |
| `description` | string | Yes | One-line description copied from the manifest. |
| `triggers` | array of strings | Yes | Non-empty trigger list copied from the manifest. |
| `homepage` | string | No | HTTP or HTTPS URL copied from the manifest. |
| `repository` | string | No | HTTP or HTTPS URL copied from the manifest. |
| `documentation` | string | No | HTTP or HTTPS URL copied from the manifest. |
| `dependencies` | object | Yes | Dependency metadata copied from the manifest. |
| `hash` | string | Yes | SHA-256 hash of the published archive. |
| `yanked` | boolean | No | Present and `true` when this version is yanked. Absence means false. |

### Relationship to Manifest

All entry fields except `published_at`, `hash`, and `yanked` are copied verbatim from the plugin's `manifest.toml`. The SDK does not transform or normalize manifest values during index generation; canonical lowercase-and-underscore name normalization for collision checks happens during validation, not in the stored value. See [The Manifest Format](./manifest.md) for authoring rules.

### `plugins.name`

`name` is the plugin name copied from the manifest's `plugin.name`:

```json
"name": "downsampler"
```

The name follows the manifest name rule: 1 to 64 ASCII characters, starts with an ASCII letter, remaining characters are ASCII letters, digits, `_`, or `-`. Windows reserved device names (`con`, `prn`, `aux`, `nul`, `com0`-`com9`, `lpt0`-`lpt9`) are rejected case-insensitively. See [Manifest: `plugin.name`](./manifest.md#pluginname) for the canonical definition and examples.

Names are stored case-preserving. Registry collision checks use a canonical form: lowercase, with `-` replaced by `_`. Two different spellings that share a canonical name cannot coexist in one registry, even across versions. For example, `foo-bar` and `foo_bar` cannot both appear.

### `plugins.version`

`version` is the plugin version copied from the manifest's `plugin.version`:

```json
"version": "1.2.0"
```

The value must be valid SemVer 2.0.0. The SDK preserves the full version string, including any pre-release or build metadata.

Version identity uses SemVer precedence, which ignores build metadata. `1.0.0` and `1.0.0+build.7` are the same version for uniqueness and ordering, and only one of them can appear in a registry. To publish changed plugin contents, bump the pre-release or release version, not the build metadata.

### `plugins.published_at`

`published_at` records the original publication time for this exact version. It uses the Cargo registry `pubtime` shape:

```json
"published_at": "2026-04-29T18:45:12Z"
```

The timestamp must be UTC, use uppercase `T` and `Z`, include seconds precision, and represent a real calendar time. Offsets, fractional seconds, lowercase `z`, leap seconds, and non-UTC forms are rejected.

`published_at` is set on first publish and preserved verbatim when an entry is yanked or unyanked.

### `plugins.description`

`description` is the one-line description copied from the manifest's `plugin.description`:

```json
"description": "Downsample data on every WAL write."
```

The description must be non-empty, single-line, and no longer than 200 characters. Newline (`\n`) and carriage return (`\r`) characters are rejected. Descriptions are stored in Unicode NFC form by canonical serialization.

### `plugins.triggers`

`triggers` lists the trigger functions the plugin implements, copied from the manifest's `plugin.triggers`:

```json
"triggers": ["process_writes", "process_scheduled_call"]
```

The array must contain at least one value. Supported trigger values are:

- `process_writes`
- `process_scheduled_call`
- `process_request`

Unknown trigger strings are rejected.

### `plugins.homepage`

`homepage` is an optional URL to the plugin's home page, copied from the manifest's `plugin.homepage`:

```json
"homepage": "https://influxdata.com"
```

When present, the URL must parse and use the `http` or `https` scheme.

### `plugins.repository`

`repository` is an optional URL to the plugin's source repository, copied from the manifest's `plugin.repository`:

```json
"repository": "https://github.com/influxdata/plugin-downsampler"
```

When present, the URL must parse and use the `http` or `https` scheme.

### `plugins.documentation`

`documentation` is an optional URL to the plugin's documentation, copied from the manifest's `plugin.documentation`:

```json
"documentation": "https://github.com/influxdata/plugin-downsampler/readme.md"
```

When present, the URL must parse and use the `http` or `https` scheme.

### `plugins.dependencies`

`dependencies` is the dependency metadata copied from the manifest's `[dependencies]` table. It has the same shape:

```json
"dependencies": {
  "database_version": ">=3.2.0,<4.0.0",
  "python": ["requests>=2.31,<3", "pydantic~=2.0"]
}
```

| Field | Type | Required | Description |
|---|---|---:|---|
| `database_version` | string | Yes | SemVer version requirement for compatible InfluxDB 3 database versions. |
| `python` | array of strings | No | PEP 508 Python package requirement strings. Omitted or empty means no Python dependencies. |

`database_version` parses as a Rust `semver` version requirement. Each `python` entry parses as a PEP 508 requirement. The SDK preserves the original requirement strings.

There is no field for plugin-to-plugin dependencies. The key `dependencies.plugins` is reserved in the manifest for a future inter-plugin dependency format and is correspondingly absent from the index.

### `plugins.hash`

`hash` is the SHA-256 hash of the published archive. It uses this canonical form:

```json
"hash": "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
```

The literal prefix `sha256:` is followed by exactly 64 lowercase hexadecimal characters. The hash is calculated over the archive bytes and is verified by consumers before extraction.

### `plugins.yanked`

`yanked` marks a version as unavailable for new resolution without deleting the entry or the artifact:

```json
"yanked": true
```

Existing lockfiles can still resolve the exact artifact. To yank a version, the SDK writes `yanked: true`; to unyank, it removes the field. Absence of the field means the version is not yanked. `yanked` is the only entry field that can change after publication.

### Identity And Uniqueness

Within one index, `(name, version)` must be unique.

Version identity uses SemVer precedence, which ignores build metadata. Canonical name form (lowercase, with `-` replaced by `_`) is checked across the registry, so `foo-bar`, `foo_bar`, and `FOO-BAR` cannot be published as separate plugins in one registry.

Global registry identity is outside the index. Consumers identify a registry entry by `(index_url, name, version)`, where `index_url` is the URL configured by the registry consumer.

### Immutability

Once an entry is added to a registry, its fields are immutable. The SDK rejects any attempt to insert a second entry with the same `(name, version)`, so a published version's `description`, `triggers`, `dependencies`, `hash`, URL fields, and `published_at` cannot be changed in place. To publish changed plugin contents, bump `plugin.version` in the manifest and publish a new entry.

The only field that can change after publication is `yanked`. Yanking and unyanking flip that field on the existing entry; all other fields, including `published_at`, are preserved verbatim.

## Validation

Index parsing has two phases:

1. JSON structure and required fields are parsed.
2. Field-level validation checks names, versions, descriptions, triggers, URLs, dependency ranges, Python requirements, timestamps, and hashes against the rules defined in each field's section above.

Syntax errors, missing required fields, or wrong JSON container shape are reported as root-level JSON parse errors. If `index_schema_version` is malformed or uses an unsupported major, parsing stops with that schema-version error. Otherwise, the parser reports all field-level validation errors it can find in one pass, including duplicate entries and canonical-name collisions.

## Schema Versioning

`index_schema_version` uses `<major>.<minor>` form.

Within a supported major version, fields may be added and unknown fields are ignored. Breaking changes require a new major version. Consumers reject unsupported majors instead of guessing.

Indexes using schema `1.x` must be backfilled with a required `published_at` field on every `plugins[]` entry before they can be parsed by schema `2.0` consumers.

## Canonical Serialization

The SDK writes index JSON in canonical form:

- Field ordering matches the schema order shown above.
- `plugins[]` is sorted by `name` ascending, then `version` ascending by SemVer precedence.
- Pretty-printed JSON uses two-space indentation.
- The file ends with a trailing newline.
- Description strings are normalized to Unicode NFC.
- Optional fields are omitted when absent.
- `yanked` is omitted when false.

---

Back: [The Manifest Format](./manifest.md) | Next: [Templates](../templates/README.md)
