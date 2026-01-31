# Solver Refactoring Proposal: Query-Based Judge with Sound Core

**Status**: Proposal  
**Authors**: Architecture Review  
**Date**: January 2026  
**Version**: 2.1 (Reviewed)

---

## Executive Summary

**Goal**: TSZ is a **tsc drop-in replacement** with an optional **Sound Mode** for stricter type checking.

This document proposes evolving the TSZ Solver into a **query-based type algebra engine** using Salsa for automatic memoization and cycle recovery. The key insight is that the existing **Judge/Lawyer architecture** provides the perfect foundation:

- **Judge** (SubtypeChecker, TypeEvaluator): Pure set-theoretic computations → **Salsa queries**
- **Lawyer** (CompatChecker): TypeScript-specific quirks → **Imperative wrapper**

This separation enables:
1. **TSC Parity**: Lawyer ensures identical behavior to tsc (default mode)
2. **Sound Mode**: Bypass Lawyer to use Judge directly (opt-in via `--sound`)
3. **Performance**: Salsa caching makes repeated checks O(1)
4. **Correctness**: Proper coinductive cycle handling fixes ~25% of conformance failures

### Current Problems

| Problem | Impact | Root Cause |
|---------|--------|------------|
| Evaluate-before-cycle-check bug | ~25% conformance failures | Manual cycle detection after evaluation |
| Solver-Binder coupling | Testing difficulty | `TypeKey::Ref(SymbolRef)` in Solver |
| Limits as band-aids | Wrong results on hit | Incomplete cycle handling |
| Checker TypeKey violations | Architecture drift | 75+ instances of TypeKey matching |

### Proposed Solution

Replace manual cycle/cache management with **Salsa queries**:

```rust
#[salsa::query_group(JudgeStorage)]
pub trait Judge {
    #[salsa::cycle(coinductive_true)]
    fn is_subtype(&self, source: TypeId, target: TypeId) -> bool;
    
    #[salsa::cycle(coinductive_identity)]
    fn evaluate(&self, type_id: TypeId) -> TypeId;
}
```

Salsa automatically handles:
- **Memoization**: Every query result cached
- **Cycle recovery**: Coinductive semantics via `#[salsa::cycle]`
- **Invalidation**: Incremental recomputation when inputs change

---

## 1. Architecture Vision

### 1.1 The Judge/Lawyer Split

```
                    ┌─────────────────────────────────────┐
                    │            Checker                   │
                    │  (AST traversal, diagnostics)        │
                    └─────────────────┬───────────────────┘
                                      │
                                      ▼
                    ┌─────────────────────────────────────┐
                    │           Lawyer (CompatChecker)     │
                    │  ┌─────────────────────────────┐    │
                    │  │ TypeScript Compatibility     │    │
                    │  │ - any propagation           │    │
                    │  │ - method bivariance         │    │
                    │  │ - freshness checking        │    │
                    │  │ - void return exception     │    │
                    │  └─────────────────────────────┘    │
                    │                 │                    │
                    │                 ▼                    │
                    │  ┌─────────────────────────────┐    │
                    │  │      Judge (Salsa DB)        │    │
                    │  │  ┌───────────────────────┐  │    │
                    │  │  │ is_subtype(A, B)      │  │    │
                    │  │  │ evaluate(T)           │  │    │
                    │  │  │ instantiate(G, Args)  │  │    │
                    │  │  │ get_members(T)        │  │    │
                    │  │  └───────────────────────┘  │    │
                    │  │     Pure Set Theory         │    │
                    │  └─────────────────────────────┘    │
                    └─────────────────────────────────────┘
                                      │
                                      ▼
                              TypeInterner
                           (Global, Shared)
```

### 1.2 Component Responsibilities

| Component | Responsibility | Implementation |
|-----------|---------------|----------------|
| **Judge** | All pure type computations | Salsa query group |
| **Lawyer** | TypeScript-specific overrides | Imperative, wraps Judge |
| **Checker** | AST traversal, diagnostics | Calls Lawyer (or Judge in Sound Mode) |
| **TypeInterner** | Type storage, deduplication | Existing, unchanged |

### 1.3 Sound Mode

The Judge/Lawyer separation enables an opt-in **Sound Mode** (see `docs/aspirations/SOUND_MODE_ASPIRATIONS.md`):

```rust
pub enum TypeCheckMode {
    /// Standard TypeScript behavior (Lawyer + Judge)
    TypeScript,
    /// Strict sound checking (Judge only + Sound Lawyer)
    Sound,
}

impl CheckerState {
    fn is_assignable(&self, source: TypeId, target: TypeId) -> bool {
        match self.options.type_check_mode {
            TypeCheckMode::TypeScript => self.lawyer.is_assignable(source, target),
            TypeCheckMode::Sound => self.sound_lawyer.is_assignable(source, target),
        }
    }
}
```

Sound Mode catches issues TypeScript allows:
- Covariant mutable arrays
- Method parameter bivariance
- `any` as both top and bottom type
- Enum to number assignability
- **Excess property bypass** (see below)

#### 1.3.1 Sticky Freshness: Principled Excess Property Checking

TypeScript's excess property checking is inconsistent - it only applies to "fresh" object literals and is easily bypassed:

```typescript
// TypeScript's bypass loophole
const point3d = { x: 1, y: 2, z: 3 };     // Inferred as { x, y, z }
const point2d: { x: number; y: number } = point3d;  // ✅ No error! z is silently lost

// But direct assignment errors
const point2d: { x: number; y: number } = { x: 1, y: 2, z: 3 };  // ❌ Error
```

**Sound Mode introduces "Sticky Freshness"**: Object literals remain subject to excess property checks as long as they flow through **inferred** types. Freshness only ends at **explicit type annotations**.

```typescript
// Sound Mode behavior
const point3d = { x: 1, y: 2, z: 3 };     // point3d is "sticky fresh"
const point2d: { x: number; y: number } = point3d;  // ❌ TS9001: 'z' is excess

// Explicit opt-out still works
const wide: { x: number; y: number } = point3d as { x: number; y: number };  // ✅ OK
```

**Implementation**:

> ⚠️ **Critical**: Freshness must be tracked by **AST node**, not by **TypeId**!
> 
> Because `TypeInterner` deduplicates structurally identical types, tracking freshness
> by `TypeId` causes "Zombie Freshness" - unrelated literals with the same shape
> would share freshness state, breaking tsc compatibility.

