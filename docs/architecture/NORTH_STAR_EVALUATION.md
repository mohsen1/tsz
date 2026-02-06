# North Star Architecture Evaluation

**Date**: 2026-02-06
**Evaluator**: Automated codebase audit
**Baseline**: `docs/architecture/NORTH_STAR.md` v1.0

---

## Executive Summary

The codebase is **partially aligned** with the North Star architecture. Parser/Binder/Emitter components are well-architected (arena allocation, 16-byte nodes, parallel parsing). However, the **Solver and Checker have significant violations** that impact both performance and correctness.

### Scorecard

| Principle | Status | Performance Impact |
|-----------|--------|--------------------|
| Solver-First Architecture | **PARTIAL** - Checker has 49 TypeKey violations | Medium |
| Thin Wrappers (Checker) | **VIOLATED** - Checker contains type logic | Medium |
| Visitor Patterns | **HEAVILY VIOLATED** - 42:1 ratio of direct matching to visitor use | High |
| Arena Allocation | **GOOD** - 16-byte nodes verified, arenas in place | - |
| O(1) Type Equality | **GOOD** - TypeId interning works correctly | - |
| Zero Heap Alloc in Hot Paths | **HEAVILY VIOLATED** - 100s of allocations per check | **Critical** |
| File Size < 2000 Lines | **VIOLATED** - 35 files over limit | Low (maintainability) |
| No Duplicated Logic | **VIOLATED** - 40+ duplicated functions | Medium (correctness risk) |
| Parallel Parsing | **GOOD** - Rayon-based parallelism implemented | - |

---

## P0: CRITICAL PERFORMANCE VIOLATIONS

These directly degrade throughput and must be addressed first.

### 1. ~~SubtypeChecker Allocates 4 Hash Structures Per Construction~~ FIXED

**Status**: FIXED in commit `0f51522`

Added `reset()` method to `SubtypeChecker` that clears hash sets without deallocating. Applied reuse pattern in `narrowing.rs` (NarrowingVisitor, instanceof filtering), `contextual.rs`, and `element_access.rs`.

### 2. ~~TypeEvaluator Allocates 3 Hash Structures Per Construction~~ FIXED

**Status**: FIXED in commit `0f51522`

Added `reset()` method to `TypeEvaluator`. Same clear-without-dealloc pattern.

### 3. ~~FxHashMap/FxHashSet Created Inside Loops~~ ALREADY OPTIMIZED

**Status**: ALREADY OPTIMIZED (verified 2026-02-06)

Investigation found the codebase already uses the `.clear()` reuse pattern for all loop-internal hash sets. The `placeholder_visited` sets at lines 604/611 and 999/1006 are created before loops and cleared between iterations.

### 4. ~~`format!()` String Allocations in Generic Call Resolution~~ ALREADY FIXED

**Status**: ALREADY FIXED

The `format!("__infer_{}", var.0)` pattern has been removed from `operations.rs`.

### 5. ~~`std::collections::HashMap` Used Instead of `FxHashMap` in Hot Path~~ FIXED

**Status**: FIXED in commit `901d149`

All `std::collections::HashMap/HashSet` replaced with `FxHashMap/FxHashSet` across 23 files including solver, checker, CLI, LSP, transforms, and WASM modules.

### 6. `String` Used Where `Atom` Should Be (PARTIALLY FIXED)

**Status**: PARTIALLY FIXED

- **FIXED**: `TypeGuard::Typeof(String)` replaced with `TypeGuard::Typeof(TypeofKind)` enum — zero-allocation, Copy type.
- **BLOCKED**: `FlowFacts` String->Atom conversion blocked by separate interner architecture (parser `Interner` vs solver `ShardedInterner` are different instances with different atom numbering). FlowFacts is currently only used in tests, so impact is minimal.
- **BLOCKED**: Checker's `intern_string(&ident.escaped_text)` cannot be replaced with `ident.atom` because the parser atom namespace differs from the solver atom namespace.

### 7. `Vec<TypeId>` Instead of `SmallVec` (304 Sites) — OPEN

The codebase has **304 `Vec<TypeId>` usages** vs only **16 `SmallVec`** (all in `intern.rs`). Zero SmallVec usage in the checker.

Most unions have <8 members. Narrowing typically produces <8 results. Function signatures typically have <8 parameters.

