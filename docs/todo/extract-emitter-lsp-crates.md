# Extract Emitter and LSP into Separate Crates

## Overview

Extract the emitter and LSP implementations from the monolithic `wasm` crate into their already-existing (but empty) workspace crates `tsz-emitter` and `tsz-lsp`, plus apply build configuration optimizations. This enables parallel compilation and smaller recompilation units.

## Context

The `wasm` crate contains ~90% of the codebase. The workspace already has 8 crates (`tsz-common`, `tsz-scanner`, `tsz-parser`, `tsz-binder`, `tsz-solver`, `tsz-checker`, `tsz-emitter`, `tsz-lsp`) but the last two are empty placeholders with correct `Cargo.toml` dependencies. All emitter code lives in `src/emitter/`, `src/transforms/`, and several support modules. All LSP code lives in `src/lsp/`.

## Strategy: Re-export for Minimal Disruption

After moving files to their crates, the main crate's `src/lib.rs` will re-export the modules:
```rust
pub use tsz_emitter::emitter;
pub use tsz_emitter::transforms;
// etc.
```
This means all `crate::emitter::*` imports **in the main crate** continue to work unchanged. Only imports **within the moved files** need updating (from `crate::parser` to `tsz_parser::parser`, etc.).

---

## Phase 1: Extract `tsz-emitter`

### Files to move into `crates/tsz-emitter/src/`

- `src/emitter/` (21 files + `tests/`) -- core Printer
- `src/transforms/` (24 files + `tests/`) -- ES5/ESNext transforms
- `src/emit_context.rs` -- transform state
- `src/source_writer.rs` -- output writer
- `src/transform_context.rs` -- transform directives
- `src/lowering_pass.rs` -- Phase 1 analysis
- `src/printer.rs` -- high-level print API

### Write `crates/tsz-emitter/src/lib.rs`

Declare all modules and re-export the public API:
```rust
pub mod emitter;
pub mod transforms;
pub mod emit_context;
pub mod source_writer;
pub mod transform_context;
pub mod lowering_pass;
pub mod printer;
```

### Import rewrites within moved files

All files currently use `crate::` for intra-crate references. After the move:

- `crate::parser::*` --> `tsz_parser::parser::*` (or `tsz_parser::*` for re-exported types)
- `crate::scanner::*` --> `tsz_scanner::*`
- `crate::common::*` --> `tsz_common::common::*`
- `crate::interner::*` --> `tsz_common::interner::*`
- `crate::source_map::*` --> `tsz_common::source_map::*`
- `crate::syntax::*` --> `tsz_parser::syntax::*`
- `crate::binder::*` --> `tsz_binder::*`
- `crate::solver::*` --> `tsz_solver::*`
- `crate::checker::*` --> `tsz_checker::*`
- `crate::emitter::*`, `crate::transforms::*`, `crate::emit_context::*`, `crate::source_writer::*`, `crate::transform_context::*`, `crate::lowering_pass::*`, `crate::printer::*` --> stay as `crate::*` (same crate now)

### Main crate (`src/lib.rs`) changes

Replace `pub mod emitter;` etc. with re-exports:
```rust
pub use tsz_emitter::emitter;
pub use tsz_emitter::transforms;
pub use tsz_emitter::emit_context;
pub use tsz_emitter::source_writer;
pub use tsz_emitter::transform_context;
pub use tsz_emitter::lowering_pass;
pub use tsz_emitter::printer;
```

All test files in `src/tests/` that import emitter types (e.g., `printer_tests.rs`, `source_writer_tests.rs`, `source_map_tests_*.rs`, `transform_api_tests.rs`) will continue working via these re-exports.

### Cargo.toml

[`crates/tsz-emitter/Cargo.toml`](crates/tsz-emitter/Cargo.toml) already has the right dependencies. May need to add `memchr` if used by any moved file.

---

## Phase 2: Extract `tsz-lsp`

### Files to move into `crates/tsz-lsp/src/`

- `src/lsp/` (33 feature modules + `tests/`)

### Key simplification: position types already in `tsz-common`

