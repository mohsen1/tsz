# Session TSZ-8: Conditional Type Inference (infer Keyword)

**Started**: 2026-02-06
**Status**: 🔄 ACTIVE
**Predecessor**: TSZ-7 (Lib Infrastructure Fix - Complete)

## Task

Implement **Conditional Type Inference** - the `infer` keyword and conditional type evaluation (`T extends U ? X : Y`).

## Problem Statement

TypeScript's conditional types use the `infer` keyword to extract types from within other types. This requires a sophisticated "matching" algorithm that differs from standard subtyping.

Examples:
- `ReturnType<T>` = `T extends (...args: any[]) => infer R ? R : any`
- `Parameters<T>` = `T extends (...args: infer P) => any ? P : never`
- `Awaited<T>` = `T extends Promise<infer V> ? V : T`

## Expected Impact

- **Direct**: Fix utility types like `ReturnType`, `Parameters`, `Awaited`
- **Compatibility**: Enable advanced TypeScript library patterns
- **Test Improvement**: Fix ~15-25 of the remaining 75 failures
- **Architecture**: Move logic to Solver (Solver-First principle)

## Implementation Plan

### Phase 1: Investigate Current State
1. Examine `src/solver/infer.rs` - check for `infer` handling
2. Review `src/solver/evaluate.rs` - conditional type evaluation
3. Check `src/solver/types.rs` - TypeKey::Infer and TypeKey::Conditional

### Phase 2: Implement Inference Algorithm
1. Implement type matching logic for `infer U` against source types
2. Handle union distribution (infer over each union member)
3. Support multiple inference candidates
4. Substitute inferred types into conditional "true" branch

### Phase 3: Test and Validate
1. Test with utility types (ReturnType, Parameters, Awaited)
2. Verify edge cases (union distribution, nested conditionals)
3. Check for regressions

## Files to Modify

- `src/solver/infer.rs` - Core inference algorithm
- `src/solver/evaluate.rs` - Conditional type evaluation
- `src/solver/types.rs` - TypeKey::Infer and TypeKey::Conditional
- `src/solver/visitor.rs` - Traversal of Infer nodes

## Test Status

**Start**: 8225 passing, 75 failing
**Target**: ~8240-8250 passing (+15-25 tests)

## Related NORTH_STAR.md Rules

- **Rule 1**: Solver-First Architecture - Conditional types are pure type operations
- **Rule 4**: Visitor Pattern - Systematic traversal for conditional/infer types

## Next Steps

1. Investigate current implementation
2. Ask Gemini for approach validation (Question 1) - **CRITICAL**
3. Implement based on guidance
4. Ask Gemini for implementation review (Question 2)

## Note

**CRITICAL**: The `infer` logic has many edge cases. Must ask Gemini for approach validation before implementing.
