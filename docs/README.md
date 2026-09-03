# nightcrow documentation

Start with the [top-level README](../README.md) for the five-minute install and first session. Use [Getting started](getting-started.md) for the complete install, run, update, and development workflow. Maintainers should use [Releasing](releasing.md) for the fork-to-upstream release flow.

## User guides

| Guide | Scope |
| --- | --- |
| [Projects](projects.md) | Repository tabs and per-project terminal limits |
| [Views](views.md) | Status, commit log, tree, notices, and repository picker |
| [Keyboard and mouse](keybindings.md) | Leader commands, navigation, terminal input, and mouse routing |
| [Session state](session-state.md) | Recent-activity indicator and files written between runs |
| [Web viewer](web-viewer.md) | Browser access, cloning, mobile layout, and security |
| [Configuration](configuration.md) | `~/.nightcrow/config.toml`, defaults, validation, and reload scope |
| [Plugins](plugins.md) | Plugin installation, opt-in, and bundled recovery plugin |
| [Releasing](releasing.md) | Patch-only versioning, fork promotion, and official binary Releases |

Each guide is authoritative for its surface. Cross-links point back here or to the guide that owns a shared rule; design rationale and module boundaries remain in [Architecture](architecture.md), and historical decisions remain in [Design decisions](decisions.md).

## Development references

- [Getting started → Building and testing](getting-started.md#building-and-testing) contains the repository verification gates.
- [Architecture](architecture.md) documents system boundaries and implementation responsibilities.
