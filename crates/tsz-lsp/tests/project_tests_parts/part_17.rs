#[test]
fn test_file_id_allocator_remove_returns_old_id() {
    use crate::project::FileIdAllocator;

    let mut alloc = FileIdAllocator::new();
    let id_a = alloc.get_or_allocate("a.ts");
    assert_eq!(alloc.remove("a.ts"), Some(id_a));
    assert_eq!(alloc.remove("a.ts"), None); // already removed
}

#[test]
fn test_file_id_allocator_reverse_lookup() {
    use crate::project::FileIdAllocator;

    let mut alloc = FileIdAllocator::new();
    let id_a = alloc.get_or_allocate("a.ts");
    let id_b = alloc.get_or_allocate("b.ts");

    // Forward lookup works.
    assert_eq!(alloc.lookup("a.ts"), Some(id_a));
    assert_eq!(alloc.lookup("b.ts"), Some(id_b));

    // Reverse lookup works.
    assert_eq!(alloc.name_for_id(id_a), Some("a.ts"));
    assert_eq!(alloc.name_for_id(id_b), Some("b.ts"));

    // Out-of-range returns None.
    assert_eq!(alloc.name_for_id(999), None);
}

#[test]
fn test_file_id_allocator_reverse_lookup_after_remove() {
    use crate::project::FileIdAllocator;

    let mut alloc = FileIdAllocator::new();
    let id_a = alloc.get_or_allocate("a.ts");
    let _id_b = alloc.get_or_allocate("b.ts");

    // Remove a.ts — reverse lookup should return None.
    alloc.remove("a.ts");
    assert_eq!(alloc.name_for_id(id_a), None);

    // Re-allocating "a.ts" gets a new ID; the old slot stays cleared.
    let id_a2 = alloc.get_or_allocate("a.ts");
    assert_ne!(id_a, id_a2);
    assert_eq!(alloc.name_for_id(id_a), None);
    assert_eq!(alloc.name_for_id(id_a2), Some("a.ts"));
}

#[test]
fn test_project_file_name_for_idx() {
    let mut project = crate::project::Project::new();
    project.set_file("src/foo.ts".to_string(), "export const x = 1;".to_string());
    project.set_file("src/bar.ts".to_string(), "export const y = 2;".to_string());

    // Look up file_idx for "src/foo.ts" via a symbol's decl_file_idx.
    let foo_file = &project.files["src/foo.ts"];
    let foo_sym = foo_file
        .binder()
        .symbols
        .iter()
        .find(|s| s.decl_file_idx != u32::MAX)
        .expect("expected at least one stamped symbol");
    let resolved = project.file_name_for_idx(foo_sym.decl_file_idx);
    assert_eq!(resolved, Some("src/foo.ts"));
}

// =============================================================================
// Binder file_idx stamping tests
// =============================================================================

#[test]
fn test_binder_stamps_file_idx_on_symbols() {
    use tsz_binder::BinderState;
    use tsz_parser::ParserState;

    let mut parser = ParserState::new("test.ts".to_string(), "const x = 1;".to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.set_file_idx(42);
    binder.bind_source_file(arena, root);

    // At least one symbol should have the stamped file_idx.
    let has_stamped = binder.symbols.iter().any(|sym| sym.decl_file_idx == 42);
    assert!(
        has_stamped,
        "Expected at least one symbol with decl_file_idx == 42"
    );

    // No non-lib symbol should have u32::MAX file_idx.
    let has_unassigned = binder
        .symbols
        .iter()
        .any(|sym| sym.decl_file_idx == u32::MAX);
    assert!(
        !has_unassigned,
        "No symbol should have u32::MAX file_idx after stamping"
    );
}

#[test]
fn test_binder_semantic_defs_use_file_idx() {
    use tsz_binder::BinderState;
    use tsz_parser::ParserState;

    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "interface Foo { x: number }".to_string(),
    );
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.set_file_idx(7);
    binder.bind_source_file(arena, root);

    // The semantic_defs entry for Foo should have file_id == 7.
    assert!(
        !binder.semantic_defs.is_empty(),
        "Expected at least one semantic def"
    );
    for entry in binder.semantic_defs.values() {
        assert_eq!(
            entry.file_id, 7,
            "SemanticDefEntry.file_id should match the binder's file_idx"
        );
    }
}

#[test]
fn test_binder_default_file_idx_is_max() {
    use tsz_binder::BinderState;
    use tsz_parser::ParserState;

    let mut parser = ParserState::new("test.ts".to_string(), "const x = 1;".to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    // Without calling set_file_idx, symbols should have u32::MAX (backward compat).
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    for sym in binder.symbols.iter() {
        assert_eq!(
            sym.decl_file_idx,
            u32::MAX,
            "Default file_idx should be u32::MAX when not set"
        );
    }
}

// =============================================================================
// Project + DefinitionStore invalidation integration tests
// =============================================================================

#[test]
fn test_project_set_file_assigns_file_idx() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const x = 1;".to_string());

    let file = &project.files["a.ts"];
    assert_ne!(
        file.file_idx,
        u32::MAX,
        "ProjectFile should have a valid file_idx after set_file"
    );
}

#[test]
fn test_project_set_file_preserves_file_idx_on_update() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const x = 1;".to_string());
    let first_idx = project.files["a.ts"].file_idx;

    // Update the same file with different content.
    project.set_file("a.ts".to_string(), "export const x = 2;".to_string());
    let second_idx = project.files["a.ts"].file_idx;

    assert_eq!(
        first_idx, second_idx,
        "File index should be stable across set_file updates"
    );
}

#[test]
fn test_project_remove_file_cleans_up_file_idx() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const x = 1;".to_string());

    // Verify file is tracked.
    assert!(project.file_id_allocator.lookup("a.ts").is_some());

    project.remove_file("a.ts");

    // After removal, the allocator should no longer track the file.
    assert!(project.file_id_allocator.lookup("a.ts").is_none());
}

