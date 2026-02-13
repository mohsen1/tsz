# GOAL

**Goal**: Match `tsc` behavior exactly. Every error, inference, and edge case must be identical to TypeScript's compiler.

# TSZ North Star Architecture


## 1. Executive Summary

TSZ is a TypeScript compiler implemented in Rust, designed for high performance, maintainability, and correctness. This document describes the **ideal architecture** that all refactoring efforts should converge toward.

### Core Vision

TSZ achieves its goals through four fundamental architectural principles:

1. **Solver-First Architecture**: The Solver is the single source of truth for all type computations
2. **Thin Wrappers**: Components are orchestration layers, not logic containers
3. **Visitor Patterns**: Systematic traversal over ad-hoc pattern matching
4. **Arena Allocation**: Memory-efficient, cache-friendly data structures

### Key Metrics

| Metric | Target |
|--------|--------|
| Type equality check | O(1) via interning |
| Symbol lookup | O(1) via index |
| Node memory | 16 bytes per node header |
| File parallelism | Full parallel parsing |
| Max file size per component | Under 2000 lines |

---

## 2. Component Architecture Overview

### 2.1 Pipeline Data Flow

```
                           TSZ COMPILER PIPELINE

    Source Text
         |
         v
    +----------+     Tokens      +----------+
    | Scanner  | --------------> | Parser   |
    +----------+                 +----------+
         |                            |
         | Zero-copy                  | AST (NodeArena)
         | String interning           | 16-byte thin nodes
         v                            v
    +----------+                 +----------+
    | Interner |                 | Binder   |
    +----------+                 +----------+
         |                            |
         | Atom (u32)                 | Symbols (SymbolArena)
         |                            | Scopes (persistent tree)
         |                            | Flow Graph (FlowNodeArena)
         |                            v
         |                       +----------+
         +---------------------> | Checker  | <---- Orchestration
                                 +----------+       (thin layer)
                                      |
                                      | Type queries
                                      v
                                 +----------+
                                 | Solver   | <---- Type Engine
                                 +----------+       (WHAT)
                                      |
                                      | TypeId results
                                      v
                                 +----------+
                                 | Emitter  |
                                 +----------+
                                      |
                                      | JavaScript output
                                      v
                                 Output Files
```

### 2.2 LSP Integration

```
    +-------+     +----------+     +---------+
    | LSP   | --> | Project  | --> | Checker |
    +-------+     +----------+     +---------+
        |              |                |
        |              |                v
        |              |          +----------+
        |              +--------> | Solver   |
        |                         +----------+
        |                              |
        v                              v
    +---------------+            +----------+
    | Global Type   | <--------- | Type     |
    | Interning     |            | Interner |
    +---------------+            +----------+
```

### 2.3 Component Responsibility Matrix

```
    +-----------+--------------------------------------------------+
    | Component | Responsibility                                   |
    +-----------+--------------------------------------------------+
    | Scanner   | Lexical analysis, tokenization, string interning |
    | Parser    | Syntax analysis, AST construction                |
    | Binder    | WHO - Symbols, scopes, control flow graph        |
    | Solver    | WHAT - Pure type computations                    |
    | Checker   | WHERE - AST traversal, diagnostics               |
    | Emitter   | OUTPUT - Code generation, transforms             |
    | LSP       | CONSUMER - IDE features using checker output     |
    +-----------+--------------------------------------------------+
```

---

## 3. Core Principles (MUST be followed)

### 3.1 Solver-First Architecture

The Solver is the **central type computation engine**. It owns all type-related logic.

**Division of Concerns:**

| Component | Handles | Does NOT Handle |
|-----------|---------|-----------------|
| Solver | WHAT - Type computations | Source locations, AST nodes |
| Checker | WHERE - Source context | Type algorithms |
| Binder | WHO - Symbol definitions | Type inference |
| Emitter | OUTPUT - Code generation | Type validation |

**Solver Operations:**

```rust
// The Solver provides these pure functions:
trait Solver {
    // Type relations
    fn is_subtype_of(&self, source: TypeId, target: TypeId) -> bool;
    fn is_assignable_to(&self, source: TypeId, target: TypeId) -> bool;
    fn are_types_identical(&self, a: TypeId, b: TypeId) -> bool;

    // Type inference
    fn infer_type(&self, context: &InferContext) -> TypeId;
    fn instantiate(&self, generic: TypeId, args: &[TypeId]) -> TypeId;

    // Type evaluation
    fn evaluate(&self, type_id: TypeId) -> TypeId;
    fn evaluate_conditional(&self, cond: ConditionalTypeId) -> TypeId;
    fn evaluate_mapped(&self, mapped: MappedTypeId) -> TypeId;
    fn evaluate_keyof(&self, type_id: TypeId) -> TypeId;
    fn evaluate_index_access(&self, object: TypeId, key: TypeId) -> TypeId;

    // Type construction
    fn union(&self, types: &[TypeId]) -> TypeId;
    fn intersection(&self, types: &[TypeId]) -> TypeId;
    fn narrow(&self, type_id: TypeId, narrower: TypeId) -> TypeId;
}
```

**RULE**: If an operation involves type computation, it belongs in the Solver.

### 3.2 Type System Rules

#### Rule 1: ALL type computations go through Solver

```rust
// CORRECT: Checker delegates to Solver
impl CheckerState<'_> {
    fn check_assignment(&mut self, source: TypeId, target: TypeId) -> bool {
        // Checker asks Solver for the answer
        self.solver.is_assignable_to(source, target)
    }
}

// WRONG: Checker doing type computation
impl CheckerState<'_> {
    fn check_assignment(&mut self, source: TypeId, target: TypeId) -> bool {
        // Never do this - type logic belongs in Solver
        match (self.types.lookup(source), self.types.lookup(target)) {
            (TypeKey::Union(_), _) => { /* manual logic */ }
            // ...
        }
    }
}
```

