# Conformance Test Bugs Identified

## Session: 2026-02-11

**Slice 1 Status:** 2115/3140 tests passing (67.4%)

### High-Impact Issues

#### 1. Type Alias Conditional Resolution Bug
**Impact:** ~84 TS2322 false positives

**Minimal Reproduction:**
```typescript
type Test = true extends true ? "y" : "n"
let value: Test = "y"  // TS2322: Type 'string' is not assignable to type 'Test'
```

**Root Cause:**
- Conditional type evaluates correctly (verified with tracing)
- The conditional correctly returns the true branch ("y")
- BUT: Type alias doesn't resolve properly when used as a type annotation
- Workaround: Using conditional inline works: `let x: (true extends true ? "y" : "n") = "y"`

**Location:** Likely in `crates/tsz-checker/src/state_type_resolution.rs` or type alias resolution

---

#### 2. Type Predicates Not Applied in Array.find
**Impact:** Multiple TS2322 false positives

**Minimal Reproduction:**
```typescript
function isNumber(x: any): x is number {
  return typeof x === "number";
}
const arr = ["string", false, 0];
const result: number | undefined = arr.find(isNumber);
// Error: Type 'boolean | number | string | AbstractRange' is not assignable to 'undefined | number'
```

**Expected:** `result` should be `number | undefined` (narrowed by type predicate)
**Actual:** `result` is the full union type

**Location:** `crates/tsz-checker/src/call_checker.rs` - type predicate handling in call expressions

---

#### 3. Parser TS1005 False Positives
**Impact:** 53 tests incorrectly emit "{' expected"

**Examples:**
- Destructuring with defaults
- JSX expressions
- Complex arrow functions

**Location:** Parser recovery logic

---

### Missing Error Codes (High Priority)

From analysis of 1025 failing tests:

- **TS2792**: 17 tests (module resolution errors)
- **TS2323**: 9 tests
- **TS2301**: 8 tests
- **TS1191**: 8 tests
- **TS7005**: 7 tests

---

### Co-occurring Error Codes

Tests that need multiple fixes:
- TS2300 + TS2440: 3 tests
- TS2339 + TS7023: 2 tests
- TS2503 + TS2671: 2 tests

---

### Quick Wins

**Single Missing Error Code:**
- TS2322 (partial): 24 tests would pass
- TS2304 (partial): 11 tests would pass
- TS2339 (partial): 10 tests would pass
- TS2345 (partial): 8 tests would pass
- TS2353 (partial): 7 tests would pass

---

## Investigation Notes

### Type Alias Conditional Bug - Detailed Tracing

Added tracing to `crates/tsz-solver/src/evaluate_rules/conditional.rs`:
- Line 252: Subtype check returns `true` ✓
- True branch is selected ✓
- Conditional evaluates to TypeId for literal "y" ✓

The bug is NOT in conditional evaluation - it's in how type aliases are resolved when used as type annotations.

Relevant code:
- `crates/tsz-checker/src/state_type_analysis.rs:1945` - TYPE_ALIAS handling
- `crates/tsz-checker/src/state_type_resolution.rs:719` - `type_reference_symbol_type`

---

## Recommended Next Steps

1. **Quick win:** Implement missing error code TS2792 (17 test impact)
2. **Medium complexity:** Fix type predicate narrowing in call expressions
3. **Complex:** Fix type alias conditional resolution bug (requires checker refactoring)

---

## Test Commands

```bash
# Run slice 1
./scripts/conformance.sh run --offset 0 --max 3146

# Analyze failures
./scripts/conformance.sh analyze --offset 0 --max 3146

# Test specific issue
./scripts/conformance.sh run --filter "arrayFind" --verbose

# Check unit tests
cargo nextest run
```
