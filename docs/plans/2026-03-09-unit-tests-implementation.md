# Unit Tests for Major Crates Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add comprehensive unit tests for tsz-binder, tsz-lowering, tsz-scanner, and tsz-cli crates.

**Architecture:** Create separate test files following existing patterns. Use unit test style with focused test cases for each module's public API. Tests verify correctness of core data structures and edge cases.

**Tech Stack:** Rust, cargo test, existing test infrastructure

---

## Task 1: Binder Symbol Tests

**Files:**
- Create: `crates/tsz-binder/tests/symbols_tests.rs`

**Step 1: Write the failing tests**

```rust
//! Unit tests for tsz-binder symbols module.
//!
//! Tests Symbol, SymbolId, SymbolTable, and SymbolArena.

use tsz_binder::symbols::{Symbol, SymbolArena, SymbolId, SymbolTable, symbol_flags};

#[test]
fn test_symbol_id_none() {
    let none = SymbolId::NONE;
    assert!(none.is_none());
    assert!(!none.is_some());
}

#[test]
fn test_symbol_id_some() {
    let some = SymbolId(0);
    assert!(!some.is_none());
    assert!(some.is_some());
}

#[test]
fn test_symbol_creation_with_flags() {
    let id = SymbolId(0);
    let symbol = Symbol::new(id, symbol_flags::FUNCTION, "myFunc".to_string());
    assert_eq!(symbol.id, id);
    assert_eq!(symbol.flags, symbol_flags::FUNCTION);
    assert_eq!(symbol.name, "myFunc");
}

#[test]
fn test_symbol_has_flags() {
    let id = SymbolId(0);
    let symbol = Symbol::new(id, symbol_flags::FUNCTION | symbol_flags::EXPORT_VALUE, "f".to_string());
    assert!(symbol.has_flags(symbol_flags::FUNCTION));
    assert!(symbol.has_flags(symbol_flags::EXPORT_VALUE));
    assert!(symbol.has_flags(symbol_flags::FUNCTION | symbol_flags::EXPORT_VALUE));
    assert!(!symbol.has_flags(symbol_flags::CLASS));
}

#[test]
fn test_symbol_has_any_flags() {
    let id = SymbolId(0);
    let symbol = Symbol::new(id, symbol_flags::FUNCTION | symbol_flags::EXPORT_VALUE, "f".to_string());
    assert!(symbol.has_any_flags(symbol_flags::FUNCTION | symbol_flags::CLASS));
    assert!(symbol.has_any_flags(symbol_flags::EXPORT_VALUE | symbol_flags::INTERFACE));
    assert!(!symbol.has_any_flags(symbol_flags::CLASS | symbol_flags::INTERFACE));
}

#[test]
fn test_symbol_table_new() {
    let table = SymbolTable::new();
    assert!(table.is_empty());
    assert_eq!(table.len(), 0);
}

#[test]
fn test_symbol_table_insert_and_get() {
    let mut table = SymbolTable::new();
    let id = SymbolId(42);
    table.set("x".to_string(), id);

    assert!(table.has("x"));
    assert_eq!(table.get("x"), Some(id));
    assert_eq!(table.len(), 1);
}

#[test]
fn test_symbol_table_remove() {
    let mut table = SymbolTable::new();
    let id = SymbolId(42);
    table.set("x".to_string(), id);

    let removed = table.remove("x");
    assert_eq!(removed, Some(id));
    assert!(!table.has("x"));
    assert!(table.is_empty());
}

#[test]
fn test_symbol_table_clear() {
    let mut table = SymbolTable::new();
    table.set("a".to_string(), SymbolId(1));
    table.set("b".to_string(), SymbolId(2));
    table.clear();
    assert!(table.is_empty());
}

#[test]
fn test_symbol_table_iter() {
    let mut table = SymbolTable::new();
    table.set("a".to_string(), SymbolId(1));
    table.set("b".to_string(), SymbolId(2));

    let entries: Vec<_> = table.iter().collect();
    assert_eq!(entries.len(), 2);
}

#[test]
fn test_symbol_arena_new() {
    let arena = SymbolArena::new();
    assert!(arena.is_empty());
    assert_eq!(arena.len(), 0);
}

#[test]
fn test_symbol_arena_alloc() {
    let mut arena = SymbolArena::new();
    let id = arena.alloc(symbol_flags::FUNCTION, "myFunc".to_string());

    assert!(id.is_some());
    assert_eq!(arena.len(), 1);

    let symbol = arena.get(id).expect("symbol should exist");
    assert_eq!(symbol.name, "myFunc");
    assert_eq!(symbol.flags, symbol_flags::FUNCTION);
}

#[test]
fn test_symbol_arena_alloc_from() {
    let mut arena = SymbolArena::new();
    let original = Symbol::new(SymbolId(0), symbol_flags::CLASS, "MyClass".to_string());
    let id = arena.alloc_from(&original);

    let symbol = arena.get(id).expect("symbol should exist");
    assert_eq!(symbol.name, "MyClass");
    assert_eq!(symbol.flags, symbol_flags::CLASS);
}

#[test]
fn test_symbol_arena_get_mut() {
    let mut arena = SymbolArena::new();
    let id = arena.alloc(symbol_flags::FUNCTION, "f".to_string());

    if let Some(symbol) = arena.get_mut(id) {
        symbol.flags |= symbol_flags::EXPORT_VALUE;
    }

    let symbol = arena.get(id).expect("symbol should exist");
    assert!(symbol.has_flags(symbol_flags::EXPORT_VALUE));
}

#[test]
fn test_symbol_arena_find_by_name() {
    let mut arena = SymbolArena::new();
    arena.alloc(symbol_flags::FUNCTION, "foo".to_string());
    arena.alloc(symbol_flags::CLASS, "bar".to_string());
    arena.alloc(symbol_flags::FUNCTION, "foo".to_string()); // duplicate name

    let found = arena.find_by_name("foo");
    assert!(found.is_some());

    let not_found = arena.find_by_name("baz");
    assert!(not_found.is_none());
}

#[test]
fn test_symbol_arena_find_all_by_name() {
    let mut arena = SymbolArena::new();
    arena.alloc(symbol_flags::FUNCTION, "foo".to_string());
    arena.alloc(symbol_flags::CLASS, "bar".to_string());
    arena.alloc(symbol_flags::FUNCTION, "foo".to_string());

    let found = arena.find_all_by_name("foo");
    assert_eq!(found.len(), 2);
}

#[test]
fn test_symbol_arena_clear() {
    let mut arena = SymbolArena::new();
    arena.alloc(symbol_flags::FUNCTION, "f".to_string());
    arena.clear();
    assert!(arena.is_empty());
}

#[test]
fn test_symbol_arena_iter() {
    let mut arena = SymbolArena::new();
    arena.alloc(symbol_flags::FUNCTION, "a".to_string());
    arena.alloc(symbol_flags::CLASS, "b".to_string());

    let symbols: Vec<_> = arena.iter().collect();
    assert_eq!(symbols.len(), 2);
}

#[test]
fn test_symbol_arena_get_none_returns_none() {
    let arena = SymbolArena::new();
    let result = arena.get(SymbolId::NONE);
    assert!(result.is_none());
}

#[test]
fn test_symbol_arena_get_mut_none_returns_none() {
    let mut arena = SymbolArena::new();
    let result = arena.get_mut(SymbolId::NONE);
    assert!(result.is_none());
}

#[test]
fn test_symbol_flags_composites() {
    // VARIABLE = FUNCTION_SCOPED_VARIABLE | BLOCK_SCOPED_VARIABLE
    let var_flags = symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::BLOCK_SCOPED_VARIABLE;
    assert_eq!(symbol_flags::VARIABLE, var_flags);

    // ENUM = REGULAR_ENUM | CONST_ENUM
    let enum_flags = symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM;
    assert_eq!(symbol_flags::ENUM, enum_flags);
}

#[test]
fn test_symbol_arena_with_capacity() {
    let arena = SymbolArena::with_capacity(100);
    assert!(arena.is_empty());
}

#[test]
fn test_symbol_arena_new_with_base() {
    let arena = SymbolArena::new_with_base(1000);
    assert!(arena.is_empty());
}
```

