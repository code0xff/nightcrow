# Commit and history rules

- Each commit must pass the applicable build and tests independently. The [push hook](../../.githooks/pre-push) normally verifies only the tip, not every intermediate commit.
- Commit interface definitions and their contract tests together. Include related documentation in the same commit or the immediately following commit.
- Direct commits to `dev` are limited to simple documentation or configuration changes.
