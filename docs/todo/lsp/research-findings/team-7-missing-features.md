# Research Team 7: Missing LSP Features - Research Report

**Date:** January 30, 2026
**Team:** Research Team 7
**Mission:** Investigate unimplemented LSP features and prioritize them for implementation

---

## Executive Summary

This report analyzes missing Language Server Protocol (LSP) features in the tsz TypeScript compiler and provides a prioritized implementation roadmap. Based on comprehensive codebase analysis and Gemini AI assessment, we've identified **6 major unimplemented LSP features** with varying complexity and user value.

**Key Findings:**
- **Workspace Symbols** is the highest priority quick win (1-2 days effort)
- **Go to Implementation** is high-value but complex (1-2 weeks)
- **Call Hierarchy** provides medium value with medium effort (3-5 days)
- **Type Hierarchy** requires deep type system integration (1-2 weeks)
- **Document Links** is low-hanging fruit (1 day)
- **Native Formatting** should be deferred (months of effort)

**Total Estimated Effort for Top 5 Features:** 3-5 weeks

---

## Current LSP Implementation Status

### Implemented Features (20 modules, ~16,500 LOC)

The tsz codebase already has robust LSP support for:

| Category | Features | Files |
|----------|----------|-------|
| **Core Navigation** | Definition, References, Type Definition, Document Highlighting | 4 files |
| **Code Intelligence** | Completions, Signature Help, Hover, Inlay Hints | 4 files |
| **Refactoring** | Rename, Code Actions, Code Lens | 3 files |
| **Document Structure** | Document Symbols, Semantic Tokens, Folding Ranges, Selection Range | 4 files |
| **Workspace** | Diagnostics, Formatting (delegated) | 2 files |

**Key Infrastructure Files:**
- `/Users/mohsenazimi/code/tsz/src/lsp/symbol_index.rs` (547 lines) - Global symbol indexing for O(1) lookups
- `/Users/mohsenazimi/code/tsz/src/lsp/project.rs` (1,298 lines) - Multi-file project container
- `/Users/mohsenazimi/code/tsz/src/lsp/project_operations.rs` (1,826 lines) - Cross-file operations

### LSP Server

A functional LSP server binary exists at `/Users/mohsenazimi/code/tsz/src/bin/tsz_server.rs` with basic infrastructure for handling requests. The server already has a placeholder for `textDocument/implementation` (line 751, 1159).

---

## Missing LSP Features Analysis

### 1. Workspace Symbols (workspace/symbol)

**LSP Method:** `workspace/symbol`

**Status:** ❌ Missing

**Description:** Search for symbols across the entire project (e.g., Ctrl+T / Cmd+T fuzzy search).

**User Value:** ⭐⭐⭐⭐⭐ HIGH
- Essential for navigating large codebases
- Standard feature expected in all modern IDEs
- Dramatically improves developer productivity
- Users search for symbols by name without knowing which file they're in

**Implementation Complexity:** 🟢 LOW (1-2 days)

**Dependencies:** HIGH - heavily leverages existing infrastructure
- ✅ `symbol_index.rs` already provides the core data structure
- ✅ Symbol name → file mappings already indexed
- ✅ Export tracking exists in `SymbolIndex.exports`
- ⚠️ Missing: Fuzzy search matcher and result ranking

**Implementation Requirements:**
```rust
// File to create: /Users/mohsenazimi/code/tsz/src/lsp/workspace_symbols.rs

pub struct WorkspaceSymbolsProvider<'a> {
    symbol_index: &'a SymbolIndex,
}

impl WorkspaceSymbolsProvider<'_> {
    pub fn find_symbols(&self, query: &str) -> Vec<SymbolInformation> {
        // 1. Get all symbols from symbol_index.definitions
        // 2. Filter by query string (case-insensitive substring)
        // 3. Add fuzzy match scoring (optional, can start with substring)
        // 4. Convert to LSP SymbolInformation with:
        //    - name, kind (from symbol type)
        //    - location (file path + range)
        //    - container_name (optional)
        // 5. Return top N results
    }
}
```