```rust
/// TypeScript Mode: Syntactic freshness (matches tsc exactly)
/// Fresh = the expression IS an ObjectLiteralExpression (or wrapped in parens)
/// Not fresh = the expression is a variable reference, property access, etc.
fn is_fresh_expression(expr: &Expression) -> bool {
    match expr {
        Expression::ObjectLiteral(_) => true,
        Expression::Parenthesized(inner) => is_fresh_expression(inner),
        Expression::AsExpression(inner, _) => is_fresh_expression(inner),
        _ => false,  // Variables, calls, property access = NOT fresh
    }
}

/// Sound Mode: Track freshness through data flow
pub struct StickyFreshnessTracker {
    /// Map from variable SymbolId to whether it holds a "sticky fresh" value
    fresh_bindings: FxHashMap<SymbolId, bool>,
}

impl StickyFreshnessTracker {
    /// Variable initialized with object literal
    pub fn mark_binding_fresh(&mut self, symbol: SymbolId) {
        self.fresh_bindings.insert(symbol, true);
    }
    
    /// Variable assigned to explicit type annotation
    pub fn consume_freshness(&mut self, symbol: SymbolId) {
        self.fresh_bindings.remove(&symbol);
    }
    
    /// Check if variable reference is sticky fresh
    pub fn is_binding_fresh(&self, symbol: SymbolId) -> bool {
        self.fresh_bindings.get(&symbol).copied().unwrap_or(false)
    }
}

impl Checker {
    fn check_excess_properties(&self, expr: &Expression, source: TypeId, target: TypeId) {
        let is_fresh = if self.options.sound_mode {
            // Sound mode: check syntactic OR sticky binding freshness
            is_fresh_expression(expr) || self.is_sticky_fresh_reference(expr)
        } else {
            // TypeScript mode: only syntactic freshness (exact tsc match)
            is_fresh_expression(expr)
        };
        
        if is_fresh {
            self.check_excess_properties_impl(source, target);
        }
    }
}
```

**Why this is correct**:

| Mode | Freshness Source | tsc Compatible? |
|------|------------------|-----------------|
| TypeScript | AST node type (is it a literal?) | ✅ Yes, exact match |
| Sound | AST node OR sticky binding tracking | N/A (opt-in stricter) |

**The "Zombie Freshness" Bug (avoided)**:
```typescript
let x = { a: 1, extra: 2 };  // x is assigned, NOT fresh for tsc
let y = { a: 1, extra: 2 };  // y is a literal expression

// If we tracked by TypeId, x and y would share freshness state!
// With syntactic tracking, only the EXPRESSION matters:
let z: { a: number } = x;  // x is Identifier, not fresh ✅
let w: { a: number } = y;  // y is... wait, y is also Identifier here
                           // Only { a: 1, extra: 2 } DIRECTLY is fresh
```

**Trade-offs**:

| Scenario | TypeScript Mode | Sound Mode |
|----------|-----------------|------------|
| Direct literal assignment | ❌ Excess error | ❌ Excess error |
| Via intermediate variable | ✅ Bypass allowed | ❌ Excess error (correct) |
| Class instance to base | ✅ Allowed | ✅ Allowed (not fresh) |
| Explicit cast/annotation | ✅ Allowed | ✅ Allowed (opt-out) |
| Spread into new object | ✅ Allowed | ✅ Allowed (new object) |

**Why this is principled**:
1. **Respects structural typing**: Interfaces and classes are open (width subtyping allowed)
2. **Respects intent**: Object literals are implicitly exact definitions
3. **Closes the loophole**: Prevents accidental data loss through the bypass
4. **Provides opt-out**: Explicit annotations/casts allow widening when intentional

### 1.4 Critical Design Constraints

Based on architectural review, these constraints are **non-negotiable**:

#### Constraint 1: Inference Stays Outside Salsa

The `InferenceContext` uses `ena` (Union-Find) for mutable unification variables. **This cannot be inside Salsa.**

```
    ┌─────────────────────────────────────────────┐
    │              IMPERATIVE LAYER               │
    │  ┌───────────────────────────────────────┐  │
    │  │         InferenceContext (ena)        │  │
    │  │  - Unification variables              │  │
    │  │  - Mutable Union-Find                 │  │
    │  │  - resolve_generic_call()             │  │
    │  └───────────────┬───────────────────────┘  │
    │                  │ queries                   │
    │                  ▼                           │
    │  ┌───────────────────────────────────────┐  │
    │  │           Judge (Salsa DB)            │  │
    │  │  - is_subtype() for bounds checking   │  │
    │  │  - evaluate() for constraint solving  │  │
    │  │  - Pure, cacheable, cycle-safe        │  │
    │  └───────────────────────────────────────┘  │
    └─────────────────────────────────────────────┘
```

**Rule**: The Judge can only verify fully instantiated types. Inference (solving for `T`) remains imperative, querying the Judge for bounds checks.

#### Constraint 2: "Explain Slow" Pattern for Diagnostics

Salsa memoizes return values, not side effects. Diagnostics are side effects.

```rust
// WRONG: Trying to cache diagnostics
fn is_subtype(db: &dyn Judge, a: TypeId, b: TypeId) -> (bool, Vec<Diagnostic>) {
    // Diagnostics would be cached forever - WRONG
}

// CORRECT: Separate fast check from slow explain
impl Judge {
    /// Fast, cached - returns bool only
    fn is_subtype(&self, a: TypeId, b: TypeId) -> bool;
}

impl Lawyer {
    /// If is_subtype returns false and we need diagnostics,
    /// call this OUTSIDE Salsa to generate error messages
    fn explain_subtype_failure(&self, a: TypeId, b: TypeId) -> SubtypeFailureReason;
}
```

**Rule**: Judge returns `bool`. If `false` and diagnostics needed, Lawyer calls a separate non-Salsa `explain_*` function.

#### Constraint 3: Classification APIs, Not Just Getters

To achieve "zero TypeKey matches in Checker", the Judge must expose **classifiers**, not just getters.

```rust
// WRONG: Chatty API - Checker still has to decide
let is_array = judge.is_array(t);
let is_tuple = judge.is_tuple(t);
let has_iterator = judge.has_symbol_iterator(t);
// Checker reimplements iterable logic

// CORRECT: Classifier API - Judge decides
enum IterableKind { Array, Tuple, IteratorObject, AsyncIterable, NotIterable }
fn classify_iterable(db: &dyn Judge, t: TypeId) -> IterableKind;

enum CallableKind { Function, Constructor, Overloaded, NotCallable }
fn classify_callable(db: &dyn Judge, t: TypeId) -> CallableKind;
```

**Rule**: If the Checker needs to branch on type structure, expose a classifier that returns an enum.

#### Constraint 4: Pre-Merge Strategy for DefId

Declaration merging (interfaces, namespaces) must be handled **before** the Judge sees the type.

```rust
// The query returns a PRE-MERGED view
fn get_interface_members(db: &dyn Judge, def_id: DefId) -> Arc<Vec<PropertyInfo>> {
    // Implementation finds ALL declarations for this DefId
    // (including augmentations) and merges them BEFORE returning
}
```

| Scenario | Strategy |
|----------|----------|
| Interface merging | Single DefId, merged body |
| Class + Namespace | DefId's static members include namespace exports |
| Module augmentation | `get_members(DefId)` includes augmentations implicitly |

**Rule**: The Judge sees merged, complete types. Merging logic lives in the query implementation, not the caller.

#### Constraint 5: Freshness is Syntactic, Not Type-Based

> ⚠️ **Critical for tsc compatibility**: Freshness must be determined by the **AST expression type**, not by marking `TypeId`s as fresh.

