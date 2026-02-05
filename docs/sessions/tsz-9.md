# Session TSZ-9: Conditional Type Inference (`infer T`)

**Started**: 2026-02-05
**Status**: Active
**Goal**: Implement `infer` type parameter inference within conditional types

## Problem Statement

From NORTH_STAR.md:

TypeScript's conditional types support type parameter inference via the `infer` keyword:

```typescript
type ReturnType<T> = T extends (...args: any[]) => infer R ? R : any;
type T = ReturnType<() => string>; // T is string
```

The `infer R` declaration extracts the return type from the function type. This is critical for modern TypeScript libraries (Zod, TRPC, utility types) and requires sophisticated pattern matching within the Solver.

**Impact:**
- Blocks utility type implementations (ReturnType, Parameters, ThisParameterType, etc.)
- Prevents generic constraint inference in conditional types
- Critical for modern TypeScript ecosystem compatibility

## Technical Details

**Files**:
- `src/solver/infer.rs` - Type parameter inference logic
- `src/solver/evaluate.rs` - Conditional type evaluation
- `src/solver/subtype.rs` - Subtype checking for `extends` clause
- `src/solver/types.rs` - Type structures (ConditionalType, InferType)

**Root Cause**:
Conditional type evaluation needs to:
1. Check if `T extends U` (using subtype checker)
2. If true, extract inferred types from `infer` declarations in `V`
3. Substitute inferred types for type parameters in `V`
4. Handle contravariant positions (function parameters, `infer` in `extends` clause)
5. Handle multiple/overlapping `infer` declarations for the same type parameter

## Implementation Strategy

### Phase 1: Investigation (Pre-Implementation)
1. Read `docs/architecture/NORTH_STAR.md` sections on Conditional Types
2. Ask Gemini: "What's the correct approach for implementing `infer` in conditional types?"
3. Review existing conditional type evaluation in `src/solver/evaluate.rs`

### Phase 2: Implementation
1. Add `InferType` variant to `TypeKey` (if not present)
2. Implement inference extraction during conditional type evaluation
3. Handle variance (contravariant in function parameters)
4. Add support for `infer` in type parameter positions
5. Test with utility types (ReturnType, Parameters, etc.)

### Phase 3: Validation
1. Write unit tests for `infer` extraction
2. Test with complex conditional types
3. Ask Gemini Pro to review implementation

## Success Criteria

- [ ] `type T = ReturnType<() => string>` evaluates to `string`
- [ ] `type P = Parameters<(a: number, b: string) => void>` evaluates to `[number, string]`
- [ ] `infer` in contravariant positions works correctly
- [ ] Multiple `infer` declarations for same parameter are handled
- [ ] Conditional types with generic constraints work

## Session History

*Created 2026-02-05 after completing TSZ-4 (Lawyer Layer Audit).*
*Renamed from TSZ-8 due to naming conflict with existing session.*
