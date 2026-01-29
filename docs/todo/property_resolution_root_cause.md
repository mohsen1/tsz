# Property Resolution Root Cause Analysis

## Date: January 29, 2026

## Problem

106x TS2339 errors remain stuck despite implementing comprehensive built-in type support.

## Root Cause Identified

**Ref types are not being resolved to their structural forms from lib.d.ts.**

### Code Path Analysis

**File**: `src/solver/evaluate.rs:336-346`
```rust
TypeKey::Ref(symbol) => {
    let result =
        if let Some(resolved) = self.resolver.resolve_ref(*symbol, self.interner) {
            resolved
        } else {
            TypeId::ERROR  // ← PROBLEM: Returns ERROR when resolution fails!
        };
    // ...
}
```

**File**: `src/solver/operations.rs:2911-2930`
```rust
TypeKey::Ref(_) => {
    let evaluated = evaluate_type(self.interner, obj_type);
    if evaluated != obj_type {
        // Successfully evaluated - resolve property on the concrete type
        self.resolve_property_access_inner(evaluated, prop_name, prop_atom)
    } else {
        // Evaluation didn't change the type - try apparent members
        // ← PROBLEM: Falls back to hardcoded lists instead of lib definitions
        if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
            result
        } else {
            // Can't resolve symbol reference - return ANY to avoid false positives
            PropertyAccessResult::Success {
                type_id: TypeId::ANY,
                from_index_signature: false,
            }
        }
    }
}
```

### The Bug

1. User accesses `err.message` on an Error instance
2. `err` is `TypeKey::Ref(SymbolRef(Error))`
3. `evaluate_type()` calls `resolver.resolve_ref(symbol, interner)`
4. **Resolver returns `None`** (can't find Error symbol definition)
5. `evaluate_type()` returns `TypeId::ERROR`
6. Since evaluation failed, code falls back to `resolve_object_member()`
7. `resolve_object_member()` only checks hardcoded apparent members (from `apparent.rs`)
8. Error.message is NOT in apparent members → returns `None`
9. Code returns `Success { type_id: TypeId::ANY }` to avoid false positives

## Why Resolution Fails

The `TypeResolver` trait (in `src/solver/subtype.rs`) needs to provide implementations that can look up symbol definitions from lib.d.ts.

**Current Implementations**:
- `NoopResolver` - Always returns None (used in tests)
- `TypeEnvironment` - Has `types` HashMap but might not be populated with lib symbols

**Missing**:
- Lib file symbols need to be registered with the TypeEnvironment
- The resolver needs access to parsed lib.d.ts interface definitions

## Correct Solution

### Step 1: Ensure Lib Files Are Loaded

**Check**: Are lib.d.ts files being loaded and parsed?

**File**: `src/lib_loader.rs`, `src/preparsed_libs.rs`
- LibLoader loads files from disk
- PreParsedLibs deserializes cached AST + symbol tables
- Need to verify these contain Error, Math, JSON, Symbol definitions

### Step 2: Register Lib Symbols with TypeEnvironment

**Check**: Are lib symbols being added to the TypeEnvironment?

The binder should populate TypeEnvironment with symbol definitions from lib files:
```typescript
// lib.d.ts
interface Error {
    name: string;
    message: string;
    stack?: string;
}
```

Should create a symbol entry with interface properties.

### Step 3: Resolver Must Return Structural Type

When `resolve_ref(Error, interner)` is called:
1. Look up Error symbol in TypeEnvironment
2. Get its type definition (should be an Object or Callable shape)
3. Return that TypeId

Then `evaluate_type()` returns the structural type (not ERROR), and property access works!

## Investigation Tasks

1. **Check lib loading**: Verify Error, Math, JSON, Symbol are in preparsed_libs.bin
2. **Check TypeEnvironment**: Are lib symbols being registered?
3. **Check resolver**: Is TypeEnvironment being used as the resolver?
4. **Check shape creation**: Are interface properties converted to ObjectShape/CallableShape?
5. **Test manually**: Create simple test accessing Error.message and trace through evaluation

## NOT The Solution

❌ **WRONG**: Add more hardcoded methods to operations.rs
- This is what I did (157+ methods hardcoded)
- This violates architectural principles
- Creates maintenance nightmare
- Doesn't fix the root cause

✅ **RIGHT**: Fix the resolver to return structural types from lib.d.ts
- Symbols should resolve to their interface definitions
- Interface properties should be accessible via ObjectShape/CallableShape
- This is how TypeScript ACTUALLY works

## Priority

**CRITICAL** - This is a fundamental infrastructure issue, not a missing features issue.

Once fixed, ALL lib.d.ts defined types will work automatically:
- Error, Math, JSON, Symbol
- Promise, Map, Set, RegExp, Date
- DOM types (if loaded)
- User-defined types from lib files
- Everything!

This is the "biggest bang for the buck" - fixing this ONE issue will resolve ALL the TS2339 errors at once, not just 106x but potentially hundreds more.
