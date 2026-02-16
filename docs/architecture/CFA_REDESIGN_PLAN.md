# Control Flow Analysis (CFA) Redesign Plan

## Review Status: NEEDS REVISION

**Date**: 2026-02-16
**Reviewer**: Claude

---

## Critical Review

### What We Actually Fixed

The O(N²) `is_matching_reference` problem was **already fixed** in the current session:

1. **Symbol-based early exit** (`control_flow.rs:600-616`): Before calling expensive `assignment_targets_reference_node`, we compare symbols directly. This reduced `is_matching_reference` calls from **32M to 2**.

2. **Reference match caching** (`control_flow_narrowing.rs`): Caches results for repeated node pair comparisons.

### Why the Benchmark Is Still Slow

After our fixes:
- `is_matching_reference` calls: 32M → 2 ✓ (fixed)
- Benchmark time: 507ms → 476ms (only 6% improvement)

**The bottleneck has shifted**. The remaining ~470ms is NOT in CFA. It's likely in:
- Intersection type normalization (200-way `T extends A & B & C & ...`)
- Subtype checking against 200-member intersection
- Type instantiation/evaluation

### Comparison with TypeScript

Looking at TypeScript's `checker.ts`:

```typescript
// TypeScript's isMatchingReference - same pattern we now use
case SyntaxKind.Identifier:
    return target.kind === SyntaxKind.Identifier &&
           getResolvedSymbol(source) === getResolvedSymbol(target);  // O(1) symbol comparison!
```

TypeScript does NOT:
- Store symbols in FlowAssignment nodes
- Build symbol→flow indexes
- Pre-compute anything special

TypeScript's CFA is fast because symbol resolution is O(1). Our fix achieves the same.

---

## Revised Assessment

### Phase 1: Symbol in FlowNode - **MAYBE NOT NEEDED**

**Original proposal**: Add `affected_symbol: Option<SymbolId>` to FlowNode.

**Assessment**: Our symbol-based early exit already solves this at the checker level without binder changes. The binder doesn't need to know about symbols for flow nodes - the checker can resolve them efficiently.

**Verdict**: Low priority. Only pursue if we find concrete cases where checker-side resolution is insufficient.

### Phase 2: Symbol Flow Index - **PROBABLY OVER-ENGINEERED**

**Original proposal**: Build `FxHashMap<SymbolId, Vec<FlowNodeId>>` reverse index.

**Assessment**: TypeScript doesn't do this and is fast. The flow graph is walked backwards through antecedents, which naturally prunes irrelevant nodes. With our symbol-based filtering, we skip irrelevant ASSIGNMENT nodes in O(1).

**Verdict**: Not needed. This adds complexity without addressing the actual bottleneck.

### Phase 3: Simplified Flow Walker - **GOOD BUT ORTHOGONAL**

**Original proposal**: Clean `FlowEffect` enum and simplified API.

**Assessment**: Good code quality improvement but doesn't address performance. Current code is complex but functional.

**Verdict**: Defer. Do this when we have concrete bugs or maintenance issues, not for performance.

### Phase 4: Narrowing in Solver - **GOOD ARCHITECTURE**

**Original proposal**: Move all narrowing to solver.

**Assessment**: Aligns with NORTH_STAR. Good long-term direction.

**Verdict**: Keep as long-term goal but not urgent for this benchmark.

---

## What Actually Needs Investigation

The remaining ~470ms is NOT in CFA. Profile to find actual bottleneck:

### Hypothesis 1: Intersection Normalization

200-way intersection `T extends Constraint0 & Constraint1 & ... & Constraint199`:
- `normalize_intersection` has multiple O(N) passes
- Object merging with 400+ properties
- Already added some guards but may need more

### Hypothesis 2: Constraint Checking

Checking `allConstraints` against 200-member intersection:
- 200 subtype checks (one per member)
- Each check involves property lookup
- May have hidden quadratic behavior

### Hypothesis 3: Type Instantiation

