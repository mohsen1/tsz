# LSP Implementation Gaps

This document catalogs the Language Server Protocol (LSP) implementation status in TSZ, identifying implemented features, stub features, and known limitations.

## Overview

TSZ provides **LSP feature modules** and a basic **LSP server binary** (`tsz-lsp`). The feature modules can be used for WASM-based integrations (VS Code extensions, editor plugins), while the server binary provides a ready-to-use LSP implementation over stdio.

### Implementation Summary

| Status | Count | Description |
|--------|-------|-------------|
| ✅ Implemented | 20 | Fully functional features |
| ⚠️ Partial | 2 | Features with known limitations |
| ❌ Stub | 0 | Empty placeholder modules |

**Total LOC**: ~34,000 lines of Rust across 32 files (excluding tests)

**New in this update**:
- Selection Range fully implemented
- Type Definition fully implemented
- Code Lens fully implemented
- LSP server binary (`tsz-lsp`) added

---

## Feature Status Matrix

### Core Navigation

| Feature | Status | File | Lines | Notes |
|---------|--------|------|-------|-------|
| Go to Definition | ✅ | `definition.rs` | ~880 | Resolves identifiers to declarations |
| Find References | ✅ | `references.rs` | ~1,140 | Finds all symbol usages |
| Type Definition | ✅ | `type_definition.rs` | ~500 | Navigate to type definition |
| Document Highlighting | ✅ | `highlighting.rs` | ~400 | Read/write occurrence distinction |

### Code Intelligence

| Feature | Status | File | Lines | Notes |
|---------|--------|------|-------|-------|
| Completions | ✅ | `completions.rs` | ~1,190 | Symbols + keywords |
| Signature Help | ⚠️ | `signature_help.rs` | ~1,140 | Known issue with member calls |
| Hover | ✅ | `hover.rs` | ~520 | Type info + JSDoc |
| Inlay Hints | ⚠️ | `inlay_hints.rs` | ~305 | Type hints not implemented |

### Refactoring

| Feature | Status | File | Lines | Notes |
|---------|--------|------|-------|-------|
| Rename | ✅ | `rename.rs` | ~615 | Workspace-wide symbol rename |
| Code Actions | ✅ | `code_actions.rs` | ~2,730 | Extract, organize imports, quick fixes |
| Code Lens | ✅ | `code_lens.rs` | ~550 | Reference counts, implementations |

### Document Structure

| Feature | Status | File | Lines | Notes |
|---------|--------|------|-------|-------|
| Document Symbols | ✅ | `document_symbols.rs` | ~675 | Hierarchical outline |
| Semantic Tokens | ✅ | `semantic_tokens.rs` | ~790 | Full token classification |
| Folding Ranges | ✅ | `folding.rs` | ~535 | Blocks, comments, imports |
| Selection Range | ✅ | `selection_range.rs` | ~350 | Semantic expand/shrink |

### Workspace

| Feature | Status | File | Lines | Notes |
|---------|--------|------|-------|-------|
| Diagnostics | ✅ | `diagnostics.rs` | ~165 | Error/warning reporting |
| Formatting | ✅ | `formatting.rs` | ~400 | Delegates to external tools |

---

## Recently Implemented Features

The following features were previously stubs but are now fully implemented:

### 1. Type Definition (`type_definition.rs`) - ✅ Implemented

Navigates from a variable to its **type definition** (not value declaration).
- Resolves symbol at cursor position
- Extracts type annotation from declarations
- Finds type declarations (interfaces, type aliases, classes, enums)
- Handles TypeReference, ArrayType, UnionType, etc.

### 2. Code Lens (`code_lens.rs`) - ✅ Implemented

Displays actionable information above code elements:
- Reference counts above functions, classes, interfaces
- Implementation finder for interfaces
- Supports lazy resolution for performance

### 3. Selection Range (`selection_range.rs`) - ✅ Implemented

Expands/shrinks selection by semantic boundaries:
- Builds parent chain from cursor position to root
- Filters out internal structural nodes
- Returns nested SelectionRange for smart selection

---

## Partial Implementations

### 1. Signature Help - Incomplete Member Calls

**Location**: `signature_help.rs`, test in `signature_help_tests.rs:153`