[`crates/tsz-common/src/position.rs`](crates/tsz-common/src/position.rs) already has identical `Position`, `Range`, `Location`, `SourceLocation`, `LineMap` types. The LSP crate should:
1. Delete `src/lsp/position.rs` (duplicate)
2. Use `tsz_common::position::*` instead
3. Re-export from `lib.rs` for API compatibility

### Handle `cli::config` dependency

[`src/lsp/project.rs`](src/lsp/project.rs) line 17 imports `crate::cli::config::{load_tsconfig, resolve_compiler_options}` behind `#[cfg(not(target_arch = "wasm32"))]`. Options:
- **Recommended**: Refactor `project.rs` to accept resolved config as a parameter (dependency injection), with the main crate calling `cli::config` functions before passing to Project
- Alternative: Move `load_tsconfig` and `resolve_compiler_options` into `tsz-common` or `tsz-lsp`

### Write `crates/tsz-lsp/src/lib.rs`

Move the contents of `src/lsp/mod.rs` to `lib.rs` with updated imports.

### Import rewrites within moved files

- `crate::binder::*` --> `tsz_binder::*`
- `crate::parser::*` --> `tsz_parser::*`
- `crate::scanner::*` --> `tsz_scanner::*`
- `crate::checker::*` --> `tsz_checker::*`
- `crate::solver::*` --> `tsz_solver::*`
- `crate::comments::*` --> `tsz_common::comments::*`
- `crate::lsp::position::*` --> `tsz_common::position::*`
- `crate::lsp::submodule::*` --> `crate::submodule::*` (same crate now)

### Main crate (`src/lib.rs`) changes

Replace `pub mod lsp;` with:
```rust
pub use tsz_lsp as lsp;
```

External imports like `crate::lsp::position::LineMap` in `src/diagnostics.rs`, `src/source_file.rs`, `src/wasm_api/` will continue working via this re-export. Alternatively, switch them to `tsz_common::position::LineMap` directly.

---

## Phase 3: Build Configuration Optimizations

### Remove test LTO ([`Cargo.toml`](Cargo.toml) lines 248-249)

```toml
# REMOVE - LTO for tests is expensive and provides negligible benefit
[profile.test]
lto = "thin"
```

This will noticeably speed up `cargo test` compilation.

### Add dev profile codegen-units ([`.cargo/config.toml`](.cargo/config.toml))

The `[profile.dev]` section currently only sets `opt-level = 0`. Add:
```toml
[profile.dev]
opt-level = 0
incremental = true
codegen-units = 256
```

This matches the orchestrator profile and enables maximum parallel code generation during development.

### Consider linker optimization

If building on macOS (likely given the paths), the default linker is already reasonably fast. On Linux, using `mold` or `lld` would be a significant improvement. Add to `.cargo/config.toml`:
```toml
[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=mold"]
```

---

## Expected Build Time Impact

- **Crate extraction**: When editing emitter code, only `tsz-emitter` + `wasm` recompile (not scanner, parser, binder, solver, checker). Same for LSP. Both crates compile in parallel with each other.
- **Removing test LTO**: ~10-20% faster test compilation
- **Codegen units 256**: Faster incremental dev builds via more parallel codegen
- **Overall**: Incremental rebuilds should be meaningfully faster, especially for emitter/LSP-focused changes

## Implementation Checklist

- [ ] Move emitter/, transforms/, emit_context, source_writer, transform_context, lowering_pass, printer to crates/tsz-emitter/src/
- [ ] Write crates/tsz-emitter/src/lib.rs with module declarations and re-exports
- [ ] Update all crate:: imports within moved emitter files to point to workspace crate dependencies
- [ ] Update main crate src/lib.rs to re-export from tsz_emitter instead of pub mod
- [ ] Verify tsz-emitter compiles and main crate tests pass
- [ ] Move src/lsp/ contents to crates/tsz-lsp/src/, delete duplicate position.rs
- [ ] Write crates/tsz-lsp/src/lib.rs from lsp/mod.rs with updated imports
- [ ] Update all crate:: imports within moved LSP files to point to workspace crate dependencies
- [ ] Refactor project.rs cli::config dependency via dependency injection
- [ ] Update main crate src/lib.rs to re-export from tsz_lsp
- [ ] Verify tsz-lsp compiles and main crate tests pass
- [ ] Remove test LTO, add codegen-units=256 to dev profile, consider linker optimization