**Implementation Steps:**
1. Create `workspace_symbols.rs` module
2. Query `SymbolIndex.definitions` for symbol names matching query
3. For each match, get location from `SymbolIndex.find_definitions()`
4. Map symbol types to `SymbolKind` (Function, Class, Interface, etc.)
5. Add to `src/lsp/mod.rs` exports
6. Add handler in `tsz_server.rs`

**Why This Should Be Priority 1:**
- Infrastructure is 90% complete (`SymbolIndex` already indexes symbols)
- Provides immediate user value
- Low risk, high reward
- Builds on existing patterns (similar to `document_symbols.rs`)

**Estimated Effort:** 1-2 days

---

### 2. Go to Implementation (textDocument/implementation)

**LSP Method:** `textDocument/implementation`

**Status:** ⚠️ Stubbed (placeholder in code_lens.rs:274-294)

**Description:** Navigate from an interface definition to the classes that implement it, or from an abstract method to concrete implementations.

**User Value:** ⭐⭐⭐⭐⭐ HIGH
- TypeScript relies heavily on interfaces
- "Go to Definition" on an interface method is often useless
- Users need to see concrete implementations
- Critical for understanding polymorphic code

**Implementation Complexity:** 🔴 HIGH (1-2 weeks)

**Dependencies:** HIGH - requires type system integration
- ✅ `SymbolIndex` can find candidate classes/structs
- ✅ `code_lens.rs` already has placeholder command
- ⚠️ `checker/class_inheritance.rs` has inheritance logic
- ❌ Missing: Type-level "implements" relationship queries
- ❌ Missing: Interface method to implementation mapping

**Implementation Requirements:**
```rust
// File to create: /Users/mohsenazimi/code/tsz/src/lsp/implementation.rs

pub struct GoToImplementationProvider<'a> {
    arena: &'a NodeArena,
    binder: &'a BinderState,
    checker: &'a CheckerState,  // Need type info!
    line_map: &'a LineMap,
    file_name: String,
}

impl GoToImplementationProvider<'_> {
    pub fn get_implementations(&self, node_idx: NodeIndex) -> Vec<Location> {
        // 1. Determine if cursor is on:
        //    - Interface declaration
        //    - Interface method
        //    - Abstract class method
        // 2. Query SymbolIndex for all classes in project
        // 3. Use Checker to verify which classes implement the interface
        // 4. For interface methods, find matching method signatures
        // 5. Return locations of all implementations
    }
}
```

**Key Technical Challenges:**

1. **Type System Integration**
   - Need to expose "implements" relationships from `CheckerState`
   - Must traverse interface implementation chains
   - Handle class inheritance + interface implementation

2. **Symbol Resolution**
   - Find all classes that could implement the target
   - Filter by checking type compatibility
   - Handle re-exports and type aliases

3. **Method Matching**
   - For interface methods, find corresponding concrete methods
   - Match by signature (name + parameter types)
   - Handle override methods

**Existing Relevant Code:**
- `/Users/mohsenazimi/code/tsz/src/lsp/code_lens.rs:274-294` - Placeholder implementation lens
- `/Users/mohsenazimi/code/tsz/src/checker/class_inheritance.rs` - Class inheritance logic
- `/Users/mohsenazimi/code/tsz/src/checker/class_checker.rs` - Class checking

**Implementation Steps:**
1. Add "implements" relationship tracking to type checker (2-3 days)
2. Create `implementation.rs` provider (2-3 days)
3. Implement interface → implementation queries (2-3 days)
4. Handle method signature matching (2-3 days)
5. Add tests for complex hierarchies (1-2 days)
6. Wire up in `tsz_server.rs` (1 day)

**Why Priority 2:**
- High user value, especially for TypeScript codebases
- More complex than Workspace Symbols
- Builds foundational "implements" infrastructure for Type Hierarchy

**Estimated Effort:** 1-2 weeks

---

### 3. Call Hierarchy (textDocument/callHierarchy)

**LSP Method:**
- `textDocument/prepareCallHierarchy`
- `callHierarchy/incomingCalls`
- `callHierarchy/outgoingCalls`

