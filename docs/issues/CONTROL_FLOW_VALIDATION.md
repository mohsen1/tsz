# Control Flow Statement Validation

**Status**: FIXED
**Discovered**: 2026-02-05
**Component**: Checker / Grammar Checker
**Last Updated**: 2026-02-05

## Summary

All control flow validation errors are now implemented:
- Break/Continue statement tests: 27/28 passing (96.4%)
- The 1 skipped test has no TSC cache entry

## What's Fixed

| Code | Message | Status |
|------|---------|--------|
| TS1104 | A 'continue' statement can only be used within an enclosing iteration statement | FIXED |
| TS1105 | A 'break' statement can only be used within an enclosing iteration statement | FIXED |
| TS1107 | Jump target cannot cross function boundary | FIXED |
| TS1115 | A 'continue' statement can only target a label of an enclosing iteration statement | FIXED |
| TS1116 | A 'break' statement can only jump to a label of an enclosing statement | FIXED |

## Implementation

### Context Tracking

Added to `CheckerContext` (`src/checker/context.rs`):
- `iteration_depth: u32` - Depth of nested iteration statements
- `switch_depth: u32` - Depth of nested switch statements
- `function_depth: u32` - Depth of nested functions
- `label_stack: Vec<LabelInfo>` - Stack of labels with their types and function depths
- `had_outer_loop: bool` - Whether there was a loop in an outer function scope

### Label Info

```rust
pub struct LabelInfo {
    pub name: String,           // The label name
    pub is_iteration: bool,     // Whether label wraps an iteration statement
    pub function_depth: u32,    // Function depth when label was defined
}
```

### Validation Logic

**Break Statement** (`check_break_statement`):
- Labeled break:
  - If label not found: TS1116
  - If label crosses function boundary: TS1107
  - Otherwise: valid
- Unlabeled break:
  - If not in loop/switch AND in function with outer loop: TS1107
  - If not in loop/switch: TS1105
  - Otherwise: valid

**Continue Statement** (`check_continue_statement`):
- Labeled continue:
  - If label not found: TS1115
  - If label crosses function boundary: TS1107
  - If label not on iteration statement: TS1115
  - Otherwise: valid
- Unlabeled continue:
  - If not in loop AND in function with outer loop: TS1107
  - If not in loop: TS1104
  - Otherwise: valid

### Nested Label Handling

The implementation correctly handles nested labels:
```typescript
target1:
target2:
while (true) {
  continue target1;  // Valid - target1 transitively wraps iteration
}
```

The `is_iteration_or_nested_iteration` function recursively checks through nested labels to determine if a label ultimately wraps an iteration statement.

## Examples

### Example 1: Break at top level
```typescript
break;  // TS1105 ✓
```

### Example 2: Unlabeled break in function inside loop
```typescript
while (true) {
  function f() {
    break;  // TS1107 ✓ (crossing function boundary)
  }
}
```

### Example 3: Labeled break crossing function boundary
```typescript
target:
while (true) {
  function f() {
    while (true) {
      break target;  // TS1107 ✓
    }
  }
}
```

### Example 4: Break with non-existent label
```typescript
while (true) {
  break target;  // TS1116 ✓ (target doesn't exist)
}
```

### Example 5: Continue with non-iteration label
```typescript
target:
  continue target;  // TS1115 ✓ (target is not on a loop)
```

### Example 6: Continue with non-existent label
```typescript
while (true) {
  continue target;  // TS1115 ✓
}
```

## Related Files

- `src/checker/context.rs` - Context fields (iteration_depth, switch_depth, function_depth, label_stack, had_outer_loop)
- `src/checker/statements.rs` - StatementCheckCallbacks trait and LABELED_STATEMENT handling
- `src/checker/state_checking_members.rs` - check_break_statement, check_continue_statement implementations
- `src/checker/function_type.rs` - Function body context reset
- `src/checker/types/diagnostics.rs` - Error code constants

## Testing

Conformance tests:
```bash
./.target/release/tsz-conformance --test-dir ./TypeScript/tests/cases/conformance/parser/ecmascript5/Statements/BreakStatements --cache-file ./tsc-cache-full.json --tsz-binary ./.target/release/tsz

./.target/release/tsz-conformance --test-dir ./TypeScript/tests/cases/conformance/parser/ecmascript5/Statements/ContinueStatements --cache-file ./tsc-cache-full.json --tsz-binary ./.target/release/tsz
```

Results:
- BreakStatements: 12/13 passed (1 skipped - no cache)
- ContinueStatements: 15/15 passed (100%)
