# Performance Audit Report — 2026-02-05

## Executive Summary

**tsz is losing 9 of 11 benchmarks vs tsgo.** The root causes are:

1. **Canonicalizer on the hot path** (commit `23478b2`) — runs on EVERY non-trivial subtype check, allocates fresh HashMap+Vecs, triggers O(n²) union reduction
2. **O(n²) `Vec::remove(i)` in union/intersection reduction** — shifts elements on every removal
3. **O(n²) `Vec.contains()` throughout inference** — linear scans where HashSet should be used
4. **No cross-invocation subtype memoization in BCT** — resets checker state between candidates

These are all algorithmic/structural issues, not micro-optimization problems.

---

## Benchmark Results (Current)

| Test | tsz (ms) | tsgo (ms) | Factor | Root Cause |
|------|----------|-----------|--------|------------|
| enumLiteralsSubtypeReduction.ts | 1735.67 | 178.20 | **9.74x slower** | Canonicalizer + O(n²) union reduction |
| BCT candidates=50 | 461.64 | 193.62 | **2.38x slower** | O(n²) BCT tournament + no memoization |
| 50 generic functions | 391.07 | 173.81 | **2.25x slower** | Vec.contains() in inference + constraint conflict |
| Recursive generic depth=25 | 238.59 | 148.57 | **1.61x slower** | Canonicalizer overhead on recursive types |
| Mapped type keys=100 | 239.68 | 162.46 | **1.48x slower** | Per-property conditional evaluation overhead |
| Constraint conflicts N=30 | 162.73 | 128.83 | **1.26x slower** | O(n²) conflict detection with full subtype checks |
| Conditional dist N=50 | 281.57 | 226.47 | **1.24x slower** | Canonicalizer on conditional type expansion |
| Mapped complex template keys=50 | 149.07 | 138.49 | **1.08x slower** | Minor overhead |
| 100 classes | 266.70 | 253.27 | **1.05x slower** | Near-parity |
| manyConstExports.ts | 269.43 | 310.46 | **1.15x faster** | OK |
| largeControlFlowGraph.ts | 278.67 | 925.31 | **3.32x faster** | Our CFA is excellent |

---

## Issue #1: Canonicalizer on Hot Path (CRITICAL — estimated 3-5x impact)

**Commit:** `23478b2` — "integrate Canonicalizer as fast-path in SubtypeChecker"

### The Problem

In `src/solver/subtype.rs:1578-1582`, the canonicalizer runs on **every** `check_subtype_inner` call:

```rust
// Line 1578
if self.is_potentially_structural(source) && self.is_potentially_structural(target) {
    if self.are_types_structurally_identical(source, target) {
        return SubtypeResult::True;
    }
}
```

`is_potentially_structural()` (line 3303-3312) returns `true` for **everything except Intrinsic and Literal types**. This means Unions, Tuples, Objects, Functions, Refs, Enums, Arrays — all trigger canonicalization.

### Why It's Expensive

Each call to `are_types_structurally_identical()` (line 3318-3329):
1. **Allocates a fresh `Canonicalizer`** with empty `FxHashMap` cache, two empty `Vec`s
2. **Canonicalizes both types from scratch** — no cache reuse between calls
3. **For unions: calls `interner.union(sorted)`** which triggers `normalize_union()` → `reduce_union_subtypes()` — **O(n²)**
4. **For objects: clones every `PropertyInfo`** struct

### Cascade Effect

For `enumLiteralsSubtypeReduction.ts` with 512 enum members:
- Checking union_source subtype: 512 members × `check_subtype` per member
- Each `check_subtype` → `check_subtype_inner` → canonicalize both source AND target
- Each canonicalization of the 512-member union: sort (O(n log n)) + `interner.union()` → `reduce_union_subtypes()` (O(n²))
- **Total: ~512 × 512² = ~134 million operations**

### The Irony

The canonicalizer is meant to be a "fast path" but it's doing MORE work than the regular subtype check it's trying to skip. The regular subtype check has the QueryCache and is often O(1) for cached results. The canonicalizer has no cross-invocation cache.

### Fix

**Option A (immediate):** Only run canonicalization for types that are actually recursive (Lazy → TypeAlias). Non-recursive types won't benefit from De Bruijn canonicalization.

```rust
// Replace is_potentially_structural with is_recursive_structural
fn is_recursive_structural(&self, type_id: TypeId) -> bool {
    matches!(self.interner.lookup(type_id), Some(TypeKey::Lazy(_)))
}
```

**Option B (better):** Remove canonicalization from the subtype hot path entirely. Use it only for the specific case it was designed for: detecting structural identity of recursive type aliases. Or add a global canonicalization cache so results persist.

---

## Issue #2: O(n²) Vec::remove() in Union/Intersection Reduction (HIGH — estimated 2-4x impact on enum benchmark)

### The Problem

In `src/solver/intern.rs:1984-2002`:

