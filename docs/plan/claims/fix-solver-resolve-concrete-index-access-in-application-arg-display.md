# fix(solver): resolve concrete IndexAccess args in Application display

- **Date**: 2026-05-01
- **Branch**: `fix/solver-resolve-concrete-index-access-in-application-arg-display`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance — fingerprint parity)

## Intent

When a generic Application's arg is an `IndexAccess(obj, idx)` whose
`obj` and `idx` are both fully concrete (no type parameters / no infer
placeholders, idx is a literal or literal union), tsc resolves the
indexed access in displayed types — `View<TypeA["bar"]>` is shown as
`View<TypeB>`. The concrete index is just an indirection over the
resolved property type, so leaving it in the display is purely
cosmetic noise.

tsz historically printed the unresolved form because the per-key
mapped-type instantiation leaves the IndexAccess in the constructed
Application's arg list and the type printer rendered it verbatim.

## Files Touched

- `crates/tsz-solver/src/diagnostics/format/mod.rs`
  (helper `resolve_concrete_index_access_for_display` + 1-line callsite
  swap in the Application arg formatter).
- `crates/tsz-checker/src/tests/application_arg_concrete_index_access_display_tests.rs`
  (2 locking unit tests: positive + name-renamed positive cover).
- `crates/tsz-checker/src/lib.rs` (test wiring).

## Verification

- Targeted: `excessPropertyChecksWithNestedIntersections.ts` → **1/1 PASS**
  (was fingerprint-only).
- `cargo nextest run -p tsz-checker -p tsz-solver --lib` → **8668/8668 pass**.
- Smoke conformance:
  - `--filter excessProperty` → 10/11 PASS (1 pre-existing).
  - `--filter indexedAccess` → 13/13 PASS.
  - `--filter mapped` → 54/60 PASS (6 pre-existing).
- Generic / deferred IndexAccess (where obj or idx contains type
  parameters or idx is a non-literal) is preserved — the helper
  short-circuits in those cases.
