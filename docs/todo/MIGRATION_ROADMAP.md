# TSZ Architecture Migration Roadmap

This document provides a comprehensive migration plan from the current architecture to the North Star design. It addresses critical issues identified through codebase analysis and establishes a phased approach to minimize risk while achieving clean code, better performance, and improved maintainability.

## 1. Executive Summary

### 1.1 Current State Assessment

The TSZ codebase has several architectural issues that impact maintainability and performance:

| Category | Issue | Impact |
|----------|-------|--------|
| **CRITICAL** | CheckerState "God Object" | 12,947 lines in `state.rs` with type logic that belongs in Solver |
| **CRITICAL** | Visitor Pattern Violations | 584 direct `TypeKey::` matches vs proper visitor usage in Checker |
| **CRITICAL** | LSP Per-File Type Interning | Each `ProjectFile` owns its own `TypeInterner` |
| **CRITICAL** | LSP O(N) Reference Search | `find_references` iterates all files linearly |
| **MEDIUM** | Emitter Hybrid Architecture | Partial migration from direct-emit to IR-based |
| **MEDIUM** | Binder Dual Scope Systems | Both legacy stack-based and persistent tree-based scopes |
| **MEDIUM** | CFG Construction Duplication | Flow graph logic in both `binder/state.rs` and `flow_graph_builder.rs` |
| **MEDIUM** | Call Resolution Duplication | Both Solver and Checker implement call logic |

### 1.2 Goals

1. **Clean Code**: No module exceeds 3,000 lines; clear separation of concerns
2. **Performance**: LSP response time < 100ms; O(1) reference lookups
3. **Maintainability**: Single source of truth for each responsibility
4. **Type Safety**: Use visitor pattern for type dispatch; compiler-enforced exhaustiveness

### 1.3 Phased Approach

| Phase | Focus | Priority | Risk Level |
|-------|-------|----------|------------|
| Phase 1 | Checker Cleanup | Highest | Medium |
| Phase 2 | LSP Performance | High | High |
| Phase 3 | Emitter Unification | Medium | Low |
| Phase 4 | Binder Cleanup | Lower | Medium |

---

## 2. Phase 1: Checker Cleanup (Highest Priority)

The Checker module has accumulated responsibilities that properly belong to the Solver. This phase focuses on extracting type computation logic and establishing proper boundaries.

### 2.1 Extract Type Logic to Solver

**Current State** (Jan 2026 Progress):

- `ApplicationEvaluator` created in `src/solver/application.rs`
- `widen_literal_type` already delegates to `type_queries::get_widened_literal_type`
- `CallEvaluator` and `PropertyAccessResult` already in solver

**Target State**: CheckerState becomes a thin orchestration layer; all type logic lives in Solver.

#### Methods Status

| Current Location | Method | Status |
|-----------------|--------|--------|
| `state.rs:6669` | `evaluate_application_type` | ✅ Solver has `ApplicationEvaluator` |
| `type_checking.rs:9173` | `widen_literal_type` | ✅ Delegates to `type_queries` |
| `type_computation.rs` | `get_type_of_call_expression` | ✅ Uses `CallEvaluator` |
| `type_computation.rs` | `get_type_of_property_access` | ✅ Uses `QueryDatabase` |

#### Migration Pattern

For each method to migrate, follow these steps:

```rust
// Step 1: Add query method to Solver (src/solver/operations.rs)
impl TypeDatabase for TypeInterner {
    pub fn evaluate_application_type(&mut self, type_id: TypeId) -> TypeId {
        // Move implementation here
    }
}

// Step 2: Make CheckerState call Solver method
impl CheckerState<'_> {
    #[deprecated(note = "Use types.evaluate_application_type() directly")]
    pub(crate) fn evaluate_application_type(&mut self, type_id: TypeId) -> TypeId {
        self.ctx.types.evaluate_application_type(type_id)
    }
}

// Step 3: Update all callers to use Solver directly
// Before:
let result = checker.evaluate_application_type(ty);
// After:
let result = checker.ctx.types.evaluate_application_type(ty);

// Step 4: Remove deprecated method after all callers updated
```

#### Verification Commands

```bash
# Find all callers of deprecated methods
rg "evaluate_application_type" src/checker/ --type rust

# Verify no direct calls remain after migration
rg "checker\.(evaluate_application|widen_literal)" src/ --type rust
```

### 2.2 Replace TypeKey Matches with Visitor

**Current State**: 584 direct `TypeKey::` matches across the checker modules.

