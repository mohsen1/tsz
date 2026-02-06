# Session tsz-3: Equality Narrowing

**Started**: 2026-02-06
**Status**: Active - Investigation Phase
**Predecessor**: tsz-3-antipattern-8.1 (Anti-Pattern 8.1 Refactoring - COMPLETED)

## Completed Sessions

1. **In operator narrowing** - Filtering NEVER types from unions in control flow narrowing
2. **TS2339 string literal property access** - Implementing visitor pattern for primitive types
3. **Conditional type inference with `infer` keywords** - Fixed `collect_infer_type_parameters_inner` to recursively check nested types
4. **Anti-Pattern 8.1 refactoring** - Eliminated TypeKey matching from Checker

## Current Task: Equality Narrowing

### Task Definition (from Gemini Consultation)

Based on review of NORTH_STAR.md and docs/sessions/, the next priority task is:

**Implement Equality Narrowing** for `===`, `!==`, `==`, and `!=` operators.

This is the logical follow-up to the `in` operator narrowing fix and is critical for TypeScript conformance.

### Problem

TypeScript narrows union types when checking equality/inequality against literals:
```typescript
function foo(x: string | number) {
    if (x === "hello") {
        // x should be narrowed to the literal type "hello"
        console.log(x.toUpperCase()); // Should work
    }
}
```

Currently, tsz may not be narrowing unions correctly in equality/inequality checks.

### Files to Investigate

Per Gemini's recommendation:
1. **`src/solver/narrowing.rs`** - Main narrowing logic
   - Function: `narrow_type_by_binary_expression` (or similar)
   - Need to add equality/inequality narrowing cases
2. **`src/checker/flow_analysis.rs`** - Control flow analysis integration
3. **`src/solver/control_flow_narrowing.rs`** - May have existing narrowing infrastructure

### Implementation Approach (Pending Gemini Review)

**Before implementing, I will ask Gemini Question 1**:
1. Is this the right approach?
2. What functions should I modify?
3. What are the edge cases?

**Planned Approach**:
1. Find failing conformance tests for equality narrowing
2. Locate the narrowing function for binary expressions
3. Add equality/inequality narrowing logic:
   - `x === "literal"` should narrow x to that literal type
   - `x !== "literal"` should filter out that literal type from the union
   - Handle both `===`/`!==` (strict) and `==`/`!=` (loose) operators
4. Follow MANDATORY Gemini workflow (Two-Question Rule)

### Test Cases

**Should pass after fix**:
```typescript
// Equality narrowing with ===
function test1(x: string | number) {
    if (x === "hello") {
        // x should be narrowed to "hello"
    }
}

// Inequality narrowing with !==
function test2(x: "a" | "b" | "c") {
    if (x !== "a") {
        // x should be narrowed to "b" | "c"
    }
}

// Multiple equality checks
function test3(x: 1 | 2 | 3) {
    if (x === 1) {
        // x should be narrowed to 1
    } else if (x === 2) {
        // x should be narrowed to 2
    } else {
        // x should be narrowed to 3
    }
}
```

## MANDATORY Gemini Workflow

Per AGENTS.md, before implementing:

**Question 1 (Approach)**:
```bash
./scripts/ask-gemini.mjs --include=src/solver/narrowing.rs \
  "I need to implement Equality Narrowing for ===, !==, ==, != operators.

My plan:
1. Add cases for equality/inequality operators in the narrowing logic
2. When x === literal, narrow x to that literal type
3. When x !== literal, filter out that literal from the union
4. Handle both strict (===/!==) and loose (==/!=) operators

Is this the correct approach? What function should I modify?
What are the edge cases (e.g., discriminated unions, type guards)?"
```

**Question 2 (Review)**: After implementation, submit for review.
