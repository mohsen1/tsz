# Front End: Scanner, Parser, Binder

The front end turns text into syntax and stable declaration structure. It should
not answer type-compatibility questions.

## Scanner

`crates/tsz-scanner` owns lexical facts:

- token boundaries and token kinds;
- trivia and comments as needed by later layers;
- escape handling and string/template scanning;
- rescan behavior for ambiguous token sequences;
- scanner state and token flags.

Primary files:

- `crates/tsz-scanner/src/lib.rs`
- `crates/tsz-scanner/src/scanner_impl.rs`
- `crates/tsz-scanner/src/rescan.rs`
- `crates/tsz-scanner/src/char_codes.rs`

Scanner tests live in `crates/tsz-scanner/tests/` and scanner-facing regression
tests are also re-exported through `crates/tsz-core/src/lib.rs`.

## Parser

`crates/tsz-parser` owns syntax-only AST production:

- `crates/tsz-parser/src/parser/` contains parser state, node construction,
  flags, lists, recovery helpers, and grammar-family modules.
- `crates/tsz-parser/src/syntax/` contains syntax kinds and syntax-facing data.
- `NodeArena`, `NodeIndex`, `NodeList`, and `TextRange` are parser-facing public
  concepts.

Parser diagnostics are syntactic. The parser can say an expression was expected;
it cannot decide whether two types are assignable.

## Binder

`crates/tsz-binder` owns the declaration world:

- `crates/tsz-binder/src/binding/` walks declarations and expressions to create
  symbols, semantic definitions, validation state, and flow edges.
- `crates/tsz-binder/src/state/` stores binder state, declaration summaries,
  exports, lib merging, resolution helpers, and tests.
- `crates/tsz-binder/src/modules/` handles import/export surfaces and module
  binding.
- `crates/tsz-binder/src/nodes/` contains node-level binding helpers.
- `crates/tsz-binder/src/flow.rs` defines flow-node data.
- `crates/tsz-binder/src/scopes.rs` and `crates/tsz-binder/src/symbols.rs`
  define scope and symbol arenas.

Binder outputs are consumed by checker, solver-backed type evaluation,
declaration emit, LSP, and project APIs. A binder mistake often appears later as
an unresolved-name diagnostic, missing global, wrong export surface, or
cross-file identity bug.

## What The Front End Must Not Do

The front end should not:

- decide assignability;
- perform generic inference;
- instantiate mapped or conditional types;
- compare object shapes for compatibility;
- render semantic diagnostics from printed type strings;
- introduce user-name- or file-name-based semantic shortcuts.

When a front-end issue seems to require type facts, document the boundary and
move the semantic question into solver or a solver-backed query boundary.
