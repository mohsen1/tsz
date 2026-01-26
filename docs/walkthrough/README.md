# TSZ Code Walkthrough

> **Audience**: Developers working on the TSZ TypeScript compiler
> **Depth**: Detailed implementation-level documentation

TSZ ("Zang" - Persian for "rust") is a high-performance TypeScript compiler written in Rust, targeting drop-in compatibility with `tsc`. This walkthrough provides deep technical documentation of the compiler architecture, implementation details, and known gaps.

## Current Status

- **Conformance**: 37.0% (4,508 / 12,197 tests passing)
- **Codebase**: ~356,000 lines of Rust
- **Target**: 60%+ conformance via systematic improvements

## Compilation Pipeline

```
Source Code (.ts/.tsx)
       │
       ▼
┌─────────────────┐
│    Scanner      │  Tokenization: Source → Token Stream
│  scanner*.rs    │  ~2,800 LOC
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│    Parser       │  Parsing: Tokens → AST (NodeArena)
│  parser/        │  ~11,000 LOC
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│    Binder       │  Binding: AST → Symbol Table + Control Flow Graph
│  binder/        │  ~5,000 LOC
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│    Checker      │  Type Checking: Symbols → Type Errors
│  checker/       │  ~61,000 LOC
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│    Solver       │  Type System: Subtyping, Inference, Evaluation
│  solver/        │  ~142,000 LOC
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│    Emitter      │  Code Generation: AST → JavaScript
│  emitter/       │  ~19 files, ~71KB
└─────────────────┘
```

## Module Documentation

| Module | Documentation | Key Responsibility |
|--------|--------------|-------------------|
| Scanner | [01-scanner.md](./01-scanner.md) | Lexical analysis, tokenization |
| Parser | [02-parser.md](./02-parser.md) | Syntax analysis, AST construction |
| Binder | [03-binder.md](./03-binder.md) | Symbol table, scope management, CFG |
| Checker | [04-checker.md](./04-checker.md) | Type checking orchestration |
| Solver | [05-solver.md](./05-solver.md) | Type system core (subtyping, inference) |
| Emitter | [06-emitter.md](./06-emitter.md) | JavaScript code generation |

## Gaps Summary

See [07-gaps-summary.md](./07-gaps-summary.md) for a consolidated view of all implementation gaps, TODOs, and incomplete features across the codebase.

## Key Architectural Patterns

### 1. Arena-Based Memory Management

AST nodes are allocated in a `NodeArena` using bump allocation:
- 16-byte thin nodes (vs 208-byte fat nodes in naive design)
- 13x better cache locality (4 nodes per 64-byte cache line)
- Referenced by `NodeIndex` (u32) instead of pointers
- Enables safe parallel processing

```rust
// src/parser/node.rs - Node struct
pub struct Node {
    pub kind: u16,       // SyntaxKind
    pub flags: u16,      // NodeFlags
    pub pos: u32,        // Start position
    pub end: u32,        // End position
    pub data_index: u32, // Index into typed data pool
}
```

### 2. Type Interning with O(1) Equality

All types go through `TypeInterner` for structural deduplication:
- Same structure = same `TypeId` (u32)
- O(1) equality: `type_a == type_b`
- Enables efficient caching and memoization

```rust
// src/solver/types.rs - TypeKey enum
pub enum TypeKey {
    Intrinsic(IntrinsicKind),
    Literal(LiteralValue),
    Union(TypeListId),
    Object(ObjectShapeId),
    // ... 20+ variants
}
```

### 3. Coinductive Subtyping

Recursive types are handled via Greatest Fixed Point semantics:
- Cycle detection prevents infinite expansion
- `Provisional` result for in-progress pairs
- Mathematically sound handling of `type List<T> = { next: List<T> }`

```rust
// src/solver/subtype.rs - coinductive cycle handling
// When pair (source, target) is revisited during checking:
// Return Provisional (assume true) to break the cycle
```

### 4. Query-Based Type Checking

Leverages Salsa-style incremental computation:
- `TypeDatabase` trait for type interning/lookup
- `QueryDatabase` trait for type operations
- Memoization of expensive computations
- Future: Full Salsa integration for incremental recompilation

### 5. Visitor Pattern for Type Operations

Replaces repetitive match statements with composable visitors:

```rust
// src/solver/visitor.rs
pub trait TypeVisitor {
    fn visit_intrinsic(&mut self, kind: IntrinsicKind);
    fn visit_union(&mut self, types: &[TypeId]) { /* default */ }
    // ... extensible for new operations
}
```

### 6. Zero-Copy Source Text

Scanner uses `Arc<str>` for shared source ownership:
- No duplication across phases
- String slices for token text: `&source[start..end]`
- String interning for identifiers via `Atom` (u32)

## Critical Limits

These constants prevent pathological cases from causing hangs or OOM:

| Constant | Value | Location | Purpose |
|----------|-------|----------|---------|
| `MAX_SUBTYPE_DEPTH` | 100 | solver/subtype.rs | Recursion limit |
| `MAX_TOTAL_SUBTYPE_CHECKS` | 100,000 | solver/subtype.rs | Total checks per instance |
| `MAX_INSTANTIATION_DEPTH` | 50 | checker/state.rs | Generic instantiation |
| `MAX_CALL_DEPTH` | 20 | checker/type_computation.rs | Function call nesting |
| `MAX_EMIT_RECURSION_DEPTH` | 1000 | emitter/mod.rs | Code generation |
| `MAX_RECURSION_DEPTH` | 1000 | parser/state.rs | Parser recursion |
| `MAX_TYPE_RESOLUTION_OPS` | 500,000 | checker/context.rs | Fuel counter |

## Cross-References

This documentation complements:
- [specs/SOLVER.md](../specs/SOLVER.md) - Mathematical foundations of type solver
- [specs/TS_UNSOUNDNESS_CATALOG.md](../specs/TS_UNSOUNDNESS_CATALOG.md) - TypeScript compatibility rules
- [docs/TYPE_VISITOR_PATTERN_GUIDE.md](../TYPE_VISITOR_PATTERN_GUIDE.md) - Visitor pattern usage
- [PROJECT_DIRECTION.md](../../PROJECT_DIRECTION.md) - Conformance improvement plan
- [AGENTS.md](../../AGENTS.md) - Architecture rules for contributors

## Navigation Tips

- **File references** point to relevant source files
- **⚠️ GAP** markers indicate incomplete implementations
- **📍 KEY** markers highlight critical code locations
- Use `rg` or IDE search with function/struct names to locate implementations

---

*Last updated: January 2026*
