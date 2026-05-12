# fix(checker): no false TS2456 for merged const+type-alias indexed access

- **Date**: 2026-05-12
- **Branch**: `claude/busy-knuth-FRkWx`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance / false-positive reduction

## Intent

Fixes issue #5808: `const X = {...} as const; type X = typeof X[keyof typeof X]`
emitted a false TS2456 ("Type alias circularly references itself"). The bug: a
merged VALUE+TYPE_ALIAS symbol has `Lazy(own_def_id)` in `symbol_types` while
the type alias is being resolved. `get_type_from_type_query` was returning this
placeholder for `typeof X` inside the alias body, causing the circularity
detector to see a self-referencing Lazy and emit TS2456. Fix: when `sym_id` has
the `TYPE_ALIAS` flag and is in `symbol_resolution_set`, skip the cache and fall
through to the annotation/TypeQuery path — `typeof X` then resolves to a
`TypeQuery(sym_ref)` which the circularity detector correctly ignores.

## Files Touched

- `crates/tsz-checker/src/types/type_node_advanced.rs` (+11 lines in `get_type_from_type_query`)
- `crates/tsz-checker/tests/type_alias_typeof_circular_tests.rs` (+36 lines, 2 new tests)

## Verification

- `cargo test -p tsz-checker --test type_alias_typeof_circular_tests` — 11/11 pass
- `cargo test -p tsz-checker --test type_alias_namespace_merge_tests` — 4/4 pass
