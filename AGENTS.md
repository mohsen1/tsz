# Architecture Rules for Agents

This document defines critical architecture rules for Project Zang. All contributors must follow these guidelines.

## Primary Goal: TypeScript Compiler Compatibility

**Match tsc behavior exactly.** Every error, every type inference, every edge case must behave identically to TypeScript's compiler. If tsc reports an error, we must report it. If tsc allows code, we must allow it.

## Core Principles

0. **Real work should be done** - Do not make documentation-only commits
1. **Solver-first architecture** - Pure type logic belongs in solver; checker handles AST and diagnostics
2. **Use the visitor pattern** - Never write manual TypeKey match statements
3. **No shortcuts** - Implement correct logic, not quick fixes
4. **Test-agnostic code** - Source code must never check file names or paths
5. **Configuration-driven** - Use `CompilerOptions` for all behavior changes
6. **Fix root causes** - Never suppress errors or add special cases

**Conformance is a lagging indicator.** Focus on building a correct solver foundation. Pass rates improve as a consequence of correct architecture.

## Code Review Checklist

Before merging changes:

- [ ] Rust and WASM builds succeed (`cargo build`, `wasm-pack build`)
- [ ] Unit tests pass (`scripts/test.sh`)
- [ ] No clippy warnings (`cargo clippy -- -D warnings`)
- [ ] Conformance tests pass (`./conformance/run-conformance.sh --all`) -- does not have to be 100% but should not drop significantly
- [ ] No shortcuts taken - all fixes address root causes
- [ ] No test-aware code in source
- [ ] **Type logic is in solver, not checker** (see Solver-First Architecture below)
- [ ] **Visitor pattern used for type operations** (no manual TypeKey matches)

## Pre-commit Hooks

Git hooks are installed automatically on first `cargo build`. They enforce code quality by running:
1. `cargo fmt` - Format all code
2. `cargo clippy --fix` - Lint and auto-fix issues
3. Unit tests

To manually install: `./scripts/install-hooks.sh`

---

## Solver-First Architecture

**This is the most important architectural principle.** Read [docs/solver-type-computation-analysis.md](docs/solver-type-computation-analysis.md) for the complete guide.

### The Core Contract

> **Solver handles WHAT** (type operations and relations)
> **Checker handles WHERE** (AST traversal, scoping, control flow)

### Rules

1. **No AST in solver** - Solver functions take `TypeId` and return `TypeId` or structured results. Never pass AST nodes to solver.

2. **Solver returns structured results** - Solver returns result enums; checker formats diagnostics with source locations.

3. **Checker is a thin wrapper** - Checker extracts AST data, delegates to solver, and reports errors.

4. **No duplicated type logic** - If the same type logic exists in multiple places, consolidate it in solver.

### Example: Correct Delegation Pattern

```rust
// CORRECT - Checker delegates to solver
pub(crate) fn get_type_of_element_access(&mut self, idx: NodeIndex) -> TypeId {
    // 1. Extract AST data (CHECKER responsibility)
    let access = self.ctx.arena.get_element_access(idx)?;
    let object_type = self.get_type_of_node(access.expression);
    let index_type = self.get_type_of_node(access.argument);

    // 2. Delegate to solver (SOLVER does pure type logic)
    let evaluator = ElementAccessEvaluator::new(self.ctx.types);
    match evaluator.resolve_element_access(object_type, index_type) {
        ElementAccessResult::Success(ty) => ty,
        ElementAccessResult::NotIndexable { type_id } => {
            // 3. Report errors (CHECKER responsibility)
            self.report_error(TS2538, idx, type_id);
            TypeId::ERROR
        }
    }
}

// WRONG - Type logic in checker
pub(crate) fn get_type_of_element_access(&mut self, idx: NodeIndex) -> TypeId {
    let object_type = self.get_type_of_node(access.expression);
    // Don't do this - type logic should be in solver
    match self.ctx.types.lookup(object_type) {
        Some(TypeKey::Array(elem)) => elem,
        Some(TypeKey::Tuple(elements)) => { /* ... */ }
        // ... more type logic that belongs in solver
    }
}
```

