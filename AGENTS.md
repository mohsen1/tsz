# Zang - TypeScript Compiler in Rust/WASM

## Mission
TypeScript compiler rewritten in Rust/WASM. **Beat TypeScript-Go in performance.**

## Philosophy: Performance-First Architecture

We have time. No deadlines. Do it right.

## Must Read

- `specs/WASM_ARCHITECTURE.md`
- `specs/SOLVER.md` (when working on solver-related tasks)

## Workflow

1. **Sync first**: `git fetch origin && git merge origin/main --no-edit`
2. Run Conformance tests: `./differential-test/run-conformance.sh --all` and analyze the report.
3. Write code, add tests, run `./test.sh`.
4. Commit and push.
5. Run Conformance tests again and compare to previous report.

## Commit Format
```
<component>: <description>
```

Commit frequently and atomically.

## Key Directories

| Directory | Purpose |
|-----------|---------|
| `src/` | Rust source code |
| `tests/` | Rust unit tests |
| `specs/` | Architecture documentation |
| `differential-test/` | Conformance test infrastructure |
| `ts-tests/` | TypeScript test cases |

## Running Tests

```bash
# Rust unit tests (Docker)
./test.sh

# Conformance tests
./differential-test/run-conformance.sh --all

# Build WASM
wasm-pack build --target web --out-dir pkg
```