**Status:** ❌ Missing

**Description:** Visualize the flow of calls to and from a function. Shows which functions call the current function (incoming) and which functions the current function calls (outgoing).

**User Value:** ⭐⭐⭐⭐ MEDIUM-HIGH
- Powerful for understanding legacy code
- Essential for refactoring (what breaks if I change this?)
- Helps trace execution paths
- Valuable for impact analysis

**Implementation Complexity:** 🟡 MEDIUM (3-5 days)

**Dependencies:** MEDIUM - can reuse existing code
- ✅ `references.rs` already implements "Find References" (incoming calls!)
- ✅ `resolver.rs` has scope walking logic
- ⚠️ Missing: AST walker for outgoing calls (finding `CallExpression` nodes)

**Implementation Requirements:**
```rust
// File to create: /Users/mohsenazimi/code/tsz/src/lsp/call_hierarchy.rs

pub struct CallHierarchyProvider<'a> {
    arena: &'a NodeArena,
    binder: &'a BinderState,
    line_map: &'a LineMap,
    file_name: String,
}

impl CallHierarchyProvider<'_> {
    pub fn prepare(&self, position: Position) -> Option<Vec<CallHierarchyItem>> {
        // Return CallHierarchyItem for function at cursor
    }

    pub fn incoming_calls(&self, item: &CallHierarchyItem) -> Vec<CallHierarchyIncomingCall> {
        // Reuse FindReferences - find all call sites
        // Group by calling function
        // Return ranges where this function is called
    }

    pub fn outgoing_calls(&self, item: &CallHierarchyItem) -> Vec<CallHierarchyOutgoingCall> {
        // Walk function body AST
        // Find all CallExpression nodes
        // Resolve callee to function definitions
        // Return which functions this function calls
    }
}
```

**Key Technical Challenges:**

1. **Incoming Calls** (Easy)
   - Directly reuse `FindReferences` from `references.rs`
   - Already implemented!
   - Just need to group by calling function

2. **Outgoing Calls** (Medium)
   - Need new AST visitor to walk function body
   - Find all `CallExpression` nodes
   - Resolve call targets using `Binder`
   - Handle indirect calls (callbacks, methods)

3. **Call Hierarchy Item Structure**
   - Create item for function at cursor
   - Include name, kind, range, detail
   - Support navigation to/from items

**Existing Relevant Code:**
- `/Users/mohsenazimi/code/tsz/src/lsp/references.rs` (1,136 lines) - Find references logic
- `/Users/mohsenazimi/code/tsz/src/lsp/resolver.rs` (1,971 lines) - Scope resolution

**Implementation Steps:**
1. Create `call_hierarchy.rs` module structure (1 day)
2. Implement `prepareCallHierarchy` - resolve function at cursor (1 day)
3. Implement `incomingCalls` - reuse `FindReferences` (1 day)
4. Implement `outgoingCalls` - AST walker for call expressions (1-2 days)
5. Handle method calls, nested calls, complex cases (1 day)
6. Add tests (1 day)

**Why Priority 3:**
- Incoming calls are trivial (reuse existing code)
- Outgoing calls are moderate effort
- Provides significant value for refactoring
- No deep type system changes required

**Estimated Effort:** 3-5 days

---

### 4. Type Hierarchy (textDocument/typeHierarchy)

**LSP Method:**
- `textDocument/prepareTypeHierarchy`
- `typeHierarchy/supertypes`
- `typeHierarchy/subtypes`

**Status:** ❌ Missing

**Description:** Navigate type inheritance and interface implementation chains. Shows parent types (supertypes) and derived types (subtypes).

**User Value:** ⭐⭐⭐ MEDIUM
- Useful for understanding class hierarchies
- Less frequently used than "Go to Implementation"
- Valuable for object-oriented codebases
- Helps visualize inheritance trees

**Implementation Complexity:** 🔴 HIGH (1-2 weeks)

**Dependencies:** HIGH - requires type system infrastructure
- ✅ `class_inheritance.rs` has cycle detection
- ⚠️ `CheckerState` has type info but not exposed for queries
- ❌ Missing: Bidirectional inheritance graph
- ❌ Missing: "subtype of" relationship index

