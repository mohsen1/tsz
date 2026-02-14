use super::*;
use tsz_common::position::{Position, Range};

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
