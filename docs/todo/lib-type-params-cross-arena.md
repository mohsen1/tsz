# Cross-Arena Type Parameter Resolution for Lib Types

## Status: Partially fixed (TS2315 suppressed, root cause remains)

## Problem
`get_type_params_for_symbol` fails for symbols defined in lib file arenas (e.g., `Awaited<T>`, `Record<K,V>`, `Partial<T>`). The function creates a child `CheckerState` for cross-arena delegation, but the delegation can fail or return empty results due to the cross-arena depth guard.

## Current Fix (generic_checker.rs)
When `get_type_params_for_symbol` returns empty, we check the declaration AST directly via `symbol_declaration_has_type_parameters()` to avoid false TS2315 "Type is not generic" errors. This suppresses the diagnostic but doesn't actually resolve the type parameters — generic constraint checking (TS2344) is skipped for these types.

## Root Cause
The `get_type_params_for_symbol` delegation in `state_type_environment.rs` creates a child `CheckerState` that processes the type alias/interface/class declaration in the lib file's arena. If the cross-arena depth guard blocks the delegation, type parameters are not resolved.

## Proper Fix
1. Pre-compute type parameters for all lib symbols during lib context initialization (before file checking)
2. Store in a shared cache that all file checkers can access without delegation
3. This avoids cross-arena delegation entirely for the common case of lib type parameters

## Impact
- TS2315 false positives: FIXED (suppressed via AST check)
- TS2344 constraint validation for lib generics: still missing
- Generic instantiation for lib types: may use incorrect type argument count

## Files
- `crates/tsz-checker/src/generic_checker.rs` — `symbol_declaration_has_type_parameters`
- `crates/tsz-checker/src/state_type_environment.rs` — `get_type_params_for_symbol` delegation