**Implementation Requirements:**
```rust
// File to create: /Users/mohsenazimi/code/tsz/src/lsp/type_hierarchy.rs

pub struct TypeHierarchyProvider<'a> {
    arena: &'a NodeArena,
    binder: &'a BinderState,
    checker: &'a CheckerState,  // Need type info!
    symbol_index: &'a SymbolIndex,
    line_map: &'a LineMap,
    file_name: String,
}

impl TypeHierarchyProvider<'_> {
    pub fn prepare(&self, position: Position) -> Option<Vec<TypeHierarchyItem>> {
        // Return TypeHierarchyItem for class/interface at cursor
    }

    pub fn supertypes(&self, item: &TypeHierarchyItem) -> Vec<TypeHierarchyItem> {
        // Follow extends clause
        // Follow implements clause
        // Return parent types
    }

    pub fn subtypes(&self, item: &TypeHierarchyItem) -> Vec<TypeHierarchyItem> {
        // Query SymbolIndex for all classes
        // Use Checker to find classes that extend/implement this type
        // Return child types
    }
}
```

**Key Technical Challenges:**

1. **Supertypes** (Easy-Medium)
   - Parse `extends` and `implements` clauses
   - Direct parent relationships are in AST
   - Follow chain to root (Object)

2. **Subtypes** (Hard)
   - Need to find all classes that extend this class
   - Need to find all classes that implement this interface
   - Requires querying entire project
   - Must use type checker for compatibility

3. **Type System Integration**
   - Need to expose inheritance graph from `CheckerState`
   - Build reverse index: type → list of subtypes
   - Cache results for performance

**Existing Relevant Code:**
- `/Users/mohsenazimi/code/tsz/src/checker/class_inheritance.rs` - Inheritance cycle detection
- `/Users/mohsenazimi/code/tsz/src/checker/class_checker.rs` - Class type checking

**Implementation Steps:**
1. Build inheritance index during type checking (2-3 days)
2. Create `type_hierarchy.rs` module (1 day)
3. Implement `prepareTypeHierarchy` (1 day)
4. Implement `supertypes` - walk extends/implements (1-2 days)
5. Implement `subtypes` - query inheritance index (2-3 days)
6. Handle generic types, type parameters (2-3 days)
7. Add tests for complex hierarchies (1-2 days)

**Why Priority 4:**
- High complexity (similar to Go to Implementation)
- Lower user value (less frequently used)
- Should build on "implements" infrastructure from Priority 2
- Can be implemented after Go to Implementation

**Estimated Effort:** 1-2 weeks

---

### 5. Document Links (textDocument/documentLink)

**LSP Method:** `textDocument/documentLink`

**Status:** ❌ Missing

**Description:** Make string literals that represent paths clickable. Used for import statements, `require()` calls, and file paths in comments.

**User Value:** ⭐⭐ LOW-MEDIUM
- Nice quality-of-life improvement
- Makes imports clickable
- Less critical than navigation features
- Already partially solved by most editors

**Implementation Complexity:** 🟢 LOW (1 day)

**Dependencies:** MEDIUM - can reuse existing logic
- ✅ `project_operations.rs` has `resolve_module_specifier` (line 1691)
- ✅ Module resolution logic already exists
- ⚠️ Missing: AST visitor for import string literals

**Implementation Requirements:**
```rust
// File to create: /Users/mohsenazimi/code/tsz/src/lsp/document_links.rs

pub struct DocumentLinkProvider<'a> {
    arena: &'a NodeArena,
    binder: &'a BinderState,
    line_map: &'a LineMap,
    file_name: String,
}

impl DocumentLinkProvider<'_> {
    pub fn document_links(&self, root: NodeIndex) -> Vec<DocumentLink> {
        // 1. Walk AST for ImportDeclaration nodes
        // 2. Extract module_specifier string literal
        // 3. Call resolve_module_specifier to get target file
        // 4. Create DocumentLink with:
        //    - range (the string literal range)
        //    - target (resolved file path as URI)
        //    - tooltip (optional)
        // 5. Return all links
    }
}
```

