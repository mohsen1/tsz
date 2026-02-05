# IIFE Narrowing Bug

**Status**: NEEDS FIX
**Discovered**: 2026-02-05
**Component**: Checker (control_flow.rs)
**Conformance Impact**: ~190 extra TS2339 errors, multiple false positives

## Problem

tsz does not preserve type narrowing inside IIFEs (Immediately Invoked Function Expressions). When a variable is narrowed via a type guard (like `typeof`), and then accessed inside an IIFE, tsz incorrectly resets the narrowed type.

### Test Case

```typescript
// @strictNullChecks: true
declare function getStringOrNumber(): string | number;

function f() {
    let x = getStringOrNumber();
    if (typeof x === "string") {
        // Inside the IIFE, x should still be string
        let n = (function() {
            return x.length;  // tsz ERROR: Property 'length' does not exist on 'number | string'
        })();
    }
}
```

### Expected vs Actual

| Test | TSC Errors | tsz Errors |
|------|-----------|------------|
| controlFlowIIFE.ts | 2 | 10 (8 false positives) |

TSC correctly:
- Preserves `string` type for `x` inside IIFEs
- Only errors on lines 67 and 75 (unrelated issues)

tsz incorrectly:
- Resets `x` to `number | string` inside IIFEs
- Reports TS2339 "Property 'length' does not exist on 'number | string'"
- Also reports extra TS7006, TS2454, TS18050, TS18048 errors

## Root Cause

The issue is in `src/checker/control_flow.rs` around line 624:

```rust
// Bug #1.2 fix: Check if the reference is a CAPTURED mutable variable
// Only reset narrowing for captured mutable variables, not local ones
if self.is_mutable_variable(reference) && self.is_captured_variable(reference) {
    // Captured mutable variable - cannot use narrowing from outer scope
    // Return the initial (declared) type instead of crossing boundary
    initial_type
}
```

This code resets narrowing for **all** captured mutable variables. However, TypeScript distinguishes between:

1. **Regular closures** (callbacks that may be called later): Narrowing is reset because the callback could be called after the variable is reassigned.

2. **IIFEs** (Immediately Invoked Function Expressions): Narrowing is **preserved** because the function is called immediately, before any potential reassignment.

## Fix Approach

The fix needs to:

1. **Detect if the enclosing function is an IIFE**:
   - Find the enclosing function/arrow expression for the reference
   - Check if that function is the callee of a CallExpression (i.e., `(function() { ... })()` or `(() => { ... })()`)

2. **Preserve narrowing for IIFEs**:
   - Modify the check at line 624 to be:
   ```rust
   if self.is_mutable_variable(reference)
       && self.is_captured_variable(reference)
       && !self.is_in_immediately_invoked_function(reference) {
       // Reset narrowing only for non-IIFE captured mutable variables
   }
   ```

### Implementation Details

To implement `is_in_immediately_invoked_function(reference: NodeIndex) -> bool`:

1. Find the enclosing function node for `reference`
2. Get the parent of that function node
3. Check if parent is a `CallExpression` where the function is the callee
4. Also handle `ParenthesizedExpression` wrappers: `(function() {})()` vs `function() {}()`

### Edge Cases to Handle

1. **Arrow IIFEs**: `(() => x.length)()`
2. **Parenthesized IIFEs**: `(function() { return x.length; })()`
3. **Arrow with parameters**: `((z) => x.length + z)(1)`
4. **Nested IIFEs**: IIFE inside IIFE
5. **Assignment inside IIFE**:
   ```typescript
   (() => {
       x = 42;  // If IIFE assigns to x, should narrowing still be preserved?
   })();
   ```
   This may need special handling - TSC may invalidate narrowing if the IIFE contains assignments to the captured variable.

## Testing

### Files Affected

- `TypeScript/tests/cases/conformance/controlFlow/controlFlowIIFE.ts`
- Many `privateNames*` tests
- Many `dynamicImport*` tests

### Verification Steps

1. Run: `./.target/release/tsz TypeScript/tests/cases/conformance/controlFlow/controlFlowIIFE.ts --noEmit --strictNullChecks true --target es2017`
2. Compare with: `npx tsc TypeScript/tests/cases/conformance/controlFlow/controlFlowIIFE.ts --noEmit --strictNullChecks --target es2017`
3. Expected: Same number of errors (2)

## Gemini Consultation Required

Per CLAUDE.md, any changes to `src/checker/*.rs` require Gemini consultation:

1. **Pre-implementation**: Ask Gemini to review the approach
2. **Post-implementation**: Ask Gemini to review the code changes

This document was created when Gemini was unavailable (API fetch failed). Implementation should wait for Gemini availability.

## Related Work

- Rule #42 (closure narrowing for mutable variables)
- `is_captured_variable()` in `control_flow_narrowing.rs`
- `is_mutable_variable()` in `control_flow_narrowing.rs`
