# TSZ AGENT SPEC (LLM-COMPACT)

## 0) Mission
- Absolute target: match `tsc` behavior exactly.
- Must match diagnostics, inference, compatibility, narrowing, and edge cases.

## 1) North Star
- Solver-first.
- Thin wrappers.
- Visitor-driven type traversal.
- Arena/interning everywhere possible.
- One semantic `TypeId` universe (solver-canonical).
- Public solver API uses `TypeData` naming at boundaries; raw type internals are crate-private.

## 2) Canonical Pipeline
- `scanner -> parser -> binder -> checker -> solver -> emitter`
- LSP consumes project/checker/solver outputs; does not own type algorithms.

## 3) Responsibility Split (WHO/WHAT/WHERE/OUTPUT)
- Scanner: lex + token stream + string interning.
- Parser: syntax only, builds AST in `NodeArena`.
- Binder (`WHO`): symbols, scopes, flow graph; no type computation.
- Checker (`WHERE`): AST walk, context, diagnostics; delegates type logic.
- Solver (`WHAT`): all type relations/evaluation/inference/instantiation/narrowing.
- Emitter (`OUTPUT`): JS/transform emission; no semantic type validation.

## 4) Hard Architecture Rules
- If code computes type semantics, it belongs in Solver.
- Checker must not implement ad-hoc type algorithms.
- Checker must not pattern-match low-level type internals when Solver query exists.
- Checker must not import/construct raw `TypeKey` or perform direct solver interning.
- CLI and ancillary crates must consume checker diagnostics via `tsz_checker::diagnostics`.
- Use visitor helpers for type traversal; avoid repeated `TypeKey` matching.
- No forbidden shortcuts:
  - Binder importing Solver for semantic decisions.
  - Emitter importing Checker internals for semantic checks.
  - Any layer bypassing canonical query APIs.
  - Checker or CLI using `tsz_checker::types` internal diagnostic paths.

## 5) Judge/Lawyer Model (Compatibility)
- Judge (`SubtypeChecker`): strict structural/set-theoretic subtype logic.
- Lawyer (`CompatChecker` + `AnyPropagationRules`): TS legacy/compat quirks.
- Lawyer owns:
  - `any` propagation policy,
  - variance modes,
  - excess property/freshness,
  - `void` return exception,
  - weak type detection (TS2559).
- Preferred default: `any` must not silence structural mismatches unless compatibility mode requires it.

## 6) DefId-First Semantic Type Resolution
- Semantic refs in Solver are `TypeData::Lazy(DefId)` (with `TypeKey` sealed/private).
- Checker creates/stabilizes `DefId` and ensures environment mapping exists.
- `TypeEnvironment` resolves `DefId -> TypeId` during relation/evaluation.
- Type-shape traversal and `Lazy(DefId)` discovery must use solver visitors, not checker recursion.
- New semantic refs must not use ad-hoc symbol-backed shortcuts.

## 7) Core Data/Perf Contracts
- Identity handles:
  - `TypeId(u32)`, `SymbolId(u32)`, `FlowNodeId(u32)`, `Atom(u32)`.
- Single semantic type universe: no checker-local semantic `TypeId`/`TypeArena` in default pipeline.
- O(1): type equality, symbol lookup, node access, string equality.
- AST node header target: 16 bytes.
- Arena allocation for AST/symbols/flow.
- Global type interning for dedup + O(1) equality.

## 8) Scanner Contracts
- Zero-copy source handling (`Arc<str>` style).
- Identifiers/strings interned to `Atom`.
- Parser-driven scanning API.
- Contextual rescans supported (`>`, `/`, template).

## 9) Parser Contracts
- Produces syntax-only AST.
- Node header fixed/thin (`kind`, `flags`, `pos`, `end`, `data_index`).
- Typed side pools for payloads.
- Recursion guard limit enforced.

## 10) Binder Contracts
- Produces:
  - symbol arena,
  - persistent scope tree (not transient stack model),
  - control-flow graph.
- Handles hoisting with multi-pass strategy.
- No type inference/subtyping logic in binder.

## 11) Solver Contracts
- Owns relation/evaluation/inference/instantiation/operations/narrowing.
- Owns type construction via safe factories/builders (`array`, `union`, `intersection`, `lazy`, etc.).
- Uses memoization and cycle/coinductive handling.
- Type graph represented through interned `TypeData` variants (crate-private internals; no raw `TypeKey` leakage).
- Owns algorithmic caches (relation/evaluation/instantiation/property/index/keyof/template).
- Provides structured relation failure reasons for checker diagnostics.
- Enforce recursion/fuel limits for subtype/evaluate/instantiate workloads.

