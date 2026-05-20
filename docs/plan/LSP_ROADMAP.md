# TSZ LSP Roadmap Appendix

Status: roadmap appendix. `docs/plan/ROADMAP.md` owns active sequencing. This
file defines durable LSP/WASM direction and activation criteria; it is not a
standalone reason to start broad LSP work while project-corpus correctness is
still the primary gate.

Do not use this file as an LSP issue inventory or run log. Put current issue
state in GitHub issues, draft PR bodies, PR comments, and CI artifacts.

## Mission

Match the editor experience of `tsserver` while inheriting the compiler mission:
same answers as `tsc`, same semantic identity universe as the checker/solver,
and latency that remains inside the editor usability envelope.

Editor-specific invariants:

1. Responsiveness: hover, completion, diagnostics, and navigation should not be
   blocked behind long-running semantic work.
2. Stability: answers from before a `didChange` must not overwrite answers
   after it.
3. Identity preservation: LSP answers derive from the solver-canonical
   `TypeId` universe and project/checker outputs. LSP must not own a parallel
   type algorithm.

## Current Strategic Read

The LSP surface is broad and useful, but the roadmap keeps LSP/WASM expansion
low-bandwidth until the project corpus gate and compiler-service boundary are
healthier.

Durable facts:

1. `tsz-lsp` is a consumer/orchestrator over project state.
2. Project state shares `TypeInterner` and `DefinitionStore` across files.
3. Incremental update machinery exists through dependency graph and export
   signature invalidation.
4. LSP and WASM should converge on one compiler-service front door rather than
   matching raw solver internals.
5. Large LSP modules remain candidates for concern-based splitting, but that is
   not top-line work until activation criteria are met.

## Activation Criteria

LSP becomes an active campaign only when all of the following hold:

1. Required project-corpus rows in `ROADMAP.md` Phase 1 are green or have a
   named exception.
2. Track 9 has produced a stable compiler-service/semantic-view front door that
   LSP and WASM can consume.
3. A small responsiveness budget exists, with at least hover, completion, and
   diagnostics measured on one real project.
4. Open LSP work has been triaged into one active owner per issue/PR.

Until then, LSP work should be limited to:

1. regression fixes,
2. smoke gates that make future regressions visible,
3. small hover/JSDoc/display parity fixes,
4. WASM compatibility fixes that prevent existing consumers from breaking,
5. tests that protect existing behavior without starting architectural rewrites.

## Post-Activation Tracks

### L1: Cancellation And Request Scope

Requests need an explicit lifetime. Superseded work should cancel, and slow
workspace queries must not block lightweight editor requests.

Exit:

1. Public handlers can observe cancellation or revision invalidation.
2. A newer `didChange` invalidates in-flight semantic answers for that file.
3. Hover/completion latency is measured under contention.

Avoid: spawning background work that keeps mutable project state across an
unsafe async boundary.

### L2: Project Lifecycle And Tsconfig Reload

Configured, external, and inferred project state should behave predictably,
including tsconfig changes, path remapping, and idle project eviction.

Exit:

1. `tsconfig.json` changes reload project state without requiring callers to
   re-add every file.
2. Body-only edits do not invalidate unrelated dependents.
3. Project sleep/eviction has clear ownership and tests.

### L3: Compiler-Service Front Door

LSP, WASM, CLI, and editor-facing tools should consume one semantic-view API
over parser/binder/checker/solver results.

Exit:

1. LSP providers stop matching raw `TypeData` where a semantic view exists.
2. WASM and LSP use the same project/checker outputs for shared features.
3. Feature code states whether it needs syntax, binder, checker, or solver
   facts.

### L4: Module Decomposition

Split oversized LSP modules by concern only when it reduces risk or unblocks
feature work.

Priority split axes:

1. signature help: extraction, resolution, formatting,
2. module specifiers: paths/baseUrl/node_modules/relative,
3. imports: discovery, ranking, resolution,
4. project core: lifecycle, update path, introspection,
5. navigation/hierarchy: lookup, disambiguation, graph build.

Avoid: alphabetic or arbitrary line-count chunks.

### L5: WASM Compatibility

WASM features should avoid unguarded filesystem assumptions and consume the same
semantic views as native LSP.

Exit:

1. Filesystem-dependent paths are guarded or abstracted.
2. WASM smoke tests cover the exported language-service surface.
3. LSP/WASM shared code does not import platform-only services directly.

### L6: Response Parity And Test Gaps

Small LSP parity gaps can land before full activation when they protect existing
behavior.

Durable test gaps to keep visible:

1. `documentColor`,
2. `colorPresentation`,
3. `linkedEditingRange`,
4. file rename behavior,
5. limited `onTypeFormatting` coverage.

## Coordination Rules

1. Every LSP PR body includes `AgentName`.
2. Check open PRs and recent merged PRs before starting. Current work belongs in
   GitHub, not in this document.
3. Do not start broad LSP architecture work unless the activation criteria are
   met or the PR directly unblocks a named roadmap release gate.
4. If an LSP issue is a checker/solver semantic problem, route the fix to the
   owning compiler layer instead of patching the LSP response locally.