**Existing Relevant Code:**
- `/Users/mohsenazimi/code/tsz/src/lsp/project_operations.rs:1691` - `resolve_module_specifier` function
- Used in 8 locations for import resolution

**Implementation Steps:**
1. Create `document_links.rs` module (0.5 day)
2. Walk AST for import/export declarations (0.5 day)
3. Extract string literals and resolve targets (0.5 day)
4. Return DocumentLink objects (0.5 day)
5. Add handler in `tsz_server.rs` (0.5 day)

**Why Priority 4:**
- Very low hanging fruit
- Quick win to complete more LSP coverage
- Low complexity, can be done quickly
- Nice-to-have feature, not critical

**Estimated Effort:** 1 day

---

### 6. Native Formatting (textDocument/formatting)

**LSP Method:** `textDocument/formatting`

**Status:** ⚠️ Partial - delegates to external tools (Prettier, ESLint)

**Description:** Format code according to style guidelines. Currently delegates to `prettier` or `eslint` CLI tools.

**User Value:** ⭐⭐⭐⭐ HIGH
- Code formatting is essential
- But delegation to Prettier is standard industry practice
- Native formatter would be nice but not critical

**Implementation Complexity:** 🔴 VERY HIGH (Months)

**Dependencies:** MEDIUM - printer infrastructure exists
- ✅ `emitter/printer.rs` has code generation logic
- ⚠️ Printer is for output, not formatting preservation
- ❌ Missing: Whitespace preservation
- ❌ Missing: Comment preservation
- ❌ Missing: Non-destructive edit logic
- ❌ Missing: Style configuration

**Why This Should Be Lowest Priority:**
- Current delegation to Prettier/ESLint is acceptable
- Writing a production formatter is extremely complex
- Requires:
  - Sophisticated whitespace handling
  - Comment preservation across transformations
  - User preference parsing (indent size, tabs vs spaces, etc.)
  - Extensive testing for edge cases
  - Ongoing maintenance for style changes
- TypeScript's formatter is thousands of lines of complex logic
- ROI is low compared to other features

**Recommendation:** ❌ DO NOT IMPLEMENT NOW
- Defer until all core compiler features are complete
- Consider using existing Rust formatters (rustfmt, prettyplease) as reference
- Could eventually adapt `emitter/printer.rs` for formatting

**Estimated Effort:** 2-3 months (not recommended)

---

## Implementation Priority Ranking

Based on complexity, user value, and dependencies, here is the recommended implementation order:

### Phase 1: Quick Wins (Week 1)

**1. Workspace Symbols** (Priority 1)
- **Effort:** 1-2 days
- **Value:** High
- **Risk:** Low
- **Why:** Infrastructure exists, provides immediate value

**2. Document Links** (Priority 4)
- **Effort:** 1 day
- **Value:** Low-Medium
- **Risk:** Very Low
- **Why:** Trivial implementation, completes LSP coverage

**Phase 1 Total: 2-3 days**

---

### Phase 2: Navigation & Refactoring (Weeks 2-4)

**3. Call Hierarchy** (Priority 3)
- **Effort:** 3-5 days
- **Value:** Medium-High
- **Risk:** Medium
- **Why:** Reuses existing `FindReferences`, moderate complexity

**4. Go to Implementation** (Priority 2)
- **Effort:** 1-2 weeks
- **Value:** Very High
- **Risk:** High
- **Why:** Critical for TypeScript, builds foundational infrastructure

**Phase 2 Total: 2-3 weeks**

---

### Phase 3: Advanced Features (Weeks 5-6)

**5. Type Hierarchy** (Priority 5)
- **Effort:** 1-2 weeks
- **Value:** Medium
- **Risk:** High
- **Why:** Builds on "implements" infrastructure from Priority 2

**Phase 3 Total: 1-2 weeks**

---

### Defer Indefinitely

**6. Native Formatting**
- **Effort:** 2-3 months
- **Value:** Medium (delegation is acceptable)
- **Risk:** Very High
- **Why:** Low ROI, external tools are standard

---