```rust
fn reduce_union_subtypes(&self, flat: &mut TypeListBuffer) {
    // ... early return optimization for unit types (lines 1961-1982) ...

    let mut i = 0;
    while i < flat.len() {
        let mut redundant = false;
        for j in 0..flat.len() {           // O(n) inner loop
            if i == j { continue; }
            if self.is_subtype_shallow(flat[i], flat[j]) {
                redundant = true;
                break;
            }
        }
        if redundant {
            flat.remove(i);                 // O(n) shift per removal!
        } else {
            i += 1;
        }
    }
}
```

`Vec::remove(i)` shifts all elements after `i`, making each removal O(n). With multiple removals in the O(n²) loop, worst case is **O(n³)**.

The same pattern exists in:
- `reduce_intersection_subtypes()` at line 2007-2027
- Callable extraction in `intersect_types_raw()` at line 998

### The Mitigation That Should Help But Doesn't Always

There IS an early-return optimization (lines 1961-1982) that skips the O(n²) loop when all types are "non-reducible" (unit types, tuples, arrays, objects, enums). For `enumLiteralsSubtypeReduction.ts`, the tuples SHOULD hit this fast path.

**However**, the canonicalizer creates NEW union types via `interner.union()` during canonicalization (line 152 of canonicalize.rs). These fresh unions go through `normalize_union()` → `reduce_union_subtypes()` again, potentially with different member compositions that DON'T hit the early return.

### Fix

Replace `Vec::remove(i)` with a retain-based approach:

```rust
fn reduce_union_subtypes(&self, flat: &mut TypeListBuffer) {
    // ... early return checks ...

    let len = flat.len();
    let mut keep = vec![true; len];
    for i in 0..len {
        if !keep[i] { continue; }
        for j in 0..len {
            if i == j || !keep[j] { continue; }
            if self.is_subtype_shallow(flat[i], flat[j]) {
                keep[i] = false;
                break;
            }
        }
    }
    let mut write = 0;
    for read in 0..len {
        if keep[read] {
            flat[write] = flat[read];
            write += 1;
        }
    }
    flat.truncate(write);
}
```

This eliminates the O(n) shift per removal, reducing worst case from O(n³) to O(n²).

---

## Issue #3: O(n²) Vec.contains() in Inference (HIGH — estimated 1.5-2x impact on BCT/generic benchmarks)

### The Problem

Throughout `src/solver/infer.rs`, `Vec.contains()` (O(n) linear scan) is used where a `HashSet` would give O(1):

**Constraint deduplication** (line 81-83):
```rust
for candidate in &b.candidates {
    if !merged.candidates.contains(candidate) {  // O(n) scan!
        merged.candidates.push(candidate.clone());
    }
}
```

**Bound addition** (lines 169, 176):
```rust
if !self.lower_bounds.contains(&ty) {  // O(n) scan!
    self.lower_bounds.push(ty);
}
```

**Class hierarchy** (line 1702):
```rust
if hierarchy.contains(&ty) {  // O(n) scan!
    return;
}
```

**Constraint resolution** (line 1388):
```rust
if !upper_bounds.contains(&bound) {  // O(n) scan!
    upper_bounds.push(bound);
}
```

With 50 BCT candidates, each having multiple bounds, these linear scans compound to thousands of unnecessary comparisons.

### Fix

Replace `Vec<TypeId>` with `FxHashSet<TypeId>` for deduplication in:
- `InferenceInfo.candidates`
- `ConstraintSet.lower_bounds` / `upper_bounds`
- `collect_class_hierarchy()` visited set

Or maintain both a Vec (for ordered iteration) and a HashSet (for O(1) membership checks).

---

## Issue #4: No Subtype Memoization Within BCT (MEDIUM — estimated 1.3-1.5x impact on BCT benchmark)

### The Problem

In `src/solver/expression_ops.rs:178-203` and `src/solver/infer.rs:1523-1587`, the BCT algorithm:

1. Runs a tournament: check `is_subtype(best, candidate)` for each candidate — O(n)
2. Validates winner: check `is_subtype(ty, best)` for ALL types — O(n)
3. If no winner, falls through to `interner.union()` — triggers normalize → reduce

Between step 1 and step 2, the same subtype pairs may be checked again. The SubtypeChecker state is reset between uses (`total_checks = 0, depth = 0`).

While the global `QueryCache` provides cross-invocation caching, the BCT code in `expression_ops.rs` creates SubtypeCheckers **without** a `query_db`:

```rust
// expression_ops.rs:178 - creates checker WITHOUT query_db
let mut checker = SubtypeChecker::with_resolver(interner, res);
```

Without `query_db`, the global cache is never consulted, and every subtype check is computed from scratch.

### Fix

Pass a `QueryDatabase` to SubtypeChecker instances created within BCT and inference code, enabling cross-invocation caching.

---

## Issue #5: O(n²) Conflict Detection (MEDIUM — 1.26x observed)

### The Problem

In `src/solver/infer.rs:197-222`:

```rust
pub fn detect_conflicts(&self, interner: &dyn TypeDatabase) -> Option<ConstraintConflict> {
    // O(n²) upper bound cross-check
    for (i, &u1) in self.upper_bounds.iter().enumerate() {
        for &u2 in &self.upper_bounds[i + 1..] {
            if are_disjoint(interner, u1, u2) { ... }
        }
    }

    // O(n*m) lower vs upper bound cross-check
    for &lower in &self.lower_bounds {
        for &upper in &self.upper_bounds {
            if !is_subtype_of(interner, lower, upper) { ... }
        }
    }
}
```

