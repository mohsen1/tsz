# Bug: Symbol() Resolves to Wrong Type

## Issue
When calling `Symbol('test')`, the type checker resolves it to `RTCEncodedVideoFrameType` instead of `symbol`.

## Reproduction
```typescript
const s: symbol = Symbol('test');
```

**Expected**: No error (Symbol() returns symbol)
**Actual**: `error TS2322: Type 'RTCEncodedVideoFrameType' is not assignable to type 'symbol'.`

## Impact
- Affects all tests using Symbol()
- Causes false positive TS2322 errors
- WeakMap/WeakSet/WeakRef tests all fail due to this

## Root Cause Analysis
The `Symbol` global constructor is being resolved to the wrong type. Possible causes:

1. **Symbol table collision**: Multiple symbols with same name, wrong one picked
2. **Lib file loading order**: WebRTC types loaded before/instead of ES2015 Symbol types  
3. **Type computation bug**: Call expression return type computed incorrectly
4. **Type ID corruption**: TypeId values getting mixed up

## Next Steps
1. Debug symbol resolution with tracing for `Symbol` identifier
2. Check lib file loading order and symbol merging
3. Verify call signature extraction from lib.d.ts
4. Test if other global constructors (Array, Object, etc.) have similar issues

## Test Case
```bash
echo 'const s: symbol = Symbol("test");' > /tmp/test.ts
./.target/dist-fast/tsz /tmp/test.ts
# Should pass but shows RTCEncodedVideoFrameType error
```

## Related Files
- `crates/tsz-checker/src/symbol_resolver.rs` - Symbol resolution logic
- `crates/tsz-checker/src/state_type_analysis.rs` - Type computation
- Lib files in TypeScript submodule
