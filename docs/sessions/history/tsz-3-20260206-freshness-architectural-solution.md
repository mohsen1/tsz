# Session tsz-3: Object Literal Freshness - Implementation Plan

**Started**: 2026-02-06
**Status**: ✅ READY TO IMPLEMENT
**Predecessor**: Index Access Type Evaluation (Already Implemented)

## Problem Summary

Object literal freshness widening is not working correctly. Previous cache-mutation approach failed.

## Solution: Lawyer/Judge Pattern

Gemini validated the approach - move Excess Property Checking (EPC) to the Lawyer layer.

### Architecture

- **Judge** (`src/solver/subtype.rs`): Pure structural subtyping, ignores freshness
- **Lawyer** (`src/solver/compat.rs`): Handles TypeScript-specific EPC
- **Checker** (`src/checker/`): Calls Lawyer's `is_assignable_to`, stores widened types

### Implementation Tasks

#### 1. `src/solver/compat.rs` - Implement EPC in Lawyer

**Function: `check_excess_properties`** (new or modify existing)
```rust
fn check_excess_properties(&mut self, source: TypeId, target: TypeId) -> bool {
    // 1. Check if source has FRESH_LITERAL flag
    if !is_fresh_object_type(self.interner, source) {
        return true; // Not fresh, no EPC needed
    }

    // 2. Resolve target (handle Lazy, Ref, Intersection)
    let resolved_target = self.resolve_type(target);

    // 3. If target has string index signature, disable EPC
    if has_string_index_signature(resolved_target) {
        return true;
    }

    // 4. Collect all target properties (handle intersections/unions)
    let target_props = collect_target_properties(resolved_target);

    // 5. Check each source property exists in target
    for prop in get_source_properties(source) {
        if !target_props.contains(&prop.name) {
            return false; // Excess property found
        }
    }
    true
}
```

**Function: `is_assignable_impl`** (modify)
- Call `check_excess_properties` before `self.subtype.is_subtype_of`
- If EPC fails, return `false` immediately

**Function: `explain_failure`** (verify)
- Ensure it returns `SubtypeFailureReason::ExcessProperty` for EPC failures

#### 2. `src/checker/state_checking.rs` - Use Lawyer

**Function: `check_variable_declaration`**
- Remove manual `check_object_literal_excess_properties` calls
- Use `self.is_assignable_to(init_type, declared_type)` instead
- Call `explain_failure` to get diagnostic when assignability fails

#### 3. Edge Cases

1. **Empty Object Target `{}`**: Bypass EPC
2. **Intersections in Target**: Collect properties from all branches
3. **Unions in Target**: Property is excess only if excess for BOTH
4. **Nested Objects**: Recursive EPC

#### 4. Potential Pitfalls

1. **Reversed Subtype Check**: Check literal prop → target prop, not reverse
2. **Missing Type Resolution**: Must resolve Lazy/Ref types
3. **Optional Properties**: Handle `?` correctly
4. **O(N^2) Performance**: Use FxHashSet for large property counts

## Files to Modify

1. `src/solver/compat.rs` - Main implementation
2. `src/checker/state_checking.rs` - Use Lawyer instead of manual EPC
3. `src/solver/freshness.rs` - Verify `widen_freshness` works correctly

## Tests

- `src/checker/tests/freshness_stripping_tests.rs` (6 failing tests should pass)
- `tests/conformance/expressions/objectLiterals/excessPropertyChecking.ts`

## Next Step

Start implementing `check_excess_properties` in `src/solver/compat.rs`.