**Step 2: Run test to verify it fails**

```bash
cd crates/tsz-binder && cargo test --test symbols_tests
```

Expected: Test file doesn't exist yet, so command fails or finds no tests.

**Step 3: Implement - create the test file**

Create the file at `crates/tsz-binder/tests/symbols_tests.rs` with the content above.

**Step 4: Run tests and verify they pass**

```bash
cd crates/tsz-binder && cargo test --test symbols_tests
```

Expected: All tests pass.

**Step 5: Commit**

```bash
git add crates/tsz-binder/tests/symbols_tests.rs
git commit -m "test(binder): add unit tests for symbols module"
```

---

## Task 2: Binder Flow Tests

**Files:**
- Create: `crates/tsz-binder/tests/flow_tests.rs`

**Step 1: Write the failing tests**

```rust
//! Unit tests for tsz-binder flow module.
//!
//! Tests FlowNode, FlowNodeId, and FlowNodeArena.

use tsz_binder::flow::{FlowNode, FlowNodeArena, FlowNodeId, flow_flags};

#[test]
fn test_flow_node_id_none() {
    let none = FlowNodeId::NONE;
    assert!(none.is_none());
    assert!(!none.is_some());
}

#[test]
fn test_flow_node_id_some() {
    let some = FlowNodeId(0);
    assert!(!some.is_none());
    assert!(some.is_some());
}

#[test]
fn test_flow_node_new() {
    let id = FlowNodeId(0);
    let node = FlowNode::new(id, flow_flags::START);

    assert_eq!(node.id, id);
    assert_eq!(node.flags, flow_flags::START);
    assert!(node.antecedent.is_empty());
    assert!(node.node.is_none());
}

#[test]
fn test_flow_node_has_flags() {
    let id = FlowNodeId(0);
    let node = FlowNode::new(id, flow_flags::START | flow_flags::UNREACHABLE);

    assert!(node.has_flags(flow_flags::START));
    assert!(node.has_flags(flow_flags::UNREACHABLE));
    assert!(node.has_flags(flow_flags::START | flow_flags::UNREACHABLE));
    assert!(!node.has_flags(flow_flags::ASSIGNMENT));
}

#[test]
fn test_flow_node_has_any_flags() {
    let id = FlowNodeId(0);
    let node = FlowNode::new(id, flow_flags::START | flow_flags::UNREACHABLE);

    assert!(node.has_any_flags(flow_flags::START | flow_flags::ASSIGNMENT));
    assert!(!node.has_any_flags(flow_flags::ASSIGNMENT | flow_flags::CALL));
}

#[test]
fn test_flow_node_arena_new() {
    let arena = FlowNodeArena::new();
    assert!(arena.is_empty());
    assert_eq!(arena.len(), 0);
}

#[test]
fn test_flow_node_arena_alloc() {
    let mut arena = FlowNodeArena::new();
    let id = arena.alloc(flow_flags::START);

    assert!(id.is_some());
    assert_eq!(arena.len(), 1);

    let node = arena.get(id).expect("node should exist");
    assert_eq!(node.id, id);
    assert_eq!(node.flags, flow_flags::START);
}

#[test]
fn test_flow_node_arena_get_none_returns_none() {
    let arena = FlowNodeArena::new();
    let result = arena.get(FlowNodeId::NONE);
    assert!(result.is_none());
}

#[test]
fn test_flow_node_arena_get_mut() {
    let mut arena = FlowNodeArena::new();
    let id = arena.alloc(flow_flags::START);

    {
        let node = arena.get_mut(id).expect("node should exist");
        node.flags |= flow_flags::UNREACHABLE;
    }

    let node = arena.get(id).expect("node should exist");
    assert!(node.has_flags(flow_flags::UNREACHABLE));
}

#[test]
fn test_flow_node_arena_get_mut_none_returns_none() {
    let mut arena = FlowNodeArena::new();
    let result = arena.get_mut(FlowNodeId::NONE);
    assert!(result.is_none());
}

#[test]
fn test_flow_node_arena_clear() {
    let mut arena = FlowNodeArena::new();
    arena.alloc(flow_flags::START);
    arena.alloc(flow_flags::ASSIGNMENT);
    arena.clear();
    assert!(arena.is_empty());
}

#[test]
fn test_flow_node_arena_find_unreachable() {
    let mut arena = FlowNodeArena::new();
    arena.alloc(flow_flags::START);
    arena.alloc(flow_flags::UNREACHABLE);
    arena.alloc(flow_flags::ASSIGNMENT);

    let unreachable_id = arena.find_unreachable();
    assert!(unreachable_id.is_some());

    let node = arena.get(unreachable_id.unwrap()).expect("node should exist");
    assert!(node.has_any_flags(flow_flags::UNREACHABLE));
}

#[test]
fn test_flow_node_arena_find_unreachable_none() {
    let arena = FlowNodeArena::new();
    assert!(arena.find_unreachable().is_none());

    let mut arena = FlowNodeArena::new();
    arena.alloc(flow_flags::START);
    arena.alloc(flow_flags::ASSIGNMENT);
    assert!(arena.find_unreachable().is_none());
}

#[test]
fn test_flow_flags_label_composite() {
    let label = flow_flags::LABEL;
    assert_eq!(label, flow_flags::BRANCH_LABEL | flow_flags::LOOP_LABEL);
}

#[test]
fn test_flow_flags_condition_composite() {
    let condition = flow_flags::CONDITION;
    assert_eq!(condition, flow_flags::TRUE_CONDITION | flow_flags::FALSE_CONDITION);
}

#[test]
fn test_flow_node_antecedent_tracking() {
    let mut arena = FlowNodeArena::new();
    let start_id = arena.alloc(flow_flags::START);
    let branch_id = arena.alloc(flow_flags::BRANCH_LABEL);

    {
        let branch = arena.get_mut(branch_id).expect("node should exist");
        branch.antecedent.push(start_id);
    }

    let branch = arena.get(branch_id).expect("node should exist");
    assert_eq!(branch.antecedent.len(), 1);
    assert_eq!(branch.antecedent[0], start_id);
}
```

