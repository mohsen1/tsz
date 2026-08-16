
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

## Performance

`tsz` is aiming to be 2x faster than tsgo on all benchmarks. It is 3x faster in small file samples. Work on larger projects is underway.
<!-- PERFORMANCE_START -->
<p align="left">
  <a href="https://tsz.dev/benchmarks/">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="crates/tsz-website/static/benchmark-data/readme-perf-dark.png">
      <source media="(prefers-color-scheme: light)" srcset="crates/tsz-website/static/benchmark-data/readme-perf-light.png">
      <img src="crates/tsz-website/static/benchmark-data/readme-perf-light.png" alt="Latest tsz vs tsgo benchmark performance" width="760">
    </picture>
  </a>
</p>
<!-- PERFORMANCE_END -->

## Install

> [!WARNING]
> `tsz` is pre-release software and not yet a drop-in replacement for `tsc`.
> Diagnostics, inference, and emit may differ from TypeScript today. Use for
> experimentation only.

To check whether `tsz` currently matches `tsc` on your project:

```sh
npx try-tsz
```

**macOS & Linux**

```sh
curl -fsSL https://tsz.dev/install | sh
```

**Windows (PowerShell)**

```powershell
irm https://tsz.dev/install.ps1 | iex
```

## TypeScript compatibility

`tsz` runs TypeScript's own test suite for compatibility across type-checking, code emission, and LSP.
<!-- TS_VERSION_START -->
Currently targeting `TypeScript`@`7.0.2`
<!-- TS_VERSION_END -->
### Type Checker

To ensure tsz is a drop-in replacement for `tsc`, we run the official TypeScript conformance
test suite against it.


<!-- CONFORMANCE_START -->
```
Progress: [███████████████████░] 96.8% (11,657/12,043 runnable tests)
Candidates: 12,585 (12,043 runnable, 507 unsupported, 35 skipped)
```
<!-- CONFORMANCE_END -->


### Emitter


<!-- EMIT_START -->
```
JavaScript:  [████████████████████] 100.0% (11,562 / 11,563 tests)
Declaration: [████████████████████] 99.1% (1,377 / 1,390 tests)
```
<!-- EMIT_END -->

### Language Service

<!-- FOURSLASH_START -->
```
Progress: [████████████████████] 100.0% (6,562 / 6,562 tests)
```
<!-- FOURSLASH_END -->


<a id="footnote-1">1</a>: "Zang" is the Persian word for "rust".