## Dependencies and Prerequisites

### Cross-Feature Dependencies

```
Go to Implementation (Priority 2)
    └──> Type Hierarchy (Priority 5)
         └──> Can reuse "implements" infrastructure

Call Hierarchy (Priority 3)
    └──> Independent, no dependencies

Workspace Symbols (Priority 1)
    └──> Independent, no dependencies

Document Links (Priority 4)
    └──> Independent, no dependencies
```

### Required Infrastructure Additions

**For Go to Implementation & Type Hierarchy:**

1. **Type System Extensions** (3-5 days)
   - Expose "implements" relationships from `CheckerState`
   - Build interface → implementations index
   - Add class/interface hierarchy queries

2. **Symbol Index Enhancements** (1-2 days)
   - Add type kind metadata to symbols
   - Track inheritance relationships
   - Build reverse lookup (interface → implementing classes)

**For Call Hierarchy:**

1. **AST Visitors** (2-3 days)
   - Create function body walker for outgoing calls
   - Find all `CallExpression` nodes
   - Resolve call targets

**For Workspace Symbols:**

1. **Fuzzy Matching** (1 day)
   - Add substring search
   - Optional: implement fuzzy scoring algorithm
   - Result ranking

---

## Effort Estimates Summary

| Feature | Complexity | User Value | Estimated Effort | Priority |
|---------|-----------|------------|------------------|----------|
| Workspace Symbols | Low | ⭐⭐⭐⭐⭐ | 1-2 days | 1 |
| Go to Implementation | High | ⭐⭐⭐⭐⭐ | 1-2 weeks | 2 |
| Call Hierarchy | Medium | ⭐⭐⭐⭐ | 3-5 days | 3 |
| Document Links | Low | ⭐⭐ | 1 day | 4 |
| Type Hierarchy | High | ⭐⭐⭐ | 1-2 weeks | 5 |
| Native Formatting | Very High | ⭐⭐⭐⭐ | 2-3 months | ❌ Defer |

**Total Recommended Effort:** 3-5 weeks for top 5 features

---

## File Structure for New Features

```
src/lsp/
├── workspace_symbols.rs     [NEW] - Workspace-wide symbol search
├── implementation.rs        [NEW] - Go to implementation navigation
├── call_hierarchy.rs        [NEW] - Call hierarchy visualization
├── type_hierarchy.rs        [NEW] - Type hierarchy navigation
├── document_links.rs        [NEW] - Clickable import paths
└── mod.rs                   [UPDATE] - Export new providers

src/bin/
└── tsz_server.rs            [UPDATE] - Add LSP method handlers
```

---

## Testing Strategy

### Test Files to Create

```
src/lsp/tests/
├── workspace_symbols_tests.rs      [NEW]
├── implementation_tests.rs          [NEW]
├── call_hierarchy_tests.rs          [NEW]
├── type_hierarchy_tests.rs          [NEW]
└── document_links_tests.rs          [NEW]
```

### Test Coverage Targets

**Workspace Symbols:**
- Fuzzy name matching
- Case-insensitive search
- Multiple symbol types (functions, classes, interfaces)
- Result ranking
- Large project performance

**Go to Implementation:**
- Interface → class implementations
- Interface method → concrete methods
- Abstract class → concrete classes
- Multiple implementations
- Re-export handling
- Generic interfaces

**Call Hierarchy:**
- Incoming calls (reuse FindReferences tests)
- Outgoing calls (new tests)
- Direct calls
- Indirect calls (callbacks, methods)
- Nested calls
- Recursive call detection

**Type Hierarchy:**
- Supertypes (extends, implements)
- Subtypes (derived classes)
- Multiple inheritance (interfaces)
- Generic type parameters
- Deep inheritance chains

**Document Links:**
- Import declarations
- Export declarations
- Dynamic imports
- Relative paths
- Node module resolution

---

## Performance Considerations

### SymbolIndex Usage

The `SymbolIndex` (547 lines) provides efficient O(1) lookups and is already used for:
- Reference finding
- Definition resolution
- Export/import tracking

