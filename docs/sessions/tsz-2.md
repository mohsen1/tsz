# Session tsz-2: Coinductive Subtyping (Recursive Types)

**Started**: 2026-02-05
**Status**: Active
**Goal**: Implement coinductive subtyping logic to handle recursive types without infinite loops

## Problem Statement

From NORTH_STAR.md Section 4.4:

> "TypeScript uses 'coinductive' subtyping for recursive types. This means we compute the Greatest Fixed Point (GFP) rather than Least Fixed Point (LFP). When comparing `type A = { self: A }` and `type B = { self: B }`, we assume they are subtypes and verify consistency."

Without coinductive subtyping, the compiler will crash or enter infinite loops when comparing recursive types.

**Impact**:
- Blocks complex recursive type definitions (linked lists, trees, Redux state)
- Causes stack overflow crashes
- Prevents proper type checking of self-referential generics

## Technical Details

**Files**:
- `src/solver/subtype.rs` - Core subtype checking logic
- `src/solver/mod.rs` - Solver state management
- `src/solver/visitor.rs` - Traversal of recursive structures

**Root Cause**:
When comparing `A` and `B` where both contain references to themselves, the naive approach leads to infinite recursion: `is_subtype_of(A, B)` → check properties → `is_subtype_of(A, B)` → ...

## Implementation Strategy

### Phase A: Fix the Build (Janitor Phase) ✅ COMPLETE
**Problem**: 9 compilation errors blocking tests

**Completed** (commit eae3bd048):
1. ✅ Fixed `compute_best_common_type` calls in expression_ops.rs to use `NoopResolver`
2. ✅ Fixed `narrow_by_discriminant` calls to pass `&[Atom]` instead of `Atom`
3. ✅ Removed deprecated `Ref` handling (Phase 4.3 uses `Lazy`)

**Result**: Build compiles successfully. Runtime shows stack overflow - the exact problem Coinductive Subtyping will solve.

### Phase B: Coinductive Subtyping (Judge Phase)
**Goal**: Implement Greatest Fixed Point (GFP) logic for recursive types

1. **Create failing test** (src/solver/tests/subtype_tests.rs):
   ```typescript
   type A = { next: A };
   // A should be assignable to A
   ```

2. **Audit check_subtype** (src/solver/subtype.rs):
   - Verify `in_progress.insert(pair)` happens BEFORE `evaluate_type(source)`
   - Critical: If evaluate happens first, expansive types bypass cycle detection

3. **Implement seen_defs/seen_refs logic**:
   - Use `seen_defs: FxHashSet<(DefId, DefId)>` for definition-level cycles
   - Check cycles in `check_lazy_lazy_subtype` before expansion
   - **MANDATORY**: Ask Gemini to validate implementation before writing

4. **Verify with tracing**:
   - Run: `TSZ_LOG=debug cargo test test_recursive_assignability`
   - Check for "Cycle detected" log

### Phase C: Validation
1. Run conformance tests to verify no regressions
2. Test with complex recursive types (linked lists, trees)
3. Ask Gemini Pro to review implementation

## Success Criteria

- [ ] No stack overflows when comparing recursive types
- [ ] `type A = { self: A }` and `type B = { self: B }` are correctly identified as subtypes
- [ ] Depth limiting prevents infinite loops
- [ ] Unit tests cover simple and mutually recursive types
- [ ] Generic recursive types work (e.g., `List<number>` vs `List<string>`)

## Session History

*Created 2026-02-05 after completing Application type expansion.*
