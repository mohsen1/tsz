# Visibility-Based Type Inlining - Implementation Plan

**Date:** 2026-02-04
**Status:** Defined, not started
**Complexity:** High (4/5)

## Problem

When a function returns a non-exported or local type, tsz currently emits `any` instead of inlining the type structure like TypeScript does.

**Example:**
```typescript
// Input
export function foo() {
    type Local = { value: number };
    return { value: 42 } as Local;
}

// Expected (tsc)
export declare function foo(): { value: number; };

// Current (tsz)
export declare function foo(): any;
```

## Gemini's Architectural Guidance

### 1. Detection Logic

**Local (inside function):** Symbol's parent chain hits `Function` or `Method` scope instead of `SourceFile` scope.

**Non-exported (module-level):** Symbol is child of `SourceFile` but doesn't have `EXPORT` flag in `SymbolFlags`.

**Check:**
```rust
let Some(symbol) = binder.symbols.get(sym_id) else { return false };
(symbol.flags & crate::binder::symbol_flags::EXPORT) != 0
```

### 2. Implementation Location

**Primary file:** `src/solver/format.rs`
- Function: `format_key` (around line 118)
- Modification: Check if Lazy type's symbol is exported
- If not exported: resolve to structure and format recursively

**Alternative:** `src/emitter/type_printer.rs`
- Similar modification to `print_type`

### 3. Inline Logic

When encountering `TypeKey::Lazy(def_id)`:
1. Map `def_id` to `SymbolId` using `type_cache.def_to_symbol`
2. Check if that `SymbolId` is exported
3. If **not exported**: resolve `def_id` to structural `TypeId` using `resolver.resolve_lazy()`
4. Recursively print structural type instead of name
5. Check `current_depth` vs `max_depth` to prevent infinite recursion (line 103 in format.rs)

### 4. Type Inlining Reference

**File:** `src/solver/format.rs`
- Function: `format_object` (line 241)
- Function: `format_object_with_index` (line 256)
- Converts `ObjectShape` to string like `{ value: number; }`

## Implementation Steps

1. **Update TypePrinter/TypeFormatter**
   - Add binder access to check symbol visibility
   - Implement Lazy type resolution with inlining fallback
   - Add recursion depth checking

2. **Coordinate with UsageAnalyzer**
   - Flag non-exported symbols as "needs inlining"
   - Don't generate invalid imports for inlined types

3. **Test Cases**
   - Local types in functions
   - Non-exported module-level types
   - Self-referential types (watch for infinite recursion)
   - Complex nested structures

## Key Files

- `src/solver/format.rs` - TypeFormatter with format_key logic
- `src/emitter/type_printer.rs` - TypePrinter for declaration emit
- `src/binder/symbol_flags.rs` - EXPORT_VALUE flag (line 46)
- `src/declaration_emitter/mod.rs` - print_type_id (line 1445)
- `src/declaration_emitter/usage_analyzer.rs` - Usage tracking

## Success Criteria

- Local types are inlined in function return types
- Non-exported module-level types are inlined
- Exported types continue to use name references
- No infinite recursion on self-referential types
- Matches tsc output exactly
