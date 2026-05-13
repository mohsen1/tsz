# fix(audit): follow up missed-review threads (#5104, #5655, #5712)

- **Date**: 2026-05-12
- **Branch**: `codex/audit-followup-keyof-namespace-20260512`
- **PR**: #6068
- **Status**: ready
- **Workstream**: review audit follow-ups

## Intent

Close three high-signal missed-review clusters from the last-500-PR audit:

- `#5104` (`keyof` + well-known symbol key identity)
- `#5655` (recovered namespace malformed function-arrow expression-body emit)
- `#5712` (recursive-array fingerprint rewrite over-suppression safeguards)

## Changes

- review comments left on #5104:
  - added `TypeResolver::resolve_well_known_symbol_name(...)` and `TypeEnvironment` mapping storage for canonical well-known symbol key names (`[Symbol.xxx] -> SymbolRef`).
  - registered well-known symbol key mappings from checker computed-property resolution paths in both checker type environments (`type_env` and `type_environment`).
  - updated solver `keyof` key extraction for symbol-named properties to prefer stable symbol identity and fall back to `symbol` when identity is unavailable.
  - added regression coverage proving `keyof { [Symbol.iterator]: number }` preserves assignability with `typeof Symbol.iterator`.

- review comments left on #5655:
  - extracted statement-position expression emission helper `emit_expression_in_statement_position(...)` and reused it from standard expression-statement emission.
  - switched recovered namespace malformed function-arrow body emission to use that helper so object/function-leading expressions are disambiguated like normal expression statements.
  - updated IR printer expression-statement emission to parenthesize object literals.
  - added emitter regressions for malformed namespace arrow object-literal body preservation and IR expression-statement object-literal wrapping.

- review comments left on #5712:
  - require all expected `flat/flat1/flat2` call-site markers to exist before applying recursive-array fingerprint suppression/injection.
  - scope suppression to diagnostics anchored on those call-site lines so unrelated `TS2322` messages with matching text are preserved.
  - keep duplicate-safe reinjection behavior through the existing `push_unique_diagnostic` path.

- audit manifest refresh:
  - current merge resolution keeps `origin/main`'s latest shared audit snapshot files to avoid reverting newer audit retirements.

## Files Touched

- `crates/tsz-solver/src/def/resolver.rs`
- `crates/tsz-solver/src/evaluation/evaluate_rules/keyof.rs`
- `crates/tsz-checker/src/types/queries/core.rs`
- `crates/tsz-checker/src/types/type_node_property_names.rs`
- `crates/tsz-checker/tests/symbol_index_signature_tests.rs`
- `crates/tsz-emitter/src/emitter/statements/core.rs`
- `crates/tsz-emitter/src/emitter/declarations/namespace.rs`
- `crates/tsz-emitter/src/transforms/ir_printer.rs`
- `crates/tsz-emitter/src/emitter/declarations/namespace/tests.rs`
- `crates/tsz-emitter/tests/ir_printer.rs`
- `crates/tsz-checker/src/state/state_checking/source_file.rs`
- `crates/tsz-checker/tests/recursive_type_references_tests.rs`
- `docs/plan/claims/codex-review-audit-keyof-namespace-followup-20260512.md`

## Verification

- `cargo test -p tsz-checker --test symbol_index_signature_tests -- --nocapture`
  - result: `7 passed; 0 failed`
- `cargo test -p tsz-emitter namespace_recovers_malformed_export_function_arrow_object_literal_body -- --nocapture`
  - result: `1 passed; 0 failed`
- `cargo test -p tsz-emitter test_emit_expression_statement_wraps_object_literal -- --nocapture`
  - result: `1 passed; 0 failed`
- `cargo test -p tsz-checker --test recursive_type_references_tests recursive_array_rewrite_ -- --nocapture`
  - result: `3 passed; 0 failed`
- `cargo fmt --all`
  - result: success