**Issue**: Signature help fails for incomplete member method calls
```typescript
interface Obj { method(a: number, b: string): void; }
declare const obj: Obj;
obj.method(|  // ← Signature help doesn't work here
```

**Root Cause**: Member expression resolution doesn't fully handle cases where the call is incomplete (missing closing paren, typing in progress).

**Test Status**:
```rust
#[test]
#[ignore = "TODO: Signature help for incomplete member calls"]
fn test_signature_help_incomplete_member_call()
```

**Impact**: Users don't get parameter hints while typing method calls with `.` notation.

---

### 2. Inlay Hints - Type Hints Not Implemented

**Location**: `inlay_hints.rs:270-279`

**Issue**: Type hints for implicitly-typed variables are not implemented
```rust
/// Collect type hints for variable declarations without explicit type annotations.
fn collect_type_hints(&self, _decl_idx: NodeIndex, _hints: &mut Vec<InlayHint>) {
    // Type hints require type inference which needs the TypeInterner.
    // For now, this is a placeholder.
}
```

**What Works**:
- ✅ Parameter name hints for function calls

**What's Missing**:
- ❌ Type hints for `let x = 1` → showing `: number`
- ❌ Generic parameter hints where types are inferred

**Implementation Requirements**:
- Add `TypeInterner` and `CheckerState` to `InlayHintsProvider`
- Call type inference for variable declarations without explicit types
- Format inferred type as hint text

---

## Code Actions Inventory

The following code actions are implemented in `code_actions.rs`:

| Action | Kind | Description |
|--------|------|-------------|
| Remove Unused Import | `quickfix` | Delete unused import statements |
| Add Missing Property | `quickfix` | Add property to object literal |
| Add Missing Import | `quickfix` | Import unresolved identifier |
| Organize Imports | `source.organizeImports` | Sort import statements alphabetically |
| Extract to Constant | `refactor.extract` | Extract selection to named constant |

### Missing Code Actions (vs TypeScript)

| Action | Priority | Notes |
|--------|----------|-------|
| Extract Function | High | Extract selection to function |
| Extract Method | High | Extract selection to class method |
| Move to New File | Medium | Move declaration to separate file |
| Generate Getter/Setter | Medium | From class property |
| Implement Interface | Medium | Add missing members |
| Add All Missing Imports | Medium | Batch import resolution |
| Convert to Named Import | Low | Convert `import x` to `import { x }` |
| Convert to Default Import | Low | Inverse of above |
| Infer Function Return Type | Low | Add explicit return type |

---

## Architectural Limitations

### 1. LSP Server (tsz-lsp) - Basic Infrastructure

**Status**: A basic LSP server binary (`tsz-lsp`) is now available.

**Current Capabilities**:
- JSON-RPC protocol handling over stdio
- Document synchronization (open/change/close)
- Go to Definition, Type Definition
- Find References
- Document Symbols
- Selection Range
- Code Lens

**Pending Features** (require full type checker):
- Hover (needs TypeInterner)
- Full Completions (needs type-aware suggestions)
- Signature Help (needs function type info)
- Semantic Tokens (partial - needs type classification)

**Usage**:
```bash
tsz-lsp                    # Start server on stdio
tsz-lsp --verbose          # With debug logging
```

---

### 2. Stateless Query Model

**Issue**: Each LSP operation independently parses, binds, and resolves.

**Pros**:
- Thread-safe
- WASM compatible
- No complex state management

**Cons**:
- Inefficient for repeated queries on same file
- No incremental updates (except Project container)
- Limited caching opportunities

**Impact**: Performance may degrade with frequent queries on large files.

---

### 3. Single-File Focus

**Issue**: Most LSP features work within individual files; cross-file navigation is limited.

**Affected Features**:
- Find References: May miss references in unopened files
- Rename: Requires all affected files to be in Project
- Add Missing Import: Depends on pre-populated import candidates

**Workaround**: Use `Project` container to manage multi-file contexts.

---

### 4. Type Checker Integration Gaps

**Issue**: Some LSP features depend on type information that may be incomplete.

**Root Cause**: Type checker gaps documented in `07-gaps-summary.md` affect LSP accuracy:
- Definite assignment analysis missing
- TDZ checking incomplete
- Intersection reduction partial
- Promise detection issues

**Affected Features**:
- Hover: May show incomplete type information
- Completions: May miss members from complex types
- Signature Help: May show incorrect parameter types

