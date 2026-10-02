# Release policy

- Support only `0.1.x`, with matching application package versions. The no-tag bootstrap is `0.1.1`; later official tags advance exactly one patch. Major/minor changes require an explicit maintainer decision and a policy change reviewed by `@code0xff`.
- Change versions with [`prepare-release.mjs`](../../scripts/prepare-release.mjs) and commit the related files together. Follow [releasing.md](../../docs/releasing.md) and the machine-readable [release-policy.json](../../.github/release-policy.json).
- Release policy and automation changes require `@code0xff` review under [CODEOWNERS](../../.github/CODEOWNERS).
- Publish only from official `main` after all platform builds and tests pass. Reject other repositories/branches, an existing tag at another SHA, and incomplete asset sets.
- Resume drafts only by adding missing assets after checking existing digests and sizes. Published assets are immutable.
