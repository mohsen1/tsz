# perf(cli): share checker lib contexts

Status: claim
Owner: Codex
Branch: `perf/share-checker-lib-contexts`
Created: 2026-05-01 18:13:36 UTC

## Intent

Workstream 5 large-repo residency: keep checker-facing lib contexts behind a
shared `Arc<Vec<LibContext>>` after `load_checker_libs` so the CLI
`ProjectEnv` setup can install them without deep-cloning the vector.

## Planned Scope

- `crates/tsz-cli/src/driver/check.rs`
- Any direct tests/fixtures that construct `CheckerLibSet`.

## Verification Plan

- `cargo fmt --check`
- `cargo check -p tsz-cli`
- `cargo test -p tsz-cli driver`
- `scripts/bench/perf-hotspots.sh --quick`
