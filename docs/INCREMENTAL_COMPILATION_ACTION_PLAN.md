# Action Plan: TypeScript-Compatible Incremental Compilation

## Overview

This document outlines the plan to complete tsz's incremental compilation feature to match tsc behavior. The goal is to properly load and use `.tsbuildinfo` files to skip recompilation of unchanged files.

## Implementation Status: ✅ COMPLETE (Phase 1-2, P1-P3)

**Completed:**
- ✅ BuildInfo loading and persistence
- ✅ Cache parameter passing to `collect_diagnostics`
- ✅ BuildInfo population from `CompilationCache`
- ✅ Tests for first build, second build (no changes), and third build (with changes)
- ✅ **P3: BuildInfo version compatibility** - Graceful handling of version mismatches
- ✅ **P2: Semantic diagnostics caching** - Diagnostics persist across builds
- ✅ **P1: Smart invalidation via export hash comparison** - Work queue algorithm

**Remaining (Future Work):**
- P4: Compression optimizations (fileIdsList, referencedMap)
- P5: Composite project support (latestChangedDtsFile) -- requires project references and declaration emit

## Current Status

**Working:**
- `.tsbuildinfo` file generation and persistence after successful compilation
- BuildInfo loading with graceful version mismatch handling
- Smart invalidation: only type-checks files that changed or depend on changed files
- Semantic diagnostics persist across builds (no "ghost diagnostics")
- Export hash comparison triggers cascading invalidation of dependents
- Work queue algorithm prevents infinite loops and duplicate checks

**Implementation Details:**
- `collect_diagnostics` uses work queue initialized with files not in cache
- After checking each file, export hash is computed and compared with cached hash
- If export signature changed, dependent files are queued for re-checking
- `checked_files` HashSet prevents infinite loops and duplicate checks
- Final diagnostics collected from cache for all files (checked + cached)
- Version mismatches trigger cache miss (fresh build) instead of error

## Root Cause Analysis

The current implementation has a **critical gap**:

1. `build_info_to_compilation_cache()` only populates `export_hashes` and `dependencies`
2. It does NOT restore `bind_cache` (ASTs) or `type_caches` (type information)
3. Since these caches are empty, ALL files are re-parsed and re-bound on every build
4. The only benefit currently is avoiding emit if output hasn't changed

**Why ASTs aren't cached:**
- Serializing full ASTs to JSON would be extremely large and complex
- TypeScript uses a similar approach - they cache metadata, not full trees
- The real optimization comes from export hash comparison

**⚠️ CRITICAL CONSTRAINT (from Gemini Review):**
We CANNOT skip parsing/binding of dependencies even if they haven't changed. The Checker *needs* the Symbols from those files to resolve imports in changed files. The optimization is limited to skipping the **Type Checking** phase (the expensive part), not the Parsing/Binding phase.

**Additional issues identified:**
1. **Ghost diagnostics:** If we skip checking a file, we must load and re-emit its cached diagnostics, otherwise errors disappear
2. **Cascading invalidation:** If a dependent's export hash changes, we must invalidate its dependents too
3. **Rust ownership:** Creating a unified `effective_cache` variable has lifetime issues; need explicit handling

## The Fix Strategy

Based on Gemini's analysis, here's the approach:

### Phase 1: Fix Scope Issue (Immediate)

The current blocker is that `collect_diagnostics` is trying to access `local_cache` and `using_local_cache` which are not in scope.

**Change:** Pass the cache as a parameter to `collect_diagnostics`

**File:** `src/cli/driver.rs`

**Steps:**
1. Modify `collect_diagnostics` signature to accept `Option<&mut CompilationCache>` parameter
2. Update the function body to use the passed cache instead of trying to access outer scope variables
3. Update the call site in `compile_inner` to pass the appropriate cache (either the parameter or local_cache)
4. Remove the broken `if using_local_cache` blocks inside `collect_diagnostics`

### Phase 2: Enable Real Incremental Behavior

**IMPORTANT CORRECTION (from Gemini Review):**

We **CANNOT** skip parsing/binding of unchanged files. The reason:
- If File A (changed) imports File B (unchanged)
- The Checker needs Symbols from File B to type-check File A
- If we skip parsing B, the Symbol Table is empty → compilation fails

**What we CAN optimize:**
1. **Skip Type Checking** for unchanged files (the expensive Solver phase)
2. **Reuse cached diagnostics** for unchanged files
3. **Smart invalidation** via export hash comparison

