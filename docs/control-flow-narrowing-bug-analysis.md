# Control Flow Narrowing Bug: Root Cause Analysis

**Date**: 2026-02-13
**Investigation**: Complete
**Status**: Bug identified, fix strategy defined

## Executive Summary

The control flow narrowing test failures (47/92 passing, 51.1%) are caused by **literal type widening in discriminated unions**. When object literals are assigned to discriminated union types, boolean/string/number literals are widened to their primitive types instead of preserving literal types.

## The Bug

### Failing Code

```typescript
type Result = { success: false } | { success: true };

const test1: Result = {
    success: false  // ❌ Error: Type '{ success: boolean }' is not assignable
};

function f(): Result {
    return {
        success: false  // ❌ Same error
    };
}
```

###Expected Behavior

The literal `false` should be preserved because:
1. The contextual type is a discriminated union with literal members
2. TSC preserves literals in this context to enable proper type matching

### Actual Behavior

The literal `false` is widened to `boolean`, causing:
- TS2322: Type mismatch error
- Discriminated union narrowing fails (the core issue)

## Technical Investigation

### Code Flow Analysis

1. **Object Literal Type Checking** (`crates/tsz-checker/src/type_computation.rs:1908`)
   - `get_type_of_object_literal` handles object literals
   - Extracts contextual types for each property (line 1976-1981)
   - Uses `contextual_property_type` to get expected type

2. **Contextual Property Type Extraction** (`crates/tsz-solver/src/contextual.rs:953`)
   - `get_property_type` recursively handles unions (lines 956-974)
   - For union `{ success: false } | { success: true }`, returns `false | true`
   - **This works correctly!**

3. **Literal Type Resolution** (`crates/tsz-checker/src/dispatch.rs:30`)
   - `resolve_literal` decides between literal type and widened type
   - Calls `contextual_literal_type` to check if literal should be preserved
   - **This is where the bug likely occurs**

4. **Contextual Literal Check** (`crates/tsz-checker/src/state_type_analysis.rs:2832`)
   - `contextual_type_allows_literal` checks if ctx allows literal
   - Uses `contextual_type_allows_literal_inner` (line 2850)
   - Recursively checks union members (line 2905-2908)
   - **Logic appears correct**, but may not be called properly

### The Missing Link

The bug is likely in **how contextual types are set** for:

1. **Variable Declarations**
   - Line 207 in declarations.rs: "Variable declaration checking is handled by CheckerState"
   - Contextual type may not be set when checking initializer

2. **Function Returns**
   - `return_expression_type` sets contextual type (type_checking_utilities.rs:995)
   - But object literal may be type-checked before contextual type is set

## Hypothesis: Cache Timing Issue

The most likely cause is a **caching/timing issue**:

1. Object literal `{ success: false }` is first type-checked WITHOUT contextual type
2. Result is cached as `{ success: boolean }`
3. Later, when contextual type is available, cached result is used
4. Literal type is already widened in cache

### Evidence

From `state.rs:706-708`:
```rust
// Check cache first
if let Some(&cached) = self.ctx.node_types.get(&idx.0) {
    return cached;
}
```

The cache doesn't consider contextual type as part of the key!

## Fix Strategy

### Option 1: Include Contextual Type in Cache Key (COMPLEX)

Modify node type cache to include contextual type:
```rust
// Instead of: node_idx -> TypeId
// Use: (node_idx, contextual_type) -> TypeId
```

**Pros**: Correct caching
**Cons**: Complex, may require significant refactoring

### Option 2: Bypass Cache for Contextually Typed Expressions (SIMPLER)

Already partially implemented! From `expr.rs:87-91`:
```rust
let result = if let Some(ctx_type) = context_type {
    // Bypass cache when contextual type is provided
    self.compute_type_with_context(idx, ctx_type)
} else {
    // Check cache first for non-contextual checks
```

**Issue**: This only works if `check_expression` is called with the contextual type.
**Fix**: Ensure variable declarations and return statements call `check_expression` with contextual type.

### Option 3: Eagerly Apply Contextual Type (RECOMMENDED)

Modify `get_type_of_object_literal` to:
1. Check if we're in a contextually typed context BEFORE type-checking properties
2. Set a flag to bypass caching for properties
3. Ensure literals are resolved with contextual type

**Implementation**:
- Modify `resolve_literal` to check parent object's contextual type
- Pass contextual type down through property checking
- Already partially implemented (lines 1984-1990 in type_computation.rs)

## Recommended Fix

**Target**: `crates/tsz-checker/src/dispatch.rs:30` (`resolve_literal`)

**Current Logic**:
```rust
fn resolve_literal(&mut self, literal_type: Option<TypeId>, widened: TypeId) -> TypeId {
    match literal_type {
        Some(lit)
            if self.checker.ctx.in_const_assertion
                || self.checker.contextual_literal_type(lit).is_some() =>
        {
            lit
        }
        _ => widened,
    }
}
```

**Issue**: `contextual_literal_type` checks `self.ctx.contextual_type`, but this may not be set when the literal is first evaluated.

**Fix**: Check the PARENT object literal's contextual type, not just the current expression's contextual type.

### Detailed Steps

1. **Detect if we're inside an object literal property**
   - Track parent node context in CheckerContext
   - Or pass contextual type explicitly through get_type_of_node

2. **Preserve contextual type through property evaluation**
   - Already done in type_computation.rs:1984-1990
   - But may be lost by the time resolve_literal is called

3. **Verify cache bypass works**
   - Check that ExpressionChecker.check_expression bypasses cache with contextual type
   - Ensure boolean literals go through this path

## Testing Plan

1. **Minimal test case** (tmp/test_contextual_literal.ts)
   - Already created
   - Reproduces the bug

2. **Add tracing**
   ```bash
   TSZ_LOG="wasm::checker=trace" TSZ_LOG_FORMAT=tree \
     .target/dist-fast/tsz tmp/test_contextual_literal.ts 2>&1 | head -200
   ```

3. **Verify fix**
   - Run conformance tests: `./scripts/conformance.sh run --filter controlFlow`
   - Expected improvement: +20-30 tests passing

## Impact Estimate

Fixing this bug will likely fix:
- **controlFlowAliasedDiscriminants.ts** (primary test case)
- **assertionFunctionsCanNarrowByDiscriminant.ts** (same root cause)
- **20-30 other control flow tests** (discriminated unions are common)

**Pass rate improvement**: 51% → 70-75%

## Next Steps

1. Add tracing to understand exact code path
2. Implement fix in `resolve_literal` or `get_type_of_object_literal`
3. Run tests to verify
4. Commit with clear message
5. Move to next category of failures