#### Rule 2: Use visitor pattern for ALL type operations

```rust
// CORRECT: Visitor pattern
fn get_referenced_types(types: &TypeInterner, type_id: TypeId) -> Vec<TypeId> {
    use crate::solver::visitor::collect_referenced_types;
    collect_referenced_types(types, type_id)
}

// WRONG: Direct TypeKey matching
fn get_referenced_types(types: &TypeInterner, type_id: TypeId) -> Vec<TypeId> {
    match types.lookup(type_id) {
        Some(TypeKey::Union(list_id)) => { /* manual extraction */ }
        Some(TypeKey::Object(shape_id)) => { /* manual extraction */ }
        // Repeating for 24+ variants...
    }
}
```

#### Rule 3: Checker NEVER inspects type internals

```rust
// CORRECT: Checker uses Solver queries
fn is_string_type(&self, type_id: TypeId) -> bool {
    self.solver.is_subtype_of(type_id, TypeId::STRING)
}

// WRONG: Checker matching on TypeKey
fn is_string_type(&self, type_id: TypeId) -> bool {
    matches!(
        self.types.lookup(type_id),
        Some(TypeKey::Intrinsic(IntrinsicKind::String))
    )
}
```

### 3.3 Judge vs. Lawyer Architecture

The Solver implements a two-layer design for type compatibility:

**Judge (SubtypeChecker):**
- Implements strict, sound set-theory semantics
- Knows nothing about TypeScript legacy behavior
- Performs structural subtype checking

**Lawyer (AnyPropagationRules + CompatChecker):**
- Applies TypeScript-specific compatibility rules
- Handles `any` propagation (the "black hole" that is both top and bottom type)
- Manages function variance modes (strict vs. bivariant)
- Tracks object literal freshness for excess property checking
- Implements the void exception (`() => void` matches `() => T`)
- Detects weak types (TS2559)

```rust
// src/solver/lawyer.rs
pub struct AnyPropagationRules {
    /// Whether to allow `any` to silence structural mismatches.
    pub allow_any_suppression: bool,
}

impl AnyPropagationRules {
    /// Strict mode: `any` does NOT silence structural mismatches
    pub fn strict() -> Self { Self { allow_any_suppression: false } }

    /// Legacy mode: `any` suppresses errors for backward compatibility
    pub fn new() -> Self { Self { allow_any_suppression: true } }
}
```

**Key Principle:** `any` should NOT silence structural mismatches. While `any` is TypeScript's escape hatch, the Lawyer layer ensures real errors are still caught.

### 3.4 Dependency Direction Rules

The architectural direction is strict and one-way. This is the canonical dependency policy.

#### Allowed High-Level Flow

`scanner -> parser -> binder -> checker -> solver -> emitter`

LSP is a consumer/orchestrator around project state and checker results; it does not define type algorithms.

#### Layer Rules

1. Parser/Scanner do not depend on Checker/Solver internals.
2. Binder owns symbol/scope/control-flow facts (`WHO`), not type computation (`WHAT`).
3. Checker orchestrates AST traversal and diagnostics (`WHERE`), delegating type algorithms.
4. Solver owns type relations, inference, narrowing, evaluation, and type queries (`WHAT`).
5. Emitter consumes checked/transformed representations and does not perform semantic type validation.

#### Forbidden Cross-Layer Shortcuts

1. Checker implementing ad-hoc type algorithms that duplicate Solver logic.
2. Checker pattern-matching directly on low-level type internals when Solver query helpers exist.
3. Binder importing Solver logic for semantic type decisions.
4. Emitter importing Checker internals for on-the-fly semantic checks.
5. Any layer bypassing canonical query APIs and reaching into another layer's private representation.

#### Review Heuristic

For every new change, ask:

1. Is this computing `WHAT` (type algorithm) or only deciding `WHERE` to report it?
2. If `WHAT`, move it to Solver or a Solver query helper.
3. If `WHERE`, keep it in Checker and call the Solver/queries.

### 3.5 DefId-Centric Type Resolution

TSZ's current architecture is **DefId-first** for semantic type references.

#### Canonical Model

1. Binder owns symbol identity (`SymbolId`) and declaration graphs.
2. Checker creates stable definition identities (`DefId`) for semantic type references.
3. Solver represents unresolved semantic references as `TypeKey::Lazy(DefId)`.
4. `TypeEnvironment` resolves `DefId -> TypeId` during evaluation/compatibility.
5. Checker ensures required `DefId -> TypeId` mappings exist before deep relation checks.

#### Why This Matters

- Eliminates fragile direct symbol-handle type references in the solver type graph.
- Makes lazy type resolution explicit and queryable.
- Stabilizes recursive and cross-module type resolution behavior.

#### Rules

1. New semantic type references must use `Lazy(DefId)`, not ad-hoc symbol-backed ref keys.
2. Checker code should prefer solver query helpers (`get_lazy_def_id`, classifiers) over direct low-level matching.
3. Relation/evaluation code paths must guarantee needed `DefId` mappings are available in `TypeEnvironment`.

| TypeScript Quirk | Judge Behavior | Lawyer Override |
|------------------|----------------|-----------------|
| `any` assignability | Strict sets | Both top & bottom type |
| Function params | Contravariant | Bivariant for methods |
| Object literals | Width subtyping | Excess property check |
| Void returns | Normal checking | Allow any return |

