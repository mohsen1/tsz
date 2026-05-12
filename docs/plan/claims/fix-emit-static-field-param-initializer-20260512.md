# fix(emit): lower static-field class expressions in parameter initializers

- **Date**: 2026-05-12
- **Branch**: `fix/emit-static-field-param-initializer-20260512`
- **PR**: #5981
- **Status**: implemented
- **Workstream**: 2 (JS emit pass rate)

## Intent

Fix the live JavaScript emit mismatch for
`classWithStaticFieldInParameterInitializer`. ESNext already matches TypeScript,
but ES2015/ES5 lowering emits the class expression directly inside the parameter
initializer instead of matching TypeScript's transformed default-parameter shape
and class-name preservation around the static field assignment.

## Files Touched

- `docs/plan/claims/fix-emit-static-field-param-initializer-20260512.md`
- `crates/tsz-emitter/src/emitter/declarations/class/helpers.rs`
- `crates/tsz-emitter/src/emitter/es5/helpers_async.rs`
- `crates/tsz-emitter/src/emitter/functions.rs`
- `crates/tsz-emitter/src/lowering/helpers.rs`
- `crates/tsz-emitter/tests/printer.rs`

## Verification

- `RUSTC_WRAPPER= CARGO_INCREMENTAL=0 cargo test -p tsz-emitter static_field_class_expression_in_ -- --nocapture`
- `RUSTC_WRAPPER= CARGO_INCREMENTAL=0 cargo test -p tsz-emitter anonymous_class_expr -- --nocapture`
- `RUSTC_WRAPPER= CARGO_INCREMENTAL=0 cargo test -p tsz-emitter --test tc39_named_class_expression_set_function_name_tests -- --nocapture`
- `RUSTC_WRAPPER= CARGO_INCREMENTAL=0 cargo test -p tsz-emitter es5_static_class_expression -- --nocapture`
- `RUSTC_WRAPPER= CARGO_INCREMENTAL=0 cargo test -p tsz-emitter`
- `RUSTC_WRAPPER= CARGO_INCREMENTAL=0 cargo build --profile dist-fast -p tsz-cli --bin tsz`
- `TSZ_BIN="$PWD/.target/dist-fast/tsz" ./scripts/emit/run.sh --filter=classWithStaticFieldInParameterInitializer --skip-build --verbose --js-only --concurrency=1 --timeout=10000`
