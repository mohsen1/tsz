# TS2411 Remaining Issues

## Current Status
TS2411 is partially implemented:
- ✅ **Inherited index signatures**: Properties are checked against index signatures inherited from base interfaces
- ❌ **Own index signatures**: Properties are NOT checked against index signatures declared in the same interface

## Failing Test Case
```typescript
interface Derived {
    [x: string]: { a: number; b: number };
    y: { a: number; }  // Should error TS2411 but doesn't
}
```

**Expected**: TS2411 error on property `y`
**Actual**: No error

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

## Potential Fix
Add a second check that looks for index signature members directly in the interface's member list:

```rust
// Current: Check via type
let index_info = self.ctx.types.get_index_signatures(iface_type);

// Additional: Check members array for index signatures
for &member in iface.members.nodes {
    if let Some(member_node) = self.ctx.arena.get(member) {
        if let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) {
            // Check all other non-index-signature members against this
        }
    }
}
```

This would ensure both inherited AND own index signatures are checked.
