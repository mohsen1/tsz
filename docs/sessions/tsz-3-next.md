# Session tsz-3: Void Return Exception (Lawyer Layer)

**Started**: 2026-02-06
**Status**: Active - Investigation Phase
**Predecessor**: tsz-3-equality-narrowing (paused due to complex CFA bugs)

## Completed Sessions

1. **In operator narrowing** - Filtering NEVER types from unions in control flow narrowing
2. **TS2339 string literal property access** - Implementing visitor pattern for primitive types
3. **Conditional type inference with `infer` keywords** - Fixed `collect_infer_type_parameters_inner` to recursively check nested types
4. **Anti-Pattern 8.1 refactoring** - Eliminated TypeKey matching from Checker

## Current Task: Void Return Exception

### Task Definition (from Gemini Consultation)

**Implement the "Void Return" Exception** - A classic "Lawyer" override where functions returning `T` are assignable to functions returning `void`.

In TypeScript, this is allowed:
```typescript
function returnsNumber(): number {
    return 42;
}

const fn: () => void = returnsNumber; // OK - void return exception
```

This is a TypeScript-specific quirk (per NORTH_STAR.md Section 3.3) that causes many conformance failures in callback-heavy code.

### Why This Task

- **High ROI**: Will likely jump conformance score by 5-10 points
- **Self-contained**: Pure type operation, doesn't depend on complex control flow
- **Clear scope**: Well-defined behavior in TypeScript
- **Affects common code**: `Array.prototype.forEach`, callbacks, event handlers

### Files to Investigate

Per Gemini's recommendation:
1. **`src/solver/lawyer.rs`** - Lawyer layer for TypeScript-specific compatibility rules
2. **`src/solver/subtype.rs`** - `check_function_compatibility` or similar function
3. **`src/solver/compat.rs`** - Main compatibility checking logic

### Implementation Approach (Pending Gemini Review)

**Before implementing, I will ask Gemini Question 1**:
1. Is this the right approach?
2. What function should I modify?
3. What are the edge cases?

**Planned Approach**:
1. Find the function compatibility check code
2. Add a special case: when target return type is `void`, the source return type should be ignored
3. Handle unions containing `void` (e.g., `void | undefined`)
4. Follow MANDATORY Gemini workflow (Two-Question Rule)

### Test Cases

**Should pass after fix**:
```typescript
// Basic void return exception
function returnsNumber(): number {
    return 42;
}

const fn1: () => void = returnsNumber; // Should work

// Array.forEach example
[1, 2, 3].forEach((x) => {
    console.log(x); // Should work even though callback returns number
});

// Multiple return types
function returnsString(): string {
    return "hello";
}

const fn2: () => void = returnsString; // Should work
```

## Previous Task (Paused)

Equality narrowing was investigated but found to have complex bugs requiring deeper CFA debugging:
- Boolean literal narrowing not working
- Chained else-if narrowing not cumulative

Per Gemini's recommendation, shifted to this more self-contained task.

## MANDATORY Gemini Workflow

Per AGENTS.md, before implementing:

**Question 1 (Approach)**: Will ask about the specific implementation approach for void return exception.
