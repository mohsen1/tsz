# Claim - Source-file symbol-arena cache eligibility attribution

**Owner:** Codex session
**Branch:** `codex/perf-symbol-arena-cache-eligibility-20260513`
**Sequences after:** #6191 (`DelegateCrossArenaSymbol` stable source-file cache)
**Input decision record:** [`perf-runs/2026-05-13-delegate-bucket-empty-attribution.md`](../perf-runs/2026-05-13-delegate-bucket-empty-attribution.md)

## Goal

Classify the remaining post-#6191 `DelegateCrossArenaSymbol` residue before
widening cache keys or direct-lowering behavior.

#6191 converted 96 stable source-file symbol-arena `bucket_empty` misses into
cross-file cache hits on `monorepo-006`, but left 828
`DelegateCrossArenaSymbol` child-checker constructions. Existing counters show
those misses are all `symbol_arenas`, mostly `variable` and `interface`, but do
not explain why a given delegation is cacheable, a first miss, or structurally
ineligible for the source-file symbol-arena cache.

## Implemented slice

Add a perf-counter-only classification array:

```text
source_file_symbol_arena_cache_eligibility_outcomes
```

The new buckets are:

- `cacheable`
- `cross_file_target`
- `non_symbol_arena`
- `module_augmentation`
- `missing_delegate_arena`
- `current_arena`
- `missing_source_file`
- `target_declaration_file`
- `missing_symbol`
- `not_class_or_interface`
- `multiple_declarations`
- `declaration_arena_mismatch`
- `missing_file_index`

The counter is recorded at the existing
`symbol_arena_symbol_type_cache_file_idx` decision point. It does not change
cache lookup keys, cache writes, direct lowering, or child-checker fallback
behavior.

## Non-goals

- No cache-key widening beyond #6191.
- No source-file interface direct-lowering behavior changes.
- No attempt to reduce `DelegateCrossArenaSymbol` count in this preparatory
  slice.
- No timing-mode wall-time claim.

## Verification

- `cargo fmt`
- `git diff --check`
- `cargo test -p tsz-common perf_counters --lib`

`cargo test -p tsz-checker stable_source_file_symbol_type_cache_key_uses_scope_without_requester --lib`
is blocked in this local toolchain by the pre-existing
`Cell::<[T; N]>::as_array_of_cells` instability in `tsz-solver`.

## Follow-up

Run attribution mode on `monorepo-006` and use this new array together with
`cross_file_cache_miss_causes.bucket_empty` to choose the next T2.2 behavior
slice.
