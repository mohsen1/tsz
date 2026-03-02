# Architectural Plan: Hard Conformance Problems

This document outlines the strategic approach to solving three major architectural challenges identified in the TSZ conformance test suite (Track B). These problems require foundational changes to the type system, symbol resolution, and project orchestration.

---

## 1. The `TypeId::UNKNOWN` Dual-Use Problem (TS18046)

### Context & Problem
Currently, the type solver uses `TypeId::UNKNOWN` for two distinct purposes:
1. **Explicit Unknown:** The user explicitly annotated a variable with `: unknown`.
2. **Resolution Failure Fallback:** The checker couldn't resolve a type (e.g., missing import, broken heritage clause) and fell back to a default type to prevent compiler panics.

This dual-use prevents accurate diagnostic emission. If the compiler tries to access a property on an unresolved type, it emits TS18046 ("'x' is of type 'unknown'"). This creates cascading false-positive errors because the root cause (the missing import) already generated an error, and downstream usage should be silently suppressed (acting like `any`), not treated as a strict `unknown` type.

### Proposed Architecture: `TypeId::ERROR` (Error Type)
We need to mirror TypeScript's internal `errorType` concept.

1. **New Type Primitive:** Introduce `TypeId::ERROR` (or `TypeId::UNRESOLVED`) alongside `UNKNOWN`, `ANY`, `NEVER`, etc., in the `tsz-solver`'s `TypeInterner`.
2. **Behavioral Semantics:**
   - **Assignability:** `TypeId::ERROR` should be bi-directionally assignable to/from *any* type (just like `TypeId::ANY`). This prevents "Type X is not assignable to type Error" cascades.
   - **Property Access:** Accessing *any* property on `TypeId::ERROR` should return `TypeId::ERROR` silently.
   - **Call/Construct:** Calling `TypeId::ERROR` should return `TypeId::ERROR` silently.
3. **Diagnostic Suppression:** The checker must check for `TypeId::ERROR` before emitting type-related diagnostics and bail out. TS18046 should *only* trigger for the genuine `TypeId::UNKNOWN`.

### Implementation Steps
1. Add `TypeId::ERROR` to `tsz-solver/src/primitives.rs` and `TypeInterner`.
2. Update the `assignability`/`subtype` rules in the solver to treat `ERROR` identically to `ANY` structurally.
3. Audit `tsz-checker` for fallback usages. Replace `return TypeId::UNKNOWN` with `return TypeId::ERROR` in error-recovery paths (e.g., failed identifier resolution, missing properties).
4. Update TS18046 emission logic to ensure it strictly checks for `TypeId::UNKNOWN` and ignores `TypeId::ERROR`.
5. Run the conformance suite and measure the reduction in cascading false positives.

---

## 2. Cross-File SymbolId Collisions (TS2506)

### Context & Problem
The binder currently operates on a per-file basis, resulting in isolated `BinderState` instances for each file. Within a file, symbols are tracked via a `SymbolId` (which is typically just a local index like `u32`).
When the checker resolves a heritage clause across files (e.g., `class A extends imported.B`), it pulls a `SymbolId` from File A's binder, but subsequently attempts to look up members or exports using File B's binder. Because `SymbolId`s are local, `SymbolId(5)` in File A corresponds to "MyClass", but `SymbolId(5)` in File B corresponds to an entirely unrelated symbol (e.g., "SomeOtherVar"), causing bizarre false positives.

### Proposed Architecture: `QualifiedSymbolId` (or Global Symbol Arena)
To fix this, the system must know *which* binder a `SymbolId` belongs to whenever passing symbol boundaries.

**Option A: Scoped Identifiers (Recommended)**
Instead of passing around raw `SymbolId`s, the cross-file boundary APIs should use a `QualifiedSymbolId`:
```rust
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct QualifiedSymbolId {
    pub file_id: FileId,    // or ModuleId / BinderId
    pub symbol_id: SymbolId,
}
```

**Option B: Global Symbol Arena**
Transition the binder to mutate a shared, workspace-wide `SymbolArena`. This would make all `SymbolId`s globally unique. *Note: This is a heavier refactor and may impact parallel binding performance if lock contention occurs.*

### Implementation Steps (Assuming Option A)
1. Introduce `QualifiedSymbolId` in `tsz-binder` or `tsz-common`.
2. Update `ExportSymbol` and related structures to store the `file_id` alongside the exported `symbol_id`.
3. Refactor `resolve_heritage_symbol` and `resolve_export` in the checker to require and return `QualifiedSymbolId`.
4. Update the `CheckerState` symbol lookup utilities so that when a cross-file symbol is queried, it correctly loads the target file's binder and queries using the local `SymbolId` portion.
5. Add rigorous unit tests simulating circular references and cross-file heritage collision to prevent regressions (like `commentOnAmbientModule.ts`).

---

## 3. Project References & `--build` (TS5057)

### Context & Problem
The compiler currently lacks support for composite projects (`"references": [{ "path": "./foo" }]` in `tsconfig.json`). The conformance suite includes 52 tests specifically for composite project orchestration (TS5057). 

### Proposed Architecture
Implementing `tsc --build` requires a multi-phase orchestration system above the standard compiler driver. It requires:
1. **Config Graph Parsing:** Reading `tsconfig.json`, extracting `references`, and building a Directed Acyclic Graph (DAG) of project dependencies.
2. **Up-to-date Checks:** Determining if a project needs rebuilding based on `.tsbuildinfo` files and source file mtimes.
3. **Orchestrated Compilation:** Compiling projects in topological order, using the `.d.ts` outputs of upstream projects as inputs for downstream projects.

### Implementation Steps (Phased)

**Phase 1: Config Parsing & Validation**
1. Add `composite` and `references` properties to the TSConfig parser models.
2. Implement validation (TS5057): If a project has references, ensure the referenced `tsconfig.json` exists.
3. Implement `tsc --build` CLI flag parsing in `tsz-cli`.

**Phase 2: Project Graph Construction**
1. Create a `ProjectGraph` builder that walks references and detects cycles (emitting appropriate TS errors for circular dependencies).
2. Implement a topological sort to determine build order.

**Phase 3: Execution & Output (Future Work)**
1. Implement the up-to-date checker (reading `.d.ts` mtimes).
2. Orchestrate isolated `tsz_server/driver` runs per project, ensuring the `program` state leverages the declaration outputs of its upstream dependencies rather than re-compiling them from source.

---

## Recommended Order of Operations

1. **Phase 1: `TypeId::ERROR` Refactor.** This is highly localized to the solver/checker, provides immediate accuracy wins across the whole codebase by stopping false positive cascades, and is a prerequisite for cleaning up complex inference bugs.
2. **Phase 2: Cross-File `SymbolId`.** This touches the boundary between Binder and Checker. It is critical for the correctness of external module resolution and heritage clauses.
3. **Phase 3: Project References.** This is a large feature scoped primarily to `tsz-cli` and `tsconfig` orchestration. It can be built iteratively in parallel with core checker fixes.
