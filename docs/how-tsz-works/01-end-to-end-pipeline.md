# End To End Pipeline

The compiler turns TypeScript source text into diagnostics and optionally
JavaScript, declaration output, and source maps. The happy path is easy to draw,
but the hard part is keeping ownership crisp while matching `tsc` edge cases.

```text
source text
  -> scanner tokens
  -> parser AST in arenas
  -> binder symbols, scopes, modules, flow skeleton
  -> checker traversal and source-context decisions
  -> solver semantic facts, relations, inference, narrowing
  -> checker diagnostic orchestration
  -> emitter JS / .d.ts / source maps
  -> CLI, LSP, WASM, website, conformance, benchmark rows
```

## Scanner

`crates/tsz-scanner` owns lexing. It reads text and produces tokens, trivia,
escape handling, token flags, and scanner state. It may know character codes and
TypeScript token spelling; it must not know parser recovery or semantic meaning.

Important entrypoints include `crates/tsz-scanner/src/lib.rs`,
`crates/tsz-scanner/src/scanner_impl.rs`, `crates/tsz-scanner/src/rescan.rs`,
and `crates/tsz-scanner/src/char_codes.rs`.

## Parser

`crates/tsz-parser` owns syntax. It consumes scanner output and writes AST nodes
into `NodeArena`. Parser ids like `NodeIndex` are traversal coordinates, not
semantic identity. Parser recovery affects syntactic diagnostics and downstream
tree shape, so parser changes can shift many later tests.

The parser's public facade is `crates/tsz-parser/src/lib.rs`; implementation
lives under `crates/tsz-parser/src/parser/` and syntax definitions under
`crates/tsz-parser/src/syntax/`.

## Binder

`crates/tsz-binder` owns declarations, scopes, symbols, module surfaces,
hoisting, and the control-flow skeleton. It stabilizes semantic identity inputs
such as `SymbolId`, `ScopeId`, `FlowNodeId`, and declaration summaries. It must
not compute type relations or inference.

The binder's state machinery starts at `crates/tsz-binder/src/state/`; binding
passes live under `crates/tsz-binder/src/binding/`; module import/export work
lives under `crates/tsz-binder/src/modules/`.

## Checker

`crates/tsz-checker` owns AST orchestration. It decides when to ask semantic
questions, how to account for source context, where diagnostics attach, and how
diagnostic suppression/priority works. It should not grow a parallel semantic
kernel; semantic answers should cross `crates/tsz-checker/src/query_boundaries/`.

Checker work fans out through:

- `crates/tsz-checker/src/state/` for whole-file/program checking state.
- `crates/tsz-checker/src/checkers/` for syntax-family checkers.
- `crates/tsz-checker/src/assignability/` for checker-side relation gateway and
  diagnostic mapping.
- `crates/tsz-checker/src/flow/` for flow graph construction and analysis.
- `crates/tsz-checker/src/error_reporter/` for diagnostic rendering helpers.

## Solver

`crates/tsz-solver` owns semantic facts. It knows type construction, type
queries, relations, inference, instantiation, operations, narrowing, semantic
caches, and compatibility policy. It returns semantic answers or structured
failure reasons; it does not choose source spans or user-facing diagnostic order.

Important solver areas include `crates/tsz-solver/src/relations/`,
`crates/tsz-solver/src/evaluation/`, `crates/tsz-solver/src/inference/`,
`crates/tsz-solver/src/instantiation/`, `crates/tsz-solver/src/operations/`,
and `crates/tsz-solver/src/type_queries/`.

## Emitter

`crates/tsz-emitter` owns output. It consumes AST plus checked summaries and
produces JavaScript, declaration output, and source maps. It should not perform
semantic validation. If emit needs a semantic fact, that fact should arrive as a
precomputed summary or cache view.

Key areas are `crates/tsz-emitter/src/lowering/`,
`crates/tsz-emitter/src/emitter/`, `crates/tsz-emitter/src/transforms/`,
`crates/tsz-emitter/src/declaration_emitter/`, and
`crates/tsz-emitter/src/output/`.

## Consumers

The pipeline is consumed by:

- `crates/tsz-core` as a public facade and common integration layer.
- `crates/tsz-cli` for `tsz`, `try_tsz`, `tsz_lsp`, and audit binaries.
- `crates/tsz-lsp` for editor features.
- `crates/tsz-wasm` for browser and JavaScript integration.
- `crates/tsz-website` for the public site and playground.
- `crates/conformance`, `scripts/conformance/`, `scripts/emit/`, and
  `scripts/fourslash/` for parity gates.