### 3.4 Memory Architecture

#### Arena Allocation Model

```
    ARENA ALLOCATION STRATEGY

    +----------------+     +------------------+     +------------------+
    | NodeArena      |     | SymbolArena      |     | FlowNodeArena    |
    | (AST nodes)    |     | (Symbols)        |     | (Control flow)   |
    +----------------+     +------------------+     +------------------+
           |                       |                        |
           v                       v                        v
    +------+------+         +------+------+          +------+------+
    | NodeId(u32) |         | SymbolId    |          | FlowNodeId  |
    |             |         | (u32)       |          | (u32)       |
    +-------------+         +-------------+          +-------------+

    +----------------------------------------------------------+
    | TypeInterner (Global)                                     |
    | +------+------+------+------+------+------+------+-----+ |
    | |Type 0|Type 1|Type 2| ...  | ...  | ...  | ...  | ... | |
    | +------+------+------+------+------+------+------+-----+ |
    |        Deduplication via hash map                        |
    +----------------------------------------------------------+
           |
           v
    +-------------+
    | TypeId(u32) |
    +-------------+
```

**Key Rules:**

1. **AST Nodes**: Arena-allocated via `NodeArena`, accessed by `NodeIndex`
2. **Symbols**: Arena-allocated via `SymbolArena`, accessed by `SymbolId`
3. **Flow Nodes**: Arena-allocated via `FlowNodeArena`, accessed by `FlowNodeId`
4. **Types**: Globally interned via `TypeInterner`, accessed by `TypeId`
5. **Strings**: Interned via `Interner`, accessed by `Atom` (u32)

**Benefits:**

- O(1) equality for types: `type_a == type_b`
- O(1) equality for strings: `atom_a == atom_b`
- Cache-friendly linear memory layout
- Zero fragmentation (no individual allocations)
- Automatic deduplication

---

## 4. Detailed Component Specifications

### 4.1 Scanner

**Purpose**: Transform source text into tokens.

**Architecture**:
```
    Source Text (Arc<str>)
           |
           v
    +------------------+
    | ScannerState     |
    |------------------|
    | pos: usize       |
    | token: SyntaxKind|
    | interner: ref    |
    +------------------+
           |
           v
    Token Stream + Atoms
```

**Key Properties**:

| Property | Implementation |
|----------|----------------|
| Zero-copy | `Arc<str>` source, slice references |
| String interning | All identifiers become `Atom(u32)` |
| Driver-driven | Parser controls `scan()` calls |
| Pre-interned keywords | 100+ common words pre-cached |
| Fast ASCII path | Single-byte optimization for ASCII |

**API Surface**:
```rust
impl ScannerState {
    fn scan(&mut self) -> SyntaxKind;
    fn get_token(&self) -> SyntaxKind;
    fn get_token_atom(&self) -> Atom;
    fn get_token_start(&self) -> usize;
    fn get_token_end(&self) -> usize;
    fn has_preceding_line_break(&self) -> bool;

    // Context-sensitive rescanning
    fn re_scan_greater_token(&mut self) -> SyntaxKind;
    fn re_scan_slash_token(&mut self) -> SyntaxKind;
    fn re_scan_template_token(&mut self) -> SyntaxKind;
}
```

### 4.2 Parser

**Purpose**: Transform tokens into an Abstract Syntax Tree.

**Architecture**:
```
    Token Stream
         |
         v
    +------------------+
    | ParserState      |
    |------------------|
    | scanner: ref     |
    | arena: NodeArena |
    | context_flags    |
    +------------------+
         |
         v
    NodeArena (AST)
```

**16-Byte Thin Node Design**:
```rust
pub struct Node {
    pub kind: u16,        // SyntaxKind enum
    pub flags: u16,       // NodeFlags
    pub pos: u32,         // Start byte position
    pub end: u32,         // End byte position
    pub data_index: u32,  // Index into typed data pool
}
// Total: 16 bytes - 4 nodes per cache line
```

**Data Pool Organization**:
```
    NodeArena
    +------------------+
    | nodes: Vec<Node> |  <- All node headers (16 bytes each)
    +------------------+
    | identifiers      |  <- IdentifierData pool
    | binary_exprs     |  <- BinaryExpressionData pool
    | call_exprs       |  <- CallExpressionData pool
    | blocks           |  <- BlockData pool
    | functions        |  <- FunctionData pool
    | classes          |  <- ClassData pool
    | ... 40+ pools    |
    +------------------+
```

**Key Properties**:

| Property | Value |
|----------|-------|
| Node size | 16 bytes (header only) |
| Cache efficiency | 4 nodes per cache line |
| Pre-allocation | ~1 node per 20 source characters |
| Recursion limit | 1000 levels |
| No type info | Pure syntax tree |

### 4.3 Binder

**Purpose**: Build symbol table, scope tree, and control flow graph.

**Architecture**:
```
    NodeArena (AST)
         |
         v
    +------------------+
    | BinderState      |
    |------------------|
    | symbols: Arena   |
    | scopes: Vec      |
    | flow_nodes: Arena|
    +------------------+
         |
         +---> Symbols (who declares what)
         +---> Scopes (persistent tree, not stack)
         +---> Flow Graph (control flow edges)
```

**Symbol Structure**:
```rust
pub struct Symbol {
    pub flags: u32,                // Kind + modifiers
    pub escaped_name: String,      // Symbol name
    pub declarations: Vec<NodeIndex>,
    pub value_declaration: NodeIndex,
    pub parent: SymbolId,
    pub exports: Option<Box<SymbolTable>>,
    pub members: Option<Box<SymbolTable>>,
}
```

