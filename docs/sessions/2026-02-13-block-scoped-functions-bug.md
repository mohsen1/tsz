# Block-Scoped Function Declarations Bug - 2026-02-13

## Problem

Function declarations inside blocks are incorrectly hoisted to module scope in ES6+ modules (which are automatically strict mode).

## Reproduction

```typescript
// test.ts (module file with imports/exports)
if (true) {
    function foo() { return "inside"; }
    foo(); // OK
}
foo(); // Should ERROR: TS2304 Cannot find name 'foo'
export = foo; // Should ERROR: TS2304 Cannot find name 'foo'
```

**TSC**: Reports TS1252 and TS2304 (x2)  
**TSZ**: Reports nothing - incorrectly allows access

## Root Cause

**Location**: `crates/tsz-binder/src/state.rs:2118-2127`

The `collect_hoisted_declarations` function:
1. Recursively enters blocks to collect var declarations (line 2125)
2. When it finds a FUNCTION_DECLARATION, it unconditionally adds it to `hoisted_functions` (line 2119)
3. These functions are then processed at module scope (line 1602)

The comment says "Always recurse into blocks for var hoisting" but the recursion also collects functions, which is incorrect for ES6+ strict mode.

## TypeScript Behavior

### In ES5 or Non-Strict Mode
- Function declarations ARE hoisted to function/module scope
- `foo()` outside the block would work

### In ES6+ Strict Mode (Modules)
- Modules are automatically strict
- Function declarations inside blocks are block-scoped
- TS1252: "Function declarations are not allowed inside blocks in strict mode"
- TS2304: "Cannot find name 'foo'" for out-of-scope access

## Solution Approach

### Option 1: Don't Collect Functions from Blocks in Modules

Add a parameter or field to track if we're collecting from a block:

```rust
pub(crate) fn collect_hoisted_declarations(
    &mut self,
    arena: &NodeArena,
    statements: &NodeList,
    in_block: bool, // NEW
) {
    // ...
    k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
        // Only hoist if not in a block OR not in strict mode
        if !in_block || !self.is_strict_mode() {
            self.hoisted_functions.push(stmt_idx);
        }
    }
    k if k == syntax_kind_ext::BLOCK => {
        if let Some(block) = arena.get_block(node) {
            self.collect_hoisted_declarations(arena, &block.statements, true);
        }
    }
}
```

### Option 2: Check Scope at Process Time

When processing hoisted functions, check if they're actually in a nested scope:

```rust
pub(crate) fn process_hoisted_functions(&mut self, arena: &NodeArena) {
    let functions = std::mem::take(&mut self.hoisted_functions);
    for func_idx in functions {
        // Skip if function is in a block and we're in strict mode
        if self.is_in_block_scope(func_idx) && self.is_strict_mode() {
            continue; // Will be bound normally later
        }
        // ... existing logic
    }
}
```

### Recommended: Option 1

More explicit and prevents collecting in the first place. Cleaner separation of concerns.

## Additional Work Required

1. **Add TS1252 Error**: "Function declarations are not allowed inside blocks in strict mode"
   - Check at binding time in `bind_function_declaration`
   - Only for modules or explicit strict mode
   - Location: `crates/tsz-checker/src/` (diagnostic reporting)

2. **Test Both Modes**: 
   - ES5 target (functions ARE hoisted)
   - ES6+ modules (functions are block-scoped)

## Impact

- Affects 12+ conformance tests with missing TS2304 errors
- Correctness issue (too lenient) rather than false positive
- Medium priority - affects code that relies on proper scoping

## Test Cases

```typescript
// Should work - top level
function top() {}
top(); // OK

// Should fail - in block (module)
if (true) {
    function blocked() {}
    blocked(); // OK inside block
}
blocked(); // ERROR: TS2304

// Should work - in block (ES5 or non-strict)
// @target: ES5
if (true) {
    function es5Hoisted() {}
}
es5Hoisted(); // OK in ES5
```

## Files to Modify

1. `crates/tsz-binder/src/state.rs` - Fix hoisting logic
2. `crates/tsz-checker/src/` - Add TS1252 diagnostic
3. Tests - Add test cases for both modes

## References

- TypeScript issue #20454 (mentioned in test comment)
- ES6 spec: Block-scoped function declarations in strict mode
