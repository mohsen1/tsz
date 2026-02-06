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
