# Architecture Rules for Agents

This document defines critical architecture rules for Project Zang. All contributors must follow these guidelines.

## Primary Goal: TypeScript Compiler Compatibility

**Match tsc behavior exactly.** Every error, every type inference, every edge case must behave identically to TypeScript's compiler. If tsc reports an error, we must report it. If tsc allows code, we must allow it.

## Core Principles

0. **Real work should be done** - Do not make documentation only commits. 
1. **No shortcuts** - Implement correct logic, not quick fixes
2. **Test-agnostic code** - Source code must never check file names or paths
3. **Configuration-driven** - Use `CompilerOptions` for all behavior changes
4. **Fix root causes** - Never suppress errors or add special cases
5. **Clean code** - Write clean code

## Code Review Checklist

Before merging changes:

- [ ] Rust and WASM builds succeed (`cargo build`, `wasm-pack build`)
- [ ] Unit tests pass (`scripts/test.sh`)
- [ ] No clippy warnings (`cargo clippy -- -D warnings`)
- [ ] Conformance tests pass (`./conformance/run-conformance.sh --all`) -- does not have to be 100% but should not drop significantly
- [ ] No shortcuts taken - all fixes address root causes
- [ ] No test-aware code in source

## Pre-commit Hooks

Git hooks are installed automatically on first `cargo build`. They enforce code quality by running:
1. `cargo fmt` - Format all code
2. `cargo clippy --fix` - Lint and auto-fix issues
3. Unit tests

To manually install: `./scripts/install-hooks.sh`

## References

- **PROJECT_DIRECTION.md**: Project priorities and architecture rules
- **specs/TS_UNSOUNDNESS_CATALOG.md**: Catalog of known unsoundness issues and required compat layer rules
- **specs/SOLVER.md**: Type resolution architecture and guidelines
- **specs/WASM_ARCHITECTURE.md**: WASM build and runtime architecture

Below are key internal documents and a couple of external references used by contributors:

- [PROJECT_DIRECTION.md](PROJECT_DIRECTION.md): Project priorities and architecture rules.
- [specs/TS_UNSOUNDNESS_CATALOG.md](specs/TS_UNSOUNDNESS_CATALOG.md): Catalog of known unsoundness issues and required compat-layer rules.
- [specs/SOLVER.md](specs/SOLVER.md): Type resolution architecture and design guidelines.
- [specs/WASM_ARCHITECTURE.md](specs/WASM_ARCHITECTURE.md): WASM build and runtime architecture.
- [specs/COMPILER_OPTIONS.md](specs/COMPILER_OPTIONS.md): Semantics and supported `CompilerOptions`.
- [specs/DIAGNOSTICS.md](specs/DIAGNOSTICS.md): Diagnostic message guidelines and error mapping.
- [scripts/test.sh](scripts/test.sh): Recommended test runner (runs tests in Docker).
- [conformance/run-conformance.sh](conformance/run-conformance.sh): Conformance test harness and invocation.

External references:

- [TypeScript Compiler (tsc) — GitHub](https://github.com/microsoft/TypeScript): Reference behavior to match for compatibility.
- [ECMAScript® Language Specification](https://tc39.es/ecma262/): Language semantics referenced by the project.


## When work is done?

All unit tests should pass. There should be zero clippy warnings. It's okay if conformance goes down after some work but a huge drop in conformance is not acceptables

## Run commands with a reasonable timeout

ALWAYS run commands with a reasonable timeout to avoid commands that will hang

## Run tests in docker
Always run tests in docker to ensure a consistent environment. Using `scripts/test.sh` will automatically use docker

## Disk Usage with Worktrees

Each worktree maintains its own `.target/` directory (~400 MB) and `node_modules` (~185 MB). With incremental compilation enabled, these grow over time.

**Periodic cleanup:**
```bash
# Clean incremental caches only (keeps compiled deps)
rm -rf .target/*/incremental

# Full clean (will require rebuild)
cargo clean

# Clean node_modules if not actively using
rm -rf node_modules TypeScript/node_modules conformance/node_modules
```

**When to clean:**
- Before switching to a worktree you haven't used in a while
- When disk usage becomes problematic
- After major dependency updates
