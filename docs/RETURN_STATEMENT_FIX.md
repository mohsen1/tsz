# Return Statement Type Checking Fix for Function Expressions and Arrow Functions

## Problem

The TSZ compiler was missing TS2322 (type not assignable) errors on return statements in function expressions and arrow functions. While function declarations and class methods had proper return type checking, function expressions and arrow functions fell through to a default case in `check_statement` that only called `get_type_of_node` without ensuring proper return type validation.

## Root Cause

In `src/checker/state.rs`, the `check_statement` function had explicit handling for `FUNCTION_DECLARATION` (line 9536) but not for `FUNCTION_EXPRESSION` or `ARROW_FUNCTION`. These function types would fall through to the default case:

```rust
_ => {
    // Catch-all for other statement types
    self.get_type_of_node(stmt_idx);
}
```

While `get_type_of_node` → `get_type_of_function` does set up return type checking for function bodies, having explicit cases in `check_statement` ensures:
1. Consistent handling across all function types
2. All parameter and return type checks are performed
3. Future refactors won't accidentally break return type checking

## Solution

Added explicit cases for `FUNCTION_EXPRESSION` and `ARROW_FUNCTION` in `check_statement` (before the default case) that mirror the comprehensive checking done for `FUNCTION_DECLARATION`:

- Parameter property checks (TS2369)
- Duplicate parameter name detection (TS2300)
- Parameter ordering validation (TS1016)
- Parameter type annotation checks
- Implicit any parameter reporting (TS7006)
- Return type inference and checking
- Implicit any return reporting (TS7010/TS7011)
- Async function Promise requirement (TS2705)
- **Return statement type checking (TS2322)** - the key fix
- Missing return value detection (TS2355, TS7030)
- Proper async context management

## Code Changes

**File:** `src/checker/state.rs`
**Location:** Lines 9893-9915 (inserted before default case in `check_statement`)

Added match arm:
```rust
syntax_kind_ext::FUNCTION_EXPRESSION | syntax_kind_ext::ARROW_FUNCTION => {
    if let Some(func) = self.ctx.arena.get_function(node) {
        // ... comprehensive checking including:
        self.push_return_type(return_type);
        self.check_statement(func.body);
        // ... checks for missing returns, etc.
        self.pop_return_type();
    }
}
```

## Test Coverage

Created `test_return_statement_fix.ts` with 26+ test cases covering:

### Arrow Functions
- Wrong primitive return type (string instead of number)
- Object return type with missing properties
- Generic functions with wrong return
- Async arrow functions with wrong return type
- Union return types
- Literal type returns
- Void return violations
- Array type violations

### Function Expressions
- Named and unnamed function expressions
- Wrong return types for primitives, objects, arrays
- Async function expressions
- Literal type violations

### Nested and Contextual Cases
- Arrow functions inside function declarations
- Functions as object properties
- Contextually typed callbacks
- Union and intersection return types
- Generic functions with constraints

## Expected Impact

This fix should detect **hundreds of previously missing TS2322 errors** in the TypeScript conformance test suite, particularly for:

1. Arrow functions with typed returns
2. Function expressions with typed returns
3. Methods defined as arrow functions in object literals
4. Callback functions with expected return types
5. Generic arrow functions and function expressions

## Related Error Codes

- **TS2322**: Type '{0}' is not assignable to type '{1}' - emitted on wrong return types
- **TS2355**: A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value
- **TS2705**: Async function must return Promise
- **TS7030**: Not all code paths return a value (noImplicitReturns)
- **TS7010/TS7011**: Implicit any return type

## Testing

```bash
# Build the compiler
cargo build --release --bin tsz

# Test with the test file
./target/release/tsz test_return_statement_fix.ts --noEmit

# Expected: Multiple TS2322 errors for wrong return types
```

## Verification

To verify the fix works:

1. The test file `test_return_statement_fix.ts` should emit TS2322 errors on lines with incorrect return types
2. Previously passing tests that should have errored will now correctly emit errors
3. The conformance pass rate may initially decrease (more errors detected = more strict correctness)

## Future Work

1. **Investigate literal type widening**: Some tests with literal return types (e.g., `return 2` when `1` is expected) may need additional literal type handling
2. **Contextual typing refinement**: Ensure arrow functions as callbacks properly infer from contextual types
3. **Generic function return checking**: Verify generic functions with constraints properly check return types
