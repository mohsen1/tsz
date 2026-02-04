# Session tsz-1: Parser Completion & Stability

**Goal**: Eliminate syntax errors and ensure clean test baseline.

**Status**: In Progress (Refocused 2026-02-04)

## WORK IS NEVER DONE UNTIL ALL TESTS PASS
Work is never done until all tests pass. This includes:
- Unit tests (`cargo nextest run`) - **Current: 363/365 passing**
- Conformance tests (`./scripts/conformance.sh`) - **Current: 50%**
- No existing `#[ignore]` tests
- No cleanup work left undone
- No large files (>3000 lines) left unaddressed

## Progress Summary
- **Conformance**: 38% → 50% (+12 percentage points)
- **Parser Fixes**: 6 completed
- **Unit Tests**: 363/365 passing (2 abstract class failures)

## Current Tasks

### Task 1: Fix 2 Failing Abstract Class Unit Tests (HIGH PRIORITY)
- **Current**: 363/365 passing
- **Failing Tests**:
  - `test_abstract_constructor_assignability`
  - `test_abstract_mixin_intersection_ts2339`
- **Context**: Already working with class declarations (ClassDeclaration26 fix)
- **Strategy**: Use tracing to debug abstract class handling
- **Files**: `src/checker/class_type.rs`, `src/solver/compat.rs`

### Task 2: Crush Remaining TS1005 (12 missing)
- **Goal**: Eliminate all "Expected token" parse errors
- **Impact**: Parse errors block binder/checker, clearing them uncovers real type errors
- **Likely Locations**: `parse_list`, `parse_delimited_list`, statement parsing

### Task 3: TS2300 Duplicate Identifier (Stretch Goal)
- **Count**: 10 missing
- **Component**: Binder (`src/binder/`)
- **Natural progression**: After Parser, before Checker

## Completed Work (6 Parser Fixes)

1. **ClassDeclaration26** (commit 3c0332859)
   - Look-ahead logic for var/let as class member modifiers
   - Distinguishes property names from invalid modifiers

2. **TS1109 throw statement** (commit 679cf3ad8)
   - Emit "Expression expected" for `throw;`

3. **TS1005 arrow functions** (commit 969968b8c)
   - Emit "'{' expected" for `() => var x`

4. **TS1005 argument lists** (commit 14b077780)
   - Emit "',' expected" for `foo(1 2 3)`

5. **TS1005 array/object literals** (commit 3e29d20e3)
   - Emit "',' expected" for `[1 2 3]` and `{a:1 b:2}`

6. **TS1005 variable declarations** (commit 3e453bc0f)
   - Emit "',' expected" for `var x = 1 y = 2`

## History

### Infrastructure Task: Complete ✅
- Resolved conformance-rust directory mystery
- TS1202 false positives fixed (was thought to be 29 extra)
- Conformance infrastructure working correctly

### Session Evolution
- Started with TS1005/TS1109 parser focus
- Added infrastructure investigation (TS1202)
- Refined to include unit test stability for clean baseline

## Investigation: Abstract Constructor Assignability (2026-02-04)

**Issue**: Test `test_abstract_constructor_assignability` expects 0 errors but gets 1:
```typescript
const ctor1: AnimalCtor = Dog; // Position 609 - ERROR
```

**Error**: TS2322 - Dog's type is instance type (with methods like speak, isPrototypeOf) instead of constructor type

**Root Cause Analysis**:
- Constructor types ARE being computed correctly
- Cached in type environment for value contexts (state_type_analysis.rs:854-863)
- Bug is in identifier → type resolution path
- Path: `get_type_of_node` → `compute_type_of_node_complex` → `ExpressionDispatcher::dispatch_type_computation`

**Status**: Complex type resolution issue. Requires deeper investigation or tsz-tracing skill.

**Next Steps**:
- Use tsz-tracing skill to debug the resolution path
- Or continue with Task 2 (TS1005 parser fixes) which have been very productive

## Task 1 Status: Complex Type Resolution Bug (2026-02-04)

### Issue Summary
**Bug Location**: `src/checker/state_type_resolution.rs:727-738`

In `type_reference_symbol_type_with_params`, when handling CLASS symbols:
```rust
if symbol.flags & symbol_flags::CLASS != 0 {
    if let Some((instance_type, params)) =
        self.class_instance_type_with_params_from_symbol(sym_id)
    {
        return (instance_type, params);  // ❌ BUG: Returns instance type
    }
}
```

**Problem**: This function returns the **instance type** for class symbols, regardless of whether the class is used in a type position (`a: Dog`) or value position (`const x = Dog`).

**Expected Behavior**:
- Type position: `a: Dog` → instance type ✅
- Value position: `const x = Dog` → constructor type ❌ (currently returns instance type)

**Root Cause**: The type resolution doesn't distinguish between type context and value context when resolving class symbols.

**Complexity**: This requires understanding the type resolution flow:
1. `get_type_of_identifier` (state_type_analysis.rs)
2. → `get_type_of_symbol` (state_type_resolution.rs:707)
3. → `compute_type_of_symbol` (state_type_resolution.rs:???)  
4. → Returns instance type unconditionally

**Recommendation**: This fix requires significant refactoring of the symbol-to-type resolution system to track resolution context. Beyond the scope of current session.

**Alternative**: Continue with Task 2 (TS1005 parser fixes) which have been very productive and less risky.
