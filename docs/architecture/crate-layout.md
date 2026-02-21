# Crate Layout Standard

This document defines the default source layout for crates in the TSZ workspace.

## Goals

- Keep crate roots as thin facades.
- Make module ownership obvious by folder name.
- Reduce `src/*.rs` sprawl as crates grow.
- Stage migration through low-risk crates first.

## Standard Folder Hierarchy

Not every crate needs every folder, but new code should use these domains when applicable:

- `src/api/`: public crate-facing APIs, facades, and integration entrypoints.
- `src/core/`: core data structures and behavior that implement crate-local semantics.
- `src/passes/`: ordered pipeline/pass execution logic.
- `src/diagnostics/`: diagnostic types, formatting, and reporting helpers.
- `src/tests/fixtures/`: fixture files used by tests.

Crates can add additional domain folders (`flow/`, `resolution/`, `wasm_api/`, etc.) when they better describe ownership.

## Root File Policy

- `src/lib.rs` and `src/main.rs` are allowed as root facades.
- Root-level non-facade modules (`src/*.rs`) are tolerated for legacy crates but are **not** the target end-state.
- New modules must land in a domain folder once a crate has more than **4** root-level **non-facade** modules (`TSZ_CRATE_ROOT_FILE_THRESHOLD=4`, excluding `lib.rs`/`main.rs`).

## Current Crate Mapping

Legend:
- **No-op move**: path/module wiring only, no intended behavior change.
- **Behavior-sensitive**: ordering, initialization, or algorithm coupling where code motion can alter runtime/compiler behavior.

| Crate | Current shape (high level) | Convention mapping target | No-op move candidates | Behavior-sensitive refactors |
|---|---|---|---|---|
| `tsz-scanner` | `lib.rs` + root impl files | `api/` + `core/` | Move `scanner_impl.rs`, `char_codes.rs` under `core/`; keep `lib.rs` as facade. | None expected when visibility/re-exports are preserved. |
| `tsz-lowering` | `lib.rs` + root lowering files | `api/` + `passes/` | Move `lower.rs`, `lower_advanced.rs` under `passes/`; keep `lib.rs` entrypoints. | Pass ordering/dispatch changes can alter transforms. |
| `tsz-binder` | many root `state_*` modules | `api/` + `core/` + `passes/` | Move state pass files to `passes/`; move loader/debug helpers to `core/`/`api/`. | Scope graph/flow graph construction sequencing and hoist pass interactions. |
| `tsz-parser` | `parser/`, `syntax/` + thin root | keep current; optional `api/` for façade helpers | Keep parser internals in `parser/`; optional root cleanup only. | Grammar/state machine extraction that changes parse ordering or recovery. |
| `tsz-common` | mixed root + `diagnostics/` | `core/` + `diagnostics/` | Move generic helpers (`position`, `span`, `numeric`, etc.) into `core/`. | Changes to shared ids/diagnostic data contracts used by many crates. |
| `tsz-emitter` | strong folders + some root helpers | `api/` + `passes/` + existing domains | Move remaining root helpers/facades under `api/`/`passes/`. | Transform pass ordering, helper injection timing, declaration emit interactions. |
| `tsz-lsp` | many root feature files + `code_actions/` | `api/` + feature domain folders | Group protocol features under folders (`features/`, `diagnostics/`, `symbols/`, etc.). | Incremental project state/update ordering and request routing coupling. |
| `tsz-cli` | root drivers + bin entrypoints | `api/` + `core/` + `diagnostics/` | Move command wiring/reporting into `api/` + diagnostics foldered modules. | Watch/incremental flow and project resolution path behavior. |
| `tsz-wasm` | root files + `wasm_api/` | keep `wasm_api/`, add `core/` for internals | Move non-export internals under `core/`; keep wasm boundary in `wasm_api/`. | JS/WASM boundary serialization + API stability changes. |
| `tsz-solver` | many root algorithm modules | algorithm domain folders under `core/` + `diagnostics/` | Folder by relation/evaluate/infer/instantiate/ops without changing call graph. | Any refactor touching relation/evaluation/inference semantics or caches. |
| `tsz-checker` | many root orchestration modules + `query_boundaries/` | `api/` + `passes/` + `diagnostics/` + `query_boundaries/` | Folder orchestration modules while keeping boundary entrypoints stable. | Diagnostic priority/suppression behavior and query boundary flow. |
| `tsz-conformance` (`conformance`) | runner/cache/cli at root | `api/` + `core/` + `tests/fixtures/` | Move runner/cache internals to `core/`; keep cli facade stable. | Test selection, cache compatibility, output normalization behavior. |


### Phase 1 detailed module mapping (initial implementation target)

#### `tsz-scanner`
- Current root modules: `scanner_impl.rs`, `char_codes.rs` (+ facade `lib.rs`).
- Target mapping:
  - `core/scanner_impl.rs` (**no-op move**)
  - `core/char_codes.rs` (**no-op move**)
  - `lib.rs` remains facade and re-export boundary (**no-op move**)

#### `tsz-lowering`
- Current root modules: `lower.rs`, `lower_advanced.rs` (+ facade `lib.rs`).
- Target mapping:
  - `passes/lower.rs` (**no-op move**, when call graph/order unchanged)
  - `passes/lower_advanced.rs` (**behavior-sensitive** if pass order or dispatch changes)
  - `lib.rs` remains facade and pass entrypoint surface (**no-op move**)

#### `tsz-binder`
- Current root modules include `state.rs`, `state_*`, `lib_loader.rs`, `module_resolution_debug.rs`, `lib.rs`.
- Target mapping:
  - `passes/state.rs`, `passes/state_*.rs` (**mixed**: path-only is no-op, but pass sequencing is behavior-sensitive)
  - `core/lib_loader.rs`, `core/module_resolution_debug.rs` (**no-op move**)
  - `lib.rs` remains public facade (**no-op move**)

## Rollout Order

1. **Phase 1 (low risk):** `tsz-scanner`, `tsz-lowering`, `tsz-binder`.
2. **Phase 2 (medium):** `tsz-parser`, `tsz-common`, `tsz-emitter`, `tsz-lsp`, `tsz-cli`, `tsz-wasm`, `tsz-conformance`.
3. **Phase 3 (high):** `tsz-solver`, `tsz-checker`.

Each migration PR should explicitly call out which moved files are no-op vs behavior-sensitive and include targeted tests for behavior-sensitive buckets.

## CI Guardrail

`./scripts/check-crate-root-files.sh` enforces the root-file policy for newly-added files:

- Looks at newly added `crates/*/src/*.rs` files in branch diff.
- If a crate currently has more than the threshold non-facade root files, adding another root file fails.
- `lib.rs`/`main.rs` are exempt as crate root facades.
- Accepts an explicit diff base via `TSZ_CRATE_ROOT_FILE_BASE` (or first CLI arg) and falls back to `origin/main` merge-base when unavailable.

This keeps migrations incremental while preventing further root-level sprawl.
