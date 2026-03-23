# `tsz`

`tsz` is a performance-first TypeScript compiler in Rust. _z_ is for _Zang_!<sup>[1](#footnote-1)</sup>
The goal is a correct, fast, drop-in replacement for `tsc`, with both native and WASM targets.

`tsz` is built the with help of AI-assistant coding. Many tools and AI models were used during its development.

TypeScript is intentionally unsound. `tsz` keeps a sound core solver and layers a compatibility
engine on top to match TypeScript behavior while preserving correctness where possible.

### Status

> [!WARNING]
> This project is not ready for general use yet.

`tsz` will be released after TypeScript 6 stable is released. `tsz` will only be compatible with TypeScript 6, not any older versions.

<!-- TS_VERSION_START -->
Currently targeting `TypeScript`@`6.0.1-rc`
<!-- TS_VERSION_END -->

## Progress

### Type Checker

To ensure tsz is a drop-in replacement for `tsc`, we run the official TypeScript conformance
test suite against it.


<!-- CONFORMANCE_START -->
```
Progress: [██████████████████░░] 88.9% (11,180/12,581 tests)
```
<!-- CONFORMANCE_END -->

Conformance is measured by diagnostic fingerprint comparison: each diagnostic must match tsc in
error code, file, line, column, and message.

### Emitter

We compare tsz JavaScript/declaration emit output against TypeScript's baseline files
to ensure correct code generation.

<!-- EMIT_START -->
```
JavaScript:  [██████████████████░░] 89.2% (12,062 / 13,526 tests)
Declaration: [██████████████░░░░░░] 72.6% (1,203 / 1,658 tests)
```
<!-- EMIT_END -->

### Language Service

We run TypeScript's fourslash language service tests against `tsz-server` to measure
language service feature coverage (completions, quickinfo, go-to-definition, etc.).

<!-- FOURSLASH_START -->
```
Progress: [██████████████░░░░░░] 71.0% (4,487 / 6,320 tests)
```
<!-- FOURSLASH_END -->


<a id="footnote-1">1</a>: "Zang" is the Persian word for "rust".