**Scope Tree** (NOT Stack):
```
    SourceFile Scope
         |
         +-- Function Scope
         |        |
         |        +-- Block Scope
         |        |
         |        +-- Block Scope
         |
         +-- Class Scope
                  |
                  +-- Method Scope
```

**Flow Graph**:
```
    START
      |
      v
    [condition] ----TRUE----> [then]
      |                         |
      FALSE                     |
      |                         |
      v                         v
    [else] -----------------> [join]
      |                         |
      v                         v
    UNREACHABLE              [next]
```

**Key Rules**:
1. Binder does NO type computations
2. Scope tree is persistent (for incremental updates)
3. Hoisting handled in two passes (collect, then process)
4. Flow nodes link via antecedent chains

### 4.4 Solver (Type Engine)

**Purpose**: All pure type computations.

**Architecture**:
```
    TypeId Inputs
         |
         v
    +------------------+
    | Solver           |
    |------------------|
    | interner: ref    |  <- TypeInterner (global)
    | cache: HashMap   |  <- Memoization
    | cycle_stack      |  <- Coinductive semantics
    +------------------+
         |
         v
    TypeId Outputs
```

**Type Representation**:
```rust
// TypeId: 4-byte handle (O(1) equality)
pub struct TypeId(pub u32);

// TypeKey: Actual type structure
pub enum TypeKey {
    // Primitives
    Intrinsic(IntrinsicKind),
    Literal(LiteralValue),

    // Collections
    Array(TypeId),
    Tuple(TupleListId),

    // Objects
    Object(ObjectShapeId),
    ObjectWithIndex(ObjectShapeId),

    // Composites
    Union(TypeListId),
    Intersection(TypeListId),

    // Functions
    Function(FunctionShapeId),
    Callable(CallableShapeId),

    // Generics / semantic references
    TypeParameter(TypeParamInfo),
    Lazy(DefId),
    Application(TypeApplicationId),

    // Advanced
    Conditional(ConditionalTypeId),
    Mapped(MappedTypeId),
    IndexAccess(TypeId, TypeId),  // T[K]
    KeyOf(TypeId),
    TemplateLiteral(TemplateLiteralId),
    TypeQuery(SymbolRef),
    ThisType,
    UniqueSymbol(SymbolRef),

    // Modifiers
    ReadonlyType(TypeId),  // readonly T[]

    // String intrinsics
    StringIntrinsic {      // Uppercase<T>, Lowercase<T>, etc.
        kind: StringIntrinsicKind,
        type_arg: TypeId,
    },

    // Inference
    Infer(TypeParamInfo),  // infer R in conditional types

    // Error recovery
    Error,  // Error type for invalid type expressions
}
```

**Subtyping Algorithm** (Coinductive):
```
    solve_subtype(source, target):
        1. Identity check: source == target => true
        2. Top/bottom: source=NEVER => true, target=UNKNOWN => true
        3. Cycle check: (source, target) in stack => true (GFP)
        4. Push (source, target) to stack
        5. Structural check based on TypeKey
        6. Pop from stack
        7. Return result
```

**Key Properties**:

| Property | Implementation |
|----------|----------------|
| Stateless | Takes inputs, returns outputs |
| Memoized | Query results cached |
| Coinductive | Greatest Fixed Point for recursion |
| Visitor-based | TypeVisitor trait for traversal |

**Critical Limits**:
```rust
const MAX_SUBTYPE_DEPTH: u32 = 100;
const MAX_TOTAL_SUBTYPE_CHECKS: u32 = 100_000;
const MAX_INSTANTIATION_DEPTH: u32 = 50;
const MAX_EVALUATE_DEPTH: u32 = 50;
const MAX_TOTAL_EVALUATIONS: u32 = 100_000;
```

### 4.5 Checker

**Purpose**: Thin orchestration layer that walks AST and reports diagnostics.

**Architecture**:
```
    NodeArena (AST)
    SymbolArena (Binder output)
    FlowGraph (Binder output)
         |
         v
    +------------------+
    | CheckerState     |
    |------------------|
    | solver: ref      |  <- Delegates type ops
    | ctx: Context     |  <- Shared state
    +------------------+
         |
         +---> Diagnostics (with source locations)
         +---> Node types (cached TypeId per node)
```

**Checker Context**:
```rust
pub struct CheckerContext<'a> {
    // Options
    pub options: &'a CompilerOptions,
    pub strict_null_checks: bool,

    // Caching
    pub symbol_types: RefCell<FxHashMap<SymbolId, TypeId>>,
    pub node_types: RefCell<FxHashMap<NodeIndex, TypeId>>,

    // Recursion guards
    pub symbol_resolution_stack: RefCell<Vec<SymbolId>>,

    // Diagnostics
    pub diagnostics: RefCell<Vec<Diagnostic>>,

    // Fuel counter
    pub fuel: RefCell<u32>,  // MAX: 500,000
}
```

**Checker Responsibilities**:

| Does | Does NOT |
|------|----------|
| Walk AST nodes | Compute type relations |
| Extract data from AST | Implement subtyping |
| Call Solver queries | Match on TypeKey |
| Report diagnostics | Own type logic |
| Track source locations | Evaluate meta-types |
| Apply flow analysis | Implement inference |

**File Size Rule**: Each checker file under 2000 lines.

```
src/checker/
├── state.rs          # Orchestration (main entry)
├── expr.rs           # Expression checking
├── statements.rs     # Statement checking
├── declarations.rs   # Declaration checking
├── type_checking.rs  # Type validation utilities
├── flow_analysis.rs  # CFA integration
└── ...
```

### 4.6 Emitter

**Purpose**: Generate JavaScript output with transforms.

