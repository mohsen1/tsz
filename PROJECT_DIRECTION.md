# Project Zang

## Mission

Project Zang is a complete rewrite of the TypeScript compiler and type checker in Rust, compiled to WebAssembly. The goal is to achieve performance improvements while maintaining compatibility with the original TypeScript compiler.

## Architecture Overview

### Never Break The Build

- No commit should break the build or cause test failures
- All changes must pass the unit tests
- No change should reduce conformance test accuracy


### Keep the architecture clean

- dont take shortcuts
- dont modify the code specifically for tests. source code should not know about tests. revert changes that have done that in the past
- make good judgement on how to approach work
- pick the right work item based on the current state of the codebase

### Key Components

| Component | Location | Purpose |
|-----------|----------|---------|
| Parser | `src/thin_parser.rs` | Rust implementation of TypeScript parser |
| Checker | `src/thin_checker.rs` | Type checking and semantic analysis |
| Solver | `src/solver/` | Type resolution and constraint solving |
| Binder | `src/binder/` | Symbol binding and scope management |
| Diagnostics | `src/checker/types/diagnostics.rs` | Error codes and messages |

### Spec Documents

- `specs/WASM_ARCHITECTURE.md` - Architecture deep dive
- `specs/SOLVER.md` - Type solver design
- `specs/` - Other component designs

---

## Running Conformance Tests

To get current metrics, run the conformance test suite:

```bash
# Quick test (200 files, ~1 min)
./differential-test/run-conformance.sh --max=200 --workers=4

# Standard test (500 files, ~3 min)
./differential-test/run-conformance.sh --max=500 --workers=8

# Full test (all files, ~15 min)
./differential-test/run-conformance.sh --all --workers=14
```

### Understanding Results

| Metric | Meaning |
|--------|---------|
| Exact Match | WASM and TSC emit identical error codes |
| Same Error Count | Same number of errors (may differ in codes) |
| Missing Errors | TSC emits but WASM doesn't (under-reporting) |
| Extra Errors | WASM emits but TSC doesn't (over-reporting) |
| Crashed | WASM panicked during test |

**Target:** 95%+ exact match before production release.

### Building WASM

```bash
wasm-pack build --target web --out-dir pkg
```

---

## Priority Issues

### Tier 0: Quality & Stability Foundations

**Goal:** Fix cross-cutting gaps that block correctness across all tiers.

| Issue | Description |
|-------|-------------|
| Application type expansion | `TypeKey::Application` is not expanded, leading to incorrect diagnostics/assignability |
| Readonly types | `readonly` arrays/tuples are currently treated as mutable |
| AST child enumeration | `get_children` returns empty in parser arenas, breaking traversal-based features |
| Solver test coverage | `infer/subtype/evaluate` tests are commented out due to API drift |
| Panic hardening | Non-test paths still `panic!/unwrap` instead of recovering or re-parsing |
| Definite assignment gaps | TS2565 not implemented; interface type parameters TODO |

**Key Files:** `src/solver/evaluate.rs`, `src/solver/intern.rs`, `src/parser/arena.rs`, `src/parser/thin_node.rs`, `src/solver/subtype.rs`, `src/solver/infer.rs`, `src/cli/driver.rs`, `src/interner.rs`, `src/thin_checker.rs`

### Tier 1: Parser Accuracy

**Goal:** Parser should accept all valid TypeScript syntax without emitting false errors.

| Issue | Description |
|-------|-------------|
| TS1109 extra | Parser emits "Expression expected" for valid syntax |
| TS1005 extra | Parser emits "X expected" for valid constructs |
| ASI handling | Automatic semicolon insertion edge cases |

**Key Files:** `src/thin_parser.rs`

### Tier 2: Type Checker Accuracy

**Goal:** Checker should emit the same semantic errors as TSC.

| Issue | Description |
|-------|-------------|
| TS2571 extra | "Object is of type 'unknown'" over-reported |
| TS2683 missing | "'this' implicitly has type 'any'" not emitted |
| TS2507 incomplete | Non-constructor extends not fully checked |
| TS2348 extra | "Cannot invoke expression" over-reported |
| TS2322 accuracy | Type assignability false positives |

**Key Files:** `src/thin_checker.rs`, `src/solver/`

### Tier 3: Symbol Resolution

**Goal:** All symbols should resolve correctly, including globals and modules.

| Issue | Description |
|-------|-------------|
| TS2304 gaps | "Cannot find name" for valid symbols |
| TS2524 missing | Module member resolution failures |
| Global merging | Interface/namespace merging across files |

**Key Files:** `src/binder/`, `src/thin_checker.rs`

### Tier 4: Implicit Any Checks

**Goal:** Emit TS7006/TS7008 only when type cannot be inferred.

| Issue | Description |
|-------|-------------|
| TS7006 extra | Parameter implicit any over-reported |
| TS7005 extra | Variable implicit any over-reported |

**Key Files:** `src/thin_checker.rs`

### Tier 5: Async/Await

**Goal:** Correct handling of async functions, generators, and await expressions.

| Issue | Description |
|-------|-------------|
| TS2705 gaps | Async function return type checking |
| TS1359 missing | 'await' reserved word detection |
| Async generators | `AsyncGenerator` vs `Promise` return types |

**Key Files:** `src/thin_checker.rs`

---

## File Reference

| File | Purpose |
|------|---------|
| `src/thin_parser.rs` | Main parser implementation |
| `src/thin_checker.rs` | Main type checker (~22k lines) |
| `src/binder/` | Symbol table and scope management |
| `src/solver/` | Type resolution engine |
| `src/checker/types/diagnostics.rs` | Error codes and message templates |
| `differential-test/` | Conformance test infrastructure |
| `differential-test/run-conformance.sh` | Docker-based test runner |

---

## Adding New Diagnostics

1. Add code to `src/checker/types/diagnostics.rs`:
   ```rust
   pub const NEW_ERROR_CODE: u32 = XXXX;
   ```

2. Add message template:
   ```rust
   pub const NEW_ERROR_MESSAGE: &str = "Error message with {0} placeholder.";
   ```

3. Emit in checker:
   ```rust
   use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

   let message = format_message(diagnostic_messages::NEW_ERROR_MESSAGE, &[arg]);
   self.error_at_node(node_idx, &message, diagnostic_codes::NEW_ERROR_CODE);
   ```

4. Rebuild and test:
   ```bash
   wasm-pack build --target web --out-dir pkg
   node /tmp/test.mjs
   ```
