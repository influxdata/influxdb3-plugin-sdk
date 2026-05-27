# First Steps with the InfluxDB 3 Plugin SDK

This section provides a quick sense for the `influxdb3-plugin` command line tool. We demonstrate its ability to create a local registry, create a plugin from a template, package the plugin, and view the plugin in the registry's index.

Start by listing available templates with `influxdb3-plugin new list`:

```console
$ influxdb3-plugin new list

Template Name           Short Name
----------------------  ----------------------
Process Writes Plugin   process_writes
Scheduled Call Plugin   process_scheduled_call
Process Request Plugin  process_request
Index                   index
```

A [registry](../reference/registry.md) is a collection of plugins and an `index.json` file. Let's create an index using `influxdb3-plugin new index`:

```console
$ influxdb3-plugin new index registry

Scaffolded index (index template) at registry
  files written:
    index.json
```

This creates an [index](../reference/registry-index.md) file at `registry/index.json` with these contents:

```json
{
  "index_schema_version": "2.0",
  "artifacts_url": "file:///path/to/registry",
  "plugins": []
}
```

As we can see, the index has an `artifacts_url` and an empty collection of `plugins`.

By default the `artifacts_url` points to the absolute path of the local fileystem, but `--artifacts-url` can be used to specify a remote `https://` url.

Next, let's create a Scheduled Call Plugin for our registry using `influxdb3-plugin new process_scheduled_call`:

```console
$ influxdb3-plugin new process_scheduled_call src/hello-world

Scaffolded plugin (process_scheduled_call template) at src/hello-world
  name: hello-world
  files written:
    manifest.toml
    __init__.py
    README.md
```

This is all we need to get started. First, let's check out `manifest.toml`:


```console
manifest_schema_version = "1.1"

[plugin]
name = "hello-world"
version = "0.1.0"
description = "A new scheduled-call plugin."
triggers = ["process_scheduled_call"]

[dependencies]
database_version = ">=3.0.0"
```

The [manifest](../reference/manifest.md) contains all of the metadata needed to package the plugin.

Here's what's in `__init__.py`:

```python
"""Plugin entry point for the `process_scheduled_call` trigger."""


def process_scheduled_call(influxdb3_local, schedule_time, args):
    """Called on each scheduled fire. `schedule_time` is a naive UTC datetime."""
    influxdb3_local.info(f"scheduled call at {schedule_time}")
```

`process_scheduled_call` is a special function that gets called by InfluxDB 3 when the plugin is triggered. 

Now let's package the plugin with `influxdb3-plugin package`:

```console
$ influxdb3-plugin package src/hello-world --index registry/index.json --out build

Packaged hello-world@0.1.0
  artifact: build/hello-world-0.1.0.tar.gz
  index:    build/index.json
  hash:     sha256:5836485b76fad264ac2a13c4d0dc4ba1b067ac800b1c9914b3b2e4644c74c9a5
```

Now if we inspect the newly generated `build/index.json`, we can see that it contains our plugin version's metadata:

```json
{
  "index_schema_version": "2.0",
  "artifacts_url": "file:///Users/rcater/.config/superpowers/worktrees/influxdb3-plugin-sdk/docs/design-spec/docs/superpowers/tmp/registry",
  "plugins": [
    {
      "name": "hello-world",
      "version": "0.1.0",
      "published_at": "2026-05-26T20:03:54Z",
      "description": "A new scheduled-call plugin.",
      "triggers": [
        "process_scheduled_call"
      ],
      "dependencies": {
        "database_version": ">=3.0.0",
        "python": []
      },
      "hash": "sha256:5836485b76fad264ac2a13c4d0dc4ba1b067ac800b1c9914b3b2e4644c74c9a5"
    }
  ]
}
```

Every published plugin version gets it's own entry in the index.

The package and publish steps are separate, so we can publish the plugin by moving the artifact and the index from the `build` directory into the `registry` directory, overwriting the existing index:

```console
$ mv build/index.json registry 
$ mv build/hello-world-0.1.0.tar.gz registry
```

The CLI's `search` command can be used to query the registry index:

```console
$ influxdb3-plugin search --index registry/index.json

hello-world  0.1.0  process_scheduled_call  A new scheduled-call plugin.
```

And `info` can be used to inspect the plugin version's metadata:

