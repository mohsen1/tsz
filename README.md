# Project Zang

Project Zang is a performance-first TypeScript compiler in Rust.<sup>[1](#footnote-1)</sup>
The goal is a correct, fast, drop-in replacement for `tsc`, with both native and WASM targets.

TypeScript is intentionally unsound. Zang keeps a sound core solver and layers a compatibility
engine on top to match TypeScript behavior while preserving correctness where possible.

## Progress

> [!WARNING]
> This project is not ready for general use yet.

<!-- TS_VERSION_START -->
