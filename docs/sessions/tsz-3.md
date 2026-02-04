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

## Tasks

### Task 1: Diagnose Global Resolution Gaps
Create a minimal reproduction case and investigate how lib_contexts are used:

**Files to investigate**:
- `src/checker/symbol_resolver.rs` - Symbol resolution logic
- `src/checker/context.rs` - Type context with lib_contexts
- `src/binder/mod.rs` - Binder with lib file loading

**Actions**:
1. Create test case using standard globals (console, Promise, Array)
2. Trace through resolution to find where lib symbols aren't being found
3. Check if lib_contexts are being queried correctly

### Task 2: Fix Lib Context Merging
Ensure lib.d.ts symbols are correctly merged into scope chain:

**Actions**:
1. Verify `resolve_identifier_symbol` in symbol_resolver.rs
2. Check fallback to lib binders when symbol not in local scope
3. Ensure lib_contexts from CLI driver are passed through to checker

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
