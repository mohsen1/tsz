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

### 1. SubtypeChecker Allocates 4 Hash Structures Per Construction

**File**: `src/solver/subtype.rs:1304-1363`

Every `SubtypeChecker::new()` allocates:
- `in_progress: FxHashSet<(TypeId, TypeId)>`
- `seen_refs: FxHashSet<(SymbolRef, SymbolRef)>`
- `seen_defs: FxHashSet<(DefId, DefId)>`
- `eval_cache: FxHashMap`

The free function `is_subtype_of()` at line 4058 creates a fresh checker per call. The codebase already acknowledges this was catastrophic for `best_common_type` (see `expression_ops.rs:170-172` comment about 262,144 allocations), but the pattern persists elsewhere.

**Fix**: Pool/reuse `SubtypeChecker` instances. Clear hash sets between uses instead of reallocating.

### 2. TypeEvaluator Allocates 3 Hash Structures Per Construction

**File**: `src/solver/evaluate.rs:56-67, 125-145`

Every `TypeEvaluator::new()` allocates:
- `cache: FxHashMap`
- `visiting: FxHashSet`
- `visiting_defs: FxHashSet`

The free function `evaluate_type()` at line 982-984 creates a fresh evaluator per call.

**Fix**: Same as SubtypeChecker — pool/reuse or thread-local caching.

### 3. FxHashMap/FxHashSet Created Inside Loops

**File**: `src/solver/operations.rs:749`

`FxHashSet::default()` is created **inside a loop body** during generic inference. Each iteration of the constraint-resolution loop allocates a new hash set.

Additional per-call allocations at lines 603-604, 642, 732, 816, 987-988, 1021, 1935, 2086-2087, 2110, 2584.

**Fix**: Hoist hash set allocation above the loop; clear between iterations.

### 4. `format!()` String Allocations in Generic Call Resolution

**File**: `src/solver/operations.rs:621, 999, 2092`

```rust
format!("__infer_{}", var.0)
```

Creates a heap-allocated `String` per type parameter per generic function call. Generic calls are extremely frequent.

**Fix**: Use a pre-interned naming scheme or use `TypeId`/index directly as the map key.

### 5. `std::collections::HashMap` Used Instead of `FxHashMap` in Hot Path

**File**: `src/solver/subtype.rs:367-421`

`TypeEnvironment` uses `std::collections::HashMap<u32, ...>` (SipHash) for **9 maps + 1 set**, all keyed by `u32`. This is in the subtype checking path.

Additional `std::HashMap/HashSet` instances: `src/solver/evaluate.rs:425,435`, `src/solver/infer.rs:3447-3478`, `src/checker/type_api.rs`, `src/checker/class_checker.rs`, `src/checker/state_type_environment.rs` — **43 occurrences across 15 files**.

**Fix**: Replace all `std::collections::HashMap/HashSet` with `FxHashMap/FxHashSet` in solver and checker.

### 6. `String` Used Where `Atom` Should Be

**File**: `src/solver/flow_analysis.rs:25-51`

`FlowFacts` uses `FxHashMap<String, TypeId>` and `FxHashSet<String>` for variable narrowings, definite assignments, and TDZ violations. These are created/merged on every control flow node.

Also: `NarrowingCondition::Typeof(String)` at `narrowing.rs:66` allocates a heap `String` for typeof checks.

~30+ additional `String`-keyed collections across 12 checker files.

**Fix**: Convert to `Atom` keys — they're interned u32 values with O(1) equality.

### 7. `Vec<TypeId>` Instead of `SmallVec` (304 Sites)

The codebase has **304 `Vec<TypeId>` usages** vs only **16 `SmallVec`** (all in `intern.rs`). Zero SmallVec usage in the checker.

Most unions have <8 members. Narrowing typically produces <8 results. Function signatures typically have <8 parameters.

**Worst offenders**:
- `src/solver/narrowing.rs`: 26 `Vec<TypeId>` sites (filtered union members)
- `src/solver/operations.rs`: multiple `Vec::new()` for return_types, failures, param_types
- `src/solver/operations_property.rs`: `app.args.clone()` called **14 times** (clones `Vec<TypeId>`)

**Fix**: Use `SmallVec<[TypeId; 8]>` for local collections. Use `Arc` or `Rc` for `app.args` to make clone O(1).

### 8. Signature Cloning in Call Resolution

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

### 10. Visitor Pattern Massively Underused (42:1 Ratio)

**North Star Rule 2**: "Use visitor pattern for ALL type operations"

| Metric | Count |
|--------|-------|
| Direct `TypeKey::` matching in solver (excl visitor.rs) | 1,873 |
| Visitor imports/calls in solver | 45 |
| Match blocks with all 29 TypeKey variants | 7 |
| Match blocks with 5+ variants | 35 |

**7 functions match all 29 TypeKey variants** to walk the type tree — each is a copy-paste of the same traversal:

| File | Function |
|------|----------|
| `solver/infer.rs:725` | `type_contains_param()` |
| `solver/operations.rs:1461` | `type_contains_placeholder()` |
| `solver/operations.rs:1643` | `is_contextually_sensitive()` |
| `solver/lower.rs:1481` | `contains_meta_type_inner()` |
| `solver/lower.rs:1821` | `collect_infer_bindings()` |
| `solver/evaluate_rules/infer_pattern.rs:46` | `type_contains_infer_inner()` |
| `solver/canonicalize.rs:87` | `canonicalize()` |

`visitor.rs` already provides `for_each_child()` and `contains_type_matching()` that do exactly this — but they're never used outside `visitor.rs`.

**Fix**: Replace all "contains" functions with `visitor::contains_type_matching()`. Replace structural walks with `for_each_child()`.

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

1. **Pool SubtypeChecker and TypeEvaluator** — eliminates thousands of hash allocations per file
2. **Replace std HashMap with FxHashMap** in TypeEnvironment (subtype.rs) — 9 maps on hot path
3. **Hoist FxHashSet out of loops** in operations.rs — eliminates per-iteration allocations
4. **Convert FlowFacts to Atom keys** — eliminates String allocations in control flow
5. **Replace format!() in inference** with pre-interned or index-based keys

### Short-term (P1 Architecture)

6. **Consolidate "contains" functions** — replace 7 copy-paste traversals with visitor calls
7. **Add solver query methods** for checker TypeKey violations — enables removing 49 violations
8. **Unify divergent duplicates** — fix `is_unit_type`, `is_enum_type`, `get_property_type`

### Medium-term (P1 Maintenance)

9. **SmallVec for narrowing and call resolution** — 304 Vec<TypeId> → SmallVec where <8 elements typical
10. **Arc for shared signature data** — eliminates signature cloning in call resolution
11. **Split God Objects** — especially subtype.rs (4174), operations.rs (3639), type_checking.rs (3703)
12. **Delete type_queries.rs duplicates** — 14 functions already exist in visitor.rs
