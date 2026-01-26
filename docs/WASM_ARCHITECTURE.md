# WASM Compiler Architecture (Updated 2026)

**Status Legend:**
- ✅ **Implemented** - Verified in codebase
- ⚠️ **Partial** - Partially implemented, needs completion
- ❌ **Aspirational** - Planned but not yet implemented

Codename Zang (Persian for rust) is the working name for this Rust/WASM compiler effort. The CLI binary is `tsz`.

## Foundation & parsing

### 1. Architectural Philosophy

This compiler is designed specifically for **WebAssembly (WASM)** execution environments where memory latency and boundary crossing costs are the primary bottlenecks.

#### Key Constraints & Decisions

- **Data-Oriented Design (DOD):** Objects are banned in hot paths. We use Struct-of-Arrays (SoA) and arenas (indices) instead of pointers.
- **Zero-Copy by Default:** The compiler operates on a single `&str` slice of the source code. Strings are **never** allocated during parsing or scanning unless escaping is strictly required.
- **Cache Locality:** AST nodes must fit within a single CPU cache line (64 bytes). We achieve 4 nodes per cache line (16 bytes/node).
- **Concurrency:** The architecture must support `rayon` for parallel file processing. Shared state (Interners) must be sharded to prevent lock contention.

### 2. The Data Pipeline

The pipeline is linear but separates **Syntax** (Parser) from **Semantics** (Solver).

```mermaid
graph LR
    A[Source Text] -->|Zero-Copy Slice| B(Scanner)
    B -->|Lazy Tokens| C(Parser)
    C -->|Node Indices| D[NodeArena]

    subgraph "Phase 1: Syntax (Parallel)"
    B
    C
    D
    end

    D --> E(Binder)
    E -->|SymbolGraph| F(Solver)

    subgraph "Phase 2: Semantics (Lazy)"
    E
    F
    end

    F --> G(Emitter)
    G --> H[JavaScript Output]
```

### 3. Memory Architecture: The "Node" System

The core innovation is the **Node** representation. Benchmarks confirm the Emitter achieves **500 MB/s** throughput due to this layout.

#### 3.1. The 16-Byte Header ✅ **IMPLEMENTED**

Every AST node is a fixed-size, 16-byte header stored in `Vec<Node>`.

**Verified**: `src/parser/node.rs:32-53` matches this specification exactly.

```rust
#[repr(C)]
pub struct Node {
    pub kind: u16,        // SyntaxKind
    pub flags: u16,       // NodeFlags (Contextual info)
    pub pos: u32,         // Start position (byte offset)
    pub end: u32,         // End position (byte offset)
    pub data_index: u32,  // Index into typed data pool (u32::MAX if none)
}
```

#### 3.2. Typed Data Pools (SoA) ✅ **IMPLEMENTED**

Node-specific data is stripped from the header and stored in typed pools.
**Verified**: `src/parser/node.rs` implements typed pools with Atom-based identifiers.

**Critical Update:** To solve the "String Bloat" performance issue (50 MB/s parse speed), data pools **must not** store heap-allocated `String`s.

| Node Category   | Storage Pool         | Data Layout (Updated)                                        | Status |
| :-------------- | :------------------- | :----------------------------------------------------------- | :------ |
| `Identifier`    | `arena.identifiers`  | `{ name: Atom }` (u32 index, no String)                      | ✅      |
| `StringLiteral` | `arena.literals`     | `{ text: Atom }` (u32 index)                                 | ✅      |
| `BinaryExpr`    | `arena.binary_exprs` | `{ left: NodeIndex, op: u16, right: NodeIndex }`             | ✅      |
| `Function`      | `arena.functions`    | `{ name: NodeIndex, params: NodeList, body: NodeIndex ... }` | ✅      |

### 4. String Handling: The Zero-Copy Mandate ✅ **IMPLEMENTED**

Benchmarks revealed that eager string allocation in the Scanner/Parser destroys performance. We enforce a **View-Based** approach.

**Verified**: `src/interner.rs` implements Atom-based string storage with sharding.

#### 4.1. The Interner (Atoms) ✅ **IMPLEMENTED**

- **Role:** Deduplicates identifiers and literals.
- **Type:** `Atom` (u32).
- **Mechanism:** `HashMap<&str, Atom>`.
- **Concurrency:** ✅ **Implemented** - The global interner uses **64 shards** with `DashMap` to allow parallel parsing without serialization bottlenecks.
- **Implementation:** `ShardedInterner` in `src/interner.rs:276`

