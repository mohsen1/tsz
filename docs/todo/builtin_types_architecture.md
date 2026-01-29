# TypeScript Built-in Types Architecture

## Date: January 29, 2026

## Problem Statement

The user correctly identified that hardcoded built-in type methods (Promise, Map, Set, RegExp, Date, Math, JSON, Error, Symbol) are the WRONG approach. These should come from lib.d.ts files, not from Rust code.

## Architectural Principle

**TypeScript's type system IS lib.d.ts.**

All built-in types are defined in TypeScript declaration files:
- `lib.d.ts` - Core JavaScript built-ins (Error, Math, JSON, Symbol, etc.)
- `lib.es2015.d.ts` - ES2015 features (Promise, Map, Set, etc.)
- `lib.es2015.promise.d.ts` - Promise-specific definitions
- `lib.dom.d.ts` - DOM APIs
- etc.

## Wrong Approach (What Was Done)

Hardcoded property resolution in Rust code:

```rust
// src/solver/operations.rs
fn resolve_math_property(&self, prop_name: &str) -> PropertyAccessResult {
    match prop_name {
        "PI" => PropertyAccessResult::Success { type_id: TypeId::NUMBER },
        // ... 35+ more hardcoded properties
    }
}
```

### Problems with This Approach:

1. **Violates Single Source of Truth**: Types defined in TWO places (lib.d.ts + Rust)
2. **Maintenance Nightmare**: TypeScript adds features → must update in TWO places
3. **Type Inconsistency**: Can diverge from TypeScript's actual definitions
4. **Wrong Abstraction Layer**: Solver should CHECK types, not DEFINE them
5. **Duplicated Effort**: lib.d.ts already has these definitions!

## Correct Approach

The solver should:

1. **Load lib.d.ts files** (already implemented in `src/lib_loader.rs`, `src/preparsed_libs.rs`)
2. **Parse interface definitions** (already done by binder/parser)
3. **Look up properties from parsed interfaces** (THIS IS WHAT'S BROKEN)

### Expected Code Flow:

```typescript
// lib.d.ts says:
interface Error {
  name: string;
  message: string;
  stack?: string;
}
```

```rust
// Compiler should:
fn resolve_property_access(obj_type: TypeId, prop_name: &str) -> PropertyAccessResult {
    // 1. Check if obj_type is a Ref to an interface
    if let TypeKey::Ref(symbol_ref) = self.interner.lookup(obj_type) {
        // 2. Get the symbol's interface definition from lib.d.ts
        if let Some(interface) = self.get_interface_definition(symbol_ref) {
            // 3. Look up the property in the interface
            if let Some(prop) = interface.properties.get(prop_name) {
                return PropertyAccessResult::Success { type_id: prop.type_id };
            }
        }
    }

    // 4. Fall back to computed/apparent properties
    self.resolve_apparent_property(obj_type, prop_name)
}
```

## Root Cause Investigation

The 106x TS2339 errors are NOT from missing built-in types. They're likely from:

1. **Interface property lookup failing** - Symbol refs not resolving to interface definitions
2. **Object/Callable types** - Properties defined on Object/Callable shapes not being found
3. **Type evaluation issues** - Types not being evaluated to their structural form

## Next Steps

### Immediate Actions:

1. **STOP adding hardcoded methods** (they're architectural debt)
2. **Investigate why interface property lookup is failing**
3. **Fix the ROOT CAUSE** in the property resolution code

### Investigation Plan:

1. Check if lib files are being loaded correctly
2. Check if symbols are resolving to their interface definitions
3. Check if interface properties are being stored correctly
4. Check if property lookup is querying the interface definitions

### Files to Investigate:

- `src/solver/operations.rs:2933-2976` - Ref type handling (should NOT need hardcoded checks)
- `src/solver/types.rs` - How are Object/Callable shapes stored?
- `src/solver/db.rs` - TypeDatabase trait - what methods are available for property lookup?
- `src/binder` - How are interface properties bound?

## Key Insight

**The solver already HAS the infrastructure (lib loading, parsing, binding). The bug is in the PROPERTY LOOKUP code path, not missing definitions.**

We need to FIX THE LOOKUP, not add more hardcoded workarounds.
