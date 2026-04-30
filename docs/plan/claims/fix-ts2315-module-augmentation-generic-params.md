---
branch: fix/ts2315-module-augmentation-generic-params
status: ready
scope: checker (TS2315 / module augmentation)

## Summary

Fix a false-positive TS2315 ("Type 'X' is not generic") when a non-generic
symbol is re-exported and a module augmentation adds a generic version.

## Root Cause

`validate_type_reference_type_arguments` checked only the resolved target
symbol for type parameters. When a non-generic interface is imported from a
module (e.g., `import {Row2} from '.'`), the resolver follows the import alias
to the target symbol, losing the `import_module` info needed to find module
augmentations that add type parameters (`declare module '.' { type Row2<T> = {} }`).

## Fix

- Added `module_augmentation_has_type_params` to `CheckerState` — checks
  module augmentations for a given module specifier and interface name for
  type parameters (type alias, interface, or class).
- In `validate_type_reference_type_arguments`, before emitting TS2315, look up
  the import alias in `file_locals` (which preserves `import_module`), and use
  the new helper to check for augmented type params.

## Files Changed

- `crates/tsz-checker/src/checkers/generic_checker/mod.rs`
- `crates/tsz-checker/src/types/module_augmentation.rs`

## Verification

- Conformance: 7 improvements, +13 net delta (12277 → 12290)
  - `mergeSymbolReexportedTypeAliasInstantiation.ts` now passes
- Unit tests: 2 new tests for `module_augmentation_has_type_params`
- No regressions in checker (3062) or solver (5567) test suites
