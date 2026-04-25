use super::*;
use tsz_binder::BinderState;
use tsz_common::position::{Position, Range};
use tsz_parser::ParserState;

fn make_location(file: &str, line: u32, start_col: u32, end_col: u32) -> Location {
    Location::new(
        file.to_string(),
        Range::new(Position::new(line, start_col), Position::new(line, end_col)),
    )
}

#[test]
fn test_add_and_find_references() {
    let mut index = SymbolIndex::new();

    // Add references for 'foo' in two files
    index.add_reference("a.ts", "foo", make_location("a.ts", 0, 6, 9));
    index.add_reference("a.ts", "foo", make_location("a.ts", 5, 0, 3));
    index.add_reference("b.ts", "foo", make_location("b.ts", 2, 10, 13));

    let refs = index.find_references("foo");
    assert_eq!(refs.len(), 3);

    let files = index.get_files_with_symbol("foo");
    assert_eq!(files.len(), 2);
    assert!(files.contains(&"a.ts".to_string()));
    assert!(files.contains(&"b.ts".to_string()));
}

#[test]
fn test_add_and_find_definitions() {
    let mut index = SymbolIndex::new();

    index.add_definition("MyClass", make_location("types.ts", 10, 0, 7));
    index.add_definition("MyClass", make_location("types.d.ts", 5, 0, 7));

    let defs = index.find_definitions("MyClass");
    assert_eq!(defs.len(), 2);
}

#[test]
fn test_import_tracking() {
    let mut index = SymbolIndex::new();

    index.add_import(
        "consumer.ts",
        ImportInfo {
            local_name: "foo".to_string(),
            source_module: "./utils".to_string(),
            exported_name: "foo".to_string(),
            kind: ImportKind::Named,
        },
    );

    index.add_import(
        "other.ts",
        ImportInfo {
            local_name: "utils".to_string(),
            source_module: "./utils".to_string(),
            exported_name: "*".to_string(),
            kind: ImportKind::Namespace,
        },
    );

    let importers = index.get_importing_files("./utils");
    assert_eq!(importers.len(), 2);

    let imports = index.get_imports("consumer.ts");
    assert_eq!(imports.len(), 1);
    assert_eq!(imports[0].local_name, "foo");
}

#[test]
fn test_export_tracking() {
    let mut index = SymbolIndex::new();

    index.add_export("utils.ts", "foo");
    index.add_export("utils.ts", "bar");
    index.add_export("types.ts", "MyType");

    let utils_exports = index.get_exports("utils.ts");
    assert_eq!(utils_exports.len(), 2);

    let types_exports = index.get_exports("types.ts");
    assert_eq!(types_exports.len(), 1);
}

#[test]
fn test_remove_file() {
    let mut index = SymbolIndex::new();

    // Add data for multiple files
    index.add_reference("a.ts", "foo", make_location("a.ts", 0, 0, 3));
    index.add_reference("b.ts", "foo", make_location("b.ts", 0, 0, 3));
    index.add_definition("foo", make_location("a.ts", 0, 0, 3));
    index.add_export("a.ts", "foo");
    index.add_import(
        "b.ts",
        ImportInfo {
            local_name: "foo".to_string(),
            source_module: "./a".to_string(),
            exported_name: "foo".to_string(),
            kind: ImportKind::Named,
        },
    );

    // Remove file a.ts
    index.remove_file("a.ts");

    // Verify a.ts data is removed
    let refs = index.find_references("foo");
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].file_path, "b.ts");

    let defs = index.find_definitions("foo");
    assert!(defs.is_empty());

    let exports = index.get_exports("a.ts");
    assert!(exports.is_empty());

    // b.ts data should still exist
    let imports = index.get_imports("b.ts");
    assert_eq!(imports.len(), 1);
}

#[test]
fn test_stats() {
    let mut index = SymbolIndex::new();

    index.add_reference("a.ts", "foo", make_location("a.ts", 0, 0, 3));
    index.add_reference("a.ts", "bar", make_location("a.ts", 1, 0, 3));
    index.add_definition("foo", make_location("a.ts", 0, 0, 3));
    index.add_export("a.ts", "foo");
    index.add_import(
        "b.ts",
        ImportInfo {
            local_name: "foo".to_string(),
            source_module: "./a".to_string(),
            exported_name: "foo".to_string(),
            kind: ImportKind::Named,
        },
    );

    let stats = index.stats();
    assert_eq!(stats.unique_symbols, 2); // foo, bar
    assert_eq!(stats.total_references, 2);
    assert_eq!(stats.total_definitions, 1);
    assert_eq!(stats.files_with_exports, 1);
    assert_eq!(stats.files_with_imports, 1);
}

#[test]
fn test_clear() {
    let mut index = SymbolIndex::new();

    index.add_reference("a.ts", "foo", make_location("a.ts", 0, 0, 3));
    index.add_definition("foo", make_location("a.ts", 0, 0, 3));

    index.clear();

    let refs = index.find_references("foo");
    assert!(refs.is_empty());

    let defs = index.find_definitions("foo");
    assert!(defs.is_empty());
}

// =========================================================================
// Definition kind tracking tests
// =========================================================================

#[test]
fn test_add_definition_with_kind_stores_kind() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "processItems",
        make_location("utils.ts", 0, 0, 12),
        SymbolKind::Function,
    );

    assert_eq!(
        index.get_definition_kind("processItems"),
        Some(SymbolKind::Function)
    );
    // The definition location should also be stored
    let defs = index.find_definitions("processItems");
    assert_eq!(defs.len(), 1);
}

#[test]
fn test_add_definition_without_kind_has_no_kind() {
    let mut index = SymbolIndex::new();
    index.add_definition("foo", make_location("a.ts", 0, 0, 3));

    assert_eq!(index.get_definition_kind("foo"), None);
}

#[test]
fn test_definition_kind_survives_multiple_adds() {
    let mut index = SymbolIndex::new();
    // First add without kind
    index.add_definition("bar", make_location("a.ts", 0, 0, 3));
    assert_eq!(index.get_definition_kind("bar"), None);

    // Then add with kind (simulating a re-index that now has binder data)
    index.add_definition_with_kind("bar", make_location("b.ts", 0, 0, 3), SymbolKind::Class);
    assert_eq!(index.get_definition_kind("bar"), Some(SymbolKind::Class));
    // Both definition locations should exist
    assert_eq!(index.find_definitions("bar").len(), 2);
}

#[test]
fn test_clear_removes_definition_kinds() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind("Foo", make_location("a.ts", 0, 0, 3), SymbolKind::Class);

    index.clear();

    assert_eq!(index.get_definition_kind("Foo"), None);
}

#[test]
fn test_remove_file_cleans_up_definition_kinds() {
    let mut index = SymbolIndex::new();

    // Add a definition with kind from file a.ts
    index.add_definition_with_kind("MyClass", make_location("a.ts", 0, 0, 7), SymbolKind::Class);

    assert_eq!(
        index.get_definition_kind("MyClass"),
        Some(SymbolKind::Class)
    );

    // Remove the file
    index.remove_file("a.ts");

    // The kind should be cleaned up because no definitions remain
    assert_eq!(index.get_definition_kind("MyClass"), None);
    assert!(index.find_definitions("MyClass").is_empty());
}

