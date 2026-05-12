# perf(checker): T2.2 — finish CrossFileQueryKind migration, drop legacy const aliases

- **Date**: 2026-05-10
- **Branch**: `perf/t2.2-finish-query-kind-migration-2026-05-10`
- **PR**: TBD
- **Status**: claim
- **Workstream**: T2.2 PR 6B+ preparation (PERFORMANCE_PLAN.md §7)

## Intent

Finish the typed-query-kind migration started by #5043 (`CrossFileQueryKind`
enum) and #5053 (cross-file-query helpers). Two inline raw-`u8` call sites
remained:

- `crates/tsz-checker/src/state/type_analysis/cross_file.rs:1188` — write into
  the `ClassInstanceType` bucket from the cross-arena class-instance fallback.
- `crates/tsz-checker/src/context/resolver.rs:637` — read from the
  `SymbolType` bucket inside `resolve_lazy`.

Both now call `CrossFileQueryKind::Variant.as_storage_kind()`. With no
inline call sites left, the legacy `pub(crate) const CROSS_FILE_QUERY_*`
aliases at `cross_file.rs:66-73` are removed entirely, along with the
`const_aliases_match_enum_storage` test that locked them to the enum.

The remaining storage-stability invariant — absolute `u8` values 1, 2, 3, 4
for `InterfaceType`, `ClassInstanceType`, `InterfaceMemberSimpleType`,
`SymbolType` — is still enforced by the `discriminants_match_historical_constants`
unit test, so changing the discriminant of any variant breaks the build the
same way it did before.

## What changed

- `crates/tsz-checker/src/state/type_analysis/cross_file.rs`:
  - Inline write site at line 1188 now uses
    `CrossFileQueryKind::ClassInstanceType.as_storage_kind()`.
  - The four `pub(crate) const CROSS_FILE_QUERY_*` aliases are removed.
  - The `const_aliases_match_enum_storage` test is removed (its purpose —
    locking the consts to enum storage — is moot once the consts are gone).
- `crates/tsz-checker/src/context/resolver.rs`:
  - Read site at line 637 now uses
    `CrossFileQueryKind::SymbolType.as_storage_kind()`.
- `crates/tsz-checker/src/context/cross_file_query.rs`:
  - Module docstring updated: discriminants route through `CrossFileQueryKind`;
    the storage layer is the only place a bare `u8` lives.
  - Per-helper doc comments referencing the old `CROSS_FILE_QUERY_*` bucket
    names are rewritten to `CrossFileQueryKind::Variant`.

## Verification

- `cargo check -p tsz-checker` — clean
- `cargo clippy -p tsz-checker --all-targets -- -D warnings` — clean
- `cargo nextest run -p tsz-checker --lib -E 'test(/cross_file_query/)'`
  — 5/5 pass (`discriminants_match_historical_constants`,
  `key_implements_required_traits`, `key_hash_and_eq_round_trip`,
  `answer_variants_constructible`,
  `cross_file_cache_readers_reject_non_interned_type_ids`).
- Pre-commit hook (fmt, clippy, arch guard, full nextest suite) — to be
  confirmed before push.

## Storage stability

Storage discriminants are byte-identical to the legacy constants:
`CrossFileQueryKind::InterfaceType.as_storage_kind() == 1`, ditto 2/3/4 for
the other three variants. The `discriminants_match_historical_constants`
test enforces this. Migration is purely call-site hygiene — no behavior
change.

## Conformance

No semantic change. Storage discriminants unchanged. Conformance unaffected.
