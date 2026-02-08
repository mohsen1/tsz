# Perf: 200 Classes & Constraint Conflicts (2026-02-08)

## Problem

Two benchmarks from `bench-vs-tsgo.sh` were slower than desired:

| Benchmark | Lines | Bytes | tsz (ms) | tsgo (ms) | Winner | Ratio |
|---|---|---|---|---|---|---|
| 200 classes | 9203 | ~162KB | 76.59 | 144.43 | tsz | 1.89x |
| Constraint conflicts N=200 | 819 | ~321KB | 156.52 | 157.81 | tsz | 1.01x |

While tsz wins both, the margins are thin compared to other benchmarks where tsz is 2-5x faster. The goal is to widen the gap.

## Profiling Results

Used `sample` (macOS) on the optimized (`dist` profile) binary running both test files in a loop.

### 200 Classes (`generate_synthetic_file 200`)

The file generates 200 interface+class pairs, each with ~7 methods.
Key hotspots:
- **Property lookup during subtype checks**: O(N) linear scans in `lookup_property`, `check_private_brand_compatibility`  
- **Intersection property merging**: O(N²) nested loops in `intern.rs` intersection flattening and `objects.rs` property collector
- **Declaration node dispatch**: `dispatch_type_computation` falling through to the default (expensive) case for CLASS_DECLARATION, INTERFACE_DECLARATION etc.

### Constraint Conflicts N=200 (`generate_constraint_conflict_file 200`)

The file generates 200 interfaces and a function with T constrained by all 200 via intersection.
Key hotspots:
- **Intersection property merging**: Same O(N²) issue when merging 200 interfaces' properties
- **Property overlap detection**: O(N×M) nested scan in `intern.rs` union reduction `has_any_property_overlap`
- **Binary search fallback**: `lookup_object_property` falling back to linear scan when property map not cached

## Changes Made

### 1. Declaration node early-out in dispatch.rs
**File**: `crates/tsz-checker/src/dispatch.rs`

Added `CLASS_DECLARATION`, `TYPE_ALIAS_DECLARATION`, `ENUM_DECLARATION` to the set of declaration nodes that return `TypeId::VOID` immediately, avoiding expensive type computation for nodes that are handled via `check_statement`.

### 2. Binary search for property lookups (multiple files)
**Files**: 
- `crates/tsz-solver/src/subtype_rules/objects.rs` — `lookup_property()`
- `crates/tsz-solver/src/operations_property.rs` — `lookup_object_property()`
- `crates/tsz-solver/src/compat.rs` — private brand property matching

Properties in `ObjectShape` are sorted by `Atom` (u32), so switched from `.iter().find()` O(N) to `.binary_search_by_key()` O(log N).

### 3. HashMap-indexed property merging in intersection flattening
**File**: `crates/tsz-solver/src/intern.rs`

When flattening intersection types, the old code used nested `.iter().find()` to check for duplicate properties — O(N²) overall. Replaced with a `FxHashMap<Atom, usize>` index for O(1) lookup, making the total O(N).

### 4. HashMap-indexed property merging in PropertyCollector
**File**: `crates/tsz-solver/src/objects.rs`

Same pattern: `PropertyCollector::merge_shape()` used linear scan to find existing properties. Added `prop_index: FxHashMap<Atom, usize>` for O(1) lookup.

### 5. Merge-scan for property overlap detection
**File**: `crates/tsz-solver/src/intern.rs`

In union reduction, `has_any_property_overlap` checked if two object shapes share any property name using nested iteration O(N×M). Since properties are sorted by Atom, replaced with a merge-scan O(N+M).

### 6. Fast path for private brand checking
**File**: `crates/tsz-solver/src/subtype_rules/objects.rs`

`check_private_brand_compatibility` resolved every property's Atom to a string and checked `starts_with("__private_brand_")`. Added:
- Early exit if no non-public properties exist (most objects)
- Only resolve Atom for non-public properties

## Validation

- `cargo nextest run --release -p tsz-solver` — 3513 passed, 25 skipped
- `cargo nextest run --release -p tsz-checker` — 293 passed, 20 skipped

## What Remains (TODO for next session)

### Further profiling needed
1. **Constraint conflicts**: The N=200 case is still close to tsgo (~1.01-1.44x depending on system load). The intersection of 200 constraints (`T extends C0 & C1 & ... & C199`) creates a massive intersection type. Need to profile:
   - How many times this intersection gets re-evaluated
   - Whether memoization of intersection flattening would help
   - Whether the solver's constraint inference path has redundant work

2. **200 classes startup overhead**: tsz's ~116ms vs tsgo's ~97ms suggests process startup or lib loading may still be a factor for this file size. Need to measure:
   - Time in parser vs binder vs checker
   - Whether lib loading is disproportionate for small files

### Optimization ideas not yet implemented
1. **Intersection type caching**: Cache the flattened intersection result so `T extends A & B & C` doesn't re-flatten on every use
2. **Lazy intersection property collection**: Don't collect all properties upfront; resolve on-demand during property access
3. **Batch property map creation**: Pre-build property maps for large object shapes at intern time instead of lazily on first access
4. **Constraint solver fast path**: For `T extends A & B & ...`, if all constraints are simple interfaces with no overlap, skip conflict detection

### How to reproduce benchmarks
```bash
# Generate test files and run benchmarks
./scripts/bench-vs-tsgo.sh --rebuild --filter 'classes|conflict'

# Quick comparison (no rebuild)
./scripts/bench-vs-tsgo.sh --filter '200 classes|Constraint conflicts N=200'
```
