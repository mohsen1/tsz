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

## Priority List
- [ ] Make sure all code in source that is doing trailer made work to satisfy the test is removed. Source should not have any awareness of test 
- [ ] Improve testing infrastructure to configure the environment before dunking the tests based on @ directives 
- [ ] Before diving into more parser and checker enhancement lets review the code and make sure architecture is solid 
- [ ] We should be able to use ts-tests directory how the original typescript repo use their tests to ensure parser and checker is working correctly. Testing infrastructure should be solid 
- [ ] Study test results and write a new plan of attack for getting to 100% tsc compatibility 
- [ ] Update agents.md to enforce good practices as mentioned above 
- [ ] Work on getting to 100%



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
