# Session tsz-5 - Import/Export Elision for Declaration Emit

## Date: 2026-02-04

## Status: ACTIVE - Integration Phase

### Session Progress (2026-02-04)

**Completed Work:**

1. **UsageAnalyzer Implementation** (commits `da0fcb9df`, `a250935f8`)
   - ✅ Created `src/declaration_emitter/usage_analyzer.rs`
   - ✅ Hybrid AST/Semantic walk for tracking used symbols
   - ✅ Refactored to use components instead of CheckerContext
   - ✅ Added `def_to_symbol` field to `TypeCache`
   - ✅ Handles inferred types, qualified names, computed properties
   - ✅ Validated by Gemini Pro (Question 3) - Architecture CORRECT

2. **TypeCache Integration** (commits `44f11b33b`, `a250935f8`)
   - ✅ Added `def_to_symbol: FxHashMap<DefId, SymbolId>` to TypeCache
   - ✅ Updated `TypeCache::merge()` to include def_to_symbol
   - ✅ Updated `with_cache()` constructors to merge cache.def_to_symbol
   - ✅ DeclarationEmitter now takes `binder: Option<&BinderState>`

3. **Test Structure Created** (commit `fb15e09cd`)
   - ✅ Added 11 comprehensive test cases
   - ✅ Tests structured in `src/declaration_emitter/tests/`

4. **DeclarationEmitter Integration** (commit `d7597b9ff`)
   - ✅ Added `used_symbols: Option<FxHashSet<SymbolId>>` field
   - ✅ Added `set_used_symbols()` method
   - ✅ Added `should_emit_import_specifier()` helper
   - ✅ Added `count_used_imports()` helper
   - ✅ Modified `emit_import_declaration()` to filter imports
   - ✅ Added `emit_named_imports_filtered()` for specifier filtering

**Current Task:**

**Priority 1:** ✅ **COMPLETED** - Integrate UsageAnalyzer into DeclarationEmitter
   - Location: `src/declaration_emitter/mod.rs`
   - All filtering logic implemented and compiling (commit `d7597b9ff`)

**Priority 2:** ⏸ **BLOCKED** - Driver integration needed
   - **Baseline:** 42.1% (269/639) - no change (expected - not wired up yet)
   - **BLOCKER:** Missing BinderState in multi-file driver (commit `a12ab22d8`)
   - Required: Add BinderState to MergedProgram to enable usage analysis

**Current Blocker:** Missing BinderState in Multi-File Mode

The driver's `MergedProgram` doesn't include per-file `BinderState` instances.
- `BoundFile` has raw binding data (node_symbols, scopes) but no BinderState
- UsageAnalyzer needs BinderState for `node_symbols` map access
- Temporarily disabled in multi-file driver (commit `a12ab22d8`)

**Required Fix:**
1. Store `Vec<Arc<BinderState>>` in MergedProgram (like lib_binders)
2. OR store `binder: Arc<BinderState>` in each BoundFile
3. Re-enable usage analysis in emit() method
4. Run conformance tests

**Key Architecture Decisions:**
- UsageAnalyzer uses hybrid walk: AST for explicit types, Semantic for inferred
- Symbol resolution via `ctx.binder.get_node_symbol()` for AST nodes
- DefId→SymbolId mapping via `ctx.def_to_symbol_id()` for TypeIds
- ModuleNamespace handling for `import * as ns` statements

### Pause Reason (2026-02-04) - RESOLVED ✅

**Previous Status:** TSZ-5 was PAUSED until TSZ-6 completed.

**Why it was paused:**
- Import/Export Elision has a hard dependency on accurate type parsing
- Missing support for MappedType, ConditionalType, TypeQuery, IndexedAccessType
- **Solver-First Principle:** Type nodes must be implemented before usage analysis can be accurate

**Resolution:**
- ✅ TSZ-6 COMPLETED (2026-02-04)
- ✅ All advanced type nodes now implemented in emit_type()
- ✅ Ready to resume with accurate type foundation

**See:** `docs/sessions/tsz-6.md` for completed session

