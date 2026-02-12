# TS2411 Remaining Issues

## Current Status (FIXED as of 2026-02-12)
TS2411 is now fully implemented:
- ✅ **Inherited index signatures**: Properties are checked against index signatures inherited from base interfaces
- ✅ **Own index signatures**: Properties are now checked against index signatures declared in the same interface (FIXED)

## Previously Failing Test Case (NOW FIXED)
```typescript
interface Derived {
    [x: string]: { a: number; b: number };
    y: { a: number; }  // Now correctly emits TS2411
}
```

**Expected**: TS2411 error on property `y`
**Actual**: ✅ Now correctly emits TS2411

## Root Cause
The check runs in `check_interface_declaration` which gets the interface type via `get_type_of_symbol`. However, at this point:
1. The interface's own index signatures might not be included in the resolved type yet
2. OR `get_index_signatures` doesn't look at the interface's own members, only inherited ones

## Investigation Needed
1. Check when index signatures are added to interface types during type construction
2. Verify if `get_index_signatures` includes the interface's directly declared index signatures
3. Consider checking members array directly for index signature nodes in addition to type-based check

## Impact
- Estimated ~5 more TS2411 tests could pass with this fix
- Tests like `interfaceWithStringIndexerHidingBaseTypeIndexer.ts` are affected

## Fix Applied (2026-02-12)
The fix involved two changes in `check_interface_declaration`:

1. **Guard condition fix**: Check for own index signatures in AST before deciding to call `check_index_signature_compatibility`:
```rust
// Check if there are own index signatures by scanning members
let has_own_index_sig = iface.members.nodes.iter().any(|&member_idx| {
    self.ctx.arena.get(member_idx)
        .map(|node| node.kind == INDEX_SIGNATURE)
        .unwrap_or(false)
});

// Call checker if inherited OR own index signatures exist
if index_info.string_index.is_some()
    || index_info.number_index.is_some()
    || has_own_index_sig {
    self.check_index_signature_compatibility(&iface.members.nodes, iface_type);
}
```

2. **Inside `check_index_signature_compatibility`**: Scan members array to extract own index signatures and merge with inherited ones:
```rust
// Get inherited index signatures from type
let mut index_info = self.ctx.types.get_index_signatures(iface_type);

// ALSO scan members array directly for own index signatures
for &member_idx in members {
    if let Some(index_sig) = /* extract from AST */ {
        // Add to index_info, overriding inherited ones
        index_info.string_index = Some(...);
    }
}
```

This ensures both inherited AND own index signatures are checked.

Commit: `fix(checker): emit TS2411 for properties incompatible with own index signatures`
