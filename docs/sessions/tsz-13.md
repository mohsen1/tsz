# Session TSZ-13: Index Signature Implementation

**Started**: 2026-02-06
**Status**: 🔄 IN PROGRESS - Implementation Phase
**Predecessor**: TSZ-12 (Cache Invalidation - Complete)

## Task

Implement Index Signature support for TypeScript subtyping and element access.

## Problem Statement

**Problem**: Element access and subtyping for objects with index signatures are failing.

**TypeScript behavior**:
1. Element access priority: named property → numeric index → string index
2. Numeric indexer must be subtype of string indexer
3. Properties must satisfy index signature constraints
4. Handle intersections, unions, lazy resolution

**Tests affected** (~3 tests):
- `test_checker_lowers_element_access_string_index_signature`
- `test_checker_lowers_element_access_number_index_signature`
- `test_checker_property_access_union_type`

## Implementation Plan (Validated by Gemini)

### Phase 1: Lowering (src/solver/lower.rs)
**Functions to modify**:
- `lower_type_literal`: Detect `IndexSignatureData`, use `interner.object_with_index`
- `lower_interface_declarations`: Aggregate index signatures from merged interfaces
- `index_signature_properties_compatible`: Check explicit properties satisfy index signature

### Phase 2: Subtyping (src/solver/subtype.rs)
**Functions to modify**:
- `SubtypeVisitor::visit_object`: Call `check_object_to_indexed` for `ObjectWithIndex` targets
- `SubtypeVisitor::visit_object_with_index`: Handle `Indexed <: Indexed` and `Indexed <: Object`
- `check_object_to_indexed`: Verify properties are subtypes of index signature value type

**Critical requirements**:
- Resolve `Lazy(DefId)` types before comparing
- Use `in_progress`/`seen_defs` sets to prevent infinite recursion
- Preserve `symbol` field for nominal identity

### Phase 3: Evaluation (src/solver/evaluate.rs)
**New function**: `evaluate_index_access(interner, object_type, index_type)`

**Logic**:
1. Resolve `object_type` (handle Union, Intersection, Ref/Lazy)
2. If key is string literal, look for named property first
3. If no match and key is number/numeric literal, check numeric index signature
4. If key is string/string literal, check string index signature
5. For intersections, search all members using `collect_properties`

**Edge cases**:
- Numeric indexer must be subtype of string indexer
- Property named `"123"` caught by numeric index signature
- `any` index signature → any property access returns `any`
- `readonly` index signatures prevent writes

### Phase 4: Checker Integration
**File**: `src/checker/state.rs` (not `expr.rs`)
- Call `solver.evaluate_index_access` from element access checking
- Delegate to Solver following North Star Rule 3

## Expected Impact

- **Direct**: Fix ~3 tests
- **Indirect**: Halo effect on tests using index signatures
- **Goal**: 8250+ passing, 50- failing

## Files to Modify

1. **src/solver/lower.rs** - Index signature detection
2. **src/solver/subtype.rs** - Subtyping logic
3. **src/solver/evaluate.rs** - Element access evaluation
4. **src/solver/objects.rs** - Property collection for intersections
5. **src/checker/state.rs** - Checker integration

## Test Status

**Start**: 8247 passing, 53 failing
**Current**: 8247 passing, 53 failing

## Notes

**Gemini Question 1 (Approach Validation)**: Complete
- Validated the overall approach
- Corrected: use `evaluate.rs` not `expr.rs` for element access
- Identified specific functions and edge cases
- Emphasized: Resolver Lazy types, prevent infinite recursion, preserve nominal identity

**AGENTS.md Mandatory Workflow**:
- Question 1 (Pre-implementation): ✅ Complete
- Question 2 (Post-implementation): Pending (after implementing solver logic)

**Next Step**: Implement Phase 1 (Lowering) and Phase 2 (Subtyping) in Solver, then ask Gemini Question 2 for code review before Checker integration.

**Deferred**:
- Readonly infrastructure tests (~6) - test setup issue
- Enum error duplication (~2) - diagnostic deduplication issue
