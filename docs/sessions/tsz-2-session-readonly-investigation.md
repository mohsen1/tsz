# Session tsz-2: Readonly Assignment Investigation

**Date:** 2026-02-06  
**Status:** Partial fix completed, investigation ongoing

## Completed Work

### Fix: Readonly Array Element Assignment (TS2540)

**Problem:**
```typescript
const xs: readonly number[] = [1, 2];
xs[0] = 3;  // Should emit TS2540
```

**Root Cause:**
- `ReadonlyType(Array(number))` wrapper was being lost during CFA type resolution
- In `get_type_of_identifier`, flow_type from CFA was used instead of declared_type
- The condition only preserved declared_type for `ObjectWithIndex` when flow_type == `ANY`

**Solution:**
- Modified `src/checker/type_computation_complex.rs` to preserve `ReadonlyType` wrapper unconditionally
- Check for `ReadonlyType` before checking for `ObjectWithIndex`

**Test Results:**
- ✅ test_readonly_array_element_assignment_2540 - PASS
- ✅ test_readonly_property_assignment_2540 - PASS
- ❌ test_readonly_element_access_assignment_2540 - FAIL (interface readonly property)
- ❌ test_readonly_index_signature_element_access_assignment_2540 - FAIL (interface readonly index signature)
- ❌ test_readonly_method_signature_assignment_2540 - FAIL (interface readonly method)

**Commit:** c2db62fbe

## Ongoing Investigation

### Issue: Interface Readonly Properties Not Checked

**Problem:**
```typescript
interface Config {
    readonly name: string;
}
let config: Config = { name: "ok" };
config["name"] = "error";  // Should emit TS2540 but doesn't
config.name = "error";    // Should emit TS2540 but doesn't
```

**Investigation Findings:**

1. **Interface Lowering is Correct:**
   - `merge_property` receives `readonly=true` 
   - `finish_interface_parts` passes `readonly=true`
   - ObjectShape SHOULD be created with `readonly=true`

2. **Type Resolution Path:**
   - `obj_type = get_type_of_node(access.expression)` returns `Object(ObjectShapeId(3026))`
   - Shape has 1 property `name` with `readonly=false` (should be `true`)
   - Type is already resolved (not `Lazy`), so the issue is NOT in Lazy resolution

3. **Root Cause Hypothesis:**
   - The ObjectShape being used might not be the lowered interface shape
   - Object literal widening might be creating a new shape without readonly flags
   - OR there's a hash collision returning the wrong shape

4. **`property_is_readonly` Limitation:**
   - Function uses `NoopResolver` which can't resolve `Lazy(DefId)` types
   - But the type is already resolved to `Object`, so this shouldn't be the issue
   - Located in `src/solver/operations_property.rs`

5. **Object Literal Type Computation:**
   - Object literals create properties with `readonly: false` (line 1560, 1605 in type_computation.rs)
   - This is correct - there's no `readonly` syntax in object literals
   - The issue is in how the annotated type (`Config`) is used vs the initializer type

**Next Steps:**
- Trace the exact TypeId flow: what TypeId is created for `Config` interface vs what TypeId is used for `config` variable
- Check if there are multiple ObjectShapes with the same structure but different readonly flags
- Investigate hash collision in type interning
- OR investigate if the object literal type is being used instead of the interface type

## Technical Notes

### Files Modified:
- `src/checker/type_computation_complex.rs` - Preserve ReadonlyType wrapper
- `src/checker/state_checking.rs` - Readonly checking logic
- `src/solver/operations_property.rs` - `property_is_readonly` function
- `src/tests/checker_state_tests.rs` - Added lib context setup

### Key Functions:
- `get_type_of_identifier` in `type_computation_complex.rs` - Variable type resolution
- `check_readonly_assignment` in `state_checking.rs` - TS2540 checking
- `property_is_readonly` in `operations_property.rs` - Readonly property lookup
- `object_property_is_readonly` in `operations_property.rs` - Object shape readonly check

---

## Deep Investigation (2026-02-06 Continued)

### Issue: Type Precedence - Object Literal Override Interface

After extensive debugging with Gemini's guidance, identified the core issue:

**Debug Output:**
```
declared_type=TypeId(494) (Some(Lazy(DefId(7)))), flow_type=TypeId(571) (Some(Object(ObjectShapeId(63))))
```