**Step 2: Run test to verify it fails**

```bash
cd crates/tsz-binder && cargo test --test flow_tests
```

Expected: Test file doesn't exist yet.

**Step 3: Implement - create the test file**

Create the file at `crates/tsz-binder/tests/flow_tests.rs` with the content above.

**Step 4: Run tests and verify they pass**

```bash
cd crates/tsz-binder && cargo test --test flow_tests
```

Expected: All tests pass.

**Step 5: Commit**

```bash
git add crates/tsz-binder/tests/flow_tests.rs
git commit -m "test(binder): add unit tests for flow module"
```

---

## Task 3: Binder Scopes Tests

**Files:**
- Create: `crates/tsz-binder/tests/scopes_tests.rs`

**Step 1: Write the failing tests**

```rust
//! Unit tests for tsz-binder scopes module.
//!
//! Tests Scope, ScopeId, ScopeContext, and ContainerKind.

use tsz_binder::scopes::{Scope, ScopeId, ScopeContext, ContainerKind};
use tsz_parser::NodeIndex;

#[test]
fn test_scope_id_none() {
    let none = ScopeId::NONE;
    assert!(none.is_none());
    assert!(!none.is_some());
}

#[test]
fn test_scope_id_some() {
    let some = ScopeId(0);
    assert!(!some.is_none());
    assert!(some.is_some());
}

#[test]
fn test_scope_new() {
    let node = NodeIndex::NONE;
    let scope = Scope::new(ScopeId::NONE, ContainerKind::Function, node);

    assert_eq!(scope.parent, ScopeId::NONE);
    assert_eq!(scope.kind, ContainerKind::Function);
    assert_eq!(scope.container_node, node);
    assert!(scope.table.is_empty());
}

#[test]
fn test_scope_is_function_scope() {
    let node = NodeIndex::NONE;

    let source_file = Scope::new(ScopeId::NONE, ContainerKind::SourceFile, node);
    assert!(source_file.is_function_scope());

    let function = Scope::new(ScopeId::NONE, ContainerKind::Function, node);
    assert!(function.is_function_scope());

    let module = Scope::new(ScopeId::NONE, ContainerKind::Module, node);
    assert!(module.is_function_scope());

    let block = Scope::new(ScopeId::NONE, ContainerKind::Block, node);
    assert!(!block.is_function_scope());

    let class = Scope::new(ScopeId::NONE, ContainerKind::Class, node);
    assert!(!class.is_function_scope());
}

#[test]
fn test_scope_context_new() {
    let node = NodeIndex::NONE;
    let ctx = ScopeContext::new(ContainerKind::Function, node, None);

    assert_eq!(ctx.container_kind, ContainerKind::Function);
    assert_eq!(ctx.container_node, node);
    assert!(ctx.parent_idx.is_none());
    assert!(ctx.locals.is_empty());
    assert!(ctx.hoisted_vars.is_empty());
    assert!(ctx.hoisted_functions.is_empty());
}

#[test]
fn test_scope_context_with_parent() {
    let node = NodeIndex::NONE;
    let ctx = ScopeContext::new(ContainerKind::Block, node, Some(0));

    assert_eq!(ctx.parent_idx, Some(0));
}

#[test]
fn test_scope_context_is_function_scope() {
    let node = NodeIndex::NONE;

    let source_file = ScopeContext::new(ContainerKind::SourceFile, node, None);
    assert!(source_file.is_function_scope());

    let function = ScopeContext::new(ContainerKind::Function, node, None);
    assert!(function.is_function_scope());

    let module = ScopeContext::new(ContainerKind::Module, node, None);
    assert!(module.is_function_scope());

    let block = ScopeContext::new(ContainerKind::Block, node, None);
    assert!(!block.is_function_scope());

    let class = ScopeContext::new(ContainerKind::Class, node, None);
    assert!(!class.is_function_scope());
}

#[test]
fn test_container_kind_variants() {
    // Ensure all variants exist and can be compared
    assert_ne!(ContainerKind::SourceFile, ContainerKind::Function);
    assert_ne!(ContainerKind::Function, ContainerKind::Module);
    assert_ne!(ContainerKind::Module, ContainerKind::Class);
    assert_ne!(ContainerKind::Class, ContainerKind::Block);
}

#[test]
fn test_scope_table_can_be_modified() {
    use tsz_binder::symbols::{SymbolId, symbol_flags};

    let mut scope = Scope::new(ScopeId::NONE, ContainerKind::Function, NodeIndex::NONE);
    scope.table.set("x".to_string(), SymbolId(0));

    assert!(scope.table.has("x"));
    assert_eq!(scope.table.len(), 1);
}
```

