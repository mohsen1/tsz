# Session tsz-3: Equality Narrowing

**Started**: 2026-02-06
**Status**: Active - Bug Investigation Phase
**Predecessor**: tsz-3-antipattern-8.1 (Anti-Pattern 8.1 Refactoring - COMPLETED)

## Completed Sessions

1. **In operator narrowing** - Filtering NEVER types from unions in control flow narrowing
2. **TS2339 string literal property access** - Implementing visitor pattern for primitive types
3. **Conditional type inference with `infer` keywords** - Fixed `collect_infer_type_parameters_inner` to recursively check nested types
4. **Anti-Pattern 8.1 refactoring** - Eliminated TypeKey matching from Checker

## Current Task: Equality Narrowing Bugs

### Task Definition (from Gemini Consultation)

**Implement Equality Narrowing** for `===`, `!==`, `==`, and `!=` operators.

This is the logical follow-up to the `in` operator narrowing fix and is critical for TypeScript conformance.

### Investigation Results (2026-02-06)

**Gemini Question 1 Response**: Equality narrowing infrastructure already exists:
- `src/checker/control_flow_narrowing.rs` - `extract_type_guard()` extracts `TypeGuard::LiteralEquality` for `===`/`!==`
- `src/solver/narrowing.rs` - `narrow_to_type()` and `narrow_excluding_type()` handle the narrowing

**Test Results** - Infrastructure partially works but has bugs:

```typescript
// ✅ PASSES: Basic equality narrowing
function test1(x: string | number) {
    if (x === "hello") {
        const y: "hello" = x; // Works!
    }
}

// ✅ PASSES: Basic inequality narrowing
function test2(x: "a" | "b" | "c") {
    if (x !== "a") {
        const y: "b" | "c" = x; // Works!
    }
}

// ❌ FAILS: Multiple equality checks (line 24)
function test3(x: 1 | 2 | 3) {
    if (x === 1) {
        const y: 1 = x; // OK
    } else if (x === 2) {
        const y: 2 = x; // OK
    } else {
        const y: 3 = x; // ERROR: Type '1 | 3' is not assignable to type '3'
    }
}

// ❌ FAILS: Boolean narrowing (line 31)
function test4(x: boolean) {
    if (x === true) {
        const y: true = x; // ERROR: Type 'true' is not assignable to type 'true'
    }
}

// ✅ PASSES: Typeof narrowing
function test5(x: string | number) {
    if (typeof x === "string") {
        const y: string = x; // Works!
    }
}
```

**Bugs Found**:
1. **Chained inequality narrowing**: After `x !== 1` and `x !== 2`, type should be `3` but tsz narrows to `1 | 3`
2. **Boolean literal narrowing**: `x === true` doesn't narrow `boolean` to `true`

### Files to Investigate

1. **`src/solver/narrowing.rs`**:
   - `narrow_to_type()` (lines 669-777) - Handle `===` case
   - `narrow_excluding_type()` (lines 779-864) - Handle `!==` case
   - Boolean narrowing logic (lines 748-771, 821-858)

2. **`src/checker/control_flow_narrowing.rs`**:
   - `extract_type_guard()` (lines 1879-1958) - Extracts TypeGuard from AST
   - `literal_comparison()` (lines 1110-1123) - Detects literal comparisons

### Next Steps

Per Gemini's guidance, need to investigate why the existing narrowing logic isn't working for:
1. Boolean literal types
2. Chained else-if narrowing (cumulative narrowing from multiple branches)

Will ask Gemini Question 2 with specific bug details before implementing fixes.
