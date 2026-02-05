# TSZ-1 Session Log

**Session ID**: tsz-1
**Last Updated**: 2025-02-05
**Focus**: Core Type Relations & Structural Soundness (The "Judge" Layer)

## Session Redefined (2025-02-05)

**Strategic Position**: Having completed the **Structural Identity Milestone**, the Judge now possesses a "Canonical Engine" capable of recognizing isomorphic recursive types. The focus shifts to **canonicalization integration** - ensuring that complex type algebra (Conditional, Mapped, Template Literals) produces canonical results that obey structural identity laws.

**Key Insight**: The "mechanics" of evaluation are often implemented, but the **structural soundness** and **canonicalization** integration is the Judge's remaining work.

### Coordination Map

| Session | Layer | Responsibility | Interaction with tsz-1 |
|:---|:---|:---|:---|
| **tsz-2** | **Interface** | Thinning the Checker | **Constraint**: Relies on Judge for all `evaluate` and `simplify` calls. |
| **tsz-4** | **Lawyer** | Nominality & Quirks | **Dependency**: Relies on Judge's variance calculations for generic assignability. |
| **tsz-1** | **Judge** | **Structural Soundness** | **Foundation**: Provides the Canonicalizer and ensures canonical results. |

## Milestone Status: Structural Identity ✅ COMPLETE

| Task | Title | Status | Outcome |
|:---|:---|:---|:---|
| **#32** | **Graph Isomorphism (Canonicalizer)** | ✅ **COMPLETE** | Implemented De Bruijn indices for recursive types. |
| **#35** | **Callable & Intersection Canonicalization** | ✅ **COMPLETE** | Intersections and overloads now have stable canonical forms. |
| **#36** | **Judge Integration: Fast-Path** | ✅ **COMPLETE** | `SubtypeChecker` uses `TypeId` equality for O(1) structural checks. |
| **#37** | **Deep Structural Simplification** | ✅ **COMPLETE** | Recursive types are simplified during evaluation. |
| **#11** | **Refined Narrowing** | ✅ **COMPLETE** | Fixed reversed checks and missing resolution in narrowing. |
| **#25** | **Coinductive Cycle Detection** | ✅ **COMPLETE** | Sound GFP semantics for recursive subtyping. |
| **#38** | **Conditional Type Evaluation** | ✅ **ALREADY DONE** | Distributivity, infer patterns, tail-recursion already implemented. |

**Recent Fixes**:
- Fixed disjoint unit type fast-path bug with labeled tuples (Commit: `34444a290`)

---

## New Priorities: Canonicalization Integration

### Priority 1: Task #39 - Mapped Type Canonicalization 🚧 ACTIVE
**Status**: 📋 NEXT IMMEDIATE
**Why**: Mapped types are the "Transformation" engine. The Judge must ensure they produce canonical object shapes.

**Implementation Goals**:
1. **Homomorphic Mapping**: `{ [P in keyof T]: T[P] }` should preserve T's structure when P is keyof T
2. **Modifier Handling**: Correctly strip (`-`) or add (`+`) `readonly` and `?` modifiers
3. **Canonicalization**: Ensure `{ [K in 'a' | 'b']: number }` reduces to `{ a: number, b: number }` with same ObjectShapeId

**Key Question for Gemini**:
> "I'm implementing Task #39: Mapped Type Evaluation. I need to ensure homomorphic mapped types are recognized as isomorphic to their source.
> How should I handle the 'identity' of a mapped type so that `{ [P in K]: T[P] }` is structurally identical to T when K is `keyof T`?
> Where should the canonicalization happen - in TypeEvaluator or in the Canonicalizer?"

**Files**: `src/solver/evaluate_rules/mapped.rs`, `src/solver/canonicalize.rs`

---

### Priority 2: Task #41 - Variance Calculation
**Status**: 📝 Planned
**Why**: The Judge must tell the Lawyer how to check generics. This enables O(1) generic assignability.

**Implementation Goals**:
1. **VarianceVisitor**: Walk type definitions to mark type parameters as Covariant, Contravariant, Invariant, or Independent
2. **Caching**: Store VarianceMask on ObjectShape or Symbol for reuse
3. **Integration**: SubtypeChecker uses variance to skip structural recursion for generic types

**Key Question for Gemini**:
> "I want to implement Task #41: Variance Calculation. I need to create a visitor that determines type parameter variance.
> Where should I store the resulting VarianceMask? How should SubtypeChecker consume it to skip structural recursion when checking List<string> <: List<unknown>?"

**Files**: `src/solver/visitor.rs`, `src/solver/compat.rs`

---

### Priority 3: Task #40 - Template Literal Deconstruction
**Status**: 📝 Planned
**Why**: Inference from template literals requires "Reverse String Matcher" for `infer` patterns.

**Implementation Goals**:
1. **Pattern Matching**: Inverse of template literal subtyping - extract `infer ID` from `` `user_${ID}` ``
2. **Greedy vs Non-Greedy**: Handle multiple `infer` positions correctly (e.g., `` `${infer A}_${infer B}` ``)
3. **Backtracking**: Implement proper backtracking for ambiguous matches

**Files**: `src/solver/evaluate_rules/template_literal.rs`, `src/solver/infer.rs`

---

## Guidance for the Judge

### The Judge's Responsibility
The **Judge** ensures **Structural Soundness** through canonicalization:
- **Rule 1**: Every evaluation result MUST be canonicalized (via `intern_canonical` or structural identity)
- **Rule 2**: Isomorphic structures MUST have the same TypeId (O(1) equality)
- **Rule 3**: Deferred types (TypeParameters) preserve structure until instantiation
- **Rule 4**: The Judge is strict; the Lawyer (tsz-4) adds "mercy" later

### What "Already Done" Means
When a task is marked "ALREADY DONE", it means:
- The **mechanics** are implemented (evaluation works)
- The **canonicalization integration** may still be needed
- The **structural soundness** guarantees may need verification

### The "Lawyer vs Judge" Distinction
- **Lawyer** (tsz-4): How types behave in specific situations (quirks, nominality)
- **Judge** (tsz-1): Mathematical correctness and canonical identity