---

## Type System Architecture

### Use the Visitor Pattern for Type Operations

**MANDATORY**: When working with types (TypeKey, TypeId), always use the visitor pattern from `src/solver/visitor.rs` instead of writing manual match statements. Read [docs/TYPE_VISITOR_PATTERN_GUIDE.md](docs/TYPE_VISITOR_PATTERN_GUIDE.md) for the complete guide.

```rust
// GOOD - Use visitor functions
use crate::solver::visitor::*;

fn check_type(&self, type_id: TypeId) {
    if is_function_type(&self.types, type_id) { ... }
    if is_literal_type(&self.types, type_id) { ... }
    if contains_type_parameters(&self.types, type_id) { ... }
}

// BAD - Manual match statements
fn check_type(&self, type_id: TypeId) {
    match self.types.lookup(type_id) {
        Some(TypeKey::Function(_)) => { ... }
        Some(TypeKey::Literal(_)) => { ... }
        _ => {}
    }
}
```

Available visitor functions:
- `is_literal_type`, `is_function_type`, `is_object_like_type`, `is_empty_object_type`
- `is_union_type`, `is_intersection_type`, `is_array_type`, `is_tuple_type`
- `is_type_parameter`, `is_conditional_type`, `is_mapped_type`
- `contains_type_parameters`, `contains_error_type`, `contains_type_matching`
- `collect_all_types`, `collect_referenced_types`

---

## Key Documentation

**Required reading for all contributors:**

| Document | Purpose |
|----------|---------|
| [docs/solver-type-computation-analysis.md](docs/solver-type-computation-analysis.md) | **Solver-first architecture guide** - How to structure type logic |
| [docs/TYPE_VISITOR_PATTERN_GUIDE.md](docs/TYPE_VISITOR_PATTERN_GUIDE.md) | **Visitor pattern guide** - How to work with types |
| [docs/SOLVER.md](docs/SOLVER.md) | Mathematical foundations of the type solver |
| [docs/specs/TS_UNSOUNDNESS_CATALOG.md](docs/specs/TS_UNSOUNDNESS_CATALOG.md) | TypeScript compatibility rules (intentional unsoundness) |

**Additional references:**

| Document | Purpose |
|----------|---------|
| [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) | Getting started, building, testing |
| [docs/TESTING.md](docs/TESTING.md) | Testing infrastructure |
| [docs/walkthrough/](docs/walkthrough/) | Deep dive into each compiler phase |
| [docs/WASM_ARCHITECTURE.md](docs/WASM_ARCHITECTURE.md) | WASM build and architecture |
| [docs/COMPILER_OPTIONS.md](docs/COMPILER_OPTIONS.md) | Supported compiler options |
| [docs/DIAGNOSTICS.md](docs/DIAGNOSTICS.md) | Diagnostic message guidelines |

**External references:**

- [TypeScript Compiler (tsc) - GitHub](https://github.com/microsoft/TypeScript): Reference behavior to match
- [ECMAScript Language Specification](https://tc39.es/ecma262/): Language semantics

---

## When is Work Done?

All unit tests should pass. There should be zero clippy warnings. It's okay if conformance goes down slightly after some work, but a huge drop in conformance is not acceptable.

**Remember:** Conformance is a lagging indicator. A correct solver foundation will naturally improve conformance over time.

## Run Commands with Reasonable Timeout

ALWAYS run commands with a reasonable timeout to avoid commands that will hang.

## Run Tests in Docker

Always run tests in docker to ensure a consistent environment. Using `scripts/test.sh` will automatically use docker.

Exception: Pre-commit hooks use `--no-sandbox` for speed since Docker adds ~5-10s overhead per run. Full Docker-based tests run in CI.

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