**Correct flow:**
1. Load BuildInfo → get `export_hashes` and `dependencies`
2. Read all source files → compute current hashes
3. **Parse and Bind ALL files** (required for symbol resolution)
4. Compute export hashes for changed files
5. Compare with old export hashes → identify API changes
6. **Type Check ONLY:**
   - Changed files
   - Dependents of files with changed export signatures
7. **Replay cached diagnostics** for files that weren't type-checked

**Two-pass strategy:**
- **Pass 1:** Parse/Bind all files, compute export hashes for changed files
- **Pass 2:** Type check only the files in the "invalidation set"

### Phase 3: Missing Features (Future)

Based on Q8, these are missing from the current implementation:

1. **Compression Optimizations:**
   - `fileIdsList` - integer-based file IDs to reduce string duplication
   - `referencedMap` - separate tracking of module imports vs triple-slash references

2. **Composite Project Support:**
   - `latestChangedDtsFile` - track which .d.ts file changed for downstream projects
   - Project reference state caching

3. **Semantic Diagnostics Caching:**
   - Currently `semantic_diagnostics_per_file` is always empty
   - Should cache errors for unchanged files

4. **Affected Files Tracking:**
   - `affectedFilesPendingEmit` is hardcoded to `false`
   - Should track files that need emitting on next build

## Detailed Implementation Plan

### Step 1: Fix `collect_diagnostics` Scope Issue

**File:** `src/cli/driver.rs`

**Current broken code in `compile_inner`:**
```rust
// Determine the effective cache to use for diagnostics
let effective_cache = if cache.is_some() {
    cache.as_deref_mut()
} else {
    local_cache.as_mut()
};
```

This creates a reference with complex lifetime issues. The better approach:

```rust
// In compile_inner, before calling collect_diagnostics:
// The cache parameter (if provided) takes priority, otherwise use local_cache
// We need to be careful about mutable reference lifetimes

// Let's refactor: remove the complex conditional binding
// Instead, handle the two cases explicitly in the call
```

**New approach:**
1. Make `collect_diagnostics` accept `Option<&mut CompilationCache>`
2. In `compile_inner`, determine which cache to pass:
   - If `cache.is_some()`, pass that
   - Else, pass `local_cache.as_mut()`
3. Inside `collect_diagnostics`, remove all `if using_local_cache` checks
   - Just use `if let Some(c) = cache.as_deref_mut()` uniformly

### Step 2: Implement Proper Change Detection

The current code in `build_program_with_cache` checks hash matches:

```rust
let cached_ok = cache
    .bind_cache
    .get(&source.path)
    .map(|entry| entry.hash == hash)
    .unwrap_or(false);
```

This is checking if the AST is cached, which won't work when we load from BuildInfo.

**New approach:**
1. Add a check for `export_hashes` instead of `bind_cache`
2. If `export_hash` matches, we can potentially skip the file entirely
3. If it doesn't match, we need to re-parse and re-bind

**Pseudo-code:**
```rust
// In build_program_with_cache or a new function
let cached_ok = cache
    .export_hashes
    .get(&source.path)
    .map(|hash| {
        // Need to compute current hash first
        let current_hash = compute_file_hash(&source.text);
        *hash == current_hash
    })
    .unwrap_or(false);
```

Wait - this is circular. We need to compute the hash before we can check it.

**Correct approach:**
1. Always read file contents
2. Compute content hash
3. Check against `export_hashes` in cache
4. If match AND file is not in changed_paths → skip parsing/binding
5. If no match or in changed_paths → parse and bind

### Step 3: Update `build_program_with_cache`

**RECONSIDERED APPROACH (from Gemini Review):**

Since we cannot skip parsing/binding, `build_program_with_cache` should:
1. **Always parse and bind** all files
2. Use `export_hashes` to identify which files need type checking
3. For unchanged files, still create BindCacheEntry but skip type checking

**Actually, the current approach is mostly correct for parsing/binding:**
- `build_program_with_cache` checks if the hash matches in `bind_cache`
- If match, it skips re-parsing (reusing the AST)
- If no match, it parses

**The issue:** When loading from BuildInfo, `bind_cache` is empty, so everything gets re-parsed. **This is acceptable for now.**

**The real optimization happens in `collect_diagnostics`:**
1. Compute export hash for each file after binding
2. Compare with cached export hash
3. If match: Skip type checking, reuse cached diagnostics
4. If different: Type check, compute new export hash, cascade invalidation if needed

