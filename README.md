# Project Zang

Project Zang is a performance-first TypeScript compiler in Rust.<sup>[1](#footnote-1)</sup>
The goal is a correct, fast, drop-in replacement for `tsc`, with both native and WASM targets.

TypeScript is intentionally unsound. Zang keeps a sound core solver and layers a compatibility
engine on top to match TypeScript behavior while preserving correctness where possible.

## Conformance Progress

<!-- CONFORMANCE_START -->
| Metric | Value |
|--------|-------|
| **TypeScript Version** | `6.0.0-dev.20260116` |
| **Tests Passed** | 0 / 0 |
| **Pass Rate** | 0.0% |

```
Progress: [░░░░░░░░░░░░░░░░░░░░] 0.0%
```

*Automatically updated by CI on each push to main*
<!-- CONFORMANCE_END -->

## Status

Work in progress.

This project is not ready for general use yet.

## Documentation

- [Development Guide](docs/DEVELOPMENT.md) - Setup, building, and contributing
- [Testing Guide](docs/TESTING.md) - Testing infrastructure details
- [Benchmarks](docs/BENCHMARKS.md) - Performance benchmarking

## Quick Start

```bash
git clone https://github.com/mohsen1/tsz.git
cd tsz
cargo build
```

See the [Development Guide](docs/DEVELOPMENT.md) for detailed setup instructions.

---

<a id="footnote-1">1</a>: "Zang" is the Persian word for "rust".
