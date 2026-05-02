
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

### Status

> [!NOTE]
> **Nearly complete.** TypeScript support is in its final compiler stages, with remaining work focused on performance tuning and LSP support in WebAssembly.

`tsz` will be released after TypeScript 6 stable is released. `tsz` will only be compatible with TypeScript 6, not any older versions.

<!-- TS_VERSION_START -->
Currently targeting `TypeScript`@`6.0.3`
<!-- TS_VERSION_END -->

## Progress

### Type Checker

To ensure tsz is a drop-in replacement for `tsc`, we run the official TypeScript conformance
test suite against it.


<!-- CONFORMANCE_START -->
```
Progress: [████████████████████] 98.2% (12,351/12,582 tests)
```
<!-- CONFORMANCE_END -->

Conformance is measured by diagnostic fingerprint comparison: each diagnostic must match tsc in
error code, file, line, column, and message.

### Emitter

We compare tsz JavaScript/declaration emit output against TypeScript's baseline files
to ensure correct code generation.

<!-- EMIT_START -->
```
JavaScript:  [██████████████████░░] 92.1% (12,458 / 13,526 tests)
Declaration: [████████████████░░░░] 81.6% (1,362 / 1,670 tests)
```
<!-- EMIT_END -->

### Language Service

We run TypeScript's fourslash language service tests against `tsz-server` to measure
language service feature coverage (completions, quickinfo, go-to-definition, etc.).

<!-- FOURSLASH_START -->
```
Progress: [████████████████████] 100.0% (6,562 / 6,562 tests)
```
<!-- FOURSLASH_END -->


<a id="footnote-1">1</a>: "Zang" is the Persian word for "rust".
