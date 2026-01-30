# Project Zang

Project Zang is a performance-first TypeScript compiler in Rust.<sup>[1](#footnote-1)</sup>
The goal is a correct, fast, drop-in replacement for `tsc`, with both native and WASM targets.

TypeScript is intentionally unsound. Zang keeps a sound core solver and layers a compatibility
engine on top to match TypeScript behavior while preserving correctness where possible.

## Conformance Progress

To ensure tsz is a drop-in replacement for `tsc`, we run the official TypeScript conformance
test suite against it.

<!-- CONFORMANCE_START -->
Currently targeting `TypeScript`@`6.0.0-dev.20260116`

```
Progress: [██████████░░░░░░░░░░] 48.7% (6,027 / 12,379 tests)
```
<!-- CONFORMANCE_END -->

## Status

Work in progress.

This project is not ready for general use yet.

## Documentation

- [Development Guide](docs/DEVELOPMENT.md) - Setup, building, and contributing
- [Testing Guide](docs/TESTING.md) - Testing infrastructure details
- [Benchmarks](docs/BENCHMARKS.md) - Performance benchmarking

---

<a id="footnote-1">1</a>: "Zang" is the Persian word for "rust".