**The Problem with TypeId-based Freshness**:
Because `TypeInterner` deduplicates types structurally, two object literals `{ a: 1 }` share the same `TypeId`. If we mark that `TypeId` as "fresh", we create "Zombie Freshness" where unrelated literals interfere with each other.

**The Correct Approach**:
```rust
/// Freshness is a property of the EXPRESSION, not the TYPE
fn is_fresh_expression(expr: &Expression) -> bool {
    match expr {
        Expression::ObjectLiteral(_) => true,      // Direct literal = fresh
        Expression::ArrayLiteral(_) => true,       // Direct literal = fresh
        Expression::Parenthesized(e) => is_fresh_expression(e),
        Expression::AsExpression(e, _) => is_fresh_expression(e),
        _ => false,  // Variable reference = NOT fresh
    }
}
```

**Rule**: 
- Judge knows nothing about freshness
- Checker determines freshness syntactically (by inspecting the AST node)
- Sound Mode can additionally track "sticky" freshness through bindings (by SymbolId, not TypeId)

#### Constraint 6: Configuration as Salsa Input

Compiler options (`strictNullChecks`, `strictFunctionTypes`) must be Salsa inputs, not implicit state.

```rust
#[salsa::query_group(JudgeStorage)]
pub trait Judge {
    #[salsa::input]
    fn strict_null_checks(&self) -> bool;
    
    #[salsa::input]
    fn strict_function_types(&self) -> bool;
    
    // Queries that depend on config will auto-invalidate when config changes
    fn is_subtype(&self, source: TypeId, target: TypeId) -> bool;
}
```

**Implication**: Changing `strict: true` invalidates the entire cache. This is correct behavior.

---

## 2. Problem Analysis

### 2.1 The Evaluate-Before-Cycle-Check Bug

**Current flow in `subtype.rs`:**

```rust
fn check_subtype(&mut self, source: TypeId, target: TypeId) -> SubtypeResult {
    // 1. ❌ Evaluate FIRST - can produce new TypeIds
    let source_eval = self.evaluate_type(source);
    let target_eval = self.evaluate_type(target);
    
    if source_eval != source || target_eval != target {
        return self.check_subtype(source_eval, target_eval);  // New TypeIds!
    }
    
    // 2. ❌ Cycle detection happens AFTER evaluation
    if self.in_progress.contains(&(source, target)) {
        return SubtypeResult::Provisional;
    }
    // ...
}
```

**Problem**: Expansive types like `type Deep<T> = { next: Deep<Box<T>> }` produce fresh TypeIds on each evaluation. The cycle detection never sees the same pair twice.

**Impact**: Estimated ~25% of conformance failures trace back to this bug.

**With Salsa**: Cycle detection is automatic and happens at the query level:

```rust
fn is_subtype(db: &dyn Judge, source: TypeId, target: TypeId) -> bool {
    let source_eval = db.evaluate(source);  // Salsa handles cycles
    let target_eval = db.evaluate(target);  // Results are cached
    // Structural comparison...
}

// Coinductive recovery: cycles assume true
fn coinductive_true(_db: &dyn Judge, _cycle: &salsa::Cycle, ...) -> bool {
    true  // Greatest Fixed Point
}
```

### 2.2 Solver-Binder Coupling

`TypeKey` contains four variants that reference Binder symbols:

```rust
pub enum TypeKey {
    Ref(SymbolRef),           // ← Binder dependency
    TypeQuery(SymbolRef),     // ← Binder dependency
    UniqueSymbol(SymbolRef),  // ← Binder dependency
    ModuleNamespace(SymbolRef), // ← Binder dependency
    // ...
}
```

**Problems:**
- Solver cannot be tested in isolation
- Circular dependency: Solver → TypeResolver → Checker → Solver
- Resolution can fail, spreading error handling everywhere

**Solution**: Replace with Solver-owned `DefId`:

```rust
/// Solver-owned definition identifier
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct DefId(pub u32);

pub enum TypeKey {
    Lazy(DefId),  // Replaces Ref(SymbolRef)
    // TypeQuery, UniqueSymbol, ModuleNamespace lowered during checking
    // ...
}
```

### 2.3 Limits as Band-Aids

`src/limits.rs` contains 30+ constants that compensate for incomplete cycle handling:

| Limit | Current Behavior | Problem |
|-------|------------------|---------|
| `MAX_SUBTYPE_DEPTH: 100` | Return `False` | Wrong for recursive types |
| `MAX_TOTAL_SUBTYPE_CHECKS: 100,000` | Return `False` | Should return `Provisional` |
| `MAX_EVALUATE_DEPTH: 50` | Return input | May miss cycles |

**With Salsa**: Most limits become unnecessary. Salsa's cycle recovery handles recursion correctly. Remaining limits become safety nets that return proper diagnostics (TS2589).

### 2.4 Checker TypeKey Violations

The architecture mandates: *"Checker NEVER inspects type internals"*

Reality: 75+ instances of `TypeKey::` matching in the Checker.

```rust
// VIOLATION: src/checker/assignability_checker.rs
fn ensure_refs_resolved(&mut self, type_id: TypeId) {
    match type_key {
        TypeKey::Ref(symbol_ref) => { ... }
        TypeKey::Union(members) => { ... }
        // 200+ lines of manual traversal
    }
}
```

**Solution**: Expose Judge queries for everything the Checker needs:

```rust
// Judge provides queries
fn get_members(db: &dyn Judge, type_id: TypeId) -> Vec<(Atom, TypeId)>;
fn get_call_signatures(db: &dyn Judge, type_id: TypeId) -> Vec<Signature>;
fn get_index_type(db: &dyn Judge, type_id: TypeId, key: TypeId) -> TypeId;
```

---

## 3. Proposed Architecture

### 3.1 The Judge as Salsa Query Group