- `declared_type`: `Lazy(DefId(7))` - the interface `Config` with readonly properties
- `flow_type`: `Object(ObjectShapeId(63))` - the object literal type WITHOUT readonly properties
- The type being used for readonly checking: `Object(ObjectShapeId(64))` - ANOTHER object type
- Property on shape 64: `readonly=false` (should be `true`)

**Root Cause:**
In `get_type_of_identifier` (type_computation_complex.rs), the fix only preserves:
- `ReadonlyType` wrapper (for `readonly number[]` style)
- `ObjectWithIndex` when `flow_type == ANY`

For interface readonly properties:
- The declared type is `Lazy(DefId)` - the interface
- The flow type is `Object` - the object literal
- The fix doesn't handle `Lazy` types, so `flow_type` is used
- Result: readonly flags from interface are lost

### Attempted Fixes:

1. **Add TypeInterner override for `is_property_readonly`**
   - Created override in `TypeInterner` impl to use `evaluate_type_with_options`
   - This didn't help because the type is already resolved to `Object` before reaching `is_property_readonly`

2. **Preserve `Lazy` types in type resolution**
   - Tried to preserve `declared_type` when it's `Lazy`
   - Result: Infinite recursion (Lazy → resolve → Lazy → resolve → ...)
   - Reason: The `Lazy` type can't be resolved at that point in type checking

3. **Resolve `Lazy` first, then check if interface**
   - Tried calling `evaluate_type_with_options` on `Lazy` before checking
   - Result: Returns same unresolved `Lazy` type
   - Reason: Interface definition not available yet during type checking

### Gemini's Analysis (Pro Model):

**Key Insight:** This is a **fundamental architecture issue** that must be fixed.

**The Real Problem:**
- When `const x: MyInterface = { ... }` is declared:
  - TypeScript: The type of `x` for subsequent usage IS `MyInterface`
  - TSZ: The `flow_type` (Object Literal) overrides `declared_type` (Interface)

**Required Fix:**
1. **Fix type precedence in `src/checker/declarations.rs`**: Symbols MUST retain their declared interface type, not the initializer type
2. **Fix recursion in `src/solver/subtype.rs`**: Add proper cycle detection for `Lazy` types
3. **Object Literal is only for assignment check**: The literal type should be used for the initial assignability check, not for the symbol's type

**Next Steps:**
- Investigate `src/checker/declarations.rs` - ensure `register_symbol_type` uses the declared type
- Add cycle detection in `src/solver/subtype.rs` for `Lazy` types before expanding
- Use tracing to debug the recursion: `TSZ_LOG="wasm::solver::subtype=trace"`

### Test Status:
- ✅ test_readonly_array_element_assignment_2540 - PASS
- ✅ test_readonly_property_assignment_2540 - PASS
- ❌ test_readonly_element_access_assignment_2540 - FAIL (interface readonly property)
- ❌ test_readonly_index_signature_element_access_assignment_2540 - FAIL (interface readonly index signature)
- ❌ test_readonly_method_signature_assignment_2540 - FAIL (interface readonly method)

Overall: 8273 tests passing, 39 failing, 158 ignored

## Latest Attempt (2026-02-06 Final)

### Tried: Preserve Lazy types in get_type_of_identifier
Added case in type_computation_complex.rs:

```rust
Some(crate::solver::TypeKey::Lazy(_)) => declared_type,
```

**Result:** Stack overflow (infinite recursion)

**Root Cause:** Preserving Lazy type creates unresolvable cycle:
1. Return `Lazy(DefId)` from `get_type_of_identifier`
2. Later code tries to resolve it via `evaluate_type` or `property_is_readonly`
3. Resolution tries to access properties
4. Property access tries to get type again
5. Returns same `Lazy(DefId)` → infinite loop

### The Trilemma:
Three conflicting constraints:
1. **Can't preserve Lazy** → causes infinite recursion
2. **Can't resolve Lazy** → definition not available during type checking
3. **Can't use flow_type** → loses readonly flags from interface

### Conclusion:
This task is blocked on fundamental architectural work requiring:
- Cycle detection in property access resolution path
- OR separation of "symbol's declared type" from "type for current usage"
- OR tracking type modifiers separately from structural types

### Recommendation:
Move to different task with better ROI (e.g., Task #18 index signatures).
This requires deeper architectural changes that should be planned carefully.