**Worst offenders**:
- `src/solver/narrowing.rs`: 26 `Vec<TypeId>` sites (filtered union members)
- `src/solver/operations.rs`: multiple `Vec::new()` for return_types, failures, param_types
- `src/solver/operations_property.rs`: `app.args.clone()` called **14 times** (clones `Vec<TypeId>`)

**Fix**: Use `SmallVec<[TypeId; 8]>` for local collections. Use `Arc` or `Rc` for `app.args` to make clone O(1).

### 8. Signature Cloning in Call Resolution — OPEN

**File**: `src/solver/operations.rs:148-233, 2567-2570, 2895-2920, 3042-3066`

`sig.params.clone()`, `sig.type_params.clone()`, `sig.type_predicate.clone()` — each clones `Vec<ParamInfo>`, `Vec<TypeParamInfo>` etc. This pattern is repeated 6+ times in call resolution hot paths.

**Fix**: Use `Arc<[ParamInfo]>` for shared signature data, or pass by reference.

---

## P1: ARCHITECTURAL VIOLATIONS

These cause correctness risks and maintenance burden.

### 9. Checker Directly Inspects TypeKey (49 Violations)

**North Star Rule 3**: "Checker NEVER inspects type internals"

| Category | Count | Files |
|----------|-------|-------|
| `.lookup()` + TypeKey match | 22 | 9 checker files |
| `.intern(TypeKey::...)` construction | 26 | 11 checker files |
| Unused TypeKey import | 1 | 1 file |

**Worst offender files**:
- `checker/state_type_environment.rs` — 6 lookup+match
- `checker/iterators.rs` — 5 lookup+match
- `checker/type_computation_complex.rs` — 3 lookup+match
- `checker/control_flow.rs` — 2 lookup+match (includes `widen_to_primitive` reimplementation)

**Fix**: Add solver query methods (e.g., `solver.widen_to_primitive()`, `solver.is_abstract_type()`, `solver.extract_iterator_result()`) and have checker delegate.

### 10. Visitor Pattern Massively Underused (42:1 Ratio) — ANALYZED

**North Star Rule 2**: "Use visitor pattern for ALL type operations"

| Metric | Count |
|--------|-------|
| Direct `TypeKey::` matching in solver (excl visitor.rs) | 1,873 |
| Visitor imports/calls in solver | 45 |
| Match blocks with all 29 TypeKey variants | 7 |
| Match blocks with 5+ variants | 35 |

**Analysis of 7 functions matching all 29 TypeKey variants** (2026-02-06):

| File | Function | Replaceable? | Reason |
|------|----------|:---:|--------|
| `solver/infer.rs:725` | `type_contains_param()` | NO | Has scope shadowing logic for TypeParameter |
| `solver/operations.rs:1461` | `type_contains_placeholder()` | NO | Requires external `var_map` state |
| `solver/operations.rs:1643` | `is_contextually_sensitive()` | RISKY | Visitor checks ObjectWithIndex more thoroughly (false positives) |
| `solver/lower.rs:1481` | `contains_meta_type_inner()` | NO | Traverses Function.type_params that visitor does NOT |
| `solver/lower.rs:1821` | `collect_infer_bindings()` | NO | Collects data (not bool), needs full traversal control |
| `solver/evaluate_rules/infer_pattern.rs:46` | `type_contains_infer_inner()` | MAYBE | Needs verification of traversal equivalence |
| `solver/canonicalize.rs:87` | `canonicalize()` | NO | Transforms types, not just checks containment |

**Conclusion**: Only 2 of 7 are candidates, and both have subtle traversal differences. The `visitor::contains_type_matching` traversal differs from manual traversals in edge cases (e.g., Function.type_params constraints/defaults, ObjectWithIndex index signatures). Consolidation requires fixing these traversal gaps first.

**Fix**: First extend `visitor::check_key()` to traverse Function.type_params and ObjectWithIndex index signatures, then migrate the 2 candidate functions.

### 11. Duplicated Logic Between Components (40+ Functions)

**North Star Anti-Pattern 8.5**: "Duplicated Logic Between Components"

**Dangerous divergences** (same function name, different semantics):