#[test]
fn test_remove_file_keeps_kind_if_other_file_has_definition() {
    let mut index = SymbolIndex::new();

    // Add definitions in two files
    index.add_definition_with_kind(
        "Config",
        make_location("a.ts", 0, 0, 6),
        SymbolKind::Interface,
    );
    index.add_definition("Config", make_location("b.ts", 0, 0, 6));

    // Remove only a.ts
    index.remove_file("a.ts");

    // b.ts still has a definition, so the kind should be retained
    assert_eq!(index.find_definitions("Config").len(), 1);
    assert_eq!(
        index.get_definition_kind("Config"),
        Some(SymbolKind::Interface)
    );
}

// =========================================================================
// symbol_flags_to_kind tests
// =========================================================================

#[test]
fn test_symbol_flags_to_kind_function() {
    assert_eq!(
        symbol_flags_to_kind(symbol_flags::FUNCTION),
        SymbolKind::Function
    );
}

#[test]
fn test_symbol_flags_to_kind_class() {
    assert_eq!(symbol_flags_to_kind(symbol_flags::CLASS), SymbolKind::Class);
}

#[test]
fn test_symbol_flags_to_kind_interface() {
    assert_eq!(
        symbol_flags_to_kind(symbol_flags::INTERFACE),
        SymbolKind::Interface
    );
}

#[test]
fn test_symbol_flags_to_kind_regular_enum() {
    assert_eq!(
        symbol_flags_to_kind(symbol_flags::REGULAR_ENUM),
        SymbolKind::Enum
    );
}

#[test]
fn test_symbol_flags_to_kind_const_enum() {
    assert_eq!(
        symbol_flags_to_kind(symbol_flags::CONST_ENUM),
        SymbolKind::Enum
    );
}

#[test]
fn test_symbol_flags_to_kind_enum_member() {
    assert_eq!(
        symbol_flags_to_kind(symbol_flags::ENUM_MEMBER),
        SymbolKind::EnumMember
    );
}

#[test]
fn test_symbol_flags_to_kind_type_alias() {
    assert_eq!(
        symbol_flags_to_kind(symbol_flags::TYPE_ALIAS),
        SymbolKind::TypeParameter
    );
}

#[test]
fn test_symbol_flags_to_kind_module() {
    assert_eq!(
        symbol_flags_to_kind(symbol_flags::VALUE_MODULE),
        SymbolKind::Module
    );
    assert_eq!(
        symbol_flags_to_kind(symbol_flags::NAMESPACE_MODULE),
        SymbolKind::Module
    );
}

#[test]
fn test_symbol_flags_to_kind_method() {
    assert_eq!(
        symbol_flags_to_kind(symbol_flags::METHOD),
        SymbolKind::Method
    );
}

#[test]
fn test_symbol_flags_to_kind_property() {
    assert_eq!(
        symbol_flags_to_kind(symbol_flags::PROPERTY),
        SymbolKind::Property
    );
}

#[test]
fn test_symbol_flags_to_kind_constructor() {
    assert_eq!(
        symbol_flags_to_kind(symbol_flags::CONSTRUCTOR),
        SymbolKind::Constructor
    );
}

#[test]
fn test_symbol_flags_to_kind_variable() {
    assert_eq!(
        symbol_flags_to_kind(symbol_flags::BLOCK_SCOPED_VARIABLE),
        SymbolKind::Variable
    );
    assert_eq!(
        symbol_flags_to_kind(symbol_flags::FUNCTION_SCOPED_VARIABLE),
        SymbolKind::Variable
    );
}

#[test]
fn test_symbol_flags_to_kind_none() {
    assert_eq!(
        symbol_flags_to_kind(symbol_flags::NONE),
        SymbolKind::Variable
    );
}

#[test]
fn test_symbol_flags_to_kind_composite_prefers_function() {
    // A symbol with both FUNCTION and BLOCK_SCOPED_VARIABLE flags
    // should be reported as Function (higher specificity).
    let flags = symbol_flags::FUNCTION | symbol_flags::BLOCK_SCOPED_VARIABLE;
    assert_eq!(symbol_flags_to_kind(flags), SymbolKind::Function);
}

#[test]
fn test_symbol_flags_to_kind_class_with_interface_merge() {
    // Class + Interface (declaration merging): Class wins because
    // it is checked first in the specificity order.
    let flags = symbol_flags::CLASS | symbol_flags::INTERFACE;
    assert_eq!(symbol_flags_to_kind(flags), SymbolKind::Class);
}

// =========================================================================
// Prefix matching tests
// =========================================================================

#[test]
fn test_get_symbols_with_prefix_empty() {
    let index = SymbolIndex::new();
    let matches = index.get_symbols_with_prefix("foo");
    assert!(matches.is_empty());
}

#[test]
fn test_get_symbols_with_prefix_single_match() {
    let mut index = SymbolIndex::new();
    index.add_definition("fooBar", make_location("a.ts", 0, 0, 6));

    let matches = index.get_symbols_with_prefix("foo");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0], "fooBar");
}

#[test]
fn test_get_symbols_with_prefix_multiple_matches() {
    let mut index = SymbolIndex::new();
    index.add_definition("fooBar", make_location("a.ts", 0, 0, 6));
    index.add_definition("fooBaz", make_location("a.ts", 1, 0, 6));
    index.add_definition("barQux", make_location("a.ts", 2, 0, 6));

    let matches = index.get_symbols_with_prefix("foo");
    assert_eq!(matches.len(), 2);
    assert!(matches.contains(&"fooBar".to_string()));
    assert!(matches.contains(&"fooBaz".to_string()));
}

#[test]
fn test_get_symbols_with_prefix_case_sensitive() {
    let mut index = SymbolIndex::new();
    index.add_definition("fooBar", make_location("a.ts", 0, 0, 6));
    index.add_definition("FooBaz", make_location("a.ts", 1, 0, 6));

    let matches = index.get_symbols_with_prefix("foo");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0], "fooBar");

    let matches_upper = index.get_symbols_with_prefix("Foo");
    assert_eq!(matches_upper.len(), 1);
    assert_eq!(matches_upper[0], "FooBaz");
}

#[test]
fn test_get_symbols_with_prefix_returns_sorted() {
    let mut index = SymbolIndex::new();
    // Add in non-sorted order
    index.add_definition("zebra", make_location("a.ts", 0, 0, 5));
    index.add_definition("apple", make_location("a.ts", 1, 0, 5));
    index.add_definition("banana", make_location("a.ts", 2, 0, 6));

    let matches = index.get_symbols_with_prefix("a");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0], "apple");

    // All symbols should be in sorted order
    let all_matches = index.get_symbols_with_prefix("");
    assert_eq!(all_matches, vec!["apple", "banana", "zebra"]);
}

#[test]
fn test_get_symbols_with_prefix_after_remove() {
    let mut index = SymbolIndex::new();
    index.add_definition("fooBar", make_location("a.ts", 0, 0, 6));
    index.add_definition("fooBaz", make_location("b.ts", 0, 0, 6));
    index.add_definition("barQux", make_location("c.ts", 0, 0, 6));

    // Remove file a.ts which contains fooBar
    index.remove_file("a.ts");

    let matches = index.get_symbols_with_prefix("foo");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0], "fooBaz");
}

