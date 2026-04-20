/// Test that multiple export * from statements are properly tracked.
/// This ensures we can resolve exports from files that re-export from multiple sources.
#[test]
fn test_multiple_wildcard_reexports() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    // This file re-exports from multiple modules
    let source = r#"
export * from './moduleA';
export * from './moduleB';
export * from './moduleC';
"#;

    let mut parser = ParserState::new("index.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.set_debug_file("index.ts");
    binder.bind_source_file(arena, root);

    // Verify that all wildcard re-exports are tracked
    let reexports = binder.wildcard_reexports.get("index.ts");
    assert!(
        reexports.is_some(),
        "index.ts should have wildcard_reexports"
    );

    let reexports = reexports.unwrap();
    assert_eq!(
        reexports.len(),
        3,
        "Should have 3 wildcard re-exports, got: {reexports:?}"
    );

    assert!(
        reexports.contains(&"./moduleA".to_string()),
        "Should re-export from ./moduleA"
    );
    assert!(
        reexports.contains(&"./moduleB".to_string()),
        "Should re-export from ./moduleB"
    );
    assert!(
        reexports.contains(&"./moduleC".to_string()),
        "Should re-export from ./moduleC"
    );
}

/// Test that named re-exports and wildcard re-exports can coexist.
#[test]
fn test_mixed_reexports() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
export { foo, bar } from './named';
export * from './wildcard1';
export { baz as qux } from './renamed';
export * from './wildcard2';
"#;

    let mut parser = ParserState::new("mixed.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.set_debug_file("mixed.ts");
    binder.bind_source_file(arena, root);

    // Check wildcard re-exports
    let wildcards = binder.wildcard_reexports.get("mixed.ts");
    assert!(
        wildcards.is_some(),
        "mixed.ts should have wildcard_reexports"
    );
    let wildcards = wildcards.unwrap();
    assert_eq!(wildcards.len(), 2, "Should have 2 wildcard re-exports");
    assert!(wildcards.contains(&"./wildcard1".to_string()));
    assert!(wildcards.contains(&"./wildcard2".to_string()));

    // Check named re-exports
    let named = binder.reexports.get("mixed.ts");
    assert!(named.is_some(), "mixed.ts should have named reexports");
    let named = named.unwrap();
    assert!(named.contains_key("foo"), "Should re-export 'foo'");
    assert!(named.contains_key("bar"), "Should re-export 'bar'");
    assert!(
        named.contains_key("qux"),
        "Should re-export 'qux' (renamed from baz)"
    );
}

/// Test that export resolution follows multiple wildcard re-export chains.
#[test]
fn test_export_resolution_multiple_wildcards() {
    use crate::binder::{BinderState, SymbolTable, symbol_flags};
    use crate::parser::ParserState;

    // Setup: We'll create module_exports and wildcard_reexports manually
    // to test the resolution logic without parsing multiple files
    let source = "const x = 1;"; // Minimal source just to create a binder

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Setup module_exports for two modules
    let mut module_a_exports = SymbolTable::new();
    let sym_a = binder
        .symbols
        .alloc(symbol_flags::FUNCTION, "funcA".to_string());
    module_a_exports.set("funcA".to_string(), sym_a);
    binder
        .module_exports
        .insert("./moduleA".to_string(), module_a_exports);

    let mut module_b_exports = SymbolTable::new();
    let sym_b = binder
        .symbols
        .alloc(symbol_flags::FUNCTION, "funcB".to_string());
    module_b_exports.set("funcB".to_string(), sym_b);
    binder
        .module_exports
        .insert("./moduleB".to_string(), module_b_exports);

    // Setup index.ts to re-export from both
    binder.wildcard_reexports.insert(
        "./index".to_string(),
        vec!["./moduleA".to_string(), "./moduleB".to_string()],
    );

    // Test resolution: funcA should be found via index -> moduleA
    let result = binder.resolve_import_if_needed_public("./index", "funcA");
    assert!(result.is_some(), "Should resolve funcA from index");
    assert_eq!(result.unwrap(), sym_a);

    // Test resolution: funcB should be found via index -> moduleB
    let result = binder.resolve_import_if_needed_public("./index", "funcB");
    assert!(result.is_some(), "Should resolve funcB from index");
    assert_eq!(result.unwrap(), sym_b);

    // Test resolution: nonExistent should NOT be found
    let result = binder.resolve_import_if_needed_public("./index", "nonExistent");
    assert!(result.is_none(), "Should not resolve nonExistent");
}