```
src/checker/state.rs:162 matches
src/checker/type_checking.rs:95 matches
src/checker/type_computation.rs:63 matches
src/checker/control_flow.rs:25 matches
src/checker/interface_type.rs:20 matches
src/checker/iterable_checker.rs:20 matches
src/checker/iterators.rs:18 matches
src/checker/generators.rs:15 matches
src/checker/literal_type.rs:15 matches
... (27 files total)
```

**Target State**: Zero `TypeKey::` matches in Checker; all type dispatch through `TypeVisitor` trait.

#### Existing Visitor Infrastructure

The Solver already provides a visitor pattern in `src/solver/visitor.rs`:

```rust
/// Visitor pattern for TypeKey traversal and transformation.
pub trait TypeVisitor: Sized {
    type Output;

    fn visit_intrinsic(&mut self, kind: IntrinsicKind) -> Self::Output;
    fn visit_literal(&mut self, value: &LiteralValue) -> Self::Output;
    fn visit_object(&mut self, shape_id: u32) -> Self::Output;
    fn visit_union(&mut self, list_id: u32) -> Self::Output;
    fn visit_intersection(&mut self, list_id: u32) -> Self::Output;
    fn visit_array(&mut self, element_type: TypeId) -> Self::Output;
    fn visit_tuple(&mut self, list_id: u32) -> Self::Output;
    // ... more variants

    fn default_output() -> Self::Output;
}
```

#### Migration Pattern

For each `match type_key { TypeKey::... }` occurrence:

```rust
// Before (direct match in checker):
match self.ctx.types.get(type_id) {
    TypeKey::Array(elem) => { /* handle array */ }
    TypeKey::Union(list_id) => { /* handle union */ }
    TypeKey::Object(shape_id) => { /* handle object */ }
    _ => { /* default */ }
}

// After (use query methods or visitor):
// Option A: Query methods for simple checks
if self.ctx.types.is_array(type_id) {
    let elem = self.ctx.types.get_array_element(type_id).unwrap();
    // handle array
} else if self.ctx.types.is_union(type_id) {
    // handle union
}

// Option B: Visitor for complex dispatch
struct MyChecker<'a> { ctx: &'a CheckerContext<'a> }
impl TypeVisitor for MyChecker<'_> {
    type Output = TypeId;
    fn visit_array(&mut self, elem: TypeId) -> TypeId { /* ... */ }
    fn visit_union(&mut self, list_id: u32) -> TypeId { /* ... */ }
    fn default_output() -> TypeId { TypeId::ERROR }
}
```

#### Required Query Methods to Add

Add these to `TypeInterner` or a `TypeQueries` trait:

```rust
// src/solver/intern.rs or src/solver/queries.rs
impl TypeInterner {
    pub fn is_array(&self, type_id: TypeId) -> bool;
    pub fn is_tuple(&self, type_id: TypeId) -> bool;
    pub fn is_union(&self, type_id: TypeId) -> bool;
    pub fn is_intersection(&self, type_id: TypeId) -> bool;
    pub fn is_callable(&self, type_id: TypeId) -> bool;
    pub fn is_object(&self, type_id: TypeId) -> bool;
    pub fn is_literal(&self, type_id: TypeId) -> bool;
    pub fn is_intrinsic(&self, type_id: TypeId, kind: IntrinsicKind) -> bool;

    pub fn get_array_element(&self, type_id: TypeId) -> Option<TypeId>;
    pub fn get_union_members(&self, type_id: TypeId) -> Option<&[TypeId]>;
    pub fn get_callable_signatures(&self, type_id: TypeId) -> Option<&[CallSignature]>;
}
```

#### Verification Commands

```bash
# Count current violations
rg "TypeKey::" src/checker/ --type rust | wc -l

# Find specific patterns to migrate
rg "match.*types\.get\(" src/checker/ --type rust -A5

# Verify visitor usage increasing
rg "impl.*TypeVisitor" src/ --type rust
```

### 2.3 Split CheckerState

**Current State** (Jan 2026 Progress):

| File | Lines | Status |
|------|-------|--------|
| `state.rs` | 11,524 | Needs splitting (was 12,947) |
| `type_checking.rs` | 10,551 | Needs splitting (was 11,606) |
| `type_computation.rs` | 3,587 | At threshold |
| `control_flow.rs` | 3,878 | At threshold |

**Target State**: No file exceeds 3,000 lines.

#### Already Extracted (Keep These)

- `expr.rs` (321 lines) - Expression utilities
- `statements.rs` (399 lines) - Statement checking
- `declarations.rs` (1,527 lines) - Declaration checking
- `symbol_resolver.rs` (2,009 lines) - Symbol resolution
- `error_reporter.rs` (2,058 lines) - Error reporting
- `flow_analysis.rs` (1,733 lines) - Flow analysis
- `flow_graph_builder.rs` (2,239 lines) - Flow graph construction