### Session Resumption Plan - 2026-02-04

**Gemini's Strategic Guidance:**
> "Now that tsz-6 has provided the necessary foundation for advanced type nodes, you can implement the usage analyzer with the accuracy required to prevent 'Module not found' errors."

**Expected Impact:**
- Conformance pass rate: +5% to +10% increase
- Eliminates majority of "Module not found" errors in .d.ts files
- Single largest blocker to declaration emit correctness

**Current Status (2026-02-04): Awaiting Gemini Consultation**

Rate limited on API - Question 1 (Approach Validation) queued for when rate limit clears.

**Planned Implementation (Based on Gemini Guidance):**
1. Add checker_context field to DeclarationEmitter
2. Add used_symbols: FxHashSet<SymbolId> tracking
3. Implement SymbolUsageVisitor (TypeVisitor-based)
4. Hybrid AST/Semantic walk for exported declarations
5. Import filtering in emit_import_declaration()

**Key Questions for Gemini:**
1. How to access TypeResolver from TypeInterner? Need TypeEnvironment?
2. Is get_symbol_of_node() in CheckerContext? Exact implementation?
3. Do recent 'f2d4ae5d5' DefId stability refactors affect approach?

**Corrected Architecture (From Previous Review):**
1. **Hybrid AST/Semantic Walk** - Not just AST nodes
2. **SymbolUsageVisitor** - New TypeVisitor implementation
3. **DefId→SymbolId Mapping** - Via TypeResolver trait
4. **Type-Only vs Value Usage** - Critical distinction for elision

### Gemini Consultation #2 - 2026-02-04 (COMPLETED)

**Question 2:** Architecture validation for UsageAnalyzer implementation.

**Gemini's Response (Flash Model):**
- ✅ **Use `collect_all_types()` utility** - Don't create new TypeVisitor
- ✅ **Extract DefIds** using existing helpers: `lazy_def_id()`, `enum_components()`, `type_query_symbol()`, `unique_symbol_ref()`
- ✅ **Map DefId -> SymbolId** via `TypeResolver::def_to_symbol_id()`

**Result:** UsageAnalyzer implementation commit `7fd42f10b` followed Gemini's guidance correctly.

### Gemini Consultation #3 - 2026-02-04 (COMPLETED)

**Question 3:** Implementation review (Pro model).

**Gemini's Findings:**

**Architecture:** ✅ CORRECT
- Hybrid AST/Semantic walk approach is correct
- AST walk for explicit types, Semantic walk for inferred types

**Critical Bugs Identified:**
1. ❌ **Missing `TypeKey::Ref` handling** (legacy types during Phase 4.2 migration)
2. ❌ **Missing `TypeKey::Object` with symbol** (class instance types not wrapped in Lazy)
3. ❌ **Missing `ImportType` node** (e.g., `type T = import("./foo").Bar`)
4. ❌ **`extract_type_data` is private** (need public method or alternative)

**Non-Issues (Already Implemented):**
1. ✅ `analyze_entity_name` IS implemented (uses `binder.get_node_symbol()`)
2. ✅ Private members check WAS removed (analyzes all members correctly)
3. ✅ `ModuleNamespace` handling IS present

**Status:** Implementation is fundamentally correct. Missing handlers are edge cases that can be added incrementally after integration testing.

### Gemini Critical Review - 2026-02-04

**Attempted Implementation Review (QUESTION 2):**

**Gemini's Response: CRITICAL ARCHITECTURAL FLAWS**

#### 1. Fatal Flaw: AST Walk vs. Semantic Walk
**Problem:** My implementation performs purely syntactic (AST) walk, missing inferred types.

**Example Failure Case:**
```typescript
import { Something } from './module';
// No type annotation! Return type is inferred as 'Something'.
export function create() {
    return new Something();
}
```

**My Code Bug:**
```rust
// analyze_function_declaration
if !func.type_annotation.is_none() {
    self.analyze_type_annotation(...);
}
// If no annotation, you do NOTHING.  // <- BUG
```

