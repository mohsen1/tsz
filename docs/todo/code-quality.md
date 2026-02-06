# Code Quality & Organization

Tasks to improve codebase organization, build times, and developer experience.

## Cargo Workspaces

**Priority**: Medium → **IN PROGRESS**
**Impact**: Build times, test isolation, CI parallelism

Currently the entire compiler is a single crate (`wasm`) with ~620K lines of Rust. Any change to any module recompiles the entire crate for tests (~20s incremental, ~107s clean).

### Investigation findings

- **Clean test build**: ~107s
- **Incremental rebuild** (any file touched): ~20s
- **No-op**: <1s
- **Single test binary**: all 5000+ tests compile into one binary

### Implemented workspace structure

```
tsz/
├── Cargo.toml              (workspace root)
├── crates/
│   ├── tsz-common/         (interner, common types, span, limits, position, source_map, comments)
│   ├── tsz-scanner/        (tokenizer, SyntaxKind, ScannerState)
│   ├── tsz-parser/         (AST, NodeArena, ParserState, syntax utils)
│   ├── tsz-binder/         (symbol binding, scopes, flow nodes)
│   ├── tsz-solver/         (type system: interning, subtyping, inference, narrowing)
│   ├── tsz-checker/        (type checking, diagnostics, enums)
│   ├── tsz-emitter/        (JS code generation, printer, transforms)
│   └── tsz-lsp/            (language server protocol features)
├── src/                    (root 'wasm' crate: CLI, WASM bindings, bins)
├── conformance-rust/       (existing conformance runner)
└── benches/                (benchmarks)
```

### Dependency Graph (Bottom-Up)

```
Layer 0: tsz-common (interner, common, span, limits, position, source_map, comments)
    ↑
Layer 1: tsz-scanner (scanner/)
    ↑
Layer 2: tsz-parser (parser/, syntax/)
    ↑
Layer 3: tsz-binder (binder/, lib_loader, imports, exports)
    ↑
Layer 4: tsz-solver (solver/) + tsz-checker (checker/, enums/)
    ↑
Layer 5: tsz-emitter (emitter/, transforms/, declaration_emitter/, emit_context, etc.)
    ↑
Layer 6: tsz-lsp (lsp/)
    ↑
Root: wasm (cli/, wasm_api/, wasm.rs, bin/, parallel.rs)
```

### Benefits

- **Faster incremental builds**: Changing `tsz-scanner` only recompiles scanner + dependents, not solver/checker
- **Test isolation**: `cargo test -p tsz-solver` runs only solver tests (~130K lines) without compiling LSP/CLI/WASM
- **CI parallelism**: Each crate can be tested independently and in parallel
- **Clearer dependency graph**: Enforces module boundaries at the crate level (e.g., solver cannot accidentally import LSP code)
- **Better IDE experience**: rust-analyzer indexes smaller crates faster

### Implementation Status

- [x] Workspace skeleton created (root Cargo.toml, crate directories, Cargo.toml files)
- [x] Shared workspace dependencies and lints configured
- [x] tsz-common extracted (interner, common, span, limits, position, source_map, comments)
- [ ] tsz-scanner extracted
- [ ] tsz-parser extracted
- [ ] tsz-binder extracted
- [ ] tsz-solver extracted
- [ ] tsz-checker extracted
- [ ] tsz-emitter extracted
- [ ] tsz-lsp extracted
- [ ] All tests passing with full extraction

### Implementation approach

Using **re-exports** for backwards compatibility:
- Root `lib.rs` uses `pub use tsz_common::interner;` instead of `pub mod interner;`
- Existing `crate::interner::...` imports in root crate still work via re-export
- New sub-crates import from `tsz_common::interner` directly
- Original files removed from `src/` after moving to crate

### Key Cross-Dependency Issues

