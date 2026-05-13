# fix(checker): resolve mixin extends parameter shadowing

- **Date**: 2026-05-12
- **Branch**: `fix-mixin-extends-parameter-shadow-20260512`
- **Base**: `main`
- **Issue**: [#6101](https://github.com/mohsen1/tsz/issues/6101)
- **PR**: [#6105](https://github.com/mohsen1/tsz/pull/6105)
- **Status**: ready
- **Workstream**: 1 (diagnostic conformance / false-positive binder/checker bug)

## Intent

Make `tsz` match `tsc` when a mixin function parameter shadows an outer
abstract class name and a returned class expression extends that parameter.
The heritage expression is in value position, so `extends Base` should resolve
to the function parameter `Base: TBase`, not to the outer abstract class `Base`.

## Final Scope

- Added a focused regression for the #6101 repro that requires no diagnostics.
- Made the abstract-member direct-base shortcut use scoped heritage resolution
  instead of `file_locals` text lookup, so function parameters in value position
  can shadow outer abstract class declarations.
- Preserved genuine TS2653 diagnostics for class expressions that directly
  extend an abstract base without implementing abstract members.

## Verification

- `cargo fmt`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker mixin_extends_parameter_shadowing_abstract_class_no_ts2653 --lib -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker class_expression_extending_abstract_class_still_emits_ts2653 --lib -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker class_member_closure_tests --lib -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker --lib`
- `env CARGO_INCREMENTAL=0 cargo check -p tsz-checker`
- `env CARGO_INCREMENTAL=0 cargo build --release -p tsz-cli --bin tsz`
- Manual #6101 repro comparison:
  - `./scripts/node_modules/.bin/tsc --noEmit --strict --lib ES2020 "$repro"` exited 0
  - `.target/release/tsz --noEmit --strict --lib ES2020 "$repro"` exited 0
