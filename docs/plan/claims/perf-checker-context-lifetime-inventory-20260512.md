# perf(checker): add CheckerContext lifetime inventory guard

- **Date**: 2026-05-12
- **Branch**: `perf/checker-context-lifetime-inventory-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 5 / T2.1.A (checker lifetime split before pooling)

## Intent

Add the first generated inventory guard for `CheckerContext` field lifetime
classification. This is a no-behavior architecture/perf prerequisite for the
large-repo lifetime split: every field must have an explicit lifetime class
before later PRs move state into `ProgramContext`, worker-owned scratch, or
file-session reset boundaries.

## Files Touched

- `docs/plan/claims/perf-checker-context-lifetime-inventory-20260512.md`
- `crates/tsz-checker/src/context/checker_context_lifetimes.toml`
- `scripts/arch/arch_guard.py`
- `scripts/arch/test_arch_guard.py`

## Verification

- Pending
