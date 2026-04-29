# fix(solver): two-multi-overload union with no compat sigs emits TS2349 (not TS2684)

- **Date**: 2026-04-29
- **Branch**: `fix/conformance-roadmap-1777416042`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

`resolve_union_call` (`crates/tsz-solver/src/operations/core/call_resolution.rs`)
has two branches for unions of callable types: one for unions where exactly one
member has multiple overloads, and one for ≥2 members with multiple overloads.

The single-multi-overload branch correctly returns `NotCallable` (→ TS2349)
when no compatible signature pair exists across overloads. The two-multi-overload
branch ran a different path: it set `force_not_callable_with_this_mismatch` +
`force_union_this_type` to the intersection of all members' `this` types, then
the deferred-this-error block converted the result to `ThisTypeMismatch`
(→ TS2684) whenever the actual `this` failed the (impossible) intersection.

This violates tsc parity. tsc's `getUnionSignatures` emits TS2349 when no
compatible signatures exist — regardless of whether the actual `this` would
have satisfied the intersection. Fix mirrors the single-multi-overload arm:
when no compat sigs and all multi-overload members are non-generic, return
`NotCallable` directly, skipping the deferred-this path.

Affects `conformance/types/union/unionTypeCallSignatures6.ts`:
`x1.f3();` (with `f3: F3 | F4` where F3 has `(this:A)`/`(this:B)` and F4 has
`(this:C)`/`(this:D)`) now emits TS2349 instead of TS2684, matching tsc.

A residual extra TS2349 fingerprint at `x1.f2();` (`f2: F1 | F4`, single +
multi-overload, no compat sigs) remains — that path enters the
`has_multi_overload_members == 1` arm and tsc allows the call there for
reasons not yet pinned down (likely a `getUnionSignatures` quirk where a
single-member's lone signature still passes through). Tracking as a follow-up.

## Files Touched

- `crates/tsz-solver/src/operations/core/call_resolution.rs` (~15 LOC: simplify the no-compat branch in `resolve_union_call`).
- `crates/tsz-checker/src/tests/call_architecture_tests.rs` (~70 LOC: 2 new regression tests).

## Verification

- `cargo nextest run -p tsz-solver --lib` (5,545 pass)
- `cargo nextest run -p tsz-checker --lib` (2,962 pass — incl. 2 new locks)
- `./scripts/conformance/conformance.sh run --filter "unionTypeCallSignatures6" --verbose` — fingerprint mismatches dropped from 3 → 1; all `missing-fingerprints` cleared.
- Full conformance net delta TBD (running).
