# Session TSZ-14: Fix Literal Type Widening in Object Literal Expression Inference

**Started**: 2026-02-05
**Status**: 🔄 PENDING
**Focus**: Fix object literal expression inference to respect contextual types

## Problem Statement

**Discovered during tsz-1 investigation**:

Object literal expressions are being inferred without considering the contextual type from function parameters, causing literals to be widened to their primitive types.

### Test Case
```typescript
type A = { kind: "a", value: number };
type B = { kind: "b", value: string };

function test(obj: A | B) {
    if (obj.kind === "a") {
        const val: number = obj.value; // Should work - obj narrowed to A
    }
}
```

### Actual Investigation Findings (2026-02-05)

**IMPORTANT**: The bug is NOT in type alias lowering!

1. **Type Alias Lowering (SOLVER)**: ✅ Working correctly
   - `lower_literal_type` in `src/solver/lower.rs` creates literal types correctly
   - Debug confirms: `StringLiteral found, text='a', creating literal`
   - Type alias `A` is correctly stored as `{ kind: "a" }`

2. **Expression Inference (CHECKER)**: ❌ Bug is here
   - Argument expression `{ kind: "a" }` is inferred as `{ kind: string }`
   - Error message: `Argument of type '{ kind: string }' is not assignable to parameter of type 'A'`
   - The object literal is inferred WITHOUT considering the contextual type from parameter `A`

### Current (Buggy) Behavior
- Object literal `{ kind: "a" }` is inferred as `{ kind: string }` (widened)
- Inference ignores the contextual type from function parameter `A`
- Assignment fails because `{ kind: string }` doesn't match `{ kind: "a" }`

### Expected Behavior (matches tsc)
- Object literal `{ kind: "a" }` should be inferred as `{ kind: "a" }` (literal preserved)
- Inference should respect the contextual type from function parameter `A`
- Assignment should succeed because types match

## Why This Matters

TypeScript preserves literal types in expressions when there's a contextual type:

```typescript
type A = { kind: "a" };
function test(obj: A) {}

test({ kind: "a" }); // Should work - { kind: "a" } inferred, not { kind: string }
```

If expressions don't respect contextual types:
1. **Literal types are lost** - even when target type expects literals
2. **Type safety is reduced** - expressions become less precise
3. **Compatibility with tsc breaks** - TypeScript preserves literals in contextual inference

## Why This Matters

Literal types in type definitions must NOT be widened. This is a fundamental TypeScript rule:

```typescript
type T = "a";  // Must remain literal "a", NOT widened to string
type U = 1;    // Must remain literal 1, NOT widened to number
type V = true; // Must remain literal true, NOT widened to boolean
```

If literals are widened:
1. **Discriminant narrowing breaks** - all union members match the same literal
2. **Type safety is lost** - types become less precise than intended
3. **Compatibility with tsc breaks** - TypeScript preserves literals in type definitions

## Success Criteria

### Test Case 1: String Literals
```typescript
type A = { kind: "a" };
function test(obj: A) {
    const k: "a" = obj.kind; // Should work
}
```

### Test Case 2: Number Literals
```typescript
type B = { count: 42 };
function test(obj: B) {
    const c: 42 = obj.count; // Should work
}
```

### Test Case 3: Boolean Literals
```typescript
type C = { flag: true };
function test(obj: C) {
    const f: true = obj.flag; // Should work
}
```

### Test Case 4: Union with Literal Discriminants
```typescript
type D = { type: "circle"; radius: number }
  | { type: "square"; side: number };

function test(obj: D) {
    if (obj.type === "circle") {
        const r: number = obj.radius; // Should work - obj narrowed to first member
    }
}
```

## Implementation Plan

### Phase 1: Locate the Bug

**File**: `src/solver/lower.rs` (primary suspect)

**Tasks**:
1. Find the function that lowers AST nodes to `TypeId`
2. Locate the handling of `LiteralType` nodes
3. Identify where literals are being widened to primitives
4. Check if there's special handling needed vs. variable declarations

### Phase 2: Understand the Correct Approach

**MANDATORY**: Ask Gemini PRE-implementation question:
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver/lower.rs "
I am starting session tsz-14.
Problem: Literal types in type aliases (e.g., type T = 'a') are being widened to 'string'.
I suspect the issue is in src/solver/lower.rs where LiteralType nodes are lowered.

Questions:
1. How are LiteralType nodes currently being lowered?
2. What is the correct way to intern a specific string literal TypeId?
3. Are there any existing patterns for handling literal types in the codebase?
4. What edge cases should I watch out for?

Please provide: file paths, function names, and implementation guidance.
"
```

### Phase 3: Implementation

**Expected Fix** (to be validated with Gemini):
- Modify lowering logic to distinguish `LiteralType` from primitive types
- For string literals: Extract text, create `TypeKey::Literal(LiteralValue::String(atom))`
- For number literals: Extract value, create `TypeKey::Literal(LiteralValue::Number(n))`
- For boolean literals: Return `TypeId::BOOLEAN_TRUE` or `TypeId::BOOLEAN_FALSE`

**Edge Cases**:
- Template literal types with no substitutions
- Negative number literals
- Union literal types (`"a" | "b"`)

### Phase 4: Validation

1. Add unit tests for literal type lowering
2. Test all success criteria above
3. Verify tsc compatibility
4. Run full test suite to check for regressions

## MANDATORY Gemini Workflow

Per AGENTS.md and CLAUDE.md, **MUST ask Gemini TWO questions**:

### Question 1 (PRE-implementation) - REQUIRED
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver/lower.rs "
I am starting session tsz-14: Fix Literal Type Widening in Type Aliases.

Problem: Literal types in type alias definitions are being widened to primitives.
Example: type T = { kind: \"a\" } results in kind: string instead of kind: \"a\".

I suspect the bug is in src/solver/lower.rs where LiteralType nodes are lowered.

Questions:
1. How are LiteralType nodes currently being lowered?
2. What functions should I use to create literal TypeIds?
3. Are there any existing patterns for handling literal types in the codebase?
4. What edge cases should I watch out for?

Please provide: file paths, function names, and implementation guidance.
"
```

### Question 2 (POST-implementation) - REQUIRED
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver/lower.rs "
I fixed the literal type widening bug in [FILE].

Changes: [PASTE CODE OR DESCRIBE CHANGES]

Please review:
1) Is this correct for TypeScript's type lowering semantics?
2) Did I miss any edge cases (template literals, negative numbers, etc.)?
3) Are there type system bugs?

Be specific if it's wrong - tell me exactly what to fix.
"
```

## Dependencies

- **tsz-1**: Fix and Harden Discriminant Narrowing (COMPLETE) - discovered this issue

## Related Sessions

- **tsz-1**: Fix and Harden Discriminant Narrowing (COMPLETE)
- **tsz-2**: Coinductive Subtyping (COMPLETE)

## Related Issues

- This bug prevents discriminant narrowing from working correctly in tsz
- Affects type precision for any type alias with literal members
