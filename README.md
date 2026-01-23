# Project Zang

Project Zang is a performance-first TypeScript compiler in Rust.<sup>[1](#footnote-1)</sup>
The goal is a correct, fast, drop-in replacement for `tsc`, with both native and WASM targets.

TypeScript is intentionally unsound. Zang keeps a sound core solver and layers a compatibility
engine on top to match TypeScript behavior while preserving correctness where possible.


## Status

Work in progress.

This project is not ready for general use yet.


## Development

### Getting Started

```bash
# Clone the repository
git clone https://github.com/mohsen1/tsz.git
cd tsz

# Build the project (also installs git hooks automatically)
cargo build
```

### Git Hooks

Pre-commit hooks are automatically installed on first build. They run:
- `cargo fmt` - Format code
- `cargo clippy --fix` - Lint and auto-fix issues
- Unit tests via `scripts/test.sh`

To manually install hooks:
```bash
./scripts/install-hooks.sh
```

To skip hooks for a single commit (not recommended):
```bash
git commit --no-verify
```

---

<a id="footnote-1">1</a>: "Zang" is the Persian word for "rust".