#[test]
fn test_get_symbols_with_prefix_empty_prefix() {
    let mut index = SymbolIndex::new();
    index.add_definition("zebra", make_location("a.ts", 0, 0, 5));
    index.add_definition("apple", make_location("b.ts", 0, 0, 5));

    let matches = index.get_symbols_with_prefix("");
    assert_eq!(matches.len(), 2);
    // Should be sorted
    assert_eq!(matches[0], "apple");
    assert_eq!(matches[1], "zebra");
}

#[test]
fn test_get_symbols_with_prefix_no_match() {
    let mut index = SymbolIndex::new();
    index.add_definition("fooBar", make_location("a.ts", 0, 0, 6));

    let matches = index.get_symbols_with_prefix("xyz");
    assert!(matches.is_empty());
}

// =========================================================================
// Additional coverage tests
// =========================================================================

#[test]
fn test_remove_nonexistent_file() {
    let mut index = SymbolIndex::new();
    index.add_definition("foo", make_location("a.ts", 0, 0, 3));

    // Removing a file that was never added should not panic or affect existing data
    index.remove_file("nonexistent.ts");

    let defs = index.find_definitions("foo");
    assert_eq!(defs.len(), 1, "Existing definitions should be preserved");
}

#[test]
fn test_readd_same_file_updates_index() {
    let mut index = SymbolIndex::new();
    index.add_definition("foo", make_location("a.ts", 0, 0, 3));
    index.add_reference("a.ts", "bar", make_location("a.ts", 5, 0, 3));

    // Remove and re-add with different content
    index.remove_file("a.ts");
    index.add_definition("baz", make_location("a.ts", 0, 0, 3));

    // Old symbols should be gone
    let foo_defs = index.find_definitions("foo");
    assert!(
        foo_defs.is_empty(),
        "Old definition 'foo' should be removed"
    );

    let bar_refs = index.find_references("bar");
    assert!(bar_refs.is_empty(), "Old reference 'bar' should be removed");

    // New symbol should be present
    let baz_defs = index.find_definitions("baz");
    assert_eq!(baz_defs.len(), 1, "New definition 'baz' should be present");
}

#[test]
fn test_add_reference_creates_file_symbol_tracking() {
    let mut index = SymbolIndex::new();
    index.add_reference("x.ts", "myVar", make_location("x.ts", 0, 0, 5));

    let files = index.get_files_with_symbol("myVar");
    assert!(files.contains(&"x.ts".to_string()));
}

#[test]
fn test_prefix_search_with_special_characters() {
    let mut index = SymbolIndex::new();
    index.add_definition("$scope", make_location("a.ts", 0, 0, 6));
    index.add_definition("$element", make_location("a.ts", 1, 0, 8));
    index.add_definition("_private", make_location("a.ts", 2, 0, 8));

    let dollar_matches = index.get_symbols_with_prefix("$");
    assert_eq!(dollar_matches.len(), 2);
    assert!(dollar_matches.contains(&"$element".to_string()));
    assert!(dollar_matches.contains(&"$scope".to_string()));

    let underscore_matches = index.get_symbols_with_prefix("_");
    assert_eq!(underscore_matches.len(), 1);
    assert_eq!(underscore_matches[0], "_private");
}

#[test]
fn test_many_symbols_in_single_file() {
    let mut index = SymbolIndex::new();
    for i in 0..200 {
        let name = format!("sym_{i}");
        index.add_definition(&name, make_location("big.ts", i, 0, 10));
    }

    let stats = index.stats();
    assert_eq!(stats.total_definitions, 200);

    // Prefix search should work
    let matches = index.get_symbols_with_prefix("sym_1");
    // Should match sym_1, sym_10..sym_19, sym_100..sym_199
    assert!(matches.len() > 10);

    // Remove the file
    index.remove_file("big.ts");
    let after_stats = index.stats();
    assert_eq!(
        after_stats.total_definitions, 0,
        "All definitions should be removed"
    );
}

#[test]
fn test_find_references_nonexistent_symbol() {
    let index = SymbolIndex::new();
    let refs = index.find_references("nonexistent");
    assert!(refs.is_empty());
}

#[test]
fn test_find_definitions_nonexistent_symbol() {
    let index = SymbolIndex::new();
    let defs = index.find_definitions("nonexistent");
    assert!(defs.is_empty());
}

#[test]
fn test_get_files_with_symbol_nonexistent() {
    let index = SymbolIndex::new();
    let files = index.get_files_with_symbol("nonexistent");
    assert!(files.is_empty());
}

#[test]
fn test_import_and_importer_tracking() {
    let mut index = SymbolIndex::new();

    index.add_import(
        "app.ts",
        ImportInfo {
            local_name: "default".to_string(),
            source_module: "./utils".to_string(),
            exported_name: "default".to_string(),
            kind: ImportKind::Default,
        },
    );
    index.add_import(
        "app.ts",
        ImportInfo {
            local_name: "side".to_string(),
            source_module: "./polyfill".to_string(),
            exported_name: "".to_string(),
            kind: ImportKind::SideEffect,
        },
    );

    let imports = index.get_imports("app.ts");
    assert_eq!(imports.len(), 2);

    let importers = index.get_importing_files("./utils");
    assert!(importers.contains(&"app.ts".to_string()));

    let polyfill_importers = index.get_importing_files("./polyfill");
    assert!(polyfill_importers.contains(&"app.ts".to_string()));
}

#[test]
fn test_remove_file_cleans_up_imports() {
    let mut index = SymbolIndex::new();
    index.add_import(
        "consumer.ts",
        ImportInfo {
            local_name: "foo".to_string(),
            source_module: "./lib".to_string(),
            exported_name: "foo".to_string(),
            kind: ImportKind::Named,
        },
    );

    index.remove_file("consumer.ts");

    let imports = index.get_imports("consumer.ts");
    assert!(imports.is_empty(), "Imports should be cleaned up");

    let importers = index.get_importing_files("./lib");
    assert!(
        importers.is_empty(),
        "Importer reference should be cleaned up"
    );
}

#[test]
fn test_has_file_with_exports() {
    let mut index = SymbolIndex::new();
    index.add_export("mod.ts", "default");

    assert!(index.has_file("mod.ts"));
    assert!(!index.has_file("other.ts"));
}

#[test]
fn test_has_file_with_imports() {
    let mut index = SymbolIndex::new();
    index.add_import(
        "consumer.ts",
        ImportInfo {
            local_name: "x".to_string(),
            source_module: "./mod".to_string(),
            exported_name: "x".to_string(),
            kind: ImportKind::Named,
        },
    );

    assert!(index.has_file("consumer.ts"));
}

#[test]
fn test_sorted_names_deduplication() {
    let mut index = SymbolIndex::new();
    // Add same symbol name from multiple files
    index.add_definition("shared", make_location("a.ts", 0, 0, 6));
    index.add_definition("shared", make_location("b.ts", 0, 0, 6));
    index.add_definition("shared", make_location("c.ts", 0, 0, 6));

    // Prefix search should only return one entry for "shared"
    let matches = index.get_symbols_with_prefix("shared");
    assert_eq!(
        matches.len(),
        1,
        "Sorted names should be deduplicated, got: {matches:?}"
    );
}

#[test]
fn test_definition_with_kind_overwrites_previous_kind() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind("Foo", make_location("a.ts", 0, 0, 3), SymbolKind::Class);
    // Overwrite with a different kind
    index.add_definition_with_kind("Foo", make_location("b.ts", 0, 0, 3), SymbolKind::Interface);

    // The last kind wins
    assert_eq!(
        index.get_definition_kind("Foo"),
        Some(SymbolKind::Interface)
    );
    // But both definitions exist
    assert_eq!(index.find_definitions("Foo").len(), 2);
}

