# Testing

- Update contract tests before changing interfaces, including failure paths and boundary conditions.
- Keep Rust unit tests in sibling `*_tests.rs` files or adjacent `tests/` directories, crate public-API integration tests in root `tests/`, and TS/TSX tests in sibling `*.test.ts(x)` files.
- Fix flaky tests or temporarily skip them with a linked issue; do not suppress unrelated failures.