1. **Position types**: `Position`, `Range`, `LineMap` now in tsz-common, duplicated in `lsp/position.rs` until lsp is extracted
2. **diagnostics.rs** uses `source_file::SourceFile` and `lsp::position::Range` — both will go in tsz-common
3. **enums/** depends on both checker and solver — goes in tsz-checker
4. **declaration_emitter/** depends on checker and solver — stays with tsz-emitter with those as deps

---

## Import Organization

**Priority**: Low
**Impact**: Code readability, consistency

### Current state

- No `rustfmt.toml` in the repo
- Imports are manually organized, inconsistent across files
- Some files have `use crate::` scattered mid-file or in impl blocks

### Options

1. **Nightly rustfmt** (recommended): Add `rustfmt.toml` with:
   ```toml
   unstable_features = true
   group_imports = "StdExternalCrate"
   imports_granularity = "Crate"
   ```
   Run with `cargo +nightly fmt`. Groups imports into std / external / crate sections automatically.

2. **rust-analyzer organize imports on save**: Add to VS Code settings:
   ```json
   {
     "editor.codeActionsOnSave": {
       "source.organizeImports": "explicit"
     }
   }
   ```
   Removes unused imports and sorts, but doesn't relocate scattered `use` statements.

3. **Stable rustfmt only**: Just use `cargo fmt` -- sorts alphabetically within existing groups but doesn't regroup.

### Note

`group_imports` and `imports_granularity` are still **nightly-only** as of Feb 2026. Known blockers: non-idempotent formatting bugs when combined together.

---

## Visibility Tightening (`pub` -> `pub(crate)`)

**Priority**: Medium
**Impact**: Encapsulation, API surface, catches dead code
**Automatable**: Partially (tooling-assisted)

### Current state

- **1,319** `pub fn/struct/enum` items across `src/`
- Only **77** `pub(crate)` items
- Ratio is ~17:1 -- most things are `pub` by default even when only used within the crate

### Action

- Run `cargo +nightly fix --edition` or use rust-analyzer's "restrict visibility" code action
- Start with non-test, non-`lib.rs` files: tighten `pub` to `pub(crate)` wherever an item isn't part of the WASM/public API
- This also surfaces truly dead code that clippy can't catch when items are `pub`

### Effort: Low-Medium (can be done module-by-module)

---

## Reduce `.to_string()` Allocations

**Priority**: Medium
**Impact**: Performance, memory allocations
**Automatable**: Partially (grep + manual review)

### Current state

- **~6,000** instances of `"literal".to_string()` across the codebase (most in tests, but ~372 in non-test code)
- Hot paths like `solver/infer.rs` have `.to_string()` calls that allocate on every invocation
- Many could use `&str`, `Cow<str>`, or `&'static str` instead

### Action

1. Audit non-test `.to_string()` calls in solver/checker/binder hot paths
2. Replace string-literal `.to_string()` with `&'static str` or `Cow<'static, str>` where the type allows it
3. For error messages, consider an interned diagnostic message table instead of per-call `format!()`

---

## Test File Hygiene

**Priority**: Medium
**Impact**: Compile times, correctness
**Automatable**: Yes (scripted)

### Missing `#[cfg(test)]` guards

**114 test files** under `src/*/tests/*.rs` do not contain their own `#[cfg(test)]` module guard. They rely on being included via `#[path = "tests/..."]` in parent modules under `#[cfg(test)]`, but this is fragile and hard to verify.

Affected areas:
- `src/checker/tests/` -- 20+ files
- `src/solver/tests/` -- 40+ files
- `src/lsp/tests/`, `src/cli/tests/`, `src/transforms/tests/`

### Action

Add `#[cfg(test)]` at the top of each test file, or ensure the parent `mod.rs` wraps them properly. A script can do this:
```bash
for f in $(find src -path "*/tests/*.rs" -not -name "mod.rs"); do
  if ! grep -q '#\[cfg(test)\]' "$f"; then
    # Add guard at top
  fi
done
```

### Oversized test files

Several test files are enormous and would benefit from splitting:

| File | Lines | Tests |
|------|-------|-------|
| `solver/tests/evaluate_tests.rs` | 47,138 | 988 |
| `transforms/tests/class_es5_tests.rs` | 48,974 | 549 |
| `tests/checker_state_tests.rs` | 33,470 | 652 |
| `solver/tests/subtype_tests.rs` | 30,908 | 794 |
| `tests/source_map_tests_{1-4}.rs` | ~64,000 | N/A |
| `solver/tests/infer_tests.rs` | 15,564 | 514 |

These slow down rust-analyzer and make merge conflicts more likely. Split by category/feature.

---

## `#[allow(dead_code)]` Audit

**Priority**: Low
**Impact**: Code cleanliness, catches unused code
**Automatable**: Yes (grep + remove + compile)

### Current state

~40 `#[allow(dead_code)]` annotations scattered across the codebase, concentrated in:
- `src/solver/contextual.rs` (7)
- `src/checker/control_flow.rs` (5)
- `src/module_resolver.rs` (4)
- `src/declaration_emitter/mod.rs` (4)

### Action

Remove each `#[allow(dead_code)]` one at a time and compile. If the code is truly dead, delete it. If it's needed for a future feature, add a comment explaining why.

---

## `unsafe` Code Audit

**Priority**: High
**Impact**: Safety, correctness
**Automatable**: Partially (grep + manual review)

### Current state

12 files contain `unsafe` blocks (excluding tests):
- `src/solver/sound.rs` (2)
- `src/solver/operations_property.rs` (1)
- `src/solver/compat.rs` (1)
- `src/scanner/mod.rs` (1)
- `src/checker/type_computation.rs` (1)
- `src/wasm_api/type_checker.rs` (1)

### Action

Each `unsafe` block should have a `// SAFETY:` comment explaining the invariant. Audit each for correctness. Consider if safe alternatives exist (e.g., `get_unchecked` -> `get` with a bounds check).

---

## Diagnostic Message Deduplication

**Priority**: Low-Medium
**Impact**: Maintainability, consistency with tsc
**Automatable**: Partially

### Current state

Error message strings like `"Type {0} is not assignable to type {1}"` appear in **13 different files**. Diagnostic codes like `2322` and `2304` are referenced in 6+ places each with hardcoded integers.

### Action

1. Create a centralized `diagnostics` module (or expand `src/diagnostics.rs`) with all error messages as constants:
   ```rust
   pub const TS2322: DiagnosticMessage = DiagnosticMessage {
       code: 2322,
       category: Error,
       message: "Type '{0}' is not assignable to type '{1}'.",
   };
   ```
2. Reference these constants everywhere instead of inline strings
3. This matches how TypeScript itself organizes `diagnosticMessages.json`

---

## Inline String Literal Constants

**Priority**: Low
**Impact**: Maintainability
**Automatable**: Yes (grep + refactor)

### Current state

**372 inline string literals** assigned to `let` bindings in non-test code. Many are repeated magic strings (file extensions, error prefixes, default values).

### Action

Extract repeated string literals to `const` or `static` items. Focus on strings that appear 3+ times.

---

## `.unwrap()` / `.expect()` Audit

**Priority**: Medium
**Impact**: Robustness, panic safety
**Automatable**: Partially (grep + review)

### Current state (non-test code)

Top offenders for `.unwrap()`:
- `src/cli/driver.rs` (93)
- `src/parallel.rs` (66)
- `src/bin/tsz_server/main.rs` (58)
- `src/checker/error_reporter.rs` (54)
- `src/lsp/project_operations.rs` (47)
- `src/checker/interface_type.rs` (45)

`.expect()` is heavily used in test files (expected), but also appears in production code.

### Action

- Replace `.unwrap()` in library code with proper error handling or `.expect("reason")`
- In `src/parallel.rs` and `src/cli/driver.rs`, propagate `Result` instead of panicking
- LSP server code (`tsz_server/main.rs`) should never panic -- convert all unwraps

---

## `.clone()` Hot-Path Audit

**Priority**: Medium
**Impact**: Performance
**Automatable**: Partially (grep + profiling)

### Current state

Top `.clone()` usage in non-test code:
- `src/cli/driver.rs` (93)
- `src/parser/node_arena.rs` (67)
- `src/bin/tsz_server/main.rs` (58)
- `src/checker/error_reporter.rs` (54)

### Action

Profile hot paths and replace unnecessary clones with borrows or `Rc`/`Arc` where appropriate. Focus on the solver and checker first since they're the performance-critical paths.

---

## Reduce `lib.rs` Bloat

**Priority**: Medium
**Impact**: Readability, maintainability
**Automatable**: Mostly (move code, update imports)

### Current state

`lib.rs` is **2,702 lines** containing:
- ~80 functions and 5 impl blocks
- WASM API (`Parser`, `WasmProgram`, `WasmTransformContext` structs with `#[wasm_bindgen]`)
- Path utilities (`normalize_slashes`, `get_base_file_name`, etc.)
- String comparison utilities (`compare_strings_case_sensitive`, etc.)
- Character classification (`is_line_break`, `is_digit`, etc.)
- 26+ `#[cfg(test)] #[path = "tests/..."]` test module registrations
- `CompilerOptions` struct and deserialization (200+ lines)

### Action

1. Move path/string utilities -> `src/path_utils.rs` or `src/common/`
2. Move `CompilerOptions` -> `src/compiler_options.rs`
3. Move WASM API structs -> `src/wasm_api/parser.rs` and `src/wasm_api/program.rs`
4. Move test registrations into their respective modules
5. Target: `lib.rs` should be <200 lines of module declarations and re-exports

---

## Consolidate Test Infrastructure

**Priority**: Low-Medium
**Impact**: Maintainability
**Automatable**: Mostly

### Current state

- 26+ test files registered in `lib.rs` via `#[path = "tests/..."]`
- `src/tests/` directory has 26 files that could live in their respective modules
- Test harness (`test_harness.rs`, `test_fixtures.rs`, `isolated_test_runner.rs`) is in `src/tests/` but used across modules

### Action

- Move module-specific tests (e.g., `checker_state_tests.rs`) into `src/checker/tests/`
- Move module-specific tests (e.g., `parser_state_tests.rs`) into `src/parser/tests/`
- Keep shared test infrastructure in `src/tests/` or a `test-utils` crate

---

## Add `rustfmt.toml` (Stable Options)

**Priority**: Low
**Impact**: Consistency
**Automatable**: Yes (one-time `cargo fmt`)

Even without nightly features, codify style:
```toml
edition = "2024"
max_width = 100
use_small_heuristics = "Max"
```

Then run `cargo fmt` once to normalize everything.

---

## Clippy Allow-List Cleanup

**Priority**: Low
**Impact**: Code quality
**Automatable**: Yes (iterative)

### Current state

**60+ clippy lints** suppressed in `Cargo.toml`. Many were added during initial port and may no longer be needed.

### Action

1. Comment out 5-10 allows at a time
2. Run `cargo clippy` to see what fires
3. Fix the easy ones, re-allow the intentional ones with a comment
4. Repeat until the list is minimal

---

## `tsz_server/main.rs` Decomposition

**Priority**: Low-Medium
**Impact**: Readability, maintainability
**Automatable**: Mostly (extract functions/modules)

### Current state

`src/bin/tsz_server/main.rs` is **4,685 lines** -- a single file for the entire LSP server binary. Contains request handlers, state management, and protocol logic all mixed together.

### Action

Split into:
- `main.rs` -- entry point, server setup (~100 lines)
- `handlers.rs` -- request/notification handlers
- `state.rs` -- server state management
- `protocol.rs` -- LSP protocol helpers

---

## Feature-Gated Code Extraction

**Priority**: Low
**Impact**: Build times for WASM target
**Automatable**: Partially

### Current state

Multiple files use `#[cfg(not(target_arch = "wasm32"))]` to gate native-only code:
- `src/lsp/project.rs` (9 instances)
- `src/lib.rs` (6)
- `src/limits.rs` (5)
- `src/parallel.rs` (4)

### Action

Group native-only code into dedicated modules (e.g., `src/native/`) or use the existing `cli` feature flag more consistently. This makes the WASM build smaller and clearer.

---

## Summary Table

| Task | Priority | Effort | Status |
|------|----------|--------|--------|
| Cargo workspaces | Medium | Large | **In Progress** |
| Visibility tightening (`pub` -> `pub(crate)`) | Medium | Medium | Todo |
| Reduce `.to_string()` allocations | Medium | Medium | Todo |
| Test file `#[cfg(test)]` guards | Medium | Low | Todo |
| Split oversized test files | Medium | Medium | Todo |
| `#[allow(dead_code)]` audit | Low | Low | Todo |
| `unsafe` code audit | High | Low | Todo |
| Diagnostic message dedup | Low-Med | Medium | Todo |
| Inline string constants | Low | Low | Todo |
| `.unwrap()` audit (non-test) | Medium | Medium | Todo |
| `.clone()` hot-path audit | Medium | Medium | Todo |
| Reduce `lib.rs` bloat | Medium | Medium | Todo |
| Consolidate test infrastructure | Low-Med | Medium | Todo |
| Add `rustfmt.toml` | Low | Low | Todo |
| Clippy allow-list cleanup | Low | Low | Todo |
| `tsz_server/main.rs` decomposition | Low-Med | Medium | Todo |
| Feature-gated code extraction | Low | Low | Todo |
| Import organization (nightly fmt) | Low | Low | Todo |