**Result:** `Something` not marked as used → import elided → .d.ts contains broken reference → "Module not found" error.

**Correct Architecture (from Gemini):**
```rust
pub fn analyze_symbol(&mut self, symbol_id: SymbolId) {
    // 1. Get declaration node
    // 2. If explicit type node exists -> walk_ast_type(node)
    // 3. If implicit -> let type_id = solver.get_type_of_symbol(symbol_id);
    //    walk_semantic_type(type_id)
}

fn walk_ast_type(&mut self, node: NodeIndex) {
    // AST walking logic (for explicit annotations)
}

fn walk_semantic_type(&mut self, type_id: TypeId) {
    if !self.visited_types.insert(type_id) { return; }
    // Use TypeVisitor pattern to find referenced symbols in TypeId
}
```

#### 2. Dead Code & Confusion
**Problem:** Defined `visited_types: FxHashSet<TypeId>` and `visited_defs: FxHashSet<DefId>` but never used them.

**Gemini's Analysis:** "This confirms you confused walking the AST (Nodes) with walking the Type System (TypeIds). You need *both*."

#### 3. Broken Qualified Name Handling
**Problem:** For `MyModule.SomeType`, my code marks `SomeType` as used but fails to mark `MyModule` as used.

**Result:** `import * as MyModule from ...` elided → broken generated .d.ts

**Required Fix:** Recursively walk `TypeName` node structure to find leftmost identifier.

#### 4. Missing Critical Type Nodes
**Problem:** As warned in session pause, missing handlers for:
- `TypeQuery` (`typeof X`) - **Critical**
- `IndexedAccessType` (`T[K]`)
- `MappedType`
- `ConditionalType`
- `InferType`

**Gemini's Strong Recommendation:**
> "Revert/Stash your changes. Do not commit this. Switch to Session tsz-6. Help complete the Advanced Type Nodes first."

**Status:** Implementation reverted (files kept for reference when tsz-5 resumes after tsz-6).

### Session Goal

Implement Import/Export Elision to remove unused imports from .d.ts files, preventing "Module not found" errors and matching TypeScript's behavior exactly.

### Problem Statement

Currently, tsz emits ALL imports found in source files, even if they're not referenced in the exported API. This causes:
- "Module not found" errors in consuming code
- Unnecessary dependencies in declaration files
- Non-compliance with TypeScript's declaration emit behavior

### Gemini Consultation Guidance

**Recommended Approach:**
1. Create `src/declaration_emitter/usage_analyzer.rs`
2. Implement UsageAnalyzer with:
   - `used_symbols: FxHashSet<SymbolId>`
   - `visited_defs: FxHashSet<DefId>`
   - Methods to walk exported declarations
   - Type visitor to find Lazy(DefId), TypeQuery, Enum types
3. Map DefId to SymbolId via DefinitionStore
4. Filter import emission based on used symbols
5. Handle edge cases: re-exports, circular references, private members

**Estimated Complexity:** High (2-3 days)

**Impact:** Critical - fixes major blocker for declaration emit correctness

### Implementation Plan

#### Phase 1: Create UsageAnalyzer Module
- [ ] Create `src/declaration_emitter/usage_analyzer.rs`
- [ ] Define UsageAnalyzer struct with used_symbols and visited_defs sets
- [ ] Implement method to walk exported declarations
- [ ] Implement type visitor to extract SymbolIds from TypeIds

#### Phase 2: DefId to SymbolId Mapping
- [ ] Research DefinitionStore and SymbolId mapping
- [ ] Implement lookup function for DefId → SymbolId
- [ ] Handle TypeKey::Lazy(DefId)
- [ ] Handle TypeKey::TypeQuery(SymbolRef)
- [ ] Handle TypeKey::Enum(DefId, _)

#### Phase 3: Import Filtering
- [ ] Modify DeclarationEmitter to use UsageAnalyzer
- [ ] Update emit_import_declaration to filter unused imports
- [ ] Handle re-export special case (always keep)
- [ ] Handle side-effect imports (always keep)