#[test]
fn test_stats_indexed_files() {
    let mut index = SymbolIndex::new();
    index.add_reference("a.ts", "x", make_location("a.ts", 0, 0, 1));
    index.add_reference("b.ts", "y", make_location("b.ts", 0, 0, 1));

    let stats = index.stats();
    assert_eq!(stats.indexed_files, 2);
}

#[test]
fn test_clear_resets_everything() {
    let mut index = SymbolIndex::new();
    index.add_reference("a.ts", "x", make_location("a.ts", 0, 0, 1));
    index.add_definition("y", make_location("b.ts", 0, 0, 1));
    index.add_definition_with_kind("z", make_location("c.ts", 0, 0, 1), SymbolKind::Function);
    index.add_export("d.ts", "w");
    index.add_import(
        "e.ts",
        ImportInfo {
            local_name: "v".to_string(),
            source_module: "./mod".to_string(),
            exported_name: "v".to_string(),
            kind: ImportKind::Named,
        },
    );

    index.clear();

    let stats = index.stats();
    assert_eq!(stats.unique_symbols, 0);
    assert_eq!(stats.total_references, 0);
    assert_eq!(stats.total_definitions, 0);
    assert_eq!(stats.files_with_exports, 0);
    assert_eq!(stats.files_with_imports, 0);
    assert_eq!(stats.indexed_files, 0);

    let all = index.get_symbols_with_prefix("");
    assert!(all.is_empty(), "sorted_names should be cleared");
}

#[test]
fn test_symbol_index_many_definitions_same_symbol() {
    let mut index = SymbolIndex::new();
    for i in 0..10 {
        index.add_definition("overloaded", make_location("main.ts", i, 0, 10));
    }
    let defs = index.find_definitions("overloaded");
    assert_eq!(defs.len(), 10, "Should store all 10 definitions");
}

#[test]
fn test_symbol_index_many_references_same_symbol() {
    let mut index = SymbolIndex::new();
    for i in 0..20 {
        index.add_reference(
            "main.ts",
            "frequently_used",
            make_location("main.ts", i, 0, 15),
        );
    }
    let refs = index.find_references("frequently_used");
    assert_eq!(refs.len(), 20, "Should store all 20 references");
}

#[test]
fn test_symbol_index_prefix_no_match() {
    let mut index = SymbolIndex::new();
    index.add_definition("alpha", make_location("a.ts", 0, 0, 5));
    index.add_definition("beta", make_location("b.ts", 0, 0, 4));
    let results = index.get_symbols_with_prefix("gamma");
    assert!(
        results.is_empty(),
        "Non-matching prefix should return empty"
    );
}

#[test]
fn test_symbol_index_prefix_partial_match() {
    let mut index = SymbolIndex::new();
    index.add_definition("processData", make_location("a.ts", 0, 0, 11));
    index.add_definition("processFile", make_location("b.ts", 0, 0, 11));
    index.add_definition("handleError", make_location("c.ts", 0, 0, 11));
    let results = index.get_symbols_with_prefix("process");
    assert_eq!(
        results.len(),
        2,
        "Should find both 'process' prefixed symbols"
    );
}

#[test]
fn test_symbol_index_case_sensitive_prefix() {
    let mut index = SymbolIndex::new();
    index.add_definition("MyClass", make_location("a.ts", 0, 0, 7));
    index.add_definition("myFunction", make_location("b.ts", 0, 0, 10));
    let upper = index.get_symbols_with_prefix("My");
    let lower = index.get_symbols_with_prefix("my");
    assert_eq!(upper.len(), 1, "Upper case prefix should match MyClass");
    assert_eq!(lower.len(), 1, "Lower case prefix should match myFunction");
}

#[test]
fn test_symbol_index_imports_same_file() {
    let mut index = SymbolIndex::new();
    index.add_import(
        "app.ts",
        ImportInfo {
            local_name: "foo".to_string(),
            source_module: "./utils".to_string(),
            exported_name: "foo".to_string(),
            kind: ImportKind::Named,
        },
    );
    index.add_import(
        "app.ts",
        ImportInfo {
            local_name: "bar".to_string(),
            source_module: "./utils".to_string(),
            exported_name: "bar".to_string(),
            kind: ImportKind::Named,
        },
    );
    let imports = index.get_imports("app.ts");
    assert_eq!(imports.len(), 2, "Should have both imports for same file");
}

#[test]
fn test_symbol_index_exports_multiple_files() {
    let mut index = SymbolIndex::new();
    index.add_export("a.ts", "alpha");
    index.add_export("a.ts", "beta");
    index.add_export("b.ts", "gamma");
    let a_exports = index.get_exports("a.ts");
    let b_exports = index.get_exports("b.ts");
    assert_eq!(a_exports.len(), 2);
    assert_eq!(b_exports.len(), 1);
}

#[test]
fn test_symbol_index_stats_comprehensive() {
    let mut index = SymbolIndex::new();
    index.add_definition("x", make_location("a.ts", 0, 0, 1));
    index.add_definition("y", make_location("b.ts", 0, 0, 1));
    index.add_reference("a.ts", "x", make_location("a.ts", 5, 0, 1));
    index.add_reference("c.ts", "x", make_location("c.ts", 0, 0, 1));
    index.add_export("a.ts", "x");
    index.add_import(
        "d.ts",
        ImportInfo {
            local_name: "x".to_string(),
            source_module: "./a".to_string(),
            exported_name: "x".to_string(),
            kind: ImportKind::Named,
        },
    );
    let stats = index.stats();
    assert!(
        stats.unique_symbols >= 2,
        "Should have at least 2 unique symbols"
    );
    assert!(
        stats.total_definitions >= 2,
        "Should have at least 2 definitions"
    );
    assert!(
        stats.total_references >= 2,
        "Should have at least 2 references"
    );
}

#[test]
fn test_symbol_index_empty_string_symbol() {
    let mut index = SymbolIndex::new();
    index.add_definition("", make_location("a.ts", 0, 0, 0));
    let defs = index.find_definitions("");
    // Empty string symbol behavior is implementation-defined
    let _ = defs;
}

#[test]
fn test_symbol_index_unicode_symbol_names() {
    let mut index = SymbolIndex::new();
    index.add_definition("日本語", make_location("a.ts", 0, 0, 3));
    let defs = index.find_definitions("日本語");
    assert_eq!(defs.len(), 1, "Should find Unicode symbol name");
}

#[test]
fn test_symbol_index_special_chars_symbol() {
    let mut index = SymbolIndex::new();
    index.add_definition("$element", make_location("a.ts", 0, 0, 8));
    index.add_definition("_private", make_location("b.ts", 0, 0, 8));
    let dollar = index.find_definitions("$element");
    let underscore = index.find_definitions("_private");
    assert_eq!(dollar.len(), 1);
    assert_eq!(underscore.len(), 1);
}

// =========================================================================
// Additional coverage tests for untested API surface
// =========================================================================

#[test]
fn test_all_definition_names_returns_all_names() {
    let mut index = SymbolIndex::new();
    index.add_definition("alpha", make_location("a.ts", 0, 0, 5));
    index.add_definition("beta", make_location("b.ts", 0, 0, 4));
    index.add_definition("gamma", make_location("c.ts", 0, 0, 5));

    let names: Vec<&str> = index.all_definition_names().collect();
    assert_eq!(names.len(), 3);
    assert!(names.contains(&"alpha"));
    assert!(names.contains(&"beta"));
    assert!(names.contains(&"gamma"));
}

