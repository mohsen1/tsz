# Module Resolution and Binder Issues

## Scope
This document summarizes module-resolution and binder-related issues identified by code study only (no new runtime experiments in this pass). The goal is to explain where parity drift with `tsc` is likely coming from, especially for `TS2307` and `TS2305`.

## 1) Dual Resolution Engines (Behavior Drift Risk)
- `src/module_resolver.rs` implements a full resolver (Node/Node16/NodeNext/Bundler, `paths`, package `exports/imports`, extension rules, specific error mapping).
- `crates/tsz-checker/src/module_resolution.rs` implements a simplified file-to-file map resolver (`build_module_resolution_maps`) based on relative paths.
- Impact:
  - Different execution paths can disagree on whether a module exists and which error to emit.
  - Checker/LSP/test flows that rely on the simplified map can diverge from CLI flow that uses `ModuleResolver`.

## 2) Driver Uses Mixed Resolution Strategy
- In `crates/tsz-cli/src/driver.rs` (`collect_diagnostics`), resolution starts with `ModuleResolver`, but on `NotFound` it can fallback through `resolve_module_specifier(...)`.
- Impact:
  - The fallback may accept modules the canonical resolver rejected (or reject modules canonical resolver would accept in other modes).
  - This can cause inconsistent `TS2307` behavior and unstable target mappings for export/member validation.

## 3) TS2307 Emission Is Distributed Across Multiple Checker Layers
- Module-not-found handling appears in:
  - `crates/tsz-checker/src/import_checker.rs`
  - `crates/tsz-checker/src/module_checker.rs`
  - `crates/tsz-checker/src/state_type_resolution.rs` (`emit_module_not_found_error`)
- Dedup exists (`modules_with_ts2307_emitted`) but logic still runs in multiple places.
- Impact:
  - Error timing/order can differ by path.
  - Message/code selection can differ depending on which path emits first.

## 4) Export Surface Is Reconstructed Heuristically
- `crates/tsz-checker/src/state_type_resolution.rs`:
  - `resolve_effective_module_exports`
  - `merge_export_equals_members`
  - `collect_reexported_symbols`
- Includes fallback behavior that may use first available table from a binder state.
- Impact:
  - `export =` + merged namespace/interface/class cases are sensitive to symbol-table shape and merge order.
  - Likely source of `TS2305`/`TS2339` cascades where module exists but effective exports are miscomputed.

## 5) Re-export Graph Traversal Depends on Multiple Key Spaces
- Binder tracks:
  - `module_exports`
  - `reexports`
  - `wildcard_reexports`
  in `crates/tsz-binder/src/state.rs` and `crates/tsz-binder/src/state_binding.rs`.
- Checker traverses these using current file index + module-specifier candidates.
- Impact:
  - Small key mismatches (file name vs specifier variant) can break `export *`/named re-export resolution.
  - This directly manifests as missing exported member (`TS2305`) even when symbols exist.

## 6) Module Specifier Canonicalization Is Repeated and Non-Canonical
- Candidate generation/normalization logic appears in multiple places:
  - `crates/tsz-checker/src/context.rs`
  - `crates/tsz-checker/src/state_type_resolution.rs`
  - Other lookup call-sites in import/module checkers.
- Impact:
  - One subsystem may find `"foo"` while another expects `'foo'` or normalized slash form.
  - Inconsistent lookups produce false unresolved-module/member errors.

## 7) LSP/Server Path Uses Simplified Resolution Context
- In `crates/tsz-cli/src/bin/tsz_server/main.rs`, checker context is populated via `build_module_resolution_maps`.
- This bypasses parts of the full resolver model used in CLI diagnostics.
- Impact:
  - IDE diagnostics may disagree with CLI diagnostics for mode-specific and package-resolution scenarios.

## 8) Ambient Module / Augmentation / export= Interactions Are Fragile
- Binder handling in:
  - `crates/tsz-binder/src/state_binding.rs` (`populate_module_exports`, augmentation detection)
  - `crates/tsz-binder/src/state.rs` (import resolution with reexports, module tables)
- Checker applies additional corrective heuristics to determine non-module entities and namespace/value usage.
- Impact:
  - Edge cases around ambient external modules and merged declarations can produce extra or missing diagnostics (`TS2305`, `TS2339`, `TS2708`).

## 9) Architectural Tension: Resolver Truth vs Checker Inference
- Intended model: resolver determines module existence/target; checker validates semantics.
- Current model: checker still performs substantial module-existence/export inference from binder tables.
- Impact:
  - Duplicate logic paths increase parity drift and maintenance burden.

