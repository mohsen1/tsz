# fix(emitter): preserve ambiguous type export values

- **Date**: 2026-05-03
- **Branch**: `fix/dts-export-specifier-values-05031450`
- **PR**: TBD
- **Status**: ready
- **Workstream**: §2 (Emit pass rate)

## Intent

Preserve value declarations referenced by ambiguous export specifiers such as
`export { type as }` and `export { type something }` during declaration emit.
These specifiers can mark the file-local binding as publicly used even when the
declaration name node is mapped to a different direct symbol, so visibility now
falls back through the same name lookup path used by the usage analyzer.

## Files Touched

- `crates/tsz-emitter/src/declaration_emitter/helpers/visibility.rs`
  (preserve public API dependencies found by name fallback).
- `crates/tsz-emitter/src/declaration_emitter/tests/export_specifiers.rs`
  (regression coverage for ambiguous `type` export specifiers).

## Verification

- `cargo nextest run -p tsz-emitter type_modifier_ambiguous_export_specifiers_keep_local_values`
  (1/1 pass).
- `./scripts/safe-run.sh ./scripts/emit/run.sh --dts-only --filter=exportSpecifiers --timeout=20000 --json-out=/tmp/tsz-emit-exportSpecifiers-final.json`
  (1/1 declaration emit pass).
- `./scripts/safe-run.sh cargo nextest run -p tsz-emitter`
  (1824/1824 pass, 5 skipped).
