# fix(dts): emit default function expandos as namespace aliases

- **Date**: 2026-05-05
- **Branch**: `fix-dts-default-function-expando`
- **PR**: TBD
- **Status**: ready
- **Workstream**: Declaration emit conformance

## Intent

TypeScript declaration emit lowers a named `export default function` with
late-bound property assignments into a local function declaration, a merged
namespace, and a trailing default export alias. This PR teaches the declaration
emitter to use that shape so `exportDefaultNamespace` matches the TypeScript
baseline.

## Files Touched

- `crates/tsz-emitter/src/declaration_emitter/exports/mod.rs`
- `crates/tsz-emitter/src/declaration_emitter/tests/simple_declarations.rs`

## Verification

- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo check --package tsz-emitter`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target scripts/safe-run.sh cargo test --package tsz-emitter test_export_default_function_with_late_bound_assignment_emits_default_alias --lib`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target scripts/safe-run.sh ./scripts/emit/run.sh --dts-only --filter=exportDefaultNamespace --concurrency=1 --timeout=10000`
