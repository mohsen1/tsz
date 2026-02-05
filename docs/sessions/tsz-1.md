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

### Priority 1: Task #41 - Variance Calculation ✅ PHASE 3 COMPLETE
**Status**: ✅ COMPLETE
**Why**: Critical for North Star O(1) performance targets. Enables skipping structural recursion for generic types.

**Phase 1 Completed** (Commit: `e800bb82d`):
1. ✅ **Variance Types**: Added `Variance` bitflags type in `types.rs` with COVARIANT, CONTRAVARIANT flags
2. ✅ **VarianceVisitor**: Created `src/solver/variance.rs` with visitor that traverses types with polarity tracking
3. ✅ **All TypeKey Variants**: Properly handles all variants with correct polarity rules

**Phase 2 Completed** (Commit: `f5167b61c`):
1. ✅ **QueryDatabase Integration**: Added `get_type_param_variance` to `QueryDatabase` trait
2. ✅ **Variance Cache**: Added `variance_cache` to `QueryCache` for memoization
3. ✅ **TypeResolver Integration**: Added `get_type_param_variance` to `TypeResolver` trait
4. ✅ **SubtypeChecker Integration**: Modified `check_application_to_application_subtype` to use variance-aware checking

**Phase 3 Completed** (Commits: `39d70dbd4`, `3619bb501`):
1. ✅ **Lazy Type Resolution**: Implemented `visit_lazy` to resolve `Lazy(DefId)` types
2. ✅ **Ref Type Handling**: Implemented `visit_ref` for legacy `Ref(SymbolRef)` types
3. ✅ **Recursive Variance Composition**: Implemented variance composition in `visit_application`:
   - Queries base type's variance mask from `get_type_param_variance`
   - Composes variance: Covariant base preserves polarity, Contravariant flips it
   - Falls back to invariance if base variance unknown
4. ✅ **Keyof Contravariance**: Fixed `visit_keyof` to flip polarity (keyof is contravariant)
5. ✅ **Gemini Pro Review**: Implementation reviewed and approved

**Variance Rules Implemented**:
- **Covariant**: Check `s_arg <: t_arg` (e.g., `Array<T>`)
- **Contravariant**: Check `t_arg <: s_arg` (e.g., function parameters, keyof)
- **Invariant**: Check both directions (e.g., mutable properties)
- **Independent**: Skip check (type parameter not used)

**Variance Composition Examples**:
- `type Box<T> = { value: T }` → Covariant (previously Independent)
- `type Wrapper<T> = Box<T>` → Covariant (previously Invariant)
- `type Reader<T> = (x: T) => void` → Contravariant (function parameter)

**Files**: `src/solver/variance.rs`, `src/solver/types.rs`, `src/solver/db.rs`, `src/solver/subtype.rs`, `src/solver/subtype_rules/generics.rs`

---

### Priority 2: Task #40 - Template Literal Deconstruction ✅ COMPLETE
**Status**: ✅ COMPLETE
**Why**: Inference from template literals requires "Reverse String Matcher" for `infer` patterns.

**Implementation Completed** (Commits: `c9ee174f3`, `5484ab6e7`):
1. ✅ **Pattern Matching**: Implemented `infer_from_template_literal` in InferenceContext
2. ✅ **Non-Greedy Matching**: Correctly handles multiple `infer` positions with anchor-based matching
3. ✅ **Adjacent Placeholders**: Fixed bug for `${infer A}${infer B}` patterns (empty string capture)
4. ✅ **Special Cases**: Handles `any` and `string` intrinsic types correctly
5. ✅ **Union Support**: Matches each union member against the pattern

**Examples**:
- `"user_123" extends `user_${infer ID}` → ID = "123"
- `"a_b_c" extends `${infer A}_${infer B}_${infer C}` → A = "a", B = "b", C = "c"
- `"abc" extends `${infer A}${infer B}` → A = "", B = "abc" (adjacent placeholders)

**Files**: `src/solver/infer.rs`

---

### Priority 3: Task #42 - Canonicalization Integration ✅ COMPLETE
**Status**: ✅ COMPLETE
**Why**: North Star O(1) equality goal requires that all type-producing operations return canonicalized TypeIds.

**Findings**: Task #42 is already implemented! The `TypeInterner` already has comprehensive canonicalization:
1. ✅ **Order Independence**: `normalize_union` and `normalize_intersection` sort members by TypeId
2. ✅ **Deduplication**: Both use `dedup()` to remove redundant members
3. ✅ **Unit Type Collapsing**: Handles `any`, `unknown`, `never`, literal absorption
4. ✅ **Callable Order Preservation**: Intersections preserve callable order for overload resolution
5. ✅ **Disjoint Primitive Checking**: Detects incompatible intersections

**Tests Added** (Commit: `b88b34892`):
- `test_union_order_independence`: Verifies `A | B == B | A`
- `test_intersection_order_independence`: Verifies `A & B == B & A` (non-callables)
- `test_union_redundancy_elimination`: Verifies `A | A` simplifies to `A`
- `test_intersection_redundancy_elimination`: Verifies `A & A` simplifies to `A`

**Files**: `src/solver/intern.rs` (already canonicalized), `src/solver/tests/intern_tests.rs` (tests added)

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
