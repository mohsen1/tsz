# Stack Overflow in Cross-Arena Type Delegation

## Status: Mitigated but not fully resolved

## Problem
When checking files with circular class inheritance across multiple files (e.g., `classExtendsItselfIndirectly3.ts`), the compiler stack overflows. The conformance test still crashes.

## Root Cause
`CheckerState::with_parent_cache` clones ~20+ `FxHashMap` fields from the parent context. In debug builds, each delegation level uses ~50MB+ of stack. Cross-arena delegation for interdependent lib types (Array → ReadonlyArray → IterableIterator → Iterator → ...) creates deeply nested chains.

Three cross-arena delegation points exist:
1. `state_type_analysis.rs` — `delegate_cross_arena_symbol_resolution`
2. `state_type_environment.rs` — `get_type_params_for_symbol`
3. `type_computation_complex.rs` — value declaration resolution

## Current Mitigations
- Thread-local `CROSS_ARENA_DEPTH` counter (max 5) shared across all 3 delegation points (`state.rs`)
- `Box<CheckerState>` for heap allocation of child checker
- Main thread spawned with 64MB stack
- Rayon workers configured with 8MB stacks

## Proper Fix Needed
Refactor `with_parent_cache` to use `Rc<RefCell<FxHashMap>>` shared references for large caches (`symbol_types`, `symbol_instance_types`, `node_types`, `relation_cache`, etc.) instead of cloning. This would reduce per-delegation stack usage from ~50MB to ~1KB.

## Affected Tests
- `classExtendsItselfIndirectly3.ts` — still crashes
- Any multi-file test with circular class inheritance + many files

## Files
- `crates/tsz-checker/src/state.rs` — `CROSS_ARENA_DEPTH` thread-local
- `crates/tsz-checker/src/context.rs` — `with_parent_cache` constructor
- `crates/tsz-cli/src/bin/tsz.rs` — thread stack size configuration
