# [WIP] fix(emitter): preserve spread enum key order

- **Date**: 2026-05-02
- **Branch**: `fix/dts-spread-string-enum`
- **PR**: #2217
- **Status**: validated locally
- **Workstream**: 2 (declaration emit pass rate)

## Intent

Fix `declarationEmitSpreadStringlyKeyedEnum`, where declaration emit prints
the correct spread enum object members but in a non-tsc order. The target is a
narrow ordering fix that preserves enum declaration order for stringly keyed
enum spread object types.

## Files Touched

- `crates/tsz-checker/src/state/type_environment/core.rs`
- `crates/tsz-cli/tests/driver_tests.rs`

## Verification

- Focused emit repro for `declarationEmitSpreadStringlyKeyedEnum`.
- `cargo fmt --check`
- `cargo nextest run -p tsz-cli declaration_emit_spread_stringly_keyed_enum_preserves_member_order`
- `TSZ_BIN=/tmp/tsz-tail-failures/.target/release/tsz scripts/safe-run.sh ./scripts/emit/run.sh --dts-only --filter=declarationEmitSpreadStringlyKeyedEnum --verbose --json-out=/tmp/tsz-tail-failures/.tmp-spread-string-enum-after.json`
- `cargo nextest run -p tsz-checker`
