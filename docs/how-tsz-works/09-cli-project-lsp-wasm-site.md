# CLI, Project, LSP, WASM, And Site

Compiler layers are consumed by user-facing and automation-facing surfaces. They
should orchestrate projects and presentation, not own duplicate type semantics.

## Core Facade

`crates/tsz-core` is the public facade. It re-exports scanner, parser, binder,
checker, emitter, solver, LSP, common utilities, config, embedded libraries,
module resolution, source files, parallel support, and WASM-oriented helpers.

Important paths:

- `crates/tsz-core/src/lib.rs`
- `crates/tsz-core/src/config/`
- `crates/tsz-core/src/module_resolver/`
- `crates/tsz-core/src/source_file/`
- `crates/tsz-core/src/embedded_libs/`
- `crates/tsz-core/src/parallel/`
- `crates/tsz-core/src/api/wasm/`

## CLI

`crates/tsz-cli` owns command-line and watch/project orchestration:

- `crates/tsz-cli/src/bin/tsz.rs` is the main CLI binary.
- `crates/tsz-cli/src/bin/try_tsz.rs` powers the `try-tsz` experiment path.
- `crates/tsz-cli/src/bin/tsz_lsp.rs` runs the language-server binary.
- `crates/tsz-cli/src/bin/audit_unsoundness.rs` exposes unsoundness audit flows.
- `crates/tsz-cli/src/commands/`, `driver/`, `project/`, and `reporting/`
  contain argument, build, watch, filesystem, incremental, reporter, and tracing
  logic.

The CLI reports diagnostics through checker/common diagnostic surfaces. It
should not own diagnostic algorithms.

## Project Orchestration

Project APIs gather files, compiler options, libraries, module resolution, and
incremental state before invoking the pipeline. Project code is allowed to know
about workspace state and filesystem shape; it is not allowed to decide type
relations.

Important project-facing paths include `crates/tsz-cli/src/project/`,
`crates/tsz-core/src/module_resolver/`, and LSP project state under
`crates/tsz-lsp/src/project/`.

## LSP

`crates/tsz-lsp` exposes editor features:

- diagnostics;
- completions;
- hover;
- navigation and references;
- rename and linked editing;
- code actions;
- formatting;
- semantic tokens and highlighting;
- symbols, hierarchy, document links, inlay hints, and color support;
- fourslash helpers and variants for test harness integration.

LSP features consume parse/bind/check/project APIs. They should not reimplement
semantic algorithms locally.

## WASM

`crates/tsz-wasm` adapts core APIs to WebAssembly:

- `crates/tsz-wasm/src/lib.rs` re-exports core APIs and public WASM boundary
  types.
- `crates/tsz-wasm/src/wasm_api/` contains compile, diagnostic, parser, and
  transform adapters.
- `crates/tsz-wasm/js/` contains JavaScript-side packaging helpers.

The WASM boundary serializes and packages compiler results; it should keep
semantic behavior shared with native code.

## Website

`crates/tsz-website` builds the public site and playground:

- `crates/tsz-website/src/` contains Eleventy pages, data modules, playground
  source, sound-mode page code, bundled lib declarations, and site assets.
- `crates/tsz-website/static/` contains generated/static browser assets.
- `crates/tsz-website/scripts/` contains website generation/sync scripts.
- `docs/site/` contains source documentation consumed by the site.

The site displays benchmark, compatibility, install, and architecture-facing
information. It is a product surface over the compiler, not a compiler layer.
