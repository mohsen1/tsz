# TSZ-1 Session Log

**Session ID**: tsz-1
**Last Updated**: 2025-02-05
**Focus**: Core Type Relations & Structural Soundness (The "Judge" Layer)

## Session Redefined (2025-02-05)

**Strategic Position**: Having completed the **Structural Identity Milestone**, the Judge now possesses a "Canonical Engine" capable of recognizing isomorphic recursive types. The focus now shifts from **Identity** to **Transformation**—implementing the complex type algebra of TypeScript (Conditional and Mapped types) while ensuring they produce canonical results.

**Core Responsibility**: Ensure that advanced type evaluations (Conditional, Mapped, Template Literals) are mathematically sound and integrated into the canonical interning system.

### Coordination Map

| Session | Layer | Responsibility | Interaction with tsz-1 |
|:---|:---|:---|:---|
| **tsz-2** | **Interface** | Thinning the Checker | **Constraint**: Relies on Judge for all `evaluate` and `simplify` calls. |
| **tsz-4** | **Lawyer** | Nominality & Quirks | **Dependency**: Relies on Judge's variance calculations for generic assignability. |
| **tsz-1** | **Judge** | **Structural Soundness** | **Foundation**: Provides the Canonicalizer and Evaluation engine. |

## Milestone Status: Structural Identity ✅ COMPLETE

| Task | Title | Status | Outcome |
|:---|:---|:---|:---|
| **#32** | **Graph Isomorphism (Canonicalizer)** | ✅ **COMPLETE** | Implemented De Bruijn indices for recursive types. |
| **#35** | **Callable & Intersection Canonicalization** | ✅ **COMPLETE** | Intersections and overloads now have stable canonical forms. |
| **#36** | **Judge Integration: Fast-Path** | ✅ **COMPLETE** | `SubtypeChecker` uses `TypeId` equality for O(1) structural checks. |
| **#37** | **Deep Structural Simplification** | ✅ **COMPLETE** | Recursive types are simplified during evaluation. |
| **#11** | **Refined Narrowing** | ✅ **COMPLETE** | Fixed reversed checks and missing resolution in narrowing. |
| **#25** | **Coinductive Cycle Detection** | ✅ **COMPLETE** | Sound GFP semantics for recursive subtyping. |

**Recent Fixes**:
- Fixed a bug in the disjoint unit type fast-path where tuples were incorrectly identified as disjoint (Commit: `34444a290`).

---

## New Priorities: Advanced Type Algebra

### Priority 1: Task #38 - Conditional Type Evaluation 🚧 ACTIVE
**Status**: 📋 NEXT IMMEDIATE
**Why**: Conditional types are the "logic" of the type system. They must be distributive and support `infer`.

**Implementation Goals**:
1. **Distributivity**: `(A | B) extends U ? X : Y` → `(A extends U ? X : Y) | (B extends U ? X : Y)`.
2. **Inference**: Implement `infer R` support by extending the `InferContext` during evaluation.
3. **Canonicalization**: Ensure the result of a conditional evaluation is passed through `intern_canonical`.

**Files**: `src/solver/evaluate.rs`, `src/solver/infer.rs`

---

### Priority 2: Task #39 - Mapped Type Evaluation
**Status**: 📝 Planned
**Why**: Essential for utility types like `Partial<T>`, `Readonly<T>`, and `Pick<T, K>`.

**Implementation Goals**:
1. **Key Mapping**: Correctly evaluate `{ [K in keyof T]: T[K] }`.
2. **Modifier Mapping**: Handle `+readonly`, `-readonly`, `+?`, and `-?`.
3. **Homomorphic Mapped Types**: Preserve the structure of the source type when mapping over `keyof T`.

**Files**: `src/solver/evaluate.rs`

---

### Priority 3: Task #40 - Template Literal Type Inference
**Status**: 📝 Planned
**Why**: Allows the Judge to "deconstruct" strings (e.g., inferring `ID` from `` `user_${infer ID}` ``).

**Implementation Goals**:
1. **Pattern Matching**: Implement the inverse of template literal subtyping.
2. **Greedy vs. Non-greedy**: Match TypeScript's specific backtracking behavior for multiple `infer` positions.

**Files**: `src/solver/subtype_rules/literals.rs`, `src/solver/evaluate.rs`

---

### Priority 4: Task #41 - Variance Calculation
**Status**: 📝 Planned
**Why**: The Judge must tell the Lawyer how to check generics.

**Implementation Goals**:
1. **Variance Visitor**: A visitor that walks a type definition to determine if a type parameter is in a covariant, contravariant, or invariant position.
2. **Optimization**: Cache variance results on the `ObjectShape` or `Symbol`.

**Files**: `src/solver/visitor.rs`, `src/solver/compat.rs`

---

## Active Tasks

### Task #38: Conditional Type Evaluation
**Status**: 📋 Starting
**Priority**: Critical

**Description**:
Implement the full evaluation logic for `TypeKey::Conditional(ConditionalTypeId)`.

**Two-Question Rule - Question 1 (Approach)**:
1. How should we handle the "checked type" when it is a `TypeParameter`? (Wait for instantiation or evaluate against constraint?)
2. Where should the distributive logic live? (Inside `evaluate_conditional` or as a pre-pass in `TypeEvaluator`?)
3. How do we integrate the `Canonicalizer` to ensure `T extends U ? X : Y` is simplified if `X` and `Y` become structurally identical?

**Next Step**: Ask Gemini Question 1 for Task #38.

---

## Guidance for the Judge
- **Rule 1**: Every evaluation result MUST be canonicalized.
- **Rule 2**: Use the `cycle_stack` in `TypeEvaluator` to prevent infinite unrolling of recursive conditional types.
- **Rule 3**: When in doubt, the Judge should be strict. The Lawyer (tsz-4) can add the "mercy" later.
