# Emit Fix Attempt: Arrow Function Object Literal Parentheses

## Issue
Arrow functions that return object literals need parentheses when converted to ES5:

**Input**: `var v = a => <any>{}`

**Expected**: `var v = function (a) { return ({}); };`

**We emit**: `var v = function (a) { return {}; };`

## Root Cause Investigation

The issue is that `<any>{}` is a **type assertion** wrapping an object literal, not a direct object literal. When we check `ret.expression` in `emit_return_statement`, we get a TypeAssertion node, not an ObjectLiteralExpression node.

## Attempted Fix (Reverted)

Modified `emit_return_statement` in `crates/tsz-emitter/src/emitter/statements.rs` to add parentheses:

```rust
let needs_parens = self
    .arena
    .get(ret.expression)
    .map(|expr_node| expr_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
    .unwrap_or(false);
```

**Result**: Did not work because the expression is TypeAssertion, not ObjectLiteralExpression.

## Proper Fix Required

Need to:
1. Unwrap type assertions to find the underlying expression
2. Check if the underlying expression is an object literal
3. Add parentheses if so

**Complexity**: Medium - requires helper to unwrap type assertions/parentheses

**Code location**: `crates/tsz-emitter/src/emitter/statements.rs:422-435`

## Status

**Reverted**: The attempted fix was too simplistic and didn't handle type assertions.

**Recommendation**: Implement a helper function `unwrap_expression_for_parens_check` that recursively unwraps:
- TypeAssertion nodes
- ParenthesizedExpression nodes
- AsExpression nodes

Then use that helper in `emit_return_statement`.

**Estimated effort**: 1-2 hours to implement properly with tests.
