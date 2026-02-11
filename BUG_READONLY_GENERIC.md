# Bug: TS2339 False Positive on Generic Parameters with Readonly

## Problem
When accessing properties on generic parameters wrapped in `Readonly<T>`, we incorrectly emit TS2339 "Property does not exist on type 'unknown'".

## Test Case
```typescript
interface Props {
    foo: string;
}

// Works fine
function test1<P extends Props>(props: P) {
    props.foo; // ✓ OK
}

// Incorrectly fails
function test2<P extends Props>(props: Readonly<P>) {
    props.foo; // ✗ ERROR TS2339: Property 'foo' does not exist on type 'unknown'
}
```

## Root Cause Analysis

### What Happens (traced with TSZ_LOG=trace)
1. Parameter `props` in test2 has symbol 3939, type 11154 (`Readonly<P>`)
2. When we access `props.foo`, `get_type_of_element_access` calls:
   - `get_type_of_node(access.expression)` → returns 11154 ✓
   - `evaluate_application_type(11154)` → returns **3** (WRONG!)
   - Symbol 3819 (`Readonly` type alias) has type 3
3. Then `resolve_type_for_property_access(3)` is called
4. Type 3 doesn't have property `foo` → TS2339 error

### The Bug
In `get_type_of_element_access` at line 1131:
```rust
let object_type = self.evaluate_application_type(object_type);
```

`evaluate_application_type` is incorrectly evaluating `Readonly<P>` where `P` is a type parameter.
It returns type 3 (likely the type of the `Readonly` symbol itself) instead of keeping it as the application.

### Partial Fix Applied
Modified `resolve_type_for_property_access_inner` to recursively resolve type arguments in Application types:
```rust
PropertyAccessResolutionKind::Application(app_id) => {
    // Resolve args: Readonly<P> where P extends Props → Readonly<Props>
    let app = self.ctx.types.type_application(app_id);
    let mut resolved_args = Vec::new();
    for &arg in &app.args {
        resolved_args.push(self.resolve_type_for_property_access_inner(arg, visited));
    }
    if any_changed {
        self.ctx.types.application(base, resolved_args)
    } else {
        type_id
    }
}
```

However, this doesn't help because `evaluate_application_type` already corrupted the type to 3.

## Required Fix

Need to either:
1. **Fix `evaluate_application_type_inner`** to not evaluate `Readonly<P>` when `P` is an uninstantiated type parameter
2. **Don't call `evaluate_application_type`** before `resolve_type_for_property_access` in `get_type_of_element_access`
3. **Call `resolve_type_for_property_access` before `evaluate_application_type`** to resolve type parameters first

Option 3 seems safest - swap lines 1131 and 1234 in `type_computation.rs`:
```rust
// Current (wrong order):
let object_type = self.evaluate_application_type(object_type); // Line 1131
// ... 100 lines ...
let object_type = self.resolve_type_for_property_access(object_type); // Line 1234

// Should be:
let object_type = self.resolve_type_for_property_access(object_type);
let object_type = self.evaluate_application_type(object_type);
```

## Impact
Affects 150 conformance tests with TS2339 false positives on generic-wrapped types.

## Files Modified
- `crates/tsz-checker/src/state_type_environment.rs`: Added Application arg resolution
- Tests: test_generic_props.ts, test_readonly_generic.ts, test_both.ts

## Next Steps
1. Investigate `evaluate_application_type_inner` to understand why it returns type 3
2. Try swapping the order of `evaluate` and `resolve` calls
3. Add unit tests for generic parameter property access
4. Verify fix doesn't break existing tests
