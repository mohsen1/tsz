# TS2694: Namespace Re-export Fix

## Problem
When a namespace is re-exported using `export { Source }`, accessing its members (e.g., `Destination.Source.sourceValue`) fails with TS2339 "Property does not exist" even though it should work.

## Root Cause
The re-exported symbol (`Source` in `Destination`) becomes an `ALIAS` symbol that points to the namespace `Source`. When we access `Destination.Source.sourceValue`, the type checker:
1. Resolves `Destination.Source` to a `Ref(SymbolRef(alias_id))`
2. Tries to access `sourceValue` on this alias type
3. The alias is not recognized as a namespace, so property access fails

## Solution
Modified `resolve_type_for_property_access_inner` in `src/checker/state.rs` to:
1. Detect when a `Ref` points to an `ALIAS` symbol
2. Check if the alias target is a namespace/module
3. Resolve through the alias to get the actual namespace type
4. Allow property access on the resolved namespace type

## Code Change

In `src/checker/state.rs`, function `resolve_type_for_property_access_inner`:

```rust
TypeKey::Ref(SymbolRef(sym_id)) => {
    let sym_id = SymbolId(sym_id);
    if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
        // ... existing class+namespace handling ...

        // NEW: Handle aliases to namespaces/modules
        if symbol.flags & symbol_flags::ALIAS != 0
            && symbol.flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE | symbol_flags::MODULE) != 0
        {
            let mut visited_aliases = Vec::new();
            if let Some(target_sym_id) = self.resolve_alias_symbol(sym_id, &mut visited_aliases) {
                let target_type = self.get_type_of_symbol(target_sym_id);
                if target_type != type_id {
                    return self.resolve_type_for_property_access_inner(target_type, visited);
                }
            }
        }
    }
    // ... rest of function ...
}
```

## Test Cases

```typescript
// Test: Namespace re-export
namespace Source {
    export const sourceValue = "test";
}

namespace Destination {
    export { Source };
}

// Should work: accessing re-exported namespace member
Destination.Source.sourceValue;

// Before fix: TS2339 - Property 'Source' does not exist
// After fix: Works correctly
```

## Impact
- Reduces extra TS2339 errors (property does not exist)
- Reduces extra TS2694 errors (namespace has no exported member)
- Properly handles re-export chains
- Matches TypeScript's behavior

## Related Error Codes
- TS2694: Namespace has no exported member
- TS2339: Property does not exist on type
