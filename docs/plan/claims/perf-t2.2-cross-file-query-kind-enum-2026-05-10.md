# perf(checker): T2.2 prep — typed CrossFileQueryKind enum

- **Date**: 2026-05-10
- **Branch**: `perf/t2.2-cross-file-query-kind-enum-2026-05-10`
- **PR**: TBD
- **Status**: claim
- **Workstream**: T2.2 PR 6B+ preparation (PERFORMANCE_PLAN.md §7)

## Intent

First foundational step toward the typed cross-file query API the plan
§7 prescribes. Today the cache-bucket "kind" is a bare `u8` constant
across four call-site contexts. This PR wraps those `u8` values in a
typed `CrossFileQueryKind` enum that matches the plan's API:

```rust
pub enum CrossFileQueryKind {
    SymbolType,
    ClassInstanceType,
    InterfaceType,
    InterfaceMemberSimpleType,
}
```

The four `pub(crate) const CROSS_FILE_QUERY_*` aliases stay around so
existing call sites (~25) keep compiling. PR 6B+ migrates them onto
the typed `CrossFileQueryKind` one bucket at a time.

## Why now

- T2.1.A is in flight (#5034 merged, #5037/#5040 in flight). Subsequent
  T2.2 work needs a typed query primitive; introducing it now means
  PR 6B+ doesn't have to ship the enum + a migration in the same diff.
- The discriminants (`u8` values 1, 2, 3, 4) are storage-stable: they
  appear in `DefinitionStore::resolved_cross_file_queries` cache keys.
  This PR explicitly assigns each variant its historical discriminant
  via `#[repr(u8)]` and asserts the values via unit tests so any future
  PR that re-numbers a variant trips the test.

## Files Touched

- `crates/tsz-checker/src/state/type_analysis/cross_file.rs`:
  - Replace four `pub(crate) const ... : u8 = N;` lines with the
    `CrossFileQueryKind` enum and a `pub(crate) const fn
    as_storage_kind(self) -> u8` accessor.
  - Re-define each historical const as `CrossFileQueryKind::Variant.as_storage_kind()`
    so call sites continue to compile.
  - Add inline `#[cfg(test)]` module with two tests:
    - `discriminants_match_historical_constants` — asserts `1, 2, 3, 4`.
    - `const_aliases_match_enum_storage` — asserts the `pub(crate) const`
      aliases match the enum's storage value.

## Verification

- `cargo check -p tsz-checker` clean
- `cargo nextest run -p tsz-checker --lib -E 'test(cross_file_query_kind)'`
  — 2/2 tests pass
- Pre-commit hook (fmt, clippy `-D warnings`, arch guard, full nextest
  suite) — to be confirmed before push

## Conformance

No semantic change. Storage discriminants are byte-identical to the
existing `u8` constants. Cached query values, diagnostics, and
conformance snapshots are unaffected.
