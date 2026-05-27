# InfluxDB 3 Plugin SDK

The InfluxDB 3 Plugin SDK is a CLI tool and set of libraries to help author and manage plugins. **Plugin repository maintainers** use the SDK to publish versioned plugin registries from CI, and **plugin authors** use the SDK to create versioned plugins.

## Why Use The Plugin SDK?

### A Registry Solves Versioning
The most common way to install plugins is to fetch them directly from GitHub using the `gh:` prefix with the `influxdb3` CLI. E.g. `influxdb3 create trigger --path gh:influxdata/downsampler/downsampler.py`. This install path has several problems:

- **No plugin versioning** 
    - Changes to plugin source are automatically forced onto users because `gh:` plugins are fetched from the source's `main` branch.
    - Users have no control over what code is running; they cannot specify a version, cannot pin, or roll back.
    - Plugin authors are unable to update plugins without potentially breaking users.
- **No dependency management** 
    - Plugin authors cannot declare which InfluxDB version their plugin supports.
    - Users cannot know whether a plugin is compatible with their InfluxDB version until after they install it and encounter runtime errors.
    - There is no standardized way to communicate plugin dependencies on third-party libraries. 
- **Multifile plugins are not supported** 
    - Plugin authors cannot create plugins that span multiple files

Using the plugin SDK, plugin repository maintainers can solve these problems by publishing a [plugin registry](./reference/registry.md). This will result in the following benefits for plugin authors and users:

- **Plugins are versioned**
    - Each published plugin version is an immutable artifact with a stable `(registry, name, version)` identity.
    - Plugin authors can publish updates without breaking or forcing changes on existing users.
    - Users can install, pin, compare, and report the exact plugin version they are running.
- **Plugins declare dependencies and compatibility**
    - All dependencies declared for each plugin version. 
    - Consumers can reject incompatible plugin versions before they fail at runtime.
- **Multifile plugins are supported**
    - Plugin authors can split plugin code across multiple files.
- **No breaking changes**
    - Both `gh:` and registry consumers can coexist in the same repository.
- **Minimal effort to set up and maintain**
    - Use a CI workflow to maintain the registry (templates provided to get started).
    - No change to plugin author workflow. 