**Architecture**:
```
    NodeArena (AST)
    TransformContext (directives)
         |
         v
    +------------------+
    | Printer          |
    |------------------|
    | arena: ref       |
    | writer: Writer   |
    | transforms: ctx  |
    +------------------+
         |
         v
    JavaScript + Source Maps
```

**Two-Phase Transform Architecture** (for ES5 downleveling):
```
    Phase 1: Transform (AST -> IR)    [src/transforms/]
    +------------------+
    | Transformer      |
    | - transform_*()  |
    +------------------+
           |
           v
    IRNode (structured)               [src/transforms/ir.rs]
           |
           v
    Phase 2: Print (IR -> String)     [src/transforms/ir_printer.rs]
    +------------------+
    | IRPrinter        |
    | - emit_to_string |
    +------------------+
           |
           v
    JavaScript String
```

**Transform Directives**:
```rust
pub enum EmitDirective {
    ES5Class,
    ES5ClassExpression,
    ES5Namespace,
    ES5Enum,
    ES5ArrowFunction { captures_this: bool },
    ES5AsyncFunction,
    ES5ForOf,
    ES5ObjectLiteral,
    ES5VariableDeclarationList,
    ES5FunctionParameters,
    ES5TemplateLiteral,
    CommonJSExport,
    CommonJSExportDefaultExpr,
    ModuleWrapper { format: ModuleFormat, dependencies: Vec<String> },
}
```

### 4.7 LSP

**Purpose**: IDE features consuming checker output.

**Architecture**:
```
    +------------------+
    | LSP Server       |
    |------------------|
    | project: Project |  <- Multi-file container
    +------------------+
           |
           v
    +------------------+
    | Project          |
    |------------------|
    | files: HashMap   |
    | global_interner  |  <- Shared type interning
    | reverse_deps     |  <- Incremental updates
    +------------------+
           |
           +---> definition.rs     (Go to Definition)
           +---> references.rs     (Find References)
           +---> completions.rs    (Code Completion)
           +---> hover.rs          (Hover Information)
           +---> rename.rs         (Symbol Rename)
           +---> code_actions.rs   (Refactorings)
           +---> ...
```

**Key Properties**:

| Property | Implementation |
|----------|----------------|
| Global type interning | Shared across files |
| Incremental updates | Reverse dependency graph |
| Symbol index | O(1) lookups |
| Persistent state | Checker state retained |
| WASM compatible | No filesystem, no threads |

---

## 5. Data Structures

### 5.1 Type System

```rust
// Type identity: 4-byte handle
pub struct TypeId(pub u32);

// Built-in type IDs (0-99 reserved)
impl TypeId {
    pub const NONE: TypeId = TypeId(0);
    pub const ERROR: TypeId = TypeId(1);
    pub const NEVER: TypeId = TypeId(2);
    pub const UNKNOWN: TypeId = TypeId(3);
    pub const ANY: TypeId = TypeId(4);
    pub const VOID: TypeId = TypeId(5);
    pub const UNDEFINED: TypeId = TypeId(6);
    pub const NULL: TypeId = TypeId(7);
    pub const BOOLEAN: TypeId = TypeId(8);
    pub const NUMBER: TypeId = TypeId(9);
    pub const STRING: TypeId = TypeId(10);
    pub const BIGINT: TypeId = TypeId(11);
    pub const SYMBOL: TypeId = TypeId(12);
    pub const OBJECT: TypeId = TypeId(13);
    pub const BOOLEAN_TRUE: TypeId = TypeId(14);
    pub const BOOLEAN_FALSE: TypeId = TypeId(15);
    pub const FUNCTION: TypeId = TypeId(16);
    pub const PROMISE_BASE: TypeId = TypeId(17);
    pub const FIRST_USER: u32 = 100;
}

// Interned secondary structures
pub struct TypeListId(pub u32);       // Union/intersection members
pub struct ObjectShapeId(pub u32);    // Object properties
pub struct TupleListId(pub u32);      // Tuple elements
pub struct FunctionShapeId(pub u32);  // Function signature
pub struct CallableShapeId(pub u32);  // Overloaded signatures
pub struct TypeApplicationId(pub u32); // Generic<Args>
pub struct ConditionalTypeId(pub u32); // T extends U ? X : Y
pub struct MappedTypeId(pub u32);     // { [K in T]: V }
pub struct TemplateLiteralId(pub u32); // `prefix${T}suffix`
```

### 5.2 Symbol System

```rust
// Symbol identity
pub struct SymbolId(pub u32);

impl SymbolId {
    pub const NONE: SymbolId = SymbolId(u32::MAX);
}

// Symbol flags (bitfield)
pub const FUNCTION_SCOPED_VARIABLE: u32 = 1 << 0;
pub const BLOCK_SCOPED_VARIABLE: u32 = 1 << 1;
pub const PROPERTY: u32 = 1 << 2;
pub const ENUM_MEMBER: u32 = 1 << 3;
pub const FUNCTION: u32 = 1 << 4;
pub const CLASS: u32 = 1 << 5;
pub const INTERFACE: u32 = 1 << 6;
pub const TYPE_PARAMETER: u32 = 1 << 18;
pub const TYPE_ALIAS: u32 = 1 << 19;
pub const ALIAS: u32 = 1 << 21;  // Imports

// Modifier flags
pub const PRIVATE: u32 = 1 << 28;
pub const PROTECTED: u32 = 1 << 29;
pub const ABSTRACT: u32 = 1 << 30;
pub const STATIC: u32 = 1 << 31;
```

### 5.3 Flow System

