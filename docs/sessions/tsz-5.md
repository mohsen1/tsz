# Session tsz-5 - Import/Export Elision for Declaration Emit

## Date: 2026-02-04

## Status: PAUSED - Waiting for TSZ-6 (Advanced Type Nodes)

### Pause Reason (2026-02-04)

Per Gemini strategic consultation, TSZ-5 is **PAUSED** until TSZ-6 completes.

**Strategic Rationale:**
- Import/Export Elision has a hard dependency on accurate type parsing
- Current emit_type() lacks support for MappedType, ConditionalType, TypeQuery, IndexedAccessType
- This causes 75+ TS1005 syntax errors that would corrupt usage analysis
- **Solver-First Principle:** Type nodes must be implemented before usage analysis can be accurate

**Gemini Quote:**
> "If a .d.ts contains `export const x: typeof InternalVar;` and TypeQuery isn't implemented, the usage analyzer will fail to see that InternalVar is 'used', leading to incorrect import elision."

**Next Steps:**
1. Complete TSZ-6 (Advanced Type Nodes)
2. Verify TS1005 errors reduced significantly
3. Resume TSZ-5 with accurate type foundation

**See:** `docs/sessions/tsz-6.md` for active session

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
