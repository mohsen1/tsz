# TS2322 Parser Recovery Fix (2026-02-15)

## Scope
- Address parser-recovery mismatch where `TS2693`/`TS2322`-related behavior regressed around async-generator
  computed-property cases when parse errors are present and symbol scopes span multiple files.
- Affected area:
  - `repro_async_generator_class_methods_cross_file`
  - async generator computed yield recovery cases in parser-conformance suites.

## Root Cause
- Identifier lookup could return a symbol id from the local binder even when that id was reused in another file.
- In cross-file scenarios (`all_binders`), `resolve_identifier_symbol` and type-position fallback returned a local
  `SymbolId` whose numeric value collided with a symbol from the other file.
- Subsequent checker logic used the local binder's symbol table first, which could treat `yield` as an unrelated symbol (`C21`)
  and suppress the expected type/diagnostic behavior.

## Fix
- `crates/tsz-checker/src/symbol_resolver.rs`
  - Added expected-name filtering after `resolve_identifier_with_filter` in `resolve_identifier_symbol_inner`.
  - Added cross-binder fallback in both value and type-position resolution:
    - `resolve_identifier_symbol_from_all_binders(name, |sym_id, symbol| ...)`
  - Updated fallback closure to receive symbol metadata (`FnMut(SymbolId, &Symbol)`), so class-member filtering remains accurate.
  - Added cross-file target tracking whenever:
    - symbol name differs from local binder entry, or
    - symbol is missing in the local binder.
  - Keeps existing behavior for non-cross-file resolution unchanged.
- `crates/tsz-checker/src/type_computation_complex.rs`
  - `get_type_of_identifier` now resolves symbol metadata through
    `self.get_cross_file_symbol(sym_id).or_else(|| self.ctx.binder.get_symbol(sym_id))`
    to avoid stale local symbol collisions during type checking.

## Validation
- `cargo test -p tsz-checker repro_async_generator_class_methods_cross_file -- --nocapture`
- `cargo test -p tsz-checker repro_async_generator_ -- --nocapture`

Both test groups now report `2693` diagnostics for cross-file/parser-recovery async generator cases as expected.

## Notes
- No parser grammar changes were required; this is a checker symbol-resolution path correction aligned with
  the cross-binder symbol map protocol already used by namespace/member diagnostics.
