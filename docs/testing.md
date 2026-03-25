# Testing

MIT licensed open source project by AlphaOne LLC.

## Standard validation

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
./scripts/smoke.sh
```

## Coverage

This project uses `cargo-llvm-cov` for line coverage enforcement.

Recommended setup:

```bash
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov
```

Run coverage:

```bash
./scripts/coverage.sh
```

Default policy:

- minimum line coverage: `85%`
- output HTML report: `target/llvm-cov/html/index.html`
- output LCOV report: `target/llvm-cov/lcov.info`

You can override the local threshold:

```bash
CODEX_MEMORY_MIN_LINE_COVERAGE=90 ./scripts/coverage.sh
```

## Coverage intent

The test suite is intended to cover:

- storage and retrieval behavior
- exact tag filtering
- expiry and pruning behavior
- prompt bundle generation
- TOON escaping behavior
- API end-to-end flow
- CLI query construction