// =============================================================================
// Lib Symbol Merging Tests (SymbolId Collision Fix)
// =============================================================================

/// Regression test: Two lib files with overlapping local `SymbolIds` must resolve
/// to the correct global symbols after merge.
///
/// This tests the fix for the SymbolId collision bug where lib files had
/// overlapping indices (both starting from SymbolId(0)), causing incorrect
/// symbol lookups.
#[test]
fn test_lib_symbol_merge_avoids_id_collision() {
    use crate::binder::LibContext;
    use std::sync::Arc;

    // Create two "lib" binders with overlapping SymbolIds
    // Lib 1: has "Object" at SymbolId(0) and "Array" at SymbolId(1)
    let mut lib1_binder = BinderState::new();
    let lib1_object_id = lib1_binder
        .symbols
        .alloc(symbol_flags::INTERFACE, "Object".to_string());
    let lib1_array_id = lib1_binder
        .symbols
        .alloc(symbol_flags::INTERFACE, "Array".to_string());
    lib1_binder
        .file_locals
        .set("Object".to_string(), lib1_object_id);
    lib1_binder
        .file_locals
        .set("Array".to_string(), lib1_array_id);

    // Lib 2: has "Promise" at SymbolId(0) and "Map" at SymbolId(1)
    // These IDs intentionally overlap with lib1's IDs!
    let mut lib2_binder = BinderState::new();
    let lib2_promise_id = lib2_binder
        .symbols
        .alloc(symbol_flags::INTERFACE, "Promise".to_string());
    let lib2_map_id = lib2_binder
        .symbols
        .alloc(symbol_flags::INTERFACE, "Map".to_string());
    lib2_binder
        .file_locals
        .set("Promise".to_string(), lib2_promise_id);
    lib2_binder.file_locals.set("Map".to_string(), lib2_map_id);

    // Verify the collision exists (both start at 0)
    assert_eq!(lib1_object_id.0, 0, "lib1 Object should be at ID 0");
    assert_eq!(
        lib2_promise_id.0, 0,
        "lib2 Promise should be at ID 0 (collision!)"
    );

    // Create a dummy NodeArena (needed for LibContext)
    let dummy_arena = Arc::new(crate::parser::node::NodeArena::new());

    // Create lib contexts
    let lib_contexts = vec![
        LibContext {
            arena: Arc::clone(&dummy_arena),
            binder: Arc::new(lib1_binder),
        },
        LibContext {
            arena: Arc::clone(&dummy_arena),
            binder: Arc::new(lib2_binder),
        },
    ];

    // Create main binder and merge lib symbols
    let mut main_binder = BinderState::new();
    main_binder.merge_lib_contexts_into_binder(&lib_contexts);

    // The flag should be set
    assert!(
        main_binder.lib_symbols_are_merged(),
        "lib_symbols_merged flag should be true"
    );

    // All four symbols should be in file_locals with UNIQUE IDs
    let object_id = main_binder
        .file_locals
        .get("Object")
        .expect("Object should exist");
    let array_id = main_binder
        .file_locals
        .get("Array")
        .expect("Array should exist");
    let promise_id = main_binder
        .file_locals
        .get("Promise")
        .expect("Promise should exist");
    let map_id = main_binder
        .file_locals
        .get("Map")
        .expect("Map should exist");

    // All IDs must be unique (the fix)
    let ids = [object_id, array_id, promise_id, map_id];
    let unique_ids: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(
        unique_ids.len(),
        4,
        "All four symbols must have unique IDs after merge"
    );

    // Each symbol must resolve to correct data via get_symbol
    let object_sym = main_binder
        .get_symbol(object_id)
        .expect("Object symbol must resolve");
    assert_eq!(
        object_sym.escaped_name, "Object",
        "Object symbol name mismatch"
    );

    let promise_sym = main_binder
        .get_symbol(promise_id)
        .expect("Promise symbol must resolve");
    assert_eq!(
        promise_sym.escaped_name, "Promise",
        "Promise symbol name mismatch"
    );

    let array_sym = main_binder
        .get_symbol(array_id)
        .expect("Array symbol must resolve");
    assert_eq!(
        array_sym.escaped_name, "Array",
        "Array symbol name mismatch"
    );

    let map_sym = main_binder
        .get_symbol(map_id)
        .expect("Map symbol must resolve");
    assert_eq!(map_sym.escaped_name, "Map", "Map symbol name mismatch");
}

