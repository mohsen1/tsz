# Session tsz-8: Contextual Typing & Bidirectional Inference

**Started**: 2026-02-04
**Status**: 🟢 ACTIVE (Generic Inference Focus)
**Current Priority**: Implement Generic Contextual Inference

---

## Session Goal

Implement **bidirectional type inference** to enable TypeScript's contextual typing patterns, with emphasis on **generic contextual inference** as the highest priority.

### Why This Is Critical

Without generic contextual inference, common patterns like `array.map(x => ...)` fail because the compiler cannot infer that `x` should be seeded with the array's element type.

---

## Current Work: Generic Contextual Inference (HIGHEST PRIORITY)

### Foundation ✅ COMPLETE
- `get_contextual_signature()` implemented in `src/solver/operations.rs`
- Uses `TypeVisitor` pattern for clean type traversal
- Handles `FunctionShape` and `CallableShape`
- `Contextual` priority added to `InferencePriority` enum

### Next Steps: In Progress

**Task 1.1: Implement `visit_application` for Generic Types (HIGH)**
- **File**: `src/solver/operations.rs`
- **Goal**: Handle `type Handler<T> = (item: T) => void` patterns
- **Implementation**: Add `visit_application()` to `ContextualSignatureVisitor`
  - Extract base signature (e.g., `(item: T) => void`)
  - Instantiate with type arguments (e.g., `T = string`)
  - Return concrete `FunctionShape` with substituted types

**Task 1.2: Seed InferenceContext with Contextual Constraints (HIGH)**
- **File**: `src/solver/infer.rs`
- **Goal**: Allow `InferenceContext` to accept external contextual hints
- **Implementation**:
  - Modify `resolve_generic_call` to use contextual return type
  - Seed type parameters with contextual constraints before argument checking
  - Ensure `Contextual` priority is lower than `Argument`

**Task 1.3: Wire up `check_with_context` in ExpressionChecker (MEDIUM)**
- **File**: `src/checker/expr.rs`
- **Goal**: Propagate contextual types down to function expressions
- **Implementation**:
  - Add `check_with_context(idx, context_type)` method
  - Detect when expressions have contextual types
  - Call `get_contextual_signature()` to infer parameters

---

## Background: Original Session Plan

Implement **contextual typing** to enable TypeScript's bidirectional type inference, where types can flow "down" from assignment targets into function expressions.

### Problem Statement

Currently, tsz only implements "upward" inference (from arguments to return type). This fails to handle common TypeScript patterns:

```typescript
// Without contextual typing, 'x' is inferred as 'any' or 'unknown'
// With contextual typing, 'x' should be inferred as 'string' from the target type
const f: (x: string) => void = (x) => { console.log(x); };

// Array methods should use contextual typing
const nums = [1, 2, 3];
const doubled = nums.map(x => x * 2); // 'x' should be 'number' from array type
```

### Success Criteria

1. Function expressions can infer parameter types from their target signature
2. Arrow functions use contextual typing for parameters
3. Contextual inference respects priority (contextual < argument inference)
4. No regressions in existing upward inference

---

## Prioritized Tasks

### Task 1: Add contextual_type propagation in Checker (HIGH)
**File**: `src/checker/expr.rs`

**Goal**: Modify expression checking to pass `contextual_type: Option<TypeId>` down to sub-expressions.

**Implementation Plan**:
1. Add `contextual_type` parameter to relevant `check_expression` methods
2. Identify contexts where target type is known:
   - Variable declarations with type annotations
   - Assignment expressions
   - Return statements (function return type)
   - Call arguments (parameter types)
3. Pass contextual_type to function expression and arrow function checking

### Task 2: Enhance InferenceContext for external constraints (HIGH)
**File**: `src/solver/infer.rs`

**Goal**: Allow `InferenceContext` to accept "seed" constraints from contextual types.

**Implementation Plan**:
1. Add method to pre-populate inference variables with contextual hints
2. Ensure contextual constraints have lower priority than argument inference
3. Use `InferencePriority::ReturnType` (already exists) for contextual constraints

### Task 3: Implement contextual parameter inference (MEDIUM)
**Files**: `src/solver/operations.rs`, `src/solver/infer.rs`

**Goal**: When a function expression has a contextual signature, infer parameter types from it.

**Implementation Plan**:
1. Extract parameter types from contextual function type
2. Create inference variables for function parameters
3. Seed InferenceContext with contextual parameter types
4. Ensure argument-based inference can override contextual hints

### Task 4: Handle union contextual types (MEDIUM)
**File**: `src/solver/operations.rs`

**Goal**: Support contextual typing when the target type is a union of function types.

**Example**:
```typescript
type Handler = (x: string) => void | (x: number) => void;
const h: Handler = (x) => {}; // What is 'x'?
```

**Implementation Plan**:
1. Detect when contextual type is a union containing functions
2. Use best common type of parameter types across union members
3. Fallback to `unknown` if parameter types are incompatible

---

## Session History

### Phase 1: Approach Validation (IN PROGRESS)

**Current Step**: Following Two-Question Rule from AGENTS.md

**Question 1** (Ready to ask Gemini):
```
I need to implement Contextual Typing for function expressions. My plan is to:
1. Modify `check_expression` in `src/checker/expr.rs` to pass `contextual_type: Option<TypeId>` down
2. Use this in `src/solver/infer.rs` to seed the `InferenceContext`

Questions:
1. Is this the right architectural touchpoint?
2. How should I handle cases where the contextual type is a union of functions?
3. What function in the Solver should handle the 'merging' of contextual hints with inferred constraints?
```

**Question 2** (After implementation):
```
I implemented contextual typing in [FILES]. Please review:
1. Is this correct for TypeScript?
2. Does it match tsc behavior?
3. Are there edge cases I missed?
```

---

## Dependencies

**Built on**:
- Session tsz-3: Generic Inference & Nominal Hierarchy Integration ✅
  - InferenceContext infrastructure
  - TypeResolver integration
  - Constraint propagation (strengthen_constraints)

**Related Sessions**:
- tsz-2: Type Narrowing & CFA (narrowing interacts with contextual typing)
- tsz-1: Core Solver Correctness (foundational type operations)

---

## Complexity: HIGH

**Why High**:
- Contextual typing requires bidirectional type flow
- Must correctly handle unions, intersections, and generics
- Priority system must prevent contextual hints from overriding strong evidence
- Edge cases: void context, recursive functions, conditional types

**Risk**: Incorrect implementation could cause `any` leakage or over-constrain types.

**Mitigation**: Follow Two-Question Rule strictly. All changes reviewed by Gemini.

---

## Coordination Notes

### Avoid (domain conflicts):
- **tsz-4/tsz-5**: Declaration Emit tasks (different domain)
- **tsz-6/tsz-7**: Completed import/export work

### Coordinate with:
- **tsz-2**: Narrowing logic may need updates for contextual types
- **tsz-1**: Core Solver changes should be coordinated

### Leverage:
- **tsz-3 work**: InferenceContext, TypeResolver, constraint propagation already in place
- **SubtypeChecker**: Can be used for contextual subtype checking

---

## Session History

- 2026-02-04: Session started - building on tsz-3 Generic Inference work
- Focus: Implementing bidirectional type inference for function expressions
