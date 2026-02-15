use super::*;
use tsz_common::position::{Position, Range};

fn make_location(file: &str, line: u32, start_col: u32, end_col: u32) -> Location {
    Location::new(
        file.to_string(),
        Range::new(Position::new(line, start_col), Position::new(line, end_col)),
    )
}

fn setup_index() -> SymbolIndex {
    let mut index = SymbolIndex::new();

    // Use add_definition_with_kind to supply accurate binder-derived kinds
    index.add_definition_with_kind(
        "MyClass",
        make_location("types.ts", 0, 0, 7),
        SymbolKind::Class,
    );
    index.add_definition_with_kind(
        "myFunction",
        make_location("utils.ts", 5, 0, 10),
        SymbolKind::Function,
    );
    index.add_definition_with_kind(
        "myVariable",
        make_location("main.ts", 0, 0, 10),
        SymbolKind::Variable,
    );
    index.add_definition_with_kind(
        "IMyInterface",
        make_location("types.ts", 10, 0, 12),
        SymbolKind::Interface,
    );
    index.add_definition_with_kind(
        "MAX_COUNT",
        make_location("constants.ts", 0, 0, 9),
        SymbolKind::Constant,
    );
    index.add_definition_with_kind(
        "helper",
        make_location("utils.ts", 20, 0, 6),
        SymbolKind::Function,
    );
    index.add_definition_with_kind(
        "MyOtherClass",
        make_location("types.ts", 30, 0, 12),
        SymbolKind::Class,
    );

    index
}

#[test]
fn test_empty_query_returns_empty() {
    let index = setup_index();
    let provider = WorkspaceSymbolsProvider::new(&index);

    let results = provider.find_symbols("");
    assert!(results.is_empty(), "Empty query should return no results");
}

#[test]
fn test_exact_match() {
    let index = setup_index();
    let provider = WorkspaceSymbolsProvider::new(&index);

    let results = provider.find_symbols("helper");
    assert!(!results.is_empty(), "Should find 'helper'");
    assert_eq!(results[0].name, "helper");
}

#[test]
fn test_case_insensitive_exact_match() {
    let index = setup_index();
    let provider = WorkspaceSymbolsProvider::new(&index);

    let results = provider.find_symbols("HELPER");
    assert!(
        !results.is_empty(),
        "Should find 'helper' case-insensitively"
    );
    assert_eq!(results[0].name, "helper");
}

#[test]
fn test_prefix_match() {
    let index = setup_index();
    let provider = WorkspaceSymbolsProvider::new(&index);

    let results = provider.find_symbols("my");
    assert!(
        results.len() >= 3,
        "Should find myFunction, myVariable, MyClass, MyOtherClass"
    );

    // All results should contain "my" (case-insensitive)
    for result in &results {
        assert!(
            result.name.to_lowercase().contains("my"),
            "'{}' should contain 'my'",
            result.name
        );
    }
}

#[test]
fn test_substring_match() {
    let index = setup_index();
    let provider = WorkspaceSymbolsProvider::new(&index);

    let results = provider.find_symbols("class");
    assert!(
        results.len() >= 2,
        "Should find MyClass and MyOtherClass via substring match"
    );

    for result in &results {
        assert!(
            result.name.to_lowercase().contains("class"),
            "'{}' should contain 'class'",
            result.name
        );
    }
}

#[test]
fn test_no_match() {
    let index = setup_index();
    let provider = WorkspaceSymbolsProvider::new(&index);

    let results = provider.find_symbols("nonexistent");
    assert!(
        results.is_empty(),
        "Should find no results for nonexistent query"
    );
}

#[test]
fn test_relevance_sorting() {
    let mut index = SymbolIndex::new();

    // Add symbols that will match "foo" with different relevance
    index.add_definition("foo", make_location("a.ts", 0, 0, 3)); // exact
    index.add_definition("fooBar", make_location("b.ts", 0, 0, 6)); // prefix
    index.add_definition("barfoo", make_location("c.ts", 0, 0, 6)); // substring

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("foo");

    assert_eq!(results.len(), 3);
    // Exact match first
    assert_eq!(results[0].name, "foo");
    // Prefix match second
    assert_eq!(results[1].name, "fooBar");
    // Substring match last
    assert_eq!(results[2].name, "barfoo");
}

#[test]
fn test_max_results_limit() {
    let mut index = SymbolIndex::new();

    // Add more than MAX_RESULTS (100) symbols
    for i in 0..150 {
        let name = format!("symbol_{i}");
        index.add_definition(&name, make_location("test.ts", i, 0, 10));
    }

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("symbol");

    assert_eq!(
        results.len(),
        MAX_RESULTS,
        "Should limit to {MAX_RESULTS} results"
    );
}

#[test]
fn test_infer_symbol_kind_class() {
    assert_eq!(
        WorkspaceSymbolsProvider::infer_symbol_kind("MyClass"),
        SymbolKind::Class
    );
}

#[test]
fn test_infer_symbol_kind_interface() {
    assert_eq!(
        WorkspaceSymbolsProvider::infer_symbol_kind("IMyInterface"),
        SymbolKind::Interface
    );
}

#[test]
fn test_infer_symbol_kind_constant() {
    assert_eq!(
        WorkspaceSymbolsProvider::infer_symbol_kind("MAX_COUNT"),
        SymbolKind::Constant
    );
}

#[test]
fn test_infer_symbol_kind_variable() {
    assert_eq!(
        WorkspaceSymbolsProvider::infer_symbol_kind("myVariable"),
        SymbolKind::Variable
    );
}

