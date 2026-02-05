# TSZ-1 Session Log

**Session ID**: tsz-1
**Last Updated**: 2026-02-05
**Focus**: Core Type Relations & Structural Soundness (The "Judge" Layer)

## Session Redefined (2025-02-05)

**Strategic Position**: Having completed the **Structural Identity Milestone**, the Judge now possesses a "Canonical Engine" capable of recognizing isomorphic recursive types. The focus shifts to **Performance Optimization** through variance calculation and canonicalization integration.

**Key Insight**: The "mechanics" of evaluation are often implemented, but the **structural soundness** and **performance optimization** integration is the Judge's remaining work.

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
| **#39** | **Mapped Type Canonicalization** | ✅ **COMPLETE** | Mapped types now achieve O(1) equality with alpha-equivalence. |
| **#11** | **Refined Narrowing** | ✅ **COMPLETE** | Fixed reversed checks and missing resolution in narrowing. |
| **#25** | **Coinductive Cycle Detection** | ✅ **COMPLETE** | Sound GFP semantics for recursive subtyping. |
| **#38** | **Conditional Type Evaluation** | ✅ **ALREADY DONE** | Distributivity, infer patterns, tail-recursion already implemented. |

**Recent Fixes**:
- Fixed disjoint unit type fast-path bug with labeled tuples (Commit: `34444a290`)
- Mapped type canonicalization achieved 9 test improvements (Commit: `a15dc43ba`)

---

## New Priorities: Performance Optimization

### Priority 1: Task #41 - Variance Calculation ✅ PHASE 1 COMPLETE
**Status**: 📋 INTEGRATION PENDING
**Why**: Critical for North Star O(1) performance targets. Enables skipping structural recursion for generic types.

**Phase 1 Completed** (Commit: `e800bb82d`):
1. ✅ **Variance Types**: Added `Variance` bitflags type in `types.rs` with COVARIANT, CONTRAVARIANT flags
2. ✅ **VarianceVisitor**: Created `src/solver/variance.rs` with visitor that traverses types with polarity tracking
3. ✅ **All TypeKey Variants**: Properly handles all variants with correct polarity rules:
   - Function parameters: contravariant (flip polarity)
   - Function returns: covariant (preserve polarity)
   - Conditional types: check_type covariant, extends_type contravariant
   - Mapped types: constraint contravariant, template covariant
   - Mutable properties: invariant (visit both polarities)
   - Readonly properties: covariant
   - Methods: bivariant parameters (skip variance check)
   - Generic applications: conservative invariance (both polarities)
   - Infer declarations: excluded (not usages of outer type params)

**Critical Fixes** (per Gemini Pro review):
1. ✅ Mutable properties now correctly marked as invariant (not covariant)
2. ✅ Generic applications safely assume invariance (not unsound covariance)
3. ✅ Method bivariance supported by skipping parameter variance
4. ✅ Conditional check_type polarity fixed (covariant, not contravariant)
5. ✅ Infer declarations excluded (declarations are not usages)

**Phase 2 Pending** (Integration):
- Add variance query to `QueryDatabase` trait
- Implement variance memoization cache
- Integrate variance mask into `check_application_to_application_subtype` in `generics.rs`

**Key Edge Cases**:
- **Polarity Flipping**: Covariant × Contravariant = Contravariant
- **Mutable Properties**: read/write properties immediately promote to Invariant
- **Circular References**: Track visiting set of (TypeId, Polarity) pairs
- **Private Members**: Often treated as Independent for structural subtyping
- **Conditional Types**: extends clause is contravariant position

**Files**: `src/solver/variance.rs` (NEW), `src/solver/types.rs`, `src/solver/db.rs`, `src/solver/subtype.rs`

---

### Priority 2: Task #40 - Template Literal Deconstruction
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
The **Judge** ensures **Structural Soundness** through canonicalization and optimization:
- **Rule 1**: Every evaluation result MUST be canonicalized (via `intern_canonical` or structural identity)
- **Rule 2**: Isomorphic structures MUST have the same TypeId (O(1) equality)
- **Rule 3**: Deferred types (TypeParameters) preserve structure until instantiation
- **Rule 4**: The Judge is strict; the Lawyer (tsz-4) adds "mercy" later

### What "Already Done" Means
When a task is marked "ALREADY DONE", it means:
- The **mechanics** are implemented (evaluation works)
- The **canonicalization integration** may still be needed
- The **performance optimization** may be required

### The "Lawyer vs Judge" Distinction
- **Lawyer** (tsz-4): How types behave in specific situations (quirks, nominality)
- **Judge** (tsz-1): Mathematical correctness and canonical identity