| Function | Locations | Risk |
|----------|-----------|------|
| `is_unit_type` | solver/visitor.rs vs checker/control_flow.rs | **HIGH** — solver includes `NEVER`, enums, unique symbols; checker does not |
| `is_number_type` | solver/type_queries.rs vs checker/control_flow.rs | **MEDIUM** — checker includes number literals, solver does not |
| `is_enum_type` | 3 different implementations | **HIGH** — three different definitions of "enum type" |
| `get_property_type` | 3 independent implementations | **HIGH** — inconsistent handling of intersections and index signatures |
| `is_naked_type_parameter` | 3 implementations with completely different semantics | **MEDIUM** |

**Intra-solver duplication**: `type_queries.rs` and `visitor.rs` contain **14 parallel copies** of the same `is_*_type` functions.

**Fix**: Delete `type_queries.rs` duplicates, canonicalize on `visitor.rs` versions. Unify divergent implementations.

### 12. God Object Files (35 Files Over 2000 Lines)

**North Star**: "Each checker file under 2000 lines"

**Worst offenders**:

| File | Lines | Over by |
|------|------:|--------:|
| `bin/tsz_server/main.rs` | 4,687 | +2,687 |
| `solver/subtype.rs` | 4,174 | +2,174 |
| `declaration_emitter/mod.rs` | 4,165 | +2,165 |
| `solver/infer.rs` | 3,789 | +1,789 |
| `checker/type_checking.rs` | 3,703 | +1,703 |
| `solver/operations.rs` | 3,639 | +1,639 |
| `lsp/completions.rs` | 3,467 | +1,467 |
| `lsp/code_actions.rs` | 3,457 | +1,457 |
| `binder/state.rs` | 3,447 | +1,447 |
| `checker/control_flow.rs` | 3,445 | +1,445 |

11 solver files, 12 checker files, and 12 other files exceed the limit.

---

## What's Working Well

| Principle | Status |
|-----------|--------|
| **16-byte Node struct** | Verified at 16 bytes with `#[repr(C)]`, test assertions, 4 nodes per cache line |
| **Arena allocation** | NodeArena, SymbolArena, FlowNodeArena all properly indexed by u32 handles |
| **Type interning** | TypeId(u32) with global TypeInterner, O(1) equality |
| **String interning** | Atom(u32) with O(1) equality (though underused — see String findings above) |
| **Parallel parsing** | Rayon-based with clean macro abstraction in `parallel.rs` |
| **Binder separation** | Zero TypeId/TypeKey/solver imports — cleanly separated |
| **SmallVec in intern.rs** | `TypeListBuffer = SmallVec<[TypeId; 8]>` for union/intersection flattening |
| **SubtypeChecker reuse** | `expression_ops.rs` already reuses one checker for best_common_type |

---

## Recommended Priority Order

### Immediate (P0 Performance — highest ROI)

1. ~~**Pool SubtypeChecker and TypeEvaluator**~~ DONE — `reset()` + reuse in narrowing, contextual, element_access
2. ~~**Replace std HashMap with FxHashMap**~~ DONE — 23 files migrated
3. ~~**Hoist FxHashSet out of loops**~~ VERIFIED — already uses `.clear()` pattern
4. ~~**Convert FlowFacts to Atom keys**~~ PARTIALLY DONE — TypeGuard::Typeof fixed, FlowFacts blocked by interner architecture
5. ~~**Replace format!() in inference**~~ VERIFIED — already removed

### Next (Remaining P0 Performance)

6. **SmallVec for narrowing and call resolution** — 304 Vec<TypeId> → SmallVec where <8 elements typical
7. **Arc for shared signature data** — eliminates signature cloning in call resolution

### Short-term (P1 Architecture)

8. **Add solver query methods** for checker TypeKey violations — enables removing 49 violations
9. **Unify divergent duplicates** — fix `is_unit_type`, `is_enum_type`, `get_property_type`
10. **Extend visitor traversal** — add Function.type_params and ObjectWithIndex to `check_key()`
11. **Unify parser/solver interner** — enables FlowFacts Atom keys and ident.atom usage

### Medium-term (P1 Maintenance)

12. **Split God Objects** — especially subtype.rs (4174), operations.rs (3639), type_checking.rs (3703)
13. **Delete type_queries.rs duplicates** — 14 functions already exist in visitor.rs
