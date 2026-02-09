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

## Additional Findings

### Other Constructors Work Fine ✓
Tested other global constructors - all work correctly:
```typescript
const arr: number[] = Array(1, 2, 3); // ✓ OK
const obj: object = Object();          // ✓ OK
const str: string = String('hello');   // ✓ OK
const num: number = Number(42);        // ✓ OK
```

Only Symbol is affected, suggesting the issue is specific to Symbol, not a general problem.

### Type Definitions
- **Symbol**: Defined in `TypeScript/src/lib/es2015.symbol.d.ts`
  - `SymbolConstructor` call signature: `(description?: string | number): symbol`
- **RTCEncodedVideoFrameType**: Defined in `dom.generated.d.ts`
  - `type RTCEncodedVideoFrameType = "delta" | "empty" | "key"`
- **No logical connection** between these types

### Test Environment
- Unit tests only load ES5 lib files (Symbol not available in ES5)
- CLI loads full libs including ES2015 and DOM
- Bug only reproduces with full lib set (CLI), not in unit test environment

## Next Steps
1. Debug symbol resolution with tracing for `Symbol` identifier
2. Examine type cache for Symbol - possible stale/wrong entry
3. Check call signature return type extraction for SymbolConstructor
4. Investigate if DOM type definitions interfere with ES2015 types

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
