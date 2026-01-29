# Module Resolution Improvement Plan (Jan 2026)

## Overview

This document outlines the gaps in module resolution and import handling for Project Zang, along with a phased approach to address them.

### Current State (from conformance analysis)

| Error Code | Count | Type | Description |
|------------|-------|------|-------------|
| TS2307 | 182 extra + 12 missing | Both | Cannot find module |
| TS2711 | 230 missing | Missing | Dynamic import/export issues |
| TS2305 | ~200 | Missing | Module has no exported member |

---

## Architecture Analysis

### Current Implementation

**Files involved:**
- `src/module_resolver.rs` - Path resolution (Node/Node16/NodeNext/Bundler)
- `src/module_graph.rs` - Dependency tracking, circular detection
- `src/imports.rs` - Import tracking (ES6, CommonJS, dynamic, type-only)
- `src/exports.rs` - Export tracking
- `src/checker/module_checker.rs` - Dynamic import validation (TS2307)
- `src/checker/module_validation.rs` - Import member validation
- `src/binder/state.rs` - Symbol creation for imports

### Solver-First Architecture Requirements

Per `AGENTS.md`, module resolution must follow the **solver-first** principle:

| Component | Responsibility |
|-----------|---------------|
| **Binder** | Creates symbol table for modules, tracks exports/imports |
| **Solver** | Converts symbols to types, constructs `ModuleNamespace` type |
| **Checker** | Asks "What is the type of this import?" - delegates to solver |
| **Driver** | Coordinates file resolution, passes graphs to checker |