```rust
// Flow node identity
pub struct FlowNodeId(pub u32);

// Flow node flags
pub const UNREACHABLE: u32 = 1 << 0;
pub const START: u32 = 1 << 1;
pub const BRANCH_LABEL: u32 = 1 << 2;
pub const LOOP_LABEL: u32 = 1 << 3;
pub const ASSIGNMENT: u32 = 1 << 4;
pub const TRUE_CONDITION: u32 = 1 << 5;
pub const FALSE_CONDITION: u32 = 1 << 6;
pub const SWITCH_CLAUSE: u32 = 1 << 7;
pub const ARRAY_MUTATION: u32 = 1 << 8;
pub const CALL: u32 = 1 << 9;

// Flow node structure
pub struct FlowNode {
    pub flags: u32,
    pub id: FlowNodeId,
    pub antecedent: Vec<FlowNodeId>,  // Predecessor nodes
    pub node: NodeIndex,               // Associated AST node
}
```

---

## 6. Performance Requirements

### 6.1 Memory

| Operation | Complexity | Notes |
|-----------|------------|-------|
| Type equality | O(1) | Compare TypeId values |
| Type lookup | O(1) | Index into interner |
| Symbol lookup | O(1) | Index into arena |
| Node access | O(1) | Index into arena |
| String equality | O(1) | Compare Atom values |

**Allocation Strategy**:
- Pre-allocate arenas based on source size
- Reuse string buffer in scanner
- Batch allocations in typed pools
- Zero per-operation heap allocations in hot paths

### 6.2 Speed

| Feature | Implementation |
|---------|----------------|
| Parallel parsing | File-level parallelism with rayon |
| Incremental checking | Reverse dependency tracking |
| Lazy evaluation | Meta-types evaluated on demand |
| Query memoization | Salsa-style caching |
| Fast paths | Identity check before structural |

**Target Benchmarks**:
- Parse: 500K+ lines/second
- Bind: 300K+ lines/second
- Check: 100K+ lines/second (incremental: 10x faster)

---

## 7. Code Organization

```
src/
├── scanner.rs              # SyntaxKind enum, token types
├── scanner_impl.rs         # Lexical analysis implementation
├── interner.rs             # String interning (Atom)
├── char_codes.rs           # Character code constants
│
├── parser/
│   ├── mod.rs              # Module exports
│   ├── base.rs             # NodeIndex, NodeList, TextRange
│   ├── node.rs             # Node struct, NodeArena, typed pools
│   ├── state.rs            # ParserState, recursive descent
│   └── flags.rs            # NodeFlags, ModifierFlags
│
├── binder.rs               # Symbol struct, SymbolFlags
├── binder/
│   └── state.rs            # BinderState, scope tree, flow graph
│
├── solver/
│   ├── mod.rs              # Module exports
│   ├── types.rs            # TypeId, TypeKey, TypeListId, etc.
│   ├── intern.rs           # TypeInterner implementation
│   ├── db.rs               # TypeDatabase, QueryDatabase traits
│   ├── subtype.rs          # Subtyping algorithm
│   ├── infer.rs            # Type inference (Union-Find)
│   ├── instantiate.rs      # Generic substitution
│   ├── evaluate.rs         # Meta-type evaluation
│   ├── lower.rs            # AST -> TypeId bridge
│   ├── visitor.rs          # TypeVisitor trait
│   ├── compat.rs           # TypeScript compatibility layer
│   ├── lawyer.rs           # Judge/Lawyer compat layer (any-propagation)
│   ├── narrowing.rs        # Type narrowing
│   ├── operations.rs       # Type operations
│   ├── diagnostics.rs      # Subtype failure reasons
│   └── tracer.rs           # Zero-cost debug tracing
│
├── checker/
│   ├── mod.rs              # Module exports
│   ├── state.rs            # CheckerState orchestration
│   ├── context.rs          # CheckerContext shared state
│   ├── expr.rs             # Expression checking
│   ├── statements.rs       # Statement checking
│   ├── declarations.rs     # Declaration checking
│   ├── type_checking.rs    # Type validation utilities
│   ├── flow_analysis.rs    # CFA integration
│   ├── symbol_resolver.rs  # Symbol resolution
│   └── types/              # Type-specific modules
│
├── emitter/
│   ├── mod.rs              # Printer, node dispatch
│   ├── expressions.rs      # Expression emission
│   ├── statements.rs       # Statement emission
│   ├── declarations.rs     # Declaration emission
│   ├── functions.rs        # Function emission
│   ├── jsx.rs              # JSX emission
│   ├── types.rs            # Type emission (for .d.ts)
│   ├── es5_helpers.rs      # ES5 downlevel transforms
│   ├── module_wrapper.rs   # AMD/UMD/System wrappers
│   └── module_emission.rs  # Import/export handling
│
├── transforms/
│   ├── mod.rs              # Transform documentation
│   ├── ir.rs               # IR node definitions
│   ├── ir_printer.rs       # IR -> String conversion
│   ├── enum_es5_ir.rs      # Enum ES5 transform
│   ├── class_es5.rs        # Class ES5 transform
│   ├── namespace_es5.rs    # Namespace ES5 transform
│   ├── async_es5.rs        # Async ES5 transform
│   └── ...                 # Additional transforms
│
├── lsp/
│   ├── mod.rs              # Module exports
│   ├── project.rs          # Multi-file project container
│   ├── resolver.rs         # Symbol resolution utilities
│   ├── definition.rs       # Go to Definition
│   ├── references.rs       # Find References
│   ├── completions.rs      # Code Completion
│   ├── hover.rs            # Hover Information
│   ├── rename.rs           # Symbol Rename
│   ├── code_actions.rs     # Refactorings
│   ├── semantic_tokens.rs  # Syntax highlighting
│   ├── document_symbols.rs # File outline
│   ├── folding.rs          # Code folding
│   ├── signature_help.rs   # Parameter hints
│   ├── inlay_hints.rs      # Inline hints
│   └── ...                 # Additional features
│
├── diagnostics.rs          # Diagnostic codes and messages
├── source_file.rs          # SourceFile representation
├── span.rs                 # TextSpan, TextRange
└── ...
```

