# Control Flow Statement Validation

**Status**: NOT IMPLEMENTED
**Discovered**: 2026-02-05
**Component**: Checker / Grammar Checker
**Conformance Impact**: ~7 parser tests fail due to missing validation

## Problem

tsz does not validate that `break` and `continue` statements appear in valid contexts. These errors are produced by TSC's grammar checker (part of the binder).

## Missing Error Codes

| Code | Message | Description |
|------|---------|-------------|
| TS1104 | A 'continue' statement can only be used within an enclosing iteration statement | `continue;` at top level or in switch |
| TS1105 | A 'break' statement can only be used within an enclosing iteration statement | `break;` at top level (without loop/switch) |
| TS1107 | Jump target cannot cross function boundary | `break label;` where label is in parent function |
| TS1116 | A 'continue' statement can only jump to a label of an enclosing iteration statement | `continue label;` where label is not on a loop |

## Examples

### Example 1: Break at top level

```typescript
// parser_breakNotInIterationOrSwitchStatement1.ts
break;
```

**Expected**: TS1105 "A 'break' statement can only be used within an enclosing iteration statement"
**Actual**: No error

### Example 2: Continue at top level

```typescript
// parser_continueNotInIterationStatement1.ts
continue;
```

**Expected**: TS1104 "A 'continue' statement can only be used within an enclosing iteration statement"
**Actual**: No error

### Example 3: Break crossing function boundary

```typescript
// parser_breakNotInIterationOrSwitchStatement2.ts
while (true) {
  function f() {
    break;  // Error: crosses function boundary
  }
}
```

**Expected**: TS1105 (break is not in an iteration statement from its perspective)
**Actual**: No error

### Example 4: Continue in switch

```typescript
// parser_continueNotInIterationStatement3.ts
switch (0) {
  default:
    continue;  // Error: continue not valid in switch
}
```

**Expected**: TS1104 "A 'continue' statement can only be used within an enclosing iteration statement"
**Actual**: No error

### Example 5: Break target crossing function boundary

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
**Actual**: No error

## Implementation Notes

In TSC, these checks are performed by the grammar checker (`checkGrammarStatementInAmbientContext`), which is part of the binder phase. The checker tracks:
1. Whether we're inside an iteration statement (for/while/do-while)
2. Whether we're inside a switch statement
3. Available labels and their types (iteration vs non-iteration)
4. Function boundaries for label accessibility

## Recommended Approach

1. Add context tracking during AST visiting in the checker
2. Track iteration context, switch context, and label context
3. Emit appropriate errors when break/continue appear in invalid contexts

## Related Files

- TSC: `src/compiler/checker.ts` (checkBreakOrContinueStatement)
- tsz: `src/checker/state_checking_*.rs` (needs implementation)

## Testing

```bash
./scripts/conformance.sh run --filter "BreakStatements" --verbose
./scripts/conformance.sh run --filter "ContinueStatements" --verbose
```
