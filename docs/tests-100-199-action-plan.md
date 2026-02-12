# Action Plan: Conformance Tests 100-199

**Mission:** Maximize pass rate for conformance tests at offset 100, max 100 (tests 100-199).

## Test Range Overview

Tests 100-199 primarily focus on **async/await ES5 transpilation**:
- `async` keyword on various declarations (enum, class, interface, getter, setter)
- `await` expressions in various contexts
- Binary expressions with await
- Call expressions with await
- Promise type handling

## Current Code Status

### Already Implemented

#### TS1042: 'async' modifier cannot be used here

Location: `crates/tsz-checker/src/state_checking_members.rs:1485-1507`

```rust
// Checks for async on getters and setters
syntax_kind_ext::GET_ACCESSOR => {
    if let Some(accessor) = self.ctx.arena.get_accessor(node)
        && self.has_async_modifier(&accessor.modifiers)
    {
        self.error_at_node(
            member_idx,
            "'async' modifier cannot be used here.",
            diagnostic_codes::MODIFIER_CANNOT_BE_USED_HERE,
        );
    }
}
```

Also checks:
- Async on class declarations (line 2562)
- Async on interface declarations (line 27)
- Async on enum declarations (line 3997)
- Async on module/namespace declarations (line 4020)

#### Promise Type Checking

Location: `crates/tsz-checker/src/promise_checker.rs`

- `is_promise_type()` - strict Promise type detection
- `type_ref_is_promise_like()` - conservative Promise-like checking
- `classify_promise_type()` - detailed Promise type classification

### Potential Issues to Investigate

#### 1. TS2378: A 'get' accessor must return a value

Test: `asyncGetter_es5.ts` expects this error for async getters with no return.

**Action:** Check if getter return value validation is implemented.

Location to check: `crates/tsz-checker/src/state_checking_members.rs`

```rust
// Look for getter validation that checks for return statements
// Should emit TS2378 when getter has no return value
```

#### 2. Async Function Return Type Validation

Many async tests check whether async functions properly return Promise types.

**Action:** Verify `check_async_function_return_type()` is called for all async functions.

#### 3. Await Expression Type Checking

Tests like `awaitBinaryExpression*` and `awaitCallExpression*` check await in various contexts.

**Action:** Ensure await expression type unwrapping is correct.
- `await Promise<T>` should resolve to `T`
- `await T` where T is not Promise should remain `T`

## Testing Strategy

### Phase 1: Build and Baseline

```bash
# Clean build
cargo build --profile dist-fast -p tsz-cli -p tsz-conformance

# Get current pass rate
./scripts/conformance.sh run --max=100 --offset=100

# Analyze failures by category
./scripts/conformance.sh analyze --max=100 --offset=100
```

### Phase 2: Target High-Impact Errors

```bash
# Find most common error codes
./scripts/conformance.sh analyze --max=100 --offset=100 --category all-missing

# Find close tests (1-2 errors away from passing)
./scripts/conformance.sh analyze --max=100 --offset=100 --category close
```

### Phase 3: Fix and Verify

For each issue:

1. **Create minimal reproduction:**
   ```bash
   # Copy failing test to tmp/
   cp TypeScript/tests/cases/conformance/async/es5/asyncGetter_es5.ts tmp/test.ts
   ```

2. **Run tsz and compare:**
   ```bash
   ./.target/dist-fast/tsz tmp/test.ts 2>&1 | tee tmp/actual.txt
   # Compare with expected errors in baseline
   ```

3. **Fix the issue** in appropriate module

4. **Verify fix:**
   ```bash
   # Unit tests
   cargo nextest run --release

   # Conformance tests
   ./scripts/conformance.sh run --max=100 --offset=100 --filter "asyncGetter"
   ```

5. **Commit and sync:**
   ```bash
   git add <files>
   git commit -m "fix: <description>"
   git pull --rebase origin main
   git push origin main
   ```

## Key Files to Review

### Checker
- `crates/tsz-checker/src/state_checking_members.rs` - Member validation (getters, setters)
- `crates/tsz-checker/src/promise_checker.rs` - Async/Promise type checking
- `crates/tsz-checker/src/type_checking.rs` - Expression type checking
- `crates/tsz-checker/src/function_type.rs` - Function return type validation

### Diagnostics
- `crates/tsz-common/src/diagnostics.rs` - Error code definitions

## Expected Error Codes in This Range

Based on test inspection:

- **TS1042** - 'async' modifier cannot be used here ✅ (already implemented)
- **TS2378** - A 'get' accessor must return a value ⚠️ (needs verification)
- **TS2705** - An async function or method must return a Promise
- **TS2524** - 'await' expressions cannot be used in a parameter initializer
- **Various emit-related errors** for ES5 target

## Architecture Notes

Per `docs/HOW_TO_CODE.md`:
- Checker never inspects type internals directly
- Use classifier queries from Solver for type branching
- Keep hot paths performant (avoid allocations in loops)
- Use tracing, never `eprintln!`

## Next Steps

1. ✅ Document current state (this file)
2. ⏳ Build binary successfully
3. ⏳ Run conformance tests and get baseline
4. ⏳ Analyze failure patterns
5. ⏳ Implement fixes starting with "close" category
6. ⏳ Achieve >90% pass rate for tests 100-199

---

*Created: 2026-02-12*
*Status: Ready for execution once build environment is stable*
