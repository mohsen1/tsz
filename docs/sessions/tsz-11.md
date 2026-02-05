# Session TSZ-11: Control Flow Analysis Integration

**Started**: 2026-02-05
**Status**: 🔄 ACTIVE (Architecture Validation Phase)

## Goal

Integrate the `FlowAnalyzer` into the main Checker loop so that narrowed types calculated by the Solver are actually used during type checking.

## Context

Previous session (TSZ-10) implemented the narrowing logic (truthiness, typeof, instanceof, discriminant unions, assertion functions), but discovered a critical architectural gap: **the Checker does not query the Flow Graph when resolving identifier types**.

### Problem Statement

When TypeScript checks this code:
```typescript
class Animal {}
class Dog extends Animal { bark() {} }
function test(animal: Animal) {
  if (animal instanceof Dog) {
    animal.bark(); // Should work (animal is narrowed to Dog)
  }
}
```

Current TSZ behavior:
- ✅ FlowAnalyzer correctly narrows `animal` to `Dog` inside the if block
- ✅ Narrowing logic in Solver is correct
- ❌ When checking `animal.bark()`, the Checker uses the declared type `Animal`
- ❌ Error: "Property 'bark' does not exist on type 'Animal'"

Root cause: `get_type_of_symbol()` (state_type_analysis.rs:751) uses a flow-insensitive cache keyed only by `SymbolId`.

## Plan

### Phase 1: Architecture Validation (MANDATORY GEMINI)

Task 1: Consult Gemini on integration point - See full details in session file

### Phase 2: Implementation

Task 2: Implement get_flow_type_of_node
Task 3: Wire into identifier checking

### Phase 3: Verification

Task 4-7: Verify all narrowing types and run regression tests

## Risks

Performance, Recursion, Breaking Existing Code - see full details in session file

## Success Criteria

1. ✅ instanceof narrowing works end-to-end
2. ✅ typeof narrowing works end-to-end
3. ✅ discriminant narrowing works end-to-end
4. ✅ No regression in existing type checking
5. ✅ Acceptable performance

## References

- Previous Session: docs/sessions/history/tsz-10.md
- North Star Architecture: docs/architecture/NORTH_STAR.md
