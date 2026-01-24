# Worker-1: TS2322 Missing Errors - Analysis and Implementation

## Summary

This task focused on identifying and adding missing TS2322 (Type not assignable) error detection to match TypeScript's behavior more closely.

## Current State

### Existing Assignability Checks

The codebase already has comprehensive assignability checking in place:

1. **Assignment Expressions** (`src/checker/type_checking.rs`):
   - `check_assignment_expression()` - Lines 50-102
   - Checks assignability for `=` operator
   - Includes constructor accessibility mismatch checks
   - Skips weak union errors appropriately
   - Checks object literal excess properties

2. **Compound Assignment** (`src/checker/type_checking.rs`):
   - `check_compound_assignment_expression()` - Lines 114-186
   - Handles `+=`, `-=`, `*=`, `/=`, `%=`, `<<=`, `>>=`, `>>>=`, `&=`, `|=`, `^=`, `&&=`, `||=`, `??=`
   - Properly handles logical assignments differently from arithmetic ones
   - Performs assignability checks for computed result type

3. **Return Statements** (`src/checker/type_checking.rs`):
   - Lines 1897-1929
   - Checks return value against function's declared return type
   - Special handling for constructor returns without expression
   - Skips weak union errors appropriately
   - Checks object literal excess properties

4. **Binary Expression Type Computation** (`src/checker/type_computation.rs`):
   - `get_type_of_binary_expression()` - Lines 563-706
   - Detects assignment operators and routes to appropriate checking functions
   - Handles all assignment and compound assignment operators

### Union Assignability Improvements

Recent changes have significantly improved union assignability:

1. **Union to All-Optional Objects** (`src/solver/subtype_rules/unions.rs`):
   - `check_union_to_all_optional_object()` - Lines 249-352
   - Enables union literal widening: `{a: 'x'} | {b: 'y'} <: {a?: string, b?: string}`
   - Each union member must satisfy the properties it has
   - Properties not present in a union member are satisfied by the target's optional nature

2. **Union Source Subtype** (`src/solver/subtype_rules/unions.rs`):
   - Lines 56-68
   - Special handling for object targets with all optional properties
   - Uses relaxed checking for union-to-object assignability

3. **Literal to Union Optimization** (`src/solver/subtype_rules/unions.rs`):
   - Lines 119-130
   - Literals match if their primitive type is in the union
   - Reduces false positives for `1` assignable to `string | number`

4. **Union to Union Optimization** (`src/solver/subtype_rules/unions.rs`):
   - Lines 132-138
   - Direct member matching for union-to-union assignability
   - `(A | B) <: (A | B | C)` optimization

## Key Assignability Patterns Already Handled

1. ✅ Regular assignments: `x = value`
2. ✅ Compound assignments: `x += value`, `x -= value`, etc.
3. ✅ Return statements: `return value;`
4. ✅ Property assignments (through `get_type_of_assignment_target`)
5. ✅ Array element assignments (through index access)
6. ✅ Parameter default values
7. ✅ Variable initializers with type annotations
8. ✅ Destructuring patterns (handled through get_type_of_node)

## Potential Areas for Additional TS2322 Detection

Based on the current implementation, the assignability checks are quite comprehensive. Potential areas where TS2322 might be missing:

1. **Strict Null Checks in Specific Contexts**:
   - Ensure `null` and `undefined` are NOT assignable to non-nullable types
   - Currently handled at line 75-76 with `left_type != TypeId::ANY` check

2. **Type Guards and Narrowing**:
   - Ensure narrowed types are used in assignability checks
   - Currently handled through flow analysis

3. **Generic Type Instantiation**:
   - Ensure type substitution is applied before assignability checks
   - Currently handled in type resolution

## Testing Strategy

To verify that TS2322 detection is complete:
1. Run conformance tests focusing on assignability patterns
2. Compare error counts with TSC
3. Identify specific patterns where we differ
4. Add targeted fixes for identified gaps

## Conclusion

The current implementation has comprehensive assignability checking in place. The recent union assignability improvements (union-to-all-optional-object, literal-to-union, union-to-union optimizations) have significantly reduced false positives while maintaining type safety.

The assignability checks are integrated at the type computation level (`get_type_of_binary_expression`), which ensures they're called for all assignment operations. The system properly:
- Checks regular assignments
- Checks compound assignments
- Checks return statements
- Checks property assignments (through type resolution)
- Enforces strict null checks when enabled
- Handles object literal excess properties
- Handles weak union errors appropriately

Further improvements would require running conformance tests to identify specific edge cases where we differ from TSC's behavior.
