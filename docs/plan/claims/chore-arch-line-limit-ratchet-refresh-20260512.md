# chore(arch): refresh checker line-limit ratchet

- **Date**: 2026-05-12
- **Branch**: `chore/arch-line-limit-ratchet-refresh-20260512`
- **PR**: https://github.com/mohsen1/tsz/pull/6018
- **Status**: ready
- **Workstream**: 4 / architecture guardrails

## Intent

Refresh the checker source line-limit exclusion baseline so the architecture
guard unit tests match the current tree. Several files listed as 2000+ LOC
exceptions have since dropped under the limit, while the pinned exclusion
count is stale relative to the current over-limit set. This is a no-behavior
guardrail maintenance PR.

## Files Touched

- `docs/plan/claims/chore-arch-line-limit-ratchet-refresh-20260512.md`
- `scripts/arch/arch_guard.py`
- `scripts/arch/test_arch_guard.py`

## Verification

- `python3 -m py_compile scripts/arch/arch_guard.py scripts/arch/test_arch_guard.py`
- `python3 -m unittest scripts.arch.test_arch_guard`
- `python3 scripts/arch/arch_guard.py`
