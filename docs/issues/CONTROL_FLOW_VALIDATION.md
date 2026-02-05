# Control Flow Statement Validation

**Status**: PARTIALLY FIXED
**Discovered**: 2026-02-05
**Component**: Checker / Grammar Checker
**Conformance Impact**: ~7 parser tests fail due to missing validation

## What's Fixed

- **TS1104**: A 'continue' statement can only be used within an enclosing iteration statement (FIXED)
- **TS1105**: A 'break' statement can only be used within an enclosing iteration statement (FIXED)

Implementation:
- Added `iteration_depth` and `switch_depth` tracking to CheckerContext
- Break is valid if `iteration_depth > 0` OR `switch_depth > 0`
- Continue is valid only if `iteration_depth > 0`
- See commit 21d7ff3 for details

## What's Not Yet Fixed

- **TS1107**: Jump target cannot cross function boundary (labeled break/continue)
- **TS1116**: A 'continue' statement can only jump to a label of an enclosing iteration statement

These require label tracking and function boundary detection, which is more complex.

## Problem

tsz does not validate that `break` and `continue` statements appear in valid contexts. These errors are produced by TSC's grammar checker (part of the binder).

## Error Codes

| Code | Message | Status |
|------|---------|--------|
| TS1104 | A 'continue' statement can only be used within an enclosing iteration statement | FIXED |
| TS1105 | A 'break' statement can only be used within an enclosing iteration statement | FIXED |
| TS1107 | Jump target cannot cross function boundary | NOT FIXED |
| TS1116 | A 'continue' statement can only jump to a label of an enclosing iteration statement | NOT FIXED |

## Examples

### Example 1: Break at top level (FIXED)

```typescript
// parser_breakNotInIterationOrSwitchStatement1.ts
break;
```

**Expected**: TS1105 "A 'break' statement can only be used within an enclosing iteration statement"
**Actual**: TS1105 emitted ✓

### Example 2: Continue at top level (FIXED)

```typescript
// parser_continueNotInIterationStatement1.ts
continue;
```

**Expected**: TS1104 "A 'continue' statement can only be used within an enclosing iteration statement"
**Actual**: TS1104 emitted ✓

### Example 3: Break crossing function boundary (PARTIALLY FIXED)

```typescript
// parser_breakNotInIterationOrSwitchStatement2.ts
while (true) {
  function f() {
    break;  // Error: crosses function boundary
  }
}
```

**Expected**: TS1105 (break is not in an iteration statement from its perspective)
**Actual**: TS1105 emitted ✓ (works because we track depth, but for wrong reason - should be function boundary)

### Example 4: Continue in switch (FIXED)

```typescript
// parser_continueNotInIterationStatement3.ts
switch (0) {
  default:
    continue;  // Error: continue not valid in switch
}
```

**Expected**: TS1104 "A 'continue' statement can only be used within an enclosing iteration statement"
**Actual**: TS1104 emitted ✓

### Example 5: Break target crossing function boundary (NOT FIXED)

```typescript
// parser_breakTarget5.ts
target:
while (true) {
  function f() {
    while (true) {
      break target;  // Error: label is in parent function
    }
  }
}
```

**Expected**: TS1107 "Jump target cannot cross function boundary"
**Actual**: No error (labeled jumps not yet fully validated)

## Implementation Notes

In TSC, these checks are performed by the grammar checker (`checkGrammarStatementInAmbientContext`), which is part of the binder phase. The checker tracks:
1. Whether we're inside an iteration statement (for/while/do-while) - **DONE**
2. Whether we're inside a switch statement - **DONE**
3. Available labels and their types (iteration vs non-iteration) - **NOT DONE**
4. Function boundaries for label accessibility - **NOT DONE**

## Related Files

- TSC: `src/compiler/checker.ts` (checkBreakOrContinueStatement)
- tsz: `src/checker/state_checking_members.rs` (check_break_statement, check_continue_statement)
- tsz: `src/checker/context.rs` (iteration_depth, switch_depth)
- tsz: `src/checker/statements.rs` (enter/leave iteration/switch methods)

## Testing

Unit tests in `src/tests/control_flow_validation_tests.rs`:
```bash
cargo test control_flow_validation --release
```

Conformance tests:
```bash
./scripts/conformance.sh run --filter "BreakStatements" --verbose
./scripts/conformance.sh run --filter "ContinueStatements" --verbose
```
