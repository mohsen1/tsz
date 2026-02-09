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

## Code Path Analysis

### Call Expression Type Resolution
Location: `crates/tsz-checker/src/type_computation_complex.rs`

**Key Functions**:
- `get_type_of_call_expression` (line 908) - Entry point with recursion guard
- `get_type_of_call_expression_inner` (line 921) - Main resolution logic

**Resolution Flow for `Symbol('test')`**:
```rust
// 1. Get callee type (should resolve to SymbolConstructor)
let mut callee_type = self.get_type_of_node(call.expression); // line 935

// 2. Apply type arguments if present (none for Symbol())
let callee_type_for_resolution = self.apply_type_arguments_to_callable_type(...); // line 1037

// 3. Classify callable and extract call signatures
let overload_signatures = tsz_solver::type_queries::classify_for_call_signatures(...); // line 1043

// 4. Resolve call with signatures to get return type
let return_type = self.resolve_overloaded_call_with_signatures(...); // line 1073
```

**Bug Location**: Return type comes back as `RTCEncodedVideoFrameType` instead of `symbol`.
Likely issues:
- Callee type resolves to wrong symbol (step 1)
- Call signature extraction gets wrong signature (step 3)
- Return type mapping is corrupted (step 4)

### Symbol Resolution Path
Location: `crates/tsz-checker/src/symbol_resolver.rs`

When `Symbol` identifier is encountered:
1. Check local scope chain
2. Check type parameters
3. Check module exports
4. Check file locals
5. **Fallback to lib_contexts** (lines 322-341) - Global symbols

**Key Investigation**: How are DOM and ES2015 lib symbols merged?

## Next Steps (Prioritized)

### High Priority
1. **Add targeted debug output**:
   ```rust
   // In get_type_of_call_expression_inner at line 935
   eprintln!("DEBUG: Callee type for {:?} = {:?}", call.expression, callee_type);
   ```

2. **Check symbol resolution**:
   - What symbol does "Symbol" identifier resolve to?
   - Is it getting SymbolConstructor or something else?

3. **Verify call signature extraction**:
   - What signatures are extracted for the resolved type?
   - Does it have the correct `(): symbol` signature?

### Medium Priority
4. **Check type cache**: Look at `state_type_analysis.rs` `get_type_of_symbol`
5. **Test with isolated libs**: Run with only ES2015, no DOM types
6. **Examine lib merging**: How are conflicting global names handled?

### Investigation Commands
```bash
# Add tracing to type resolution
TSZ_LOG="tsz_checker::type_computation_complex=trace" cargo run -- /tmp/test.ts

# Check what type Symbol resolves to
TSZ_LOG="tsz_checker::symbol_resolver=debug" cargo run -- /tmp/test.ts
```

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
