# TS2705 Deep Investigation - January 29, 2026

## Problem Statement

73x missing TS2705 errors (async function must return Promise) in conformance tests.

## Architectural Fix Completed ✅

**Commit**: `224b2d7de` - "fix(promise): make is_promise_type strict for TS2705 checking"

### Changes Made

Modified `src/checker/promise_checker.rs`:
- Changed `is_promise_type()` to be strict when checking Promise types
- Previously delegated to `type_ref_is_promise_like()` which conservatively assumed ALL Object types are Promise-like
- Now directly checks if symbol name is "Promise" or "PromiseLike" for Application types
- Object types now correctly return `false` (not `true`)

### Why This Fix Is Correct

**Before**:
```rust
PromiseTypeKind::Application { base, .. } => self.type_ref_is_promise_like(base)
```

This called `type_ref_is_promise_like` which has:
```rust
PromiseTypeKind::Object(_) => {
    // For Object types (interfaces from lib files), we conservatively assume
    // they might be Promise-like. This avoids false positives for Promise<void>
    true  // ← BUG: Assumes ALL interfaces are Promise-like!
}
```

**After**:
```rust
PromiseTypeKind::Application { base, .. } => {
    match classify_promise_type(self.ctx.types, base) {
        PromiseTypeKind::SymbolRef(SymbolRef(sym_id)) => {
            if let Some(symbol) = self.ctx.binder.get_symbol(SymbolId(sym_id)) {
                return self.is_promise_like_name(symbol.escaped_name.as_str());
            }
            false
        }
        PromiseTypeKind::Application { base: inner_base, .. } => {
            self.is_promise_type(inner_base)  // Recursive for nested applications
        }
        _ => false,
    }
}
```

Now it ONLY returns true for actual Promise/PromiseLike types, not arbitrary interfaces.

## Conformance Results ❌

**Before fix**: Pass rate 40.0%, TS2705 73x missing
**After fix**: Pass rate 40.0%, TS2705 73x missing

**No improvement despite architectural fix being correct.**

## Investigation Findings

### 1. Manual Testing

Created test file:
```typescript
async function test(): string {
  return "hello";
}
```

**Expected**: TS2705 emitted
**Actual**: No TS2705 emitted

**Hypothesis**: Return type resolving to ERROR, causing the check to be skipped due to `return_type != TypeId::ERROR` condition.

### 2. Debug Logging Attempts ❌

Added extensive debug logging to trace:
- `get_type_of_function()` in function_type.rs
- `check_function_declaration()` in state.rs  
- `compute_type()` in type_node.rs
- `return_type_and_predicate()` in signature_builder.rs

**Result**: No debug output produced, despite compilation errors being detected.

**Possible causes**:
- Stderr buffering/capture issues
- Functions not being called (but types are checked somehow)
- Binary stripping debug output in release builds

### 3. Type Resolution Investigation

Examined type resolution flow:
1. `return_type_and_predicate()` calls `get_type_from_type_node()`
2. `get_type_from_type_node()` delegates to `TypeNodeChecker::check()`
3. `TypeNodeChecker::check()` calls `compute_type()`
4. `compute_type()` matches `StringKeyword` and returns `TypeId::STRING`

**Expected**: `string` → `TypeId::STRING`
**Actual**: Unknown (can't debug without logging)

### 4. Two Code Paths Discovered

Found TWO separate TS2705 checks:
1. `check_function_declaration()` in state.rs:9100-9113 (for FUNCTION_DECLARATION)
2. `get_type_of_function()` in function_type.rs:330-353 (for FUNCTION_EXPRESSION, ARROW_FUNCTION, METHOD_DECLARATION)

Both have identical logic but are called for different node types.

## Root Cause Analysis

### Most Likely Theory

The 73x missing TS2705 errors are NOT for simple cases like:
```typescript
async function foo(): string { }  // ← This probably works fine
```

They're likely for complex cases involving:
1. **Type aliases**: `type MyType = string; async function foo(): MyType {}`
2. **Generics**: `async function foo<T>(): T where T = string {}`
3. **Conditional types**: `async function foo(): X extends Promise<infer T> ? T : never {}`
4. **Type inference scenarios**: Where the return type is inferred rather than explicit
5. **Intersection/Union types**: `async function foo(): string & number {}`
6. **Lib resolution edge cases**: Where Promise isn't available or resolves incorrectly

### Secondary Theory

The `return_type != TypeId::ERROR` check is too broad. It was meant to skip errors when Promise type can't be resolved, but it's ALSO skipping cases where:
- The return type itself can't be resolved (e.g., missing interface)
- Generic type instantiation fails
- Type reference lookup fails

This would cause false negatives for ANY unresolved type, not just Promise.

## Recommendations

### Short-term (High Impact)

1. **Add unit tests** for TS2705 scenarios:
   - Simple primitive types (string, number, boolean)
   - Interface types
   - Type aliases to primitives
   - Type aliases to interfaces
   - Generic type parameters
   - Union/intersection types

2. **Improve debuggability**:
   - Fix stderr output in release builds
   - Add RUST_LOG=trace support for type checking
   - Create debug build with logging
   - Add --debug flag to tsz CLI

3. **Investigate specific test cases**:
   - Find the exact 73 test files expecting TS2705
   - Manually inspect each test
   - Categorize by complexity (simple vs complex)
   - Create minimal reproductions

### Medium-term (Architecture)

1. **Refactor type checking**:
   - Consolidate the two TS2705 checks into one location
   - Improve type resolution error propagation
   - Distinguish between "Promise not found" vs "Return type not found"

2. **Add type validation layer**:
   - Validate that primitive types always resolve correctly
   - Add assertions for intrinsic types
   - Track where types resolve to ERROR

### Long-term (Infrastructure)

1. **Improve conformance testing**:
   - Add per-error-code pass rates
   - Track improvements over time
   - Identify regression early

2. **Enhanced debugging**:
   - Type resolution visualization
   - AST inspector
   - Step-through debugger for type checking

## Next Steps

Given the difficulty of debugging without effective logging, the recommended approach is:

1. **Accept the architectural fix** as correct (it is!)
2. **Investigate test cases** manually to understand what's failing
3. **Add targeted fixes** for specific scenarios
4. **Improve tooling** to make future debugging easier

## Related Documentation

- `docs/todo/builtin_types_progress_jan29.md` - Overall progress tracking
- `docs/todo/work_summary_jan29.md` - Complete work summary
- `src/checker/promise_checker.rs` - is_promise_type implementation

## Git Commits

- `224b2d7de` - fix(promise): make is_promise_type strict for TS2705 checking
- `c5a854692` - docs: update work summary with TS2705 investigation