```rust
// src/solver/judge.rs

#[salsa::query_group(JudgeStorage)]
pub trait Judge: TypeDatabase {
    // === Configuration Inputs ===
    
    #[salsa::input]
    fn strict_null_checks(&self) -> bool;
    
    #[salsa::input]
    fn strict_function_types(&self) -> bool;
    
    // === Core Type Relations ===
    
    /// Structural subtype check with coinductive cycle recovery
    #[salsa::cycle(recover_subtype_cycle)]
    fn is_subtype(&self, source: TypeId, target: TypeId) -> bool;
    
    /// Type identity (stricter than subtyping)
    fn are_identical(&self, a: TypeId, b: TypeId) -> bool;
    
    // === Type Evaluation ===
    
    /// Evaluate meta-types (Conditional, Mapped, KeyOf, etc.)
    #[salsa::cycle(recover_evaluate_cycle)]
    fn evaluate(&self, type_id: TypeId) -> TypeId;
    
    /// Instantiate generic with type arguments
    #[salsa::cycle(recover_instantiate_cycle)]
    fn instantiate(&self, generic: TypeId, args: Vec<TypeId>) -> TypeId;
    
    // === Type Classifiers (Checker uses these instead of TypeKey matching) ===
    
    /// Classify how a type can be iterated
    fn classify_iterable(&self, type_id: TypeId) -> IterableKind;
    
    /// Classify how a type can be called
    fn classify_callable(&self, type_id: TypeId) -> CallableKind;
    
    /// Get primitive behavior flags
    fn classify_primitive(&self, type_id: TypeId) -> PrimitiveFlags;
    
    /// Classify truthiness for control flow
    fn classify_truthiness(&self, type_id: TypeId) -> TruthinessResult;
    
    // === Type Queries ===
    
    /// Get apparent type (unwrap type params, resolve constraints)
    fn apparent_type(&self, type_id: TypeId) -> TypeId;
    
    /// Get members of a type (handles merging internally)
    fn get_members(&self, type_id: TypeId) -> Arc<Vec<(Atom, TypeId)>>;
    
    /// Get a specific property type
    fn get_property_type(&self, type_id: TypeId, name: Atom) -> Option<TypeId>;
    
    /// Get call signatures
    fn get_call_signatures(&self, type_id: TypeId) -> Arc<Vec<CallSignature>>;
    
    /// Get construct signatures
    fn get_construct_signatures(&self, type_id: TypeId) -> Arc<Vec<CallSignature>>;
    
    /// Get index type: T[K]
    fn get_index_type(&self, object: TypeId, key: TypeId) -> TypeId;
    
    /// Get index signature type (string or number indexer)
    fn get_index_signature(&self, type_id: TypeId, kind: IndexKind) -> Option<TypeId>;
    
    /// Get keyof: keyof T
    fn get_keyof(&self, type_id: TypeId) -> TypeId;
    
    /// Get narrowed type after type guard
    fn get_narrowed_type(&self, original: TypeId, guard: TypeGuard) -> TypeId;
}

// === Classification Enums ===

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum IterableKind {
    Array(TypeId),          // Array<T> - element type
    Tuple,                  // [T, U, V]
    String,                 // string (iterates chars)
    IteratorObject,         // Has Symbol.iterator
    AsyncIterable,          // Has Symbol.asyncIterator
    NotIterable,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CallableKind {
    Function,               // Regular function
    Constructor,            // new-able
    Overloaded(u32),        // Multiple signatures (count)
    NotCallable,
}

bitflags! {
    pub struct PrimitiveFlags: u32 {
        const STRING_LIKE = 1 << 0;
        const NUMBER_LIKE = 1 << 1;
        const BOOLEAN_LIKE = 1 << 2;
        const BIGINT_LIKE = 1 << 3;
        const SYMBOL_LIKE = 1 << 4;
        const VOID_LIKE = 1 << 5;
        const NULLABLE = 1 << 6;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TruthinessResult {
    AlwaysTruthy,
    AlwaysFalsy,
    Sometimes,              // Could be either
}

// === Coinductive recovery functions ===

fn recover_subtype_cycle(_db: &dyn Judge, _cycle: &salsa::Cycle, _s: TypeId, _t: TypeId) -> bool {
    true  // Greatest Fixed Point: cycles are compatible
}

fn recover_evaluate_cycle(_db: &dyn Judge, _cycle: &salsa::Cycle, type_id: TypeId) -> TypeId {
    type_id  // Return input for circular type aliases
}

fn recover_instantiate_cycle(_db: &dyn Judge, _cycle: &salsa::Cycle, generic: TypeId, _args: Vec<TypeId>) -> TypeId {
    TypeId::ERROR  // Circular instantiation is an error
}
```

### 3.2 The Lawyer as Imperative Wrapper

```rust
// src/solver/lawyer.rs (enhanced)

pub struct Lawyer<'db> {
    judge: &'db dyn Judge,
    options: &'db CompilerOptions,
    freshness: FreshnessTracker,
}

impl<'db> Lawyer<'db> {
    /// TypeScript-compatible assignability check
    pub fn is_assignable(&mut self, source: TypeId, target: TypeId) -> bool {
        // 1. Check TypeScript-specific overrides first
        if self.allows_any_escape(source, target) {
            return true;
        }
        
        if self.allows_void_return(source, target) {
            return true;
        }
        
        // 2. Delegate to Judge for structural check
        let judge_result = self.judge.is_subtype(source, target);
        
        // 3. Apply additional TypeScript rules
        if !judge_result {
            // Check bivariance for method parameters
            if self.allows_bivariant_method(source, target) {
                return true;
            }
        }
        
        judge_result
    }
    
    /// Check excess property errors (fresh object literals)
    pub fn check_excess_properties(&mut self, source: TypeId, target: TypeId) -> Vec<Diagnostic> {
        if !self.freshness.is_fresh(source) {
            return vec![];
        }
        // ... excess property checking logic
    }
}
```

### 3.3 The DefId Abstraction

```rust
// src/solver/def.rs

/// Solver-owned definition identifier (replaces SymbolRef in types)
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct DefId(pub u32);

/// Kind of definition (affects evaluation strategy)
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum DefKind {
    /// Type alias: always expand (transparent)
    TypeAlias,
    /// Interface: keep opaque until needed
    Interface,
    /// Class: opaque with nominal brand
    Class,
    /// Enum: special handling for member access
    Enum,
}

/// Storage for lazy type definitions
pub struct DefinitionStore {
    /// DefId -> body TypeId mapping
    bodies: DashMap<DefId, TypeId>,
    /// DefId -> kind mapping
    kinds: DashMap<DefId, DefKind>,
    /// DefId -> type parameters
    type_params: DashMap<DefId, Vec<TypeParamInfo>>,
    /// Next available DefId
    next_id: AtomicU32,
}

impl DefinitionStore {
    /// Register a new type definition
    pub fn register(&self, kind: DefKind, body: TypeId, params: Vec<TypeParamInfo>) -> DefId {
        let id = DefId(self.next_id.fetch_add(1, Ordering::SeqCst));
        self.kinds.insert(id, kind);
        self.bodies.insert(id, body);
        self.type_params.insert(id, params);
        id
    }
}
```

### 3.4 Revised TypeKey

```rust
pub enum TypeKey {
    // === Primitives ===
    Intrinsic(IntrinsicKind),
    Literal(LiteralValue),
    
    // === Structural Types ===
    Object(ObjectShapeId),
    Array(TypeId),
    Tuple(TupleListId),
    Function(FunctionShapeId),
    
    // === Composite Types ===
    Union(TypeListId),
    Intersection(TypeListId),
    
    // === Lazy Types (require Judge queries to resolve) ===
    Lazy(DefId),                    // Named type (interface, class, type alias)
    Application(TypeApplicationId), // Generic<Args>
    Conditional(ConditionalTypeId), // T extends U ? X : Y
    Mapped(MappedTypeId),           // { [K in T]: V }
    IndexAccess(TypeId, TypeId),    // T[K]
    KeyOf(TypeId),                  // keyof T
    
    // === Type Parameters ===
    TypeParameter(TypeParamInfo),
    Infer(TypeParamInfo),           // infer R in conditional types
    
    // === Special ===
    ThisType,
    TemplateLiteral(TemplateLiteralId),
    StringIntrinsic { kind: StringIntrinsicKind, type_arg: TypeId },
    ReadonlyType(TypeId),
    
    // === Error Recovery ===
    Error,
    
    // === REMOVED (migrated during Phase 3) ===
    // Ref(SymbolRef)        → Lazy(DefId)
    // TypeQuery(SymbolRef)  → Lowered during checking
    // UniqueSymbol(SymbolRef) → Nominal brand in Object
    // ModuleNamespace(SymbolRef) → Object type
}
```

