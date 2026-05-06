# [WIP] fix(conformance): align assertion type predicate diagnostics

- **Date**: 2026-05-05
- **Claimed**: 2026-05-05 17:00:18 UTC
- **Branch**: `conformance/assertion-type-predicates1-20260505`
- **PR**: #3127
- **Status**: implementation
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the picked `assertionTypePredicates1.ts` conformance mismatch. The current
fingerprint expects TS1228, TS2775, TS2776, and TS7027, but tsz only emits
TS1228, so the work will identify why assertion-call diagnostics and unreachable
code are missing.

## Pick

```text
path:     TypeScript/tests/cases/conformance/controlFlow/assertionTypePredicates1.ts
category: only-missing
expected: TS1228,TS2775,TS2776,TS7027
actual:   TS1228
missing:  TS2775,TS2776,TS7027
extra:    -
pool:     131
```

## Files Touched

- `crates/tsz-checker/src/types/computation/call/mod.rs`
- `crates/tsz-checker/src/types/computation/call/inner.rs`
- `crates/tsz-checker/src/flow/reachability_checker.rs`
- `crates/tsz-checker/src/types/type_node.rs`
- `crates/tsz-checker/src/state/type_environment/type_node_resolution.rs`
- `crates/tsz-checker/src/tests/assertion_type_predicate_diagnostics_tests.rs`
- `crates/tsz-checker/src/lib.rs`

## Verification

- `cargo fmt --check`
- `cargo check --package tsz-checker`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run --package tsz-checker --lib assertion_type_predicate_diagnostics_tests` — 5 passed
- `cargo nextest run --package tsz-checker --lib` — 3438 passed, 10 skipped
- `./scripts/conformance/conformance.sh run --filter "assertionTypePredicates1" --verbose` — 1/1 passed
- `./scripts/conformance/conformance.sh run --max 200` — 200/200 passed

Full safe-run is still blocked in this worktree: repeated attempts at
`scripts/safe-run.sh ./scripts/conformance/conformance.sh run` and a reduced
`--workers 8` run were terminated by SIGTERM during the dist-fast build before
the conformance tests started. Keep PR #3127 WIP until the full safe-run can
complete.
