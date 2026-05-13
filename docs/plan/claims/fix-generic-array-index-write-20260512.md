# fix(checker): allow generic array indexed writes

- **Date**: 2026-05-12
- **Branch**: `fix-generic-array-index-write-20260512`
- **Base**: `main`
- **Issue**: [#6100](https://github.com/mohsen1/tsz/issues/6100)
- **PR**: [#6102](https://github.com/mohsen1/tsz/pull/6102)
- **Status**: ready
- **Workstream**: 1 (diagnostic conformance / false-positive checker bug)

## Intent

Make `tsz` match `tsc` for mutable generic array writes such as
`this.items[index] = value` when `items` has type `T[]`. The checker should not
emit TS2862 for a writable array indexed by `number`, while preserving TS2862
for readonly or broad generic indexed writes.

## Final Scope

- Added focused regressions for the #6100 `T[]` repro and for direct writes
  through a mutable array-constrained type parameter.
- Narrowed the TS2862 generic indexed-write check so mutable arrays and tuples
  use normal assignability instead of being treated as read-only generic
  index-signature targets.
- Preserved existing TS2862 coverage for broad generic object/index-signature
  writes.

## Verification

- `cargo fmt`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker --test ts2862_keyof_tparam_tests -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker --test conformance_issues ts2862 -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker ts2862 -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker --lib`
- `env CARGO_INCREMENTAL=0 cargo check -p tsz-checker`
- `env CARGO_INCREMENTAL=0 cargo build --release -p tsz-cli --bin tsz`
- Manual #6100 repro comparison:
  - `./scripts/node_modules/.bin/tsc --noEmit --strict --lib ES2020 "$repro"` exited 0
  - `.target/release/tsz --noEmit --strict --lib ES2020 "$repro"` exited 0