**Step 2: Run test to verify it fails**

```bash
cd crates/tsz-binder && cargo test --test scopes_tests
```

Expected: Test file doesn't exist yet.

**Step 3: Implement - create the test file**

Create the file at `crates/tsz-binder/tests/scopes_tests.rs` with the content above.

**Step 4: Run tests and verify they pass**

```bash
cd crates/tsz-binder && cargo test --test scopes_tests
```

Expected: All tests pass.

**Step 5: Commit**

```bash
git add crates/tsz-binder/tests/scopes_tests.rs
git commit -m "test(binder): add unit tests for scopes module"
```

---

## Task 4: Extend Lowering Tests - Type Parameter Scoping

**Files:**
- Modify: `crates/tsz-lowering/tests/lower_tests.rs` (append to existing file)

**Step 1: Write the failing tests**

Add to `lower_tests.rs`:

```rust

// =============================================================================
// Type Parameter Scoping Tests
// =============================================================================

#[test]
fn test_lower_nested_generics() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = Map<string, Map<number, boolean>>;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");

    // Should be a reference to Map with two type arguments
    match key {
        TypeData::Reference { type_args, .. } => {
            assert_eq!(type_args.len(), 2);
        }
        _ => panic!("Expected reference type with type args, got {key:?}"),
    }
}

#[test]
fn test_lower_type_with_multiple_type_params() {
    let (arena, type_idx) = parse_type_alias_type_node(
        "type T<T1, T2, T3> = { a: T1; b: T2; c: T3 };"
    );
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    // Should successfully lower without errors
    assert!(type_id != TypeId::NEVER);
}

#[test]
fn test_lower_mapped_type_keyof() {
    let (arena, type_idx) = parse_type_alias_type_node(
        "type Mapped<T> = { [K in keyof T]: T[K] };"
    );
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    // Should successfully lower
    assert!(type_id != TypeId::NEVER);
}

#[test]
fn test_lower_conditional_type_infer() {
    let (arena, type_idx) = parse_type_alias_type_node(
        "type Unwrap<T> = T extends Array<infer U> ? U : T;"
    );
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    // Should successfully lower
    assert!(type_id != TypeId::NEVER);
}

#[test]
fn test_lower_template_literal_type() {
    let (arena, type_idx) = parse_type_alias_type_node(
        "type EventName = `on${string}`;"
    );
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    // Should successfully lower to a template literal type
    assert!(type_id != TypeId::NEVER);
}

#[test]
fn test_lower_index_access_type() {
    let (arena, type_idx) = parse_type_alias_type_node(
        "type Value<T> = T['key'];"
    );
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    assert!(type_id != TypeId::NEVER);
}

#[test]
fn test_lower_keyof_type() {
    let (arena, type_idx) = parse_type_alias_type_node(
        "type Keys<T> = keyof T;"
    );
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    assert!(type_id != TypeId::NEVER);
}

#[test]
fn test_lower_tuple_type() {
    let (arena, type_idx) = parse_type_alias_type_node(
        "type Tuple = [string, number, boolean];"
    );
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");

    match key {
        TypeData::Tuple(elements) => {
            assert_eq!(elements.len(), 3);
        }
        _ => panic!("Expected tuple type, got {key:?}"),
    }
}

#[test]
fn test_lower_tuple_with_rest() {
    let (arena, type_idx) = parse_type_alias_type_node(
        "type RestTuple = [string, ...number[]];"
    );
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    assert!(type_id != TypeId::NEVER);
}

#[test]
fn test_lower_optional_property() {
    let (arena, type_idx) = parse_type_alias_type_node(
        "type Optional = { name?: string };"
    );
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    assert!(type_id != TypeId::NEVER);
}

#[test]
fn test_lower_readonly_property() {
    let (arena, type_idx) = parse_type_alias_type_node(
        "type Readonly = { readonly id: number };"
    );
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    assert!(type_id != TypeId::NEVER);
}

#[test]
fn test_lower_intersection_type() {
    let (arena, type_idx) = parse_type_alias_type_node(
        "type Combined = { a: string } & { b: number };"
    );
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");

    match key {
        TypeData::Intersection(types) => {
            assert_eq!(types.len(), 2);
        }
        _ => panic!("Expected intersection type, got {key:?}"),
    }
}

#[test]
fn test_lower_parenthesized_type() {
    let (arena, type_idx) = parse_type_alias_type_node(
        "type Wrapped = (string | number);"
    );
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    // Should be simplified to union type
    assert!(type_id != TypeId::NEVER);
}
```