#### Recently Created Modules (Jan 2026)

| New Module | Lines | Status |
|------------|-------|--------|
| `call_checker.rs` | 238 | ✅ Created - call argument collection, overload resolution |
| `class_checker.rs` | 913 | ✅ Created - inheritance/interface/abstract member checking |
| `jsx.rs` | 541 | ✅ Exists - JSX element type checking |

#### Proposed Additional Modules

Extract from `state.rs` and `type_checking.rs`:

| New Module | Responsibility | Est. Lines |
|------------|----------------|------------|
| `assignment_checker.rs` | Assignment compatibility | ~1,500 |
| `generic_checker.rs` | Generic type argument checking | ~1,500 |
| `property_checker.rs` | Property access validation | ~1,500 |

#### Extraction Pattern

```rust
// src/checker/call_checker.rs
pub struct CallChecker<'a, 'ctx> {
    ctx: &'a mut CheckerContext<'ctx>,
}

impl<'a, 'ctx> CallChecker<'a, 'ctx> {
    pub fn new(ctx: &'a mut CheckerContext<'ctx>) -> Self {
        Self { ctx }
    }

    pub fn check_call_expression(&mut self, idx: NodeIndex) -> TypeId {
        // Moved from state.rs
    }

    pub fn check_new_expression(&mut self, idx: NodeIndex) -> TypeId {
        // Moved from state.rs
    }
}

// src/checker/state.rs - becomes thin coordinator
impl CheckerState<'_> {
    pub fn get_type_of_call_expression(&mut self, idx: NodeIndex) -> TypeId {
        CallChecker::new(&mut self.ctx).check_call_expression(idx)
    }
}
```

#### Verification Commands

```bash
# Monitor file sizes
wc -l src/checker/*.rs | sort -n | tail -10

# Find natural extraction boundaries (large method clusters)
rg "pub.*fn.*\(" src/checker/state.rs --type rust | wc -l
```

---

## 3. Phase 2: LSP Performance

The LSP server has performance issues due to per-file type interning and linear reference searches. This phase addresses these scalability concerns.

### 3.1 Global Type Interning

**Current State**: Each `ProjectFile` owns its own `TypeInterner`.

```rust
// src/lsp/project.rs:74-84
pub struct ProjectFile {
    file_name: String,
    root: NodeIndex,
    parser: ParserState,
    binder: BinderState,
    line_map: LineMap,
    type_interner: TypeInterner,  // <-- Per-file interner
    type_cache: Option<TypeCache>,
    scope_cache: ScopeCache,
    strict: bool,
}
```

**Problems**:
- TypeIds from different files are incomparable
- Cross-file type operations require re-interning
- Memory duplication for common types
- Cache invalidation is file-scoped, not project-scoped

**Target State**: Program-level shared `TypeInterner`.

#### Migration Steps

**Step 1: Create Shared TypeInterner in ProjectManager**

```rust
// src/lsp/project.rs
pub struct ProjectManager {
    files: FxHashMap<String, ProjectFile>,
    shared_types: Arc<RwLock<TypeInterner>>,  // New: shared interner
    // ...
}

impl ProjectManager {
    pub fn new() -> Self {
        Self {
            files: FxHashMap::default(),
            shared_types: Arc::new(RwLock::new(TypeInterner::new())),
        }
    }
}
```

**Step 2: Update ProjectFile to Use Shared Interner**

```rust
pub struct ProjectFile {
    file_name: String,
    root: NodeIndex,
    parser: ParserState,
    binder: BinderState,
    line_map: LineMap,
    // type_interner: TypeInterner,  // REMOVED
    type_cache: Option<TypeCache>,
    scope_cache: ScopeCache,
    strict: bool,
}

impl ProjectFile {
    pub fn with_shared_types(
        file_name: String,
        source_text: String,
        shared_types: Arc<RwLock<TypeInterner>>,
    ) -> Self {
        // ...
    }

    pub fn check_with_types(&mut self, types: &mut TypeInterner) {
        // Pass shared interner to checker
    }
}
```

**Step 3: Add TypeId Validation**

```rust
impl TypeInterner {
    /// Validate a TypeId came from this interner
    pub fn validate(&self, type_id: TypeId) -> bool {
        type_id.0 < self.types.len() as u32
    }

    /// Debug assertion for cross-interner TypeId usage
    #[cfg(debug_assertions)]
    pub fn assert_owned(&self, type_id: TypeId) {
        debug_assert!(
            self.validate(type_id),
            "TypeId {} not from this interner (max: {})",
            type_id.0,
            self.types.len()
        );
    }
}
```

