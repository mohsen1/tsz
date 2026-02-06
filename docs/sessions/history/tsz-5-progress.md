# Session tsz-5 Progress Summary

**Committed:** a6331b03f, 7f742cf42 (pushed to origin)

## Achievements
- Fixed string enum to string assignability bug
- Fixed number to enum MEMBER assignability bug
- All enum tests now passing (185/185 in checker_state_tests)
- Overall: 8254 passed, 46 failed, 158 ignored (up from 8248)

## Key Fixes

### 1. String Enum -> String Assignability
**Problem:** String enums were incorrectly rejected when assigned to string type.

**Root Cause:** Lines 1329-1337 in compat.rs explicitly returned `Some(false)` for all string enum -> string assignments.

**Solution:** Removed the early check and let Case 3 handle it by falling through to structural checking.

**Tests Fixed:**
- test_string_enum_not_assignable_to_string (was expecting error, should not)
- test_string_enum_to_string (renamed from test_string_enum_not_to_string)

### 2. Number -> Enum MEMBER Assignability  
**Problem:** `const x: E.A = 1` was incorrectly allowed when it should be rejected.

**Root Cause:** Early check at lines 1312-1330 returned `Some(true)` for `number -> numeric enum` without distinguishing between enum TYPE and enum MEMBER.

**Solution:** Removed the early check. The logic is now correctly handled in Case 2 which checks `is_enum_type(target)` to distinguish between TYPE and MEMBER.

**Tests Fixed:**
- test_number_to_numeric_enum_member

## Files Modified
- `src/solver/compat.rs`: Removed incorrect early checks, simplified Case 3
- `src/checker/tests/enum_nominality_tests.rs`: Fixed test expectations

## Note
- test_number_string_union_minus_emits_ts2362 was already failing before this commit (unrelated pre-existing issue)