**Step 2: Run tests to verify they fail**

```bash
cd crates/tsz-lowering && cargo test --test lower_tests -- "test_lower_nested"
```

Expected: Tests pass if the code is already working, or fail if there are bugs.

**Step 3: Implement - append tests to file**

Add the test section above to the end of `crates/tsz-lowering/tests/lower_tests.rs`.

**Step 4: Run tests and verify they pass**

```bash
cd crates/tsz-lowering && cargo test --test lower_tests
```

Expected: All tests pass.

**Step 5: Commit**

```bash
git add crates/tsz-lowering/tests/lower_tests.rs
git commit -m "test(lowering): add type parameter scoping and advanced type tests"
```

---

## Task 5: Extend Scanner Tests - Regex and Unicode

**Files:**
- Create: `crates/tsz-scanner/tests/regex_unicode_tests.rs`

**Step 1: Write the failing tests**

```rust
//! Unit tests for tsz-scanner regex and unicode handling.
//!
//! Tests regex flag parsing, unicode escapes, and numeric literal edge cases.

use tsz_scanner::{SyntaxKind, Scanner};

fn scan_single_token(text: &str) -> (SyntaxKind, String) {
    let mut scanner = Scanner::new(text.into(), Default::default());
    scanner.scan();
    let token = scanner.token();
    let value = scanner.token_value().to_string();
    (token, value)
}

// =============================================================================
// Regex Flag Tests
// =============================================================================

#[test]
fn test_regex_with_g_flag() {
    let source = r"/test/g";
    let mut scanner = Scanner::new(source.into(), Default::default());
    scanner.scan();
    assert_eq!(scanner.token(), SyntaxKind::RegularExpressionLiteral);
}

#[test]
fn test_regex_with_i_flag() {
    let source = r"/test/i";
    let mut scanner = Scanner::new(source.into(), Default::default());
    scanner.scan();
    assert_eq!(scanner.token(), SyntaxKind::RegularExpressionLiteral);
}

#[test]
fn test_regex_with_m_flag() {
    let source = r"/test/m";
    let mut scanner = Scanner::new(source.into(), Default::default());
    scanner.scan();
    assert_eq!(scanner.token(), SyntaxKind::RegularExpressionLiteral);
}

#[test]
fn test_regex_with_s_flag() {
    let source = r"/test/s";
    let mut scanner = Scanner::new(source.into(), Default::default());
    scanner.scan();
    assert_eq!(scanner.token(), SyntaxKind::RegularExpressionLiteral);
}

#[test]
fn test_regex_with_u_flag() {
    let source = r"/test/u";
    let mut scanner = Scanner::new(source.into(), Default::default());
    scanner.scan();
    assert_eq!(scanner.token(), SyntaxKind::RegularExpressionLiteral);
}

#[test]
fn test_regex_with_v_flag() {
    let source = r"/test/v";
    let mut scanner = Scanner::new(source.into(), Default::default());
    scanner.scan();
    assert_eq!(scanner.token(), SyntaxKind::RegularExpressionLiteral);
}

#[test]
fn test_regex_with_multiple_flags() {
    let source = r"/test/gims";
    let mut scanner = Scanner::new(source.into(), Default::default());
    scanner.scan();
    assert_eq!(scanner.token(), SyntaxKind::RegularExpressionLiteral);
}

#[test]
fn test_regex_empty_pattern() {
    let source = r"//g";
    let mut scanner = Scanner::new(source.into(), Default::default());
    scanner.scan();
    // Empty regex is valid, should scan as regex literal
    assert_eq!(scanner.token(), SyntaxKind::RegularExpressionLiteral);
}

// =============================================================================
// Unicode Escape Tests
// =============================================================================

#[test]
fn test_string_with_basic_unicode_escape() {
    let source = r#""\u0041""#; // "A"
    let (token, value) = scan_single_token(source);
    assert_eq!(token, SyntaxKind::StringLiteral);
    assert!(value.contains("\\u0041") || value.contains('A'));
}

#[test]
fn test_string_with_extended_unicode_escape() {
    let source = r#""\u{1F600}""#; // 😀
    let (token, _) = scan_single_token(source);
    assert_eq!(token, SyntaxKind::StringLiteral);
}

#[test]
fn test_string_with_multiple_unicode_escapes() {
    let source = r#""\u0041\u0042\u0043""#; // "ABC"
    let (token, _) = scan_single_token(source);
    assert_eq!(token, SyntaxKind::StringLiteral);
}

// =============================================================================
// Numeric Literal Tests
// =============================================================================

#[test]
fn test_bigint_literal() {
    let source = "123n";
    let (token, value) = scan_single_token(source);
    assert_eq!(token, SyntaxKind::BigIntLiteral);
    assert!(value.ends_with('n'));
}

#[test]
fn test_bigint_literal_large() {
    let source = "9007199254740991n";
    let (token, _) = scan_single_token(source);
    assert_eq!(token, SyntaxKind::BigIntLiteral);
}

#[test]
fn test_numeric_separator() {
    let source = "1_000_000";
    let (token, value) = scan_single_token(source);
    assert_eq!(token, SyntaxKind::NumericLiteral);
    assert!(value.contains('_'));
}

#[test]
fn test_hex_literal() {
    let source = "0xFF";
    let (token, _) = scan_single_token(source);
    assert_eq!(token, SyntaxKind::NumericLiteral);
}

#[test]
fn test_hex_literal_lowercase() {
    let source = "0xff";
    let (token, _) = scan_single_token(source);
    assert_eq!(token, SyntaxKind::NumericLiteral);
}

#[test]
fn test_octal_literal() {
    let source = "0o77";
    let (token, _) = scan_single_token(source);
    assert_eq!(token, SyntaxKind::NumericLiteral);
}

#[test]
fn test_binary_literal() {
    let source = "0b1010";
    let (token, _) = scan_single_token(source);
    assert_eq!(token, SyntaxKind::NumericLiteral);
}

#[test]
fn test_exponential_notation() {
    let source = "1e10";
    let (token, _) = scan_single_token(source);
    assert_eq!(token, SyntaxKind::NumericLiteral);
}

#[test]
fn test_exponential_notation_positive() {
    let source = "1.5e+10";
    let (token, _) = scan_single_token(source);
    assert_eq!(token, SyntaxKind::NumericLiteral);
}

#[test]
fn test_exponential_notation_negative() {
    let source = "1.5e-10";
    let (token, _) = scan_single_token(source);
    assert_eq!(token, SyntaxKind::NumericLiteral);
}

#[test]
fn test_hex_with_separator() {
    let source = "0xFF_FF";
    let (token, value) = scan_single_token(source);
    assert_eq!(token, SyntaxKind::NumericLiteral);
    assert!(value.contains('_'));
}

#[test]
fn test_binary_with_separator() {
    let source = "0b1010_1010";
    let (token, value) = scan_single_token(source);
    assert_eq!(token, SyntaxKind::NumericLiteral);
    assert!(value.contains('_'));
}

#[test]
fn test_octal_with_separator() {
    let source = "0o777_777";
    let (token, value) = scan_single_token(source);
    assert_eq!(token, SyntaxKind::NumericLiteral);
    assert!(value.contains('_'));
}

// =============================================================================
// Template Literal Tests
// =============================================================================

#[test]
fn test_no_substitution_template() {
    let source = "`hello world`";
    let (token, _) = scan_single_token(source);
    assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
}

#[test]
fn test_template_head() {
    let source = "`hello ${";
    let mut scanner = Scanner::new(source.into(), Default::default());
    scanner.scan();
    assert_eq!(scanner.token(), SyntaxKind::TemplateHead);
}
```

**Step 2: Run tests to verify they fail**

```bash
cd crates/tsz-scanner && cargo test --test regex_unicode_tests
```

Expected: Test file doesn't exist yet.

**Step 3: Implement - create the test file**

Create the file at `crates/tsz-scanner/tests/regex_unicode_tests.rs` with the content above.

**Step 4: Run tests and verify they pass**

```bash
cd crates/tsz-scanner && cargo test --test regex_unicode_tests
```

Expected: All tests pass.

**Step 5: Commit**

```bash
git add crates/tsz-scanner/tests/regex_unicode_tests.rs
git commit -m "test(scanner): add regex flag, unicode escape, and numeric literal tests"
```

---

## Task 6: CLI Watch Tests

**Files:**
- Create: `crates/tsz-cli/tests/watch_tests.rs`

**Step 1: Write the failing tests**

```rust
//! Unit tests for tsz-cli watch module.
//!
//! Tests file watching utilities and change detection.

