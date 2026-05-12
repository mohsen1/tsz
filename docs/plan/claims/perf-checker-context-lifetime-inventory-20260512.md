# perf(checker): add CheckerContext lifetime inventory guard

- **Date**: 2026-05-12
- **Branch**: `perf/checker-context-lifetime-inventory-20260512`
- **PR**: https://github.com/mohsen1/tsz/pull/6008
- **Status**: ready
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

- `python3 -m py_compile scripts/arch/arch_guard.py scripts/arch/test_arch_guard.py`
- `python3 -m unittest scripts.arch.test_arch_guard.ArchGuardStructFieldCountTests scripts.arch.test_arch_guard.ArchGuardCheckerContextLifetimeManifestTests`
- `python3 scripts/arch/arch_guard.py`
- `python3 scripts/arch/arch_guard.py --checker-context-lifetime-table > /tmp/checker_context_lifetime_table.md && wc -l /tmp/checker_context_lifetime_table.md`
- Known pre-existing ratchet failure: full `python3 -m unittest scripts.arch.test_arch_guard`
  currently fails in `ArchGuardRatchetDirectionTests` because the checker
  line-limit exclusion list has 45 entries against a pinned max of 36, and
  several listed files are already below the 2000-line exclusion threshold.
  The lifetime inventory tests pass.
