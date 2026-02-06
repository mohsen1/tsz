# Session TSZ-13: Index Signature Implementation

**Started**: 2026-02-06
**Status**: 🔄 IN PROGRESS - Ready for Implementation
**Predecessor**: TSZ-12 (Cache Invalidation - Complete)

## Task

Implement Index Signature support for TypeScript subtyping and element access.

## Project Status (Gemini Assessment)

**Current State**: High-momentum stabilization phase
- **8247 passing, 53 failing** (started at 8232, 68 failing)
- **+15 tests fixed** in tsz-12
- Architectural integrity: Strong adherence to North Star
- Solver robustness: Becoming "single source of truth"
- Workflow compliance: Two-Question Rule preventing bugs

**Priority Ranking** (per Gemini):
1. **Index Signatures** (this session) - Highest priority, fundamental to TypeScript object model
2. **Flow Narrowing** (~5 tests) - Highest complexity, architecturally sensitive
3. **Readonly Infrastructure** (~6 tests) - High priority, test setup issues
4. **Diagnostic Deduplication** (~2 tests) - Medium priority, polish task

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
- Follow Visitor Pattern (North Star Rule 2)

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

**Important**: Keep logic separated between Judge (structural) and Lawyer (TS-specific compatibility) per NORTH_STAR.md Section 3.3.

## AGENTS.md Workflow

**Question 1 (Approach Validation)**: ✅ Complete
- Validated implementation plan
- Corrected: use `evaluate.rs` not `expr.rs`
- Identified specific functions and edge cases

**Question 2 (Code Review)**: ⏳ Pending
- After implementing solver logic
- Ask Gemini Pro to review: `./scripts/ask-gemini.mjs --pro --include=src/solver`

## Expected Impact

- **Direct**: Fix ~3 index signature tests
- **Indirect**: Halo effect on tests using index signatures
- **Goal**: 8250+ passing, 50- failing

## Files to Modify

1. **src/solver/lower.rs** - Index signature detection
2. **src/solver/subtype.rs** - Subtyping logic
3. **src/solver/evaluate.rs** - Element access evaluation
4. **src/solver/objects.rs** - Property collection for intersections
5. **src/checker/state.rs** - Checker integration

## Roadmap (Gemini Recommendation)

| Phase | Session | Focus |
|-------|---------|-------|
| **Immediate** | **TSZ-13** | Index Signatures (this session) |
| **Short-term** | **TSZ-14** | Readonly Infrastructure (~6 tests) |
| **Mid-term** | **TSZ-15** | Flow Narrowing (~5 tests) - Use `--pro` |
| **Long-term** | **TSZ-16** | Module/Overload Resolution (~7 tests) |

## Test Status

**Start**: 8247 passing, 53 failing
**Current**: 8247 passing, 53 failing

## Notes

**Gemini Recommendation**: Do not pivot. The plan is validated and ready-to-implement. Pivoting now would abandon guaranteed-correct implementation.

**Next Step**: Begin Phase 1 (Lowering) implementation in `src/solver/lower.rs`.
