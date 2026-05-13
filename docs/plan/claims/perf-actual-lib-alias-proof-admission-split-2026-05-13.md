# Claim: Actual-lib alias proof/admission split

Date: 2026-05-13
Status: stacked behind `perf/actual-lib-alias-proof-result-20260513`

## Claim

Actual-lib alias proof should be independent from the direct-return admission
gate. The helper can prove a bundled-lib alias body, carry its `DefId`, type
parameters, and measured outcome, while the caller still admits only the narrow
decorator metadata aliases.

This is behavior-neutral for checker results. It does not return more aliases
from `direct_actual_lib_symbol_type`, does not change lib delegation cache-hit
behavior, and keeps generic aliases plus `PropertyKey` on fallback.

## Evidence

- `crates/tsz-checker/src/state/type_analysis/cross_file_direct.rs`
  - adds an explicit `is_direct_actual_lib_alias_body_admitted` gate.
  - moves that gate after actual-lib declaration, resolver, `DefinitionStore`,
    body, and type-parameter proof.
  - returns a `DirectActualLibAliasBodyProof` for proven generic aliases with
    `DirectActualLibAliasBodyOutcome::GenericAlias`.
  - returns a proof for proven non-generic but unadmitted aliases with
    `DirectActualLibAliasBodyOutcome::NameNotAdmitted`.
  - keeps `direct_actual_lib_symbol_type` returning `None` unless the proof
    outcome is `Success`.
- Unit coverage asserts:
  - `PropertyKey` has a proven body but remains unadmitted/fallback.
  - `Record` has a proven generic body with two type params but remains
    fallback.
  - the direct proof body and type-parameter arity for `Record`, `Partial`, and
    `Readonly` match the existing child-checker fallback result.

## Validation

- `cargo test -p tsz-checker --lib direct_actual_lib_symbol_type -- --nocapture`
- `cargo test -p tsz-common perf_counters::json_tests -- --nocapture`
- `cargo check -p tsz-checker`
- `cargo fmt --all --check`
- `git diff --check`