**So Step 3 is actually:** Ensure `collect_diagnostics` properly implements export hash comparison.

### Step 4: Implement Export Hash Comparison

After binding changed files, we need to:

1. Compute export hashes for newly bound files
2. Compare with previous export hashes
3. If export signature changed, find and invalidate dependents

This logic already exists in `compile_with_cache_and_changes` - we just need to make sure it's being called when using BuildInfo.

## Testing Strategy

### Unit Tests
1. Test BuildInfo loading with missing/corrupt files
2. Test export hash comparison detects type changes
3. Test dependency graph traversal finds all dependents

### Integration Tests
1. Create a project with multiple files
2. First build: generates .tsbuildinfo
3. Second build (no changes): should be instant
4. Third build (change one file): only that file + dependents rebuild
5. Verify .d.ts and .js outputs are correct

### Manual Testing
```bash
# Setup
mkdir /tmp/test_incremental
cd /tmp/test_incremental
cat > tsconfig.json << 'EOF'
{
  "compilerOptions": {
    "incremental": true,
    "tsBuildInfoFile": "dist/tsconfig.tsbuildinfo",
    "outDir": "dist"
  }
}
EOF

# Create source files
# ... (create a.ts, b.ts, c.ts where b imports a, c imports b)

# First build
time cargo run -- --bin tsz
# Check that dist/tsconfig.tsbuildinfo exists

# Second build (no changes)
time cargo run -- --bin tsz
# Should be much faster

# Change a.ts
echo "// change" >> a.ts

# Third build
time cargo run -- --bin tsz
# Should only rebuild a.ts, b.ts, c.ts
```

## Success Criteria

1. ✓ BuildInfo files are generated after successful compilation
2. ✓ BuildInfo files are loaded on subsequent builds
3. ✓ Loaded BuildInfo populates `export_hashes` and `dependencies`
4. ✓ BuildInfo is updated with `CompilationCache` state after each build
5. ✓ All tests pass, including new incremental tests
6. ✓ **Files with unchanged export signatures are NOT type-checked** (P1 smart invalidation)
7. ✓ **Semantic diagnostics persist across builds** (P2 diagnostics caching)
8. ✓ **Version mismatches handled gracefully** (P3 version compatibility)

## Implementation Summary

### What Was Done (2025-02-02 - 2025-02-03)

**Phase 1: Foundation (2025-02-02)**
1. **Fixed Scope Issue in `collect_diagnostics`**
   - Removed broken `if using_local_cache` blocks
   - Now uses unified `effective_cache` parameter

2. **Implemented Unified Cache Reference**
   - Created `effective_cache` that works for both `local_cache` (from BuildInfo) and `cache` parameter

3. **BuildInfo Persistence**
   - BuildInfo is now populated from `CompilationCache` using `compilation_cache_to_build_info()`
   - Includes `export_hashes`, `dependencies`, and other metadata

4. **Enhanced Test Coverage**
   - Test now verifies: first build, second build (no changes), third build (with changes)
   - All scenarios pass successfully

**Phase 2: Smart Invalidation (2025-02-03)**
5. **P3: BuildInfo Version Compatibility**
   - Changed `BuildInfo::load()` to return `Result<Option<Self>>`
   - Gracefully handle version mismatches by returning `Ok(None)`
   - Added compiler version validation alongside format version
   - Version mismatches trigger cache miss instead of build failure

6. **P2: Semantic Diagnostics Caching**
   - Added `CachedDiagnostic` and `CachedRelatedInformation` structs
   - Updated `BuildInfo.semantic_diagnostics_per_file` to use `CachedDiagnostic`
   - Implemented conversion from `Diagnostic` to `CachedDiagnostic` in `compilation_cache_to_build_info`
   - Implemented reverse conversion in `build_info_to_compilation_cache`
   - Diagnostics now persist across builds to avoid "ghost diagnostics"

7. **P1: Smart Invalidation via Export Hash Comparison**
   - Replaced simple for loop with work queue algorithm in `collect_diagnostics`
   - Initialize work queue with files not in cache (physically changed)
   - After checking each file, compute export hash and compare with cached hash
   - If export signature changed, queue all dependent files for re-checking
   - Use `checked_files` HashSet to prevent infinite loops and duplicate checks
   - Collect final diagnostics from cache for all files (checked + cached)

### Current Limitations

**Due to architectural design decisions:**

