# Solver Improvement Plan — Conformance Gap Analysis

> **Baseline**: 65.0% (7987/12284) — 4425 failing tests
> **Date**: 2026-02-24
> **Goal**: Systematic plan to address solver-level conformance gaps, prioritized by test impact and implementation feasibility.

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Workstream 1: Property Resolution Precision (TS2339)](#2-workstream-1-property-resolution-precision-ts2339)
3. [Workstream 2: Assignability Precision (TS2322/TS2345)](#3-workstream-2-assignability-precision-ts2322ts2345)
4. [Workstream 3: False Positive Reduction](#4-workstream-3-false-positive-reduction)
5. [Workstream 4: Narrowing & Type Guards](#5-workstream-4-narrowing--type-guards)
6. [Workstream 5: Contextual Typing](#6-workstream-5-contextual-typing)
7. [Workstream 6: Class Type Checking](#7-workstream-6-class-type-checking)
8. [Workstream 7: Quick-Win Error Codes](#8-workstream-7-quick-win-error-codes)
9. [Workstream 8: Feature Gaps (satisfies, operators)](#9-workstream-8-feature-gaps)
10. [Execution Order & Priority Matrix](#10-execution-order--priority-matrix)

---

## 1. Executive Summary

Six parallel research agents analyzed the codebase against conformance failure data. The findings reveal that **solver-level precision gaps** in assignability and property resolution dominate failures, touching 1000+ tests. However, the highest-ROI fixes are **surgical changes** to the property resolution pipeline and false-positive hotspots.

### Impact Distribution

| Workstream | Est. Tests Impacted | Complexity | ROI |
|---|---|---|---|
| Property Resolution (TS2339) | ~390 | Medium | **Very High** |
| Assignability (TS2322/TS2345) | ~700+ | High | **High** |
| False Positive Reduction | ~649 | Medium | **High** |
| Narrowing & Type Guards | ~100+ | Medium-High | Medium |
| Contextual Typing | ~93 (TS7006) + 17 | Medium | Medium |
| Class Type Checking | ~215 | Medium | Medium |
| Quick-Win Error Codes | ~60+ | Low-Medium | **Very High** |
| Feature Gaps (satisfies, ops) | ~140+ | Medium-Hard | Low-Medium |

### Key Architectural Finding

The solver's property resolution pipeline has a **systemic `ANY` fallback pattern**: when the solver can't resolve a property on a complex type (conditional, mapped, indexed, lazy, etc.), it returns `PropertyAccessResult::Success { type_id: TypeId::ANY }` instead of `PropertyNotFound`. This single pattern accounts for a large share of both missing TS2339 errors AND downstream false positives in other error codes (properties that shouldn't exist appear as `any`, cascading through subsequent type checks).

---

## 2. Workstream 1: Property Resolution Precision (TS2339)

**Impact**: ~390 tests (243 false positives, 147 missing)
**Primary files**:
- `crates/tsz-solver/src/operations/property.rs` (main dispatch)
- `crates/tsz-solver/src/operations/property_visitor.rs` (visitor)
- `crates/tsz-solver/src/operations/property_helpers.rs` (helpers)
- `crates/tsz-solver/src/objects/index_signatures.rs` (index sigs)
- `crates/tsz-checker/src/error_reporter/properties.rs` (error emission)

### Phase 1: Fix `ANY` Fallback Pattern (Quick Wins — ~5-20 lines each)

These are the highest-ROI changes in the entire plan. Each is a small, targeted fix.

#### 1.1 Handle `NoInfer<T>` transparently
- **File**: `property.rs:857` (currently falls to `_ =>` catch-all)
- **Fix**: Add match arm before catch-all: `TypeData::NoInfer(inner) => self.resolve_property_access_inner(inner, prop_name, prop_atom)`
- **Lines**: ~3
- **Risk**: None — `NoInfer` is semantically transparent for property access

#### 1.2 Handle `StringIntrinsic` in fallback match
- **File**: `property.rs:857` (falls to catch-all when visitor returns None)
- **Fix**: Add arm: `TypeData::StringIntrinsic { .. } => self.resolve_string_property(prop_name, prop_atom)`
- **Lines**: ~5
- **Risk**: Low

#### 1.3 Change catch-all for known-bad types
- **File**: `property.rs:857-871`
- **Fix**: For `UniqueSymbol`, `BoundParameter`, `Recursive`, `Error` — return `PropertyNotFound` instead of `Success(ANY)`. Keep `ANY` fallback only for truly unknown variants where silence is safer.
- **Lines**: ~15
- **Risk**: Medium — may expose new TS2339 errors; verify with conformance run

#### 1.4 Fix `ThisType` to return `PropertyNotFound`
- **File**: `property.rs:802-814`
- **Fix**: When `ThisType` can't resolve apparent members, return `PropertyNotFound` instead of `ANY`. The checker should resolve `this` before reaching the solver.
- **Lines**: ~3
- **Risk**: Low — surfaces errors that were previously hidden

#### 1.5 Fix `Lazy` resolution failure to return `PropertyNotFound`
- **File**: `property.rs:830-849`
- **Fix**: When `resolve_lazy(def_id)` fails and apparent members don't match, return `PropertyNotFound` instead of `ANY`.
- **Lines**: ~3
- **Risk**: Low-Medium — may need to verify broken lazy refs don't cascade

### Phase 2: Fix Index Signature Resolution for Composite Types

#### 2.1 Union index signature resolution
- **File**: `objects/index_signatures.rs:79-82`
- **Current**: `StringIndexResolver::visit_union` returns first member with index sig
- **Fix**: Collect index signatures from ALL union members, return union of value types
- **Lines**: ~20
- **Risk**: Medium

#### 2.2 Intersection index signature resolution
- **File**: `objects/index_signatures.rs:84-88`
- **Current**: Returns first member's index sig
- **Fix**: Collect from ALL intersection members, return intersection of value types
- **Lines**: ~20
- **Risk**: Medium

#### 2.3 `IndexInfoCollector` for composite types
- **File**: `objects/index_signatures.rs:271-284`
- **Current**: `visit_union` and `visit_intersection` return empty
- **Fix**: Collect and merge index info from all members
- **Lines**: ~30
- **Risk**: Medium

### Phase 3: Fix Intersection and Application Property Resolution

#### 3.1 Intersection property type merging
- **File**: `property.rs:446-550`
- **Current**: Index signature fallback returns FIRST found, not intersection of all
- **Fix**: When multiple intersection members have the same property via index signatures, intersect their types
- **Lines**: ~40
- **Risk**: Medium-High

#### 3.2 Application type parameter substitution
- **File**: `property_helpers.rs:385-396`
- **Current**: Uses `get_array_base_type_params()` for ALL Application types (array-specific)
- **Fix**: Use `get_type_params(symbol_ref)` or `get_lazy_type_params(def_id)` for non-array generics
- **Lines**: ~30
- **Risk**: Medium — requires understanding of type param resolution chain

### Phase 4: Fix Checker-Side Suppression

#### 4.1 Narrow TS2339 suppression for unions with type parameters
- **File**: `error_reporter/properties.rs:40-44`
- **Current**: Suppresses TS2339 for ANY union containing type parameters
- **Fix**: Only suppress when type parameters are the direct cause (e.g., `T | string` where property exists on `string` but not `T`)
- **Lines**: ~15
- **Risk**: Medium — may expose valid TS2339 errors that were incorrectly suppressed

---

## 3. Workstream 2: Assignability Precision (TS2322/TS2345)

**Impact**: ~700+ tests (426 false positives, 297 missing)
**Primary files**:
- `crates/tsz-solver/src/relations/subtype.rs` (SubtypeChecker — judge)
- `crates/tsz-solver/src/relations/compat.rs` (CompatChecker — lawyer)
- `crates/tsz-solver/src/relations/subtype_visitor.rs` (visitor dispatch)
- `crates/tsz-solver/src/relations/subtype_cache.rs` (caching + cycle detection)
- `crates/tsz-solver/src/relations/compat_overrides.rs` (enum overrides)
- `crates/tsz-solver/src/relations/freshness.rs` (freshness tracking)
- `crates/tsz-checker/src/query_boundaries/assignability.rs` (gate)
- `crates/tsz-checker/src/assignability/assignability_checker.rs` (checker entry)

### Phase 1: Intersection Freshness (TS2353 — ~48 false positives)

#### 2.1 Fix intersection freshness propagation
- **File**: `intern/intersection.rs:384-385`
- **Current**: `FRESH_LITERAL` flag propagates from ANY constituent to merged intersection
- **Fix**: Only propagate freshness when the intersection is created from a direct object literal expression, not from computed intersections
- **Lines**: ~15
- **Risk**: Medium — needs careful testing of excess property check behavior

#### 2.2 Type parameter bail-out in intersection EPC
- **File**: `assignability_checker.rs:1109-1137`
- **Current**: Intersection handler skips type parameters in property collection
- **Fix**: When an intersection member is a type parameter, bail out of excess property checking (type params accept any properties)
- **Lines**: ~10
- **Risk**: Low

### Phase 2: Weak Type Detection (TS2559)

#### 2.3 Fix weak type bypass for indexed types
- **File**: `compat.rs:1040-1044`
- **Current**: `ObjectWithIndex` types bypass weak type detection entirely
- **Fix**: Only bypass when the index signature is meaningful (not `any` index)
- **Lines**: ~10
- **Risk**: Low-Medium

### Phase 3: `any` Propagation Precision

#### 2.4 Fix `TopLevelOnly` depth semantics
- **File**: `subtype.rs:88-104`, `lawyer.rs:184-189`
- **Current**: `allows_any_at_depth()` uses `guard.depth()` which increments on every `check_subtype` call, making `TopLevelOnly` effectively "only outermost comparison"
- **Fix**: Track "structural depth" (entering object/function/tuple) separately from "recursion depth" (any check_subtype call). `TopLevelOnly` should allow `any` at structural depth 0-1.
- **Lines**: ~30
- **Risk**: Medium — requires careful behavioral verification

### Phase 4: Callable/Intersection Improvements

#### 2.5 Fix callable-to-interface heuristic
- **File**: `subtype.rs:879-916`
- **Current**: Accepts function values when interface has only `call` or `apply`, uses `required_props.len() == 1` heuristic
- **Fix**: Check that the function actually satisfies the call/apply signature, and verify no other required properties exist beyond call/apply/bind/length
- **Lines**: ~30
- **Risk**: Medium

#### 2.6 Fix source intersection callable merging
- **File**: `subtype_visitor.rs:169-241`
- **Current**: For intersection sources against function targets, only tests individual members
- **Fix**: When target is callable and source is intersection, merge call signatures from intersection members
- **Lines**: ~50
- **Risk**: Medium-High — intersection call signature merging is complex

### Phase 5: Evaluation & Diagnostic Consistency

#### 2.7 Fix explain-failure using fresh compat checker
- **File**: `query_boundaries/assignability.rs:248-266`
- **Current**: Failure explanation creates a fresh `CompatChecker`, potentially with different config
- **Fix**: Reuse the same configuration/state from the original check
- **Lines**: ~15
- **Risk**: Low

#### 2.8 Fix cycle detection for evaluated types
- **File**: `subtype_cache.rs:303-320`
- **Current**: When evaluation produces a different-but-equivalent type, recursive `check_subtype` may hit cycle detection and return `true` (CycleDetected), hiding errors
- **Fix**: Track evaluation results separately in the cycle detection cache; don't conflate pre-evaluation and post-evaluation types
- **Lines**: ~30
- **Risk**: High — cycle detection is subtle

### Phase 6: Enum Assignability Edge Cases

#### 2.9 Fix numeric enum literal assignability
- **File**: `compat_overrides.rs:356-365`
- **Current**: Numeric enum targets allow bare `number`, but specific literals may pass structural check when they shouldn't
- **Fix**: For numeric enum targets, check that the source literal is actually a member value
- **Lines**: ~20
- **Risk**: Medium

---

## 4. Workstream 3: False Positive Reduction

**Impact**: ~649 false positive tests across multiple error codes
**Cross-cutting theme**: Many false positives stem from incomplete type evaluation before error checking.

### TS2304 "Cannot find name" — 87 false positives

#### 3.1 Fix uppercase name heuristic masking
- **File**: `types/computation/identifier.rs:856`
- **Current**: Any unresolved uppercase name silently returns `TypeId::ANY`
- **Fix**: Only apply this heuristic in specific contexts (type position, not value position), or remove it and let resolution fail properly
- **Lines**: ~15
- **Risk**: Medium — may expose many new TS2304 errors; needs conformance verification

#### 3.2 Fix binder lib symbol merge bug
- **File**: `symbols/symbol_resolver.rs:179-218`
- **Current**: After `lib_symbols_merged = true`, binder skips lib lookup; fallback only checks `file_locals` by name
- **Fix**: Ensure lib symbol lookup continues to work after merge, checking nested namespaces and declaration merging
- **Lines**: ~30
- **Risk**: Medium

#### 3.3 Add global/module augmentation support
- **File**: Binder (`crates/tsz-binder/`)
- **Current**: `declare global {}` and `declare module "..." {}` augmentation not supported
- **Fix**: Implement augmentation merging in the binder's symbol table construction
- **Lines**: ~100+
- **Risk**: High — fundamental binder feature, but high impact

### TS7006 "Parameter implicitly has 'any'" — 76 false positives

*See Workstream 5 (Contextual Typing) — these share the same root causes.*

### TS2353 "Excess property check" — 48 false positives

*See Workstream 2 Phase 1 (items 2.1, 2.2) — intersection freshness fixes.*

### TS2554 "Expected N arguments, but got M" — 43 false positives

#### 3.4 Fix `this` parameter exclusion in arg count
- **File**: `operations/call_args.rs:102`
- **Current**: May count `this` parameter in expected count
- **Fix**: Explicitly exclude `this` parameter from `arg_count_bounds`
- **Lines**: ~5
- **Risk**: Low

#### 3.5 Fix spread argument expansion
- **File**: `operations/call_args.rs`
- **Current**: Spread arguments with tuple types compute bounds from tuple, but generic arrays don't expand properly
- **Fix**: When spread argument is generic array, treat as variable-length (min=0, max=infinity)
- **Lines**: ~15
- **Risk**: Low-Medium

#### 3.6 Fix overload arg-count collapse
- **File**: `operations/mod.rs:958-992`
- **Current**: Collapses all overload failures into one TS2554 with min/max across all signatures
- **Fix**: Use the closest-matching overload's expected count, not the global min/max
- **Lines**: ~20
- **Risk**: Medium

### TS2769 "No overload matches" — 38 false positives

#### 3.7 Add relaxed assignability mode for overload selection
- **File**: `checkers/call_checker.rs:503-651`
- **Current**: Uses strict assignability for overload candidate selection
- **Fix**: Use relaxed mode during candidate selection (allow `any` to match, bivariant callbacks), then strict mode for the selected overload
- **Lines**: ~40
- **Risk**: Medium — need to match tsc's two-phase overload resolution semantics

#### 3.8 Improve generic type parameter inference for callbacks
- **File**: Solver generic inference module
- **Current**: Inference may fail for complex patterns (conditional/mapped types in callback positions)
- **Fix**: Incremental improvement — ensure basic callback inference works before attempting complex cases
- **Lines**: ~50+
- **Risk**: Medium-High

---

## 5. Workstream 4: Narrowing & Type Guards

**Impact**: ~100+ tests (type guards 1.6%, optional chaining 4.0%)
**Key finding**: The solver narrowing machinery is **comprehensive** — the issue is likely checker integration, not solver deficiency.

**Primary files**:
- `crates/tsz-solver/src/narrowing/` (all submodules)
- `crates/tsz-checker/src/flow/control_flow.rs` (FlowAnalyzer)
- `crates/tsz-checker/src/flow/control_flow_condition_narrowing.rs`
- `crates/tsz-checker/src/flow/control_flow_type_guards.rs`

### Phase 1: Diagnose the Root Cause

#### 4.1 Run specific failing type guard tests with tracing
- **Action**: Pick 5-10 representative failing type guard tests, run with tracing enabled
- **Goal**: Determine whether failures are in: (a) the solver's narrow_type(), (b) the checker's condition analysis, (c) type computation feeding into narrowing, or (d) flow graph construction
- **This is investigation, not implementation** — results determine the rest of the phase

### Phase 2: Optional Chaining Narrowing Fixes

#### 4.2 Fix end-of-chain undefined propagation
- **File**: `flow/control_flow_condition_narrowing.rs:394-406`
- **Current**: `x?.y?.z` may not correctly propagate undefined through chained accesses
- **Fix**: When narrowing optional chain, ensure each link in the chain propagates the undefined possibility
- **Lines**: ~30
- **Risk**: Medium

#### 4.3 Fix false branch for optional call + type predicate
- **File**: `flow/control_flow_condition_narrowing.rs:326-330`
- **Current**: False branch returns type_id unchanged for optional calls
- **Fix**: On false branch of `obj?.isString(x)`, narrow to include the case where `obj` is nullish
- **Lines**: ~20
- **Risk**: Medium

### Phase 3: Missing Narrowing Features

#### 4.4 Implement `Symbol.hasInstance` for instanceof
- **File**: `narrowing/instanceof.rs:39`
- **Current**: TODO comment — custom instanceof via `[Symbol.hasInstance]` not implemented
- **Fix**: Check target type for static `[Symbol.hasInstance]` method and use its return type predicate
- **Lines**: ~40
- **Risk**: Low-Medium

#### 4.5 Clean up dead code (`FlowFacts`/`FlowTypeEvaluator`)
- **File**: `crates/tsz-solver/src/flow_analysis.rs:1-231`
- **Current**: Appears to be dead code — older approach with string-based variable names
- **Fix**: Verify unused and remove
- **Lines**: -231 (deletion)
- **Risk**: Low

---

## 6. Workstream 5: Contextual Typing

**Impact**: ~93 tests (76 TS7006 false positives + 17 contextualTyping tests at 0%)
**Key finding**: The solver's `ContextualTypeContext` is **comprehensive**. The gap is in the checker's propagation of contextual types to all required positions.

**Primary files**:
- `crates/tsz-solver/src/contextual/mod.rs` (solver module — well-implemented)
- `crates/tsz-checker/src/types/function_type.rs` (checker integration)
- `crates/tsz-checker/src/state/state_checking_members/implicit_any_checks.rs`

### Phase 1: Contextual Type Propagation

#### 5.1 Track and re-check closures that skipped TS7006
- **File**: `types/function_type.rs:400-401`
- **Current**: Closures skip TS7006 during `build_type_environment` phase, may never be re-checked
- **Fix**: Track which closures were skipped and ensure they're re-evaluated with context during statement checking
- **Lines**: ~30
- **Risk**: Medium

#### 5.2 Propagate contextual types through generic instantiation
- **File**: `types/function_type.rs:162-177`
- **Current**: Evaluates Application/Lazy/IndexAccess/KeyOf but not Conditional/Mapped/TemplateLiteral
- **Fix**: Extend evaluation to cover Conditional, Mapped, and TemplateLiteral contextual types
- **Lines**: ~30
- **Risk**: Medium

#### 5.3 Propagate context to return statements, spreads, ternary branches
- **File**: Checker dispatch/expression checking modules
- **Current**: Contextual type is set in some positions but may not reach return statements in nested functions, spread arguments, or both branches of ternary expressions
- **Fix**: Audit and fix all positions where tsc propagates contextual types
- **Lines**: ~50+
- **Risk**: Medium — needs systematic audit

### Phase 2: Overload Resolution with Context

#### 5.4 Improve overload signature selection with contextual types
- **File**: `checkers/call_checker.rs`
- **Current**: Two-pass approach (union context then per-signature) may not be specific enough
- **Fix**: Match tsc's approach of trying each overload with contextual type inference
- **Lines**: ~50+
- **Risk**: Medium-High

---

## 7. Workstream 6: Class Type Checking

**Impact**: ~215 tests (53.7% pass rate on 464 tests)
**Key finding**: Basic class infrastructure is solid. Gaps are in cross-file resolution, transitive inheritance, and missing error codes.

**Primary files**:
- `crates/tsz-checker/src/classes/` (all submodules)
- `crates/tsz-checker/src/types/class_type.rs`
- `crates/tsz-solver/src/classes/`

### Phase 1: Quick Wins

#### 6.1 Fix transitive abstract member checking
- **File**: `classes/class_implements_checker.rs:56-226`
- **Current**: Only checks abstract members from direct base class
- **Fix**: Walk up the full inheritance chain when collecting abstract members
- **Lines**: ~30
- **Risk**: Low-Medium

#### 6.2 Fix heritage symbol resolution (file_locals scope)
- **File**: `classes/class_implements_checker.rs:104`, `class_checker_compat.rs:992`
- **Current**: Resolves base class via `file_locals.get()` — only finds symbols in current file
- **Fix**: Use `resolve_identifier()` or `resolve_heritage_symbol()` (already exists in `class_inheritance.rs`)
- **Lines**: ~15
- **Risk**: Low

#### 6.3 Add constraint checking at extends sites (TS2344)
- **File**: `classes/class_checker.rs:685-707`
- **Current**: Type arguments in `extends Base<T>` are resolved but not constraint-checked
- **Fix**: After resolving type arguments, verify each satisfies its constraint
- **Lines**: ~20
- **Risk**: Low-Medium

### Phase 2: Missing Error Codes

#### 6.4 Implement TS2507 (extends non-constructor)
- **File**: `classes/class_inheritance.rs:243-258`
- **Current**: Non-identifier/property-access extends expressions are silently ignored
- **Fix**: When heritage expression doesn't resolve to a construct signature, emit TS2507
- **Lines**: ~25
- **Risk**: Medium

#### 6.5 Implement TS2684 (`this` context type checking)
- **File**: `classes/class_checker.rs` or new file
- **Current**: Not implemented
- **Fix**: When calling a method, check that `this` parameter type is compatible with the call site's `this` type
- **Lines**: ~50
- **Risk**: Medium

#### 6.6 Fix visibility conflict matrix
- **File**: `classes/class_checker_compat.rs:1017-1032`
- **Current**: `(Protected, Private)` always treated as conflict
- **Fix**: Align with tsc's visibility override rules
- **Lines**: ~10
- **Risk**: Low

### Phase 3: Architectural Improvements

#### 6.7 Activate solver-side `merge_properties`
- **File**: `crates/tsz-solver/src/classes/class_hierarchy.rs:16`
- **Current**: `merge_properties` is `#[cfg(test)]` only
- **Fix**: Activate for production use, move class member merging from checker to solver (aligns with architecture spec)
- **Lines**: ~50+
- **Risk**: High — significant refactor

---

## 8. Workstream 7: Quick-Win Error Codes

**Impact**: ~60+ tests from small, targeted changes
**These are the fastest path to improving the conformance number.**

| Priority | Code | Tests | Description | Difficulty | Where |
|---|---|---|---|---|---|
| 1 | TS2433 | 10 | Already implemented! | Test infra | Multi-file test handling |
| 2 | TS7017 | 6 | No index signature + noImplicitAny | Low-Medium | `types/computation/access.rs` |
| 3 | TS1382 | 8 | JSX unexpected token suggestion | Low-Medium | Parser JSX module |
| 4 | TS17019 | ~8 | JSDoc syntax in TS files | Low-Medium | Parser/checker |
| 5 | TS2550 | 9 | Property not found + lib suggestion | Medium | Property error reporter |
| 6 | TS2497 | 13 | Module ESM import restriction | Medium-Hard | Import checker |
| 7 | TS6046 | 8 | Compiler option validation | Medium | CLI/config |

### Implementation Details

#### 7.1 TS7017 — Element implicitly has 'any' type (no index signature)
- **File**: `crates/tsz-checker/src/types/computation/access.rs`
- **Fix**: When element access on a type with no matching index signature resolves to `any` and `noImplicitAny` is enabled, emit TS7017
- **Lines**: ~20

#### 7.2 TS1382 — JSX unexpected token suggestion
- **File**: `crates/tsz-parser/src/parser/` (JSX parsing module)
- **Fix**: When parsing JSX text content and encountering bare `>` or `}`, emit TS1382 with entity suggestion
- **Lines**: ~25

#### 7.3 TS2550 — Property not found + lib suggestion
- **File**: `crates/tsz-checker/src/error_reporter/properties.rs`
- **Fix**: Add a mapping of well-known property names to required lib versions (e.g., `includes` -> ES2016, `at` -> ES2022). When TS2339 would fire on a known type, emit TS2550 instead.
- **Lines**: ~40 (including mapping table)

#### 7.4 TS7017 co-occurrence with TS2339
- When index access fails AND no index signature exists, choose TS7017 (implicit any) over TS2339 (property not found) based on access syntax (bracket vs dot).

---

## 9. Workstream 8: Feature Gaps

### `satisfies` (0/16 tests)

**Status**: Fully implemented for `.ts` files. All 16 conformance tests are JSDoc `@satisfies` in `.js` files.

#### 8.1 Fix JSDoc `@satisfies` in JS files
- **File**: `types/utilities/jsdoc.rs:216-263`, checker dispatch
- **Issue**: Multi-file test handling + JSDoc `@typedef` resolution in JS files
- **Lines**: ~50
- **Risk**: Medium
- **Note**: The core `satisfies` logic works; this is an infrastructure gap

### Binary/Unary Operators (3-4.5% pass rate)

**Status**: Extensively implemented — infrastructure is solid. The ~95% failure rate suggests systemic issues rather than missing operators.

#### 8.2 Improve type resolution depth for operands
- **File**: `types/computation/binary.rs`, solver `operations/binary_ops.rs`
- **Issue**: Complex types (generics, enums, type parameters, conditionals) as operands may not fully resolve before operator checking
- **Fix**: Ensure `evaluate_type()` is called on both operands before operator dispatch
- **Lines**: ~30
- **Risk**: Medium

#### 8.3 Fix enum arithmetic edge cases
- **File**: `operations/binary_ops.rs`
- **Issue**: Enum type arithmetic has precision gaps
- **Lines**: ~30
- **Risk**: Medium

---

## 10. Execution Order & Priority Matrix

### Tier 0: Immediate Quick Wins (1-2 days each, high confidence)

These changes are small, well-scoped, and have disproportionate impact:

| # | Item | Tests | Lines | Risk |
|---|---|---|---|---|
| 1 | **1.1** NoInfer property transparency | ~5+ | 3 | None |
| 2 | **1.2** StringIntrinsic fallback arm | ~5+ | 5 | Low |
| 3 | **1.4** ThisType → PropertyNotFound | ~5+ | 3 | Low |
| 4 | **1.5** Lazy failure → PropertyNotFound | ~5+ | 3 | Low |
| 5 | **3.4** `this` param exclusion in arg count | ~10+ | 5 | Low |
| 6 | **6.2** Heritage symbol resolution fix | ~10+ | 15 | Low |
| 7 | **2.2** Type param bail-out in intersection EPC | ~10+ | 10 | Low |

**Estimated total**: ~50+ tests recovered, <50 lines changed

### Tier 1: High-ROI Medium Effort (3-5 days each)

| # | Item | Tests | Lines | Risk |
|---|---|---|---|---|
| 8 | **1.3** Catch-all → PropertyNotFound for known types | ~30+ | 15 | Medium |
| 9 | **2.1** Intersection freshness propagation fix | ~48 | 15 | Medium |
| 10 | **4.1** Diagnose type guard root cause (investigation) | ~63 | 0 | — |
| 11 | **7.1** TS7017 implementation | 6 | 20 | Low-Med |
| 12 | **7.2** TS1382 implementation | 8 | 25 | Low-Med |
| 13 | **6.1** Transitive abstract member checking | ~15+ | 30 | Low-Med |
| 14 | **4.1** TS2339 suppression narrowing (unions+type params) | ~20+ | 15 | Medium |
| 15 | **5.1** Closure TS7006 re-check tracking | ~20+ | 30 | Medium |
| 16 | **3.6** Overload arg-count: closest match | ~15+ | 20 | Medium |

**Estimated total**: ~260+ tests recovered

### Tier 2: Significant Effort, High Impact (1-2 weeks each)

| # | Item | Tests | Risk |
|---|---|---|---|
| 17 | **2.4** `any` propagation depth semantics | ~30+ | Medium |
| 18 | **2.5** Callable-to-interface heuristic fix | ~20+ | Medium |
| 19 | **3.7** Relaxed assignability for overload selection | ~38 | Medium |
| 20 | **5.2-5.3** Contextual type propagation audit | ~50+ | Medium |
| 21 | **Phase 2** Index signature composite type fixes | ~30+ | Medium |
| 22 | **Phase 3** Intersection/Application property fixes | ~50+ | Med-High |
| 23 | **6.3-6.5** Class missing error codes | ~20+ | Medium |
| 24 | **7.3** TS2550 lib suggestion mapping | 9 | Medium |
| 25 | **8.2** Operator type resolution depth | ~50+ | Medium |

**Estimated total**: ~300+ tests recovered

### Tier 3: Large Architectural Work (2+ weeks each)

| # | Item | Tests | Risk |
|---|---|---|---|
| 26 | **3.3** Global/module augmentation in binder | ~50+ | High |
| 27 | **2.6** Intersection callable merging | ~20+ | Med-High |
| 28 | **2.8** Cycle detection for evaluated types | ~15+ | High |
| 29 | **5.4** Overload resolution with contextual inference | ~30+ | Med-High |
| 30 | **6.7** Solver-side merge_properties activation | ~20+ | High |

**Estimated total**: ~135+ tests recovered

---

## Appendix: Cross-Cutting Observations

### The `ANY` Fallback Pattern
The single most pervasive issue across the codebase is the pattern of returning `TypeId::ANY` when type resolution fails. This was likely an early design decision to avoid cascading errors, but it now:
- Hides genuine errors (missing TS2339, TS2322, etc.)
- Causes downstream false positives (properties/assignments that work because one side is `any`)
- Makes it hard to distinguish "intentionally any" from "failed to resolve"

**Recommendation**: Introduce a `TypeId::RESOLUTION_FAILED` sentinel (or use `TypeId::ERROR`) that suppresses downstream errors (like `any` does) but is distinguishable from legitimate `any`. This would allow tracking resolution failures without cascading noise.

### Single-File vs Multi-File Gap
Many false positives (TS2304, TS2433, class heritage resolution) stem from the conformance test harness running in a mode that expects multi-file/module resolution capabilities. Improving the test harness or adding basic multi-file support would unlock a non-trivial number of tests without any solver changes.

### Evaluation Depth
Multiple workstreams (property resolution, assignability, operators) share the problem of types not being fully evaluated before checks. A systematic "evaluate-before-check" pass at the solver boundary could fix multiple categories simultaneously.
