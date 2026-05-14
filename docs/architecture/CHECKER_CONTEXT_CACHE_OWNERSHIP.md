# CheckerContext Cache Ownership

This inventory classifies cache-like fields carried by
`CheckerContext`. The goal is to make ownership and reset behavior explicit
before moving semantic caches across the checker/query-boundary/solver split.

Owners:

- Checker orchestration: source, binder, diagnostic, traversal, or driver state.
- Query boundary: semantic answers that depend on explicit checker request or
  type-environment inputs.
- Solver: pure type/relation/evaluation memoization that should not depend on
  source traversal state.

## Inventory

| Field | Key | Lifetime and reset | Owner | Notes |
|---|---|---|---|---|
| `symbol_types` | `SymbolId` | Restored from `TypeCache`; cleared on `switch_to_file` because binder-local `SymbolId`s collide | Checker orchestration | Symbol lookup cache for the active binder. |
| `symbol_instance_types` | `SymbolId` | Same as `symbol_types` | Checker orchestration | Active-binder class instance lookup cache. |
| `enum_namespace_types` | `SymbolId` | Cleared on `switch_to_file` | Checker orchestration | Values are tied to active binder symbols. |
| `var_decl_types` | `SymbolId` | Cleared on `switch_to_file` | Checker orchestration | Used for TS2403 checks in the active file/session. |
| `lib_type_resolution_cache` | `String` lib type name | Retained across file switches; not serialized in `TypeCache` | Checker orchestration | Program-stable lib lookup cache. |
| `shared_lib_type_cache` | `String` lib type name | Shared by `Arc<DashMap>` across parallel file checks | Checker orchestration | Driver-provided shared lib lookup cache. |
| `lib_delegation_cache` | `SymbolId` | Cleared on `switch_to_file`; inherited by child checkers | Checker orchestration | Child-checker delegation cache with binder-local keys. |
| `namespace_member_resolution_cache` | namespace name -> member name | Inherited by child checkers; not part of file-session reset today | Checker orchestration | Cross-binder name lookup cache. Values are symbol ids. |
| `export_equals_named_cache` | `(file_idx, module_name, export_name)` | Inherited by child checkers; not part of file-session reset today | Checker orchestration | Stores hits and safe misses for `export=` named lookups. |
| `nested_namespace_candidates_cache` | namespace name | Inherited by child checkers; not part of file-session reset today | Checker orchestration | Paired with `nested_namespace_candidates_cache_complete`. |
| `symbol_name_candidates_cache` | string name | Cleared on `switch_to_file` because values carry binder-local `SymbolId`s | Checker orchestration | Name key is stable, but values are file-local. |
| `lowering_entity_name_resolution_cache` | entity-name string | Cleared on `switch_to_file` because values carry `DefId`/symbol-derived references | Checker orchestration | Declaration-lowering lookup cache. |
| `namespace_exports_cache` | `(requesting_file_idx, module_specifier)` | Cleared on `switch_to_file` | Checker orchestration | Relative specifiers depend on requesting file. |
| `cross_file_type_params_cache` | `(target_file_idx, decl_idx)` | Program-wide `Arc<DashMap>` shared from `ProgramContext` | Query boundary | Memoizes slow-path type-parameter extraction across child checkers. |
| `node_types` | raw node index `u32` | Restored from `TypeCache` only for same file; cleared on `switch_to_file` | Checker orchestration | `NodeIndex` is arena-local and cannot cross files. |
| `request_node_types` | `(node_idx, RequestCacheKey)` | Cleared on `switch_to_file` | Query boundary | Request-aware node type cache for audited non-empty requests. |
| `type_environment` | `DefId` internals | Rebuilt on `switch_to_file`; snapshots feed `TypeCache` | Query boundary | Semantic environment used by assignability and emit. |
| `flow_analysis_cache` | `(FlowNodeId, SymbolId, initial TypeId)` | Serialized in `TypeCache`; cleared on file-session reset | Query boundary | Semantic flow result with file-local flow graph keys. |
| `flow_worklist`, `flow_in_worklist`, `flow_visited`, `flow_results` | `FlowNodeId` | Cleared on file-session reset | Checker orchestration | Reusable flow-analysis buffers, not durable caches. |
| `narrowing_cache` | solver-owned narrowing keys | Reused within checker; reset by construction rather than explicit file-session code | Solver | Already uses `tsz_solver::NarrowingCache`; keep access behind flow/query helpers. |
| `narrowable_identifier_cache` | raw node index `u32` | Replaced on file-session reset | Checker orchestration | AST-shape cache; source-local by construction. |
| `flow_switch_reference_cache` | `(node_idx, node_idx)` | Cleared on file-session reset | Checker orchestration | Source-reference comparison cache. |
| `flow_numeric_atom_cache` | numeric literal bits | Cleared on file-session reset | Checker orchestration | Atom conversion cache used during flow analysis. |
| `flow_reference_match_cache` | `(node_idx, node_idx)` | Cleared on file-session reset | Checker orchestration | Source-reference equivalence cache. |
| `symbol_last_assignment_pos` | `SymbolId` | Cleared on file-session reset | Checker orchestration | Closure narrowing helper tied to active flow graph. |
| `symbol_flow_confirmed` | `(SymbolId, declared TypeId)` | Cleared on file-session reset | Query boundary | Semantic narrowing shortcut tied to active flow graph. |
| `js_export_surface_cache` | file index | Cleared on file-session reset | Checker orchestration | Synthesized JS/CommonJS export surface cache. |
| `class_instance_type_cache` | class declaration `NodeIndex` | Serialized in `TypeCache`; cleared on file-session reset and symbol invalidation | Checker orchestration | Node-keyed class lookup cache. |
| `class_constructor_type_cache` | class declaration `NodeIndex` | Same as `class_instance_type_cache` | Checker orchestration | Node-keyed constructor lookup cache. |
| `class_chain_summary_cache` | class declaration `NodeIndex` | Cleared on file-session reset | Checker orchestration | Inheritance summary cache tied to source nodes. |
| `env_eval_cache` | `TypeId` | Cleared on file-session reset | Query boundary | Candidate for solver/query-boundary ownership; see below. |
| `class_symbol_to_decl_cache` | `SymbolId` | Cleared on file-session reset | Checker orchestration | Symbol-to-source lookup for inheritance queries. |
| `heritage_symbol_cache` | heritage expression `NodeIndex` | Cleared on file-session reset | Checker orchestration | Source-node lookup cache. |
| `base_constructor_expr_cache` | heritage expression `NodeIndex` | Cleared on file-session reset | Query boundary | Semantic fallback result but source-node keyed. |
| `base_instance_expr_cache` | heritage expression `NodeIndex` | Cleared on file-session reset | Query boundary | Semantic fallback result but source-node keyed. |
| `class_decl_miss_cache` | `TypeId` | Cleared on file-session reset | Query boundary | Semantic miss cache for class declaration lookup. |
| `jsx_intrinsic_props_cache` | `(intrinsic_elements_type, tag_atom)` | Cleared on file-session reset | Query boundary | Semantic JSX props evaluation cache. |
| `jsx_namespace_symbol_cache` | singleton hit/miss | Reset on file-session reset | Checker orchestration | Current-file JSX namespace lookup cache. |
| `jsx_intrinsic_elements_symbol_cache` | singleton hit/miss | Reset on file-session reset | Checker orchestration | Current-file JSX namespace member lookup cache. |
| `jsx_intrinsic_elements_type_cache` | singleton hit/miss | Reset on file-session reset | Query boundary | Current-file JSX type-position lookup cache. |

## Candidate Move

`env_eval_cache` is the best low-risk candidate for the next ownership move. It
stores semantic evaluation answers for `evaluate_type_with_env`, is keyed by
`TypeId`, and already tracks solver recursion-limit state in
`EnvEvalCacheEntry`. The cache should move behind a query-boundary helper before
it moves into solver storage directly, because the current answer also depends
on the active `TypeEnvironment`. The move should make that environment identity
or snapshot an explicit input instead of reading ambient `CheckerContext` state.

## Reset Invariants

- Raw `NodeIndex`, `FlowNodeId`, and current-file singleton caches must be
  cleared or rebuilt on `reset_for_next_file`.
- `SymbolId` caches are safe only when the active binder namespace is stable.
  The sequential file-session path swaps binders, so these caches are cleared on
  `switch_to_file`.
- String-keyed caches are not automatically durable. If values contain
  `SymbolId`, `DefId`, `NodeIndex`, or file-relative module state, reset with
  the active file.
- Program-stable shared caches need explicit owners: `ProgramContext` for
  cross-file checker orchestration, query-boundary helpers for semantic
  request answers, and solver caches for pure type/relation evaluation.
