# fix(emitter): avoid imports for referenced declaration type queries

- **Date**: 2026-05-03
- **Branch**: `fix/dts-reference-directive-typeof-05031312`
- **PR**: TBD
- **Status**: ready
- **Workstream**: §2 (Emit pass rate)

## Intent

Investigate and fix declaration emit cases where a triple-slash reference
directive already makes a declaration-file symbol visible, but tsz still adds a
synthetic import for a `typeof` query in the generated `.d.ts`. The target
cluster is the `typeReferenceDirectives` pair with a single extra import line.

## Files Touched

- `crates/tsz-emitter/src/declaration_emitter/usage_analyzer.rs`
- `crates/tsz-emitter/src/declaration_emitter/core/emit_declarations.rs`
- `crates/tsz-cli/src/driver/emit.rs`
- `crates/tsz-cli/tests/driver_tests.rs`

## Verification

- `./scripts/safe-run.sh ./scripts/emit/run.sh --dts-only --filter=typeReferenceDirectives --verbose --timeout=20000 --json-out=/tmp/tsz-emit-typeReferenceDirectives-before.json`
  (reproduces 9/11 pass; failures are `typeReferenceDirectives5` and
  `typeReferenceDirectives13` with one extra import line).
- `cargo fmt --all --check && git --no-pager diff --check`
- `./scripts/safe-run.sh ./scripts/emit/run.sh --dts-only --filter=typeReferenceDirectives --verbose --timeout=20000 --json-out=/tmp/tsz-emit-typeReferenceDirectives-final.json`
  (11/11 pass).
- `cargo nextest run -p tsz-cli declaration_emit_type_reference_typeof_uses_referenced_global_value_without_import declaration_emit_type_reference_typeof_keeps_imported_value`
- `./scripts/safe-run.sh cargo nextest run -p tsz-emitter`
- `./scripts/safe-run.sh cargo clippy -p tsz-emitter --all-targets -- -D warnings`
- `./scripts/safe-run.sh cargo clippy -p tsz-cli --all-targets -- -D warnings`