#[test]
fn test_all_definition_names_empty_index() {
    let index = SymbolIndex::new();
    let names: Vec<&str> = index.all_definition_names().collect();
    assert!(names.is_empty());
}

#[test]
fn test_all_definition_names_after_remove() {
    let mut index = SymbolIndex::new();
    index.add_definition("keep", make_location("a.ts", 0, 0, 4));
    index.add_definition("remove", make_location("b.ts", 0, 0, 6));

    index.remove_file("b.ts");

    let names: Vec<&str> = index.all_definition_names().collect();
    assert_eq!(names.len(), 1);
    assert!(names.contains(&"keep"));
}

#[test]
fn test_stats_total_import_relationships() {
    let mut index = SymbolIndex::new();
    index.add_import(
        "a.ts",
        ImportInfo {
            local_name: "x".to_string(),
            source_module: "./mod1".to_string(),
            exported_name: "x".to_string(),
            kind: ImportKind::Named,
        },
    );
    index.add_import(
        "b.ts",
        ImportInfo {
            local_name: "y".to_string(),
            source_module: "./mod1".to_string(),
            exported_name: "y".to_string(),
            kind: ImportKind::Named,
        },
    );
    index.add_import(
        "c.ts",
        ImportInfo {
            local_name: "z".to_string(),
            source_module: "./mod2".to_string(),
            exported_name: "z".to_string(),
            kind: ImportKind::Named,
        },
    );

    let stats = index.stats();
    // mod1 has 2 importers (a.ts, b.ts), mod2 has 1 (c.ts) = 3 total
    assert_eq!(stats.total_import_relationships, 3);
    assert_eq!(stats.files_with_imports, 3);
}

#[test]
fn test_has_file_with_references() {
    let mut index = SymbolIndex::new();
    index.add_reference("ref.ts", "sym", make_location("ref.ts", 0, 0, 3));

    assert!(index.has_file("ref.ts"));
    assert!(!index.has_file("other.ts"));
}

#[test]
fn test_get_exports_nonexistent_file() {
    let index = SymbolIndex::new();
    let exports = index.get_exports("nonexistent.ts");
    assert!(exports.is_empty());
}

#[test]
fn test_get_imports_nonexistent_file() {
    let index = SymbolIndex::new();
    let imports = index.get_imports("nonexistent.ts");
    assert!(imports.is_empty());
}

#[test]
fn test_get_importing_files_nonexistent_module() {
    let index = SymbolIndex::new();
    let importers = index.get_importing_files("./nonexistent");
    assert!(importers.is_empty());
}

#[test]
fn test_remove_file_twice_does_not_panic() {
    let mut index = SymbolIndex::new();
    index.add_definition("foo", make_location("a.ts", 0, 0, 3));
    index.remove_file("a.ts");
    index.remove_file("a.ts"); // second remove should be safe
    let defs = index.find_definitions("foo");
    assert!(defs.is_empty());
}

#[test]
fn test_multiple_files_same_export_name() {
    let mut index = SymbolIndex::new();
    index.add_export("a.ts", "default");
    index.add_export("b.ts", "default");

    let a_exports = index.get_exports("a.ts");
    let b_exports = index.get_exports("b.ts");
    assert!(a_exports.contains(&"default".to_string()));
    assert!(b_exports.contains(&"default".to_string()));
}

#[test]
fn test_import_kind_variants() {
    let mut index = SymbolIndex::new();

    index.add_import(
        "app.ts",
        ImportInfo {
            local_name: "React".to_string(),
            source_module: "react".to_string(),
            exported_name: "default".to_string(),
            kind: ImportKind::Default,
        },
    );
    index.add_import(
        "app.ts",
        ImportInfo {
            local_name: "ns".to_string(),
            source_module: "./helpers".to_string(),
            exported_name: "*".to_string(),
            kind: ImportKind::Namespace,
        },
    );
    index.add_import(
        "app.ts",
        ImportInfo {
            local_name: "useState".to_string(),
            source_module: "react".to_string(),
            exported_name: "useState".to_string(),
            kind: ImportKind::Named,
        },
    );
    index.add_import(
        "app.ts",
        ImportInfo {
            local_name: "".to_string(),
            source_module: "./polyfill".to_string(),
            exported_name: "".to_string(),
            kind: ImportKind::SideEffect,
        },
    );

    let imports = index.get_imports("app.ts");
    assert_eq!(imports.len(), 4);

    // Verify each kind is stored correctly
    assert!(imports.iter().any(|i| i.kind == ImportKind::Default));
    assert!(imports.iter().any(|i| i.kind == ImportKind::Namespace));
    assert!(imports.iter().any(|i| i.kind == ImportKind::Named));
    assert!(imports.iter().any(|i| i.kind == ImportKind::SideEffect));
}

#[test]
fn test_symbol_flags_to_kind_accessor() {
    assert_eq!(
        symbol_flags_to_kind(symbol_flags::ACCESSOR),
        SymbolKind::Property
    );
}

#[test]
fn test_symbol_flags_to_kind_get_accessor() {
    assert_eq!(
        symbol_flags_to_kind(symbol_flags::GET_ACCESSOR),
        SymbolKind::Property
    );
}

#[test]
fn test_symbol_flags_to_kind_type_parameter() {
    assert_eq!(
        symbol_flags_to_kind(symbol_flags::TYPE_PARAMETER),
        SymbolKind::TypeParameter
    );
}

// =========================================================================
// Additional edge case tests
// =========================================================================

#[test]
fn test_add_same_definition_twice_same_location() {
    let mut index = SymbolIndex::new();
    let loc = make_location("a.ts", 0, 0, 3);
    index.add_definition("foo", loc.clone());
    index.add_definition("foo", loc);

    let defs = index.find_definitions("foo");
    // Both adds should be recorded (no dedup on location)
    assert_eq!(defs.len(), 2);
}

#[test]
fn test_add_export_same_name_twice_same_file() {
    let mut index = SymbolIndex::new();
    index.add_export("a.ts", "foo");
    index.add_export("a.ts", "foo");

    let exports = index.get_exports("a.ts");
    // Implementation may or may not deduplicate; just verify no crash
    assert!(!exports.is_empty());
}

#[test]
fn test_stats_after_remove_all_files() {
    let mut index = SymbolIndex::new();
    index.add_reference("a.ts", "x", make_location("a.ts", 0, 0, 1));
    index.add_reference("b.ts", "y", make_location("b.ts", 0, 0, 1));
    index.add_definition("x", make_location("a.ts", 0, 0, 1));
    index.add_export("a.ts", "x");

    index.remove_file("a.ts");
    index.remove_file("b.ts");

    let stats = index.stats();
    assert_eq!(stats.total_references, 0);
    assert_eq!(stats.total_definitions, 0);
    assert_eq!(stats.files_with_exports, 0);
}

