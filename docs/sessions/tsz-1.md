# Session tsz-1: Simple Diagnostic Fixes (Continued)

**Started**: 2026-02-04 (Seventh iteration)
**Goal**: Fix simple diagnostic emission issues with clear test cases

## Previous Achievements (from history)
1. ✅ Parser fixes (6 TS1005 variants)
2. ✅ TS2318 core global type checking
3. ✅ Duplicate getter/setter detection
4. ✅ Switch statement flow analysis (TS2564)
5. ✅ Lib contexts fallback for global symbols
6. ⏸️ Interface property access (documented as complex)
7. ⏸️ Discriminant narrowing (documented as complex)

## Completed Work

### Test: test_duplicate_class_members

**Issue**: Test expected 2 TS2300 errors but only 1 was being emitted

**Investigation**:
- Traced duplicate detection logic in `src/checker/state_checking_members.rs`
- Found conflicting test expectations:
  - `test_duplicate_class_members` (older, Jan 31): Expected 2 TS2300
  - `test_duplicate_property_then_property` (newer, Feb 3): Expected 1 TS2300
- Verified tsc behavior: Emits exactly 1 TS2300 (on second property) + TS2717

**Resolution**:
- The newer test was correct
- Fixed the older test expectation to match tsc behavior
- Updated test comment to clarify tsc behavior

**Result**: ✅ Conformance improved from 51 to 50 failing tests

## Investigation: Additional Tests

### test_readonly_element_access_assignment_2540
**Issue**: TS2540 not emitted when assigning to readonly property via element access

**Investigation**:
- Test case: `config["name"] = "error"` where `name` is readonly in interface
- Code exists in `check_readonly_assignment()` at `src/checker/state_checking.rs:928`
- Function `is_property_readonly()` exists and checks property readonly flag
- Issue likely: Interface readonly properties not being flagged in type system

**Complexity**: Medium - Requires understanding how interface readonly properties are represented in type system

### test_import_alias_non_exported_member
**Issue**: TS2694 not emitted for import alias of non-exported member

**Investigation**:
- Found explicit TODO in code: `src/checker/import_checker.rs:431`
- Comment: "TODO: If left is resolved, check if right member exists (TS2694)"
- Feature not yet implemented

**Complexity**: Medium - Requires implementing export checking for qualified name imports

## Current Task: Implement TS2540 for Readonly Element Access

### Test: test_readonly_element_access_assignment_2540

**Problem**: TS2540 not emitted when assigning to readonly property via element access

**Test Case**:
```typescript
interface Config {
    readonly name: string;
}
let config: Config = { name: "ok" };
config["name"] = "error";  // Should emit TS2540
```

**Expected**: TS2540 "Cannot assign to 'name' because it is a read-only property"
**Actual**: No error emitted

**Investigation Status**:
- Code exists: `check_readonly_assignment()` at `src/checker/state_checking.rs:928`
- Function `is_property_readonly()` checks property readonly flag
- Hypothesis: Interface readonly properties not being flagged in type system
- OR: Element access not reaching the readonly check

**Implementation Plan**:
1. Use tracing to see what type is returned for `config["name"]`
2. Check if `is_property_readonly()` is being called
3. Determine if the issue is in type construction or checking logic
4. Fix the root cause

## Deep Dive: TS2540 Readonly Element Access Investigation

### Key Finding: Broader Issue Discovered

The problem is NOT specific to element access. Testing shows that TS2540 is not emitted for **either**:
- `config.name = "error"` (dot access)
- `config["name"] = "error"` (element access)

Both should emit TS2540 when `name` is readonly, but neither does in tsz.

### Root Cause Analysis

The issue is in how interface types are constructed. When an interface like:
```typescript
interface Config {
    readonly name: string;
}
```
is converted to a type in the system, the `readonly` modifier on the property is **not being preserved** in the PropertyInfo structure.

### Code Flow
1. Interface declaration → checked in `check_interface_declaration()`
2. Type creation → needs to create ObjectShape with PropertyInfo
3. PropertyInfo should have `readonly: true` for "name" property
4. **BUG**: The readonly flag is not being set during type construction

### Impact
This is a **high-complexity architectural issue**:
- Requires understanding how interface types are constructed from AST
- Involves the binder, type resolution, and solver integration
- Affects all readonly property checks on interfaces, not just element access

## Detailed Investigation: TS2540 Readonly Properties

### Root Cause Identified

`property_is_readonly` in `src/solver/operations_property.rs` does NOT handle `TypeKey::Lazy` types. Interface types are stored as `TypeKey::Lazy(def_id)`, so when checking if a property is readonly, the function returns `false` immediately.

### Investigation Findings

1. **Interface types ARE created with readonly flags**: The code in `interface_type.rs:173` correctly sets `readonly: self.has_readonly_modifier(&sig.modifiers)`

2. **Lazy type evaluation**: Interfaces use `TypeKey::Lazy(def_id)` which needs to be expanded/resolved to access the actual properties

3. **Missing case in `property_is_readonly`**: The function handles:
   - ReadonlyType, Object, ObjectWithIndex, Union, Intersection ✓
   - **Lazy ✗** - MISSING!

4. **Attempted fix**: Adding `TypeKey::Lazy` case that calls `evaluate_type(interner, type_id)`
   - **Problem**: `evaluate_type` needs resolver context to expand Lazy types
   - `property_is_readonly` only has `&dyn TypeDatabase` without resolver
   - Result: Test fails with no diagnostics (type evaluation broken)

### Architectural Issue

This requires understanding:
- How lazy types are evaluated with proper resolver context
- When types should be evaluated vs lazily resolved
- The relationship between TypeDatabase, TypeResolver, and QueryCache

### Status: NEEDS ARCHITECTURE EXPERTISE
This is a complex lazy type evaluation issue that requires deep knowledge of the solver architecture. Recommend deferring to expert session or filing detailed issue.
