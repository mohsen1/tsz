# fix(checker): preserve recursive alias name in TS2322 target display

- **Date**: 2026-05-09
- **Branch**: `claude/brave-thompson-OTMCR`
- **PR**: TBD
- **Status**: claim
- **Workstream**: type-display-parity (Tier 1)

## Intent

When the TS2322 assignment target is `Application(Lazy(D), args)` and `D`'s
alias body recursively references `D` (directly or through nested types),
the diagnostic printer used to expand the body with alias names skipped,
producing an unbounded `[42, [42, [42, [..., ...]]]]` cascade. tsc keeps
the alias annotation in this case (e.g. `T2<U>`).

This PR adds a structural query helper
(`query_boundaries::recursive_alias::is_recursive_type_alias_application`)
that detects "alias body reaches its own DefId again" and uses it in
`format_assignment_target_type_for_diagnostic` to short-circuit before the
`raw_tuple_…_without_alias` expansion path. The recursive-alias case now
preserves the user's annotation text via `format_annotation_like_type`,
matching tsc's TS2322 output. Non-recursive generic alias applications
keep their existing display behaviour.

## Files Touched

- `crates/tsz-checker/src/query_boundaries/mod.rs` (register module)
- `crates/tsz-checker/src/query_boundaries/recursive_alias.rs` (new helper)
- `crates/tsz-checker/src/error_reporter/core/diagnostic_source/assignment_formatting.rs` (call site)
- `crates/tsz-checker/src/lib.rs` (test wiring)
- `crates/tsz-checker/tests/recursive_alias_application_target_display_tests.rs` (regression unit tests)

## Verification

- `cargo test -p tsz-checker --lib` — 3789 tests pass, 0 fail.
- `./scripts/conformance/conformance.sh run --filter "inferFromNestedSameShapeTuple" --verbose` — passes.
- Targeted reproductions show `Type 'T1<U>' is not assignable to type 'T2<U>'.`,
  matching tsc.

## Conformance impact

- Conformance: 12537 → 12538 (+1) for `inferFromNestedSameShapeTuple.ts`.
- New failures: 0.
- The non-recursive `MyTuple<X>` simpler case continues to expand to its
  body — that is a separate fingerprint-only failure outside this PR's
  scope (would require adjusting `preserve_tuple_alias_display` more broadly
  and verifying snapshot deltas).