#[test]
fn test_symbol_information_fields() {
    let index = setup_index();
    let provider = WorkspaceSymbolsProvider::new(&index);

    let results = provider.find_symbols("MyClass");
    assert_eq!(results.len(), 1);

    let sym = &results[0];
    assert_eq!(sym.name, "MyClass");
    assert_eq!(sym.kind, SymbolKind::Class);
    assert_eq!(sym.location.file_path, "types.ts");
    assert_eq!(sym.location.range.start.line, 0);
    assert_eq!(sym.location.range.start.character, 0);
    assert_eq!(sym.location.range.end.character, 7);
}

#[test]
fn test_multiple_definitions_same_name() {
    let mut index = SymbolIndex::new();

    // Same symbol defined in two files (e.g., declaration + .d.ts)
    index.add_definition("Config", make_location("config.ts", 0, 0, 6));
    index.add_definition("Config", make_location("config.d.ts", 0, 0, 6));

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("Config");

    assert_eq!(results.len(), 2, "Should return both definitions");
    assert!(results.iter().all(|r| r.name == "Config"));
}

// =========================================================================
// Binder-data-driven kind tests
// =========================================================================

#[test]
fn test_kind_from_index_function() {
    // A function named with camelCase would be misclassified as Variable
    // by the heuristic, but the index stores the correct kind.
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "processItems",
        make_location("utils.ts", 0, 0, 12),
        SymbolKind::Function,
    );

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("processItems");

    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].kind,
        SymbolKind::Function,
        "processItems should be Function from index, not Variable from heuristic"
    );
}

#[test]
fn test_kind_from_index_interface_without_i_prefix() {
    // An interface named "Point" (no I-prefix) would be misclassified as
    // Class by the heuristic. The index should report it correctly.
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "Point",
        make_location("types.ts", 0, 0, 5),
        SymbolKind::Interface,
    );

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("Point");

    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].kind,
        SymbolKind::Interface,
        "Point should be Interface from index, not Class from heuristic"
    );
}

#[test]
fn test_kind_from_index_enum() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "Color",
        make_location("types.ts", 5, 0, 5),
        SymbolKind::Enum,
    );

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("Color");

    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].kind,
        SymbolKind::Enum,
        "Color should be Enum from index, not Class from heuristic"
    );
}

#[test]
fn test_kind_from_index_module() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "MyNamespace",
        make_location("ns.ts", 0, 0, 11),
        SymbolKind::Module,
    );

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("MyNamespace");

    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].kind,
        SymbolKind::Module,
        "MyNamespace should be Module from index, not Class from heuristic"
    );
}

#[test]
fn test_kind_from_index_type_alias() {
    // A type alias like `type Result<T> = ...` would be misclassified as
    // Class by the heuristic (PascalCase). The index stores TypeParameter
    // (the LSP SymbolKind used for type aliases in this codebase).
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "Result",
        make_location("types.ts", 0, 0, 6),
        SymbolKind::TypeParameter,
    );

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("Result");

    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].kind,
        SymbolKind::TypeParameter,
        "Result should be TypeParameter from index, not Class from heuristic"
    );
}

#[test]
fn test_kind_fallback_to_heuristic_when_no_kind_stored() {
    // When add_definition is used (without kind), the heuristic should
    // kick in as a fallback.
    let mut index = SymbolIndex::new();
    index.add_definition("MyWidget", make_location("ui.ts", 0, 0, 8));

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("MyWidget");

    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].kind,
        SymbolKind::Class,
        "MyWidget should fall back to Class from PascalCase heuristic"
    );
}

#[test]
fn test_kind_index_overrides_heuristic() {
    // Verify that a stored kind wins even when the heuristic would
    // produce a different answer.
    let mut index = SymbolIndex::new();
    // "MAX_RETRIES" looks like a SCREAMING_CASE constant, but is actually
    // a function (unlikely but tests the override mechanism).
    index.add_definition_with_kind(
        "MAX_RETRIES",
        make_location("config.ts", 0, 0, 11),
        SymbolKind::Function,
    );

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("MAX_RETRIES");

    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].kind,
        SymbolKind::Function,
        "Index kind should override heuristic"
    );
}

#[test]
fn test_setup_index_function_kind() {
    // Verify that the updated setup_index correctly reports functions.
    // Previously, "myFunction" would have been Variable (heuristic);
    // now the index stores Function.
    let index = setup_index();
    let provider = WorkspaceSymbolsProvider::new(&index);

    let results = provider.find_symbols("myFunction");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].kind,
        SymbolKind::Function,
        "myFunction should be Function, not Variable"
    );
}

#[test]
fn test_setup_index_helper_is_function() {
    let index = setup_index();
    let provider = WorkspaceSymbolsProvider::new(&index);

    let results = provider.find_symbols("helper");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].kind,
        SymbolKind::Function,
        "helper should be Function from index"
    );
}

#[test]
fn test_get_definition_kind_returns_none_without_kind() {
    let mut index = SymbolIndex::new();
    index.add_definition("foo", make_location("a.ts", 0, 0, 3));
    assert_eq!(
        index.get_definition_kind("foo"),
        None,
        "add_definition without kind should not store a kind"
    );
}

#[test]
fn test_get_definition_kind_returns_some_with_kind() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind("bar", make_location("b.ts", 0, 0, 3), SymbolKind::Enum);
    assert_eq!(
        index.get_definition_kind("bar"),
        Some(SymbolKind::Enum),
        "add_definition_with_kind should store the kind"
    );
}
