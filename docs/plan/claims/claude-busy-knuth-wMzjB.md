# fix(checker): eliminate false TS2456/TS2322 for merged const+type alias pattern (issue #5808)

- **Date**: 2026-05-12
- **Branch**: `claude/busy-knuth-wMzjB`
- **PR**: #5885
- **Status**: ready
- **Workstream**: checker conformance — merged symbol resolution

## Intent

Fixes a false-positive TS2456 ("Type alias circularly references itself") and
TS2322 for the idiomatic TypeScript pattern:

```typescript
const Direction = { Up: 0, Down: 1 } as const;
type Direction = typeof Direction[keyof typeof Direction];
const d: Direction = 0;  // was wrongly rejected
```

The binder merges `const X` and `type X` into a single symbol. During type-alias
body lowering, `symbol_types[X]` holds a `Lazy(DefId)` placeholder; `typeof X`
inside the alias body was incorrectly resolving to that placeholder, triggering
the circularity check. The fix pre-computes the VALUE-side type into
`merged_value_types` before lowering begins, and routes `typeof X` to that cache.

Separately, the eager-evaluation block for non-generic alias bodies was extended
from conditional-only to also cover `IndexAccess` and `KeyOf` bodies, so that
`type V = Obj[keyof Obj]` aliases are resolved to their concrete union/key types
during alias resolution (matching tsc behaviour).

## Files Touched

- `crates/tsz-checker/src/context/mod.rs` — added `merged_value_types` field
- `crates/tsz-checker/src/context/constructors.rs` — initialized new field
- `crates/tsz-checker/src/state/type_analysis/computed/type_alias_variable_alias.rs` — pre-compute VALUE type; extend eager eval to IndexAccess/KeyOf
- `crates/tsz-checker/src/state/variable_checking/core.rs` — extend merged-symbol guard to TYPE_ALIAS (not just INTERFACE)
- `crates/tsz-checker/src/types/type_node_advanced.rs` — route `typeof X` for merged VALUE+TYPE_ALIAS to `merged_value_types`
- `crates/tsz-checker/src/query_boundaries/common.rs` — added `is_evaluable_meta_type` helper
- `crates/tsz-checker/tests/merged_symbol_tests.rs` — added regression tests; registered in Cargo.toml

## Verification

- `cargo test -p tsz-checker --test merged_symbol_tests` (8 tests pass)
- `cargo test -p tsz-checker --test conformance_issues` (895 pass, 16 ignored)
- `cargo run --bin tsz -- /tmp/test_issue_5808.ts` (no errors emitted)
