# TSZ Code Walkthrough

> **Audience**: Developers working on the TSZ TypeScript compiler
> **Depth**: Detailed implementation-level documentation

TSZ ("Zang" - Persian for "rust") is a high-performance TypeScript compiler written in Rust, targeting drop-in compatibility with `tsc`. This walkthrough provides deep technical documentation of the compiler architecture, implementation details, and known gaps.

## Current Status

- **Codebase**: ~600,000 lines of Rust
- **Target**: Systematic conformance improvements

## Compilation Pipeline

```
Source Code (.ts/.tsx)
       │
       ▼
┌─────────────────┐
│    Scanner      │  Tokenization: Source → Token Stream
│  scanner/       │  ~3,500 LOC
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│    Parser       │  Parsing: Tokens → AST (NodeArena)
│  parser/        │  ~18,000 LOC
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│    Binder       │  Binding: AST → Symbol Table + Control Flow Graph
│  binder/        │  ~6,000 LOC
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│    Checker      │  Type Checking: Symbols → Type Errors
│  checker/       │  ~69,000 LOC
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│    Solver       │  Type System: Subtyping, Inference, Evaluation
│  solver/        │  ~165,000 LOC
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│    Emitter      │  Code Generation: AST → JavaScript
│  emitter/       │  19 files, ~8,600 LOC
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

**Key types in `solver/types.rs`:**
- `TypeId` - lightweight handle with built-in constants (`NONE`, `ERROR`, `NEVER`, `UNKNOWN`, `ANY`, `VOID`, `UNDEFINED`, `NULL`, `BOOLEAN`, `NUMBER`, `STRING`, `BIGINT`, `SYMBOL`, `OBJECT`, `FUNCTION`, etc.)
- `TypeKey` - structural representation with 25+ variants including `Intrinsic`, `Literal`, `Object`, `Union`, `Intersection`, `Array`, `Tuple`, `Function`, `Callable`, `TypeParameter`, `Ref`, `Application`, `Conditional`, `Mapped`, `IndexAccess`, `TemplateLiteral`, `KeyOf`, `StringIntrinsic`, `ModuleNamespace`, etc.

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

**Key trait in `solver/visitor.rs`:**
- `TypeVisitor` trait with associated `type Output`
- Required methods: `visit_intrinsic()`, `visit_literal()`
- Optional methods with defaults: `visit_object()`, `visit_union()`, `visit_intersection()`, `visit_array()`, `visit_tuple()`, `visit_function()`, `visit_type_parameter()`, etc.
- Convenience functions: `is_type_kind()`, `is_literal_type()`, `is_function_type()`, `contains_type_parameters()`, etc.

### 6. Zero-Copy Source Text

Scanner uses `Arc<str>` for shared source ownership:
- No duplication across phases
- String slices for token text: `&source[start..end]`
- String interning for identifiers via `Atom` (u32)

## Critical Limits

These constants in `src/limits.rs` prevent pathological cases from causing hangs or OOM:

| Constant | Value | Purpose |
|----------|-------|---------|
| `MAX_SUBTYPE_DEPTH` | 100 | Subtype recursion limit |
| `MAX_TOTAL_SUBTYPE_CHECKS` | 100,000 | Total checks per instance |
| `MAX_INSTANTIATION_DEPTH` | 50 | Generic instantiation depth |
| `MAX_CALL_DEPTH` | 20 | Function call nesting |
| `MAX_EMIT_RECURSION_DEPTH` | 1,000 | Code generation depth |
| `MAX_PARSER_RECURSION_DEPTH` | 1,000 | Parser recursion |
| `MAX_TYPE_RESOLUTION_OPS` | 100,000 (native) / 20,000 (WASM) | Fuel counter |
| `MAX_EVALUATE_DEPTH` | 50 | Type evaluation depth |
| `MAX_TOTAL_EVALUATIONS` | 100,000 | Total type evaluations |
| `TEMPLATE_LITERAL_EXPANSION_LIMIT` | 100,000 (native) / 2,000 (WASM) | Template expansion |

## Cross-References

This documentation complements:
- [docs/specs/TS_UNSOUNDNESS_CATALOG.md](../specs/TS_UNSOUNDNESS_CATALOG.md) - TypeScript compatibility rules
- [docs/architecture/NORTH_STAR.md](../architecture/NORTH_STAR.md) - Target architecture guide
- [AGENTS.md](../../AGENTS.md) - Architecture rules for contributors

## Navigation Tips

- **File references** point to relevant source files
- **⚠️ GAP** markers indicate incomplete implementations
- **📍 KEY** markers highlight critical code locations
- Use `rg` or IDE search with function/struct names to locate implementations

---

*Last updated: January 2026*