**Step 4: Update All Type Operations**

```rust
// Before
let file = self.files.get_mut(file_name)?;
let ty = file.type_interner.intern_object(...);

// After
let mut types = self.shared_types.write();
let ty = types.intern_object(...);
```

#### Verification Commands

```bash
# Find all TypeInterner constructions
rg "TypeInterner::new\(\)" src/lsp/ --type rust

# Find all per-file type_interner accesses
rg "\.type_interner" src/lsp/ --type rust

# Verify shared interner usage
rg "shared_types" src/lsp/ --type rust
```

### 3.2 Symbol Index

**Current State**: Reference search iterates all files linearly.

```rust
// src/lsp/project.rs:1036
for file in self.files.values_mut() {
    // Check each file for references
}
```

**Target State**: O(1) symbol-to-locations lookup via index.

#### Data Structure Design

```rust
// src/lsp/symbol_index.rs
use rustc_hash::FxHashMap;

/// Global symbol index for O(1) reference lookups.
pub struct SymbolIndex {
    /// Symbol name → files containing that symbol
    name_to_files: FxHashMap<String, Vec<String>>,

    /// (file, symbol_id) → all reference locations in that file
    symbol_refs: FxHashMap<(String, SymbolId), Vec<Location>>,

    /// Symbol name → definition locations (for go-to-definition)
    definitions: FxHashMap<String, Vec<Location>>,

    /// Export index: module path → exported names
    exports: FxHashMap<String, Vec<String>>,

    /// Import index: file → imported symbols with source modules
    imports: FxHashMap<String, Vec<ImportInfo>>,
}

#[derive(Clone)]
pub struct ImportInfo {
    pub local_name: String,
    pub source_module: String,
    pub exported_name: String,
    pub kind: ImportKind,
}

impl SymbolIndex {
    pub fn new() -> Self {
        Self {
            name_to_files: FxHashMap::default(),
            symbol_refs: FxHashMap::default(),
            definitions: FxHashMap::default(),
            exports: FxHashMap::default(),
            imports: FxHashMap::default(),
        }
    }

    /// Find all references to a symbol across the project
    pub fn find_references(&self, name: &str) -> Vec<Location> {
        let mut result = Vec::new();
        if let Some(files) = self.name_to_files.get(name) {
            for file in files {
                // Collect refs from each file that has this symbol
                // ...
            }
        }
        result
    }

    /// Update index for a single file (on file change)
    pub fn update_file(&mut self, file_name: &str, binder: &BinderState) {
        self.remove_file(file_name);
        self.index_file(file_name, binder);
    }

    /// Remove a file from the index
    pub fn remove_file(&mut self, file_name: &str) {
        // Remove all entries for this file
        for files in self.name_to_files.values_mut() {
            files.retain(|f| f != file_name);
        }
        self.symbol_refs.retain(|(f, _), _| f != file_name);
        self.exports.remove(file_name);
        self.imports.remove(file_name);
    }

    /// Index a file during binding
    fn index_file(&mut self, file_name: &str, binder: &BinderState) {
        for (sym_id, symbol) in binder.symbols.iter() {
            let name = &symbol.escaped_name;

            // Add to name → files index
            self.name_to_files
                .entry(name.clone())
                .or_default()
                .push(file_name.to_string());

            // Index definitions
            for decl in &symbol.declarations {
                // Add to definitions index
            }

            // Index exports
            if symbol.is_exported() {
                self.exports
                    .entry(file_name.to_string())
                    .or_default()
                    .push(name.clone());
            }
        }
    }
}
```

#### Integration with ProjectManager

```rust
pub struct ProjectManager {
    files: FxHashMap<String, ProjectFile>,
    shared_types: Arc<RwLock<TypeInterner>>,
    symbol_index: SymbolIndex,  // New: global symbol index
}

impl ProjectManager {
    pub fn add_file(&mut self, file_name: String, source_text: String) {
        let file = ProjectFile::new(file_name.clone(), source_text);

        // Update symbol index
        self.symbol_index.update_file(&file_name, file.binder());

        self.files.insert(file_name, file);
    }

    pub fn update_file(&mut self, file_name: &str, source_text: String) {
        // Remove old index entries
        self.symbol_index.remove_file(file_name);

        // Re-parse and re-bind
        let file = ProjectFile::new(file_name.to_string(), source_text);

        // Re-index
        self.symbol_index.update_file(file_name, file.binder());

        self.files.insert(file_name.to_string(), file);
    }

    pub fn find_references(&self, file_name: &str, position: Position) -> Vec<Location> {
        // Use index for O(1) lookup instead of iterating all files
        let file = self.files.get(file_name)?;
        let symbol_name = file.resolve_symbol_at(position)?;

        self.symbol_index.find_references(&symbol_name)
    }
}
```