---

## 4. Migration Path

### Phase 1: Bug Fixes (Immediate)

**Goal**: Fix the evaluate-before-cycle-check bug without major refactoring.

1. **Move cycle check before evaluation** in `check_subtype`:
   ```rust
   fn check_subtype(&mut self, source: TypeId, target: TypeId) -> SubtypeResult {
       // Identity fast path
       if source == target { return True; }
       
       // ✅ Cycle check FIRST
       if !self.in_progress.insert((source, target)) {
           return Provisional;
       }
       
       // Then evaluate
       let source_eval = self.evaluate_type(source);
       let target_eval = self.evaluate_type(target);
       // ...
   }
   ```

2. **Return `Provisional` on limit hit** (not `False`):
   ```rust
   if self.depth >= MAX_SUBTYPE_DEPTH {
       self.depth_exceeded = true;
       return SubtypeResult::Provisional;  // Not False!
   }
   ```

3. **Add DefId-level cycle detection** for `Lazy` type comparisons.

**Expected Impact**: ~25% conformance improvement.

### Phase 2: Salsa Integration

**Goal**: Migrate Judge queries to Salsa.

#### Prerequisites (Blockers)

> ⚠️ These must be completed BEFORE Salsa integration:

1. **Refactor `CheckerContext` to use trait**:
   ```rust
   // CURRENT (blocker):
   pub struct CheckerContext<'a> {
       pub types: &'a TypeInterner,  // Concrete type
   }
   
   // REQUIRED:
   pub struct CheckerContext<'a> {
       pub db: &'a dyn QueryDatabase,  // Trait object
   }
   ```

2. **Remove `FreshnessTracker` from `src/solver/lawyer.rs`**:
   - Current code tracks freshness by `TypeId` (WRONG - causes Zombie Freshness)
   - Freshness must be tracked in Checker by `NodeIndex` (syntactic)
   - Lawyer should receive `is_fresh: bool` as a parameter, not track it

3. **Remove `RefCell` caches from `src/solver/evaluate.rs`**:
   - Salsa handles caching; internal `RefCell` caches cause thread-safety issues
   - Recursion depth tracking moves to Salsa cycle detection

#### Integration Steps

1. **Define Judge trait** as Salsa query group
2. **Implement `is_subtype` query** with cycle recovery
3. **Implement `evaluate` query** with cycle recovery
4. **Update Lawyer** to call Judge queries instead of SubtypeChecker methods
5. **Remove manual cycle tracking** (`in_progress`, `seen_defs`)

#### Salsa Memoization Scope

> ⚠️ Don't memoize cheap operations - focus on high-value queries.

| Memoize | Don't Memoize |
|---------|---------------|
| `is_subtype(ComplexObject, Interface)` | `is_subtype(string, number)` |
| `evaluate(MappedType)` | `evaluate(Intrinsic)` |
| `get_members(Class)` | Primitive checks |
| Generic instantiations | Identity checks |

**Implementation**: Add fast-path short-circuits BEFORE Salsa query invocation:
```rust
fn is_subtype(db: &dyn Judge, source: TypeId, target: TypeId) -> bool {
    // Fast paths (no Salsa overhead)
    if source == target { return true; }
    if target == TypeId::UNKNOWN { return true; }
    if source == TypeId::NEVER { return true; }
    
    // Delegate to Salsa only for complex cases
    db.is_subtype_impl(source, target)
}
```

**Key Files**:
- Create `src/solver/judge.rs`
- Enhance `src/solver/salsa_db.rs`
- Refactor `src/solver/subtype.rs` into query implementations
- Refactor `src/solver/evaluate.rs` into query implementations
- **Refactor `src/checker/context.rs`** to use `QueryDatabase` trait
- **Remove `FreshnessTracker`** from `src/solver/lawyer.rs`

### Phase 3: DefId Migration

**Goal**: Remove `SymbolRef` from `TypeKey`.

1. **Create `DefId` and `DefinitionStore`**
2. **Add `TypeKey::Lazy(DefId)`** alongside existing `Ref`
3. **Migrate lowering** to produce `Lazy(DefId)` instead of `Ref(SymbolRef)`
4. **Remove legacy variants**:
   - `Ref(SymbolRef)` → `Lazy(DefId)`
   - `TypeQuery(SymbolRef)` → Lowered to concrete type
   - `UniqueSymbol(SymbolRef)` → Nominal brand property
   - `ModuleNamespace(SymbolRef)` → Object type
5. **Remove `TypeResolver` trait**

### Phase 4: Checker Cleanup

**Goal**: Zero `TypeKey` matching in Checker.

1. **Expose Judge queries** for every type inspection the Checker needs
2. **Replace all `TypeKey::` matches** with Judge query calls
3. **Move `ensure_refs_resolved`** logic into Judge
4. **Audit remaining violations**

### Phase 5: Sound Mode (Future)

**Goal**: Enable opt-in strict type checking.

1. **Add `--sound` compiler flag**
2. **Create bypass path** that calls Judge directly
3. **Add diagnostic differences** for Sound Mode
4. **Document differences** from TypeScript

---

## 5. Testing Strategy

### Unit Tests

| Test Category | What to Test |
|---------------|--------------|
| **Cycle Detection** | Recursive types: `type A = { next: A }` |
| **Expansive Types** | `type Deep<T> = { next: Deep<Box<T>> }` |
| **Salsa Memoization** | Same query called twice returns cached result |
| **Coinductive Recovery** | Cycles return `true` not `false` |
| **DefId Resolution** | `Lazy(DefId)` expands correctly |

### Conformance Tests

Monitor these categories during migration:

| Category | Expected Change |
|----------|-----------------|
| Recursive types | **Improve** (proper cycles) |
| Generic instantiation | **Improve** (cached results) |
| "Type too complex" | **Change** (proper TS2589) |
| Interface merging | **Stable** |

### Performance Benchmarks

| Metric | Current | Target |
|--------|---------|--------|
| Subtype cache hit rate | ~60% | ~95% (Salsa) |
| Repeated checks | O(n) | O(1) (memoized) |
| Recursive type depth | 100 (limit) | Unlimited (cycles) |

---

## 6. Design Decisions

> **Context**: TSZ is a tsc drop-in replacement. Sound Mode is an opt-in feature built on top.

### 6.1 Salsa Version

**Decision: Salsa 0.17 (stable)**