**Optimization Strategies:**
1. **Incremental Updates** - Already supported via `update_file()`
2. **Lazy Loading** - Build hierarchy indexes on-demand
3. **Caching** - Cache expensive queries (implementation lists, hierarchies)
4. **Project-Wide Indexing** - Index all files on project load

### Potential Bottlenecks

**Go to Implementation:**
- Requires querying all classes in project
- Solution: Pre-compute "implements" index during binding

**Type Hierarchy Subtypes:**
- Requires full project scan for each query
- Solution: Build reverse index (type → subtypes) incrementally

**Call Hierarchy Outgoing:**
- Requires AST walk for each function
- Solution: Cache call graph, update incrementally

---

## Compatibility with Existing Code

### Reusable Components

| New Feature | Can Reuse | From File |
|-------------|-----------|-----------|
| Workspace Symbols | SymbolIndex | `symbol_index.rs` |
| Go to Implementation | SymbolIndex, type checking | `symbol_index.rs`, `class_checker.rs` |
| Call Hierarchy (incoming) | FindReferences | `references.rs` |
| Call Hierarchy (outgoing) | Scope resolution | `resolver.rs` |
| Document Links | Module resolution | `project_operations.rs` |
| Type Hierarchy | Class inheritance logic | `class_inheritance.rs` |

### No Breaking Changes Required

All new features are **additive** - they don't modify existing code:
- New modules only
- New LSP handlers only
- No changes to existing providers
- No changes to type checker API (for initial implementation)

---

## Integration with tsz_server.rs

### Required Server Handlers

```rust
// In src/bin/tsz_server.rs

impl TsServer {
    // Add to handle_request method:
    "workspace/symbol" => self.handle_workspace_symbols(seq, &request),
    "textDocument/implementation" => self.handle_implementation(seq, &request),
    "textDocument/callHierarchy" => self.handle_call_hierarchy(seq, &request),
    "textDocument/typeHierarchy" => self.handle_type_hierarchy(seq, &request),
    "textDocument/documentLink" => self.handle_document_link(seq, &request),
}
```

### Capability Registration

Update server capabilities to advertise new features:

```rust
ServerCapabilities {
    workspace_symbol_provider: Some(OneOf::Left(true)),
    implementation_provider: Some(ImplementationProviderCapability::Simple(true)),
    call_hierarchy_provider: Some(CallHierarchyServerCapabilities::Simple(true)),
    type_hierarchy_provider: Some(TypeHierarchyServerCapabilities::Simple(true)),
    document_link_provider: Some(DocumentLinkOptions {
        resolve_provider: Some(false),
        work_done_progress_options: WorkDoneProgressOptions { work_done_progress: None },
    }),
    // ... existing capabilities
}
```

---

## Comparison with TypeScript LSP

### Feature Parity

| Feature | TypeScript | tsz (Current) | tsz (After This Plan) |
|---------|-----------|---------------|----------------------|
| Workspace Symbols | ✅ | ❌ | ✅ |
| Go to Implementation | ✅ | ⚠️ Stub | ✅ |
| Call Hierarchy | ✅ | ❌ | ✅ |
| Type Hierarchy | ✅ | ❌ | ✅ |
| Document Links | ✅ | ❌ | ✅ |
| Formatting | ✅ Native | ⚠️ Delegated | ⚠️ Delegated (acceptable) |

**Post-Implementation Parity:** ~95% (missing only native formatter)

---

## Risk Assessment

### Technical Risks

**Go to Implementation & Type Hierarchy**
- **Risk:** Type system may not expose needed relationships
- **Mitigation:** Start with interfaces, add complexity incrementally
- **Fallback:** Provide partial results (direct implementations only)

**Call Hierarchy Outgoing**
- **Risk:** Complex call resolution (callbacks, dynamic calls)
- **Mitigation:** Start with direct calls, add indirect calls later
- **Fallback:** Show only direct call expressions

**Performance**
- **Risk:** Large projects may be slow
- **Mitigation:** Use incremental indexing, caching
- **Fallback:** Limit results, add pagination

### Schedule Risks