#[test]
fn test_prefix_search_with_long_prefix() {
    let mut index = SymbolIndex::new();
    index.add_definition("handleUserAuthentication", make_location("a.ts", 0, 0, 24));
    index.add_definition("handleUserRegistration", make_location("a.ts", 1, 0, 22));
    index.add_definition("handleError", make_location("a.ts", 2, 0, 11));

    let matches = index.get_symbols_with_prefix("handleUser");
    assert_eq!(matches.len(), 2);
    assert!(matches.contains(&"handleUserAuthentication".to_string()));
    assert!(matches.contains(&"handleUserRegistration".to_string()));
}

#[test]
fn test_reference_location_data_preserved() {
    let mut index = SymbolIndex::new();
    let loc = make_location("module.ts", 5, 10, 15);
    index.add_reference("module.ts", "myFunc", loc);

    let refs = index.find_references("myFunc");
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].file_path, "module.ts");
    assert_eq!(refs[0].range.start.line, 5);
    assert_eq!(refs[0].range.start.character, 10);
    assert_eq!(refs[0].range.end.character, 15);
}

#[test]
fn test_definition_location_data_preserved() {
    let mut index = SymbolIndex::new();
    let loc = make_location("types.ts", 12, 4, 20);
    index.add_definition("MyInterface", loc);

    let defs = index.find_definitions("MyInterface");
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].file_path, "types.ts");
    assert_eq!(defs[0].range.start.line, 12);
    assert_eq!(defs[0].range.start.character, 4);
    assert_eq!(defs[0].range.end.character, 20);
}

#[test]
fn test_remove_file_preserves_other_file_imports() {
    let mut index = SymbolIndex::new();
    index.add_import(
        "a.ts",
        ImportInfo {
            local_name: "x".to_string(),
            source_module: "./shared".to_string(),
            exported_name: "x".to_string(),
            kind: ImportKind::Named,
        },
    );
    index.add_import(
        "b.ts",
        ImportInfo {
            local_name: "y".to_string(),
            source_module: "./shared".to_string(),
            exported_name: "y".to_string(),
            kind: ImportKind::Named,
        },
    );

    index.remove_file("a.ts");

    // b.ts imports should still exist
    let b_imports = index.get_imports("b.ts");
    assert_eq!(b_imports.len(), 1);
    assert_eq!(b_imports[0].local_name, "y");

    // The shared module should still have b.ts as an importer
    let importers = index.get_importing_files("./shared");
    assert_eq!(importers.len(), 1);
    assert!(importers.contains(&"b.ts".to_string()));
}

#[test]
fn test_has_file_after_clear() {
    let mut index = SymbolIndex::new();
    index.add_export("a.ts", "foo");
    assert!(index.has_file("a.ts"));

    index.clear();
    assert!(!index.has_file("a.ts"));
}

#[test]
fn test_symbol_flags_to_kind_set_accessor() {
    assert_eq!(
        symbol_flags_to_kind(symbol_flags::SET_ACCESSOR),
        SymbolKind::Property
    );
}

#[test]
fn test_symbol_flags_to_kind_signature() {
    // SIGNATURE flags should map to Method or Function
    let kind = symbol_flags_to_kind(symbol_flags::SIGNATURE);
    // The exact mapping is implementation-defined; just verify no panic
    let _ = kind;
}

#[test]
fn test_all_definition_names_with_kinds() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind("Alpha", make_location("a.ts", 0, 0, 5), SymbolKind::Class);
    index.add_definition_with_kind("Beta", make_location("b.ts", 0, 0, 4), SymbolKind::Function);
    index.add_definition("Gamma", make_location("c.ts", 0, 0, 5));

    let names: Vec<&str> = index.all_definition_names().collect();
    assert_eq!(names.len(), 3);
    assert!(names.contains(&"Alpha"));
    assert!(names.contains(&"Beta"));
    assert!(names.contains(&"Gamma"));

    assert_eq!(index.get_definition_kind("Alpha"), Some(SymbolKind::Class));
    assert_eq!(
        index.get_definition_kind("Beta"),
        Some(SymbolKind::Function)
    );
    assert_eq!(index.get_definition_kind("Gamma"), None);
}

// =========================================================================
// Batch 4: additional edge-case and coverage tests
// =========================================================================

#[test]
fn test_symbol_index_reference_across_many_files() {
    let mut index = SymbolIndex::new();
    for i in 0..50 {
        let file = format!("file_{i}.ts");
        index.add_reference(&file, "shared", make_location(&file, 0, 0, 6));
    }
    let refs = index.find_references("shared");
    assert_eq!(refs.len(), 50, "Should find references across 50 files");
    let files = index.get_files_with_symbol("shared");
    assert_eq!(files.len(), 50, "Should track 50 files with the symbol");
}

#[test]
fn test_symbol_index_remove_file_with_multiple_symbols() {
    let mut index = SymbolIndex::new();
    index.add_reference("a.ts", "alpha", make_location("a.ts", 0, 0, 5));
    index.add_reference("a.ts", "beta", make_location("a.ts", 1, 0, 4));
    index.add_reference("a.ts", "gamma", make_location("a.ts", 2, 0, 5));
    index.add_definition("alpha", make_location("a.ts", 0, 0, 5));

    index.remove_file("a.ts");

    assert!(index.find_references("alpha").is_empty());
    assert!(index.find_references("beta").is_empty());
    assert!(index.find_references("gamma").is_empty());
    assert!(index.find_definitions("alpha").is_empty());
}

#[test]
fn test_symbol_index_mixed_imports_different_modules() {
    let mut index = SymbolIndex::new();
    index.add_import(
        "app.ts",
        ImportInfo {
            local_name: "a".to_string(),
            source_module: "./mod_a".to_string(),
            exported_name: "a".to_string(),
            kind: ImportKind::Named,
        },
    );
    index.add_import(
        "app.ts",
        ImportInfo {
            local_name: "b".to_string(),
            source_module: "./mod_b".to_string(),
            exported_name: "b".to_string(),
            kind: ImportKind::Named,
        },
    );

    let imports = index.get_imports("app.ts");
    assert_eq!(imports.len(), 2);

    let importers_a = index.get_importing_files("./mod_a");
    assert_eq!(importers_a.len(), 1);
    let importers_b = index.get_importing_files("./mod_b");
    assert_eq!(importers_b.len(), 1);
}

#[test]
fn test_symbol_index_definition_kind_enum_member() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "Red",
        make_location("colors.ts", 1, 2, 5),
        SymbolKind::EnumMember,
    );
    assert_eq!(
        index.get_definition_kind("Red"),
        Some(SymbolKind::EnumMember)
    );
}

#[test]
fn test_symbol_index_definition_kind_variable() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "myConst",
        make_location("a.ts", 0, 0, 7),
        SymbolKind::Variable,
    );
    assert_eq!(
        index.get_definition_kind("myConst"),
        Some(SymbolKind::Variable)
    );
}

#[test]
fn test_symbol_index_definition_kind_method() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "doWork",
        make_location("service.ts", 5, 2, 8),
        SymbolKind::Method,
    );
    assert_eq!(
        index.get_definition_kind("doWork"),
        Some(SymbolKind::Method)
    );
}

#[test]
fn test_symbol_index_definition_kind_constructor() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "constructor",
        make_location("a.ts", 3, 2, 13),
        SymbolKind::Constructor,
    );
    assert_eq!(
        index.get_definition_kind("constructor"),
        Some(SymbolKind::Constructor)
    );
}