| Option | Pros | Cons |
|--------|------|------|
| Salsa 0.17 | Stable, well-documented, production-ready | Older cycle API |
| Salsa 2022 | Better cycle handling, newer patterns | Still evolving, breaking changes |

**Rationale**: For a drop-in replacement, stability is paramount. Salsa 0.17's cycle recovery via `#[salsa::cycle]` is sufficient for our coinductive semantics. We can migrate to Salsa 2022 later when it stabilizes.

**Action**: Use Salsa 0.17. Abstract cycle recovery behind our own traits so migration is localized.

### 6.2 DefId Scope

**Decision: Per-compilation with stable hashing for LSP**

| Mode | DefId Strategy |
|------|----------------|
| CLI (`tsc` replacement) | Fresh DefIds per compilation (simple, matches tsc) |
| LSP/Watch | Content-addressed DefIds (hash of declaration) for stability |

**Rationale**: 
- CLI mode must match tsc semantics exactly. Fresh DefIds per run is fine.
- LSP needs stable IDs across edits for incremental updates. Use content-based hashing.

```rust
impl DefId {
    /// CLI mode: sequential allocation
    pub fn allocate_fresh(store: &DefinitionStore) -> DefId;
    
    /// LSP mode: content-addressed for stability
    pub fn from_content_hash(name: Atom, file: FileId, span: Span) -> DefId;
}
```

### 6.3 Interface Merging

**Decision: Single DefId for merged interface (Option A)**

**Rationale**: This matches tsc's conceptual model. A merged interface is ONE type, not multiple types stitched together.

```typescript
// These are ONE interface with ONE DefId
interface User { name: string; }
interface User { age: number; }

// Conceptually: DefId(42) -> { name: string, age: number }
```

**Implementation**:
1. Binder creates ONE symbol for merged declarations
2. Lowering produces ONE DefId for that symbol
3. `get_members(DefId)` query collects from all declarations

**Edge case - Augmentation**:
Module augmentations also merge into the same DefId. The query implementation must have global visibility of augmentations.

### 6.4 Incremental Checking

**Decision: Two-tier approach**

| Mode | Incrementality | Why |
|------|----------------|-----|
| CLI | **Minimal** (within compilation) | Match tsc behavior, predictable |
| LSP | **Full** (cross-compilation) | Fast IDE response, rust-analyzer style |

**CLI Mode (tsc replacement)**:
- Salsa caches within a single `tsc` invocation
- Each `tsc` run starts fresh (like tsc)
- `--build` mode can reuse across project references

**LSP Mode (IDE)**:
- Full Salsa incrementality
- Edit a file → only re-check affected types
- Cross-file caching persists in memory

```rust
pub enum CompilationMode {
    /// Fresh start, cache only within this run
    Cli,
    /// Persistent cache, incremental updates
    Lsp { db: SalsaDatabase },
}
```

### 6.5 Sound Mode Integration

**Decision: Compiler flag with Lawyer bypass**

```bash
# Standard tsc-compatible mode (default)
tsz check src/

# Sound mode (opt-in)
tsz check --sound src/
```

**Implementation**:
```rust
pub struct CompilerOptions {
    // ... existing options ...
    
    /// Enable sound type checking (stricter than TypeScript)
    pub sound_mode: bool,
}

impl CheckerState {
    fn is_assignable(&self, source: TypeId, target: TypeId) -> bool {
        if self.options.sound_mode {
            // Bypass Lawyer, use Judge directly
            self.judge.is_subtype(source, target)
        } else {
            // Standard TypeScript behavior
            self.lawyer.is_assignable(source, target)
        }
    }
}
```

**Sound Mode catches**:
- Covariant array mutation: `let animals: Animal[] = dogs; animals.push(cat);`
- Method parameter bivariance
- `any` escaping structural checks
- Enum-to-number unsoundness
- Index signature unsoundness

**Sound Mode diagnostics** use different error codes (TS9xxx) to distinguish from standard TypeScript errors.

### 6.6 Error Compatibility

**Decision: Exact error message matching for tsc mode**

| Mode | Error Format |
|------|--------------|
| TypeScript | Exact tsc error codes and messages |
| Sound | New error codes (TS9xxx) with explanations |

**Rationale**: Drop-in replacement means identical error output. Users switching from tsc should see no difference.

```rust
impl Diagnostic {
    /// Format for tsc compatibility
    fn format_typescript(&self) -> String;
    
    /// Format for sound mode (more detailed)
    fn format_sound(&self) -> String;
}
```

---

## 7. Current Code Gaps

> ⚠️ These are mismatches between this proposal and the current codebase that must be fixed.

### Gap 1: FreshnessTracker Uses TypeId (Critical)

**Current Code** (`src/solver/lawyer.rs`):
```rust
pub struct FreshnessTracker {
    fresh_types: FxHashSet<TypeId>,  // WRONG
}
```

**Problem**: TypeIds are structurally interned. Two identical literals `{ a: 1 }` share the same TypeId. Marking one fresh makes both fresh ("Zombie Freshness").

**Fix**: 
- Remove `FreshnessTracker` from Solver entirely
- Checker determines freshness syntactically: `is_fresh_expression(expr: &Expression) -> bool`
- Lawyer receives `is_fresh: bool` parameter, doesn't track it

### Gap 2: CheckerContext Uses Concrete TypeInterner (Blocker)

**Current Code** (`src/checker/context.rs`):
```rust
pub struct CheckerContext<'a> {
    pub types: &'a TypeInterner,  // Concrete type
}
```

**Problem**: Cannot swap in Salsa database without changing this to a trait.

**Fix**:
```rust
pub struct CheckerContext<'a> {
    pub db: &'a dyn QueryDatabase,  // Or generic D: QueryDatabase
}
```

### Gap 3: Manual Cycle Detection in SubtypeChecker

**Current Code** (`src/solver/subtype.rs`):
```rust
pub struct SubtypeChecker {
    in_progress: HashSet<(TypeId, TypeId)>,  // Manual tracking
}
```

**Problem**: Salsa queries must be pure functions. Internal mutable state breaks Salsa's model.

**Fix**: Remove `in_progress` and rely on Salsa's `#[salsa::cycle]` attribute for cycle recovery.

### Gap 4: RefCell Caches in Evaluator

**Current Code** (`src/solver/evaluate.rs`):
```rust
pub struct TypeEvaluator {
    cache: RefCell<FxHashMap<TypeId, TypeId>>,  // Internal cache
    visiting: RefCell<FxHashSet<TypeId>>,        // Cycle detection
}
```

**Problem**: RefCell is not thread-safe. Salsa provides caching; internal caches are redundant and problematic.

**Fix**: Remove RefCell caches, let Salsa handle memoization and cycle detection.

---

## 8. Implementation Risks

Based on architectural review, these are the key risks to monitor:

### Risk A: Coinductive Cycle Topology (High)

**Context**: TypeScript subtyping involves complex cycle patterns, including bivariant cross-recursion (`A <: B` triggers `B <: A`).