## Suggested Prioritization (Analysis-Only)
1. Unify around one canonical resolution result model consumed by checker everywhere.
2. Centralize TS2307/TS2792/TS2834-family emission from resolver outcomes, not scattered checker branches.
3. Canonicalize module specifier normalization in one utility and reuse everywhere.
4. Replace ad-hoc effective-export synthesis with a single canonical export-surface builder keyed by resolved module target.
5. Align LSP/server path with CLI resolution pipeline to avoid split behavior.

## Action Plan (Code-Grounded)
## Phase 0: Preconditions and Safety Rails
1. Fix conformance runner build break to restore feedback loop.
- `crates/conformance/src/runner.rs` currently fails to compile due `TestResult::Fail` field mismatch (`missing_fingerprints`, `extra_fingerprints`).
- This is a precondition for validating module-resolution changes at scale.

2. Add explicit module-resolution parity guard tests before refactor.
- Create targeted regression tests around existing failures and recently fixed cases:
- `ambientExternalModuleWithoutInternalImportDeclaration.ts`
- `aliasOnMergedModuleInterface.ts`
- Preserve both CLI and checker-unit expectations.

3. Define invariants document in code comments.
- Resolver invariants (existence, target identity, diagnostic code source) in `src/module_resolver.rs`.
- Checker invariants (never infer existence ad hoc when resolver data exists) in `crates/tsz-checker/src/context.rs`.

## Phase 1: Single Resolution Contract (CLI Path First)
1. Introduce a single contract object built once per file.
- Current driver builds three structures:
- `resolved_module_paths: (file_idx, specifier) -> target_file_idx`
- `resolved_module_specifiers: (file_idx, specifier)` set
- `resolved_module_errors: (file_idx, specifier) -> ResolutionError`
- Files:
- `crates/tsz-cli/src/driver.rs` (`collect_diagnostics`)
- Replace these parallel structures with one typed record map, e.g.:
- `ResolvedImportRecord { status, target_idx, error_code, error_message, kind }`

2. Stop mixed fallback logic in the diagnostics path.
- Today, after `ModuleResolver::resolve_with_kind(...)` fails, driver still uses `resolve_module_specifier(...)` fallback in multiple places.
- Files:
- `crates/tsz-cli/src/driver.rs`
- `crates/tsz-cli/src/driver_resolution.rs`
- Action:
- In diagnostics path, treat `ModuleResolver` as source of truth for resolution outcome.
- Keep legacy fallback only in clearly isolated compatibility paths if absolutely needed, behind a separate function boundary and comments.

3. Unify parallel and sequential file-check paths.
- Parallel path bridges specifiers using precomputed maps.
- Sequential path still re-resolves and mutates `resolved_modules` with additional fallback checks.
- File:
- `crates/tsz-cli/src/driver.rs`
- Action:
- Extract shared helper that computes per-file checker resolution context from the contract object.
- Use same helper in both parallel and sequential branches.

## Phase 2: Canonical Specifier Normalization
1. Implement one normalization utility and use it everywhere.
- Currently candidate expansion is duplicated:
- `crates/tsz-checker/src/context.rs` (`module_specifier_candidates`)
- `crates/tsz-checker/src/state_type_resolution.rs` (`module_specifier_candidates`)
- Driver-specific normalization in multiple spots
- Action:
- Add one canonical function (preferably in shared crate) returning ordered key variants.
- Replace duplicated local implementations.

2. Key all cross-module maps through normalized keys.
- `module_exports`, `reexports`, `wildcard_reexports`, and `resolved_module_errors` lookups should consistently pass through the same normalization layer.
- Files:
- `crates/tsz-checker/src/context.rs`
- `crates/tsz-checker/src/import_checker.rs`
- `crates/tsz-checker/src/module_checker.rs`
- `crates/tsz-checker/src/state_type_resolution.rs`

## Phase 3: Centralize TS2307/Related Error Emission
1. Consolidate unresolved-module diagnostics path.
- Today TS2307-family emission happens in:
- `crates/tsz-checker/src/import_checker.rs`
- `crates/tsz-checker/src/module_checker.rs`
- `crates/tsz-checker/src/state_type_resolution.rs`
- Action:
- Add one checker helper (single entry point) that consumes resolver contract + current AST location and emits the diagnostic once.
- Keep `modules_with_ts2307_emitted` dedupe, but make dedupe owned by this single helper.

