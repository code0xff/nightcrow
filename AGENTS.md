# nightcrow

Keep personal tool settings, account/access details, and checkout-specific procedures in untracked `AGENTS.local.md`; read it when present at the checkout root.

The session daemon owns repositories and terminal panes; the TUI and web viewer attach to the same session. Follow the ownership boundaries and shared invariants in [architecture.md](docs/architecture.md).

## Applicable guides

The rules in [`.agents/rules/`](.agents/rules/) always apply. Tool-specific rule directories must symlink to this source rather than copy it.

Before changing a path, read its full applicable `AGENTS.md` hierarchy and explicitly read any applicable `AGENTS.local.md`. At each scope, `AGENTS.override.md` replaces that scope's `AGENTS.md`. Read every scoped guide when a change crosses scopes: [docs](docs/AGENTS.md), [src](src/AGENTS.md), [viewer-ui](viewer-ui/AGENTS.md), and [plugins](plugins/AGENTS.md).

## Source organization

Group growing implementation areas by stable responsibility so flat directories do not become the default. Add a subdirectory when it represents a coherent responsibility or ownership boundary; avoid directories that only wrap a lone file without a durable reason. Keep facades stable when splitting their implementation, including the existing visibility of their items, and keep cross-layer design contracts in [architecture.md](docs/architecture.md) rather than duplicating them in local guides.

## Changes and verification

- Follow [guardrails.md](.agents/rules/guardrails.md) for platform and code constraints.
- Update the relevant tests and documentation when behavior or interfaces change; follow [testing.md](.agents/rules/testing.md).
- Use [Building and testing](docs/getting-started.md#building-and-testing) for local verification and [CI](.github/workflows/ci.yml) for the required gates. Report any applicable checks that were not run.
- Follow [commits.md](.agents/rules/commits.md) for commit and history constraints.

## Pull requests and releases

- Development PRs target `code0xff/nightcrow:dev` and include their purpose and verification results.
- Merge only after CI passes, using merge commits. Do not use rebase or squash merges; synchronize `main` into `dev` with a history-preserving merge as well.
- Release through a separate `dev` to `main` promotion PR; follow [releases.md](.agents/rules/releases.md).