**Risk**: Salsa's default cycle handling may not match our coinductive semantics perfectly.

**Mitigation**:
1. Extensive test suite for recursive types
2. Verify bivariant cycles work correctly
3. Consider Salsa 2022's improved cycle API

### Risk B: Loss of Laziness (Medium)

**Context**: Current checker resolves types very lazily. DefId generation may require eager scanning.

**Risk**: If merging/augmentation requires global knowledge upfront, first-check latency may increase.

**Mitigation**:
1. Profile before/after for representative projects
2. Consider two-phase: fast DefId allocation, lazy body resolution
3. Accept some eagerness for correctness

### Risk C: RefCell Elimination (Medium)

**Context**: `evaluate.rs` uses `RefCell` for local caching. Salsa requires pure functions.

**Risk**: Removing `RefCell` may require significant refactoring.

**Mitigation**:
1. Replace `RefCell` cache with Salsa memoization
2. Pass recursion depth as query argument OR rely on Salsa cycle detection
3. Ensure thread safety for parallel checking

### Risk D: Error Type Propagation (Low)

**Context**: `TypeId::ERROR` should silently propagate to prevent cascading errors.

**Risk**: Salsa queries returning `ERROR` may trigger unnecessary diagnostics.

**Mitigation**:
1. Distinguish `Ok(TypeId::ERROR)` (silent) from query failure
2. Checker suppresses diagnostics when source or target is `ERROR`
3. Document error propagation rules

---

## 9. Success Criteria

### Architecture Metrics

| Metric | Current | Target |
|--------|---------|--------|
| Checker `TypeKey::` matches | 75+ | 0 |
| `SymbolRef` in TypeKey | 4 variants | 0 |
| Conformance pass rate | ~48% | 95%+ (tsc parity) |
| Recursive type handling | Limits | Coinductive |
| TS2589 accuracy | Returns `False` | Returns diagnostic |

### TSC Drop-in Replacement Criteria

| Requirement | Metric |
|-------------|--------|
| **Error parity** | Same error codes, same locations as tsc |
| **Behavior parity** | Identical type inference results |
| **CLI parity** | Same flags, same output format |
| **Performance** | Faster than tsc (target: 2-5x) |
| **Zero regressions** | Any tsc-valid code must pass tsz |

### Sound Mode Criteria

| Requirement | Metric |
|-------------|--------|
| **Opt-in only** | Zero impact when `--sound` not specified |
| **Clear diagnostics** | TS9xxx codes explain unsoundness caught |
| **Gradual adoption** | Per-file `// @ts-sound` pragma support |
| **Documentation** | Every caught unsoundness documented with examples |

### Sound Mode Features

| Feature | TypeScript Behavior | Sound Mode Fix |
|---------|--------------------| ---------------|
| **Covariant arrays** | `Animal[] = Dog[]` ✅ | ❌ TS9002: Mutable array covariance |
| **Method bivariance** | Bivariant params ✅ | ❌ TS9003: Contravariant params required |
| **Any escape** | `any` silences errors | ❌ TS9004: `any` doesn't bypass structure |
| **Excess property bypass** | Via intermediate var ✅ | ❌ TS9001: Sticky freshness catches |
| **Enum-number** | `enum E { A }; let n: number = E.A` ✅ | ❌ TS9005: Enum is not number |
| **Index signature** | Missing index okay | ❌ TS9006: Index signature required |

---

## Appendix A: Judge Queries Reference

### Core Relations

| Query | Input | Output | Cycle Recovery |
|-------|-------|--------|----------------|
| `is_subtype` | `(A, B)` | `bool` | `true` (coinductive) |
| `are_identical` | `(A, B)` | `bool` | `true` (coinductive) |

### Evaluation

| Query | Input | Output | Cycle Recovery |
|-------|-------|--------|----------------|
| `evaluate` | `T` | `TypeId` | Identity (return input) |
| `instantiate` | `(G, Args)` | `TypeId` | `Error` |
| `apparent_type` | `T` | `TypeId` | Identity |

### Classifiers (Replace TypeKey Matching)

| Query | Input | Output | Purpose |
|-------|-------|--------|---------|
| `classify_iterable` | `T` | `IterableKind` | for-of, spread |
| `classify_callable` | `T` | `CallableKind` | call expressions |
| `classify_primitive` | `T` | `PrimitiveFlags` | binary ops |
| `classify_truthiness` | `T` | `TruthinessResult` | control flow |

### Property Access

| Query | Input | Output | Cycle Recovery |
|-------|-------|--------|----------------|
| `get_members` | `T` | `Vec<(Atom, TypeId)>` | Empty |
| `get_property_type` | `(T, name)` | `Option<TypeId>` | `None` |
| `get_call_signatures` | `T` | `Vec<Signature>` | Empty |
| `get_construct_signatures` | `T` | `Vec<Signature>` | Empty |
| `get_index_type` | `(T, K)` | `TypeId` | `Error` |
| `get_index_signature` | `(T, kind)` | `Option<TypeId>` | `None` |
| `get_keyof` | `T` | `TypeId` | `never` |
| `get_narrowed_type` | `(T, guard)` | `TypeId` | Identity |

---

## Appendix B: Lawyer Overrides

| TypeScript Behavior | Judge Result | Lawyer Override |
|--------------------|--------------|-----------------|
| `any` assignability | Structural check | Both top & bottom |
| Method bivariance | Contravariant params | Bivariant |
| Object freshness | Width subtyping | Excess property check |
| Void returns | Normal check | Allow any return |
| Weak types | Normal check | TS2559 warning |

---

## Appendix C: "Explain Slow" Diagnostic Pattern

The Lawyer provides diagnostic generation **outside** Salsa:

```rust
/// Structured failure reasons (not cached by Salsa)
pub enum SubtypeFailureReason {
    MissingProperty { name: Atom, in_type: TypeId },
    PropertyTypeMismatch { 
        name: Atom, 
        expected: TypeId, 
        actual: TypeId,
        nested: Box<SubtypeFailureReason>,
    },
    SignatureMismatch {
        kind: SignatureKind,
        reason: Box<SubtypeFailureReason>,
    },
    ParameterCountMismatch { expected: usize, actual: usize },
    ReturnTypeMismatch { expected: TypeId, actual: TypeId },
    IndexSignatureMissing { kind: IndexKind },
    UnionMemberMismatch { 
        member_index: usize,
        member_type: TypeId,
        reason: Box<SubtypeFailureReason>,
    },
    CircularReference,
    TypeTooComplex,
}

impl Lawyer {
    /// Fast path: just check compatibility (cached by Judge)
    pub fn is_assignable(&self, source: TypeId, target: TypeId) -> bool {
        self.judge.is_subtype(source, target) || self.allows_ts_quirk(source, target)
    }
    
    /// Slow path: generate detailed error (NOT cached)
    pub fn explain_assignment_failure(
        &self, 
        source: TypeId, 
        target: TypeId
    ) -> SubtypeFailureReason {
        // Re-run the check with tracing enabled
        // This is only called when we need to display an error
        self.trace_subtype_failure(source, target)
    }
}
```

