# fix(audit): follow up missed-review threads (#5104, #5655)

- **Date**: 2026-05-12
- **Branch**: `codex/audit-followup-parser-20260512`
- **PR**: TBD
- **Status**: ready
- **Workstream**: review audit follow-ups

## Intent

Close two high-signal missed-review clusters from the last-500-PR audit:

- `#5104` (`keyof` + well-known symbol key identity)
- `#5655` (recovered namespace malformed function-arrow expression-body emit)

## Changes

- review comments left on #5104:
  - added `TypeResolver::resolve_well_known_symbol_name(...)` and
    `TypeEnvironment` mapping storage for canonical well-known symbol key names
    (`[Symbol.xxx] -> SymbolRef`).
  - registered well-known symbol key mappings from checker computed-property
    resolution paths in both checker type environments (`type_env` and
    `type_environment`).
  - updated solver `keyof` key extraction for symbol-named properties:
    - resolve `__unique_<id>` as before,
    - also resolve canonical `[Symbol.xxx]` via resolver mapping,
    - and when identity is unavailable, fall back to `symbol` (not
      string-literal key) to avoid incorrect string-key `keyof` surfaces.
  - added regression coverage proving
    `keyof { [Symbol.iterator]: number }` preserves assignability with
    `typeof Symbol.iterator` (no TS2322).

- review comments left on #5655:
  - extracted statement-position expression emission helper
    `emit_expression_in_statement_position(...)` and reused it from standard
    expression-statement emission.
  - switched recovered namespace malformed function-arrow body emission to use
    that helper so object/function-leading expressions are disambiguated exactly
    like normal expression statements.
  - updated IR printer expression-statement emission to parenthesize object
    literals (in addition to function expressions).
  - added emitter regressions for malformed namespace arrow object-literal body
    preservation and IR expression-statement object-literal wrapping.

- audit manifest refresh:
  - added PRs `5104` and `5655` to `excluded_followed_up_prs`.
  - removed all candidate threads tied to those PRs from current queue.
  - updated snapshot summary from:
    - excluded `43 -> 45`
    - candidates `59 -> 53`.

## Files Touched

- `crates/tsz-solver/src/def/resolver.rs`
- `crates/tsz-solver/src/evaluation/evaluate_rules/keyof.rs`
- `crates/tsz-checker/src/types/queries/core.rs`
- `crates/tsz-checker/src/types/type_node.rs`
- `crates/tsz-checker/tests/symbol_index_signature_tests.rs`
- `crates/tsz-emitter/src/emitter/statements/core.rs`
- `crates/tsz-emitter/src/emitter/declarations/namespace.rs`
- `crates/tsz-emitter/src/transforms/ir_printer.rs`
- `crates/tsz-emitter/src/emitter/declarations/namespace/tests.rs`
- `crates/tsz-emitter/tests/ir_printer.rs`
- `docs/plan/review-comment-audit-latest.json`
- `docs/plan/review-comment-audit-latest.md`

## Verification

- `cargo test -p tsz-checker --test symbol_index_signature_tests -- --nocapture`
  - result: `7 passed; 0 failed`
- `cargo test -p tsz-emitter namespace_recovers_malformed_export_function_arrow_object_literal_body -- --nocapture`
  - result: `1 passed; 0 failed`
- `cargo test -p tsz-emitter test_emit_expression_statement_wraps_object_literal -- --nocapture`
  - result: `1 passed; 0 failed`
- `cargo fmt --all`
  - result: success
