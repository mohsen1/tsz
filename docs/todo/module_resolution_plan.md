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

### Gap 1: Module Resolver ↔ Checker Integration

**Problem:** The `ModuleResolver` resolves file paths but doesn't integrate with the checker's type computation pipeline.

**Current flow:**
```
Import statement → Binder creates symbol → Checker resolves symbol → ???
```

**Missing:**
```
Import statement → Binder creates symbol → ModuleResolver finds file
→ Checker gets module's type environment → Computes imported member types
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

### Gap 2: Dynamic Import Return Types

**Problem:** Dynamic imports return `Promise<any>` instead of `Promise<typeof module>`.

**Current:**
```rust
// src/checker/type_computation.rs:2687-2693
if self.is_dynamic_import(call) {
    self.check_dynamic_import_module_specifier(call);
    // Dynamic imports return Promise<typeof module>
    // For unresolved modules, return any to allow type flow to continue
    return TypeId::ANY;  // <-- Should be Promise<ModuleType>
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

**Root cause:** The `declared_modules` set doesn't include pattern matching logic.

### Gap 4: Missing customConditions Support

**Problem:** The `customConditions` tsconfig option is not implemented.

```rust
// src/module_resolver.rs:380
custom_conditions: Vec::new(), // TODO: Add customConditions to ResolvedCompilerOptions
```

### Gap 5: Cross-File Module Type Computation

**Problem:** When file A imports from file B, getting B's exported types requires:
1. B to be parsed and bound
2. B's type environment to be accessible from A's checker
3. Type instantiation to work correctly across module boundaries

Currently, `module_exports` only tracks symbol IDs, not computed types.

---

## Phase 1: Foundation (High Impact)

### 1.1 Integrate ModuleResolver with Checker Context

**Goal:** Make resolved modules available to type computation.

**Tasks:**
- [ ] Add `resolved_module_types: HashMap<String, TypeId>` to `CheckerContext`
- [ ] After binding, populate module type environments in the checker
- [ ] Update `get_type_of_symbol` to use resolved module types for imports

**Files to modify:**
- `src/checker/context.rs` - Add resolved_module_types
- `src/checker/state.rs` - Populate during initialization
- `src/cli/driver.rs` - Pass resolution results to checker

### 1.2 Fix Dynamic Import Return Types

**Goal:** Return `Promise<typeof module>` instead of `Promise<any>`.

**Tasks:**
- [ ] Create `get_module_type(specifier: &str)` helper in checker
- [ ] Wrap result in `Promise<T>` using `create_promise_type` from solver
- [ ] Handle unresolved modules gracefully (fall back to `any`)

**Example implementation:**
```rust
fn get_dynamic_import_type(&mut self, specifier: &str) -> TypeId {
    let module_type = match self.get_module_type(specifier) {
        Some(ty) => ty,
        None => TypeId::ANY,  // Unresolved module
    };
    self.solver.create_promise_type(module_type)
}
```

### 1.3 Wildcard Ambient Module Pattern Matching

**Goal:** Support `declare module "foo*"` and `declare module "*bar"` patterns.

**Tasks:**
- [ ] Implement glob-style pattern matching for `declared_modules`
- [ ] Add `ambient_module_patterns: Vec<(String, SymbolId)>` to binder
- [ ] Update `is_declared_module` to check patterns after exact match fails
- [ ] Add TS5061 error for multiple wildcards (`foo*bar*baz`)

**Pattern matching algorithm:**
```rust
fn matches_ambient_pattern(pattern: &str, specifier: &str) -> bool {
    if !pattern.contains('*') {
        return pattern == specifier;
    }
    // Single * acts like glob
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() != 2 {
        return false;  // Multiple * not supported (TS5061)
    }
    specifier.starts_with(parts[0]) && specifier.ends_with(parts[1])
}
```

---

## Phase 2: Enhanced Resolution (Medium Impact)

### 2.1 Implement customConditions

**Goal:** Support `tsconfig.json` `customConditions` option.

**Tasks:**
- [ ] Add `custom_conditions: Vec<String>` to `ResolvedCompilerOptions`
- [ ] Parse from tsconfig.json
- [ ] Pass to `ModuleResolver::get_export_conditions`
- [ ] Prepend to default conditions list

### 2.2 Module Namespace Types

**Goal:** Properly type `import * as ns from 'module'` namespace objects.

**Tasks:**
- [ ] Create `ModuleNamespace` type in solver for namespace imports
- [ ] Populate namespace type with all module exports
- [ ] Support `typeof ns` correctly

**Current gap:**
```typescript
import * as utils from './utils';
utils.helper();  // Currently may not have proper typing
```

### 2.3 Export Validation Improvements

**Goal:** Properly validate that exported members exist.

**Tasks:**
- [ ] Validate named re-exports: `export { foo } from './bar'`
- [ ] Validate wildcard re-exports don't create conflicts
- [ ] Emit TS2305 for missing exported members

---

## Phase 3: Advanced Features (Lower Impact)

### 3.1 Import Type Inference

**Goal:** Infer types for `const x = await import('./mod')` destructuring.

**Tasks:**
- [ ] Track dynamic import results in flow analysis
- [ ] Apply contextual typing to import destructuring
- [ ] Support `import('./mod').then(m => m.foo)`

### 3.2 Module Kind Detection

**Goal:** Correctly detect ESM vs CommonJS modules.

**Tasks:**
- [ ] Check package.json `type` field
- [ ] Check file extension (.mjs, .cjs, .mts, .cts)
- [ ] Apply correct import/export semantics per module kind

### 3.3 Type-Only Import Elision

**Goal:** Correctly elide type-only imports in emission.

**Tasks:**
- [ ] Track which imports are type-only vs value
- [ ] Elide `import type` statements
- [ ] Elide `import { type X }` bindings

---

## Implementation Priority

### Week 1: Critical Fixes
1. **1.2 Dynamic Import Return Types** - High conformance impact (TS2711)
2. **1.3 Wildcard Ambient Patterns** - Unblocks conformance tests

### Week 2: Integration
3. **1.1 ModuleResolver ↔ Checker Integration** - Foundation for all other fixes
4. **2.3 Export Validation** - Reduces TS2305 errors

### Week 3: Enhancement
5. **2.1 customConditions** - Config completeness
6. **2.2 Module Namespace Types** - Type accuracy

### Week 4: Polish
7. **3.1-3.3** - Advanced features as time permits

---

## Testing Strategy

### Unit Tests
- Module pattern matching edge cases
- Dynamic import type computation
- Cross-module type resolution

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
| `src/checker/state.rs` | `get_type_of_symbol` for imports (line ~4600) |
| `src/checker/type_computation.rs` | `check_call_expression` for dynamic imports |
| `src/binder/state.rs` | Import symbol creation |
| `src/cli/driver.rs` | Multi-file resolution coordination |

---

## Open Questions

1. **Module caching strategy**: Should we cache parsed/bound modules globally or per-worker?
2. **Incremental resolution**: How to invalidate module resolution when dependencies change?
3. **Circular module handling**: Current approach may need refinement for complex cycles.
