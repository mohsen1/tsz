# perf(checker): T2.2 — route cross_file_query helpers through CrossFileQueryKind

- **Date**: 2026-05-10
- **Branch**: `perf/t2.2-helpers-use-query-kind-2026-05-10`
- **PR**: TBD
- **Status**: claim
- **Workstream**: T2.2 PR 6B+ preparation (PERFORMANCE_PLAN.md §7)

## Intent

First consumer of the typed `CrossFileQueryKind` enum (#5043). The
nine helpers in `crates/tsz-checker/src/context/cross_file_query.rs`
that wrap `DefinitionStore::get_resolved_cross_file_query` /
`cache_resolved_cross_file_query` previously passed bare
`CROSS_FILE_QUERY_*` `u8` constants. This PR migrates them to call
`CrossFileQueryKind::Variant.as_storage_kind()` instead.

The legacy `pub(crate) const CROSS_FILE_QUERY_*` aliases stay in
`cross_file.rs` for any remaining inline call sites (those move
later, one bucket per PR per the §7 migration order).

## What changed

- Module docstring updated to note bucket discriminants now route
  through `CrossFileQueryKind`.
- `use crate::state_type_analysis::cross_file::CrossFileQueryKind;`
  added.
- 9 call sites changed from
  `crate::state_type_analysis::cross_file::CROSS_FILE_QUERY_X` to
  `CrossFileQueryKind::Variant.as_storage_kind()`.
- Doc comments referencing the bucket NAMES (e.g. "via the canonical
  `CROSS_FILE_QUERY_SYMBOL_TYPE` bucket") are retained as descriptive
  identifiers — those are not constant references, just bucket labels.

## Verification

- `cargo check -p tsz-checker` clean
- Pre-commit hook (fmt, clippy, arch guard, full nextest suite) — to
  be confirmed before push
- `CrossFileQueryKind::*.as_storage_kind()` returns byte-identical
  `u8` values to the legacy constants (asserted by #5043's existing
  unit tests).

## Conformance

No semantic change. Storage discriminants are unchanged.
