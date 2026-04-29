# fix(checker): anchor TS2769 at argument for overloaded functions (not just unions)

- **Date**: 2026-04-29
- **Branch**: `fix/conformance-roadmap-1777455997`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance — fingerprint-parity Tier 1)

## Intent

`error_no_overload_matches_at` (`crates/tsz-checker/src/error_reporter/call_errors/error_emission.rs`) had a single rule for the "non-identical failures on a shared object-literal argument" case: anchor TS2769 at the callee. This was correct for UNION-OF-CALLABLES (e.g. `var v: F1 | F2; v({...})`) but wrong for plain OVERLOADED FUNCTIONS (e.g. `function fn(a:{x}); function fn(a:{y}); fn({z,a})`) — tsc anchors at the argument in the latter case.

Distinguish the two: only apply the callee-anchor short-circuit when the call target is a Union type. For plain overloaded functions, fall through to the argument anchor.

## Files Touched

- `crates/tsz-checker/src/error_reporter/call_errors/error_emission.rs` (~22 LOC: add `callee_is_union` check, gate the short-circuit on it).
- `crates/tsz-checker/src/tests/overload_anchor_at_argument_tests.rs` (new, 2 regression tests).
- `crates/tsz-checker/src/lib.rs` (1-line module mount).

## Verification

- Targeted: `./scripts/conformance/conformance.sh run --filter "excessPropertiesInOverloads"` — flips FAIL → PASS.
- Lib tests: 2 new pass; 2 pre-existing main-side failures remain unrelated to this PR (`test_solver_imports_go_through_query_boundaries` for `import_members_tests.rs`, and `inferred_generic_call_suppresses_ts2345_when_other_argument_is_error`).
- Full conformance: **12,235 → 12,243 (+8)**, 23 improvements / 15 regressions. Mixed with baseline drift; see PR body for details.
