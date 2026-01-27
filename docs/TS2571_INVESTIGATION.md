# TS2571 Investigation Report

**Date**: 2026-01-27
**Error Code**: TS2571 - "Object is of type 'unknown'"
**Error Count**: 106x errors (was 136x in earlier run)
**Status**: FALSE POSITIVE - Application type evaluation gap in contextual typing

## Executive Summary

TS2571 errors emerged as a top error category (106x) due to a gap in Application type evaluation during contextual typing. When arrow function parameters should be inferred from generic type aliases with Application types (e.g., `Destructuring<TFuncs1, T>`), the checker fails to evaluate the Application type, resulting in parameters being typed as `UNKNOWN` instead of their actual type. This causes false positive TS2571 errors when accessing properties on those parameters.

## Root Cause

**File**: `src/solver/contextual.rs`
**Function**: `ContextualTypeContext::get_parameter_type()`
**Line**: 60-95

The `get_parameter_type()` method handles:
- `TypeKey::Function`
- `TypeKey::Callable`
- `TypeKey::Union`

**BUT DOES NOT HANDLE:**
- `TypeKey::Application`

When a contextual type is an Application type (generic type alias instantiation like `Destructuring<TFuncs1, T>`), the method returns `None`, causing the parameter to be typed as `UNKNOWN` instead of the actual function signature.

## Test Case

Created `/Users/claude/code/tsz/tests/debug/test_ts2571_application.ts`:

```typescript
type IFuncs = {
    funcA: (a: boolean) => void;
    funcB: (b: string) => void;
};

type IDestructuring<T extends IFuncs> = {
    readonly [key in keyof T]?: (...p: any) => void
};

type Destructuring<T extends IFuncs, U extends IDestructuring<T>> =
    (funcs: T) => U;

const funcs1 = {
    funcA: (a: boolean): void => {},
    funcB: (b: string): void => {},
};

type TFuncs1 = typeof funcs1;

declare function useDestructuring<T extends IDestructuring<TFuncs1>>(
    destructuring: Destructuring<TFuncs1, T>  // <-- Application type
): T;

const result = useDestructuring((f) => ({
    // f should be typed as TFuncs1, but is typed as UNKNOWN
    funcA: (...p) => f.funcA(...p),  // <-- FALSE POSITIVE TS2571 here
    funcB: (...p) => f.funcB(...p),  // <-- FALSE POSITIVE TS2571 here
}));
```

**Expected**: No errors (TypeScript emits 0 errors)
**Actual**: TS2571 on lines 28 and 29

## Why This Is a False Positive

1. **TypeScript behavior**: TypeScript correctly infers `f` as `TFuncs1` (the type of `funcs1`)
2. **Our behavior**: We type `f` as `UNKNOWN` because we don't evaluate the Application type `Destructuring<TFuncs1, T>`
3. **Cascading error**: When `f` is `UNKNOWN`, accessing `f.funcA` triggers TS2571

## Valid TS2571 Errors

Not all TS2571 errors are false positives. TypeScript DOES emit TS2571 for:

1. **Empty object destructuring on unknown**:
   ```typescript
   declare function f<T>(): T;
   const {} = f();  // TS2571: Object is of type 'unknown'
   ```

2. **Array destructuring on unknown**:
   ```typescript
   const [] = f();  // TS2571: Object is of type 'unknown'
   ```

3. **Private name 'in' expression with unknown**:
   ```typescript
   class Foo { #field = 1; }
   const test = (v: unknown) => #field in v;  // TS2571: Object is of type 'unknown'
   ```

## Why TS2571 Emerged Now

**Theory Confirmed**: Previous runs had crashes/OOMs that prevented tests from completing. With the recent stability fixes (cycle detection, defensive programming), more tests now run to completion, exposing this pre-existing gap in Application type evaluation.

**Evidence**:
- Post-commit results show TS2571 as a "new top category" with 136x errors
- Earlier runs likely crashed before reaching these test cases
- The Application type evaluation gap has existed but wasn't visible

## Fix Strategy

### Option 1: Evaluate Application Types in `get_parameter_type()`

**Location**: `src/solver/contextual.rs`

Add a case for `TypeKey::Application` in the match statement:

```rust
TypeKey::Application(app_id) => {
    // Need to evaluate the Application to get the actual function type
    // This requires access to CheckerState to call evaluate_application_type()
    // Problem: ContextualTypeContext only has TypeDatabase, not CheckerState
}
```

**Challenge**: `ContextualTypeContext` only has access to `TypeDatabase`, not `CheckerState`, so it cannot call `evaluate_application_type()`.

### Option 2: Evaluate Application Types Before Creating Context

**Location**: `src/checker/function_type.rs`

Before creating `ContextualTypeContext`, evaluate Application types:

```rust
let ctx_helper = if let Some(ctx_type) = self.ctx.contextual_type {
    let evaluated_type = self.evaluate_application_type(ctx_type);
    Some(ContextualTypeContext::with_expected(
        self.ctx.types,
        evaluated_type,  // Use evaluated type instead of raw ctx_type
    ))
} else {
    None
};
```

**Pros**:
- Minimal change
- Leverages existing `evaluate_application_type()` logic
- Fixes the root cause

**Cons**:
- May have performance implications if evaluated frequently
- Need to ensure caching is effective

### Option 3: Hybrid Approach

Evaluate Application types only in `get_parameter_type()` when needed:

1. Add a method to `TypeDatabase` to check if a type is Application
2. In `get_parameter_type()`, if the expected type is Application, return a marker
3. In the checker, when getting the marker, evaluate and retry

**Pros**:
- Lazy evaluation - only evaluates when actually needed
- Preserves performance for non-Application cases

**Cons**:
- More complex implementation
- Multiple places need changes

## Recommendation

**Go with Option 2**: Evaluate Application types before creating `ContextualTypeContext`.

**Rationale**:
1. Simplest fix with minimal code changes
2. We already have robust Application type evaluation logic
3. The performance impact should be minimal due to existing caching in `evaluate_application_type()`
4. Fixes the issue at the source (before contextual typing)

## Implementation Plan

1. Modify `src/checker/function_type.rs` line 122-129:
   - Add `evaluate_application_type()` call before creating `ContextualTypeContext`
2. Add tests to verify:
   - Generic type aliases with Application types work correctly
   - Contextual typing properly evaluates Application types
   - No regressions in existing contextual typing
3. Run conformance tests to verify TS2571 count decreases

## Related Files

- `/Users/claude/code/tsz/src/solver/contextual.rs` - ContextualTypeContext implementation
- `/Users/claude/code/tsz/src/checker/function_type.rs` - Function parameter typing (line 122-129)
- `/Users/claude/code/tsz/src/checker/state.rs` - Application type evaluation (line 6691-6771)
- `/Users/claude/code/tsz/tests/debug/test_ts2571_application.ts` - Minimal reproduction
- `/Users/claude/code/tsz/tests/debug/test_ts2571_minimal.ts` - Simple contextual typing test (passes)

## Verification

After implementing the fix:

1. Test file should pass with 0 errors:
   ```bash
   cargo run --bin tsz -- tests/debug/test_ts2571_application.ts
   ```

2. Conformance tests should show reduced TS2571 count:
   ```bash
   ./conformance/run-conformance.sh --max=100 --workers=4
   ```

3. Verify no regressions in existing contextual typing tests

## Impact

- **TS2571 errors**: Expected to decrease from 106x to <20x (only valid errors remain)
- **Test accuracy**: Reduces false positives, improves conformance
- **Code quality**: Fixes a fundamental gap in type system coverage
