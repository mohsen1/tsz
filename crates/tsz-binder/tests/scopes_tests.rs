//! Unit tests for the scopes module.
//!
//! Tests `ScopeId`, Scope, and `ContainerKind`.

use tsz_binder::{ContainerKind, Scope, ScopeId};
use tsz_parser::NodeIndex;

// =============================================================================
// ScopeId Tests
// =============================================================================

#[test]
fn scope_id_none_is_none() {
    assert!(ScopeId::NONE.is_none());
    assert!(!ScopeId::NONE.is_some());
}

#[test]
fn scope_id_some_is_some() {
    let id = ScopeId(0);
    assert!(id.is_some());
    assert!(id.is_some());
}

#[test]
fn scope_id_arbitrary_values() {
    // Zero is valid
    let zero = ScopeId(0);
    assert!(zero.is_some());
    assert!(zero.is_some());

    // Mid-range values are valid
    let mid = ScopeId(1000);
    assert!(mid.is_some());
    assert!(mid.is_some());

    // Max - 1 is valid
    let almost_max = ScopeId(u32::MAX - 1);
    assert!(almost_max.is_some());
    assert!(almost_max.is_some());

    // Max (NONE sentinel) is not valid
    let none = ScopeId(u32::MAX);
    assert!(none.is_none());
    assert!(!none.is_some());
}

#[test]
fn scope_id_equality() {
    assert_eq!(ScopeId(0), ScopeId(0));
    assert_eq!(ScopeId(100), ScopeId(100));
    assert_eq!(ScopeId::NONE, ScopeId(u32::MAX));
    assert_ne!(ScopeId(0), ScopeId(1));
    assert_ne!(ScopeId(0), ScopeId::NONE);
}

#[test]
fn scope_id_clone_copy() {
    let id = ScopeId(42);
    let copied = id;
    let cloned = id;
    assert_eq!(id, copied);
    assert_eq!(id, cloned);
}

// =============================================================================
// ContainerKind Tests
// =============================================================================

#[test]
fn container_kind_variants() {
    // Ensure all variants exist and can be constructed
    let _ = ContainerKind::SourceFile;
    let _ = ContainerKind::Function;
    let _ = ContainerKind::Module;
    let _ = ContainerKind::Class;
    let _ = ContainerKind::Block;
}

#[test]
fn container_kind_equality() {
    assert_eq!(ContainerKind::SourceFile, ContainerKind::SourceFile);
    assert_eq!(ContainerKind::Function, ContainerKind::Function);
    assert_eq!(ContainerKind::Module, ContainerKind::Module);
    assert_eq!(ContainerKind::Class, ContainerKind::Class);
    assert_eq!(ContainerKind::Block, ContainerKind::Block);

    assert_ne!(ContainerKind::SourceFile, ContainerKind::Function);
    assert_ne!(ContainerKind::Function, ContainerKind::Block);
    assert_ne!(ContainerKind::Module, ContainerKind::Class);
}

#[test]
fn container_kind_clone_copy() {
    let kind = ContainerKind::Function;
    let copied = kind;
    let cloned = kind;
    assert_eq!(kind, copied);
    assert_eq!(kind, cloned);
}

// =============================================================================
// Scope Tests
// =============================================================================

#[test]
fn scope_new_creates_empty_table() {
    let scope = Scope::new(ScopeId::NONE, ContainerKind::SourceFile, NodeIndex::NONE);
    assert!(scope.table.is_empty());
}

#[test]
fn scope_new_sets_parent() {
    let parent = ScopeId(10);
    let scope = Scope::new(parent, ContainerKind::Function, NodeIndex::NONE);
    assert_eq!(scope.parent, parent);
}

#[test]
fn scope_new_sets_kind() {
    let scope = Scope::new(ScopeId::NONE, ContainerKind::Module, NodeIndex::NONE);
    assert_eq!(scope.kind, ContainerKind::Module);
}

#[test]
fn scope_new_sets_container_node() {
    let node = NodeIndex(42);
    let scope = Scope::new(ScopeId::NONE, ContainerKind::Block, node);
    assert_eq!(scope.container_node, node);
}

#[test]
fn scope_is_function_scope_source_file() {
    let scope = Scope::new(ScopeId::NONE, ContainerKind::SourceFile, NodeIndex::NONE);
    assert!(scope.is_function_scope());
}

#[test]
fn scope_is_function_scope_function() {
    let scope = Scope::new(ScopeId::NONE, ContainerKind::Function, NodeIndex::NONE);
    assert!(scope.is_function_scope());
}

#[test]
fn scope_is_function_scope_module() {
    let scope = Scope::new(ScopeId::NONE, ContainerKind::Module, NodeIndex::NONE);
    assert!(scope.is_function_scope());
}

#[test]
fn scope_is_function_scope_class() {
    let scope = Scope::new(ScopeId::NONE, ContainerKind::Class, NodeIndex::NONE);
    assert!(!scope.is_function_scope());
}

#[test]
fn scope_is_function_scope_block() {
    let scope = Scope::new(ScopeId::NONE, ContainerKind::Block, NodeIndex::NONE);
    assert!(!scope.is_function_scope());
}

#[test]
fn scope_table_can_add_symbols() {
    use tsz_binder::SymbolId;

    let mut scope = Scope::new(ScopeId::NONE, ContainerKind::Function, NodeIndex::NONE);
    assert!(scope.table.is_empty());

    scope.table.set("x".to_string(), SymbolId(1));
    assert_eq!(scope.table.len(), 1);
    assert_eq!(scope.table.get("x"), Some(SymbolId(1)));

    scope.table.set("y".to_string(), SymbolId(2));
    assert_eq!(scope.table.len(), 2);
    assert_eq!(scope.table.get("y"), Some(SymbolId(2)));
}

#[test]
fn scope_table_can_replace_symbols() {
    use tsz_binder::SymbolId;

    let mut scope = Scope::new(ScopeId::NONE, ContainerKind::Function, NodeIndex::NONE);

    scope.table.set("x".to_string(), SymbolId(1));
    assert_eq!(scope.table.get("x"), Some(SymbolId(1)));

    scope.table.set("x".to_string(), SymbolId(2));
    assert_eq!(scope.table.get("x"), Some(SymbolId(2)));
    assert_eq!(scope.table.len(), 1);
}

#[test]
fn scope_clone() {
    use tsz_binder::SymbolId;

    let mut scope = Scope::new(ScopeId(5), ContainerKind::Function, NodeIndex(10));
    scope.table.set("x".to_string(), SymbolId(1));

    let cloned = scope.clone();
    assert_eq!(cloned.parent, ScopeId(5));
    assert_eq!(cloned.kind, ContainerKind::Function);
    assert_eq!(cloned.container_node, NodeIndex(10));
    assert_eq!(cloned.table.get("x"), Some(SymbolId(1)));
}
