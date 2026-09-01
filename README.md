<br>
<br>

<p align="center">
	<picture>
		<source media="(prefers-color-scheme: dark)" srcset="crates/tsz-website/static/tsz_logo_dark.png">
		<source media="(prefers-color-scheme: light)" srcset="crates/tsz-website/static/tsz_logo_light.png">
		<img src="crates/tsz-website/static/tsz_logo_light.png" alt="tsz logo" width="200">
	</picture>
</p>

<br>
<br>

`tsz` is a clean-slate TypeScript compiler experiment written in Rust. It
targets the pinned TypeScript `7.0.2` compiler as its behavioral oracle. _z_ is
for _Zang_, the Persian word for “rust.”

The eventual goal is exact TypeScript compatibility and at least 3x the
throughput of `tsgo` on every project that is already correct. A project with
different diagnostics, incomplete dependencies, or an unsupported compiler
surface is not a performance win.

## Current status: R0

<!-- R0_STATUS_START -->
> [!WARNING]
> The rewrite is validation-only. There is no supported install, package
> release, WASM build, or drop-in replacement yet. Build it from source only
> when working on the compiler or its validation harnesses.

The fresh vertical slice currently proves exact seed behavior for:

- declarations, literal inference, and `let`/`var` widening;
- explicit annotations and assignment diagnostics;
- function calls, arguments, and return diagnostics;
- object properties and a bounded union subset;
- JavaScript emit for the seed syntax;
- deterministic diagnostics across repeated runs and reversed root-file order.

Exact seed assertions cover diagnostic codes, spans, messages, ordering, exit
status, and emitted bytes against TypeScript `7.0.2`.
<!-- R0_STATUS_END -->

Broad TypeScript is deliberately unsupported at R0. This includes substantial
parser recovery, configuration and module resolution, standard library loading,
classes, generics, flow analysis, advanced and recursive types, declaration
emit, source maps, incremental language-service behavior, full LSP/fourslash,
project-corpus compatibility, releases, and WASM.

For the execution plan and architectural invariants, read
[`docs/plan/ROADMAP.md`](docs/plan/ROADMAP.md) and
[`docs/architecture/RESET.md`](docs/architecture/RESET.md).

## Build and validate

The native binaries are development artifacts, not releases:

```sh
cargo run -p tsz-cli --bin tsz -- --help
```

Run the exact seed oracle and focused Rust suites with:

```sh
scripts/reset/seed-oracle.sh --tsz .target/debug/tsz
cargo nextest run --workspace --all-features
```

The conformance, emit, fourslash, project, and performance harnesses remain in
the repository. During R0, broad results are observations of unsupported work,
not a compatibility percentage or speed claim.

## Frozen legacy checkpoint

The table below records the retired implementation at parent checkpoint
`2770da88d4` on 2026-08-20. These are historical evidence only. They are not
rewrite results, not an R0 floor, and not refreshed from current artifacts.

| Retired suite | Frozen result |
| --- | ---: |
| Diagnostic conformance | 11,667 / 12,043 runnable cases (96.9%) |
| JavaScript emit | 11,562 / 11,563 cases |
| Declaration emit | 1,377 / 1,390 cases (99.1%) |
| Fourslash | 6,562 / 6,562 cases |

The old benchmark images and published dashboards are likewise frozen
pre-reset artifacts. New performance reporting resumes only for rewrite rows
that first match the pinned TypeScript oracle.

## Project principles

- TypeScript `7.0.2` behavior is the compatibility source of truth.
- Deferred semantic forms remain symbolic until their owning operation forces
  them.
- One checker owns one type universe; incomplete work does not enter definitive
  caches.
- Correctness precedes speed, and every public metric carries provenance.
- Sound Mode was removed. There is one TypeScript-compatible semantics.

`tsz` is developed with AI-assisted coding and reviewed through executable
oracle, unit, architecture, and process contracts.
