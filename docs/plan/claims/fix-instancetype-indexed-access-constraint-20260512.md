# fix(checker): suppress cascading InstanceType constraint diagnostic

- **Date**: 2026-05-12
- **Branch**: `fix-instancetype-indexed-access-constraint-20260512`
- **Base**: `main`
- **Issue**: [#6093](https://github.com/mohsen1/tsz/issues/6093)
- **PR**: [#6098](https://github.com/mohsen1/tsz/pull/6098)
- **Status**: ready
- **Workstream**: 1 (diagnostic conformance / false-positive checker bug)

## Intent

Make `tsz` match `tsc` when `InstanceType<Outer["Inner"]>` first reports
TS2749 because `Outer` is a value used as a type. The checker should not also
emit the cascading TS2344 constraint diagnostic for the invalid type argument.

## Final Scope

- Added a focused checker regression for the #6093 repro.
- Suppressed the extra TS2344 constraint check only when the type-argument
  subtree already emitted TS2749 for value-as-type usage.
- Preserved real TS2344 diagnostics for valid but constraint-incompatible
  `InstanceType` arguments.

## Verification

- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker instancetype -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker ts2344 --lib -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker --lib`
- `env CARGO_INCREMENTAL=0 cargo check -p tsz-checker`
- `env CARGO_INCREMENTAL=0 cargo build --release -p tsz-cli --bin tsz`
- Manual #6093 repro comparison:
  - `./scripts/node_modules/.bin/tsc --noEmit --strict --lib ES2020 "$repro"` emitted TS2749 only, status 2
  - `.target/release/tsz --noEmit --strict --lib ES2020 "$repro"` emitted TS2749 only, status 2
