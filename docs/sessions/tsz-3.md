# Session tsz-3 - Global Symbol Resolution (Fix TS2304 Poisoning)

**Started**: 2026-02-04
**Status**: ACTIVE
**Focus**: Critical Conformance - Global Symbol Resolution

## Goal

Fix the "poisoning" effect where missing global symbols (TS2304) cause types to default to `any`, which:
- Suppresses subsequent type errors
- Artificially inflates conformance scores
- Hides real type checking issues

## Problem Statement

From `docs/specs/DIAGNOSTICS.md` Section 2:
> When a global symbol like `console`, `Promise`, or `Array` fails to resolve (TS2304), it defaults to `any`. This "poisons" the type system by suppressing valid errors that should be emitted later.

**Example**:
```typescript
// If 'console' doesn't resolve, it becomes 'any'
console.log("hello");  // Should error but doesn't because console is 'any'
console.nonExistent(); // Should error TS2339 but doesn't
```

## Task 1: Diagnose Global Resolution Gaps ✅ COMPLETE

### Test Case
```typescript
console.nonExistentProperty;
```

**Results**:
- **tsc**: TS2339 (Property doesn't exist on type 'Console') ✅
- **tsz**: No errors - console is `any` (POISONING!) ❌

### Root Cause Found
**Location**: `src/binder/state.rs` lines 721-729

```rust
if !self.lib_symbols_merged {
    for lib_binder in lib_binders {
        if let Some(sym_id) = lib_binder.file_locals.get(name) {
            // ...
        }
    }
}
```

**The Bug**: Lib binders are only queried when `lib_symbols_merged` is FALSE. When it's TRUE (after merging), the code skips lib binder lookup entirely.

**Why this is wrong**: The checker's context has `lib_contexts` available (see `src/checker/generators.rs:925-926`), but `resolve_identifier_with_filter` doesn't check them - it only checks lib_binders conditionally.

**Evidence**:
- Checker can access lib_contexts directly: `self.ctx.lib_contexts.iter().map(|lc| &lc.binder)`
- Generators.rs successfully queries lib_contexts.file_locals
- But symbol_resolver.rs conditionally checks lib_binders based on lib_symbols_merged

### Task 2: Fix Lib Context Merging (IN PROGRESS)

**Solution**: Modify `src/checker/symbol_resolver.rs` to always query lib_contexts

**Current Code** (symbol_resolver.rs):
```rust
pub(crate) fn get_lib_binders(&self) -> Vec<Arc<crate::binder::BinderState>> {
    self.ctx.lib_contexts.iter().map(|lc| Arc::clone(&lc.binder)).collect()
}
```

**Problem**: This is passed to `resolve_identifier_with_filter`, but the binder only checks lib_binders when `lib_symbols_merged` is FALSE.

**Fix Options**:

**Option A**: Modify symbol_resolver to check lib_contexts.file_locals directly
- Similar to how generators.rs does it (line 926: `lib_ctx.binder.file_locals.get(name)`)
- More direct, bypasses the lib_symbols_merged check

**Option B**: Always query lib_binders regardless of lib_symbols_merged
- Change the binder's `resolve_identifier_with_filter` to always check lib_binders
- Remove or modify the `if !self.lib_symbols_merged` check

**Preferred**: Option A - check lib_contexts directly like other parts of the codebase

### Task 3: Verify Conformance Improvement
Run tests to verify the fix:

**Actions**:
1. Run conformance tests that rely on globals
2. Verify TS2304 is no longer emitted for valid globals
3. Verify TS2339 and other errors are now correctly emitted

## Context

**Previous Session**: Completed error formatting and module validation cleanup

**Key Insight**: This is a critical issue affecting conformance accuracy. Fixing it will likely reveal many hidden type errors that are currently being suppressed.

**Files**:
- `src/checker/symbol_resolver.rs` - Symbol resolution implementation
- `src/checker/context.rs` - Type context with lib_contexts
- `src/cli/driver.rs` - Where lib_contexts are created
- `src/binder/mod.rs` - Binder implementation

## Success Criteria

- ✅ Standard globals (console, Promise, Array, etc.) resolve correctly
- ✅ No TS2304 for valid global symbols
- ✅ Type errors are emitted correctly (not suppressed by `any`)
- ✅ Conformance tests show improvement
