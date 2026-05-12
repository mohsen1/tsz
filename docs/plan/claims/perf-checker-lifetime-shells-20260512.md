# perf(checker): add WorkerContext and FileSession shells

- **Date**: 2026-05-12
- **Branch**: `perf/checker-lifetime-shells-20260512`
- **PR**: #6022
- **Status**: shipped
- **Workstream**: 5 / T2.1.A (checker lifetime split before pooling)

## Intent

Add the no-behavior lifetime-owned shell types that complete the T2.1.A
architecture surface after the `CheckerContext` lifetime inventory guard.
`ProgramContext` already exists; this PR adds `WorkerContext` and
`FileSession` as explicit homes for later PRs to move worker-reusable scratch
and per-file reset state without changing checker behavior in this slice.

## Files Touched

- `docs/plan/claims/perf-checker-lifetime-shells-20260512.md`
- `crates/tsz-checker/src/context/lifetime_scopes.rs`
- `crates/tsz-checker/src/context/mod.rs`

## Verification

- `cargo fmt --all --check`
- `cargo test -p tsz-checker worker_context_starts_empty_file_session -- --nocapture`