**Optimistic Estimate:** 3 weeks
**Realistic Estimate:** 4-5 weeks
**Conservative Estimate:** 6-7 weeks

**Buffer for Unexpected Issues:** +30%

---

## Success Criteria

### Phase 1 Success (Week 1)
- ✅ Workspace Symbols returns top 100 results in <100ms
- ✅ Document Links resolves all imports in <50ms
- ✅ Both features work on projects with 1000+ files

### Phase 2 Success (Weeks 2-4)
- ✅ Go to Implementation finds all interface implementations
- ✅ Call Hierarchy shows incoming/outgoing calls
- ✅ Both features work on complex inheritance hierarchies

### Phase 3 Success (Weeks 5-6)
- ✅ Type Hierarchy navigates supertypes/subtypes
- ✅ All features integrated into tsz_server
- ✅ Test coverage >80% for new features

---

## Recommendations

### Immediate Actions (Next Sprint)

1. **Start with Workspace Symbols**
   - Low-risk, high-value quick win
   - Validates SymbolIndex approach
   - Builds confidence for more complex features

2. **Add Document Links**
   - Trivial implementation
   - Completes more LSP coverage
   - Provides visible user benefit

### Short-Term Plan (Next 2-3 Sprints)

3. **Implement Call Hierarchy**
   - Reuses existing `FindReferences` (incoming)
   - Moderate complexity for outgoing
   - High value for refactoring

4. **Implement Go to Implementation**
   - Most critical missing TypeScript feature
   - Requires type system extensions
   - High user value

### Medium-Term Plan (Following Sprints)

5. **Implement Type Hierarchy**
   - Builds on Go to Implementation infrastructure
   - Completes type navigation features
   - Medium priority

### Long-Term Consideration

6. **Defer Native Formatting**
   - Current delegation is acceptable
   - Re-evaluate after core features complete
   - Consider partnership with existing formatter projects

---

## Appendix A: LSP Specification References

- [LSP 3.17 Specification - Workspace Symbols](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#workspace_workspaceSymbols)
- [LSP 3.17 Specification - Implementation](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_implementation)
- [LSP 3.17 Specification - Call Hierarchy](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_callHierarchy)
- [LSP 3.17 Specification - Type Hierarchy](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_typeHierarchy)
- [LSP 3.17 Specification - Document Links](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_documentLink)

---

## Appendix B: TypeScript Test Files for Reference

The following TypeScript test files demonstrate expected behavior:

**Call Hierarchy Tests:**
- `/Users/mohsenazimi/code/tsz/TypeScript/tests/baselines/reference/callHierarchyInterfaceMethod.callHierarchy.txt`
- `/Users/mohsenazimi/code/tsz/TypeScript/tests/baselines/reference/callHierarchyClass.callHierarchy.txt`
- `/Users/mohsenazimi/code/tsz/TypeScript/tests/baselines/reference/callHierarchyFunction.callHierarchy.txt`

**Server Tests:**
- `/Users/mohsenazimi/code/tsz/TypeScript/tests/baselines/reference/tsserver/fourslashServer/callHierarchyContainerNameServer.js`

These can be used as reference for expected behavior and edge cases.

---

## Conclusion

Research Team 7 has identified 6 major missing LSP features in the tsz TypeScript compiler. The top 5 features can be implemented in **3-5 weeks** with **high user value** and **manageable complexity**.

**Recommended Implementation Order:**
1. Workspace Symbols (2 days) - Quick win, high value
2. Document Links (1 day) - Trivial, completes coverage
3. Call Hierarchy (5 days) - Reuses existing code
4. Go to Implementation (2 weeks) - Critical for TypeScript
5. Type Hierarchy (2 weeks) - Builds on #4

**Total Effort:** 3-5 weeks for 95% LSP feature parity

**Next Step:** Begin implementation of Workspace Symbols as a proof-of-concept and validate the SymbolIndex-based approach.

---

**Report Prepared By:** Research Team 7
**Report Date:** January 30, 2026
**Tools Used:** Codebase analysis, Gemini AI (gemini-3-pro-preview via ask-gemini.mjs)
**Files Analyzed:** 37 files (963 KB of source code)