/// Test that symbol merging works correctly for interfaces with the same name
/// across lib files (declaration merging).
#[test]
fn test_lib_symbol_merge_declaration_merging() {
    use crate::binder::LibContext;
    use std::sync::Arc;

    // Create two lib binders, both declaring "Array" interface
    let mut lib1_binder = BinderState::new();
    let lib1_array_id = lib1_binder
        .symbols
        .alloc(symbol_flags::INTERFACE, "Array".to_string());
    lib1_binder
        .file_locals
        .set("Array".to_string(), lib1_array_id);

    let mut lib2_binder = BinderState::new();
    let lib2_array_id = lib2_binder
        .symbols
        .alloc(symbol_flags::INTERFACE, "Array".to_string());
    lib2_binder
        .file_locals
        .set("Array".to_string(), lib2_array_id);

    // Create lib contexts
    let dummy_arena = Arc::new(crate::parser::node::NodeArena::new());
    let lib_contexts = vec![
        LibContext {
            arena: Arc::clone(&dummy_arena),
            binder: Arc::new(lib1_binder),
        },
        LibContext {
            arena: Arc::clone(&dummy_arena),
            binder: Arc::new(lib2_binder),
        },
    ];

    // Merge
    let mut main_binder = BinderState::new();
    main_binder.merge_lib_contexts_into_binder(&lib_contexts);

    // Should have only ONE "Array" symbol (merged)
    let array_id = main_binder
        .file_locals
        .get("Array")
        .expect("Array should exist");
    let array_sym = main_binder
        .get_symbol(array_id)
        .expect("Array symbol must resolve");

    // The merged symbol should still be an interface
    assert!(
        (array_sym.flags & symbol_flags::INTERFACE) != 0,
        "Merged Array should be an interface"
    );
    assert_eq!(array_sym.escaped_name, "Array");
}

/// Test that user symbols take precedence over lib symbols.
#[test]
fn test_lib_symbol_merge_user_precedence() {
    use crate::binder::LibContext;
    use std::sync::Arc;

    // Create a lib binder with "myGlobal"
    let mut lib_binder = BinderState::new();
    let lib_sym_id = lib_binder
        .symbols
        .alloc(symbol_flags::VARIABLE, "myGlobal".to_string());
    lib_binder
        .file_locals
        .set("myGlobal".to_string(), lib_sym_id);

    let dummy_arena = Arc::new(crate::parser::node::NodeArena::new());
    let lib_contexts = vec![LibContext {
        arena: Arc::clone(&dummy_arena),
        binder: Arc::new(lib_binder),
    }];

    // Create main binder with a user-defined "myGlobal"
    let mut parser = ParserState::new("test.ts".to_string(), "const myGlobal = 42;".to_string());
    let root = parser.parse_source_file();
    let mut main_binder = BinderState::new();
    main_binder.bind_source_file(parser.get_arena(), root);

    // Capture user symbol ID
    let user_sym_id = main_binder
        .file_locals
        .get("myGlobal")
        .expect("User myGlobal should exist");

    // Now merge lib symbols
    main_binder.merge_lib_contexts_into_binder(&lib_contexts);

    // User symbol should still be in file_locals (not replaced by lib)
    let final_sym_id = main_binder
        .file_locals
        .get("myGlobal")
        .expect("myGlobal should exist");
    assert_eq!(
        final_sym_id, user_sym_id,
        "User symbol should take precedence over lib symbol"
    );
}
