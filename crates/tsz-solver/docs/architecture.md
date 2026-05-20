# Solver internal architecture ownership

`tsz-solver` keeps `lib.rs` focused on orchestration, module wiring, and public exports.
Algorithmic implementation belongs in responsibility folders under `src/`.

## Folder ownership

- `relations/`: subtype + compatibility checks, query boundaries, and relation failure reasoning.
- `evaluation/`: type-level evaluation rules, reduction, and normalization entrypoints.
- `inference/`: inference contexts, constraints, and substitution resolution.
- `instantiation/`: generic/type-parameter instantiation logic.
- `visitors/`: reusable type traversal helpers for `TypeData` graphs.
- `caches/`: relation/evaluation/instantiation and related query caches.

## Boundary terminology

- Public solver boundaries use `TypeData`/`TypeId` terminology.
- Raw interned storage keys remain crate-private internals.
- New cross-module APIs should expose semantic handles (`TypeId`, `TypeData`, `DefId`) rather than interner implementation details.

## Root module policy

Root modules are thin facades where compatibility shims are still needed for imports.
New algorithmic code should be added inside the ownership folders above instead of root `src/*.rs` modules.
