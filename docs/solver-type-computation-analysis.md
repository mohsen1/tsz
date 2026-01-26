# Solver Type Computation Analysis

**Date**: January 2026
**Status**: Living guidance (foundation-first)
**Scope**: Analyzing opportunities to leverage solver for type computation

---

## Executive Summary

This document analyzes whether the solver module can handle type computation currently performed in `checker/type_computation.rs` and `solver/operations.rs`, and defines a solver-first foundation for future work.

**Focus Statement**: Conformance pass rate (~30% as of Jan 2026) is a lagging indicator. We will not chase the number directly; we will build a correct solver foundation, tighten the checker/solver contract, and document the workflow so later improvements are durable.

**Key Finding**: The solver can and should handle **more** pure type logic. The checker should become a thin orchestration layer for AST traversal, error reporting, and contextual state management.

**Recommended Actions**:
1. Move `get_element_access_type` to solver (90% pure logic)
2. Consolidate nullish type checking (3 duplicate implementations → 1)
3. Create new solver evaluators for array/object literal construction
4. Standardize the checker→solver delegation pattern
5. Document solver-first workflow and migration checklist
6. Enforce “no AST in solver” and “no test-aware behavior” guardrails

---

## Table of Contents

0. [Foundation Focus](#0-foundation-focus-conformance-is-lagging)
1. [Current Architecture](#1-current-architecture)
2. [Solver Capabilities](#2-solver-capabilities)
3. [Migration Analysis](#3-migration-analysis)
4. [Identified Duplications](#4-identified-duplications)
5. [Proposed Architecture](#5-proposed-architecture)
6. [Implementation Recommendations](#6-implementation-recommendations)
7. [Appendix: Module Reference](#7-appendix-module-reference)

---

## 0. Foundation Focus (Conformance Is Lagging)

We are currently around 30% conformance. That number is not the immediate target. The near-term goal is to build a correct, maintainable solver foundation that mirrors tsc semantics and makes future fixes straightforward.

Guiding principles:
1. **Root causes over metrics**: Fix structural issues in solver/checker boundaries rather than chasing pass rates.
2. **Solver-first logic**: Pure TypeId logic belongs in solver; checker handles AST and diagnostics.
3. **Structured results**: Solver returns structured results; checker formats errors.
4. **No shortcuts**: Avoid test-aware branches or file/path-based behavior.
5. **Document the workflow**: Update this guide when adding new evaluators or migration patterns.

---

## 1. Current Architecture

### 1.1 Design Principle

The codebase follows a clean separation of concerns:

> **Solver handles WHAT** (type operations and relations)
> **Checker handles WHERE** (AST traversal, scoping, control flow)

### 1.2 Module Sizes

| Component | Lines | Primary Responsibility |
|-----------|-------|------------------------|
| `solver/operations.rs` | 3,243 | Call resolution, property access, generic instantiation |
| `solver/evaluate.rs` | 615 | Meta-type evaluation (conditionals, mapped, index access) |
| `solver/subtype.rs` | 1,696 | Structural subtype checking |
| `solver/infer.rs` | 600+ | Union-Find generic inference |
| `checker/type_computation.rs` | 3,498 | AST → TypeId orchestration |
| `checker/state.rs` | 6,000+ | Caching, dispatch, relationship checks |

### 1.3 Type Flow Architecture

```
Direction 1: Checker → Solver (Type Creation)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

AST Node (from parser)
    │
    ▼
checker/type_computation.rs
    │
    ▼
solver/lower.rs::TypeLowering::lower_type_node()
    │
    ▼
solver/intern.rs::TypeInterner::intern(TypeKey)
    │
    ▼
TypeId (lightweight u32 handle)


Direction 2: Solver → Checker (Type Information)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

TypeId
    │
    ▼
solver/db.rs::TypeDatabase::lookup(TypeId) → TypeKey
    │
    ▼
Pattern match on TypeKey variants
    │
    ▼
Extract properties/structure
    │
    ▼
Use in checker logic (error reporting, narrowing, etc.)
```

---

## 2. Solver Capabilities

### 2.1 Already Delegated to Solver

The codebase already delegates these operations to the solver:

| Operation | Solver Component | Location |
|-----------|------------------|----------|
| Binary Operations | `BinaryOpEvaluator` | `solver/binary_ops.rs` |
| Call Resolution | `CallEvaluator` | `solver/operations.rs:101-1812` |
| Property Access | `PropertyAccessEvaluator` | `solver/operations.rs:1964-3089` |
| Subtype Checking | `SubtypeChecker` | `solver/subtype.rs` |
| Compatibility | `CompatChecker` | `solver/compat.rs` |
| Generic Inference | `InferenceContext` | `solver/infer.rs` |
| Meta-type Evaluation | `TypeEvaluator` | `solver/evaluate.rs` |
| Contextual Typing | `ContextualTypeContext` | `solver/contextual.rs` |

### 2.2 Core Traits

#### TypeDatabase (21 methods)

Provides low-level type storage and construction:

```rust
pub trait TypeDatabase {
    // Interning and lookup
    fn intern(&self, key: TypeKey) -> TypeId;
    fn lookup(&self, id: TypeId) -> Option<TypeKey>;

    // String interning
    fn intern_string(&self, s: &str) -> Atom;
    fn resolve_atom(&self, atom: Atom) -> String;

    // Component retrieval
    fn type_list(&self, id: TypeListId) -> Arc<[TypeId]>;
    fn object_shape(&self, id: ObjectShapeId) -> Arc<ObjectShape>;
    fn function_shape(&self, id: FunctionShapeId) -> Arc<FunctionShape>;
    // ... more accessors

    // Type constructors
    fn union(&self, members: Vec<TypeId>) -> TypeId;
    fn intersection(&self, members: Vec<TypeId>) -> TypeId;
    fn array(&self, element: TypeId) -> TypeId;
    fn tuple(&self, elements: Vec<TupleElement>) -> TypeId;
    fn object(&self, properties: Vec<PropertyInfo>) -> TypeId;
    fn function(&self, shape: FunctionShape) -> TypeId;
    // ... more constructors
}
```

#### QueryDatabase (9 query methods)

Provides higher-level cached operations:

```rust
pub trait QueryDatabase: TypeDatabase {
    // Meta-type evaluation
    fn evaluate_conditional(&self, cond: &ConditionalType) -> TypeId;
    fn evaluate_index_access(&self, object_type: TypeId, index_type: TypeId) -> TypeId;
    fn evaluate_type(&self, type_id: TypeId) -> TypeId;
    fn evaluate_mapped(&self, mapped: &MappedType) -> TypeId;
    fn evaluate_keyof(&self, operand: TypeId) -> TypeId;

    // Property access
    fn resolve_property_access(&self, object_type: TypeId, prop_name: &str)
        -> PropertyAccessResult;

    // Subtyping
    fn is_subtype_of(&self, source: TypeId, target: TypeId) -> bool;

    // Inference
    fn new_inference_context(&self) -> InferenceContext<'_>;
}
```

### 2.3 Solver Key Features

| Feature | Description |
|---------|-------------|
| **O(1) Type Equality** | Types interned → same structure = same TypeId |
| **Coinductive Semantics** | Automatic cycle handling for recursive types |
| **Union-Find Inference** | Efficient generic type parameter solving |
| **Lazy Evaluation** | Types computed only when needed |
| **Thread Safety** | Sharded DashMap for concurrent access |
| **Depth Limits** | Protection against pathological inputs |

---

## 3. Migration Analysis

### 3.1 Methods in `type_computation.rs`

| Method | Lines | Pure Logic % | Migration Candidate |
|--------|-------|--------------|---------------------|
| `get_element_access_type` | 108 | **90%** | ✅ Strong |
| `get_type_of_conditional_expression` | 64 | 20% | ⚠️ Partial |
| `get_type_of_array_literal` | 179 | 40% | ⚠️ Partial |
| `get_type_of_binary_expression` | 168 | 60% | ⚠️ Already uses solver |
| `get_type_of_prefix_unary` | 118 | 50% | ⚠️ Partial |
| `get_type_of_object_literal` | 307 | 30% | ❌ Too AST-coupled |
| `get_type_of_identifier` | 255 | 20% | ❌ Needs binder |
| `get_type_of_call_expression` | 722 | 70% | ⚠️ Already delegates |

### 3.2 Strong Migration Candidate: `get_element_access_type`

**Current Location**: `checker/type_computation.rs:1123-1231`

**Why It's a Candidate**: 90% pure type logic with zero AST dependencies:

```rust
// Current implementation - in checker
pub(crate) fn get_element_access_type(
    &mut self,
    object_type: TypeId,  // Pure TypeId input
    index_type: TypeId,   // Pure TypeId input
    literal_index: Option<usize>,
) -> TypeId {
    // All logic operates on TypeIds, no AST nodes needed
    match object_key {
        Some(TypeKey::Array(element)) => element,
        Some(TypeKey::Tuple(elements)) => { /* tuple indexing */ }
        Some(TypeKey::ObjectWithIndex(shape_id)) => { /* index signature */ }
        Some(TypeKey::Union(_)) => { /* distribute over union */ }
        // ...
    }
}
```

**Proposed Solver API**:

```rust
// New - in solver/operations.rs
pub struct ElementAccessEvaluator<'a> {
    interner: &'a dyn TypeDatabase,
    no_unchecked_indexed_access: bool,
}

impl<'a> ElementAccessEvaluator<'a> {
    pub fn new(interner: &'a dyn TypeDatabase) -> Self { ... }

    pub fn set_no_unchecked_indexed_access(&mut self, enabled: bool) { ... }

    /// Resolve element access: obj[idx] -> type
    pub fn resolve_element_access(
        &self,
        object_type: TypeId,
        index_type: TypeId,
        literal_index: Option<usize>,
    ) -> ElementAccessResult { ... }
}

pub enum ElementAccessResult {
    Success(TypeId),
    NotIndexable { type_id: TypeId },
    IndexOutOfBounds { type_id: TypeId, index: usize, length: usize },
    NoIndexSignature { type_id: TypeId },
}
```

**Checker Becomes Thin Wrapper**:

```rust
// Simplified checker/type_computation.rs
pub(crate) fn get_type_of_element_access(&mut self, idx: NodeIndex) -> TypeId {
    // 1. Extract AST data (STAYS HERE)
    let access = self.ctx.arena.get_element_access(idx)?;
    let object_type = self.get_type_of_node(access.expression);
    let index_type = self.get_type_of_node(access.argument);
    let literal_index = self.get_literal_index_value(access.argument);

    // 2. Delegate to solver (NEW)
    let evaluator = ElementAccessEvaluator::new(self.ctx.types);
    match evaluator.resolve_element_access(object_type, index_type, literal_index) {
        ElementAccessResult::Success(ty) => ty,
        ElementAccessResult::NotIndexable { type_id } => {
            self.report_error(TS2538, idx, type_id);  // Error reporting stays
            TypeId::ERROR
        }
        // ... handle other cases
    }
}
```

### 3.3 Partial Migration Candidates

#### Array Literal Type Construction

**Extract to Solver**:

```rust
// New solver/array_literal.rs
pub struct ArrayLiteralBuilder<'a> {
    interner: &'a dyn TypeDatabase,
}

impl<'a> ArrayLiteralBuilder<'a> {
    /// Build array type from element types
    pub fn build_array_type(
        &self,
        element_types: Vec<TypeId>,
        contextual: Option<TypeId>,
    ) -> TypeId { ... }

    /// Build tuple type from elements
    pub fn build_tuple_type(
        &self,
        elements: Vec<TupleElement>,
    ) -> TypeId { ... }

    /// Expand spread element into tuple elements
    pub fn expand_spread(
        &self,
        spread_type: TypeId,
    ) -> Vec<TupleElement> { ... }

    /// Determine best common type for array elements
    pub fn best_common_type(
        &self,
        types: &[TypeId],
    ) -> TypeId { ... }
}
```

**Checker Keeps**: AST iteration, contextual type state, error reporting

#### Conditional Expression Result

**Extract to Solver**:

```rust
// solver/operations.rs
pub fn compute_conditional_result_type(
    interner: &dyn TypeDatabase,
    when_true: TypeId,
    when_false: TypeId,
) -> TypeId {
    if when_true == when_false {
        when_true
    } else if when_true == TypeId::NEVER {
        when_false
    } else if when_false == TypeId::NEVER {
        when_true
    } else {
        interner.union(vec![when_true, when_false])
    }
}
```

### 3.4 Must Stay in Checker

These methods **cannot** move to solver:

| Method | Reason |
|--------|--------|
| `get_type_of_identifier` | Requires binder for symbol resolution |
| `get_type_of_object_literal` | Requires AST iteration for properties |
| `get_type_of_function` | Requires scope management, parameter binding |
| `get_type_of_class_member` | Requires class hierarchy traversal |

---

## 4. Identified Duplications

### 4.1 Nullish Type Checking (3 Implementations)

**Problem**: Same logic duplicated in three places:

| Location | Functions |
|----------|-----------|
| `solver/subtype.rs` | `strict_null_checks` handling in subtype rules |
| `checker/nullish.rs` | `is_definitely_nullish()`, `can_be_nullish()`, `get_non_nullish_type()` |
| `checker/optional_chain.rs` | `type_contains_undefined()`, `is_nullish_type()`, `can_be_nullish()` |

**Solution**: Consolidate into `solver/narrowing.rs`:

```rust
// solver/narrowing.rs - single source of truth
pub fn is_nullish(interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(type_id, TypeId::NULL | TypeId::UNDEFINED | TypeId::VOID)
}

pub fn is_definitely_nullish(interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // Check if type is ONLY null/undefined (not a union containing them)
}

pub fn can_be_nullish(interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // Check if type could be null/undefined (including unions)
}

pub fn remove_nullish(interner: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    // Return type with null/undefined removed
}

pub fn type_contains_undefined(interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // Check if undefined is in the type
}
```

**Estimated Impact**: -200 lines of duplicate code

### 4.2 Index Signature Logic

**Problem**: Index signature rules scattered across:
- `solver/operations.rs:3172` - `is_readonly_index_signature()`
- `solver/subtype_rules/objects.rs` - Index signature matching
- `checker/type_computation.rs` - Element access handling

**Solution**: Create `solver/index_signatures.rs`:

```rust
pub struct IndexSignatureResolver<'a> {
    interner: &'a dyn TypeDatabase,
}

impl<'a> IndexSignatureResolver<'a> {
    pub fn resolve_string_index(&self, obj: TypeId) -> Option<TypeId> { ... }
    pub fn resolve_number_index(&self, obj: TypeId) -> Option<TypeId> { ... }
    pub fn is_readonly(&self, obj: TypeId, kind: IndexKind) -> bool { ... }
    pub fn get_index_info(&self, obj: TypeId) -> IndexInfo { ... }
}

pub enum IndexKind { String, Number }

pub struct IndexInfo {
    pub string_index: Option<IndexSignature>,
    pub number_index: Option<IndexSignature>,
}
```

### 4.3 Type Parameter Handling

**Problem**: Generic type handling split across:
- `solver/instantiate.rs` - `TypeSubstitution` with default handling
- `solver/infer.rs` - `InferenceContext` constraint collection
- `checker/type_computation.rs` - Generic instantiation calls

**Solution**: Already well-separated, but ensure checker always delegates to solver for:
- Type argument validation
- Default type parameter resolution
- Constraint satisfaction checking

---

## 5. Proposed Architecture

### 5.1 Target State Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                        CHECKER LAYER                             │
│                                                                  │
│  Responsibilities:                                               │
│  ├─ AST node extraction                                          │
│  ├─ Contextual type state management                             │
│  ├─ Error reporting with source locations                        │
│  ├─ Symbol resolution (binder integration)                       │
│  └─ Caching (node_types, symbol_types)                           │
│                                                                  │
│  type_computation.rs becomes THIN:                              │
│  ├─ Extract AST data                                             │
│  ├─ Pass TypeIds to solver                                       │
│  └─ Report errors from solver results                            │
│                                                                  │
└────────────────────────────┬────────────────────────────────────┘
                             │
                             │ TypeIds only (no AST)
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│                        SOLVER LAYER                              │
│                                                                  │
│  Responsibilities:                                               │
│  ├─ ALL pure type computations (TypeId → TypeId)                 │
│  ├─ Type relationships (subtype, assignability)                  │
│  ├─ Generic inference                                            │
│  └─ Meta-type evaluation                                         │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │ EXISTING EVALUATORS                                         │ │
│  │ ┌───────────────┐ ┌───────────────┐ ┌───────────────┐      │ │
│  │ │ CallEvaluator │ │ PropertyAccess│ │ BinaryOp      │      │ │
│  │ │               │ │ Evaluator     │ │ Evaluator     │      │ │
│  │ └───────────────┘ └───────────────┘ └───────────────┘      │ │
│  │ ┌───────────────┐ ┌───────────────┐ ┌───────────────┐      │ │
│  │ │ TypeEvaluator │ │ Inference     │ │ Contextual    │      │ │
│  │ │ (conditionals)│ │ Context       │ │ TypeContext   │      │ │
│  │ └───────────────┘ └───────────────┘ └───────────────┘      │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │ NEW EVALUATORS (to be created)                              │ │
│  │ ┌───────────────┐ ┌───────────────┐ ┌───────────────┐      │ │
│  │ │ ElementAccess │ │ ArrayLiteral  │ │ ObjectLiteral │      │ │
│  │ │ Evaluator     │ │ Builder       │ │ Builder       │      │ │
│  │ └───────────────┘ └───────────────┘ └───────────────┘      │ │
│  │ ┌───────────────┐ ┌───────────────┐                        │ │
│  │ │ IndexSignature│ │ NullishHelper │                        │ │
│  │ │ Resolver      │ │ (consolidate) │                        │ │
│  │ └───────────────┘ └───────────────┘                        │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### 5.2 API Design Principles

1. **Pure Functions**: Solver functions take `TypeId` and return `TypeId` or structured results
2. **No AST**: Solver never sees AST nodes, only type representations
3. **No Diagnostics**: Solver returns structured errors, checker formats them
4. **Configuration via Setters**: Compiler options passed via setter methods
5. **Stateless where possible**: Prefer stateless functions over stateful evaluators

### 5.3 Error Handling Pattern

```rust
// Solver returns structured results
pub enum ElementAccessResult {
    Success(TypeId),
    NotIndexable { type_id: TypeId },
    IndexOutOfBounds { type_id: TypeId, index: usize, length: usize },
    NoIndexSignature { type_id: TypeId },
}

// Checker converts to diagnostics with source locations
match evaluator.resolve_element_access(obj, idx, lit) {
    ElementAccessResult::Success(ty) => ty,
    ElementAccessResult::NotIndexable { type_id } => {
        self.error(TS2538, node_idx, format_type(type_id));
        TypeId::ERROR
    }
    ElementAccessResult::IndexOutOfBounds { index, length, .. } => {
        self.error(TS2493, node_idx, index, length);
        TypeId::ERROR
    }
    // ...
}
```

---

## 6. Implementation Recommendations

### 6.1 Priority 1: Immediate Wins (Low Risk, High Value)

| Task | Effort | Impact | Risk |
|------|--------|--------|------|
| Move `get_element_access_type` to solver | 2-3 hours | High | Low |
| Consolidate nullish checking | 2-3 hours | Medium | Low |
| Extract `compute_conditional_result_type` | 30 min | Low | Very Low |

#### Task 1.1: Move Element Access to Solver

```rust
// 1. Create ElementAccessEvaluator in solver/operations.rs
pub struct ElementAccessEvaluator<'a> { ... }

// 2. Move logic from checker/type_computation.rs:1123-1231

// 3. Update checker to use new evaluator
pub(crate) fn get_element_access_type(...) -> TypeId {
    let evaluator = ElementAccessEvaluator::new(self.ctx.types);
    evaluator.resolve_element_access(obj, idx, lit)
}
```

#### Task 1.2: Consolidate Nullish Checking

```rust
// 1. Add functions to solver/narrowing.rs
pub fn is_nullish(...) -> bool { ... }
pub fn can_be_nullish(...) -> bool { ... }
pub fn remove_nullish(...) -> TypeId { ... }

// 2. Delete checker/nullish.rs duplicates

// 3. Update checker/optional_chain.rs to use solver functions
```

### 6.2 Priority 2: Medium-term Improvements

| Task | Effort | Impact | Risk |
|------|--------|--------|------|
| Create `ArrayLiteralBuilder` | 4-6 hours | Medium | Medium |
| Create `ObjectLiteralBuilder` | 4-6 hours | Medium | Medium |
| Unify index signature handling | 2-3 hours | Medium | Low |

### 6.3 Priority 3: Architectural Alignment

| Task | Effort | Impact | Risk |
|------|--------|--------|------|
| Add QueryDatabase methods | 2-3 hours | Medium | Low |
| Standardize error propagation | 4-6 hours | High | Medium |
| Document solver APIs | 2-3 hours | Medium | Very Low |

### 6.4 Foundation-first workflow

Use this workflow for any new type computation work:
1. Identify pure TypeId logic and move it to solver.
2. Define a result enum that encodes errors; no diagnostics in solver.
3. Add solver unit tests for the evaluator and checker integration tests.
4. Update this document with the new evaluator and migration notes.
5. Review conformance impact after correctness and documentation are in place.

---

## 7. Appendix: Module Reference

### 7.1 Solver Module Structure

```
solver/
├── mod.rs                 # Module declarations, re-exports
├── types.rs               # TypeId, TypeKey structural representation
├── intern.rs              # Lock-free concurrent type interning
├── db.rs                  # TypeDatabase & QueryDatabase traits
│
├── operations.rs          # CallEvaluator, PropertyAccessEvaluator
├── binary_ops.rs          # BinaryOpEvaluator
├── evaluate.rs            # Meta-type evaluation
├── evaluate_rules/        # Evaluation rule modules
│   ├── apparent.rs
│   ├── conditional.rs
│   ├── index_access.rs
│   ├── keyof.rs
│   ├── mapped.rs
│   └── template_literal.rs
│
├── subtype.rs             # SubtypeChecker core
├── subtype_rules/         # Subtype rule modules
│   ├── intrinsics.rs      # Primitive type rules
│   ├── literals.rs        # Literal type rules
│   ├── unions.rs          # Union/intersection rules
│   ├── tuples.rs          # Tuple/array rules
│   ├── objects.rs         # Object property rules
│   ├── functions.rs       # Function signature rules
│   ├── generics.rs        # Type parameter rules
│   └── conditionals.rs    # Conditional type rules
│
├── infer.rs               # Union-Find generic inference
├── instantiate.rs         # Type parameter substitution
├── compat.rs              # TypeScript compatibility rules
├── lawyer.rs              # Any propagation nuances
│
├── lower.rs               # AST → TypeId conversion
├── contextual.rs          # Contextual typing
├── narrowing.rs           # Type narrowing
├── apparent.rs            # Apparent member kinds
│
├── visitor.rs             # Type visitor pattern
├── format.rs              # Type formatting
├── tracer.rs              # Diagnostic tracing
├── diagnostics.rs         # Pending diagnostics
└── utils.rs               # Utility functions
```

### 7.2 Depth and Iteration Limits

| Constant | Value | Location | Purpose |
|----------|-------|----------|---------|
| `MAX_SUBTYPE_DEPTH` | 100 | `subtype.rs` | Prevent stack overflow |
| `MAX_TOTAL_SUBTYPE_CHECKS` | 100,000 | `subtype.rs` | Prevent infinite loops |
| `MAX_EVALUATE_DEPTH` | 50 | `evaluate.rs` | Prevent infinite expansion |
| `MAX_INSTANTIATION_DEPTH` | 50 | `instantiate.rs` | Prevent infinite generic expansion |
| `MAX_CONSTRAINT_ITERATIONS` | 100 | `infer.rs` | Prevent constraint solving loops |
| `MAX_LOWERING_OPERATIONS` | 100,000 | `lower.rs` | Prevent AST lowering loops |
| `MAX_CONSTRAINT_RECURSION_DEPTH` | 100 | `operations.rs` | Prevent constraint collection loops |

### 7.3 Key Type Definitions

```rust
// Lightweight type handle (u32)
pub struct TypeId(u32);

// Structural type representation
pub enum TypeKey {
    Intrinsic(IntrinsicKind),
    Literal(LiteralValue),
    Array(TypeId),
    Tuple(TupleListId),
    Object(ObjectShapeId),
    ObjectWithIndex(ObjectShapeId),
    Function(FunctionShapeId),
    Callable(CallableShapeId),
    Union(TypeListId),
    Intersection(TypeListId),
    Conditional(ConditionalTypeId),
    Mapped(MappedTypeId),
    Reference(SymbolRef),
    Application(TypeApplicationId),
    TemplateLiteral(TemplateLiteralId),
    TypeParameter(Atom),
    Infer(Atom),
    IndexAccess { object: TypeId, index: TypeId },
    Keyof(TypeId),
}

// Intrinsic type constants
impl TypeId {
    pub const ANY: TypeId = TypeId(0);
    pub const UNKNOWN: TypeId = TypeId(1);
    pub const NEVER: TypeId = TypeId(2);
    pub const VOID: TypeId = TypeId(3);
    pub const NULL: TypeId = TypeId(4);
    pub const UNDEFINED: TypeId = TypeId(5);
    pub const STRING: TypeId = TypeId(6);
    pub const NUMBER: TypeId = TypeId(7);
    pub const BOOLEAN: TypeId = TypeId(8);
    pub const BIGINT: TypeId = TypeId(9);
    pub const SYMBOL: TypeId = TypeId(10);
    pub const OBJECT: TypeId = TypeId(11);
    pub const TRUE: TypeId = TypeId(12);
    pub const FALSE: TypeId = TypeId(13);
    pub const ERROR: TypeId = TypeId(14);
    // User types start at 100
    pub const FIRST_USER: TypeId = TypeId(100);
}
```

---

## Conclusion

The solver module is well-architected and already handles most pure type logic. The main opportunities for improvement are:

1. **Move remaining pure type computations** from checker to solver
2. **Consolidate duplicate implementations** (especially nullish checking)
3. **Create new evaluators** for array/object literal construction
4. **Standardize the delegation pattern** between checker and solver

Following these recommendations will result in:
- Better testability (solver functions can be unit tested without AST)
- Better maintainability (type logic in one place)
- Better reusability (solver works for LSP, CLI, IDE plugins)
- Better performance (solver operations can be optimized independently)

Near-term success is measured by solver coverage, elimination of duplicated logic, and documented workflows. Conformance percentage is a lagging indicator of that foundation.

The checker should evolve to become a thin orchestration layer that extracts AST data, delegates to solver, and reports errors with source locations.
