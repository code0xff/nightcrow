# Releasing

nightcrow uses a fork-to-upstream release flow. `origin` is the fork used for PR heads, `upstream/dev` is the development line, and `upstream/main` is the official release line for `code0xff/nightcrow`.

## Prepare a patch

The release series is deliberately fixed at `0.1.x`. The tool reads [`.github/release-policy.json`](../.github/release-policy.json), checks the root package, recovery plugin, Cargo lock entries, and viewer package/lock versions, and prints a dry-run by default:

```bash
node scripts/prepare-release.mjs
node scripts/prepare-release.mjs --json
```

On a clean branch, use `--execute` to update all six version entries. The first no-tag release is `0.1.1`; after that, the next version is exactly one patch above the highest official `v0.1.*` tag. An explicit `--version` is accepted only when it matches that calculated value.

```bash
node scripts/prepare-release.mjs --execute
git add Cargo.toml Cargo.lock plugins/nightcrow-recovery/Cargo.toml viewer-ui/package.json viewer-ui/package-lock.json
git commit -m "chore: prepare release v0.1.2"
git push origin release/v0.1.2
```

Open that branch as a PR from the fork to `code0xff/nightcrow:dev`. Do not edit versions by hand, skip a patch, or raise major/minor without an explicit maintainer decision. The policy and release workflow are code-owned by `@code0xff` through [`.github/CODEOWNERS`](../.github/CODEOWNERS).

## Promote and publish

After the preparation PR is merged into `upstream/dev` and its checks are green, open a separate promotion PR from `dev` to `main`. The release workflow runs only when a commit reaches `main` in the official repository; a push to a fork never publishes anything.

The workflow verifies the package policy, runs the workspace tests on each release runner, and builds these exact assets:

| Platform | Asset |
| --- | --- |
| Linux x86_64 | `nightcrow-x86_64-unknown-linux-gnu` |
| Windows x86_64 | `nightcrow-x86_64-pc-windows-msvc.exe` |
| macOS x86_64 | `nightcrow-x86_64-apple-darwin` |
| macOS arm64 | `nightcrow-aarch64-apple-darwin` |

Only after all builds and tests pass does it create `v0.1.x`, generate `SHA256SUMS`, and publish the GitHub Release. The tag must point at the exact `main` commit. The workflow is serialized and safe to retry: an existing tag must point at the same SHA and an existing Release must have exactly the four assets plus `SHA256SUMS`; anything else fails closed.

## Updating an installation

The no-argument updater installs the latest stable binary from the official GitHub Release. It does not install from a mutable branch:

```bash
nightcrow update
nightcrow update --version 0.1.1  # install a prior stable release
```

Development and advanced source installs remain explicit:

```bash
nightcrow update --path .       # local checkout; cargo install --path
nightcrow update --git URL      # named source repository; cargo install --git
```

Restart the daemon after an update so the running session uses the new binary. Release binaries are the supported user path; local/source installs are for development and diagnosis.

## Repository settings

The upstream administrator should protect `dev` and `main`, require the CI checks and code-owner review, disallow force-push and deletion, and protect `v0.1.*` tags from update/deletion. Enable immutable Releases when available. The workflow itself still checks repository identity, branch, version series, exact tag progression, SHA, and asset names, so a copied workflow cannot publish from the fork.