**Critical Rules:**
- No AST nodes passed to Solver
- No file I/O in Solver
- Checker must NOT compute export types (Binder's job)
- Use **Visitor Pattern** for all type operations

### Gap 1: Module Resolver ↔ Checker Integration

**Problem:** The `ModuleResolver` resolves file paths but doesn't integrate with the checker's type computation pipeline.

**Current flow:**
```
Import statement → Binder creates symbol → Checker resolves symbol → ???
```

**Missing:**
```
Import statement → Binder creates symbol → ModuleResolver finds file
→ Checker gets target file's binder → Resolves exported symbol → Computes type ON DEMAND
```

**Evidence:**
```rust
// src/checker/state.rs:4604-4650
// For ES6 imports with import_module set, resolve using module_exports
if let Some(ref module_name) = symbol.import_module {
    // Check if this is a shorthand ambient module (declare module "foo" without body)
    // Imports from shorthand ambient modules are typed as `any`
    if self.ctx.binder.shorthand_ambient_modules.contains(module_name) {
        return (TypeId::ANY, Vec::new());
    }
    // ...looks up module_exports but doesn't use ModuleResolver
}
```

**Critical Issue:** `CheckerContext` has `all_arenas` (ASTs) but **lacks `all_binders`** (symbol tables). Cross-file type resolution is impossible without access to target file's symbols.

### Gap 2: Dynamic Import Return Types

**Problem:** Dynamic imports return `Promise<any>` instead of `Promise<typeof module>`.

**Current:**
```rust
// src/checker/type_computation.rs:2687-2693
if self.is_dynamic_import(call) {
    self.check_dynamic_import_module_specifier(call);
    // Dynamic imports return Promise<typeof module>
    // For unresolved modules, return any to allow type flow to continue
    return TypeId::ANY;  // <-- Should be Promise<ModuleNamespace>
}
```

**Expected (tsc behavior):**
```typescript
const mod = await import('./foo');  // mod: typeof import('./foo')
mod.someExport;  // Should have proper typing
```

### Gap 3: Wildcard Ambient Module Pattern Matching

**Problem:** `declare module "foo*bar"` patterns don't work in merged binder.

**Evidence from scratchpad:**
> `conformance/ambient/ambientDeclarationsPatterns.ts` still emits TS2307
> Indicates declared module patterns may **not** be populated in merged binder

**Root cause:** The `declared_modules` uses a `HashSet<String>` which only supports exact string matching, not glob patterns.

### Gap 4: Missing customConditions Support

**Problem:** The `customConditions` tsconfig option is not implemented.

```rust
// src/module_resolver.rs:380
custom_conditions: Vec::new(), // TODO: Add customConditions to ResolvedCompilerOptions
```

### Gap 5: Missing Edge Cases

Additional gaps identified via code analysis:

1. **Symlink Preservation** - `preserveSymlinks` compiler option not implemented
2. **`NODE_PATH` Fallback** - Legacy environment variable support missing
3. **Side-Effect Imports** - `import "mod"` handling for `isolatedModules`
4. **Node16/NodeNext Directory Restriction** - Currently too permissive with directory resolution

---

## Phase 1: Foundation (High Impact)

### 1.1 Integrate ModuleResolver with Checker Context ✅ COMPLETED (2026-01-29)

**Goal:** Make resolved modules available to type computation with **lazy evaluation**.

**Architecture:**
```
Driver resolves paths → CheckerContext gets FileId map + all binders
→ Checker requests type → Looks up target binder → Computes type ON DEMAND
```

**Status:** Implemented on 2026-01-29

**Implementation Summary:**
1. Added to `CheckerContext` (in `context.rs`):
   - `all_binders: Option<Vec<Arc<BinderState>>>` - stores all file binders
   - `resolved_module_paths: Option<FxHashMap<(usize, String), usize>>` - maps (source_idx, specifier) → target_idx
   - `current_file_idx: usize` - tracks current file being checked
   - Helper methods: `set_all_binders`, `set_resolved_module_paths`, `get_binder_for_file`, `resolve_import_target`

2. Updated `driver.rs` `collect_diagnostics()`:
   - Pre-creates all binders as `Vec<Arc<BinderState>>` before the file loop
   - Builds `resolved_module_paths` map during resolution
   - Passes both to CheckerContext for each file

3. Added to `state.rs`:
   - `resolve_cross_file_export()` - looks up export in target file's binder
   - `resolve_cross_file_namespace_exports()` - gets all exports for namespace imports
   - Updated namespace and named import handling to use cross-file resolution

**Tasks:**
- [x] Add `all_binders: Vec<Arc<BinderState>>` to `CheckerContext`
- [x] Add `resolved_module_paths: HashMap<(FileId, String), FileId>` to map (source, specifier) → target
- [x] Update `get_type_of_symbol` to use cross-file resolution for imports
- [x] Lazy evaluation via SymbolId lookup (not pre-computed TypeId)

**Files to modify:**
- `src/checker/context.rs` - Add `all_binders` and `resolved_module_paths`
- `src/checker/state.rs` - Implement cross-file symbol lookup in `get_type_of_symbol`
- `src/cli/driver.rs` - Populate `all_binders` and resolution map
- `src/wasm_api/program.rs` - **Critical:** Must also populate `all_binders` for WASM builds
- `src/wasm_api/type_checker.rs` - Ensure WASM API has access to global binder map

**Example implementation:**
```rust
// In src/checker/state.rs - get_type_of_symbol
if let Some(ref module_specifier) = symbol.import_module {
    // 1. Resolve to FileId (not TypeId!)
    let key = (self.ctx.current_file_id, module_specifier.clone());
    let target_file_id = self.ctx.resolved_module_paths.get(&key)?;
    
    // 2. Get target binder
    let target_binder = &self.ctx.all_binders[target_file_id.0];
    
    // 3. Find exported symbol
    let export_name = symbol.import_name.as_deref().unwrap_or(&symbol.escaped_name);
    let target_sym_id = target_binder.resolve_export(export_name)?;
    
    // 4. Compute type lazily with file context switch
    return self.with_file_context(*target_file_id, |checker| {
        checker.get_type_of_symbol(target_sym_id)
    });
}
```

### 1.2 Wildcard Ambient Module Pattern Matching ✅ COMPLETED (PR #185)

**Goal:** Support `declare module "foo*"` and `declare module "*bar"` patterns with **TSC-compatible specificity rules**.

**Status:** Implemented on 2026-01-28

**Implementation Summary:**
1. Fixed binder to not treat shorthand ambient modules (no body) as augmentations
   - Shorthand modules like `declare module "*.json";` are now properly stored in `shorthand_ambient_modules`
   - Only modules WITH bodies can be augmentations per Rule #44
2. Updated all checker modules to use `is_ambient_module_match()` for pattern matching:
   - `module_checker.rs`: `check_dynamic_import_module_specifier`, `check_export_module_specifier`
   - `module_validation.rs`: `validate_import_specifier`
   - `type_checking.rs`: `check_import_equals_declaration`
   - `state.rs`: `get_type_of_symbol` for shorthand ambient module type resolution
3. Added `is_shorthand_ambient_module_match()` in `symbol_resolver.rs` for checking shorthand patterns
4. Uses `globset` crate for robust wildcard matching

**Tasks:**
- [x] Fix binder shorthand ambient module detection (was incorrectly treated as augmentation)
- [x] Update all checker modules to use pattern matching instead of `.contains()`
- [x] Add `is_shorthand_ambient_module_match()` for shorthand-specific checks
- [x] Add tests: `test_shorthand_ambient_module_prevents_ts2307`, `test_wildcard_ambient_module_pattern_matching`, `test_declared_module_with_body_in_script`
- [ ] Add TS5061 error for multiple wildcards (`foo*bar*baz`) - deferred to future work
- [ ] Implement TSC specificity rules (longest prefix wins) - current impl uses globset which is sufficient for most cases

**Note:** The current implementation uses `globset` for pattern matching which handles most TypeScript patterns correctly. TSC's exact specificity algorithm (longest prefix wins, then longest suffix) is not yet implemented but can be added if conformance tests reveal issues.

### 1.3 Fix Dynamic Import Return Types ✅ COMPLETED (2026-01-29)

**Goal:** Return `Promise<ModuleNamespace>` instead of `Promise<any>`.

**Status:** Implemented on 2026-01-29

**Implementation Summary:**
1. Added `get_dynamic_import_type()` in `module_checker.rs`:
   - Extracts module specifier from dynamic import call
   - Gets module exports via local binder or cross-file resolution
   - Creates an object type with all module exports
   - Wraps in `Promise<T>` using `PROMISE_BASE` and `interner.application()`
   - Falls back to `Promise<any>` for unresolved modules

2. Updated `type_computation.rs`:
   - Changed `is_dynamic_import` branch to call `get_dynamic_import_type()` instead of returning `TypeId::ANY`

3. Helper methods added:
   - `create_promise_any()` - creates `Promise<any>` type
   - `create_promise_of(inner_type)` - creates `Promise<T>` using `PROMISE_BASE`

**Tasks:**
- [x] Create `get_dynamic_import_type(call)` in checker
- [x] Create object type from module exports
- [x] Wrap result in `Promise<T>` using `TypeKey::Application`
- [x] Handle unresolved modules gracefully (fall back to `any`)
- [x] Use cross-file resolution via `resolve_cross_file_namespace_exports`

**Note:** Uses `PROMISE_BASE` synthetic type instead of resolving global Promise symbol. This allows Promise<T> to work without lib files loaded.

---

## Phase 2: Enhanced Resolution (Medium Impact)

### 2.1 Implement customConditions ✅ COMPLETED (2026-01-29)

**Goal:** Support `tsconfig.json` `customConditions` option.

**Status:** Implemented on 2026-01-29

**Implementation Summary:**
1. Added `custom_conditions: Option<Vec<String>>` to `CompilerOptions` in `config.rs`
2. Added `custom_conditions: Vec<String>` to `ResolvedCompilerOptions`
3. Added parsing in `resolve_compiler_options()` to copy from tsconfig
4. Updated `merge_compiler_options()` for tsconfig extends support
5. Updated `ModuleResolver::new()` to use `options.custom_conditions`
6. Updated `get_export_conditions()` to prepend custom conditions to defaults

**Tasks:**
- [x] Add `custom_conditions: Vec<String>` to `ResolvedCompilerOptions`
- [x] Parse from tsconfig.json
- [x] Pass to `ModuleResolver::get_export_conditions`
- [x] Prepend to default conditions list

### 2.2 Module Namespace Types (Solver Addition) ✅ COMPLETED (2026-01-29)

**Goal:** Add proper `TypeKey::ModuleNamespace` to represent `import * as ns`.

**Status:** Implemented on 2026-01-29

**Implementation Summary:**
1. Added `TypeKey::ModuleNamespace(SymbolRef)` to `src/solver/types.rs`
2. Added `visit_module_namespace` method to `TypeVisitor` trait in `src/solver/visitor.rs`
3. Updated `visit_type_key` dispatch to handle `ModuleNamespace`
4. Added `is_module_namespace_type` and `is_module_namespace_type_db` predicates
5. Updated format.rs to display as `typeof import("module_name")`
6. Updated all match statements across the solver:
   - `instantiate.rs` - No substitution needed (like Ref)
   - `infer.rs` - No infer patterns in module namespace
   - `lower.rs` - Treated as terminal (no type params)
   - `operations.rs` - No type param containment
   - `type_queries.rs` - All classification functions updated
   - `evaluate_rules/infer_pattern.rs` - No infer bindings
   - `subtype_rules/functions.rs` - Not a function type

**Tasks:**
- [x] Add `TypeKey::ModuleNamespace(SymbolRef)` to `src/solver/types.rs`
- [x] Add `visit_module_namespace` to `src/solver/visitor.rs`
- [x] Add `is_module_namespace_type` predicate to visitor
- [x] Handle in all type classification and traversal functions

**Note:** Property resolution uses cross-file resolution from Phase 1.1. Uses `SymbolRef` for lazy evaluation to avoid circular import issues.

### 2.3 Export Validation Improvements ✅ COMPLETED (2026-01-29)

**Goal:** Properly validate that exported members exist.

**Status:** Implemented on 2026-01-29

**Implementation Summary:**
1. Added `validate_reexported_members()` to `module_checker.rs`
2. Validates named re-exports exist in target module
3. Emits TS2305 for missing exported members in re-exports
4. Handles renamed exports (`export { bar as baz }`)
5. Skips type-only re-exports (which might reference types not in exports table)

**Tasks:**
- [x] Validate named re-exports: `export { foo } from './bar'`
- [x] Emit TS2305 for missing exported members
- [ ] Validate wildcard re-exports don't create conflicts (future enhancement)

---

## Phase 3: Advanced Features (Lower Impact)

### 3.1 Import Type Inference (Remaining)

**Goal:** Infer types for `const x = await import('./mod')` destructuring.

**Note:** Phase 1.3 already returns `Promise<ModuleNamespace>` for dynamic imports.
This phase is about inferring types through destructuring and promise chains.

**Tasks:**
- [ ] Track dynamic import results in flow analysis
- [ ] Apply contextual typing to import destructuring
- [ ] Support `import('./mod').then(m => m.foo)`

### 3.2 Module Kind Detection ✅ ALREADY IMPLEMENTED

**Goal:** Correctly detect ESM vs CommonJS modules.

**Status:** Already implemented in `cli/driver.rs`

**Implementation:**
- `PackageType` enum (Module vs CommonJs) in driver.rs
- `package_type_for_dir()` walks up directories looking for package.json
- Parses "type" field from package.json
- `forces_esm()` and `forces_cjs()` on `ModuleExtension` for .mts/.mjs/.cts/.cjs
- Used for resolution in Node16/NodeNext modes

**Tasks:**
- [x] Check package.json `type` field
- [x] Check file extension (.mjs, .cjs, .mts, .cts)
- [x] Apply correct import/export semantics per module kind

### 3.3 Type-Only Import Elision ✅ ALREADY IMPLEMENTED

**Goal:** Correctly elide type-only imports in emission.

**Status:** Already implemented in `emitter/module_emission.rs`

**Implementation:**
- Checks `is_type_only` on import/export clauses
- Checks `is_type_only` on individual specifiers
- `export_clause_is_type_only()` detects type-only exports
- Skips emitting type-only imports/exports entirely

**Tasks:**
- [x] Track which imports are type-only vs value
- [x] Elide `import type` statements
- [x] Elide `import { type X }` bindings

### 3.4 Additional Edge Cases (Remaining)

**Tasks:**
- [ ] Wire up `preserveSymlinks` CLI option to skip `canonicalize()` calls
- [ ] Support `NODE_PATH` environment variable fallback
- [ ] Handle side-effect imports for `isolatedModules`
- [ ] Enforce Node16/NodeNext directory import restrictions

**Note:** `preserveSymlinks` and `isolatedModules` are already parsed from CLI args but not fully wired up to module resolution.

---

## Implementation Priority

### Week 1: Critical Fixes ✅ COMPLETED
1. ✅ **1.2 Wildcard Ambient Patterns** - COMPLETED (PR #185, 2026-01-28)
2. ✅ **1.1 ModuleResolver ↔ Checker Integration** - COMPLETED (2026-01-29)

### Week 2: Type Accuracy ✅ COMPLETED
3. ✅ **1.3 Dynamic Import Return Types** - COMPLETED (2026-01-29)
4. ✅ **2.1 customConditions** - COMPLETED (2026-01-29)

### Week 3: Validation ✅ COMPLETED
5. ✅ **2.2 Module Namespace Types** - COMPLETED (2026-01-29)
6. ✅ **2.3 Export Validation** - COMPLETED (2026-01-29)

### Week 4: Polish ✅ MOSTLY COMPLETE
7. ✅ **3.2 Module Kind Detection** - Already implemented
8. ✅ **3.3 Type-Only Import Elision** - Already implemented
9. **3.1, 3.4** - Remaining advanced features

---

## Testing Strategy

### Unit Tests
- Module pattern matching with specificity edge cases
- Dynamic import type computation
- Cross-module type resolution with circular dependencies
- `TypeKey::ModuleNamespace` visitor behavior

### Conformance Tests to Monitor
```
conformance/ambient/ambientDeclarationsPatterns.ts
conformance/moduleResolution/*
conformance/dynamicImport/*
conformance/exportsAndImports/*
```

### Regression Prevention
- Add specific tests for each TS2307/TS2711/TS2305 fix
- Monitor conformance pass rate for module-related tests

---

## Success Metrics

| Metric | Current | Target |
|--------|---------|--------|
| TS2307 extra | 182 | <50 |
| TS2307 missing | 12 | <5 |
| TS2711 missing | 230 | <50 |
| TS2305 | ~200 | <50 |
| Module-related conformance | ~40% | 70%+ |

---

## Appendix: Key Files Reference

| File | Purpose |
|------|---------|
| `src/module_resolver.rs` | Path resolution algorithms |
| `src/module_graph.rs` | Dependency graph management |
| `src/imports.rs` | Import tracking data structures |
| `src/exports.rs` | Export tracking data structures |
| `src/checker/module_checker.rs` | Dynamic import validation |
| `src/checker/module_validation.rs` | Import member validation |
| `src/checker/context.rs` | CheckerContext (needs `all_binders`, `resolved_module_paths`) |
| `src/checker/state.rs` | `get_type_of_symbol` for imports (line ~4600) |
| `src/checker/type_computation.rs` | `check_call_expression` for dynamic imports |
| `src/binder/state.rs` | Import symbol creation, `pattern_ambient_modules` |
| `src/cli/driver.rs` | Multi-file resolution coordination |
| `src/solver/types.rs` | Add `TypeKey::ModuleNamespace` |
| `src/solver/visitor.rs` | Add `is_module_namespace_type` predicate |

---

## Additional Gaps (from Gemini review)

### Wildcard `export *` Ambiguity Detection ✅ COMPLETED (2026-01-29)

**Issue:** Current `resolve_import_with_reexports_inner` returns the **first** match found. TSC requires checking **all** wildcards to detect collisions.

**TSC Behavior:** If two `export *` declarations export the same name, that name is considered **ambiguous** and is **not exported** (unless explicitly re-exported by name).

**Implementation:** Updated `resolve_reexported_member_symbol_inner` in `symbol_resolver.rs` to:
- Check ALL wildcards for the same export name
- Return `None` (ambiguous) if multiple sources define the same name
- Only return definitive result if exactly one source provides the name

### `export =` vs `export default` with esModuleInterop (Remaining)

**Issue:** `ModuleNamespace` type needs to handle:
- `import x = require('mod')` → resolves to value of `export =`
- `import * as x from 'mod'` → may resolve differently based on `esModuleInterop`
- `import x from 'mod'` with `esModuleInterop` → synthesized default export

**Status:** `esModuleInterop` is parsed from CLI but not wired to checker. Requires:
1. Add `es_module_interop` to `ResolvedCompilerOptions`
2. Wire through `CheckerContext`
3. Use when resolving default imports from CommonJS modules

### Module Augmentation Merging ✅ COMPLETED (2026-01-29)

**Issue:** `binder/state.rs` tracks `module_augmentations`. The `ModuleNamespace` type must merge these augmentations into the resolved type.

**Example:**
```typescript
// In node_modules/express/index.d.ts
declare namespace Express { interface Request {} }

// In user code
declare module 'express' {
    interface Request { user?: User; }  // Augmentation
}
```

**Implementation:** Updated `get_dynamic_import_type` in `module_checker.rs` to:
- Check `module_augmentations` for the target module
- Merge augmented declarations into module type
- Create intersection types when augmenting existing exports
- Add new properties for new augmentation declarations

---

## Open Questions (Status)

1. **Module caching strategy**: Should we cache parsed/bound modules globally or per-worker?
   - ✅ **RESOLVED**: Implemented with `Arc<BinderState>` in Phase 1.1. `all_binders` in `CheckerContext` provides shared access across files.

2. **Incremental resolution**: How to invalidate module resolution when dependencies change?
   - ⏳ **FUTURE**: Requires integration with watch mode. Currently re-resolves on each compilation.

3. **Circular module handling**: Use `TypeKey::Ref(SymbolRef)` for lazy evaluation
   - ✅ **RESOLVED**: Implemented `TypeKey::ModuleNamespace(SymbolRef)` in Phase 2.2 for lazy evaluation.

4. **Performance**: Lazy type computation is critical - never pre-compute entire module type environments
   - ✅ **FOLLOWED**: `ModuleNamespace` uses `SymbolRef` for lazy property resolution. Types computed on demand.

5. **esModuleInterop complexity**: How to handle synthetic default exports correctly?
   - ⏳ **REMAINING**: Requires wiring `es_module_interop` from CLI args through to checker. See Gap 2 above.
