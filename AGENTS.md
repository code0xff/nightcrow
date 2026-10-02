# nightcrow

Keep personal tool settings, account/access details, and checkout-specific procedures in untracked `AGENTS.local.md`; read it when present at the checkout root.

The session daemon owns repositories and terminal panes; the TUI and web viewer attach to the same session. Follow the ownership boundaries and shared invariants in [architecture.md](docs/architecture.md).

## Applicable guides

The rules in [`.agents/rules/`](.agents/rules/) always apply. Tool-specific rule directories must symlink to this source rather than copy it.

Read the scoped guide for changes in [docs](docs/AGENTS.md), [src](src/AGENTS.md), [viewer-ui](viewer-ui/AGENTS.md), or [plugins](plugins/AGENTS.md).

## Changes and verification

- Follow [guardrails.md](.agents/rules/guardrails.md) for platform and code constraints.
- Update the relevant tests and documentation when behavior or interfaces change; follow [testing.md](.agents/rules/testing.md).
- Use [Building and testing](docs/getting-started.md#building-and-testing) for local verification and [CI](.github/workflows/ci.yml) for the required gates. Report any applicable checks that were not run.
- Follow [commits.md](.agents/rules/commits.md) for commit and history constraints.

## Pull requests and releases

- Development PRs target `code0xff/nightcrow:dev` and include their purpose and verification results.
- Merge only after CI passes, using merge commits. Do not use rebase or squash merges; synchronize `main` into `dev` with a history-preserving merge as well.
- Release through a separate `dev` to `main` promotion PR; follow [releases.md](.agents/rules/releases.md).
