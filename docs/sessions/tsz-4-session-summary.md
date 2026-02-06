# Session tsz-4: Checker Infrastructure & Control Flow Integration

**Started**: 2026-02-06
**Status**: Active - Investigating element access narrowing
**Focus**: Fix flow narrowing for computed element access

## Background

Session tsz-3 achieved **SOLVER COMPLETE** - 3544/3544 solver tests pass (100% pass rate). The Solver (the "WHAT") is now complete. The next priority is the Checker (the "WHERE") - the orchestration layer that connects the AST to the Type Engine.

## Current Status (2026-02-06)

**Test Results:**
- Solver: 3544/3544 tests pass (100%)
- Checker: 504 passed, **39 failed**, 106 ignored
- Test infrastructure is working (setup_lib_contexts is functional)

**Note**: The original session summary mentioned 184 failing tests, but that was outdated. Current state is much better with only 39 failures.

## Priority Tasks

### Task #16: Fix flow narrowing for computed element access 🔥 (IN PROGRESS)
**Problem**: 7 tests fail where narrowing should apply to `obj[key]` after typeof/discriminant checks.

**Example:**
```typescript
let obj: { prop: string | number } = { prop: "ok" };
let key: "prop" = "prop";
if (typeof obj[key] === "string") {
    obj[key].toUpperCase(); // Should narrow to string, but doesn't
}
```

**Root Cause Found**: `apply_flow_narrowing` in `src/checker/flow_analysis.rs:1334` only works for identifiers:
```rust
pub(crate) fn apply_flow_narrowing(&self, idx: NodeIndex, declared_type: TypeId) -> TypeId {
    let sym_id = match self.get_symbol_for_identifier(idx) {  // <-- Only handles identifiers!
        Some(sym) => sym,
        None => return declared_type,
    };
    // ...
}
```

For element access `obj[key]`, the narrowing should:
1. Narrow the `obj` type based on typeof/discriminant guards
2. Extract the property type using the narrowed object type

**Files**: `src/checker/flow_analysis.rs`, `src/checker/type_computation.rs`

**Failing Tests:**
- flow_narrowing_applies_for_computed_element_access_const_literal_key
- flow_narrowing_applies_for_computed_element_access_const_numeric_key
- flow_narrowing_applies_for_computed_element_access_numeric_literal_key
- flow_narrowing_applies_for_computed_element_access_literal_key
- flow_narrowing_applies_across_property_to_element_access
- flow_narrowing_applies_across_element_to_property_access

### Task #17: Fix enum type resolution and arithmetic
**6 failing tests** related to enum handling.

### Task #18: Fix index access type resolution
**6 failing tests** related to index signature resolution.

## Next Steps

1. **Task #16**: Implement element access narrowing - need to handle `obj[key]` case in apply_flow_narrowing
2. Ask Gemini for implementation approach (Mandatory Two-Question Rule)
3. Implement and test the fix

## Success Criteria

- Computed element access narrowing works (typeof, discriminant)
- 0 failing tests in flow narrowing category
