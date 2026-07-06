# Initialize a Registry

A registry starts with one `index.json` file and one artifact location. The index location is the URL consumers configure; the index file itself stores only `artifacts_url`, the base URL for plugin archives.

Choose the artifact base URL:

| Location | Example `ARTIFACTS_URL` |
|---|---|
| Local filesystem | `file:///var/lib/influxdb3/plugins` |
| GitHub Releases | `https://github.com/ORG/REPO/releases/download/plugin-registry` |
| S3 HTTPS endpoint | `https://BUCKET.s3.REGION.amazonaws.com/plugins` |
| HTTPS server | `https://plugins.example.com/artifacts` |

```bash
ARTIFACTS_URL="https://plugins.example.com/artifacts"
```

Generate the empty index:

```bash
influxdb3-plugin new index --artifacts-url "${ARTIFACTS_URL}" registry-seed
```

This writes `registry-seed/index.json`:

```json
{
  "index_schema_version": "2.0",
  "artifacts_url": "https://plugins.example.com/artifacts",
  "plugins": []
}
```

Upload `registry-seed/index.json` to the registry's index location. Before publishing plugin versions, make sure your release process can:

- replace `index.json` at the index location
- upload archives to `${ARTIFACTS_URL}/{name}-{version}.tar.gz`
- avoid rewriting an archive after its `(name, version)` is published

Use `https://` for shared registries. Use an object store's HTTPS endpoint, not a native URI such as `s3://`. Use `http://` only on trusted internal networks or for local testing. Use `file://` only for local, offline, or appliance-style registries.

Next, publish plugin versions with [`influxdb3-plugin package`](./publish-new-version.md). For the registry model and index schema, see [The Registry](../explanation/registry.md) and [The Registry Index Format](../reference/registry-index.md).

---

Back: [Install the CLI](./install.md) | Next: [Publish a New Plugin Version](./publish-new-version.md)