#[test]
fn test_symbol_index_prefix_search_single_char_names() {
    let mut index = SymbolIndex::new();
    index.add_definition("a", make_location("a.ts", 0, 0, 1));
    index.add_definition("b", make_location("a.ts", 1, 0, 1));
    index.add_definition("c", make_location("a.ts", 2, 0, 1));

    let matches = index.get_symbols_with_prefix("a");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0], "a");

    let all = index.get_symbols_with_prefix("");
    assert_eq!(all.len(), 3);
}

#[test]
fn test_symbol_index_remove_file_preserves_other_exports() {
    let mut index = SymbolIndex::new();
    index.add_export("a.ts", "foo");
    index.add_export("b.ts", "bar");
    index.add_export("b.ts", "baz");

    index.remove_file("a.ts");

    assert!(index.get_exports("a.ts").is_empty());
    assert_eq!(index.get_exports("b.ts").len(), 2);
}

#[test]
fn test_symbol_index_stats_after_partial_remove() {
    let mut index = SymbolIndex::new();
    index.add_reference("a.ts", "x", make_location("a.ts", 0, 0, 1));
    index.add_reference("b.ts", "y", make_location("b.ts", 0, 0, 1));
    index.add_definition("x", make_location("a.ts", 0, 0, 1));
    index.add_definition("y", make_location("b.ts", 0, 0, 1));

    index.remove_file("a.ts");

    let stats = index.stats();
    assert_eq!(stats.total_references, 1);
    assert_eq!(stats.total_definitions, 1);
}

#[test]
fn test_symbol_flags_to_kind_method_with_property() {
    // Method + Property: Method should win (higher specificity)
    let flags = symbol_flags::METHOD | symbol_flags::PROPERTY;
    let kind = symbol_flags_to_kind(flags);
    // Method is checked before Property in specificity order
    assert_eq!(kind, SymbolKind::Method);
}

#[test]
fn test_symbol_flags_to_kind_enum_with_variable() {
    let flags = symbol_flags::REGULAR_ENUM | symbol_flags::BLOCK_SCOPED_VARIABLE;
    let kind = symbol_flags_to_kind(flags);
    // Enum should have higher specificity than variable
    assert_eq!(kind, SymbolKind::Enum);
}

#[test]
fn test_symbol_index_long_symbol_name() {
    let mut index = SymbolIndex::new();
    let long_name = "a".repeat(500);
    index.add_definition(&long_name, make_location("a.ts", 0, 0, 500));

    let defs = index.find_definitions(&long_name);
    assert_eq!(defs.len(), 1, "Should handle very long symbol names");

    let matches = index.get_symbols_with_prefix(&"a".repeat(100));
    assert_eq!(matches.len(), 1);
}

#[test]
fn test_symbol_index_numeric_symbol_name() {
    let mut index = SymbolIndex::new();
    index.add_definition("123", make_location("a.ts", 0, 0, 3));
    let defs = index.find_definitions("123");
    assert_eq!(defs.len(), 1, "Should handle numeric symbol names");
}

#[test]
fn test_symbol_index_has_file_with_definitions() {
    let mut index = SymbolIndex::new();
    index.add_definition("foo", make_location("def.ts", 0, 0, 3));
    // has_file may or may not detect files that only have definitions
    // (definitions don't include file_path tracking in the same way references do)
    let _ = index.has_file("def.ts");
}

#[test]
fn test_symbol_index_interleaved_add_remove() {
    let mut index = SymbolIndex::new();
    index.add_definition("x", make_location("a.ts", 0, 0, 1));
    index.remove_file("a.ts");
    index.add_definition("x", make_location("b.ts", 0, 0, 1));

    let defs = index.find_definitions("x");
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].file_path, "b.ts");
}

// ── index_file: automatic import extraction from binder ──────────────

fn parse_and_bind(file_name: &str, source: &str) -> (BinderState, ParserState) {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    (binder, parser)
}

#[test]
fn test_index_file_extracts_named_imports() {
    let source = r#"import { foo, bar } from './utils';"#;
    let (binder, parser) = parse_and_bind("app.ts", source);

    let mut index = SymbolIndex::new();
    index.index_file("app.ts", &binder, parser.get_arena(), source);

    let imports = index.get_imports("app.ts");
    assert!(
        imports.len() >= 2,
        "expected at least 2 imports from index_file, got {}",
        imports.len()
    );

    let foo_import = imports.iter().find(|i| i.local_name == "foo");
    assert!(foo_import.is_some(), "expected 'foo' import");
    let foo_import = foo_import.unwrap();
    assert_eq!(foo_import.source_module, "./utils");
    assert_eq!(foo_import.kind, ImportKind::Named);

    let bar_import = imports.iter().find(|i| i.local_name == "bar");
    assert!(bar_import.is_some(), "expected 'bar' import");

    // Reverse import graph should track the dependency
    let importers = index.get_importing_files("./utils");
    assert!(
        importers.contains(&"app.ts".to_string()),
        "expected app.ts in importers of ./utils"
    );
}

#[test]
fn test_index_file_extracts_default_import() {
    let source = r#"import React from 'react';"#;
    let (binder, parser) = parse_and_bind("app.ts", source);

    let mut index = SymbolIndex::new();
    index.index_file("app.ts", &binder, parser.get_arena(), source);

    let imports = index.get_imports("app.ts");
    let react_import = imports.iter().find(|i| i.local_name == "React");
    assert!(react_import.is_some(), "expected 'React' default import");
    let react_import = react_import.unwrap();
    assert_eq!(react_import.source_module, "react");
    assert_eq!(react_import.exported_name, "default");
    assert_eq!(react_import.kind, ImportKind::Default);
}

#[test]
fn test_index_file_extracts_namespace_import() {
    let source = r#"import * as utils from './utils';"#;
    let (binder, parser) = parse_and_bind("app.ts", source);

    let mut index = SymbolIndex::new();
    index.index_file("app.ts", &binder, parser.get_arena(), source);

    let imports = index.get_imports("app.ts");
    let ns_import = imports.iter().find(|i| i.local_name == "utils");
    assert!(ns_import.is_some(), "expected 'utils' namespace import");
    let ns_import = ns_import.unwrap();
    assert_eq!(ns_import.source_module, "./utils");
    assert_eq!(ns_import.exported_name, "*");
    assert_eq!(ns_import.kind, ImportKind::Namespace);
}

#[test]
fn test_index_file_extracts_side_effect_import() {
    let source = r#"import './polyfill';"#;
    let (binder, parser) = parse_and_bind("app.ts", source);

    let mut index = SymbolIndex::new();
    index.index_file("app.ts", &binder, parser.get_arena(), source);

    let imports = index.get_imports("app.ts");
    let side_effect = imports.iter().find(|i| i.source_module == "./polyfill");
    assert!(
        side_effect.is_some(),
        "expected side-effect import for './polyfill'"
    );
    assert_eq!(side_effect.unwrap().kind, ImportKind::SideEffect);

    let importers = index.get_importing_files("./polyfill");
    assert!(
        importers.contains(&"app.ts".to_string()),
        "expected app.ts in importers of ./polyfill"
    );
}

#[test]
fn test_index_file_has_file_after_import_indexing() {
    let source = r#"import { x } from './lib';"#;
    let (binder, parser) = parse_and_bind("consumer.ts", source);

    let mut index = SymbolIndex::new();
    index.index_file("consumer.ts", &binder, parser.get_arena(), source);

    assert!(
        index.has_file("consumer.ts"),
        "has_file should return true after index_file"
    );
}