## 12) Checker Contracts
- Thin orchestration only.
- Reads AST/symbol/flow facts; asks Solver for semantic answers.
- Tracks diagnostics and source locations.
- Maintains node/symbol/flow/diagnostic caches and recursion guards.
- Must not own algorithmic type caches or type-shape traversal logic.
- Checker files should stay under ~2000 LOC.

## 13) Emitter Contracts
- Prints/transforms output from checked structures.
- Supports downlevel transform pipeline (AST -> IR -> print) where applicable.
- No on-the-fly semantic type validation.

## 14) LSP Contracts
- Consumer/orchestrator over project state.
- Global type interning across files.
- Incremental updates via reverse deps.
- Should remain WASM-compatible constraints when required.

## 15) Dependency Policy (Review Gate)
For each change ask:
1. Is this `WHAT` (type algorithm) or `WHERE` (diagnostic location/orchestration)?
2. If `WHAT`: move to Solver/query helper.
3. If `WHERE`: keep in Checker and call Solver.
4. Does this introduce checker access to solver internals (`TypeKey`, raw interner)? If yes, reject.
5. Does assignability flow through the shared compatibility gate? If no, refactor first.

## 16) Type System Surface (Canonical Kinds)
- Must support primitives, literals, arrays/tuples, objects, unions/intersections,
  callables/functions, type params, lazy refs, applications, conditionals,
  mapped/index/keyof/template/this/unique symbol/readonly/string intrinsics/infer/error.
- Keep built-in TypeId reservations stable (0..99 range policy).

## 17) Flow/Symbol Contracts
- Symbols include declarations, parent linkage, member/export tables, flags/modifiers.
- Flow graph nodes include flags + antecedents + AST link.
- Preserve TS-relevant symbol/modifier semantics.

## 18) Performance Targets (Directional)
- Parse: high-throughput, parallel at file level.
- Bind/check: incremental-friendly with reverse dependency tracking.
- Hot paths avoid per-op heap allocation where practical.
- Maintain fast identity checks before structural checks.

## 19) Code Organization Guidance
- Keep modules aligned with pipeline ownership (`scanner`, `parser`, `binder`, `solver`, `checker`, `emitter`, `transforms`).
- Prefer dedicated files per major checker/solver concern.
- Avoid growth of monolith modules; split before crossing maintainability threshold.

## 20) Skills (Operational)
Available skills and triggers:
- `architecture-guardrails`: detect forbidden architecture patterns.
- `bench-gatekeeper`: benchmark vs baseline.
- `rust-test-runner`: run Rust tests via Docker wrapper.
- `sync-and-merge-assistant`: safe sync/merge workflow.
- `worker-assignment-orchestrator`: assign high-impact tasks.
- `skill-creator`: create/update skills.
- `skill-installer`: install curated/repo skills.

Skill usage rules:
- If user names a skill or task clearly matches it, use that skill this turn.
- Read `SKILL.md` minimally; load only needed referenced files.
- Reuse scripts/assets/templates from skill directories when available.
- If blocked/missing, state issue briefly and proceed with best fallback.

## 21) Non-Negotiables
- Parity with `tsc` overrides convenience.
- Architecture direction is one-way; no cross-layer semantic leakage.
- Solver is the single source of truth for type computation.

## 22) TS2322 Priority Rules
- `TS2322` parity is a top-level gate for checker/solver work.
- `TS2322`/`TS2345`/`TS2416` paths must use one compatibility gateway via `query_boundaries`.
- Gateway order is fixed: relation -> reason -> diagnostic rendering.
- Use the assignability gate helper as a single entrypoint for relation + failure analysis; avoid split ad-hoc calls.
- New checker code must not call `CompatChecker` directly for TS2322-family paths; route through `query_boundaries/assignability`.
- Checker must not instantiate solver internals in feature modules when a boundary helper can exist.
- Keep `TS2322` behavior centralized:
  - one suppression/prioritization policy,
  - one mismatch decision gate,
  - one explain-to-diagnostic rendering path.
- Prefer moving new fixes into boundary helpers and solver query logic, not ad-hoc checker branches.

## 23) TS2322 Change Review Checklist
1. Does this change alter `Assignable`/`Subtype` behavior in checker code?
2. If yes, can it be expressed as a solver relation query or boundary helper first?
3. Are weak-union/excess-property/any-propagation behaviors preserved and explicit?
4. Does the path resolve `Lazy(DefId)` via `TypeEnvironment` before relation checks?
5. Is traversal/precondition discovery delegated to solver visitors (no checker type recursion)?
6. Is the diagnostic generated from solver failure reason instead of checker-local heuristics?
