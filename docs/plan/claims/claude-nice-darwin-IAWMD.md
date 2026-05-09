# fix(binder): apply base_offset in SymbolArena::reserve_symbol_ids

- **Date**: 2026-05-09
- **Branch**: `claude/nice-darwin-IAWMD`
- **PR**: TBD
- **Status**: claim
- **Workstream**: bug fix (#4785)

## Intent

`SymbolArena::reserve_symbol_ids` constructed placeholder `SymbolId`s as
`SymbolId(index)` directly, ignoring `base_offset`. `alloc` and `alloc_from`
both apply `base_offset + index`, and `get`/`get_mut` reverse the offset, so
arenas created with `new_with_base(...)` (notably the checker's
`CHECKER_SYMBOL_BASE = 0x1000_0000`) ended up with placeholder entries whose
stored IDs collided with the binder ID range and could not be looked up
through the arena's own `get`/`get_mut`.

This claim aligns `reserve_symbol_ids` with the rest of the arena API and
adds regression tests covering the default arena, the offset-aware
reservation, and the alloc-after-reserve path.

## Files Touched

- `crates/tsz-binder/src/symbols.rs` — apply `base_offset.checked_add(...)`
  to placeholder IDs; document overflow panic.
- `crates/tsz-binder/tests/symbols_tests.rs` — four new unit tests in
  `symbol_arena_tests`.

## Verification

- `cargo test -p tsz-binder` (470 unit/integration tests pass).
- `cargo build -p tsz-checker -p tsz-solver` (dependent crates compile).