#[test]
fn test_index_file_renamed_import() {
    let source = r#"import { foo as bar } from './mod';"#;
    let (binder, parser) = parse_and_bind("app.ts", source);

    let mut index = SymbolIndex::new();
    index.index_file("app.ts", &binder, parser.get_arena(), source);

    let imports = index.get_imports("app.ts");
    let renamed = imports.iter().find(|i| i.local_name == "bar");
    assert!(renamed.is_some(), "expected renamed import 'bar'");
    let renamed = renamed.unwrap();
    assert_eq!(renamed.exported_name, "foo");
    assert_eq!(renamed.source_module, "./mod");
    assert_eq!(renamed.kind, ImportKind::Named);
}

#[test]
fn test_index_file_remove_clears_auto_imports() {
    let source = r#"import { x } from './lib';"#;
    let (binder, parser) = parse_and_bind("app.ts", source);

    let mut index = SymbolIndex::new();
    index.index_file("app.ts", &binder, parser.get_arena(), source);

    assert!(!index.get_imports("app.ts").is_empty());
    assert!(!index.get_importing_files("./lib").is_empty());

    index.remove_file("app.ts");

    assert!(
        index.get_imports("app.ts").is_empty(),
        "imports should be cleared after remove_file"
    );
    assert!(
        index.get_importing_files("./lib").is_empty(),
        "importers should be cleared after remove_file"
    );
}

// ── heritage index ───────────────────────────────────────────────────

#[test]
fn test_index_file_populates_sub_to_bases_for_class() {
    // Regression: previously the second-pass heritage builder called
    // get_identifier_text on the ClassDeclaration node itself, which
    // always returned None — so sub_to_bases was silently empty and
    // upward heritage lookups returned nothing.
    let source = r#"
        class Base {}
        interface I {}
        class Sub extends Base implements I {}
    "#;
    let (binder, parser) = parse_and_bind("file.ts", source);

    let mut index = SymbolIndex::new();
    index.index_file("file.ts", &binder, parser.get_arena(), source);

    let bases = index.get_bases_for_class("Sub");
    assert!(
        bases.contains(&"Base".to_string()),
        "expected Sub -> Base, got {bases:?}"
    );
    assert!(
        bases.contains(&"I".to_string()),
        "expected Sub -> I (implements), got {bases:?}"
    );
}

#[test]
fn test_index_file_populates_sub_to_bases_for_interface() {
    let source = r#"
        interface Base {}
        interface Mixin {}
        interface Derived extends Base, Mixin {}
    "#;
    let (binder, parser) = parse_and_bind("file.ts", source);

    let mut index = SymbolIndex::new();
    index.index_file("file.ts", &binder, parser.get_arena(), source);

    let bases = index.get_bases_for_class("Derived");
    assert!(bases.contains(&"Base".to_string()), "got {bases:?}");
    assert!(bases.contains(&"Mixin".to_string()), "got {bases:?}");
}

#[test]
fn test_index_file_sub_to_bases_survives_large_class_body() {
    // Previously the second-pass used a 50-node forward-scan to find
    // HERITAGE_CLAUSE siblings — classes with big bodies between the
    // name and heritage clauses would fall outside the window. The fix
    // walks the declaration's own heritage_clauses list, so body size
    // is irrelevant.
    let mut body = String::new();
    for i in 0..60 {
        body.push_str(&format!("    m{i}() {{}}\n"));
    }
    let source = format!("class Base {{}}\nclass Sub extends Base {{\n{body}}}\n");
    let (binder, parser) = parse_and_bind("file.ts", &source);

    let mut index = SymbolIndex::new();
    index.index_file("file.ts", &binder, parser.get_arena(), &source);

    assert_eq!(
        index.get_bases_for_class("Sub"),
        vec!["Base".to_string()],
        "heritage lookup should work regardless of class body size"
    );
}

#[test]
fn test_remove_file_clears_sub_to_bases_for_classes_in_that_file() {
    // Regression: remove_file() iterated `sub_to_bases.values_mut()` and
    // tried to remove `file_name` from each value set — but the values are
    // base symbol names (not file paths), so the loop did nothing in
    // practice. After the fix, removing a file drops `sub_to_bases` entries
    // for classes that were declared in that file, so go-to-implementation
    // and upward-rename do not return ghost edges to deleted classes.
    let source = r"class Base {} class Sub extends Base {}";
    let (binder, parser) = parse_and_bind("file.ts", source);

    let mut index = SymbolIndex::new();
    index.index_file("file.ts", &binder, parser.get_arena(), source);

    assert!(
        !index.get_bases_for_class("Sub").is_empty(),
        "precondition: sub_to_bases should be populated after indexing"
    );

    index.remove_file("file.ts");

    assert!(
        index.get_bases_for_class("Sub").is_empty(),
        "remove_file should drop sub_to_bases entries for classes declared \
         in the removed file; got stale entry {:?}",
        index.get_bases_for_class("Sub")
    );
}

#[test]
fn test_remove_file_keeps_sub_to_bases_for_classes_in_other_files() {
    // Cleanup must not over-purge: classes declared in OTHER files keep
    // their heritage edges. Removing `a.ts` (which declares `Sub extends
    // Base`) must not affect `Other` declared in `b.ts`.
    let source_a = r"class Base {} class Sub extends Base {}";
    let source_b = r"interface Mixin {} class Other extends Sub implements Mixin {}";
    let (binder_a, parser_a) = parse_and_bind("a.ts", source_a);
    let (binder_b, parser_b) = parse_and_bind("b.ts", source_b);

    let mut index = SymbolIndex::new();
    index.index_file("a.ts", &binder_a, parser_a.get_arena(), source_a);
    index.index_file("b.ts", &binder_b, parser_b.get_arena(), source_b);

    let other_bases_before = index.get_bases_for_class("Other");
    assert!(
        other_bases_before.contains(&"Sub".to_string())
            && other_bases_before.contains(&"Mixin".to_string()),
        "precondition: Other should extend Sub and implement Mixin; got {other_bases_before:?}",
    );

    index.remove_file("a.ts");

    assert!(
        index.get_bases_for_class("Sub").is_empty(),
        "Sub was declared only in a.ts; its sub_to_bases entry should be gone"
    );

    let other_bases_after = index.get_bases_for_class("Other");
    assert!(
        other_bases_after.contains(&"Sub".to_string())
            && other_bases_after.contains(&"Mixin".to_string()),
        "Other was declared in b.ts; its sub_to_bases entry must survive \
         removal of unrelated file a.ts; got {other_bases_after:?}",
    );
}

#[test]
fn test_clear_resets_heritage_and_sub_to_bases() {
    // Regression: clear() used to leave heritage_clauses and sub_to_bases
    // populated, so a fully-rebuilt index would see stale class edges.
    let source = r#"class Base {} class Sub extends Base {}"#;
    let (binder, parser) = parse_and_bind("file.ts", source);

    let mut index = SymbolIndex::new();
    index.index_file("file.ts", &binder, parser.get_arena(), source);

    assert!(!index.get_files_with_heritage("Base").is_empty());
    assert!(!index.get_bases_for_class("Sub").is_empty());

    index.clear();

    assert!(
        index.get_files_with_heritage("Base").is_empty(),
        "heritage_clauses should be cleared"
    );
    assert!(
        index.get_bases_for_class("Sub").is_empty(),
        "sub_to_bases should be cleared"
    );
}