#### Verification Commands

```bash
# Benchmark before/after
cargo bench --bench lsp_references

# Count file iterations in find_references
rg "for.*file.*in.*files" src/lsp/project.rs --type rust
```

### 3.3 Reverse Dependency Graph

**Current State**: No tracking of which files depend on which.

**Target State**: File → dependents mapping for efficient incremental updates.

#### Data Structure

```rust
// src/lsp/dependency_graph.rs
use rustc_hash::{FxHashMap, FxHashSet};

/// Tracks file dependencies for incremental updates.
pub struct DependencyGraph {
    /// file → files it imports from
    dependencies: FxHashMap<String, FxHashSet<String>>,

    /// file → files that import it (reverse lookup)
    dependents: FxHashMap<String, FxHashSet<String>>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            dependencies: FxHashMap::default(),
            dependents: FxHashMap::default(),
        }
    }

    /// Record that `file` imports from `imported_file`
    pub fn add_dependency(&mut self, file: &str, imported_file: &str) {
        self.dependencies
            .entry(file.to_string())
            .or_default()
            .insert(imported_file.to_string());

        self.dependents
            .entry(imported_file.to_string())
            .or_default()
            .insert(file.to_string());
    }

    /// Get all files that need re-checking when `file` changes
    pub fn get_affected_files(&self, file: &str) -> Vec<String> {
        let mut affected = FxHashSet::default();
        let mut queue = vec![file.to_string()];

        while let Some(current) = queue.pop() {
            if affected.insert(current.clone()) {
                if let Some(deps) = self.dependents.get(&current) {
                    queue.extend(deps.iter().cloned());
                }
            }
        }

        affected.into_iter().collect()
    }

    /// Update dependencies for a file (call after re-binding)
    pub fn update_file(&mut self, file: &str, imports: &[String]) {
        // Remove old dependencies
        if let Some(old_deps) = self.dependencies.remove(file) {
            for dep in old_deps {
                if let Some(dependents) = self.dependents.get_mut(&dep) {
                    dependents.remove(file);
                }
            }
        }

        // Add new dependencies
        for import in imports {
            self.add_dependency(file, import);
        }
    }
}
```

#### Usage for Incremental Updates

```rust
impl ProjectManager {
    pub fn on_file_change(&mut self, file_name: &str, new_content: String) {
        // 1. Re-parse the changed file
        self.update_file(file_name, new_content);

        // 2. Get affected files from dependency graph
        let affected = self.dependency_graph.get_affected_files(file_name);

        // 3. Invalidate type caches for affected files only
        for affected_file in &affected {
            if let Some(file) = self.files.get_mut(affected_file) {
                file.invalidate_type_cache();
            }
        }

        // 4. Update symbol index
        self.symbol_index.update_file(file_name, ...);
    }
}
```

---

## 4. Phase 3: Emitter Unification

The emitter has a hybrid architecture with both direct-emit and IR-based paths. This phase completes the IR migration.

### 4.1 Complete IR Migration

**Current State**: Mix of direct emission and IR transforms.

Files with emit code:
- `src/emitter/mod.rs` (1,895 lines) - Main emitter
- `src/emitter/expressions.rs` (246 lines) - Expression emission
- `src/emitter/statements.rs` (469 lines) - Statement emission
- `src/emitter/declarations.rs` (557 lines) - Declaration emission
- `src/emitter/module_emission.rs` (1,393 lines) - Module transforms

**Target State**: All emission through IR transforms with unified source map generation.

#### Migration Pattern

```rust
// Before (direct emit):
fn emit_function(&mut self, node: NodeIndex) {
    self.output.push_str("function ");
    self.emit_identifier(node.name);
    self.output.push('(');
    // ...
}

// After (IR-based):
fn transform_function(&mut self, node: NodeIndex) -> IRNode {
    IRNode::Function {
        name: self.transform_identifier(node.name),
        params: self.transform_params(node.params),
        body: self.transform_body(node.body),
        source_span: node.span,
    }
}

// Separate IR → text emission phase
fn emit_ir(&mut self, ir: &IRNode) {
    match ir {
        IRNode::Function { name, params, body, source_span } => {
            self.record_source_map(source_span);
            self.output.push_str("function ");
            self.emit_ir(name);
            // ...
        }
    }
}
```

#### Steps

1. **Identify remaining direct-emit paths**:
   ```bash
   rg "self\.output\.push" src/emitter/ --type rust | wc -l
   rg "emit_" src/emitter/ --type rust | wc -l
   ```

