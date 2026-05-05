Status: ready
Branch: fix-dts-nested-generics
Owner: Codex
Started: 2026-05-05

## Intent

Fix declaration emit for nested generic return annotations that combine function-scoped type parameters, returned local annotations, and conditional `infer` binders, targeting `declarationEmitNestedGenerics`.

## Planned Scope

- Normalize returned local annotation text in function declaration return scope.
- Preserve nested function generic shadowing while substituting conditional `typeof` parameter checks.
- Add focused regression coverage for the emitted `.d.ts` shape.

## Verification Plan

- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo fmt --package tsz-emitter -- --check`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo clippy -p tsz-emitter --lib -- -D warnings`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo test --package tsz-emitter test_returned_local_conditional_annotation_uses_function_generic_scope --lib`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target ./scripts/emit/run.sh --dts-only --filter=declarationEmitNestedGenerics --verbose --concurrency=1 --timeout=30000 --json-out=/tmp/tsz-dts-nested-generics-final.json`