#### 4.2. Scanning Strategy

The Scanner must not return `String`.

1.  **Scan:** Identify start/end bytes.
2.  **Intern:** Pass the `&str` slice to the Interner immediately.
3.  **Store:** Keep only the `Atom` (u32).

_Exceptions:_ Strings requiring escape sequence processing (e.g., `"foo\nbar"`) are allocated temporarily during interning but stored as deduplicated Atoms.

### 5. The Parser Implementation

The parser (`parser/state.rs`) is a recursive descent parser constructing the `NodeArena`.

- **Error Recovery:** No `Result<T, E>`. Errors are pushed to a side `Vec<Diagnostic>`, and the parser inserts "Missing" nodes to maintain tree integrity.
- **Incremental Parsing:** Since nodes are just arrays of integers (`Vec<Node>`), incremental updates can reuse chunks of the array (future capability).
- **Recursion Guard:** Explicit depth checks (`recursion_depth`) are required to prevent stack overflows on deep trees, as the Solver handles logic recursion, but the Parser handles syntactic recursion.

## Semantic Analysis (The "Brain")

The Semantic Layer is split into three distinct components to enforce separation of concerns:

1.  **Binder:** Establishes naming and scope (Who is `x`?).
2.  **Solver (The Engine):** Handles mechanics (Caching, Recursion, Unification).
3.  **Checker (The Judge):** Defines business logic (Is `string` assignable to `number`?).

---

### 1. The Binder: Stateless Scope Resolution

The Binder (`binder/state.rs`) performs a single pass over the AST to build a persistent **Scope Graph**.

#### 1.1. No Transient Scopes

**Critical Change:** The Type Checker must **never** manage scope stacks manually (e.g., `push_scope`/`pop_scope` during traversal). This creates temporal coupling that prevents random-access queries (LSP).

- **Binder Output:** A `ScopeGraph` or flat `node_scope: HashMap<NodeIndex, ScopeId>`.
- **Symbol Resolution:** Identifiers are resolved to `SymbolId`s during the bind phase. The Checker works purely with `SymbolId`.

#### 1.2. Symbol Storage

- **SymbolArena:** Contiguous storage for `Symbol` structs.
- **Sharding:** For parallel binding, each file produces a local `SymbolTable`. These are merged into a global table for the Solver.

---

### 2. The Solver: The Mechanics Engine

The Solver (`src/solver/`) is the generic engine that handles the mathematical and computational complexity of the type system. It does **not** know TypeScript specific rules; it knows how to solve graphs and equations.

#### 2.1. Type Representation (POD)

To fix the "String Bloat" issue, `TypeKey` must be **Plain Old Data (POD)**. No `String` or `Arc<str>` allowed.

```rust
// src/solver/types.rs
#[derive(PartialEq, Eq, Hash, Clone, Copy)] // Copy is the goal
pub enum TypeKey {
    Intrinsic(IntrinsicKind),
    Literal(Atom),        // u32 index into Interner
    Object(Slice<Prop>),  // Indices into a side table
    Union(Slice<TypeId>), // Indices into a side table
    // ...
}
```

#### 2.2. The Interner (Sharded) ✅ **IMPLEMENTED**

Type checking is a "Write-Heavy" operation (instantiating generics creates new types).

- **Problem:** A global `RwLock<HashMap>` serializes all threads.
- **Solution:** ✅ **Implemented** - **Sharded Interning**.
  - Uses `DashMap` with lock-free concurrent access.
  - **Verified**: `src/solver/intern.rs:92-106` implements `TypeInterner` with `DashMap<TypeKey, u32>`.
  - This allows multiple threads to intern different types like `Array<string>` and `Promise<number>` simultaneously.

#### 2.3. Inference (Unification)

We use the **`ena`** crate (Union-Find) to solve generic constraints.

- The Solver creates `InferenceVar`s.
- The Checker registers constraints (`Upper Bound`, `Lower Bound`).
- The Solver unifies variables and determines the final type.

---

### 3. The Checker: The Logic Judge

The Checker (`checker/state.rs`) implements the TypeScript "Business Logic". It answers questions like "Is A a subtype of B?".

#### 3.0. Why the Checker Stays Large (Even with a Solver)

The Solver is the engine for **type relations** (subtype, assignable, inference unification),
but the Checker still owns the **orchestration** that makes a TypeScript compiler behave like `tsc`.
This means the Checker must remain substantial even with a strong solver.

