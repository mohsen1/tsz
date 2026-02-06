# TSZ-1 Session Log

**Session ID**: tsz-1
**Last Updated**: 2026-02-06
**Focus**: Core Type Relations & Structural Soundness (The "Judge" Layer)

## Session Redefined (2026-02-06 - O(1) Equality Push)

**Strategic Position**: Transitioning to **"Structural Interning"** - where TypeId itself represents the canonical form. The `Canonicalizer` exists but is currently "opt-in" in SubtypeChecker fast-path. To reach North Star O(1) equality, we must move from "Physical Interning" to "Structural Interning."

**Key Insight**: Currently `are_types_structurally_identical()` creates a new `Canonicalizer` and re-traverses the graph on every call - O(N) instead of O(1). We need global canonical mapping + visitor pattern refactoring.

### Redefined Priorities (per Gemini consultation)

#### Priority 1: Task #47 - Template Literal & String Intrinsic Canonicalization ✅ COMPLETE
**Status**: ✅ COMPLETE (commit: be9cc3f07)
**File**: `src/solver/canonicalize.rs`
**Problem**: `Uppercase<T>` and `Uppercase<U>` should be identical if `T` and `U` are identical. Template literals need alpha-equivalence.
**Action**:
1. ✅ Implemented `TypeKey::TemplateLiteral(id)`: Iterate spans, canonicalize `TemplateSpan::Type(id)`
2. ✅ Implemented `TypeKey::StringIntrinsic { kind, type_arg }`: Canonicalize `type_arg`

#### Priority 2: Task #48 - SubtypeChecker Visitor Pattern Refactor (North Star Rule 2)
**Status**: 🔄 IN PROGRESS
**File**: `src/solver/subtype.rs`
**Problem**: `SubtypeChecker` is a "God Object" (~1000 lines) with massive match blocks. North Star Rule 2 mandates Visitor Pattern for all type operations.
**Action**:
1. ✅ Create `SubtypeVisitor` implementing `TypeVisitor` (commit: a318e7642)
2. ⏳ Move logic from `check_subtype_inner` into the visitor
3. ⏳ Enforce handling all 24+ `TypeKey` variants, preventing "missing variant" bugs