2. **Convert each to IR transform**:
   - Create corresponding `IRNode` variant
   - Replace direct emission with IR construction
   - Add IR emission in centralized emitter

3. **Remove direct-emit code**:
   - After all paths converted, remove `emit_*` methods
   - Keep only `transform_*` and IR emission

4. **Unify source map generation**:
   - Source maps generated only from IR spans
   - Single source of truth for mappings

---

## 5. Phase 4: Binder Cleanup

The binder has accumulated dual systems that should be unified.

### 5.1 Remove Legacy Scope System

**Current State**: Both `scope_chain` (stack-based) and persistent scope tree coexist.

```rust
// src/binder/state.rs - 17 occurrences of scope_chain
```

**Target State**: Only persistent scope tree.

#### Migration Steps

1. **Identify scope_chain usages**:
   ```bash
   rg "scope_chain" src/binder/ --type rust -n
   ```

2. **Convert each to persistent scope API**:
   ```rust
   // Before (stack-based):
   self.scope_chain.push(new_scope);
   // ... bind children
   self.scope_chain.pop();

   // After (persistent tree):
   let child_scope = self.create_child_scope(parent_scope_id);
   // ... bind children with child_scope
   // No explicit pop needed - tree structure handles it
   ```

3. **Remove scope_chain field** after all usages converted.

4. **Simplify scope resolution** to use only tree traversal.

### 5.2 Consolidate CFG Builders

**Current State**: Flow graph logic exists in two places.

| File | Lines | Purpose |
|------|-------|---------|
| `src/binder/state.rs` | 5,230 | Original CFG construction |
| `src/checker/flow_graph_builder.rs` | 2,239 | Refined CFG builder |

**Target State**: Single CFG implementation.

#### Migration Steps

1. **Compare implementations**:
   ```bash
   # Find CFG-related functions in each
   rg "fn.*flow|fn.*cfg|fn.*graph" src/binder/state.rs --type rust
   rg "fn.*flow|fn.*cfg|fn.*graph" src/checker/flow_graph_builder.rs --type rust
   ```

2. **Determine primary implementation**:
   - `flow_graph_builder.rs` appears to be the newer, more focused implementation
   - Verify it handles all cases covered by `binder/state.rs`

3. **Migrate missing features** to chosen implementation.

4. **Remove duplicate** from `binder/state.rs`.

5. **Update all references** to use single implementation.

---

## 6. Phase 5: Salsa Query System (Future)

The codebase contains `src/solver/salsa_db.rs` behind the `experimental_salsa` feature flag. This phase stabilizes query-based incremental computation.

### 5.1 Current State

```rust
// src/solver/salsa_db.rs exists with QueryDatabase trait
// Feature flag: experimental_salsa
```

### 5.2 Goals

- Fully integrate Salsa query system for memoization
- Enable fine-grained incremental checking
- Replace manual caching with query-based invalidation

### 5.3 Migration Steps

1. **Audit existing Salsa usage**:
   ```bash
   rg "salsa" src/ --type rust
   rg "experimental_salsa" Cargo.toml
   ```

2. **Identify query candidates**:
   - Type resolution queries
   - Symbol type queries
   - Subtype relation queries

3. **Convert manual caches to Salsa queries**:
   ```rust
   // Before (manual cache)
   fn get_type_of_symbol(&self, id: SymbolId) -> TypeId {
       if let Some(ty) = self.cache.get(&id) { return *ty; }
       let ty = self.compute_type_of_symbol(id);
       self.cache.insert(id, ty);
       ty
   }

   // After (Salsa query)
   #[salsa::tracked]
   fn type_of_symbol(db: &dyn TypeDatabase, id: SymbolId) -> TypeId {
       // Salsa handles memoization and invalidation
       compute_type_of_symbol(db, id)
   }
   ```

4. **Stabilize and enable by default**

### 5.4 Dependencies

- Requires Phase 1 (Checker Cleanup) to be mostly complete
- Type logic must be in Solver for clean query boundaries

---

## 7. Additional Architectural Gaps

### 7.1 Flow Analysis Boundary

**Current State**: Flow analysis exists in two places:
- `src/checker/flow_analysis.rs` - AST traversal, assignment tracking
- `src/solver/flow_analysis.rs` - Type narrowing, FlowFacts

**Gap**: The boundary between these files doesn't cleanly follow "Solver = WHAT, Checker = WHERE".

**Resolution**:
- Checker's `flow_analysis.rs`: Track which nodes have flow effects (WHERE)
- Solver's `flow_analysis.rs`: Compute narrowed types (WHAT)
- Ensure no type computation in Checker's flow analysis

