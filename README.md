
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


`tsz` is a performance-first TypeScript compiler in Rust. _z_ is for _Zang_!<sup>[1](#footnote-1)</sup>
The goal is a correct, fast, drop-in replacement for `tsc`, with both native and WASM targets.

`tsz` is built the with help of AI-assistant coding. Many tools and AI models were used during its development.

TypeScript is intentionally unsound. `tsz` keeps a sound core solver and layers a compatibility
engine on top to match TypeScript behavior while preserving correctness where possible.

## Performance

`tsz` is **2.21x faster** across the current successful micro benchmark
snapshot. Large-project performance work is still underway; the release target
is for eligible green project rows to be at least **2x faster than `tsgo`** in
the canonical benchmark artifact.

```
tsz:  [█████████░░░░░░░░░░░] 3s
tsgo: [████████████████████] 7s
```

## Install

> [!WARNING]
> `tsz` is pre-release software and not yet a drop-in replacement for `tsc`.
> Diagnostics, inference, and emit may differ from TypeScript today. Use for
> experimentation only.

**macOS & Linux**

```sh
curl -fsSL https://tsz.dev/install | sh
```

**Windows (PowerShell)**

```powershell
irm https://tsz.dev/install.ps1 | iex
```

## TypeScript compatibility
<!-- TS_VERSION_START -->
Currently targeting `TypeScript`@`6.0.3`
<!-- TS_VERSION_END -->
### Type Checker

To ensure tsz is a drop-in replacement for `tsc`, we run the official TypeScript conformance
test suite against it.


<!-- CONFORMANCE_START -->
```
Progress: [████████████████████] 100.0% (12,582/12,582 tests)
```
<!-- CONFORMANCE_END -->

Conformance is measured by diagnostic fingerprint comparison: each diagnostic must match tsc in
error code, file, line, column, and message.

The checked-in detail snapshot is exact, but release conformance also tracks
the accepted-regression strictness list separately until that deficit reaches
zero.

### Emitter

We compare tsz JavaScript/declaration emit output against TypeScript's baseline files
to ensure correct code generation.

<!-- EMIT_START -->
```
JavaScript:  [███████████████████░] 96.8% (13,094 / 13,530 tests)
Declaration: [███████████████████░] 96.2% (1,606 / 1,669 tests)
```
<!-- EMIT_END -->

This block is generated from the checked-in emit artifact with
`python3 scripts/refresh-readme.py --write`; release claims should cite the
current CI artifact.

### Language Service

We run TypeScript's fourslash language service tests against `tsz-server` to measure
language service feature coverage (completions, quickinfo, go-to-definition, etc.).

<!-- FOURSLASH_START -->
```
Progress: [████████████████████] 99.9% (6,558 / 6,562 tests)
```
<!-- FOURSLASH_END -->


<a id="footnote-1">1</a>: "Zang" is the Persian word for "rust".
