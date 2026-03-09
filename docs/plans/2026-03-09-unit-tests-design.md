# Unit Tests for Major Crates - Design Document

## Overview

Add comprehensive unit tests for under-tested crates in the TSZ compiler:
- **tsz-binder**: Core data structures (symbols, flow, scopes)
- **tsz-lowering**: AST → TypeId lowering edge cases
- **tsz-scanner**: Edge cases for regex, unicode, numeric literals
- **tsz-cli**: File watching, incremental compilation

## Current State

| Crate | Source Files | Test Files | Inline Tests | Gap Level |
|-------|--------------|------------|--------------|-----------|
| tsz-binder | 21 | 2 | 115 | High |
| tsz-lowering | 4 | 1 | 0 | Medium |
| tsz-scanner | 3 | 2 | 226 | Low |
| tsz-cli | 52 | 9 | 289 | Medium |

## Design

### 1. Binder Tests

**New Files**: `crates/tsz-binder/tests/`

#### `symbols_tests.rs`
- SymbolId NONE behavior and sentinel value handling
- Symbol creation with various flag combinations
- SymbolTable CRUD operations (get, set, remove, has, iter)
- SymbolArena allocation, lookup, and iteration
- Flag helper methods (has_flags, has_any_flags)
- Symbol name lookup (find_by_name, find_all_by_name)
- Edge cases: empty arena, invalid IDs, concurrent operations

#### `flow_tests.rs`
- FlowNodeId NONE behavior
- FlowNode creation with flag combinations
- FlowNodeArena allocation and retrieval
- Flag operations (UNREACHABLE, START, BRANCH_LABEL, etc.)
- find_unreachable() method
- Antecedent tracking
- Edge cases: empty arena, circular flow

#### `scopes_tests.rs`
- ScopeId NONE behavior
- Scope creation with ContainerKind variants
- ScopeContext creation and parent tracking
- is_function_scope() for different container kinds
- Hoisted vars/functions tracking

### 2. Lowering Tests

**Extend**: `crates/tsz-lowering/tests/lower_tests.rs`

#### New Test Categories

**Type Parameter Scoping**
- Nested generics: `Map<string, Map<number, boolean>>`
- Shadowed type params in nested scopes
- Forward references to type params
- Type param defaults

**Advanced Type Lowering**
- Mapped types: `{ [K in keyof T]: T[K] }`
- Conditional types with infer: `T extends infer U ? U : never`
- Template literal types
- Index access types
- Keyof types

**Error Recovery**
- Invalid type nodes
- Missing type info
- Circular type references
- Operation limit exceeded

**Interface Merging**
- Multiple declarations with same name
- Property conflicts
- Index signature merging
- Call/construct signature merging

### 3. Scanner Tests

**Extend**: `crates/tsz-scanner/tests/`

#### New Test Categories

**Regex Edge Cases**
- Flag combinations: g, i, m, s, u, v
- Invalid flag characters
- Duplicate flag detection
- Incompatible flags (u and v together)
- Regex with unicode escapes

**Unicode Escapes**
- `\u{...}` extended unicode
- `\uXXXX` basic escapes
- Surrogate pairs
- Invalid escape sequences

**Numeric Literals**
- BigInt suffix: `123n`
- Numeric separators: `1_000_000`
- Leading zeros (error cases)
- Hex: `0xFF`, octal: `0o77`, binary: `0b1010`
- Exponential notation

**Error Recovery**
- Unterminated strings
- Unterminated template literals
- Conflict markers
- Invalid characters

### 4. CLI Tests

**New Files**: `crates/tsz-cli/tests/`

#### `watch_tests.rs`
- File change detection
- Debouncing behavior
- Add/remove/update file handling
- Error handling for inaccessible files

#### `incremental_tests.rs` (extend existing inline tests)
- Incremental rebuild triggers
- Cache invalidation on type changes
- Dependency graph updates
- Clean build vs incremental

## Testing Strategy

### Test Organization
- Unit tests in separate `tests/` directories
- Follow existing naming: `*_tests.rs`
- Use existing test utilities and fixtures

### Test Patterns
- **Arrange-Act-Assert** structure
- Descriptive test names: `test_<module>_<scenario>_<expected>`
- Edge case coverage: empty inputs, boundary values, error paths
- Property-based testing where applicable

### Success Criteria
- All new tests pass
- No regressions in existing tests
- Code coverage increases in target modules
- Tests are maintainable and well-documented

## Implementation Order

1. **Binder tests** (highest impact, most under-tested)
   - symbols_tests.rs
   - flow_tests.rs
   - scopes_tests.rs

2. **Lowering tests** (extend existing)
   - Type parameter scoping
   - Advanced lowering
   - Error recovery

3. **Scanner tests** (extend existing)
   - Regex edge cases
   - Unicode escapes
   - Numeric literals

4. **CLI tests** (new coverage)
   - watch_tests.rs
   - Incremental tests extension

## Estimated Scope

| Crate | New Test Files | New Tests (est.) |
|-------|----------------|------------------|
| tsz-binder | 3 | 60-80 |
| tsz-lowering | 0 (extend) | 20-30 |
| tsz-scanner | 0 (extend) | 15-20 |
| tsz-cli | 1-2 | 15-25 |
| **Total** | 4-5 | 110-155 |
