# Session tsz-5 Final Summary

**Started:** 2026-02-06
**Ended:** 2026-02-06
**Commits:** a6331b03f, 7f742cf42, bcb4b5e0f, 81d76ff33 (pushed to origin)

## Completed Tasks

### Task #17: Fix enum type resolution and arithmetic ✅
- **Problem:** String enums incorrectly rejected when assigned to string
- **Problem:** Number incorrectly allowed when assigned to enum members
- **Solution:** Removed incorrect early checks, moved logic to Case 2/3
- **Result:** All 185 enum tests passing

### Task #18: Fix index access type resolution ✅
- **Status:** Both tests passing (must have been fixed previously)

### Task #20: Property Access on Unions ✅
- **Problem:** Union property access in fallback section instead of Visitor Pattern
- **Location:** `src/solver/operations_property.rs` lines 1136-1280
- **Solution:**
  - Added `visit_union_impl` helper method
  - Added `visit_union` to `TypeVisitor for &PropertyAccessEvaluator`
  - Added `TypeKey::Union` case to visitor match block in `resolve_property_access_inner`
  - Removed old Union handling from fallback section
- **Edge Cases Handled:**
  - any/error fast-paths
  - Filtering UNKNOWN members
  - Partitioning nullable/non-nullable members
  - PropertyNotFound if any member missing property
  - PossiblyNullOrUndefined if nullable members present
  - Contagious `from_index_signature` flag propagation
  - Union-level index signatures
- **Result:** +2 tests passing (8255 -> 8257)
- **Fixed Tests:**
  - `test_checker_property_access_union_type` - fixed to use `declare` to prevent CFA narrowing
  - `test_number_string_union_minus_emits_ts2362` - fixed to use `declare` to prevent CFA narrowing

## Session Investigation

### Blocked: BCT Tests
- **Issue:** `lib.es5.d.ts` from TypeScript repo not available
- **Impact:** Array methods like `push()` not available in tests
- **Files:** `TypeScript/node_modules/typescript/lib/lib.es5.d.ts` doesn't exist

### Indexed Access Tests
- **Issue:** `C["foo"]` resolves to literal `3` instead of widened `number`
- **Location:** `src/checker/type_computation.rs` (per Gemini)
- **Function:** `get_type_of_element_access` needs literal widening

### CFA Narrowing in Tests
- **Discovery:** TypeScript's Control Flow Analysis narrows union types on initialization
- **Example:** `const x: U = { a: 1 }` narrows `x` to the specific branch matching the initializer
- **Fix:** Use `declare const x: U` to prevent narrowing in tests where full union type is needed
- **Pre-existing failures:** `test_abstract_mixin_intersection_ts2339` (unrelated to this work)

## Next Session Recommendations

1. **Task #21: Readonly/Assignment (TS2540)**
   - 4 failing tests
   - Implement Judge vs. Lawyer architecture
   - File: `src/solver/lawyer.rs`

2. **Fix solver enum/instantiate tests**
   - 4 tests, don't depend on lib files
   - Feature completion for enums

3. **Fix literal widening in indexed access** (2 tests)
   - File: `src/checker/type_computation.rs`
   - Apply widening to `C["foo"]` style access

## Test Results
- **Before this session:** 8248 passed, 39 failed
- **After Task #17-18:** 8255 passed, 45 failed, 158 ignored
- **After Task #20:** 8257 passed, 43 failed, 158 ignored
- **Total Progress:** +9 tests passing (8248 -> 8257)
