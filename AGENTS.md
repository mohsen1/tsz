# Zang - TypeScript Compiler in Rust/WASM

## Mission
TypeScript compiler rewritten in Rust/WASM. **Beat TypeScript-Go in performance.**

## Philosophy: Performance-First Architecture

We have time. No deadlines. Do it right.

## Architectural Principles: Anti-Patterns to Avoid

These are CRITICAL rules that must never be violated:

1. **Never add file_name.contains(), isTestFile(), or similar checks that change compiler behavior based on file paths**
   - The compiler must not inspect file names or paths to determine how to process code
   - File path inspection creates implicit, hidden behavior that is impossible to configure or control
   - This violates the principle of explicit configuration

2. **Source code must not inspect file names to change behavior - configuration must be explicit**
   - All compiler behavior must be controlled through explicit configuration (CompilerOptions)
   - No heuristics or magic detection based on file paths, directory names, or naming patterns
   - Configuration should be declarative and visible, not inferred from context

3. **Test configuration comes from explicit CompilerOptions passed to the compiler, not from heuristics or file path inspection**
   - If tests need special compiler behavior, it must be configured via explicit compiler options
   - Never use patterns like `if path.contains("test")` or `if is_test_file()`
   - The same file processed with the same CompilerOptions must always produce the same result

**Why these rules matter:**
- Predictability: Same input + same config = same output, always
- Debuggability: No hidden behavior to discover when things go wrong
- Testability: Easy to reproduce issues by just providing the same config
- Correctness: Compiler behavior is controlled, not guessed

## Must Read

- `specs/WASM_ARCHITECTURE.md`
- `specs/SOLVER.md` (when working on solver-related tasks)

## Workflow

1. Sync first: Sync with main branch from origin.
2. Write code, add tests, run `./test.sh`.
3. Commit and push.
4. Run Conformance tests again and compare to previous report.

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

Important: Tests must always run inside Docker to ensure environment consistency.

```bash
# Rust unit tests (Docker)
./test.sh

# Conformance tests
./differential-test/run-conformance.sh --all

# Build WASM
wasm-pack build --target web --out-dir pkg
```