### 7.2 Type Environment Migration

**Current State**: `CheckerState` manages `TypeEnvironment` population via:
- `build_type_environment()`
- `get_type_of_symbol()`
- `resolve_named_type_reference()`

**Gap**: Symbol resolution for types is intertwined with Checker state.

**Resolution**:
- Move `resolve_named_type_reference` to Solver
- Checker provides symbol data, Solver resolves types
- Enables pure Solver that doesn't depend on Checker state

---

## 8. Testing Strategy

### 6.1 Regression Testing

Each migration step must maintain existing behavior.

```bash
# Run full test suite after each change
cargo test --all

# Run conformance tests as ground truth
cargo run --bin tsz_conformance -- run

# Run specific checker tests
cargo test --package tsz --lib checker

# Run LSP tests
cargo test --package tsz --lib lsp
```

#### Test Categories

| Category | Command | Purpose |
|----------|---------|---------|
| Unit Tests | `cargo test` | Individual function behavior |
| Conformance | `cargo run --bin tsz_conformance` | TypeScript compatibility |
| Integration | `cargo test --test integration` | End-to-end flows |
| Benchmark | `cargo bench` | Performance regression |

### 6.2 Performance Testing

Track metrics before and after each phase.

```bash
# Benchmark type checking
cargo bench --bench checker

# Benchmark LSP operations
cargo bench --bench lsp

# Profile memory usage
cargo build --release && heaptrack ./target/release/tsz check large_project/

# Measure check time
time cargo run --release -- check large_project/
```

#### Key Metrics

| Metric | Current | Phase 1 Target | Phase 2 Target |
|--------|---------|----------------|----------------|
| Check time (1000 files) | TBD | Same | Same |
| LSP hover response | TBD | Same | < 50ms |
| LSP find references | TBD | Same | < 100ms |
| Memory per file | TBD | Same | -20% |

---

## 9. Risk Mitigation

### 7.1 Feature Flags

Hide new implementations behind feature flags for gradual rollout.

```rust
// Cargo.toml
[features]
default = []
new_type_interner = []
symbol_index = []
unified_emitter = []

// Code
#[cfg(feature = "new_type_interner")]
fn get_type(&self) -> TypeId {
    self.shared_types.get(...)
}

#[cfg(not(feature = "new_type_interner"))]
fn get_type(&self) -> TypeId {
    self.file.type_interner.get(...)
}
```

#### Rollout Plan

1. **Week 1-2**: New code behind feature flag, off by default
2. **Week 3-4**: Enable flag in CI, test extensively
3. **Week 5-6**: Enable flag by default, old code deprecated
4. **Week 7-8**: Remove old code and feature flag

### 7.2 Rollback Plan

For each phase, maintain ability to rollback.

1. **Keep old code** until new implementation proven:
   - Mark deprecated but don't delete
   - Run both paths in tests, compare results

2. **Git tags** at stable points:
   ```bash
   git tag pre-phase1-checker-cleanup
   git tag post-phase1-checker-cleanup
   ```

3. **Documentation of rollback steps**:
   ```bash
   # Rollback Phase 1
   git checkout pre-phase1-checker-cleanup
   # OR
   cargo build --no-default-features  # Uses old paths
   ```

---

## 10. Success Metrics

### 8.1 Code Quality Metrics

| Metric | Current | Target | Measurement |
|--------|---------|--------|-------------|
| `state.rs` lines | 12,947 | < 3,000 | `wc -l src/checker/state.rs` |
| `type_checking.rs` lines | 11,606 | < 3,000 | `wc -l src/checker/type_checking.rs` |
| TypeKey matches in Checker | 584 | 0 | `rg "TypeKey::" src/checker/ \| wc -l` |
| TypeInterner per file | Yes | No | Code audit |
| O(N) reference search | Yes | No | Code audit |

### 8.2 Performance Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| LSP hover response | < 50ms | `cargo bench --bench lsp_hover` |
| LSP find references | < 100ms | `cargo bench --bench lsp_references` |
| Memory per 1000 files | Stable | `heaptrack` profiling |
| Check time regression | < 5% | `cargo bench --bench checker` |

### 8.3 Quality Gates

Before merging any phase:

- [ ] All existing tests pass
- [ ] Conformance test pass rate unchanged
- [ ] Performance benchmarks within 5% of baseline
- [ ] No new clippy warnings
- [ ] Code review approved
- [ ] Documentation updated

---

## 11. Timeline Recommendations

### Phase Priority and Dependencies

