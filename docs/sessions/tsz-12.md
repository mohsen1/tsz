# Session TSZ-12: Cache Invalidation Fix

**Started**: 2026-02-06
**Status**: ✅ COMPLETE
**Predecessor**: TSZ-11 (Readonly Type Support - Complete)
**Successor**: TSZ-13 (Foundational Cleanup & Index Signatures)

## Accomplishments

### Cache Bug Fix ✅ Complete

**Problem**: `compile_with_cache` was not reusing cached compilation results, causing all files to be re-emitted even when unchanged.

**Root Cause** (found by Gemini):
- `local_cache` was always initialized to `Some(CompilationCache::default())` at line 663
- `effective_cache = local_cache_ref.or(cache)` always picked the empty `local_cache`
- Provided `cache` with previous compilation results was ignored

**Fix** (in `src/cli/driver.rs`):
```rust
// BEFORE: Always created empty local cache
let mut local_cache: Option<CompilationCache> = Some(CompilationCache::default());

// AFTER: Only create local_cache when loading from BuildInfo
let mut local_cache: Option<CompilationCache> = None;
if cache.is_none() && (resolved.incremental || resolved.ts_build_info_file.is_some()) {
    // ... load BuildInfo ...
    local_cache = Some(build_info_to_compilation_cache(&build_info, &base_dir));
}
```

**Impact**: +15 tests fixed (8232 → 8247 passing, 68 → 53 failing)
**Commit**: `2d2f3e22c`

**Tests Fixed**:
- `compile_with_cache_emits_only_dirty_files` ✅
- `compile_with_cache_invalidates_paths` ✅
- `compile_with_cache_skips_dependents_when_exports_unchanged` ✅
- `compile_with_cache_updates_dependencies_for_changed_files` ✅
- `compile_with_cache_invalidates_dependents` ✅
- `compile_with_cache_rechecks_dependents_on_export_change` ✅
- +9 other cache-related tests ✅

## Test Status

**Start**: 8232 passing, 68 failing
**End**: 8247 passing, 53 failing
**Result**: +15 tests fixed (22% of remaining failures)

## Notes

**Gemini's Guidance**: This session successfully fixed the cache invalidation bug. The root cause was identified by Gemini as a cache preference issue where `local_cache` was always created (empty) and preferred over the provided cache parameter.

**Pattern Recognition**: Following AGENTS.md mandatory workflow (asking Gemini for investigation guidance) led directly to finding the root cause in <5 minutes, vs hours of manual debugging.

**Next Session**: TSZ-13 focuses on "Foundational Cleanup & Index Signatures":
- Readonly infrastructure tests (~6 tests) - test setup issues
- Enum error count mismatches (~2 tests) - diagnostic deduplication
- Element access index signatures (~3 tests) - high impact foundational feature