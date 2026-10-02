# Guardrails

- Keep Rust, TypeScript, TSX, and JavaScript source and test files at or below 300 lines; generated files and vendored third-party code are exempt.
- Support macOS, Linux, and Windows. Keep platform differences behind the [platform seams](../../docs/architecture.md#cross-cutting-invariants), and document limitations without an equivalent on another platform.
- Record why platform-specific tests exclude other platforms. For Unix verification from Windows, follow [Building and testing](../../docs/getting-started.md#building-and-testing).
- Write code comments in English.
- Handle or propagate errors. Never report external-call failures or malformed/truncated input as success; mark truncated results explicitly, as required by the [resource and error invariants](../../docs/architecture.md#cross-cutting-invariants).
