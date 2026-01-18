# Project Zang

Project Zang is a performance-first TypeScript compiler in Rust.<sup>[1](#footnote-1)</sup>
The goal is a correct, fast, drop-in replacement for `tsc`, with both native and WASM targets.

TypeScript is intentionally unsound. Zang keeps a sound core solver and layers a compatibility
engine on top to match TypeScript behavior while preserving correctness where possible.


## Status
This project is not ready for general use yet. The interface and distribution are in progress.

---

<a id="footnote-1">1</a>: "Zang" is the Persian word for "rust".