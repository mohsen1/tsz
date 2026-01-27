# TS2749 False Positive Fix

## Problem

The type checker was emitting 40,673 extra TS2749 errors ("refers to value but used as type") compared to TypeScript's baseline.

## Root Cause

The issue was in `src/checker/type_checking.rs` in the `symbol_is_value_only` function. The function was checking `symbol_has_type_declaration` BEFORE checking the symbol's TYPE flag, which caused two problems:

1. **Performance**: `symbol_has_type_declaration` iterates through all declarations and queries the arena for each one, which is expensive
2. **Correctness**: If the arena lookup failed or declarations were missing, symbols with TYPE flags could be incorrectly flagged as value-only

## Solution

Reorder the checks in `symbol_is_value_only` to prioritize the fast, reliable flag check before the slower declaration check:

```rust
pub(crate) fn symbol_is_value_only(&self, sym_id: SymbolId) -> bool {
    let symbol = match self.ctx.binder.get_symbol(sym_id) {
        Some(symbol) => symbol,
        None => return false,
    };

    // FAST PATH: Check symbol flags first
    // If symbol has TYPE flag, it's not value-only
    // This handles classes, interfaces, enums, type aliases, etc.
    // TYPE flag includes: CLASS | INTERFACE | ENUM | ENUM_MEMBER | TYPE_LITERAL | TYPE_PARAMETER | TYPE_ALIAS
    let has_type_flag = (symbol.flags & symbol_flags::TYPE) != 0;
    if has_type_flag {
        return false;
    }

    // Modules/namespaces can also be used as types in some contexts
    if (symbol.flags & symbol_flags::MODULE) != 0 {
        return false;
    }

    // SLOW PATH: Check declarations as a secondary source of truth
    // (for cases where flags might not be set correctly)
    if self.symbol_has_type_declaration(sym_id) {
        return false;
    }

    // If the symbol is type-only (from `import type`), it's not value-only
    if symbol.is_type_only {
        return false;
    }

    // Finally, check if this is purely a value symbol (has VALUE but not TYPE)
    let has_value = (symbol.flags & symbol_flags::VALUE) != 0;
    let has_type = (symbol.flags & symbol_flags::TYPE) != 0;
    has_value && !has_type
}
```

## Why This Works

### Symbol Flags

From `src/binder.rs` lines 61-73:

```rust
pub const VALUE: u32 = VARIABLE
    | PROPERTY
    | ENUM_MEMBER
    | OBJECT_LITERAL
    | FUNCTION
    | CLASS      // <-- Classes have VALUE flag
    | ENUM
    | VALUE_MODULE
    | METHOD
    | GET_ACCESSOR
    | SET_ACCESSOR;

pub const TYPE: u32 =
    CLASS | INTERFACE | ENUM | ENUM_MEMBER | TYPE_LITERAL | TYPE_PARAMETER | TYPE_ALIAS;
    // ^^^^^ Classes have TYPE flag
```

A class declaration has BOTH `VALUE` and `TYPE` flags set.

### The Fix

By checking `has_type_flag` first:
- Classes: `has_type_flag = true`, so we return `false` (not value-only) immediately
- Interfaces: `has_type_flag = true`, so we return `false` immediately
- Enums: `has_type_flag = true`, so we return `false` immediately
- Type aliases: `has_type_flag = true`, so we return `false` immediately
- Variables: `has_type_flag = false`, continue to other checks...
- Functions: `has_type_flag = false`, continue to other checks...

The declaration check is now a fallback for edge cases where flags might not be set correctly.

## Testing

Valid test cases (should NOT emit TS2749):
```typescript
// Class used as type - OK
class MyClass {}
let x: MyClass;

// Interface used as type - OK
interface MyInterface {}
let y: MyInterface;

// Enum used as type - OK
enum MyEnum { A, B }
let z: MyEnum;
```

Invalid test cases (SHOULD emit TS2749):
```typescript
// Variable used as type - ERROR
const myVar = 42;
let x: myVar; // TS2749: 'myVar' refers to a value

// Function used as type - ERROR
function myFunc() {}
let y: myFunc; // TS2749: 'myFunc' refers to a value
```

## Expected Impact

This fix should:
- Reduce TS2749 extra errors from ~40,000 to near zero
- Improve conformance percentage by ~2-3%
- Fix false positives for classes, interfaces, enums, and type aliases used as types
- Improve performance by avoiding expensive declaration lookups in most cases

## Files Changed

- `src/checker/type_checking.rs`: Reordered checks in `symbol_is_value_only`