2. Ensure module-not-found code selection always originates from resolver.
- Preserve special code behavior (TS2792, TS2834, TS2835, TS5097, TS2732, TS7016) from resolver outcome.
- Avoid checker-side recomputation of message/code based on partial context except when no resolver data exists.

## Phase 4: Canonical Export Surface Builder
1. Replace ad-hoc export-surface synthesis with a stable pipeline.
- Problem area:
- `resolve_effective_module_exports`
- `merge_export_equals_members`
- `collect_reexported_symbols`
- File:
- `crates/tsz-checker/src/state_type_resolution.rs`
- Action:
- Build a single export-surface resolver that:
- Uses resolved target file identity first.
- Applies named exports, `export =` overlays, named reexports, wildcard reexports in deterministic order.
- Performs cycle-safe traversal with memoized visited keys.

2. Remove heuristic fallbacks that depend on map iteration order.
- Eliminate last-resort patterns like taking first export table entry from binder maps.
- Replace with explicit target-driven lookup or explicit “unknown export surface” state.

3. Keep `export =` value/type behavior explicit.
- Preserve distinction between:
- namespace/object surface
- `export =` target value surface
- merged namespace/value shapes
- Ensure this is computed once in export-surface builder, not re-inferred in multiple checker call sites.

## Phase 5: Binder Data Model Tightening (Without Type-Algorithm Leakage)
1. Keep binder focused on symbol facts but expose stable module graph facts.
- Binder should provide canonical:
- module declaration identity
- direct exports
- re-export edges
- wildcard re-export edges
- Files:
- `crates/tsz-binder/src/state.rs`
- `crates/tsz-binder/src/state_binding.rs`

2. Add explicit module-key type.
- Replace raw string keys in critical cross-file maps with a structured key where feasible:
- resolved file key for file-backed modules
- ambient module key for string-literal modules
- This reduces string-variant ambiguity and repeated quoting heuristics.

## Phase 6: LSP/Server Parity
1. Replace map-only resolution in server diagnostics path.
- Current server path uses `build_module_resolution_maps(&file_names)` and bypasses full resolver semantics.
- File:
- `crates/tsz-cli/src/bin/tsz_server/main.rs`
- Action:
- Use same resolution contract builder as CLI diagnostics.
- Feed `resolved_module_paths` + resolver errors + resolved module status consistently into checker context.

2. Keep single-file fast path but with same semantics.
- If maintaining optimized single-file mode, still run through same resolver codepath with constrained input set.

## Phase 7: Performance Guardrails
1. Preserve and extend caching where contract is introduced.
- Reuse `ModuleResolver.resolution_cache` and package type cache.
- Avoid per-checker recomputation of candidate keys.

2. Ensure no extra N^2 lookups in export-surface traversal.
- Memoize per-file export surfaces.
- Memoize re-export traversal by `(file_idx, export_name)` where appropriate.

3. Verify parallel path stays lock-light.
- Keep per-file immutable resolution snapshots passed into checker workers.
- Avoid shared mutable structures in hot per-node checks.

## Phase 8: Validation Gates
1. Unit tests.
- Resolver:
- extension rules across Node16/NodeNext/Bundler
- `paths/baseUrl` precedence
- package `exports/imports` condition selection
- Checker:
- TS2307 dedupe and code selection through centralized helper
- TS2305 on re-export graphs and `export =` mixed surfaces

2. Conformance slices.
- Track deltas on:
- `TS2307` missing/extra
- `TS2305` missing/extra
- related cascades `TS2339` and `TS2708`

3. Cross-mode consistency checks.
- Compare CLI diagnostics and LSP diagnostics for same test corpus after unification.

## Incremental Rollout Strategy
1. Land in small, reviewable PRs by phase boundaries.
2. Do Phase 1 and Phase 3 before deep binder reshaping to stabilize diagnostics early.
3. Only then perform export-surface canonicalization and binder key tightening.
4. Keep compatibility shims short-lived and clearly marked with removal criteria.

## Definition of Done
1. One canonical resolver contract feeds checker and server paths.
2. No checker-internal duplicate TS2307 emission paths remain.
3. `resolve_effective_module_exports` no longer relies on heuristic map-order fallbacks.
4. Module specifier normalization logic exists in one shared utility.
5. Measured reduction in `TS2307`/`TS2305` mismatches without throughput regression in standard conformance runs.