**Checker responsibilities that are not solved by the solver:**
- **Type synthesis**: Map AST nodes to `TypeId` (contextual typing, literal widening, `this` typing).
- **Control flow analysis**: Narrowing, definite assignment, reachability, and flow-sensitive errors.
- **Symbol and scope resolution**: Binder integration, module/namespace handling, global lookup.
- **Diagnostics**: Error locations, tailored messages, and recovery paths.
- **Compiler options**: Enforce flags like `strictNullChecks`, `noImplicitAny`, `exactOptionalPropertyTypes`.

**Solver responsibilities (what it should own):**
- **Pure relational logic**: Subtype and assignability decisions.
- **Unification mechanics**: Generic inference and constraint solving.
- **Caching and cycle handling**: Structural comparisons without recursion blowups.

The architectural goal is not to remove the Checker, but to **thin it** by delegating all
pure relational logic to the Solver while keeping TypeScript-specific orchestration in the Checker.

#### 3.1. The "Tracer" Pattern (Zero-Cost Abstraction) ❌ **ASPIRATIONAL / NOT IMPLEMENTED**

To prevent logic drift between "Checking" (Fast/Bool) and "Explaining" (Slow/Diagnostic), we must **not** write duplicate algorithms.

**Status**: This pattern is documented here but **not found in the current implementation**. The checker currently uses direct error collection.

**Planned Design:**

We use a generic **Tracer** trait to abstract the side effects.

```rust
// The Interface
pub trait SubtypeTracer {
    fn on_mismatch(&mut self, reason: impl FnOnce() -> FailureReason) -> bool;
}

// The Fast Implementation (Zero-Cost)
struct FastTracer;
impl SubtypeTracer for FastTracer {
    #[inline(always)]
    fn on_mismatch(&mut self, _reason: impl FnOnce() -> FailureReason) -> bool {
        false // Stop immediately, allocate nothing
    }
}

// The Slow Implementation (Detailed)
struct DiagnosticTracer { error: Option<FailureReason> }
impl SubtypeTracer for DiagnosticTracer {
    fn on_mismatch(&mut self, reason: impl FnOnce() -> FailureReason) -> bool {
        self.error = Some(reason()); // Allocate detailed error
        false
    }
}
```

#### 3.2. Logic vs. Mechanics Separation

The Checker function (`check_subtype`) must be pure logic. It must **not** handle recursion limits or cycle detection manually.

**Bad (Spaghetti):**

```rust
fn check_subtype(a, b) {
    if self.depth > 100 { return provisional; } // Mechanics mixed with logic
    match (a, b) { ... }
}
```

**Good (Separated):**

```rust
// Solver handles mechanics
fn is_subtype(&mut self, a, b) {
    self.cycle_detector.run(a, b, |tracer| {
        // Checker handles logic
        Checker::check_structure(a, b, tracer)
    })
}
```

### 4. Semantic Pipeline Summary

1.  **Binder:** AST $\to$ `SymbolId` + `ScopeGraph`.
2.  **Lowering:** `SymbolId` / AST $\to$ `TypeId` (Interned).
3.  **Solver:** `(TypeId, TypeId)` $\to$ `Result`.
    - Manages cache, recursion, and unification.
    - Delegates specific rules to the Checker logic via the Tracer pattern.

## Emission, Transforms & Tools

### 1. The Emitter: A Dumb Printer ⚠️ **PARTIAL / DEBT NOTED**

The Emitter (`emitter/`) must be stripped of all business logic. Its sole responsibility is **Code Generation** (writing strings and source maps), not **Transformation** (restructuring code).

**Status**: ⚠️ **Known Architectural Debt** - As noted in `PROJECT_DIRECTION.md`, the "Transform pipeline still mixes lowering/printing per PROJECT_DIRECTION—architectural debt not yet addressed."

#### 1.1. Separation of Concerns

**Anti-Pattern (The "God Method"):**

```rust
// BAD: Printer deciding logic based on config
fn emit_class(&mut self, class: Class) {
    if self.config.target == ES5 {
        self.emit_iife_pattern(class);
    } else {
        self.write("class ");
        // ...
    }
}
```

**Correct Architecture (The Pipeline):**
The Emitter accepts a stream of **Print Commands** or traverses a **Virtual AST**.

1.  **Transforms** run first (conceptually). They convert `Node` (Source) $\to$ `VirtualNode` (Target).
2.  **Printer** simply executes the structure provided by the transform.

#### 1.2. Source Maps