---

## 8. Anti-Patterns to Avoid

### 8.1 Direct TypeKey Matching in Checker

```rust
// ANTI-PATTERN: Checker matching on TypeKey
fn check_string_index(&mut self, type_id: TypeId) {
    match self.types.lookup(type_id) {
        Some(TypeKey::Object(shape_id)) => {
            // Manual logic that should be in Solver
        }
        Some(TypeKey::Union(list_id)) => {
            // Repeated for every type variant
        }
        _ => {}
    }
}

// CORRECT: Use Solver query or visitor
fn check_string_index(&mut self, type_id: TypeId) {
    let index_type = self.solver.get_string_index_type(type_id);
    // Or use visitor pattern
}
```

### 8.2 Type Computation in Binder

```rust
// ANTI-PATTERN: Binder doing type work
fn bind_variable(&mut self, node: NodeIndex) {
    // WRONG: Computing type during binding
    let type_id = if let Some(init) = initializer {
        self.infer_type(init)  // Type inference!
    } else {
        TypeId::ANY
    };
}

// CORRECT: Binder only creates symbols
fn bind_variable(&mut self, node: NodeIndex) {
    let symbol = self.declare_symbol(name, flags, node);
    // Type will be computed by Checker later
}
```

### 8.3 Per-File Type Interning

```rust
// ANTI-PATTERN: Each file has its own interner
struct FileChecker {
    interner: TypeInterner,  // WRONG: Per-file
}

// CORRECT: Global shared interner
struct CheckerState<'a> {
    interner: &'a TypeInterner,  // Shared reference
}
```

### 8.4 Stack-Based Scope Management

```rust
// ANTI-PATTERN: Scope as mutable stack
fn enter_scope(&mut self) {
    self.scope_stack.push(Scope::new());
}
fn exit_scope(&mut self) {
    self.scope_stack.pop();  // Lost after exit!
}

// CORRECT: Persistent scope tree
fn enter_scope(&mut self, kind: ContainerKind, node: NodeIndex) -> ScopeId {
    let scope = Scope { parent: self.current_scope, kind, node, .. };
    let id = self.scopes.push(scope);  // Persisted
    self.current_scope = id;
    id
}
```

### 8.5 Duplicated Logic Between Components

```rust
// ANTI-PATTERN: Same logic in multiple places
// In checker/expr.rs:
fn is_string_type(type_id: TypeId) -> bool {
    type_id == TypeId::STRING
}

// In solver/subtype.rs:
fn is_string_type(type_id: TypeId) -> bool {
    type_id == TypeId::STRING  // Duplicated!
}

// CORRECT: Single source of truth in Solver
// solver/mod.rs exports is_string_type
// Checker imports from solver
```

### 8.6 God Objects (Files > 3000 Lines)

```rust
// ANTI-PATTERN: 5000-line state.rs
pub struct CheckerState { /* everything */ }
impl CheckerState {
    // 200+ methods in one file
}

// CORRECT: Split by responsibility
// state.rs: Core orchestration (~500 lines)
// expr.rs: Expression checking (~1500 lines)
// statements.rs: Statement checking (~1000 lines)
// declarations.rs: Declaration checking (~1500 lines)
```

---

## 9. Migration Guidelines

When refactoring existing code to match this architecture:

### Step 1: Identify Misplaced Logic
- Find TypeKey matches in Checker code
- Find type computations in Binder
- Find duplicated logic across components

### Step 2: Extract to Correct Component
- Type logic -> Solver
- Symbol logic -> Binder
- AST traversal -> Checker
- Code generation -> Emitter

### Step 3: Add Visitor Support
- Create visitor method for new operation
- Replace all TypeKey matches with visitor call
- Test visitor handles all 24+ type variants

### Step 4: Verify Invariants
- Checker never imports TypeKey
- Binder never imports type computation
- All type equality is O(1)
- All arenas are properly indexed

---

## 10. Glossary

| Term | Definition |
|------|------------|
| Arena | Contiguous memory pool for efficient allocation |
| Atom | Interned string identifier (u32) |
| Coinductive | Greatest Fixed Point semantics for recursive types |
| Flow Node | Control flow graph node for narrowing |
| Interning | Deduplication via hash-consing |
| NodeIndex | Handle to AST node in NodeArena |
| Solver | Central type computation engine |
| SymbolId | Handle to symbol in SymbolArena |
| Thin Node | 16-byte node header (data stored separately) |
| TypeId | Handle to interned type |
| TypeKey | Actual type structure/variant |
| Visitor | Pattern for systematic type traversal |


Important documents:
- `docs/HOW_TO_CODE.md` — coding conventions and patterns to follow

---

## CRITICAL: No Files in Root Directory

**NEVER create files in the project root.** This includes:
- Test `.ts` files — use `tmp/` or the appropriate `crates/*/tests/` directory
- Scratch/debug scripts — use `tmp/`
- Session notes, bug reports, analysis docs — use `docs/` subdirectories
- Any `.md`, `.ts`, `.js`, `.json`, `.sh`, `.py` files