1. **No AST Caching to Disk**
   - BuildInfo does not store `bind_cache` (ASTs) or `type_caches`
   - Reason: Would be extremely large and complex to serialize
   - Impact: All files are parsed and bound on every build
   - **Mitigation:** Type checking (the expensive part) is skipped for unchanged files

2. **No Compression**
   - `fileIdsList` and `referencedMap` are not implemented
   - Reason: Low priority (P4) - nice-to-have optimization
   - Impact: BuildInfo files are larger than necessary for large projects

3. **No Composite Project Support**
   - `latestChangedDtsFile` is not implemented
   - Reason: Requires project references and declaration emit (P5 - strategic)
   - Impact: Multi-project builds don't share .d.ts change tracking

### What Works

1. **Smart Invalidation (P1)**
   - Work queue algorithm only type-checks files that need checking
   - Export hash comparison detects API changes
   - Cascading invalidation: dependents are re-checked when exports change
   - `checked_files` HashSet prevents infinite loops
   - Significant performance improvement for unchanged codebases

2. **Semantic Diagnostics Caching (P2)**
   - Diagnostics persist in BuildInfo across builds
   - `CachedDiagnostic` and `CachedRelatedInformation` structs
   - No "ghost diagnostics" - errors persist correctly
   - Conversion between `Diagnostic` and `CachedDiagnostic` formats

3. **BuildInfo Version Compatibility (P3)**
   - Graceful handling of version mismatches
   - Returns `Ok(None)` instead of error on mismatch
   - Compiler version validation
   - Automatic cache invalidation on version changes

4. **Change Detection**
   - `export_hashes` are computed and stored
   - `dependencies` and `reverse_dependencies` are tracked
   - BuildInfo can be loaded and used to populate CompilationCache
   - Files are checked if not in cache or if dependencies' exports changed

2. **Watch Mode Optimization**
   - Within the same process (watch mode), the in-memory `CompilationCache` provides true incremental compilation
   - Files with unchanged `bind_cache` skip parsing
   - Files with unchanged `type_cache` skip type checking

3. **Foundation for Future Improvements**
   - The infrastructure is in place to add more optimizations
   - BuildInfo format is extensible
   - Export hash comparison logic can be added later

## Open Questions

1. **Should we cache ASTs to .tsbuildinfo?**
   - Pros: Faster rebuilds (skip parsing)
   - Cons: Large file size, complex serialization
   - Decision: Start without AST caching, measure performance

2. **How to handle BuildInfo version compatibility?**
   - Current: `BUILD_INFO_VERSION = "0.1.0"`
   - Need: Migration strategy when format changes
   - Decision: Reject mismatched versions for now

3. **Should we implement compression (fileIdsList)?**
   - Pros: Smaller .tsbuildinfo files
   - Cons: More complex code
   - Decision: Defer until after basic functionality works

## References

- `src/cli/incremental.rs` - BuildInfo structures and persistence
- `src/cli/driver.rs` - CompilationCache and compilation logic
- Q1-Q8 Gemini responses - Technical details on how things work
- NORTH_STAR.md - Architecture design (Salsa-style query memoization)

## Gemini Review Feedback

### Review 1: Overall Structure and Completeness
**Key insights:**
1. **Critical architectural gap:** Cannot skip parsing/binding of dependencies - Checker needs their Symbols
2. **Ghost diagnostic problem:** Must load and re-emit cached diagnostics for skipped files
3. **Global augmentation:** If a file with global augmentation changes, all files need re-checking
4. **Deleted files:** Need to handle file deletion in dependency graph

**Revised invalidation logic:**
1. Parse/Bind all files in the compilation context
2. Only skip Type Checking for files not in the "InvalidationSet"
3. Replay cached diagnostics for skipped files

### Review 2: Technical Feasibility
**Key insights:**
1. **Cache parameter approach is sound** - just need to handle it carefully
2. **Export hash comparison works** - but we must parse/bind first
3. **Ownership issues:** Cannot easily create unified `effective_cache` variable due to lifetime differences
4. **Dependency traversal is correct** - but may need cascading invalidation loop

**Recommended safe pattern:**
```rust
let mut local_cache_ref = local_cache.as_mut();
let effective_cache = local_cache_ref.as_deref_mut().or(cache.as_deref_mut());
```

**Two-pass approach recommended:**
1. Parse/Bind everything (restore graph)
2. Type check only files in InvalidationSet

