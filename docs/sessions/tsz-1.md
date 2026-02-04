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
