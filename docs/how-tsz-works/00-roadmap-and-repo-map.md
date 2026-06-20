# Roadmap And Repo Map

`tsz` is a performance-first TypeScript compiler in Rust. The north star is a
drop-in `tsc` replacement: the same project result as `tsc`, substantially
faster when it succeeds, and clearly categorized failures when it does not.

The living plan is `docs/plan/ROADMAP.md`. This guide explains how the repository
implements that plan.

## The Four Goals

Every meaningful change serves one of four goals:

| Goal | Meaning | Documentation angle |
| --- | --- | --- |
| `green` | Required benchmark projects compile with the same result as `tsc`. | Explain how project rows, diagnostics, and semantic blockers are owned. |
| `fast` | Green rows are at least 2x faster than `tsgo`. | Explain caches, residency, measurement scripts, and performance invariants. |
| `grow` | More real-world projects enter the corpus and reach Green plus Fast. | Explain project metadata and fixture growth paths. |
| `hold` | Conformance, emit, declaration emit, and LSP parity do not regress. | Explain verification gates and architecture boundaries. |

This guide mostly serves `hold`: it makes the codebase easier to navigate
without weakening boundaries. It also supports `green` and `fast` by making
semantic ownership and benchmark tooling discoverable.

## Repository Shape

Top-level layout:

| Path | Role |
| --- | --- |
| `Cargo.toml` | Workspace membership, shared dependencies, and build profiles. |
| `crates/` | Rust workspace crates for compiler layers, integrations, conformance, WASM, and website. |
| `scripts/` | CI, benchmark, conformance, emit, fourslash, setup, quality, and agent automation. |
| `docs/` | Architecture, specs, site docs, roadmap, development docs, and this guide. |
| `.github/` | Issue templates, PR template, and GitHub Actions workflows. |
| `.agents/`, `.claude/`, `.codex/` | Agent skills, context, hooks, and local assistant configuration. |
| `.cargo/`, `.config/`, `.vscode/` | Local tool configuration. |

The Rust workspace is intentionally split by compiler layer and integration
surface. The compiler core is not one crate with all decisions mixed together;
the split is part of the architecture.

## The Pipeline Contract

The durable layer order is:

```text
scanner -> parser -> binder -> checker -> solver -> emitter
```

Supporting surfaces consume the pipeline:

- `crates/tsz-cli` turns command-line inputs and watch/project flows into
  compiler runs.
- `crates/tsz-core` is the public facade used by CLI, WASM, tests, and the site.
- `crates/tsz-lsp` exposes editor features from parsed/bound/checked projects.
- `crates/tsz-wasm` adapts core APIs to JavaScript/WASM consumers.
- `crates/conformance` and `scripts/conformance/` compare `tsz` against
  TypeScript's diagnostic corpus.
- `scripts/bench/` and `scripts/ci/` measure project rows and enforce gates.

The design rule is not simply "call the next crate". Each crate has a kind of
truth it owns. If a layer needs a fact owned by another layer, the request should
cross an explicit boundary with stable ids and structured results.

## Where To Look First

- Layer boundaries: `docs/architecture/BOUNDARIES.md`.
- Query surfaces: `docs/architecture/QUERY_BOUNDARY_INVENTORY.md`.
- Emit ownership: `docs/architecture/EMIT_ARCHITECTURE.md`.
- Crate organization: `docs/architecture/crate-layout.md`.
- Current goals: `docs/plan/ROADMAP.md`.
- Generated file map: `docs/how-tsz-works/file-inventory/README.md`.