Each `is_subtype_of` creates a new `SubtypeChecker` (no cache), so for 30 constraints this is ~900 full subtype checks.

### Fix

- Use the shared `QueryCache` for subtype checks in conflict detection
- Add early-exit heuristics (check most likely conflicts first)

---

## Issue #6: SmallVec Overflow (LOW — constant factor)

### The Problem

`TYPE_LIST_INLINE` is 8 (line 43, intern.rs), meaning `SmallVec<[TypeId; 8]>` overflows to heap for any union/intersection with >8 members. For `enumLiteralsSubtypeReduction.ts` with 512 members, EVERY union operation allocates on the heap.

### Fix

Consider increasing `TYPE_LIST_INLINE` to 16 or 32 for the union reduction hot path, or use a separate buffer for large unions.

---

## Root Cause Timeline

Looking at the git history, the performance regression happened in this sequence:

1. **`c57145e`** — "implement Canonicalizer struct" — Added the canonicalization infrastructure (no perf impact yet)
2. **`e763361`** — "implement object property canonicalization" — Extended canonicalization to objects
3. **`c94970d`** — "implement Application and Function canonicalization" — Extended to functions
4. **`345862d`** — "implement Callable and Intersection canonicalization" — Extended to intersections
5. **`23478b2`** — **"integrate Canonicalizer as fast-path in SubtypeChecker"** — THIS IS THE REGRESSION. Put canonicalization on EVERY subtype check.
6. **`82fcce7`** — "disjoint unit subtype fast path" — Added a fast-path, but it runs BEFORE the canonicalizer, not instead of it.

**The key commit is `23478b2`.** Before this, canonicalization was infrastructure that wasn't on the hot path. After this, it runs on every complex type comparison.

---

## Prioritized Fix Plan

### P0: Remove Canonicalizer from hot path (estimated 3-5x improvement on enum benchmark)
- **File:** `src/solver/subtype.rs:1578-1582`
- **Change:** Restrict `is_potentially_structural` to only `TypeKey::Lazy` types, or remove the canonicalization check entirely from `check_subtype_inner`
- **Risk:** Low — the canonicalizer was just added 2 days ago, existing tests don't depend on it being in the hot path
- **Impact:** Fixes `enumLiteralsSubtypeReduction` (9.74x → ~1-2x), improves all benchmarks

### P1: Fix Vec::remove() in union/intersection reduction (estimated 1.5x improvement)
- **File:** `src/solver/intern.rs:1997-1998` and `2022`
- **Change:** Use retain/swap_remove pattern instead of `remove(i)` in loop
- **Risk:** None — purely algorithmic improvement
- **Impact:** Fixes enum/union-heavy benchmarks

### P2: Replace Vec.contains() with HashSet in inference (estimated 1.3x improvement)
- **File:** `src/solver/infer.rs` — multiple locations (lines 81, 169, 176, 1388, 1702)
- **Change:** Use `FxHashSet` for deduplication, keep Vec for ordered iteration
- **Risk:** Low — simple data structure swap
- **Impact:** Fixes BCT and generic function benchmarks

### P3: Add QueryDB to BCT/inference SubtypeCheckers (estimated 1.2x improvement)
- **File:** `src/solver/expression_ops.rs:178` and `src/solver/infer.rs`
- **Change:** Pass query_db to SubtypeChecker instances for cross-invocation caching
- **Risk:** Low — enables existing cache infrastructure
- **Impact:** Fixes repeated subtype checks in BCT algorithm

### P4: Optimize conflict detection (estimated 1.1x improvement)
- **File:** `src/solver/infer.rs:197-222`
- **Change:** Cache subtype results, add early exit heuristics
- **Risk:** Low
- **Impact:** Fixes constraint conflicts benchmark

---

## Expected Results After Fixes

| Test | Current | After P0 | After P0-P2 |
|------|---------|----------|-------------|
| enumLiteralsSubtypeReduction | 9.74x slower | ~1.5-2x slower | ~1x (parity) |
| BCT candidates=50 | 2.38x slower | ~2x slower | ~1.2x slower |
| 50 generic functions | 2.25x slower | ~1.8x slower | ~1.1x slower |
| Recursive generic depth=25 | 1.61x slower | ~1.2x slower | ~1.1x slower |

**P0 alone should flip the benchmark score from 2:9 to approximately 5:6 or better.**

---

## Methodology

This audit was conducted by:
1. Analyzing git history for the last 3 days of commits touching solver/checker
2. Reading every file in the subtype checking, union reduction, BCT, and inference hot paths
3. Tracing the call chain from `check_subtype` → `check_subtype_inner` → canonicalize → `normalize_union` → `reduce_union_subtypes`
4. Identifying algorithmic complexity of each stage
5. Cross-referencing with benchmark results to validate theories
6. Reviewing the caching infrastructure to identify missed optimization opportunities
