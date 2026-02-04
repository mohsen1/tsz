# Session tsz-1: Parser Error Detection

**Goal**: Eliminate parse errors to unblock type checking and improve conformance.

**Status**: In Progress (Pivoted 2026-02-04)

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
- **Unit Tests**: 363/365 passing (2 pre-existing abstract class failures)

## Current Focus: Task 2 - Crush Remaining TS1005

**Objective**: Fix remaining TS1005 ("Expected token") parse errors.

**Why This Focus**:
1. ✅ **Proven Success**: 6 parser fixes achieved +12% conformance
2. ✅ **Momentum**: Deep context in parser module, efficient workflow
3. ✅ **High ROI**: Parse errors block type checking, fixing them unblocks tests
4. ✅ **Low Risk**: Parser fixes are localized, less architectural risk

**Current Status**: ~12 TS1005 errors missing
- Missing commas in various contexts
- Missing delimiters in expressions
- Edge cases in statement parsing

## Blocked Task: Abstract Constructor Bug (DEFERRED)

**Issue**: Class identifiers in value position resolve to instance type instead of constructor type.

**Location**: `src/checker/state_type_resolution.rs:727-738`

**Complexity**: Requires threading "ResolutionContext" (Value vs Type) through type resolution system - fundamental architectural change.

**Action Plan**:
- Document as architectural TODO
- Requires dedicated architecture planning
- NOT suitable for current parser-focused session

## Completed Work (6 Parser Fixes)

1. **ClassDeclaration26** (commit 3c0332859)
   - Look-ahead logic for var/let as class member modifiers

2. **TS1109 throw** (commit 679cf3ad8)
   - Emit "Expression expected" for `throw;`

3. **TS1005 arrow functions** (commit 969968b8c)
   - Emit "'{' expected" for `() => var x`

4. **TS1005 argument lists** (commit 14b077780)
   - Emit "',' expected" for `foo(1 2 3)`

5. **TS1005 array/object literals** (commit 3e29d20e3)
   - Emit "',' expected" for `[1 2 3]` and `{a:1 b:2}`

6. **TS1005 variable declarations** (commit 3e453bc0f)
   - Emit "',' expected" for `var x = 1 y = 2`

## Infrastructure Success

- ✅ Resolved conformance-rust directory mystery
- ✅ TS1202 false positives eliminated
- ✅ Conformance tests working correctly