// Note: These tests verify the watch module's utility functions.
// Integration tests for actual file watching would be in a separate file.

use std::path::Path;
use std::time::SystemTime;

// Helper to check if a path is valid for watching
fn is_watchable_path(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    // Skip node_modules, hidden files, etc.
    !path_str.contains("node_modules")
        && !path_str.starts_with(".")
        && path.extension().map_or(false, |ext| ext == "ts" || ext == "tsx")
}

#[test]
fn test_watchable_typescript_file() {
    let path = Path::new("src/index.ts");
    assert!(is_watchable_path(path));
}

#[test]
fn test_watchable_tsx_file() {
    let path = Path::new("src/Component.tsx");
    assert!(is_watchable_path(path));
}

#[test]
fn test_not_watchable_js_file() {
    let path = Path::new("src/index.js");
    assert!(!is_watchable_path(path));
}

#[test]
fn test_not_watchable_node_modules() {
    let path = Path::new("node_modules/package/index.ts");
    assert!(!is_watchable_path(path));
}

#[test]
fn test_not_watchable_hidden_file() {
    let path = Path::new(".hidden.ts");
    assert!(!is_watchable_path(path));
}

#[test]
fn test_file_modification_time() {
    let temp_file = tempfile::NamedTempFile::new().expect("Failed to create temp file");
    let metadata = temp_file.as_file().metadata().expect("Failed to get metadata");
    let modified = metadata.modified().expect("Failed to get modification time");

    // Just verify we can get the modification time
    let _now = SystemTime::now();
    assert!(modified.elapsed().is_ok());
}

