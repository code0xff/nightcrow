# Release policy

- `origin` is the fork and PR head; `upstream/dev` is the development line; `upstream/main` is the only release line.
- The only supported public series is `0.1.x`. Every application package version must agree, and each official tag must advance the patch by exactly one.
- The only no-tag bootstrap is `0.1.1`. Major and minor changes require an explicit maintainer decision and a policy change reviewed by `@code0xff`.
- Run `node scripts/prepare-release.mjs` for a dry-run. Use `--execute` only on a clean release branch, then review and commit all six version-file changes together.
- A release preparation PR targets `upstream/dev`. A separate promotion PR targets `upstream/main`; a push to the official `code0xff/nightcrow` `main` creates the tag and Release only after all platform builds and tests pass.
- The release workflow must never publish from a fork, a non-`main` branch, a reused tag pointing at another SHA, or an incomplete asset set. A draft may be resumed only by adding missing files after digest/size checks; published assets are immutable.

The operational runbook is [docs/releasing.md](../../docs/releasing.md). The machine-readable policy is [.github/release-policy.json](../../.github/release-policy.json).