```
Phase 1: Checker Cleanup ────────────┐
  (4-6 weeks)                        │
                                     ├──► Can be done
Phase 2: LSP Performance ────────────┤    in parallel
  (6-8 weeks)                        │    with Phase 3
                                     │
Phase 3: Emitter Unification ────────┘
  (4-6 weeks)

Phase 4: Binder Cleanup
  (3-4 weeks)
  └── Depends on Phase 1 completion
```

### Recommended Schedule

| Week | Phase 1 | Phase 2 | Phase 3 |
|------|---------|---------|---------|
| 1-2 | Extract type logic | Design shared interner | Audit emit paths |
| 3-4 | Replace TypeKey matches | Implement shared interner | Convert expressions |
| 5-6 | Split CheckerState | Build symbol index | Convert statements |
| 7-8 | Testing & stabilization | Implement dependency graph | Convert declarations |
| 9-10 | - | Testing & stabilization | Testing & stabilization |

### Resource Allocation

| Phase | Complexity | Recommended Team Size |
|-------|------------|----------------------|
| Phase 1 | High | 2-3 engineers |
| Phase 2 | High | 2 engineers |
| Phase 3 | Medium | 1-2 engineers |
| Phase 4 | Medium | 1 engineer |

---

## 12. Appendix A: Grep Commands Reference

### Finding Issues

```bash
# TypeKey violations in checker
rg "TypeKey::" src/checker/ --type rust -c | sort -t: -k2 -nr

# Direct type interner access in LSP
rg "\.type_interner" src/lsp/ --type rust

# File iteration patterns
rg "for.*file.*in.*files" src/lsp/ --type rust

# Scope chain usage
rg "scope_chain" src/binder/ --type rust

# Direct emit patterns
rg "self\.output\.push" src/emitter/ --type rust
```

### Monitoring Progress

```bash
# Lines per checker file
wc -l src/checker/*.rs | sort -n

# TypeKey match count over time
rg "TypeKey::" src/checker/ --type rust | wc -l

# Visitor usage
rg "impl.*TypeVisitor" src/ --type rust | wc -l
```

---

## 13. Appendix B: File Reference

### Critical Files for Phase 1

| File | Lines | Role |
|------|-------|------|
| `/Users/mohsenazimi/code/tsz/src/checker/state.rs` | 11,365 | Main checker state - needs splitting (reduced from 12,947) |
| `/Users/mohsenazimi/code/tsz/src/checker/type_checking.rs` | 9,611 | Type checking logic - needs splitting (reduced from 11,606) |
| `/Users/mohsenazimi/code/tsz/src/checker/type_computation.rs` | 3,587 | Type computation - at threshold |
| `/Users/mohsenazimi/code/tsz/src/checker/control_flow.rs` | 3,878 | Control flow analysis - at threshold |
| `/Users/mohsenazimi/code/tsz/src/solver/visitor.rs` | ~500 | TypeVisitor trait - expand usage |
| `/Users/mohsenazimi/code/tsz/src/solver/operations.rs` | ~2,000 | Type operations - add query methods |

### New Modules Created (Phase 2 - 2.3 Split CheckerState)

| Module | Lines | Extracted From |
|--------|-------|---------------|
| `assignment_checker.rs` | 362 | type_checking.rs |
| `generic_checker.rs` | 183 | state.rs |
| `property_checker.rs` | 163 | type_checking.rs |
| `parameter_checker.rs` | 289 | type_checking.rs |
| `module_checker.rs` | 230 | type_checking.rs |
| `call_checker.rs` | ~200 | state.rs |
| `class_checker.rs` | ~250 | type_checking.rs |

### Critical Files for Phase 2

| File | Lines | Role |
|------|-------|------|
| `/Users/mohsenazimi/code/tsz/src/lsp/project.rs` | ~2,500 | ProjectManager - add shared interner |
| `/Users/mohsenazimi/code/tsz/src/lsp/references.rs` | ~1,200 | Reference finding - use index |
| `/Users/mohsenazimi/code/tsz/src/solver/intern.rs` | 1,799 | TypeInterner - make shareable |

### Critical Files for Phase 3

| File | Lines | Role |
|------|-------|------|
| `/Users/mohsenazimi/code/tsz/src/emitter/mod.rs` | 1,895 | Main emitter - unify paths |
| `/Users/mohsenazimi/code/tsz/src/emitter/module_emission.rs` | 1,393 | Module transforms |

### Critical Files for Phase 4

| File | Lines | Role |
|------|-------|------|
| `/Users/mohsenazimi/code/tsz/src/binder/state.rs` | 5,230 | Binder state - remove scope_chain |
| `/Users/mohsenazimi/code/tsz/src/checker/flow_graph_builder.rs` | 2,239 | CFG builder - consolidate |