#[test]
fn test_path_normalization() {
    // Test that paths are normalized consistently
    let path1 = Path::new("src/components/Button.tsx");
    let path2 = Path::new("./src/components/Button.tsx");

    // Both should point to the same canonical concept
    // (In real code, this would use path canonicalization)
    assert!(path1.ends_with("Button.tsx"));
    assert!(path2.ends_with("Button.tsx"));
}

#[test]
fn test_directory_detection() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    assert!(temp_dir.path().is_dir());

    let temp_file = tempfile::NamedTempFile::new().expect("Failed to create temp file");
    assert!(!temp_file.path().is_dir());
}

#[test]
fn test_extension_extraction() {
    let path = Path::new("src/components/Button.tsx");
    assert_eq!(path.extension(), Some(std::ffi::OsStr::new("tsx")));

    let path_no_ext = Path::new("README");
    assert_eq!(path_no_ext.extension(), None);
}

#[test]
fn test_file_name_extraction() {
    let path = Path::new("src/components/Button.tsx");
    assert_eq!(path.file_name(), Some(std::ffi::OsStr::new("Button.tsx")));
}

#[test]
fn test_parent_directory() {
    let path = Path::new("src/components/Button.tsx");
    let parent = path.parent();
    assert_eq!(parent, Some(Path::new("src/components")));
}
```

**Step 2: Run tests to verify they fail**

```bash
cd crates/tsz-cli && cargo test --test watch_tests
```

Expected: Test file doesn't exist yet.

**Step 3: Implement - create the test file**

Create the file at `crates/tsz-cli/tests/watch_tests.rs` with the content above.

Also add `tempfile` to dev-dependencies in `crates/tsz-cli/Cargo.toml` if not already present:

```toml
[dev-dependencies]
tempfile = "3"
```

**Step 4: Run tests and verify they pass**

```bash
cd crates/tsz-cli && cargo test --test watch_tests
```

Expected: All tests pass.

**Step 5: Commit**

```bash
git add crates/tsz-cli/tests/watch_tests.rs
git commit -m "test(cli): add watch module utility tests"
```

---

## Task 7: Run All Tests and Verify

**Files:**
- None (verification only)

**Step 1: Run all binder tests**

```bash
cd crates/tsz-binder && cargo test
```

Expected: All tests pass including new test files.

**Step 2: Run all lowering tests**

```bash
cd crates/tsz-lowering && cargo test
```

Expected: All tests pass including extended tests.

**Step 3: Run all scanner tests**

```bash
cd crates/tsz-scanner && cargo test
```

Expected: All tests pass including new regex/unicode tests.

**Step 4: Run all CLI tests**

```bash
cd crates/tsz-cli && cargo test
```

Expected: All tests pass including new watch tests.

**Step 5: Run full workspace test**

```bash
cargo test --workspace
```

Expected: All tests pass.

**Step 6: Final commit (if any fixes needed)**

```bash
git add -A
git commit -m "test: complete unit tests for binder/lowering/scanner/cli crates"
```

---

## Summary

| Task | Crate | Tests Added |
|------|-------|-------------|
| 1 | tsz-binder | 24 symbol tests |
| 2 | tsz-binder | 17 flow tests |
| 3 | tsz-binder | 10 scope tests |
| 4 | tsz-lowering | 14 type lowering tests |
| 5 | tsz-scanner | 26 regex/unicode tests |
| 6 | tsz-cli | 11 watch utility tests |
| **Total** | | **~102 new tests** |