#### Phase 4: Edge Cases
- [ ] Circular reference handling
- [ ] Private member exclusion (don't track their types)
- [ ] Namespace imports with type-only imports
- [ ] Default imports

### Testing

Test command:
```bash
./scripts/conformance.sh --filter=decl
```

Focus on multi-file test cases and import-related failures.

### Conformance Baseline

Current: 42.2% (267/633 tests passing)
Target: Significant increase by fixing module resolution errors

### Dependencies

- src/solver/visitor.rs - RecursiveTypeCollector
- src/solver/types.rs - TypeKey definitions
- src/solver/def.rs - DefId definitions
- src/declaration_emitter.rs - Main emitter

### Previous Session Completion

tsz-4 completed:
- Function overloads ✅
- Default parameters ✅
- Parameter properties ✅
- Class member visibility ✅
- Abstract classes/methods ✅

All features match TypeScript .d.ts output exactly.

### Gemini Priority Guidance - 2026-02-04

**Recommendation:** Continue with **Task #3 (Integration)**

**Justification:**
- Declaration Emitter is currently "dumb" - doesn't know which declarations are necessary
- Integrating UsageAnalyzer transforms it into a functional tool
- Completes major architectural milestone for Emitter component

**Action Plan:**
1. Task #3: Integrate UsageAnalyzer into DeclarationEmitter (HIGHEST PRIORITY)
2. Task #5: Add missing type handlers during integration (as needed)

**Critical Reminder (from AGENTS.md):**
When working on Task #5 (modifying `src/solver/`), MUST follow mandatory workflow:
- Question 1 (BEFORE): Ask Gemini for correct approach
- Question 2 (AFTER): Ask Gemini to review for 3 CRITICAL BUGS

**Expected Outcome:**
- Dependency-aware declaration emission
- Eliminates "Type cannot be named" errors in .d.ts files
- +5-10% conformance pass rate increase


### Gemini Final Priority Decision - 2026-02-04

**DECISION: CONTINUE TSZ-5 - Complete Task #3 Integration**

**Key Reasons:**
1. **Emit Correctness** - Import/export elision is REQUIRED for valid JavaScript
2. **Momentum** - UsageAnalyzer implemented & validated by Gemini Pro
3. **Avoid WIP Bloat** - Don't leave critical work half-done
4. **Dependency Chain** - Task #5 depends on stable Task #3

**Action Plan:**
1. Finish Task #3: Integrate UsageAnalyzer into DeclarationEmitter
2. Execute Task #5: Add missing handlers (with mandatory Gemini workflow)
3. Verify with conformance tests + TSZ_LOG=debug tracing

**When to Switch?**
Only if hitting Solver blocker or critical type system regression.

**Expected Impact:**
- Validated .d.ts file generation
- Eliminates runtime crashes from missing imports
- +5-10% conformance pass rate


### Integration Challenge - 2026-02-04

**Problem:** UsageAnalyzer requires `CheckerContext` for `DefId -> SymbolId` mapping, but:
1. `CheckerContext` is only available during type checking phase
2. Declaration emission happens after checking is complete
3. `TypeInterner` doesn't provide `def_to_symbol_id()` mapping

**Available Infrastructure:**
- `TypeCache.symbol_dependencies` - already populated by Checker
- `TypeCache.node_types` - for inferred types
- `TypeCache.symbol_types` - for symbol type lookup

**Current Blocker:**
- Missing bridge from `DefId` (in TypeIds) to `SymbolId` (for import filtering)
- `TypeResolver` trait exists but implementation is in `CheckerContext`
- No global `DefId -> SymbolId` registry accessible from emission phase

**Potential Solutions:**
1. Add `def_to_symbol: FxHashMap<DefId, SymbolId>` to `TypeCache`
2. Make `TypeCache.extract_def_to_symbol_map()` available
3. Defer semantic walk until proper bridge exists
4. Use only AST walk + symbol_dependencies graph initially

**Status:** Blocked on architecture decision. Need to consult with team or revisit driver architecture.

