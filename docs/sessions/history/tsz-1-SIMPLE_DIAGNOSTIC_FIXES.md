# Session tsz-1: Simple Diagnostic Fixes

**Started**: 2026-02-04 (Sixth iteration)
**Goal**: Fix simple diagnostic emission issues with clear test cases

## Previous Achievements
1. ✅ Parser fixes (6 TS1005 variants)
2. ✅ TS2318 core global type checking
3. ✅ Duplicate getter/setter detection
4. ✅ Interface property access investigation (documented as complex)

## Current Task: Review and Fix Simple Failing Tests

### Approach
1. Review the 52 failing tests
2. Identify simple diagnostic issues (missing/wrong error codes)
3. Focus on emission logic, not complex type resolution
4. Timebox each fix to 30 minutes

### Candidate Tests (from cargo test --lib)
- `test_abstract_mixin_intersection_ts2339` - Abstract mixin pattern
- `test_assignment_expression_condition_narrows_discriminant` - Discriminant narrowing
- `test_checker_cross_namespace_type_reference` - Namespace reference
- `test_checker_module_augmentation_merges_exports` - Module augmentation
- `test_class_implements_interface_property_access` - Interface implementation

### Success Criteria
- Fix 1-2 simple diagnostic issues
- No regressions
- All work tested and committed

## Status: READY TO BEGIN
Will pick one simple test and investigate.