Generic instantiation of `multiConstrained<T>`:
- Instantiating constraint types
- Substitution maps with many entries

### Next Steps

1. **Profile** the actual benchmark to find where time is spent
2. **Trace** solver operations to count expensive calls
3. **Compare** with tsgo's approach to large intersections

---

## Retained Recommendations

### Short-term (Performance)

1. Profile constraint benchmark to find actual bottleneck
2. Add tracing to intersection normalization, subtype checking
3. Consider size-based bailouts for very large types

### Medium-term (Code Quality)

1. Consolidate flow analysis caches (multiple overlapping caches exist)
2. Document the caching strategy clearly
3. Add metrics/counters for debugging

### Long-term (Architecture)

1. Move narrowing logic to solver (NORTH_STAR alignment)
2. Consider type normalization strategies for large types
3. Investigate tsgo's approach to constraint checking

---

## Original Problem Statement

The current CFA implementation had performance issues that emerge with many variables:
- ~~**O(N²) flow walking**: For N variables, each flow query walks through all prior assignments~~ **FIXED**
- ~~**Repeated reference comparisons**: `is_matching_reference` was called 32M times for 200 variables~~ **FIXED (now 2 calls)**
- **No symbol indexing**: Flow nodes don't know which symbols they affect ← Not actually needed
- **Complex code paths**: Multiple overlapping caching strategies ← True but not performance-critical

---

## Experiment: Narrowing in Solver (2026-02-16)

### Goal

Test the NORTH_STAR alignment of moving narrowing logic to solver.

### Findings

The architecture is **already partially implemented**:

```
Solver (narrowing.rs - 3371 lines):
├── TypeGuard enum (AST-agnostic guards)
├── NarrowingContext API
├── narrow_type(source, guard, sense) - unified entry point
└── narrow_by_* specialized functions

Checker (control_flow_narrowing.rs - 2504 lines):
├── AST extraction logic
├── Reference matching (is_matching_reference)
└── Some duplicate type logic that should be in solver
```

### Changes Made

1. **Moved `instanceof` unknown handling to solver** (`narrowing.rs:2252-2254`)
   - Before: Checker returned `instance_type` or `OBJECT` for unknown
   - After: Solver handles in `TypeGuard::Instanceof` dispatch

2. **Simplified checker's `narrow_by_instanceof`** (`control_flow_narrowing.rs:497-530`)
   - Before: Special cases + direct calls to `narrow_by_instance_type`
   - After: Extract TypeGuard, call `narrow_type(type_id, guard, sense)`

3. **Moved `in` operator unknown handling to use solver** (`control_flow_narrowing.rs:626-637`)
   - Before: Returned `TypeId::OBJECT`
   - After: Calls `narrow_type(type_id, TypeGuard::InProperty(prop_name), sense)`
   - Benefit: Solver returns `{ [prop]: unknown }` which is more precise

### Results

- **Tests**: 457 passed, 1 pre-existing failure (no regression)
- **Conformance**: 67.2% (no regression)
- **Code**: Cleaner separation of concerns

### Remaining Refactoring Opportunities

1. **`narrow_by_in_operator`**: Still has type parameter handling that could move to solver
2. **`narrow_by_call_predicate`**: Mixes AST extraction with type logic
3. **`narrow_by_typeof_negation`**: Duplicates some logic
4. **Union member filtering**: Checker does this for `in` operator, could delegate

### Recommended Next Steps

1. Add `TypeParameter` handling to solver's `narrow_by_property_presence`
2. Gradually migrate remaining checker narrowing to use `narrow_type` exclusively
3. Consider adding `TypeGuard::TypeParameter { constraint, inner_guard }` variant

---

## References

- `docs/architecture/NORTH_STAR.md` - Architecture principles
- `crates/tsz-checker/src/control_flow.rs` - Current implementation (with fixes)
- `crates/tsz-binder/src/lib.rs` - FlowNode definition
- TypeScript source: `src/compiler/checker.ts` (getFlowTypeOfReference, isMatchingReference)
