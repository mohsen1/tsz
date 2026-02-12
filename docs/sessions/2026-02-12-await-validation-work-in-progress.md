# TS1103/TS1378/TS1432 Await Validation - Work in Progress

## Session Date
2026-02-12

## Goal
Implement validation for await expressions and for-await loops:
- TS1103: for-await loops only allowed in async functions or top-level modules
- TS1308: await expressions only allowed in async functions or top-level modules (already implemented)
- TS1378: top-level await requires ES2022+/ESNext module and ES2017+ target
- TS1432: top-level for-await requires ES2022+/ESNext module and ES2017+ target

## Expected Impact
- 27 tests in conformance suite according to action plan
- Example test: `awaitInNonAsyncFunction.ts` expects [TS1103, TS1308, TS1378, TS1432]

## Changes Made (Partial)

### 1. Added `check_for_await_statement` method

**File**: `crates/tsz-checker/src/type_checking.rs`

Added basic validation for for-await loops (TS1103):
```rust
pub(crate) fn check_for_await_statement(&mut self, stmt_idx: NodeIndex) {
    if !self.ctx.in_async_context() {
        use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};
        self.error_at_node(
            stmt_idx,
            diagnostic_messages::FOR_AWAIT_LOOPS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS_OF,
            diagnostic_codes::FOR_AWAIT_LOOPS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS_OF,
        );
    }
}
```

**Status**: ✅ Basic implementation added, but incomplete
**Missing**: Top-level module check and TS1432 for module/target validation

### 2. Called from for-await loop handling

**File**: `crates/tsz-checker/src/statements.rs`

Added call to `check_for_await_statement` when processing for-await loops:
```rust
if await_modifier {
    state.check_for_await_statement(stmt_idx);
    state.check_await_expression(expression);
}
```

**Status**: ✅ Call added

### 3. Added trait method declaration

**File**: `crates/tsz-checker/src/statements.rs`

Added to `StatementCheckCallbacks` trait:
```rust
fn check_for_await_statement(&mut self, stmt_idx: NodeIndex);
```

**Status**: ✅ Trait method added

## Work Remaining

### 1. Enhance `check_await_expression` for TS1378

The existing `check_await_expression` in `type_checking.rs` (line ~853) currently only checks `!self.ctx.in_async_context()` and emits TS1308.

**Needs**:
```rust
syntax_kind_ext::AWAIT_EXPRESSION => {
    // Validate await expression context
    if !self.ctx.in_async_context() {
        use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};

        // Check if we're at top level of a module
        let at_top_level = self.ctx.function_depth == 0;

        if at_top_level {
            // TS1378: Top-level await requires ES2022+/ESNext module and ES2017+ target
            if !self.supports_top_level_await() {
                self.error_at_node(
                    current_idx,
                    diagnostic_messages::TOP_LEVEL_AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS,
                    diagnostic_codes::TOP_LEVEL_AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS,
                );
            }
        } else {
            // TS1308: 'await' expressions are only allowed within async functions
            self.error_at_node(
                current_idx,
                diagnostic_messages::AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS,
                diagnostic_codes::AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS,
            );
        }
    }
}
```

### 2. Add `supports_top_level_await()` helper method

**Location**: `crates/tsz-checker/src/type_checking.rs` (before `check_await_expression`)

```rust
/// Check if current compiler options support top-level await.
///
/// Top-level await is supported when:
/// - module is ES2022, ESNext, System, Node16, NodeNext, or Preserve
/// - target is ES2017 or higher
fn supports_top_level_await(&self) -> bool {
    use tsz_common::common::{ModuleKind, ScriptTarget};

    // Check module kind supports top-level await
    let module_ok = matches!(
        self.ctx.compiler_options.module,
        ModuleKind::ES2022
            | ModuleKind::ESNext
            | ModuleKind::System
            | ModuleKind::Node16
            | ModuleKind::NodeNext
            | ModuleKind::Preserve
    );

    // Check target is ES2017 or higher
    let target_ok = self.ctx.compiler_options.target as u32 >= ScriptTarget::ES2017 as u32;

    module_ok && target_ok
}
```

### 3. Enhance `check_for_await_statement` for TS1432

Similarly to await expressions, check_for_await_statement needs the same top-level module/target validation:

```rust
pub(crate) fn check_for_await_statement(&mut self, stmt_idx: NodeIndex) {
    if !self.ctx.in_async_context() {
        use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};

        // Check if we're at top level of a module
        let at_top_level = self.ctx.function_depth == 0;

        if at_top_level {
            // TS1432: Top-level for-await requires ES2022+/ESNext module and ES2017+ target
            if !self.supports_top_level_await() {
                self.error_at_node(
                    stmt_idx,
                    diagnostic_messages::TOP_LEVEL_FOR_AWAIT_LOOPS_ARE_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_SET,
                    diagnostic_codes::TOP_LEVEL_FOR_AWAIT_LOOPS_ARE_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_SET,
                );
            }
        } else {
            // TS1103: 'for await' loops are only allowed within async functions
            self.error_at_node(
                stmt_idx,
                diagnostic_messages::FOR_AWAIT_LOOPS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS_OF,
                diagnostic_codes::FOR_AWAIT_LOOPS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS_OF,
            );
        }
    }
}
```

## Key Insights

### Context Tracking
The checker context already tracks:
- `async_depth`: Whether we're in an async function (used by `in_async_context()`)
- `function_depth`: Whether we're at module level (0 = top-level)

### Diagnostic Codes
Confirmed diagnostic codes exist in `crates/tsz-common/src/diagnostics.rs`:
- TS1103 (code: 1103) - for-await loops location check
- TS1308 (code: 1308) - await expression location check
- TS1378 (code: 1378) - top-level await module/target check
- TS1432 (code: 1432) - top-level for-await module/target check

### Module/Target Requirements
From TypeScript diagnostic messages:
- Top-level await/for-await require:
  - Module: ES2022, ESNext, System, Node16, NodeNext, or Preserve
  - Target: ES2017 or higher

## Testing

### Test Case
`TypeScript/tests/cases/compiler/awaitInNonAsyncFunction.ts`

Expected errors: [TS1103, TS1308, TS1378, TS1432]
Current errors: [TS1308] only

### How to Test
```bash
./scripts/conformance.sh run --filter "awaitInNonAsyncFunction"
```

## Next Steps

1. Complete the implementation by adding:
   - `supports_top_level_await()` helper
   - Enhanced `check_await_expression` with TS1378
   - Enhanced `check_for_await_statement` with TS1432

2. Build and test:
   ```bash
   cargo nextest run -p tsz-checker
   ./scripts/conformance.sh run --filter "awaitInNonAsyncFunction"
   ```

3. Run full conformance suite on slice 3:
   ```bash
   ./scripts/conformance.sh run --offset 6292 --max 3146
   ```

4. Commit and push changes

## Files Modified
- `crates/tsz-checker/src/type_checking.rs` - Added check_for_await_statement (partial)
- `crates/tsz-checker/src/statements.rs` - Added call to check_for_await_statement

## Files with Unrelated Changes (Should Review/Revert)
- `crates/tsz-checker/src/context.rs` - Added written_symbols field (unrelated)
- `crates/tsz-checker/src/symbol_resolver.rs` - Unknown changes
- `crates/tsz-checker/src/type_computation.rs` - Unknown changes

## References
- Action plan: `docs/next-session-action-plan.md`
- Original investigation: Action 3 in the plan (TS1362/TS1361 - but those turned out to be different codes)
