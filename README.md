# Project Zang

Project Zang is a performance-first TypeScript compiler in Rust.<sup>[1](#footnote-1)</sup>
The goal is a correct, fast, drop-in replacement for `tsc`, with both native and WASM targets.

TypeScript is intentionally unsound. Zang keeps a sound core solver and layers a compatibility
engine on top to match TypeScript behavior while preserving correctness where possible.

## Progress

> [!WARNING]
> This project is not ready for general use yet.

<!-- TS_VERSION_START -->
Currently targeting `TypeScript`@`6.0.0-dev.20260116`
<!-- TS_VERSION_END -->

### Type Checker

To ensure tsz is a drop-in replacement for `tsc`, we run the official TypeScript conformance
test suite against it.

<!-- CONFORMANCE_START -->
```
Progress: [█████████░░░░░░░░░░░] 46.7% (5,785 / 12,379 tests)
```
<!-- CONFORMANCE_END -->

### Language Service

We run TypeScript's fourslash language service tests against `tsz-server` to measure
language service feature coverage (completions, quickinfo, go-to-definition, etc.).

<!-- FOURSLASH_START -->
```
Progress: [██░░░░░░░░░░░░░░░░░░] 11.1% (728 / 6,563 tests)
```
<!-- FOURSLASH_END -->

### Emit

We compare tsz JavaScript/declaration emit output against TypeScript's baseline files
to ensure correct code generation.

<!-- EMIT_START -->
```
JavaScript:  [██░░░░░░░░░░░░░░░░░░] 8.3% (37 / 448 tests)
Declaration: [░░░░░░░░░░░░░░░░░░░░] 0.0% (0 / 0 tests)
```
<!-- EMIT_END -->

## Documentation

- [Development Guide](docs/DEVELOPMENT.md) - Setup, building, and contributing
- [Testing Guide](docs/TESTING.md) - Testing infrastructure details
- [Benchmarks](docs/BENCHMARKS.md) - Performance benchmarking

---

<a id="footnote-1">1</a>: "Zang" is the Persian word for "rust".
