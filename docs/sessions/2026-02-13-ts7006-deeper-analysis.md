# TS7006/TS7011 Contextual Typing - Deeper Analysis

**Date**: 2026-02-13
**Status**: Issue Clarified - More Complex Than Initially Assessed

## The Real Problem

We are **TOO STRICT**, not too lenient. We report implicit any errors where TSC successfully infers types from context.

### Test Case: contextuallyTypedParametersWithInitializers1.ts

**TSC Expects**: 2 errors (TS7006 on lines 24, 40)
**TSZ Reports**: 11 errors (TS7006, TS7011, TS7019, TS7010)

### Examples of False Positive Errors

```typescript
// Line 10: TSZ reports TS7011, TSC infers types
const f10 = function ({ foo = 42 }) { return foo };
// TSC: foo is number (from default), return type is number
// TSZ: Return type is 'any' - TS7011 error

// Line 11: TSZ reports TS7011, TSC infers types
const f11 = id1(function ({ foo = 42 }) { return foo });
// TSC: Uses generic constraint to infer types
// TSZ: Return type is 'any' - TS7011 error

// Line 16: TSZ reports TS7011, TSC infers types
const f20 = function (foo = 42) { return foo };
// TSC: foo is number, return type is number
// TSZ: Return type is 'any' - TS7011 error
```

## Root Cause Analysis

### Issue 1: Default Value Type Inference
When a parameter has a default value like `foo = 42`, we should:
1. Infer parameter type from the default value (number)
2. Use inferred parameter type to infer return type
3. Not report TS7006 or TS7011

**Current behavior**: We're not inferring parameter type from default, leading to 'any', which cascades to return type.

### Issue 2: Contextual Typing in Generic Constraints
When passing a function to a generic function with constraints:

```typescript
declare function id1<T>(input: T): T;
const f11 = id1(function ({ foo = 42 }) { return foo });
```

TSC:
1. Infers T from the function expression
2. Uses the default value to infer foo: number
3. Infers return type: number
4. No errors

TSZ:
1. Can't infer types from the function expression
2. Falls back to 'any' for parameters
3. Reports TS7011 for return type

### Issue 3: Destructuring with Default Values
Destructuring patterns like `{ foo = 42 }` should:
1. Check if there's a contextual type for the parameter
2. If yes, use it; if no, infer from default value
3. Correctly type the destructured binding

**Current behavior**: Not handling this case, defaults to 'any'.

## Code Locations

### Parameter Type Inference
```
crates/tsz-checker/src/state_type_analysis.rs
  - compute_type_of_symbol (line 1911)
  - For parameters (around line 2363)

crates/tsz-checker/src/function_type.rs
  - Function expression checking
  - Parameter type extraction
```

### Contextual Type Propagation
```
crates/tsz-checker/src/dispatch.rs
  - Function expression dispatch (around line 200-250)
  - Need to set contextual type before checking

crates/tsz-checker/src/call_checker.rs
  - Generic type inference from call arguments
  - Should propagate constraints to argument checking
```

### Default Value Handling
```
crates/tsz-checker/src/type_computation.rs
  - get_type_of_node for parameters
  - Should check initializer when parameter type is not annotated
```

## Complexity Assessment

This is **significantly more complex** than initially estimated:

1. **Multiple interacting systems**:
   - Parameter type inference
   - Default value type extraction
   - Destructuring binding type inference
   - Contextual type propagation
   - Generic constraint propagation
   - Return type inference from body

2. **Edge cases**:
   - Destructuring with defaults
   - Nested destructuring
   - Rest parameters with inference
   - Generic constraints vs direct contextual types
   - Arrow functions vs function expressions

3. **Risk of regressions**:
   - Changes to contextual typing affect many areas
   - Parameter inference touches core type checking
   - Return type inference is widely used

## Revised Estimate

**Original Estimate**: 4-6 hours (medium difficulty)
**Revised Estimate**: 8-12 hours (high difficulty)

**Why**: This requires coordinated changes across:
- Parameter type checking
- Default value inference
- Contextual type propagation
- Generic constraint handling
- Return type inference

## Recommendation

### Option A: Defer to Dedicated Session
**Reasoning**: This is a substantial piece of work that needs focused time and careful testing.

**Benefits**:
- Can approach systematically
- Time for thorough testing
- Can create comprehensive unit tests first (TDD)

### Option B: Break Into Smaller Pieces
**Reasoning**: Attack one aspect at a time

**Pieces**:
1. **Part 1** (3-4 hours): Default value parameter inference
   - Make `(x = 42) => x` infer x: number
   - Fix TS7006 for simple default cases

2. **Part 2** (3-4 hours): Destructuring parameter inference
   - Make `({ foo = 42 }) => foo` infer foo: number
   - Fix TS7011 for destructured default cases

3. **Part 3** (2-4 hours): Generic constraint propagation
   - Make generic constraints provide contextual types
   - Fix remaining TS7011/TS7006 cases

### Option C: Explore Alternative High-Impact Fix
**Reasoning**: Find a different issue with better ROI

**Candidates**:
- TS2705: Missing async return checking (2 hours, 2-3 tests)
- TS2304: Symbol resolution gaps (3-4 hours, 4+ tests)
- TS2740: Missing property checks (2-3 hours, 5-10 tests)

## Recommendation for This Session

Given time constraints and complexity, **I recommend Option C**: Find a simpler high-impact fix for this session.

The contextual typing issue is important but requires a dedicated focused session with:
- TDD approach (write failing tests first)
- Systematic implementation
- Comprehensive regression testing

## Updated Priority List

1. **TS2740** (Missing property checks) - 2-3 hours, 5-10 tests - **NEW PRIORITY 1**
2. **TS2705** (Async return checking) - 2 hours, 2-3 tests
3. **TS2304** (Symbol resolution) - 3-4 hours, 4+ tests
4. **TS7006/TS7011** (Contextual typing) - 8-12 hours, 10-15 tests - **Requires dedicated session**
5. **Generic inference** (Higher-order functions) - 12-20 hours, 50-100+ tests

## Action Items

For next session focused on contextual typing:
1. Read TSC source for parameter type inference
2. Write comprehensive unit tests for each case
3. Implement part 1 (default values) first
4. Test thoroughly before moving to part 2
5. Leave part 3 for another session if needed

For this session:
- Consider implementing TS2740 missing property checks instead
- Or TS2705 async return checking
- Document findings and prepare for future work
