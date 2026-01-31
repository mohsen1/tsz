# Solver Refactoring Proposal: Query-Based Judge with Sound Core

**Status**: Proposal  
**Authors**: Architecture Review  
**Date**: January 2026  
**Version**: 2.0

---

## Executive Summary

This document proposes evolving the TSZ Solver into a **query-based type algebra engine** using Salsa for automatic memoization and cycle recovery. The key insight is that the existing **Judge/Lawyer architecture** provides the perfect foundation:

- **Judge** (SubtypeChecker, TypeEvaluator): Pure set-theoretic computations → **Salsa queries**
- **Lawyer** (CompatChecker): TypeScript-specific quirks → **Imperative wrapper**

This separation enables a future **Sound Mode** where users can bypass the Lawyer entirely for strict, mathematically-correct type checking.

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
    /// Strict sound checking (Judge only)
    Sound,
}

impl CheckerState {
    fn is_assignable(&self, source: TypeId, target: TypeId) -> bool {
        match self.options.type_check_mode {
            TypeCheckMode::TypeScript => self.lawyer.is_assignable(source, target),
            TypeCheckMode::Sound => self.judge.is_subtype(source, target),
        }
    }
}
```

Sound Mode would catch issues TypeScript allows:
- Covariant mutable arrays
- Method parameter bivariance
- `any` as both top and bottom type
- Enum to number assignability

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
    
    // === Type Queries ===
    
    /// Get apparent type (unwrap type params, resolve constraints)
    fn apparent_type(&self, type_id: TypeId) -> TypeId;
    
    /// Get members of a type
    fn get_members(&self, type_id: TypeId) -> Arc<Vec<(Atom, TypeId)>>;
    
    /// Get call signatures
    fn get_call_signatures(&self, type_id: TypeId) -> Arc<Vec<CallSignature>>;
    
    /// Get index type: T[K]
    fn get_index_type(&self, object: TypeId, key: TypeId) -> TypeId;
    
    /// Get keyof: keyof T
    fn get_keyof(&self, type_id: TypeId) -> TypeId;
}

// Coinductive recovery functions
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

1. **Define Judge trait** as Salsa query group
2. **Implement `is_subtype` query** with cycle recovery
3. **Implement `evaluate` query** with cycle recovery
4. **Update Lawyer** to call Judge queries instead of SubtypeChecker methods
5. **Remove manual cycle tracking** (`in_progress`, `seen_defs`)

**Key Files**:
- Create `src/solver/judge.rs`
- Enhance `src/solver/salsa_db.rs`
- Refactor `src/solver/subtype.rs` into query implementations
- Refactor `src/solver/evaluate.rs` into query implementations

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

## 6. Open Questions

### 6.1 Salsa Version

- **Salsa 0.17** (current stable) vs **Salsa 2022** (new API)?
- Salsa 2022 has better cycle handling but is still evolving.

### 6.2 DefId Scope

- **Global** (cross-file, enables caching across compilations)
- **Per-compilation** (simpler lifecycle, current choice)

### 6.3 Interface Merging

- **Option A**: Single DefId for merged interface body
- **Option B**: Multiple DefIds, merge at query time

### 6.4 Incremental Checking

Salsa enables incremental recomputation. How deeply to integrate?
- **Minimal**: Use Salsa for caching within a compilation
- **Full**: Use Salsa for cross-compilation incrementality (like rust-analyzer)

---

## 7. Success Criteria

| Metric | Current | Target |
|--------|---------|--------|
| Checker `TypeKey::` matches | 75+ | 0 |
| `SymbolRef` in TypeKey | 4 variants | 0 |
| Conformance pass rate | ~48% | ~55%+ |
| Recursive type handling | Limits | Coinductive |
| TS2589 accuracy | Returns `False` | Returns diagnostic |

---

## Appendix A: Judge Queries Reference

| Query | Input | Output | Cycle Recovery |
|-------|-------|--------|----------------|
| `is_subtype` | `(A, B)` | `bool` | `true` (coinductive) |
| `are_identical` | `(A, B)` | `bool` | `true` (coinductive) |
| `evaluate` | `T` | `TypeId` | Identity (return input) |
| `instantiate` | `(G, Args)` | `TypeId` | `Error` |
| `apparent_type` | `T` | `TypeId` | Identity |
| `get_members` | `T` | `Vec<(Atom, TypeId)>` | Empty |
| `get_call_signatures` | `T` | `Vec<Signature>` | Empty |
| `get_index_type` | `(T, K)` | `TypeId` | `Error` |
| `get_keyof` | `T` | `TypeId` | `never` |

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

## Appendix C: Comparison with Other Systems

| System | Type Engine | Cycle Handling | Caching |
|--------|-------------|----------------|---------|
| **tsc** | Imperative | Depth limits + maybe | Per-checker |
| **Chalk** (Rust traits) | SLG resolution | Full coinduction | Query tables |
| **rust-analyzer** | Salsa queries | Salsa cycles | Salsa DB |
| **tsz (current)** | Imperative | Manual in_progress | Per-evaluator |
| **tsz (proposed)** | Salsa queries | Salsa cycles | Salsa DB |

---

## Appendix D: References

1. `docs/architecture/NORTH_STAR.md` - Target architecture
2. `docs/aspirations/SOUND_MODE_ASPIRATIONS.md` - Sound Mode goals
3. `docs/specs/TS_UNSOUNDNESS_CATALOG.md` - TypeScript quirks to preserve
4. `src/solver/subtype.rs` - Current subtype implementation
5. `src/solver/lawyer.rs` - Current Lawyer implementation
6. `src/solver/salsa_db.rs` - Existing Salsa foundation
7. [Salsa Book](https://salsa-rs.github.io/salsa/) - Salsa documentation
8. [Chalk Book](https://rust-lang.github.io/chalk/book/) - Chalk architecture (reference)