### Checker Usage

```rust
impl CheckerState {
    fn check_assignment(&mut self, source: TypeId, target: TypeId, span: Span) {
        // Fast check (cached)
        if self.lawyer.is_assignable(source, target) {
            return;
        }
        
        // Slow explain (only when error)
        let reason = self.lawyer.explain_assignment_failure(source, target);
        
        // Convert to diagnostic with source location
        let diagnostic = self.format_assignment_error(reason, span);
        self.diagnostics.push(diagnostic);
    }
}
```

### Why This Pattern?

| Approach | Cache Behavior | Problem |
|----------|---------------|---------|
| Return `(bool, Diagnostic)` | Diagnostic cached forever | Stale error messages |
| Side-effect diagnostics | Not cached at all | Loss of memoization benefit |
| **Explain Slow** | Bool cached, diagnostic fresh | Best of both worlds |

---

## Appendix D: Declaration Merging Strategy

DefId must represent **merged** types. The query implementation handles merging, not the caller.

### Interface Merging

```typescript
// File A
interface User { name: string; }

// File B  
interface User { age: number; }
```

```rust
// Both declarations map to SAME DefId
let user_def_id = DefId(42);

// Query returns MERGED members
fn get_members(db: &dyn Judge, def_id: DefId) -> Arc<Vec<PropertyInfo>> {
    // Implementation:
    // 1. Find ALL declarations for this DefId
    // 2. Collect members from each
    // 3. Merge (later declarations override)
    // 4. Return combined view
}
```

### Class + Namespace Merging

```typescript
class Foo { x: number; }
namespace Foo { export const y = 1; }
```

```rust
// Static members include namespace exports
fn get_static_members(db: &dyn Judge, class_def: DefId) -> Arc<Vec<PropertyInfo>> {
    let class_statics = get_class_static_members(class_def);
    let namespace_exports = get_namespace_exports(class_def);
    merge(class_statics, namespace_exports)
}
```

### Module Augmentation

```typescript
// node_modules/express/index.d.ts
declare namespace Express { interface Request { } }

// src/augment.d.ts
declare namespace Express { 
    interface Request { user: User; }  // Augmentation
}
```

```rust
// The Judge must see augmentations BEFORE queries
fn get_interface_members(db: &dyn Judge, def_id: DefId) -> Arc<Vec<PropertyInfo>> {
    let base_members = get_base_members(def_id);
    let augmentations = db.get_augmentations_for(def_id);  // Global view needed
    merge_all(base_members, augmentations)
}
```

### Implementation Requirements

| Requirement | Why |
|-------------|-----|
| Global augmentation registry | Must know all augmentations before querying |
| DefId stability | Same logical type = same DefId across files |
| Lazy but complete | Can defer merging, but must merge completely when accessed |

---

## Appendix E: Comparison with Other Systems

| System | Type Engine | Cycle Handling | Caching |
|--------|-------------|----------------|---------|
| **tsc** | Imperative | Depth limits + maybe | Per-checker |
| **Chalk** (Rust traits) | SLG resolution | Full coinduction | Query tables |
| **rust-analyzer** | Salsa queries | Salsa cycles | Salsa DB |
| **tsz (current)** | Imperative | Manual in_progress | Per-evaluator |
| **tsz (proposed)** | Salsa queries | Salsa cycles | Salsa DB |

---

## Appendix F: ADR — QueryCache as Final Memoization Architecture

**Status**: Accepted
**Date**: January 2026
**Decision**: QueryCache (not Salsa) is the production memoization layer for the solver.

### Context

The original proposal (Section 3.1) envisioned routing all solver queries through Salsa for automatic memoization and coinductive cycle recovery. After implementing QueryCache as a stepping stone, we evaluated whether full Salsa integration is justified.

### Decision

**QueryCache is the final architecture. Salsa routing is deferred indefinitely.**

### Rationale

1. **Architectural mismatch**: Salsa requires pure query functions with no internal mutable state. The SubtypeChecker and TypeEvaluator use mutable `in_progress` sets, recursion depth counters, and `&mut self` methods. Converting these to pure Salsa queries would require a fundamental rewrite of the solver's core algorithms — not just wiring changes.

2. **TypeInterner is shared mutable state**: The TypeInterner interns new types during subtype checking (e.g., when evaluating conditional types or creating union normalizations). Salsa inputs are set once and then read. Making TypeInterner a Salsa input would require either:
   - Freezing the interner before queries (breaking type creation during checking)
   - Using `Arc<TypeInterner>` with interior mutability (defeating Salsa's invalidation model)

3. **Inference is fundamentally imperative**: Generic type inference uses `ena` (Union-Find) with mutable unification variables. This cannot be inside Salsa (Constraint 1 in Section 1.4). Since inference calls `is_subtype` for bounds checking, the subtype checker must be callable from both Salsa and non-Salsa contexts — making full Salsa routing impractical.

4. **QueryCache already provides the key benefits**:
   - Cross-checker memoization of subtype results (the main performance win)
   - Evaluate-type caching across checker instances
   - Thread-safe via `RwLock<FxHashMap>`
   - Zero architecture disruption

5. **Coinductive cycle recovery works without Salsa**: The existing `in_progress` set with `Provisional` result handling implements coinductive semantics correctly. Salsa's `#[salsa::cycle]` would be cleaner but provides no correctness benefit over the current approach.

### Consequences

- `salsa_db.rs` remains as a working proof-of-concept and test bed, not a production path
- QueryCache in `src/solver/db.rs` is the canonical memoization implementation
- CheckerContext.types uses `&dyn QueryDatabase` (supports both QueryCache and SalsaDatabase)
- Future Salsa work would only be justified if incremental recomputation (LSP file-edit invalidation) becomes a priority — and even then, it would likely wrap QueryCache rather than replace the solver internals

### Alternatives Considered

| Alternative | Why Rejected |
|------------|--------------|
| Full Salsa routing | Requires pure solver functions; fundamental mismatch with mutable solver state |
| Salsa for evaluate only | Partial benefit; evaluate already cached by QueryCache |
| Salsa 2022 (newer API) | Still requires pure functions; same fundamental issue |

---

## Appendix G: References

1. `docs/architecture/NORTH_STAR.md` - Target architecture
2. `docs/aspirations/SOUND_MODE_ASPIRATIONS.md` - Sound Mode goals
3. `docs/specs/TS_UNSOUNDNESS_CATALOG.md` - TypeScript quirks to preserve
4. `src/solver/subtype.rs` - Current subtype implementation
5. `src/solver/lawyer.rs` - Current Lawyer implementation
6. `src/solver/salsa_db.rs` - Existing Salsa foundation (proof-of-concept)
7. `src/solver/db.rs` - QueryCache (production memoization layer)
8. [Salsa Book](https://salsa-rs.github.io/salsa/) - Salsa documentation
9. [Chalk Book](https://rust-lang.github.io/chalk/book/) - Chalk architecture (reference)
