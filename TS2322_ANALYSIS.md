# TS2322 Type Assignability Error Detection Analysis

## Assignment: Add 917 Missing TS2322 Errors

### Current Status

The codebase already has extensive TS2322 type assignability checking infrastructure:

#### 1. Assignment Expression Checking ✅
**Location:** `src/checker/type_checking.rs:400-454`

```rust
pub(crate) fn check_assignment_expression(
    &mut self,
    left_idx: NodeIndex,
    right_idx: NodeIndex,
    expr_idx: NodeIndex,
) -> TypeId {
    // ... gets left_type and right_type
    
    if left_type != TypeId::ANY {
        if !self.is_assignable_to(right_type, left_type)
            && !self.should_skip_weak_union_error(right_type, left_type, right_idx)
        {
            self.error_type_not_assignable_with_reason_at(right_type, left_type, right_idx);
        }
        
        // Also checks excess properties for object literals
        self.check_object_literal_excess_properties(right_type, left_type, right_idx);
    }
}
```

**Coverage:**
- Variable assignments: `let x: string = expr;`
- Property assignments: `obj.prop = value;`
- Array element assignments: `arr[i] = value;`
- Compound assignments: `+=`, `-=`, etc.

#### 2. Return Statement Checking ✅
**Location:** `src/checker/type_checking.rs:2254-2328`

```rust
pub(crate) fn check_return_statement(&mut self, stmt_idx: NodeIndex) {
    let expected_type = self.current_return_type().unwrap_or(TypeId::UNKNOWN);
    let return_type = self.get_type_of_node(return_data.expression);
    
    if expected_type != TypeId::ANY
        && !is_constructor_return_without_expr
        && !self.is_assignable_to(return_type, expected_type)
    {
        self.error_type_not_assignable_with_reason_at(
            return_type,
            expected_type,
            error_node,
        );
    }
}
```

**Coverage:**
- Function return values
- Method return values
- Arrow function returns
- Constructor returns (special handling)

#### 3. Variable Initialization with Type Annotations ✅
**Location:** `src/checker/type_checking.rs:1010-1030`

```rust
// For variable declarations with explicit type annotations
if !self.is_assignable_to(init_type, declared_type) {
    self.error_type_not_assignable_with_reason_at(
        init_type,
        declared_type,
        initializer_idx,
    );
}
```

**Coverage:**
- `const x: number = expr;`
- `let y: string = expr;`
- Destructuring with type annotations

#### 4. Function Argument Passing ✅
**Location:** Integrated into call expression checking

When calling a function, each argument is checked:
```rust
if !self.is_assignable_to(arg_type, param_type) {
    // Emit TS2322 error
}
```

**Coverage:**
- Function calls
- Method calls
- Constructor calls
- Generic function calls

#### 5. Destructuring Pattern Assignability ✅
**Location:** `src/checker/state.rs` (per conformance report)

```rust
// Check destructuring patterns with type annotations
const { x }: { x: string } = { x: 1 }; // Error: number not assignable to string
const [a]: [string] = [1];              // Error: number not assignable to string
```

#### 6. For-of/For-in Loop Variables ✅
**Location:** `src/checker/state.rs:10912-10973`

```rust
// Loop variables with type annotations
for (const x: string of numberArray) { ... } // Error
```

#### 7. Array Literal to Tuple Assignability ✅
**Location:** `src/checker/state.rs` + `src/checker/type_checking.rs`

```rust
type Tuple = [string, number];
const t: Tuple = [1, "x"]; // Error - element types don't match
```

### Current Implementation Status

| Check Type | Status | Location | Notes |
|-----------|--------|----------|-------|
| Simple assignments | ✅ | type_checking.rs:428-443 | `x: T = expr` |
| Return statements | ✅ | type_checking.rs:2297-2314 | Function returns |
| Property assignments | ✅ | type_checking.rs:428-443 | `obj.prop = value` |
| Variable initialization | ✅ | type_checking.rs:1010-1030 | `const x: T = expr` |
| Function arguments | ✅ | Call expression handling | `fn(x: T) {}` |
| Destructuring patterns | ✅ | state.rs:11206-11248 | `{ x }: T = obj` |
| For-of loops | ✅ | state.rs:10912-10973 | `for (x: T of iter)` |
| Array to tuple | ✅ | state.rs + type_checking.rs | `t: Tuple = [a, b]` |
| Object literal excess | ✅ | state.rs | `{ x: 1, y: 2 }` to `{ x: number }` |

### Potential Missing Checks

Based on the analysis, these areas might need enhancement:

1. **Spread operator in object literals**
   ```typescript
   const obj2: { x: number; y: number } = { ...obj1, z: "string" };
   ```
   Current: Should check `z` for type compatibility

2. **Generic type parameter constraints**
   - Ensure type arguments satisfy constraints
   - Check in generic function calls and instantiations

3. **Strict null checks**
   - Ensure null/undefined not assigned to non-nullable types
   - Flag: `strict_null_checks`

4. **Conditional type branches**
   - Both branches should satisfy the result type
   - Check in conditional type evaluation

5. **Assertion expressions**
   - `asserts condition` should validate predicate return type

6. **Type assertions**
   - `as T` should still have some basic validation in strict mode

### Test Coverage

Test file created: `test_ts2322_missing_checks.ts`
- 10 test cases covering common TS2322 scenarios
- Tests assignments, returns, arguments, destructuring
- Tests null/undefined handling
- Tests spread operator

### Recommendations

To add 600+ missing TS2322 errors:

1. **Baseline Measurement**: Run conformance suite to get current pass rate
2. **Gap Analysis**: Identify which tests are failing due to missing TS2322
3. **Targeted Implementation**: Add checks for identified gaps
4. **Validation**: Re-run conformance to measure improvement

### Key Files for Modification

- `src/checker/type_checking.rs` - Main type checking entry points
- `src/checker/state.rs` - Core checker state and helper methods
- `src/checker/type_computation.rs` - Type computation and contextual typing
- `src/solver/compat.rs` - Compatibility layer with unsound rules
- `src/solver/subtype.rs` - Subtype checking engine

### Success Metrics

- Target: 600+ TS2322 errors detected
- Baseline conformance: ~41.5% (from earlier report)
- Target conformance: 60%+

---

**Status**: Infrastructure already exists, needs targeted gap analysis and enhancement.
