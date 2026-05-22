# Index Format

A registry index describes the plugin versions published by one registry. It is a JSON file named `index.json`.

The SDK generates and updates indexes from validated manifests and packaged artifacts. Hand-editing an index is unsupported.

## File Format

Index files are JSON.

The current index schema version is `2.0`. Consumers accept schema major version `2` and reject unsupported majors.

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

## Top-Level Fields

| Field | Type | Required | Description |
|---|---|---:|---|
| `index_schema_version` | string | Yes | Index schema version in `<major>.<minor>` form. Parsed before field-level validation. |
| `artifacts_url` | string | Yes | Base URL where flat artifact files are hosted. |
| `plugins` | array | Yes | Per-version plugin entries. Empty registries use an empty array. |

Unknown fields are ignored within a supported schema major.

## `artifacts_url`

Artifacts are addressed with this flat naming convention:

```text
{artifacts_url}/{name}-{version}.tar.gz
```

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

### Identity And Uniqueness

Within one index, `(name, version)` must be unique.

Names are also checked by canonical form: lowercase, with `-` replaced by `_`. A registry cannot contain two different spellings with the same canonical name, even across versions. For example, `foo-bar` and `foo_bar` cannot both appear in one registry.

Global registry identity is outside the index. Consumers identify a registry entry by `(index_url, name, version)`, where `index_url` is the URL configured by the registry consumer.

### `published_at`

`published_at` uses Cargo registry `pubtime` shape:

```text
YYYY-MM-DDTHH:MM:SSZ
```

The timestamp must be UTC, use uppercase `T` and `Z`, include seconds precision, and represent a real calendar time. Offsets, fractional seconds, lowercase `z`, leap seconds, and non-UTC forms are rejected.

The value records original publication time and is preserved when an entry is yanked or unyanked.

### `dependencies`

The dependency object has the same shape as the manifest's `[dependencies]` table:

| Field | Type | Required | Description |
|---|---|---:|---|
| `database_version` | string | Yes | SemVer version requirement for compatible InfluxDB 3 database versions. |
| `python` | array of strings | No | PEP 508 Python package requirement strings. Omitted or empty means no Python dependencies. |

### `hash`

Hashes use this canonical form:

```text
sha256:<64 lowercase hex characters>
```

The hash is calculated over the archive bytes and is verified before extraction.

### `yanked`

Yanking marks a version as unavailable for new resolution without deleting the entry or the artifact. Existing lockfiles can still resolve the exact artifact. To yank a version, the SDK writes `yanked: true`; to unyank, it removes the field or writes false in memory and omits it during canonical serialization.

## Validation

Index-entry validation mirrors manifest validation:

- `name` follows the manifest name rule.
- `version` is valid SemVer 2.0.0.
- `description` is non-empty, single-line, and no longer than 200 characters.
- `triggers` is non-empty and contains only supported trigger values.
- URL fields use `http` or `https` when present.
- `dependencies.database_version` parses as a SemVer range.
- `dependencies.python` entries parse as PEP 508 requirements.
- `published_at` uses strict UTC seconds format.
- `hash` uses canonical SHA-256 form.

If `index_schema_version` is malformed or uses an unsupported major, parsing stops with that schema-version error. Otherwise, the parser reports all field-level validation errors it can find in one pass, including duplicate entries and canonical-name collisions.

## Canonical Serialization

The SDK writes index JSON in canonical form:

- Field ordering matches the schema order shown above.
- `plugins[]` is sorted by `name` ascending, then `version` ascending by SemVer precedence.
- Pretty-printed JSON uses two-space indentation.
- The file ends with a trailing newline.
- Description strings are normalized to Unicode NFC.
- Optional fields are omitted when absent.
- `yanked` is omitted when false.

## Schema Versioning

`index_schema_version` uses `<major>.<minor>` form.

Within a supported major version, fields may be added and unknown fields are ignored. Breaking changes require a new major version. Consumers reject unsupported majors instead of guessing.

Indexes using schema `1.x` must be backfilled with a required `published_at` field on every `plugins[]` entry before they can be parsed by schema `2.0` consumers.

Back to [Reference](./).

Next: [Templates overview](../templates/).
