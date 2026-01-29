# Architecture Migration Plan

Remaining tasks to reach North Star architecture. See [docs/architecture/NORTH_STAR.md](../architecture/NORTH_STAR.md) for target design.

## Status Summary

| Phase | Focus | Status | Remaining Work |
|-------|-------|--------|----------------|
| Phase 1 | Checker Cleanup | ~90% Done | 57 TypeKey matches, file splitting |
| Phase 2 | LSP Performance | Partial | Integrate existing modules |
| Phase 3 | Binder Cleanup | Not Started | Scope consolidation, CFG |
| Phase 4 | Emitter IR Migration | Not Started | Convert to IR-based emission |
| Phase 5 | Salsa Integration | Future | Query system |

---

## Phase 1: Checker Cleanup (In Progress)

### 1.1 Replace TypeKey Matches with Visitor

**Current State**: 57 direct `TypeKey::` matches across checker modules.

**Target**: Zero `TypeKey::` matches in Checker; all type dispatch through `TypeVisitor` trait.

**Files with TypeKey matches:**
```
src/checker/iterators.rs: 18 matches
src/checker/generators.rs: 15 matches
src/checker/state.rs: 12 matches
src/checker/type_literal_checker.rs: 6 matches
src/checker/type_checking.rs: 2 matches
src/checker/type_node.rs: 1 match
src/checker/type_computation.rs: 1 match
src/checker/jsx_checker.rs: 1 match
src/checker/array_type.rs: 1 match
```

**Tasks:**
- [ ] Convert matches in `iterators.rs` (18 occurrences)
- [ ] Convert matches in `generators.rs` (15 occurrences)
- [ ] Convert matches in `state.rs` (12 occurrences)
- [ ] Convert matches in `type_literal_checker.rs` (6 occurrences)
- [ ] Convert remaining 5 matches in other files

**Verification:**
```bash
rg "TypeKey::" src/checker/ --type rust | wc -l  # Current: 57, Target: 0
```

---

### 1.2 Split Large Checker Files

**Current State:**
| File | Lines | Target |
|------|-------|--------|
| `state.rs` | 8,984 | < 3,000 |
| `type_checking.rs` | 7,854 | < 3,000 |
| `control_flow.rs` | 3,878 | At threshold |
| `type_computation.rs` | 3,223 | At threshold |

**Tasks:**
- [ ] Extract signature checking from `state.rs`
- [ ] Extract expression checking from `type_checking.rs`
- [ ] Extract declaration checking from `type_checking.rs`

**Verification:**
```bash
wc -l src/checker/*.rs | sort -n | tail -5
```

---

## Phase 2: LSP Performance (Partial)

Data structures exist but are NOT integrated into `ProjectManager`.

### 2.1 Global Type Interning

**Problem:** Each `ProjectFile` owns its own `TypeInterner`. TypeIds from different files are incomparable.

**Tasks:**
- [ ] Add `shared_types: Arc<RwLock<TypeInterner>>` to `ProjectManager`
- [ ] Update `ProjectFile` to use shared interner
- [ ] Remove per-file `type_interner` field

---

### 2.2 Integrate Symbol Index

**Status:** `src/lsp/symbol_index.rs` exists (547 lines) but is NOT used.

**Problem:** Reference search still iterates all files linearly (O(N)).

**Tasks:**
- [ ] Add `SymbolIndex` field to `ProjectManager`
- [ ] Update `find_references` to use index instead of file iteration
- [ ] Update index on file changes

---

### 2.3 Integrate Dependency Graph

**Status:** `src/lsp/dependency_graph.rs` exists (303 lines) but is NOT used.

**Tasks:**
- [ ] Add `DependencyGraph` field to `ProjectManager`
- [ ] Build graph during binding
- [ ] Use for cache invalidation on file changes

---

## Phase 3: Binder Cleanup (Not Started)

### 3.1 Remove Legacy Scope System

**Problem:** Both `scope_chain` (stack-based) and persistent scope tree coexist (17 usages).

**Tasks:**
- [ ] Identify all `scope_chain` usages
- [ ] Convert to persistent scope API
- [ ] Remove `scope_chain` field

**Verification:**
```bash
rg "scope_chain" src/binder/ --type rust | wc -l  # Current: 17, Target: 0
```

---

### 3.2 Consolidate CFG Builders

**Problem:** Flow graph logic in two places:
- `src/binder/state.rs` (5,230 lines)
- `src/checker/flow_graph_builder.rs` (2,231 lines)

**Tasks:**
- [ ] Compare implementations for coverage
- [ ] Choose primary (likely `flow_graph_builder.rs`)
- [ ] Migrate missing features
- [ ] Remove duplicate

---

## Phase 4: Emitter IR Migration (Not Started)

**Current State:** Emitter uses direct Printer/SourceWriter pattern - AST nodes emit directly to text.

**Target State:** IR-based emission for better source map generation, transforms, and testability.

**Current architecture:**
- `src/emitter/mod.rs` - Printer struct, direct AST traversal
- Uses `SourceWriter` for output with source position tracking
- ~19 emit modules, ~8,600 LOC total

**Benefits of IR:**
- Cleaner separation between transform and emission
- Easier to test transforms in isolation
- Better source map generation from IR spans
- Enables optimization passes

**Tasks:**
- [ ] Design `IRNode` enum for JavaScript output
- [ ] Create transform pass: AST → IR
- [ ] Create emit pass: IR → text with source maps
- [ ] Migrate expressions first (simplest)
- [ ] Migrate statements
- [ ] Migrate declarations
- [ ] Remove direct-emit code

**Verification:**
```bash
# Current direct emit patterns (should eventually be 0)
rg "write!|push_str" src/emitter/ --type rust | wc -l
```

---

## Phase 5: Salsa Query System (Future)

**Current:** `src/solver/salsa_db.rs` behind `experimental_salsa` feature flag.

**Tasks:**
- [ ] Identify query candidates (type resolution, symbol types, subtype relations)
- [ ] Convert manual caches to Salsa queries
- [ ] Enable by default after stabilization

**Dependencies:** Requires Phase 1 completion (type logic in Solver).

---

## Verification Commands

```bash
# TypeKey violations
rg "TypeKey::" src/checker/ --type rust | wc -l

# File sizes
wc -l src/checker/*.rs | sort -n | tail -5

# Per-file interners in LSP
rg "\.type_interner" src/lsp/ --type rust

# Symbol index integration
rg "symbol_index|SymbolIndex" src/lsp/project.rs

# Dependency graph integration
rg "dependency_graph|DependencyGraph" src/lsp/project.rs

# Scope chain usage
rg "scope_chain" src/binder/ --type rust | wc -l
```

---

## Success Metrics

| Metric | Current | Target |
|--------|---------|--------|
| `state.rs` lines | 8,984 | < 3,000 |
| `type_checking.rs` lines | 7,854 | < 3,000 |
| TypeKey matches in Checker | 57 | 0 |
| Per-file TypeInterner | Yes | No |
| O(N) reference search | Yes | No |
| Symbol index integrated | No | Yes |
| Dependency graph integrated | No | Yes |
| scope_chain usages | 17 | 0 |
