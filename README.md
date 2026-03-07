# Project Zang

Project Zang is a performance-first TypeScript compiler in Rust.<sup>[1](#footnote-1)</sup>
The goal is a correct, fast, drop-in replacement for `tsc`, with both native and WASM targets.

TypeScript is intentionally unsound. Zang keeps a sound core solver and layers a compatibility
engine on top to match TypeScript behavior while preserving correctness where possible.

## Progress

> [!WARNING]
> This project is not ready for general use yet.

<!-- TS_VERSION_START -->
Currently targeting `TypeScript`@`6.0.0-dev.20260306`
<!-- TS_VERSION_END -->

### Type Checker

To ensure tsz is a drop-in replacement for `tsc`, we run the official TypeScript conformance
test suite against it.


<!-- CONFORMANCE_START -->
```
Progress: [████████████████░░░░] 80.4% (10,112/12,581 tests)
```
<!-- CONFORMANCE_END -->

Conformance is measured by diagnostic fingerprint comparison: each diagnostic must match tsc in
error code, file, line, column, and message.

### Emitter

We compare tsz JavaScript/declaration emit output against TypeScript's baseline files
to ensure correct code generation.

<!-- EMIT_START -->
```
JavaScript:  [████████████████░░░░] 81.3% (11,003 / 13,526 tests)
Declaration: [████████████░░░░░░░░] 58.9% (872 / 1,480 tests)
```
<!-- EMIT_END -->

### Language Service

We run TypeScript's fourslash language service tests against `tsz-server` to measure
language service feature coverage (completions, quickinfo, go-to-definition, etc.).

<!-- FOURSLASH_START -->
```
Progress: [████████████████████] 99.6% (2,530 / 2,540 tests)
```
<!-- FOURSLASH_END -->


<a id="footnote-1">1</a>: "Zang" is the Persian word for "rust".
