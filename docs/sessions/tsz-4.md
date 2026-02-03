# Session tsz-4

## Current Work

**Task**: Investigating and fixing TS2339 errors for catch clause destructuring

Working on fixing ignored tests related to property-not-exist (TS2339) errors when destructuring catch clause variables. The issue is that tsz suppresses property access errors on `unknown` types, but TypeScript (with `useUnknownInCatchVariables=true`, which is default in strict mode) should report TS2339 when destructuring catch variables like `catch ({ x })` because the catch variable type is `unknown`.

**Test case**:
```typescript
// @strict: true
function f() {
    try {
    } catch ({ x }) {
    }
}
```

Expected: TS2339 error "Property 'x' does not exist on type '{}'"
Actual: No error (suppressed because type is `unknown`)

---

## History (Last 20)

### 2025-02-03: Investigating TS2339 catch clause destructuring issue

### 2025-02-03: Fixed 13 TS2304 (Cannot find name) ignored tests

Fixed all TS2304 ignored tests in `src/tests/checker_state_tests.rs` by adding the missing `checker.ctx.report_unresolved_imports = true;` flag and removing `#[ignore]` attributes.

**Root cause**: The tests were missing `checker.ctx.report_unresolved_imports = true;` before calling `checker.check_source_file(root);`. The flag defaults to `false` which suppresses TS2304 errors for unresolved identifiers in expressions.

**Tests fixed** (13 tests):
- test_missing_identifier_emits_2304
- test_ts2304_undeclared_var_in_function_call
- test_ts2304_undeclared_var_in_binary_expression
- test_ts2304_out_of_scope_block_variable
- test_ts2304_typo_with_suggestion
- test_ts2304_undeclared_var_in_return
- test_ts2304_undeclared_var_in_array_spread
- test_ts2304_undeclared_var_in_object_literal
- test_ts2304_undeclared_var_in_conditional
- test_ts2304_undeclared_class_in_extends
- test_ts2304_undeclared_interface_in_implements
- test_ts2304_undeclared_var_in_template_literal
- test_ts2304_undeclared_var_in_for_of

**Reduced ignored test count by 13** (from 87 to 74 total `#[ignore]` occurrences).

---

## History (Last 20)

*No work history yet*

---

## Punted Todos

*No punted items*
