# Testing

MIT licensed open source project by AlphaOne LLC.

## Standard validation

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
./scripts/test.sh
./scripts/smoke.sh
./scripts/localhost-check.sh
```

`./scripts/test.sh` runs the Rust test suite.

`./scripts/smoke.sh` verifies the daemon and CLI at a basic level.

`./scripts/localhost-check.sh` runs a fuller localhost flow:

- starts a temporary daemon
- adds project-scoped memory
- ingests a transcript file
- runs search
- builds a prompt bundle
- checks stats

## Test code locations

The repository includes explicit automated test code:

- [tests/cli_flow.rs](../tests/cli_flow.rs): end-to-end CLI and daemon integration tests
- [src/api.rs](../src/api.rs): API flow and transcript-ingest tests
- [src/storage.rs](../src/storage.rs): storage, lifecycle, retrieval, project isolation, and prompt-packing tests
- [src/service.rs](../src/service.rs): daemon startup and healthcheck test
- [src/model.rs](../src/model.rs): request/model default behavior tests
- [src/ingest.rs](../src/ingest.rs): transcript extraction and query expansion tests
- [src/toon.rs](../src/toon.rs): TOON encoding tests
- [src/config.rs](../src/config.rs): config loading and directory creation tests

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
- transcript ingestion and automatic capture
- exact tag filtering
- expiry and pruning behavior
- lifecycle operations: capture, update, reinforce, archive, delete
- project and task scoping isolation
- prompt bundle generation
- TOON escaping behavior
- API end-to-end flow
- CLI query construction
- localhost daemon execution paths
