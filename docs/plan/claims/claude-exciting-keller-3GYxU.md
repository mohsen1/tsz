# Claim: Fix variadic-rest tuple elaboration duplicate errors

**Branch**: `claude/exciting-keller-3GYxU`  
**Date**: 2026-04-29 19:40:43 UTC  
**Test**: `TypeScript/tests/cases/conformance/types/tuple/variadicTuples2.ts`

## Problem

Variadic-rest tuples with trailing fixed elements (e.g., `[number, ...string[], number]`) were
emitting spurious extra element-level TS2322 errors when assigning array literals. Specifically:

1. `try_elaborate_array_literal_elements` used `elaboration_tuple_element_type_at` to map source
   element index 2 to the trailing fixed element of `[number, ...string[], number]`, causing a
   wrong element-level error for elements in the variadic or trailing section.

2. `try_elaborate_array_literal_mismatch_from_failure_reason` similarly reported element-level
   errors for `TupleElementTypeMismatch` failures at indices ≥ leading_fixed_count.

## Fix

- **`query_boundaries/common.rs`**: Added `tuple_leading_fixed_count_before_trailing` helper that
  returns the number of leading fixed elements before a rest element when there are also trailing
  fixed elements (returns `None` if no such pattern). Used to gate element-level elaboration.

- **`error_reporter/call_errors/elaboration.rs`**: Added `max_elaborate_index` in
  `try_elaborate_array_literal_elements` to stop the element loop at `n_leading_fixed` for
  variadic-rest tuples with trailing elements, preventing wrong trailing-element elaboration.

- **`error_reporter/call_errors/elaboration_array_mismatch.rs`**: Added guard in
  `try_elaborate_array_literal_mismatch_from_failure_reason` to skip element-level reporting
  when failure index ≥ n_leading_fixed for variadic-rest tuples with trailing elements.

- **`error_reporter/core_formatting.rs`**: Added `ty_is_raw_tuple` guard in
  `authoritative_assignability_def_name` to prevent `find_def_for_type` from returning alias
  names for raw tuple TypeIds (defensive fix).

## Unit Tests

New test file: `crates/tsz-checker/tests/variadic_tuple_elaboration_tests.rs`

- Leading mismatch → exactly 1 TS2322 (not duplicated at trailing section)
- Trailing mismatch → exactly 1 TS2322 (no element-level for trailing)
- Variadic section mismatch → exactly 1 TS2322
- Valid assignment → no errors
- Plain variadic (no trailing) → element-level errors still work
- Trailing-only variadic tuple → single error

## Remaining issues (not fixed in this PR)

- **TS2322 V03/V01 alias names**: `declare let tt2: [number, ...string[], number]` shows
  "V03" in error messages instead of the structural form. Root cause: `TypeFormatter` uses
  `find_def_for_type` for `TypeData::Tuple` types, which maps interned TypeIds to alias names
  even for structural annotations. Fix requires care to avoid breaking legitimate alias display.

- **TS2345 call argument element-level errors**: `ft2(0, 'abc', 1, 'def')` emits element-level
  TS2345 errors instead of tuple-level. The call argument elaboration path is separate from the
  assignment path fixed here.

- **Inference issues**: `fn2([1])` shows wrong inferred type.
