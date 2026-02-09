# Conformance Session - February 9, 2026 (Part 2)

## Session Overview

**Duration**: ~2 hours
**Branch**: `claude/improve-conformance-tests-Hkdyk`
**Focus**: Typeof narrowing bug fix and conformance test analysis

## Completed Work

### 1. Fixed Typeof Narrowing for Indexed Access Types

**Issue**: When narrowing indexed access types like `T[K]` with typeof guards, tsz incorrectly narrowed to `never` instead of creating an intersection with the Function type.

**Test Case**:
```typescript
export function methodFnLength<T extends {}, K extends keyof T>(obj: T, methodKey: K): number {
    const fn = obj[methodKey];  // Type: T[K]
    if (typeof fn !== 'function') {
        return 0;
    }
    // Here fn should be: T[K] & Function
    return fn.length;  // Was: TS18050, Now: no error ✓
}
```

**Root Cause**: `narrow_to_function` in `/home/user/tsz/crates/tsz-solver/src/narrowing.rs` didn't handle `TypeKey::IndexAccess` types.

**Fix**: Added case to detect indexed access types and create proper intersection:
```rust
} else if index_access_parts(self.db, source_type).is_some() {
    // For indexed access types like T[K], narrow to T[K] & Function
    let function_type = self.function_type();
    self.db.intersection2(source_type, function_type)
} else {
    TypeId::NEVER
}
```

**Files Modified**:
- `crates/tsz-solver/src/narrowing.rs` (+6 lines)
- `crates/tsz-solver/src/tests/narrowing_tests.rs` (+29 lines - new unit test)

**Test Results**:
- ✅ `indexedAccessConstraints.ts`: Expected [TS2322], was [TS2322, TS18050], now [TS2322]
- ✅ All 3,518 solver unit tests pass
- ✅ All 72 control flow unit tests pass

**Commits**:
- `2ea3baa`: Fix typeof narrowing for indexed access types
- `c6359df`: Add unit test for indexed access type narrowing

## Conformance Test Analysis

### Current Pass Rate
- **Overall**: 59.1% (1,252 / 2,117 tests passing)
- **Skipped**: 10,524 tests
- **Crashed**: 1 test (`keyofAndIndexedAccess2.ts`)
- **Timeout**: 0 tests

### Top Error Code Mismatches

| Error Code | Missing | Extra | Category |
|------------|---------|-------|----------|
| **TS2322** | 33 | **85** | Type assignment (false positives) |
| **TS2339** | 27 | **85** | Property access (false positives) |
| **TS2304** | 58 | 15 | Cannot find name |
| **TS2345** | 12 | **56** | Argument type (false positives) |
| **TS1005** | 20 | 51 | Syntax errors |
| **TS2307** | 4 | 36 | Module not found |
| **TS7006** | 1 | **27** | Implicit any (false positives) |
| **TS2315** | 0 | **24** | Not generic (false positives) |
| **TS2769** | 0 | **23** | No overload matches (false positives) |
| **TS1128** | 7 | 22 | Declaration/statement expected |

