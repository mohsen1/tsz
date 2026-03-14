# Note: subtype-relations → contextual-typing

## Issue: Widened literals cause false-positive TS2322 in reverse-mapped inference

### Root cause
When a generic function with a homomorphic mapped type parameter (like `Results<T> = { [K in keyof T]: { data: T[K]; ... } }`) is called with an object literal argument, the inner nested object literal properties (e.g., `data: "foo"`) get their literal types **widened** (from `"foo"` to `string`) because the contextual type is not set when computing the inner object's property types.

### Mechanism
In `object_literal.rs`, when computing property value types:
1. The outer object literal gets contextual type from the parameter type (the mapped type Application)
2. For each outer property (e.g., `a`), `contextual_object_literal_property_type` is called to find the expected type
3. The inner object literal `{ data: "foo", onSuccess: ... }` should get its contextual type from the outer property's expected type
4. But the contextual type isn't being propagated deeply enough — `property_context_type` is `None` for the inner properties
5. Without contextual type, the widening logic at line 265 kicks in and widens `"foo"` to `string`

### Impact
~59 tests with false-positive TS2322 (many showing "Type 'X' is not assignable to type 'X'" because the source was widened)

### Affected tests (representative)
- `reverseMappedIntersectionInference1.ts` — `"foo"` not assignable to `"foo"`
- `reverseMappedIntersectionInference2.ts`
- `checkJsdocTypeTagOnExportAssignment8.ts` — `"b"` not assignable to `"b"`
- `genericContextualTypes2.ts`, `genericContextualTypes3.ts`
- Many more in the false-positive TS2322 category

### Suggested fix location
`crates/tsz-checker/src/types/computation/object_literal_context.rs` — the `contextual_object_literal_property_type` function needs to handle Application/mapped types that result from generic inference, so they can provide contextual types to nested object literal properties.
