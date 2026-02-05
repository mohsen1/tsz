# Session TSZ-12: Advanced Narrowing & Type Predicates

**Started**: 2026-02-05
**Status**: 🔄 ACTIVE (Planning Phase)

## Goal

Complete CFA narrowing parity with TypeScript by implementing:
1. The `in` operator narrowing
2. User-Defined Type Guards (`is` predicates)
3. Exhaustiveness checking (switch/if-else chains)

## Context

Previous sessions successfully implemented:
- ✅ TSZ-10: Narrowing infrastructure (truthiness, typeof, instanceof, discriminant unions, assertion functions)
- ✅ TSZ-11: Fixed instanceof narrowing by removing is_narrowable_type check

Current state: Core narrowing works, but TypeScript has more advanced features needed for full parity.

## Scope

### Task 1: `in` Operator Narrowing

Implement narrowing for property existence checks:
```typescript
function test(obj: { prop?: string } | { other: number }) {
    if ("prop" in obj) {
        obj.prop; // Should work - narrowed to first member
    }
}
```

**Files**:
- `src/solver/narrowing.rs` - Add narrow_by_in_operator logic
- `src/checker/expr.rs` - Handle binary `in` expressions

**Implementation approach** (Mandatory Gemini Question 1):
- Create new narrowing logic for property presence
- Handle unions where some members have the property
- Handle narrowing by absence (else branch)

### Task 2: User-Defined Type Guards

Implement support for `is` type predicates:
```typescript
function isString(x: unknown): x is string {
    return typeof x === "string";
}

function test(x: unknown) {
    if (isString(x)) {
        x.toUpperCase(); // Should work - narrowed to string
    }
}
```

**Files**:
- `src/checker/expr.rs` - Detect TypePredicate in function signatures
- `src/solver/narrowing.rs` - Apply predicate to flow-sensitive type

**Implementation approach**:
- Extract TypePredicate from call signatures
- Wire into CFA to narrow the argument expression
- Handle both `arg is T` and `asserts arg is T`

### Task 3: Exhaustiveness Checking

Ensure `never` is correctly inferred:
```typescript
type Shape = { kind: "circle" } | { kind: "square" };

function test(shape: Shape) {
    switch (shape.kind) {
        case "circle": return 1;
        case "square": return 2;
    }
    // Should error: shape is never here (exhaustive check)
}
```

**Files**:
- `src/checker/statements.rs` - Switch statement analysis
- `src/solver/narrowing.rs` - Never type inference

## Plan

### Phase 1: Architecture Validation (MANDATORY GEMINI)

Task 1: `in` operator approach validation
- Ask: "How should I implement `in` operator narrowing? Should I create a new NarrowingKind? How do I handle the else case (narrowing by absence)?"

Task 2: Type guard approach validation  
- Ask: "How should I implement user-defined type guards? Where do I extract the TypePredicate from function signatures?"

### Phase 2: Implementation

Task 3: Implement `in` operator narrowing
Task 4: Implement user-defined type guards
Task 5: Implement exhaustiveness checking

### Phase 3: Verification

Task 6: Test with real-world TypeScript code
Task 7: Run conformance tests for narrowing

## Risks

1. **Property presence checking**: Requires analyzing type shapes to determine if a property exists
2. **Type predicate extraction**: Need to correctly parse function signatures for `is` predicates
3. **Performance**: `in` operator checks could be expensive if not cached properly

## Success Criteria

1. ✅ `in` operator narrowing works
2. ✅ User-defined type guards work
3. ✅ Exhaustiveness checking detects when types are narrowed to `never`
4. ✅ All features match TypeScript behavior
5. ✅ No regressions in existing narrowing

## References

- Previous Sessions: docs/sessions/history/tsz-10.md, tsz-11.md
- North Star: docs/architecture/NORTH_STAR.md
- Narrowing Logic: src/solver/narrowing.rs
- Expression Checking: src/checker/expr.rs

---
**AGENTS.md Reminder**: All solver/checker changes require two-question Gemini consultation.
