# fix(checker): drill object-literal elaboration through paren/comma/assignment wrappers

- **Date**: 2026-04-26
- **Branch**: `fix/checker-elaborate-object-literal-through-comma-paren`
- **PR**: #1474
- **Status**: ready
- **Workstream**: 1 (conformance fingerprint parity)

## Intent

`var x: Foo = (void 0, { a: q = { b: ({ c: { d: 42 } }) } })` was emitting an
outer-anchored TS2322 with a one-level type display instead of drilling to
`d: 42` and reporting the deepest leaf mismatch. tsc's `elaborateElementwise`
walks past `(expr)`, comma `(a, b)`, and `lhs = rhs` wrappers when looking
for an inner object literal to recurse into; tsz only handled the literal
case. This PR teaches both the var-init entry point and the per-property
recursion to peel those wrappers, so per-property TS2322 anchors land on the
deepest mismatching leaf identifier — matching the tsc fingerprint.

Conformance: +1 (`slightlyIndirectedDeepObjectLiteralElaborations.ts`); no
regressions in `objectLiteral`, `excessProperty`, `Elaboration`, or
`destructuring` filters (verified against origin/main baseline).

## Files Touched

- `crates/tsz-checker/src/state/variable_checking/core.rs` (caller drops
  the `OBJECT_LITERAL_EXPRESSION` precondition; the helper now decides)
- `crates/tsz-checker/src/error_reporter/call_errors/elaboration.rs`
  - `try_elaborate_object_literal_properties_for_var_init` peels paren and
    comma wrappers internally
  - per-property kind-check at line ~1465 peels paren, comma, and assignment
    `=` wrappers before deciding whether to recurse via
    `try_elaborate_assignment_source_error`
- `crates/tsz-checker/tests/elaboration_wrapper_init_tests.rs` (new)
- `crates/tsz-checker/Cargo.toml` (registers the new test crate target)

## Verification

- `cargo nextest run -p tsz-checker` (5428 tests pass, 34 skipped)
- `cargo nextest run -p tsz-checker --test elaboration_wrapper_init_tests`
  (3 new regression tests pass)
- `./scripts/conformance/conformance.sh run --filter slightlyIndirected*`
  (1/1 pass; was 0/1 fingerprint-only on origin/main)
- `./scripts/conformance/conformance.sh run --filter Elaboration`
  (8/10 pass; was 7/10 on origin/main — +1 net, no regressions)
- `./scripts/conformance/conformance.sh run --filter objectLiteral` and
  `--filter excessProperty` and `--filter destructuring` (no deltas vs
  origin/main)