The `.gitignore` blocks all new files in root. Only these root files are allowed:
`Cargo.toml`, `Cargo.lock`, `LICENSE.txt`, `README.md`, `AGENTS.md`, `CLAUDE.md`, `yek.yaml`

---

## Project Status

- This is an unreleased work in progress
- Never worry about backward compatiblity

## Debugging
- **NEVER use `eprintln!`** — use the `tsz-tracing` skill instead.


## CRITICAL: Use Skills

It's very important to use the available skills frequently to maximize productivity and code quality.

### tsz-gemini skill
Use for:
- codebase questions
- architecture understanding
- code reviews
- implementation strategies
- fixing bugs and failing tests

**This skill wraps ask-gemini.mjs - use it when really really stuck!**

### tsz-tracing skill
**🚫 NEVER use `eprintln!` for debugging - use the tracing skill instead.**

Use for debugging:
- conformance test failures
- type inference issues
- narrowing and control flow analysis
- assignability check problems

Quick start:
```bash
TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -- file.ts
```

**Read the full skill at `.claude/skills/tsz-tracing/SKILL.md` for:**
- Adding `#[tracing::instrument]` and `trace!()` calls
- Filtering by module (`TSZ_LOG="wasm::solver::narrowing=trace"`)
- Reading hierarchical tree output

---

## CRITICAL: Debugging with Tracing (NOT eprintln!)

**🚫 NEVER use `eprintln!` for debugging. We have proper tracing infrastructure.**

### Why Not eprintln!
- `eprintln!` statements get left behind in production code
- No filtering capability - all or nothing
- No hierarchical context - hard to understand call relationships
- No timing information
- Creates noise in CI/test output

### Use the Tracing Crate Instead

We use the `tracing` crate with `tracing-tree` for beautiful hierarchical output.

#### Quick Start
```bash
# Run with debug-level tracing in tree format
TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -- file.ts

# Run tests with tracing (capture stderr)
TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo test test_name -- --nocapture 2>&1 | head -200
```

#### Adding Tracing to Code

**For function-level spans (recommended for most functions):**
```rust
use tracing::{trace, debug, span, Level};

#[tracing::instrument(level = "trace", skip(interner), fields(count = types.len()))]
pub fn my_function(interner: &dyn TypeDatabase, types: &[TypeId]) -> TypeId {
    // Function body - automatically creates a span with timing
    trace!("Processing {} types", types.len());
    // ...
}
```

**For inline tracing points:**
```rust
use tracing::{trace, debug};

fn some_function() {
    trace!(type_id = %id.0, "Resolved type");
    debug!(members = members.len(), "Narrowing union");
}
```

**For manual spans (when you need custom scope):**
```rust
use tracing::{trace_span, trace};

fn process_items(items: &[Item]) {
    let _span = trace_span!("process_items", count = items.len()).entered();
    for item in items {
        trace!(item_id = item.id, "Processing item");
    }
}
```

#### Log Levels

| Level | Use For |
|-------|---------|
| `error` | Actual errors, should never happen in normal operation |
| `warn` | Unusual conditions that may indicate problems |
| `info` | High-level milestones (file loaded, check complete) |
| `debug` | Useful debugging info (function entry, major decisions) |
| `trace` | Very detailed info (loop iterations, individual checks) |

#### Filtering Output

```bash
# Only solver module at trace level
TSZ_LOG="wasm::solver=trace" TSZ_LOG_FORMAT=tree cargo run -- file.ts

# Multiple modules with different levels
TSZ_LOG="wasm::solver::narrowing=trace,wasm::checker=debug" TSZ_LOG_FORMAT=tree cargo run -- file.ts

# Specific submodule for focused debugging
TSZ_LOG="wasm::solver::subtype=trace" TSZ_LOG_FORMAT=tree cargo run -- file.ts
```

#### Performance Testing with Tracing

For performance work, add timing spans:
```rust
#[tracing::instrument(level = "debug", skip(self), fields(count = members.len()))]
fn expensive_operation(&self, members: &[TypeId]) -> TypeId {
    // The span automatically records timing
    // ...
}
```

Then run with:
```bash
TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run --release -- file.ts 2>&1 | grep "ms"
```

#### Key Instrumented Areas

Already instrumented (search for `#[tracing::instrument]` or `trace!`):
- `src/solver/narrowing.rs` - Type narrowing operations
- `src/solver/subtype.rs` - Subtype checks
- `src/solver/expression_ops.rs` - Best common type
- `src/solver/intern.rs` - Type interning
- `src/checker/state.rs` - Type caching
- `src/checker/context.rs` - Type context

#### Debugging Workflow

1. **Reproduce the issue** with minimal TypeScript input
2. **Run with tracing**: `TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -- test.ts 2>&1 | head -200`
3. **Narrow the filter** if too verbose: `TSZ_LOG="wasm::solver::narrowing=trace"`
4. **Find the divergence point** - where does the trace show wrong behavior?
5. **Add more tracing** if needed to that specific area
6. **Compare with expected** - what should happen vs what does happen?

---

## Testing
- **ALWAYS use `cargo nextest run` instead of `cargo test`** — nextest is faster and gives better output
- Write unit tests for any new functionality
- It is a good idea to write a failing test first before implementing a feature

## Profiling
- Do NOT bind to port 3000. Disable profiler web UIs (`samply --no-open`, etc).

## Git Workflow
- Work in the `main` branch. Do not create feature branches or PRs.
- Commit frequently with clear messages
- Push branches to remote regularly and rebase from main before and after each commit
- Only add files you touched, do not `git add -A`
- Make semantic and short commit headers
- Important: When syncing, also push to remote


Now, make sure repo is setup properly. Run `scripts/setup.sh`