---

## Missing LSP Protocol Features

Features from the LSP specification not implemented at all:

| Feature | LSP Method | Priority |
|---------|-----------|----------|
| Workspace Symbols | `workspace/symbol` | Medium |
| Document Links | `textDocument/documentLink` | Low |
| Document Colors | `textDocument/documentColor` | Low |
| Linked Editing | `textDocument/linkedEditingRange` | Low |
| Call Hierarchy | `textDocument/callHierarchy` | Medium |
| Type Hierarchy | `textDocument/typeHierarchy` | Medium |
| Moniker | `textDocument/moniker` | Low |
| Inline Values | `textDocument/inlineValue` | Low |

---

## Testing Gaps

### Ignored Tests

| File | Test | Reason |
|------|------|--------|
| `signature_help_tests.rs:153` | `test_signature_help_incomplete_member_call` | Incomplete member call resolution |

### Untested Scenarios

Based on code review:

1. **Completions**: Object spread completions, JSX attribute completions
2. **Hover**: Generic instantiation display, union type formatting
3. **Code Actions**: Conflict resolution when multiple fixes apply
4. **Rename**: Renaming across re-exports, renaming in string literals

---

## Recommended Implementation Priority

### Phase 1: Complete Partial Features ✅ (Mostly Done)

1. ~~**Selection Range**~~ - ✅ Implemented
2. ~~**Type Definition**~~ - ✅ Implemented
3. ~~**Code Lens**~~ - ✅ Implemented
4. **Signature Help for Member Calls** - Fix existing feature (1 ignored test)
5. **Inlay Type Hints** - Complete partial implementation (needs TypeInterner)

### Phase 2: Type Checker Integration

6. **Hover with Full Types** - Currently stubbed in tsz-lsp, needs TypeInterner
7. **Full Completions** - Type-aware member suggestions
8. **Semantic Tokens with Types** - Classify by inferred type

### Phase 3: Missing Navigation

9. **Call Hierarchy** - Understand call relationships
10. **Type Hierarchy** - Navigate type inheritance
11. **Workspace Symbols** - Search across project

### Phase 4: Refactoring

12. **Extract Function/Method** - Common refactoring need
13. **Implement Interface** - Add missing members
14. **Additional Code Actions** - See missing code actions list above

### Phase 5: Advanced Features

15. **Linked Editing** - HTML/JSX tag renaming
16. **Document Links** - Clickable paths in comments
17. **Inline Values** - Debug variable values

---

## File Reference

All LSP modules are located in `src/lsp/`:

```
src/lsp/
├── mod.rs              Module exports and documentation
├── project.rs          Multi-file project container
├── resolver.rs         Symbol resolution utilities
├── code_actions.rs     Quick fixes and refactorings
├── completions.rs      Code completion
├── signature_help.rs   Parameter hints
├── references.rs       Find references
├── definition.rs       Go to definition
├── semantic_tokens.rs  Syntax highlighting
├── document_symbols.rs File outline
├── rename.rs           Symbol rename
├── code_lens.rs        Code lens
├── folding.rs          Code folding
├── hover.rs            Hover information
├── type_definition.rs  Type definition
├── highlighting.rs     Document highlighting
├── formatting.rs       Code formatting
├── selection_range.rs  Selection range
├── inlay_hints.rs      Inlay hints
├── position.rs         Position utilities
├── jsdoc.rs            JSDoc parsing
├── diagnostics.rs      Error reporting
├── utils.rs            Shared utilities
├── symbols.rs          Symbol definitions
├── symbol_index.rs     Symbol indexing
├── dependency_graph.rs Dependency tracking
└── *_tests.rs          Test files

src/bin/
└── tsz_lsp.rs          LSP server binary
```

---

## Contributing

When implementing a missing LSP feature:

1. Check the [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/) for expected behavior
2. Look at TypeScript's implementation for reference
3. Add comprehensive tests covering edge cases
4. Update this document with the new feature status
5. Consider WASM compatibility (no filesystem access, no threads)

**See also**:
- [07-gaps-summary.md](./07-gaps-summary.md) - Type system gaps affecting LSP
- [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/)
- [VS Code LSP Guide](https://code.visualstudio.com/api/language-extensions/language-server-extension-guide)

---

*Last updated: January 2026*
