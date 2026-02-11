# Conformance Test Slice 4 Analysis (Tests 9423-12563)

**Current Status:** 1668/3137 passed (53.2%)

## High-Priority Bugs Identified

### 1. Interface/Namespace Scoping Bug (CRITICAL)
**Impact:** Causes TS2314, TS2420 false positives and wrong type resolution

**Symptom:**
When both exist:
- `interface A { y: string; }` (top-level)
- `namespace M { interface A<T> { z: T; } }` (in namespace)

Resolving "A" in `class D implements A` incorrectly finds `M.A<T>` instead of top-level `A`.

**Test Case:** `/Users/mohsen/code/tsz-4/test-minimal-bug.ts`

```typescript
interface A { y: string; }
namespace M { interface A<T> { z: T; } }
class D implements A { y: string; }
// ERROR: TS2314 "Generic type 'A' requires 1 type argument(s)"
// ERROR: TS2420 "Missing members: 'z'" (using M.A shape, not top-level A!)
```

**Root Cause:** Symbol resolution in heritage clauses doesn't properly scope lookups. When resolving "A", it finds both symbols and incorrectly uses the generic one from namespace M.

**Files Involved:**
- `crates/tsz-checker/src/state_type_resolution.rs` - type reference resolution
- `crates/tsz-checker/src/state_checking.rs` - heritage clause checking
- Heritage clause symbol resolution path

**Fix Complexity:** HIGH - requires understanding full heritage clause resolution

---

### 2. Namespace Merging with Dotted Syntax
**Impact:** TS2403 false positives in 13 tests

**Symptom:**
```typescript
namespace X.Y.Z { export interface Line { ... } }
namespace X { export namespace Y.Z { export interface Line { ... } } }
```

These should merge but we emit TS2403 "Subsequent variable declarations must have the same type".

**Root Cause:** Dotted namespace syntax (`X.Y.Z`) merging not working correctly.

---

## Error Code Statistics

### Most Impactful Fixes:

**False Positives (we emit when shouldn't):**
- TS2339: 142 tests (property doesn't exist)
- TS1005: 100 tests (parse errors)
- TS2344: 90 tests (generic type errors)
- TS2322: 88 tests (type assignment)
- TS2345: 85 tests (argument type)

**Missing Errors (tsc emits, we don't):**
- TS2304: 141 tests (cannot find name)
- TS2322: 112 tests (type assignment)
- TS6053: 103 tests (file not module)
- TS2307: 89 tests (cannot find module)
- TS2339: 67 tests (property doesn't exist)

**Quick Wins (need just 1 error code):**
- TS2322: 36 tests would pass
- TS2339: 21 tests would pass
- TS2304: 16 tests would pass
- TS2300: 10 tests (duplicate identifier)
- TS2411: 9 tests (property incompatible with index)

**Not Implemented (highest impact):**
- TS1479: 23 tests (CommonJS/ESM conflict)
- TS7026: 17 tests
- TS1100: 12 tests
- TS2630: 12 tests
- TS2823: 11 tests

---

## Recommended Next Steps

### Short Term (High ROI):
1. **Fix TS2339 false positives** - 142 tests affected
   - Investigate why we over-emit property access errors
   - Look for pattern in false positive cases

2. **Fix TS2304 missing cases** - 141 missing + 16 quick wins
   - We emit TS2304 sometimes but miss it in many cases
   - Find where we're not checking for undefined names

3. **Fix TS2322 missing cases** - 112 missing + 36 quick wins
   - Type assignment checks not comprehensive enough
   - Highest quick-win potential

### Medium Term:
4. **Implement TS2300 (Duplicate identifier)** - 10 quick wins
   - Not a huge implementation, clear benefit

5. **Fix namespace/interface scoping bug** - Foundational issue
   - Complex but affects many interface merging tests
   - Blocks progress on TS2420, TS2430, etc.

### Long Term:
6. **Module system error codes** (TS1479, TS6053, TS2307)
   - Requires module resolution work
   - High test count but complex implementation

---

## Test Files Created for Debugging

- `test-interface-merge.ts` - Simple merged interfaces (PASSES)
- `test-interface-merge2.ts` - With namespace (FAILS - shows bug)
- `test-interface-scope.ts` - Scoping test (FAILS)
- `test-minimal-bug.ts` - Minimal reproduction (FAILS)
- `test-namespace-only.ts` - Just namespace, no conflict (PASSES)
- `test-simple.ts` - Single interface (PASSES)

---

## Unit Test Status
✅ All 332 checker unit tests passing

---

## Analysis Commands Used

```bash
# Full slice run
./scripts/conformance.sh run --offset 9423 --max 3140

# Detailed analysis
./scripts/conformance.sh analyze --offset 9423 --max 3140 --top 30

# Category-specific
./scripts/conformance.sh analyze --offset 9423 --max 3140 --category false-positive
./scripts/conformance.sh analyze --offset 9423 --max 3140 --category close
```