- **SourceWriter:** The low-level buffer wrapper. It tracks line/column offsets and maps generated positions back to original `Node.pos`.
- **VLQ Encoding:** Optimized to write directly to the output buffer without intermediate string allocations.

---

### 2. Transformations: The Output Logic

Transformations handles the complexity of downleveling (ES6 $\to$ ES5, Async $\to$ Generators, etc.).

#### 2.1. Strategy: Virtual Nodes / Projection

Since the `NodeArena` is immutable during the emit phase, we cannot modify the AST in place. We use **Projections**.

A Transform implementation takes a `NodeIndex` and "projects" it into a sequence of emit operations.

```rust
pub trait Transformer {
    /// Returns true if this transformer handles the node.
    /// If false, the printer falls back to default emission.
    fn transform(&self, node: NodeIndex, writer: &mut SourceWriter, ctx: &EmitContext) -> bool;
}

// Example: ClassES5Transform
impl Transformer for ClassES5Transform {
    fn transform(&self, node: NodeIndex, w: &mut SourceWriter, ctx: &EmitContext) -> bool {
        // Logic for IIFE generation lives here, completely isolated from the Printer.
        // It calls w.write(), w.indent(), etc.
        true
    }
}
```

#### 2.2. Configuration Handling

The "Configuration Matrix" (CommonJS vs ESM, ES5 vs ESNext) is handled by composing the pipeline, not by `if` statements inside the print loop.

- `ES5Pipeline`: `[DestructuringTransform, AsyncTransform, ClassES5Transform, DefaultPrinter]`
- `ESNextPipeline`: `[DefaultPrinter]`

---

### 3. Language Server Protocol (LSP)

The LSP layer requires random access to the AST and semantic data.

#### 3.1. Parent Mapping (The "Walk Up" optimization)

LSP operations often require walking _up_ the tree (e.g., `SignatureHelp` needs to find the `CallExpression` containing the cursor).

- **Problem:** `Node` (16 bytes) does not store a parent pointer to save space.
- **Solution:** A **Side Table** `Vec<NodeIndex>` (ParentMap) computed once post-parse.
  - Index = `Child NodeIndex`
  - Value = `Parent NodeIndex`
  - This allows O(1) parent lookup without bloating the core AST nodes.

#### 3.2. Stateless Queries

LSP handlers (`Hover`, `GoToDef`) must use the **Binder's Scope Graph** (from Part 2) to resolve symbols. They must **not** try to simulate scoping by traversing the AST manually.

- **Workflow:**
  1.  `find_node_at_offset(offset)` $\to$ `NodeIndex`.
  2.  `binder.get_scope(node.parent)` $\to$ `ScopeId`.
  3.  `scope.lookup(atom)` $\to$ `SymbolId`.
  4.  `solver.get_type(symbol_id)` $\to$ `TypeId`.
  5.  `formatter.print(type_id)` $\to$ String.

#### 3.3. Resilience

The Parser produces "Error Nodes" or "Missing Nodes" when syntax is invalid. The LSP must handle these gracefully.

- **Atoms:** All identifier lookups use `Atom` (u32), never `String`, preventing allocation storms during autocomplete filtering.

---

### 4. Implementation Roadmap (Summary)

| Task | Status | Notes |
|------|--------|-------|
| 1. **Refactor Memory**: Switch `IdentifierData` to use `Atom` | ✅ **COMPLETE** | Verified in `src/parser/node.rs` and `src/interner.rs` |
| 2. **Shard Interner**: Make `TypeInterner` concurrent-safe | ✅ **COMPLETE** | `TypeInterner` uses `DashMap` in `src/solver/intern.rs` |
| 3. **Refactor Logic**: Tracer Pattern for subtype checking | ❌ **TODO** | Documented in section 3.1 but not implemented |
| 3. **Refactor Logic**: Remove manual scope stacks from `Checker` | ⚠️ **PARTIAL** | Binder produces persistent scopes, but verify full removal |
| 4. **Refactor Emitter**: Extract transform logic to `transforms/` | ❌ **TODO** | Known debt per `PROJECT_DIRECTION.md:86` |
| 5. **Benchmark**: Verify parser > 200 MB/s | ⚠️ **UNKNOWN** | No current benchmark data found |

---

## References

- `AGENTS.md` - Architecture rules and compatibility requirements for this repo
- `specs/SOLVER.md` - Solver design and Judge/Lawyer architecture
- `specs/TS_UNSOUNDNESS_CATALOG.md` - Compatibility layer rules