```console
$ influxdb3-plugin info --index registry/index.json hello-world

hello-world
A new scheduled-call plugin.
version: 0.1.0
published_at: 2026-05-26T20:03:54Z
triggers: process_scheduled_call
database: >=3.0.0
python: <none>
artifact_url: file:///path/to/registry/hello-world-0.1.0.tar.gz
hash: sha256:5836485b76fad264ac2a13c4d0dc4ba1b067ac800b1c9914b3b2e4644c74c9a5
visibility: visible
```

The displayed `artifact_url` can be used to fetch the plugin artifact for installation in InfluxDB 3.

Finally, let's update our plugin and publish a new version. First, change the plugin's source:

```python
"""Plugin entry point for the `process_scheduled_call` trigger."""


def process_scheduled_call(influxdb3_local, schedule_time, args):
    """Called on each scheduled fire. `schedule_time` is a naive UTC datetime."""
    influxdb3_local.info(f"scheduled call at {schedule_time}")
    influxdb3_local.info("hello world!") # <- updated source
```

Then bump the version in `manifest.toml`:

```console
manifest_schema_version = "1.1"

[plugin]
name = "hello-world"
version = "1.0.0"
description = "A new scheduled-call plugin."
triggers = ["process_scheduled_call"]

[dependencies]
database_version = ">=3.0.0"
```

Now we package and publish the new version:

```console
influxdb3-plugin package src/hello-world --index registry/index.json --out build

Packaged hello-world@1.0.0
  artifact: build/hello-world-1.0.0.tar.gz
  index:    build/index.json
  hash:     sha256:21979050833599eb97b78ecccd13ff9385590a868528ebccd89ed0382ae47383
```

```console
$ mv build/index.json registry 
$ mv build/hello-world-1.0.0.tar.gz registry
```

Our newly created index now has both plugin versions:

```json
{
  "index_schema_version": "2.0",
  "artifacts_url": "file:///Users/rcater/.config/superpowers/worktrees/influxdb3-plugin-sdk/docs/design-spec/docs/superpowers/tmp/registry",
  "plugins": [
    {
      "name": "hello-world",
      "version": "0.1.0",
      "published_at": "2026-05-26T20:03:54Z",
      "description": "A new scheduled-call plugin.",
      "triggers": [
        "process_scheduled_call"
      ],
      "dependencies": {
        "database_version": ">=3.0.0",
        "python": []
      },
      "hash": "sha256:5836485b76fad264ac2a13c4d0dc4ba1b067ac800b1c9914b3b2e4644c74c9a5"
    },
    {
      "name": "hello-world",
      "version": "1.0.0",
      "published_at": "2026-05-27T17:52:05Z",
      "description": "A new scheduled-call plugin.",
      "triggers": [
        "process_scheduled_call"
      ],
      "dependencies": {
        "database_version": ">=3.0.0",
        "python": []
      },
      "hash": "sha256:21979050833599eb97b78ecccd13ff9385590a868528ebccd89ed0382ae47383"
    }
  ]
}
```

Inspecting the registry with `search` and `info` also shows the latest version:

```console
$ influxdb3-plugin search --index registry/index.json

hello-world  1.0.0  process_scheduled_call  A new scheduled-call plugin.
```

```console
$ influxdb3-plugin info --index registry/index.json hello-world

hello-world
A new scheduled-call plugin.
version: 1.0.0
published_at: 2026-05-27T17:52:05Z
triggers: process_scheduled_call
database: >=3.0.0
python: <none>
artifact_url: file:///Users/rcater/.config/superpowers/worktrees/influxdb3-plugin-sdk/docs/design-spec/docs/superpowers/tmp/registry/hello-world-1.0.0.tar.gz
hash: sha256:21979050833599eb97b78ecccd13ff9385590a868528ebccd89ed0382ae47383
visibility: visible
```

To install and run a plugin in InfluxDB 3, see the [official InfluxDB 3 plugin documentation](https://docs.influxdata.com/influxdb3/enterprise/get-started/process/).

Normally, a registry is hosted on a remote server so that plugins can be shared with other users. Additionally, the packaging and publishing steps are typically automated in a CI/CD pipeline. For more details, see:

- [GitHub Releases + GitHub Actions](./recipes/ghreleases--ghactions.md)
- [How publish pipelines vary](./concepts/publish-pipeline.md)
