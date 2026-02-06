# TSZ-1 Session Log

**Session ID**: tsz-1
**Last Updated**: 2026-02-05
**Focus**: Core Type Relations & Structural Soundness (The "Judge" Layer)

## Session Redefined (2025-02-05)

**Strategic Position**: Transitioning from **Performance Optimization** to **"Canonical Completeness"** milestone. The Judge now has robust canonicalization machinery (De Bruijn indices, partial intersection merging), but must ensure that EVERY path through the Solver produces canonical results and leverages O(1) equality.

**Key Insight**: While TypeId equality is O(1) for identical types, we still perform full structural walks for non-identical types on every subtype check. Subtype memoization is the biggest remaining performance win.

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

### Priority 4: Task #43 - Canonical Intersection Merging ✅ COMPLETE
**Status**: ✅ COMPLETE
**Why**: Partial merging enables O(1) equality for mixed intersections like `{a} & {b} & prim`.

**Implementation Completed** (Commit: `520309b42`):
1. ✅ **Partial Merging Strategy**: Replaced all-or-nothing merging with extraction-based approach
2. ✅ **extract_and_merge_objects()**: Extracts objects from mixed intersections, merges them
3. ✅ **extract_and_merge_callables()**: Extracts callables from mixed intersections, merges them
4. ✅ **Canonical Rebuild**: `[sorted non-callables, merged object, merged callable]`
5. ✅ **Critical Bug Fix**: Removed `narrow_literal_primitive_intersection` which was too aggressive
6. ✅ **Gemini Pro Review**: Implementation reviewed and approved

**Partial Merging Examples**:
- `{ a: string } & { b: number } & boolean` → `{ a: string; b: number } & boolean`
- `func1 & func2 & boolean` → `merged_callable(overloads: [func1, func2]) & boolean`
- `{a} & {b} & func1 & func2` → `{ a; b } & merged_callable`

**Key Insight**: Previously, merging only worked when ALL members were objects or ALL were callables. Now we extract and merge separately.

**Bug Fixed**: The `narrow_literal_primitive_intersection` function was incorrectly reducing mixed intersections like `"a" & string & { x: 1 }` to just `"a"`, losing the object member. The `reduce_intersection_subtypes()` at the end already handles literal/primitive narrowing correctly via `is_subtype_shallow` checks.

**Tests Added**:
- `test_partial_object_merging_in_intersection`: Verifies object merging in mixed intersections
- `test_partial_callable_merging_in_intersection`: Verifies callable merging in mixed intersections
- `test_partial_object_and_callable_merging`: Verifies both objects and callables are merged

**Files**: `src/solver/intern.rs`, `src/solver/tests/intern_tests.rs`

---

## New Milestone: Canonical Completeness

**Goal**: Ensure that no operation in `src/solver/` can ever produce a `TypeId` that is structurally equivalent to another `TypeId` but has a different integer value. This is the prerequisite for the Checker (tsz-2) to rely entirely on `==` for type comparisons.

### Priority 1: Task #44 - Subtype Result Caching 🚧 NEXT
**Status**: 🚧 IN PROGRESS
**Why**: Every time the Checker asks `is_subtype_of(A, B)` where `A != B`, we perform a full structural walk. Memoizing these results is the biggest remaining performance win.

**Implementation Plan**:
1. ✅ Review existing cycle_stack in `src/solver/subtype.rs` for GFP semantics
2. 🚧 Implement `SubtypeCache` that stores `(TypeId, TypeId) -> SubtypeResult`
3. ⏳ Add cache lookup before structural walk
4. ⏳ Add cache storage after successful check
5. ⏳ Handle cache invalidation for recursive types (coinduction)

**Key Challenge**: The cache must work correctly with coinductive semantics for recursive types. The `cycle_stack` prevents infinite loops, but the cache must distinguish "currently checking" from "already checked".

**Files**: `src/solver/subtype.rs`, `src/solver/types.rs`

---

### Priority 2: Task #45 - Index Access & Keyof Simplification
**Status**: ⏳ PENDING
**Why**: `evaluate_index_access` and `evaluate_keyof` must return the most simplified canonical form.

**Examples**:
- `keyof {a: 1, b: 2}` should return the same TypeId as `"a" | "b"`
- `T[K]` where `T = {a: string, b: number}` and `K = "a"` should simplify to `string`

**Files**: `src/solver/evaluate.rs`

---

### Priority 3: Task #46 - Instantiation Canonicalization
**Status**: ⏳ PENDING
**Why**: When a generic is substituted, the resulting TypeKey must be passed through canonical normalization.

**Example**: `List<string>` becoming `string | string` after substitution should collapse to `string`.

**Files**: `src/solver/instantiate.rs`

---

### Priority 4: Task #47 - Template Literal Canonicalization
**Status**: ⏳ PENDING
**Why**: Template literals need normalization for adjacent string constants and `any`/`never` absorption.

**Files**: `src/solver/intern.rs`

---

## Critical Gaps Identified

### Gap A: Double Interning
Some evaluation functions call `interner.intern()` directly, potentially bypassing canonicalization. Need to audit all calls in `evaluate.rs` and `instantiate.rs`.

### Gap B: Subtype Memoization vs. Coinduction
No long-lived cache for successful subtype checks. Every check performs a full structural walk unless types are identical.

### Gap C: Literal/Primitive Intersection Soundness
Need to refine `reduce_intersection_subtypes` to handle primitive-object intersections based on TypeScript's "boxing" rules (e.g., `string & { length: number }` is valid, but `number & { length: number }` is not).

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
