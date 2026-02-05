# Session TSZ-1: Fix and Harden Discriminant Narrowing

**Started**: 2026-02-05
**Status**: 🔄 PENDING
**Focus**: Fix 3 critical bugs in discriminant narrowing implementation

## Problem Statement

**From AGENTS.md Investigation (2026-02-04)**:

Commit `f2d4ae5d5` (discriminant narrowing) was found to have **3 CRITICAL BUGS**:

1. **Reversed Subtype Check**: Asked `is_subtype_of(property_type, literal)` instead of `is_subtype_of(literal, property_type)`
2. **Missing Type Resolution**: Didn't handle `Lazy`/`Ref`/`Intersection` types
3. **Broken for Optional Properties**: Failed on `{ prop?: "a" }` cases

**Why This Matters**:
Discriminant narrowing is how TypeScript narrows union types in control flow:
```typescript
function process(value: { kind: "a" } | { kind: "b" }) {
    if (value.kind === "a") {
        // value should be narrowed to { kind: "a" }
    }
}
```

If narrowing is broken, type safety is compromised.

## Success Criteria

### Test Case 1: Literal Discriminant
```typescript
type A = { kind: "a", value: number };
type B = { kind: "b", value: string };

function process(obj: A | B) {
    if (obj.kind === "a") {
        const val: number = obj.value; // Should work - obj narrowed to A
    }
}
```

### Test Case 2: Optional Properties
```typescript
type WithOptional = { kind?: "a"; prop: string };

function process(obj: WithOptional) {
    if (obj.kind === "a") {
        // Should narrow correctly even with optional discriminant
        console.log(obj.prop);
    }
}
```

### Test Case 3: Nested/Lazy Types
```typescript
type Nested = { data: { kind: "a" } };

function process(obj: { kind: "a" } | Nested) {
    if (obj.kind === "a") {
        // Should handle Lazy/Ref resolution correctly
    }
}
```

## Implementation Plan

### Phase 1: Investigation & Root Cause Analysis

**File**: `src/solver/narrowing.rs`

**Tasks**:
1. Find the discriminant narrowing code added in commit `f2d4ae5d5`
2. Identify the exact lines with the 3 bugs
3. Write failing tests for each bug
4. Document current behavior vs expected behavior

### Phase 2: Fix the Bugs

**Bug 1: Reversed Subtype Check**
- **Issue**: `is_subtype_of(property_type, literal)` should be `is_subtype_of(literal, property_type)`
- **Fix**: Swap arguments in subtype check
- **Validation**: Narrowing should succeed when literal matches property type

**Bug 2: Missing Type Resolution**
- **Issue**: Code doesn't handle `Lazy`/`Ref`/`Intersection` types
- **Approach**: Use visitor pattern from `src/solver/visitor.rs`
- **Fix**: Add resolution steps for complex types before matching
- **Validation**: Test with nested and generic types

**Bug 3: Optional Properties**
- **Issue**: Fails when discriminant property is optional
- **Fix**: Handle optional discriminants correctly
- **Validation**: Test with `{ kind?: "a" }` cases

### Phase 3: Validation & Testing

**Tasks**:
1. Run all narrowing tests to ensure no regressions
2. Test with real-world code patterns
3. Ask Gemini Pro for POST-implementation review
4. Compare behavior with tsc on conformance tests

## MANDATORY Gemini Workflow

Per AGENTS.md, **MUST ask Gemini TWO questions**:

### Question 1 (PRE-implementation) - REQUIRED
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver/narrowing.rs --include=src/checker/flow_analysis.rs "
I'm starting tsz-1: Fix and Harden Discriminant Narrowing.

Problem: Commit f2d4ae5d5 has 3 critical bugs:
1. Reversed subtype check (asked is_subtype_of(property_type, literal) instead of is_subtype_of(literal, property_type))
2. Missing type resolution for Lazy/Ref/Intersection types
3. Broken for optional properties ({ prop?: \"a\" })

My planned approach:
1) Find the discriminant narrowing code in src/solver/narrowing.rs
2) Identify the exact lines with the bugs
3) Fix each bug systematically
4) Test with tsc comparison

Questions:
1) Where is the discriminant narrowing code in narrowing.rs?
2) Should I use visitor pattern or direct TypeKey matching?
3) How do I resolve Lazy/Ref/Intersection types before narrowing?
4) How do I handle optional discriminant properties?

Please provide: file paths, function names, line numbers, and implementation guidance.
"
```

### Question 2 (POST-implementation) - REQUIRED
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver/narrowing.rs "
I fixed the 3 discriminant narrowing bugs in [FILE].

Changes: [PASTE CODE OR DESCRIBE CHANGES]

Please review:
1) Is this logic correct for TypeScript narrowing semantics?
2) Did I miss any edge cases?
3) Are there type system bugs?

Be specific if it's wrong - tell me exactly what to fix.
"
```

## Dependencies

- **tsz-4**: Strict Null Checks & Lawyer Layer (COMPLETE)
- **tsz-2**: Coinductive Subtyping (COMPLETE)
- **tsz-5/6/13**: Inference & Member Resolution (COMPLETE)

## Related Sessions

- **tsz-3**: CFA Stabilization (BLOCKED) - may have related narrowing issues
- **tsz-11**: Truthiness & Equality Narrowing (Active)

## Implementation Notes

### Key Principles

1. **Use Visitor Pattern**: Per NORTH_STAR.md 8.1, always prefer visitor pattern over direct TypeKey matching
2. **Test with tsc**: Every fix must match tsc behavior exactly
3. **No regressions**: Ensure existing narrowing still works

### Files to Investigate

- `src/solver/narrowing.rs` - Main narrowing logic
- `src/checker/flow_analysis.rs` - Control flow analysis
- `src/checker/control_flow_narrowing.rs` - Narrowing orchestration
- `src/solver/visitor.rs` - Type resolution helpers

## Session History

Created 2026-02-05 following completion of tsz-13 (Type Inference - discovery that it was already implemented).
