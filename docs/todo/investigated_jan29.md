# Investigated Issues (Jan 29)

## TS7010: Function implicitly has 'any' return type

**Status**: NEW - High priority issue

**Issue Count**: 110x EXTRA errors (we emit when TypeScript doesn't)

**Description**: "Function '{0}', which lacks return-type annotation, implicitly has an 'any' return type."

**Common Cause**:
1. **Recursive functions**: Most common cause - function calls itself, type depends on uncomputed result
   ```typescript
   // TypeScript infers correctly, we emit TS7010
   function factorial(n: number) {
       if (n <= 1) return 1;
       return n * factorial(n - 1);
   }
   ```

2. **Complex control flow**: Inference engine fails to resolve type

**Root Cause**:
- Inference engine in `src/solver/infer.rs` and `src/checker/function_type.rs` is less capable than tsc
- Fails to infer return types in scenarios where tsc succeeds
- Falls back to `any` and emits TS7010 incorrectly

**Priority**: **VERY HIGH** (110x extra errors - much higher impact than other issues)

**Solution Approach**:
1. Investigate recursive function type inference
2. Compare with tsc's inference algorithm
3. Improve type inference in function_type.rs
4. Consider adding explicit type annotation detection for recursive calls

**Files Involved**:
- src/checker/function_type.rs - Return type inference
- src/solver/infer.rs - Type inference engine

---

## TS2507: Type not a constructor function type

**Status**: Investigated, identified root cause

**Issue Count**: 5x extra errors in 500 test sample (73rd most frequent)

**Root Cause**: Symbol collision issue when identifier names overlap between type aliases and variables

**Example**:
```typescript
type Both = I1 & I2;  // Type alias
declare const both: Both;  // Variable with same name
class C extends both {}  // ERROR: Type 'both' is not a constructor function type
```

**Analysis**:
1. The `is_constructor_type` function (type_checking.rs:1679-1680) correctly handles intersection types
2. The issue is in symbol resolution: when resolving "both", the binder finds BOTH "Both" (type alias) and "both" (variable)
3. During type computation, the variable symbol "both" gets cached as ERROR type
4. This happens because type resolution for the variable annotation `both: Both` resolves the type alias "Both" first
5. The ERROR is then cached and reused when checking the heritage clause

**Solution Approach**:
- The symbol resolution needs to handle name collisions better
- Variable symbols and type symbols with similar names should not interfere
- May need to improve caching strategy to avoid pre-caching ERROR for valid symbols

**Files Involved**:
- src/checker/state.rs - `get_type_of_symbol`, `compute_type_of_symbol`
- src/checker/type_checking.rs - `is_constructor_type`
- src/binder/state.rs - Symbol creation and name binding

**Priority**: Low (only 5x errors in 500 tests)

---

## TS2705: Async function must return Promise

**Status**: Partially investigated

**Issue Count**: 73x missing errors in 500 test sample (#1 most frequent missing error)

**Description**: Should emit when an async function has an explicit non-Promise return type (ES5/ES3 target only)

**Example**:
```typescript
// @target: ES5
async function foo(): string {  // Should emit TS2705
    return "hello";
}
```

**Analysis**:
1. TS2705 check exists in function_type.rs:325-344
2. The check does NOT consider the compiler target (ES5 vs ES6+)
3. TypeScript only emits TS2705 when target is ES5 or ES3
4. Our code emits for all targets (or may have other issues preventing emission)

**Investigation Needed**:
- Check if target is being correctly passed to the checker
- Verify TS2705 is only emitted for ES5/ES3 targets
- Debug why the check at function_type.rs:335 is not triggering

**Files Involved**:
- src/checker/function_type.rs:325-344 - TS2705 check
- src/checker/promise_checker.rs:70-84 - `is_promise_type` function
- src/compiler_options.rs - Target settings

**Priority**: High (73x missing errors, #1 priority)

---

## Conformance Test Results (500 tests, Jan 29)

**Pass Rate**: 40.2% (201/500)

**Top Missing Errors** (we should emit but don't):
- TS2705: 73x - Async function must return Promise
- TS2300: 67x - Duplicate identifier
- TS2584: 47x - Cannot find name
- TS2804: 32x - Type literal property missing
- TS17009: 29x - JSX element attributes
- TS2446: 28x - Cannot override access modifier
- TS1109: 24x - Expected expression

**Top Extra Errors** (we emit but shouldn't):
- TS2339: 106x - Property does not exist (architectural gap)
- TS2445: 26x - Type alias is circular
- TS2507: 24x - Type not a constructor
- TS2307: 23x - Cannot find module (improved from 30x with recent fixes)
- TS2322: 21x - Type not assignable
- TS2349: 20x - Cannot invoke type
