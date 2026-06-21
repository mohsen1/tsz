# Workspace Crates

The root `Cargo.toml` declares `members = ["crates/*"]` and centralizes shared
dependency versions and build profiles. The workspace crates form the compiler
pipeline plus the surfaces that expose and test it.

## Compiler Layers

| Crate | Role | Key entrypoint |
| --- | --- | --- |
| `crates/tsz-common` | Shared ids, spans, positions, diagnostics, options, source maps, module-resolution data, and performance counters. | `crates/tsz-common/src/lib.rs` |
| `crates/tsz-scanner` | Lexing, tokenization, rescanning, and character-code helpers. | `crates/tsz-scanner/src/lib.rs` |
| `crates/tsz-parser` | Syntax-only AST creation, node arenas, parse diagnostics, and syntax kinds. | `crates/tsz-parser/src/lib.rs` |
| `crates/tsz-binder` | Symbols, scopes, declaration summaries, export surfaces, modules, hoisting, and flow skeletons. | `crates/tsz-binder/src/lib.rs` |
| `crates/tsz-checker` | AST traversal, source context, diagnostics orchestration, checker-to-solver requests, flow analysis, and syntax-family checking. | `crates/tsz-checker/src/lib.rs` |
| `crates/tsz-solver` | Type relations, evaluation, inference, instantiation, narrowing, operations, semantic caches, and compatibility policy. | `crates/tsz-solver/src/lib.rs` |
| `crates/tsz-emitter` | JavaScript emit, declaration emit, transforms, source maps, and output writers. | `crates/tsz-emitter/src/lib.rs` |
| `crates/tsz-lowering` | Lowering utilities used where AST or semantic products need transformed representations. | `crates/tsz-lowering/src/lib.rs` |

## Integration Crates

| Crate | Role | Key entrypoint |
| --- | --- | --- |
| `crates/tsz-core` | Public facade over scanner, parser, binder, checker, solver, emitter, config, embedded libs, module resolution, WASM-friendly API helpers, and test fixtures. | `crates/tsz-core/src/lib.rs` |
| `crates/tsz-cli` | Command-line binaries, argument handling, watch mode, reporting, trace setup, project filesystem flows, and `try-tsz`. | `crates/tsz-cli/src/lib.rs` |
| `crates/tsz-lsp` | Language service features such as diagnostics, completions, hover, navigation, rename, formatting, semantic tokens, code actions, and project state. | `crates/tsz-lsp/src/lib.rs` |
| `crates/tsz-wasm` | WASM adapter that re-exports core APIs and serializes compiler results for JavaScript consumers. | `crates/tsz-wasm/src/lib.rs` |
| `crates/tsz-website` | Website and playground support, including static assets, Eleventy templates, bundled library declarations, and data generation. | `crates/tsz-website/src/lib.rs` |
| `crates/conformance` | Rust conformance runner support for TypeScript diagnostic parity, cache generation, filtering, and wrapper execution. | `crates/conformance/src/lib.rs` |

## Facade Pattern

Several crates expose a thin `lib.rs` facade over internal modules. That is
intentional. The facade makes ownership visible:

- `crates/tsz-core/src/lib.rs` re-exports compiler layers for public use.
- `crates/tsz-checker/src/lib.rs` wires a large checker module tree and many
  regression tests.
- `crates/tsz-solver/src/lib.rs` exposes solver construction, query,
  computation, observability, and type handles while keeping lower-level modules
  grouped by semantic concern.
- `crates/tsz-emitter/src/lib.rs` exposes context, emitters, transforms,
  lowering, output, and declaration emit APIs.

The layout standard in `docs/architecture/crate-layout.md` pushes new code
toward named folders such as `api/`, `core/`, `passes/`, and `diagnostics/`
once root-level modules become too dense.

## Build Profiles

The root manifest also documents performance and CI profiles:

- `release` is fast to compile with reasonable runtime performance.
- `dist` is production-oriented with LTO, single codegen unit, stripped symbols,
  and `panic = "abort"`.
- `dist-fast`, `dev-fast`, `flame`, `ci-unit`, and `ci-lint` trade off compile
  time, debug info, runtime shape, and peak memory for specific workflows.

Those profiles matter because `tsz` is developed against both correctness and
iteration-speed constraints. Documentation and tests should name the command and
profile used when a claim depends on them.
