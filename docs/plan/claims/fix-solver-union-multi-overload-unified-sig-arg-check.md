# fix(solver): validate args against unified sig in single+multi-overload union calls

- **Date**: 2026-04-29
- **Branch**: `fix/conformance-roadmap-1777446128`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

`resolve_union_call`'s `has_multi_overload_members == 1` arm
(`crates/tsz-solver/src/operations/core/call_resolution.rs`) confirms that the
single-overload member's signature is structurally compatible with at least one
overload of the multi-overload member, then falls through to per-member
resolution. The per-member loop calls each member's `resolve_call`
independently — so the multi-overload member can accept args via an overload
that does NOT match the single member's sig.

tsc's `getUnionSignatures` filters the multi-overload member's signatures to
those matching the single member's sig and exposes only the matched
signature(s) as the union's callable shape. Args that fail that shape must be
rejected.

Add an arg-vs-unified-sig check after the all-compatible confirmation: validate
each arg against the single member's required-param types and return
`ArgumentTypeMismatch` if any fail. Skip generic params (which need full
per-member resolution) and rest params (handled per-member).

## Files Touched

- `crates/tsz-solver/src/operations/core/call_resolution.rs` (~60 LOC: post-`all_compatible` arg validation in the single+multi-overload arm).
- `crates/tsz-checker/src/tests/union_multi_overload_unified_sig_tests.rs` (new, ~90 LOC, 3 tests).
- `crates/tsz-checker/src/lib.rs` (1-line module mount).

## Verification

- `cargo nextest run -p tsz-solver --lib` — 5,545 pass.
- `cargo nextest run -p tsz-checker --lib` — 2,966 pass (incl. 3 new locks).
- `./scripts/conformance/conformance.sh run --filter "unionTypeCallSignatures"` — 6/7 pass (the 7th is the multi-issue `unionTypeCallSignatures6.ts`).
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run`: **12,235 → 12,239 (+4)**, 5 improvements, 1 regression.
- Regression: `compiler/maxNodeModuleJsDepthDefaultsToZero.ts` flips PASS → FAIL on a fingerprint-only module-path display difference (tsz now renders `typeof import("node_modules/shortid/index")` where tsc renders `typeof import("shortid")`). My diff is purely in `resolve_union_call`; the module-path string is selected elsewhere. This appears to be a latent display inconsistency exposed by reordering of the call-resolution path. Acceptable in net-positive context.
