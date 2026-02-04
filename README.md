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
Progress: [████░░░░░░░░░░░░] 39.4% (5292/13443 tests)
```
<!-- CONFORMANCE_END -->

### Language Service

We run TypeScript's fourslash language service tests against `tsz-server` to measure
language service feature coverage (completions, quickinfo, go-to-definition, etc.).

<!-- FOURSLASH_START -->
```
Progress: [██░░░░░░░░░░░░░░░░░░] 11.4% (747 / 6,563 tests)
```
<!-- FOURSLASH_END -->

### Emit

We compare tsz JavaScript/declaration emit output against TypeScript's baseline files
to ensure correct code generation.

<!-- EMIT_START -->
```
JavaScript:  [███░░░░░░░░░░░░░░░░░] 12.8% (1,453 / 11,353 tests)
Declaration: [░░░░░░░░░░░░░░░░░░░░] 0.0% (0 / 0 tests)
```
<!-- EMIT_END -->

## Documentation

- [Development Guide](docs/DEVELOPMENT.md) - Setup, building, and contributing
- [Testing Guide](docs/TESTING.md) - Testing infrastructure details
- [Benchmarks](docs/BENCHMARKS.md) - Performance benchmarking

## AI Assistant Integration

This project includes a [Claude Code skill](.claude/skills/tsz-gemini/SKILL.md) that integrates with Gemini AI to help you understand the codebase. The skill automatically gathers relevant files and code context to answer questions about architecture and implementation.

**Quick Start:**
```bash
# Ask about type inference (uses --solver preset)
./scripts/ask-gemini.mjs --solver "How does type inference work for generic functions?"

# Ask about error reporting (uses --checker preset)
./scripts/ask-gemini.mjs --checker "How are diagnostics reported?"

# Ask about specific files
./scripts/ask-gemini.mjs --include=src/solver/infer.rs "How does this file handle type inference?"

# List all available presets
./scripts/ask-gemini.mjs --list
```

**Environment Setup:**
```bash
# For Vertex AI Express (default)
export GCP_VERTEX_EXPRESS_API_KEY="your-key-here"

# OR for direct Gemini API (fallback)
export GEMINI_API_KEY="your-key-here"
```

See [`.claude/skills/tsz-gemini/SKILL.md`](.claude/skills/tsz-gemini/SKILL.md) for detailed usage instructions and examples.

---

<a id="footnote-1">1</a>: "Zang" is the Persian word for "rust".
