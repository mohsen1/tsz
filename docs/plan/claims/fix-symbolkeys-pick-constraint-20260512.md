# fix(solver): accept mapped symbol keys in Pick constraints

- **Date**: 2026-05-12
- **Branch**: `fix-symbolkeys-pick-constraint-20260512`
- **Base**: `main`
- **Issue**: [#6099](https://github.com/mohsen1/tsz/issues/6099)
- **PR**: [#6108](https://github.com/mohsen1/tsz/pull/6108)
- **Status**: ready
- **Workstream**: 1 (diagnostic conformance / false-positive solver bug)

## Intent

Make `tsz` match `tsc` for `Pick<T, SymbolKeys<T>>` where `SymbolKeys<T>` is
a mapped type over `keyof T` that extracts symbol keys. The extracted key type
is constructed from `keyof T`, so it should satisfy `Pick`'s `keyof T`
constraint.

## Final Scope

- Added a focused regression for the #6099 repro.
- Recognize conditional true branches of the form `K` whose base constraint
  already satisfies the instantiated target constraint before using the
  conditional `extends` type as a TS2344 proxy.
- Moved base-constraint helper code into the adjacent generic mapped-constraint
  helper module so the checker file-size guard stays green.

## Verification

- `cargo fmt`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker --test ts2344_keyof_bare_tparam_defer_tests test_mapped_symbol_keys_satisfy_pick_keyof_constraint -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker --test ts2344_keyof_bare_tparam_defer_tests -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker --test ts2344_infer_conditional_constraint -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker --test ts2344_generic_ref_scoped_param_concrete_constraint -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker ts2344 -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker --lib`
- `env CARGO_INCREMENTAL=0 cargo check -p tsz-checker`
- `env CARGO_INCREMENTAL=0 cargo build --release -p tsz-cli --bin tsz`
- Manual #6099 repro comparison:
  - `./scripts/node_modules/.bin/tsc --noEmit --strict --lib ES2020 "$repro"` exited 0
  - `.target/release/tsz --noEmit --strict --lib ES2020 "$repro"` exited 0