**Key Observation**: The highest priority issues are **false positives** (extra errors tsz emits that TSC doesn't).

## Investigation: TS2322 False Positives

### Issue Pattern Identified

**Test**: `keyofAndIndexedAccess.ts`
**Pattern**: Literal types not assignable to generic type parameters

```typescript
interface Shape {
    name: string;
    width: number;
    height: number;
    visible: boolean;
}

function getProperty<T, K extends keyof T>(obj: T, key: K): T[K] {
    return obj[key];
}

function f10(shape: Shape) {
    let widthOrHeight = getProperty(shape, cond ? "width" : "height");
    //                                           ^ TS2322: Type '"width"' is not assignable to type 'K'
}
```

**Root Cause**: When calling a generic function with a conditional expression of literal types:
- `cond ? "width" : "height"` has type `"width" | "height"`
- This should be assignable to `K extends keyof Shape`
- tsz is rejecting the individual literal types rather than the union

**Expected**: No error (TSC accepts this)
**Actual**: TS2322 errors for each literal type

**Impact**: 16+ false positives in just this one test file

**Complexity**: High - requires fixing:
1. Type argument inference for generic function calls
2. Conditional expression type computation
3. Literal type to type parameter assignability

### Other False Positive Patterns

1. **instanceof narrowing with generics** (`controlFlowInstanceof.ts`)
   - Expected: No errors
   - Actual: Multiple TS2322, TS2339 errors
   - Issue: Generic Set<T> not being narrowed correctly after instanceof

2. **Symbol type in lib APIs** (`acceptSymbolAsWeakType.ts`)
   - Expected: No errors (with esnext lib)
   - Actual: Wrong type resolution (RTCEncodedVideoFrameType instead of symbol)
   - Issue: Lib.d.ts loading or type resolution problem

## Recommendations for Next Session

### High Priority (Quick Wins)

1. **Conditional Expression Type Computation**
   - File: `crates/tsz-checker/src/type_checking.rs` (conditional expressions)
   - Fix: `cond ? "width" : "height"` should produce `"width" | "height"` not individual checks
   - Impact: Would fix many TS2322 false positives
   - Complexity: Medium

2. **Type Argument Inference Improvements**
   - File: `crates/tsz-solver/src/infer.rs`
   - Fix: Better inference for literal unions to type parameters
   - Impact: Would fix generic function call false positives
   - Complexity: High

3. **instanceof Narrowing for Generics**
   - File: `crates/tsz-checker/src/control_flow_narrowing.rs`
   - Fix: Handle `x instanceof Set` when x is `Set<T> | Set<U>`
   - Impact: Multiple control flow tests
   - Complexity: Medium

### Medium Priority

4. **Missing TS2588 Errors** (3 missing)
   - Error: "Cannot assign to X because it is a constant"
   - Test: `constDeclarations-access*.ts`
   - Impact: Low (missing errors less critical than false positives)

5. **TS2315 False Positives** (24 extra)
   - Error: "Type X is not generic"
   - Likely related to conditional types or mapped types
   - Complexity: Medium

### Low Priority (Complex)

6. **Lib.d.ts Issues**
   - Some tests have wrong type resolutions from lib files
   - May require fixes to lib loading or type resolution
   - Complexity: Very High

## Tools and Techniques Used

### Conformance Testing
```bash
# Run specific slice of tests
./.target/dist-fast/tsz-conformance --offset 3101 --max 3101 --cache-file tsc-cache-full.json --tsz-binary ./.target/release/tsz

# Filter by error code
./.target/dist-fast/tsz-conformance --error-code 2322 --max 200 --cache-file tsc-cache-full.json --tsz-binary ./.target/release/tsz

# Verbose output to see test details
./.target/dist-fast/tsz-conformance --verbose --max 100 --cache-file tsc-cache-full.json --tsz-binary ./.target/release/tsz
```

### Unit Testing
```bash
# Run specific test
cargo test -p tsz-solver test_narrow_by_typeof_indexed_access --lib

# Run all solver tests
cargo test -p tsz-solver --lib

# Run all tests
cargo test --lib
```

### Debugging
```bash
# Test single file with tsz
./.target/release/tsz path/to/test.ts

# Compare with expected errors
cat TypeScript/tests/baselines/reference/test.errors.txt

# Build in release mode
cargo build --release --bin tsz -p tsz-cli
```

## Lessons Learned

1. **Start with Failing Tests**: Look at specific failing tests to understand patterns
2. **Check Baselines**: Always compare tsz output with expected errors in baseline files
3. **Use Unit Tests**: Add unit tests for fixes to prevent regressions
4. **Focus on False Positives**: Extra errors are more critical than missing errors
5. **Isolate Issues**: Create minimal test cases to understand the root cause
6. **Use Visitor Pattern**: The solver's visitor pattern (e.g., `index_access_parts`) is useful for type introspection

## Statistics

- **Tests Analyzed**: ~200
- **Issues Identified**: 5 distinct patterns
- **Fixes Implemented**: 1 (indexed access narrowing)
- **Unit Tests Added**: 1
- **Lines of Code Changed**: 35
- **Pass Rate Improvement**: Minimal (one test fixed, but exposed broader patterns)

## Next Session Priorities

1. Fix conditional expression type computation (biggest impact)
2. Improve type argument inference for generics
3. Add more unit tests for narrowing scenarios
4. Document common false positive patterns

---

**Session End Time**: 2026-02-09 08:15 UTC
**Branch Status**: Clean, all changes committed and pushed
**Next Session Can**: Start immediately with documented priorities
