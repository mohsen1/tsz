# diagnostics: drop alias for generic application reducing to literal/primitive

- **Date**: 2026-05-01
- **Branch**: `claude/brave-thompson-UHmVt`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints — fingerprint parity)

## Intent

When a generic type-alias application (e.g. `KeysExtendedBy<M, number>`)
reduces to a literal/primitive (or a union/intersection of those), the
TS2345 diagnostic must show the resolved form (e.g. `'"b"'`) instead of
the unevaluated alias name. tsc preserves the alias only when the result
is structural (object, interface, array, etc.). Before this change, the
call-parameter formatter unconditionally printed the unevaluated
`Application` (`KeysExtendedBy<M, number>`) regardless of what it
reduced to.

Fingerprint-only conformance test fixed: `mappedTypeAsClauses.ts`.

## Files Touched

- `crates/tsz-solver/src/type_queries/data/content_predicates.rs`
  — adds `is_literal_or_primitive_or_compound_of_those` predicate.
- `crates/tsz-solver/src/type_queries/data/tests.rs`
  — 9 unit tests covering literals, primitives, compound unions, and
  rejection of object/array/application shapes.
- `crates/tsz-checker/src/query_boundaries/common.rs`
  — re-exports the predicate through the checker boundary helper.
- `crates/tsz-checker/src/error_reporter/call_errors/display_formatting.rs`
  — `format_call_parameter_type_for_diagnostic` evaluates `Application`
  parameter types via the type environment; uses the resolved form when
  the result is literal/primitive-like, otherwise keeps the alias.
- `crates/tsz-checker/tests/literal_application_alias_display_tests.rs`
  — 5 end-to-end TS2345 tests covering the positive cases (alias drops),
  the negative case (`Pick2` keeps its alias), and a name-independence
  guard (rule fires regardless of mapped iteration variable name).
- `crates/tsz-checker/src/lib.rs` — wires the new test module.

## Verification

- `cargo test -p tsz-solver --lib literal_or_primitive_compound`
  (9/9 new tests pass).
- `cargo test -p tsz-checker --lib literal_application_alias`
  (5/5 new tests pass).
- `cargo test -p tsz-solver --lib` (5576/5576 pass).
- `cargo test -p tsz-checker --lib` (3078/3078 pass, includes new tests).
- `cargo fmt --all --check` clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  clean.
- `./scripts/conformance/conformance.sh run --filter mappedTypeAsClauses`
  (1/1 passes — was previously fingerprint-only failure).
- `./scripts/conformance/conformance.sh run --max 200` (200/200 pass).
- Full conformance suite: no regressions.

## Architecture Notes

- Decision lives in the checker (WHERE), uses a query boundary helper
  for the structural classification (WHAT). No raw `TypeKey` access,
  no pattern-match against printer output, no hardcoded user-chosen
  identifier names — the rule applies regardless of the iteration
  variable name in the mapped-type alias body.
- Negative-case guard test (`ts2345_alias_resolving_to_object_keeps_alias_form`)
  prevents over-eager expansion of `Pick`/`Partial`/etc.