**Progress** (Task #48.1 Complete):
- Created `SubtypeVisitor<'a, 'b, R>` struct with `checker` and `target` fields
- Implemented `TypeVisitor` trait with all required methods
- Core intrinsics (intrinsic, literal) fully implemented
- Union/Intersection handling implemented
- Stub implementations for complex types (object, function, callable)

#### Priority 3: Task #49 - Global Canonical Mapping (The O(1) Goal)
**Status**: ⏳ PENDING
**Files**: `src/solver/db.rs`, `src/solver/intern.rs`
**Problem**: `are_types_structurally_identical` is O(N) - re-runs Canonicalizer every time.
**Action**:
1. Add `canonical_id(TypeId) -> TypeId` to `QueryDatabase` trait
2. Implement in `QueryCache` using `RwLock<FxHashMap<TypeId, TypeId>>`
3. Update `SubtypeChecker::are_types_structurally_identical` to compare `db.canonical_id(a) == db.canonical_id(b)`

#### Priority 4: Task #50 - Variance Analysis for Lazy Types
**Status**: ⏳ PENDING
**File**: `src/solver/variance.rs`
**Problem**: Variance-aware subtyping (Task #41) relies on resolver providing variance. Need to ensure Judge can compute this for `Lazy` types.
**Action**: Ensure `VarianceVisitor::visit_lazy` resolves and continues variance calculation for `Box<T>` where `Box` is a type alias.

### Redefined Priorities: Total Canonicalization

#### Priority 1: Task #46 - Instantiation Canonicalization ⏳ NEXT
**Status**: ⏳ PENDING
**File**: `src/solver/instantiate.rs`
**Problem**: When `instantiate_type` performs substitution, it constructs new types. If it calls `interner.intern()` with raw `TypeKey`, it bypasses normalization.
**Example**: `List<string | string>` must equal `List<string>`.
**Action**: Audit `TypeInstantiator::instantiate_key` to use canonical methods.

---

#### Priority 2: Task #47 - Template Literal Canonicalization
**Status**: ⏳ PENDING
**Files**: `src/solver/intern.rs`, `src/solver/evaluate_rules/template_literal.rs`
**Problem**: Template literals allow redundant structures.
**Requirements**:
1. Merge adjacent `TemplateSpan::Text` nodes
2. Remove empty string literals from interpolations
3. Never absorption: if any part is `never`, whole type is `never`
4. Any widening: if any part is `any`, whole type is `string`

---

#### Priority 3: Task #48 - Primitive-Object Intersection Soundness
**Status**: ⏳ PENDING
**File**: `src/solver/intern.rs` (Function: `reduce_intersection_subtypes`)
**Problem**: TypeScript has "boxing" rules. `string & { length: number }` is valid, but `number & { length: number }` is not.
**Action**: Refine intersection reduction for primitive-object intersections.

---

#### Priority 4: Task #49 - Global Subtype Cache Persistence
**Status**: ⏳ PENDING
**Files**: `src/solver/db.rs`, `src/solver/subtype.rs`
**Problem**: Ensure `QueryCache` acts as long-lived, thread-safe store.
**Action**: Audit `RelationCacheKey` lifecycle for correct context capture.

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

### Priority 1: Task #44 - Subtype Result Caching ✅ ALREADY DONE
**Status**: ✅ ALREADY IMPLEMENTED
**Why**: Every time the Checker asks `is_subtype_of(A, B)` where `A != B`, we perform a full structural walk. Memoizing these results is the biggest remaining performance win.

**Findings**: Task #44 is already implemented! The `SubtypeChecker` in `src/solver/subtype.rs` already has comprehensive subtype result caching:
1. ✅ **Cache Lookup**: Lines 1398-1407 - checks `QueryDatabase` cache before structural walk
2. ✅ **Cache Insertion**: Lines 1580-1589 - stores only definitive results (True/False)
3. ✅ **Non-Definitive Results**: CycleDetected and DepthExceeded are NOT cached (correct!)
4. ✅ **RelationCacheKey**: Uses `make_cache_key()` which includes compiler flags
5. ✅ **QueryDatabase Integration**: Fully integrated with `lookup_subtype_cache` and `insert_subtype_cache`

**Key Implementation Detail**: The cache correctly distinguishes definitive from non-definitive results:
- `True` and `False` → Cached
- `CycleDetected` and `DepthExceeded` → NOT cached (prevents unsoundness)

**Files**: `src/solver/subtype.rs` (already implemented), `src/solver/db.rs` (QueryCache with subtype_cache)

---

### Priority 2: Task #45 - Index Access & Keyof Simplification ✅ ALREADY DONE
**Status**: ✅ ALREADY IMPLEMENTED (370 + 825 lines)
**Why**: `evaluate_index_access` and `evaluate_keyof` must return the most simplified canonical form.

**Findings**: Comprehensive implementations already exist:
- `src/solver/evaluate_rules/keyof.rs` (370 lines) - Full keyof operator with distributivity
- `src/solver/evaluate_rules/index_access.rs` (825 lines) - Complete index access implementation

Both already use canonical `union()` and `intersection()` methods.

---

### Priority 3: Task #46 - Instantiation Canonicalization ✅ ALREADY DONE
**Status**: ✅ ALREADY HANDLED
**Why**: When a generic is substituted, the resulting TypeKey must be passed through canonical normalization.

**Findings**: Task #46 is already correctly implemented! The `TypeInstantiator` in `src/solver/instantiate.rs` already uses canonical methods:
1. ✅ **Union**: Line 302 - calls `self.interner.union(instantiated)` which normalizes
2. ✅ **Intersection**: Line 310 - calls `self.interner.intersection(instantiated)` which normalizes
3. ✅ **Mapped**: Line 560 - calls `self.interner.mapped(instantiated)`
4. ✅ **Array**: Line 316 - calls `self.interner.array(instantiated)`
5. ✅ **Tuple**: Line 328 - calls `self.interner.tuple(instantiated)`

**Example**: `List<string | string>` → When instantiating, each `string` is instantiated (returns `TypeId::STRING`), then `self.interner.union([string, string])` is called, which normalizes to just `string`.

**Meta-types** (`IndexAccess`, `KeyOf`, `ReadonlyType`) use raw `intern()` because they are deferred evaluation - they get normalized later when `evaluate()` is called.

**Files**: `src/solver/instantiate.rs` (already using canonical methods)

---

### Priority 4: Task #47 - Template Literal Canonicalization 🚧 NEXT
**Status**: ⏳ PENDING
**Why**: Template literals need normalization for adjacent string constants and `any`/`never` absorption.

**Requirements**:
1. Merge adjacent `TemplateSpan::Text` nodes
2. Remove empty string literals from interpolations
3. Never absorption: if any part is `never`, whole type is `never`
4. Any widening: if any part is `any`, whole type is `string`

**Files**: `src/solver/intern.rs`, `src/solver/evaluate_rules/template_literal.rs`

---

### Priority 5: Task #48 - Primitive-Object Intersection Soundness
**Status**: ⏳ PENDING
**Why**: TypeScript has "boxing" rules. `string & { length: number }` is valid, but `number & { length: number }` is not.

**Action**: Refine `reduce_intersection_subtypes` for primitive-object intersections.

**Files**: `src/solver/intern.rs`

---

### Priority 6: Task #49 - Global Subtype Cache Persistence
**Status**: ⏳ PENDING
**Why**: Ensure `QueryCache` acts as long-lived, thread-safe store with correct context capture.

**Action**: Audit `RelationCacheKey` lifecycle for compiler flag handling.

**Files**: `src/solver/db.rs`, `src/solver/subtype.rs`

---

## Critical Gaps - Updated Status

### Gap A: Double Interning ⏳ PENDING
Some evaluation functions call `interner.intern()` directly, potentially bypassing canonicalization. This is what Task #46 addresses.

### Gap B: Subtype Memoization vs. Coinduction ✅ RESOLVED
Task #44 confirmed that comprehensive subtype caching is already implemented with correct handling of non-definitive results.

### Gap C: Literal/Primitive Intersection Soundness ⏳ PENDING
This is now Task #48. The `reduce_intersection_subtypes` function needs refinement for TypeScript's "boxing" rules.

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
