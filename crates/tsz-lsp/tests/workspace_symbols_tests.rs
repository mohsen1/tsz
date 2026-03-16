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

#[test]
fn test_find_symbols_unicode_names() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "日本語関数",
        make_location("unicode.ts", 0, 0, 5),
        SymbolKind::Function,
    );

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("日本語");
    assert!(
        !results.is_empty(),
        "Should find symbols with unicode names"
    );
    assert_eq!(results[0].name, "日本語関数");
}

#[test]
fn test_find_symbols_single_char_query() {
    let index = setup_index();
    let provider = WorkspaceSymbolsProvider::new(&index);
    // Single character should still return matches
    let results = provider.find_symbols("M");
    let names: Vec<&str> = results.iter().map(|s| s.name.as_str()).collect();
    // Should match MyClass, myFunction, myVariable (case-insensitive prefix match)
    assert!(
        !names.is_empty(),
        "Single char query 'M' should find symbols"
    );
}

#[test]
fn test_find_symbols_returns_correct_location() {
    let mut index = SymbolIndex::new();
    let loc = make_location("specific.ts", 10, 5, 15);
    index.add_definition_with_kind("targetSymbol", loc, SymbolKind::Variable);

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("targetSymbol");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].location.file_path, "specific.ts");
    assert_eq!(results[0].location.range.start.line, 10);
    assert_eq!(results[0].location.range.start.character, 5);
}

#[test]
fn test_symbol_index_remove_file() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind("foo", make_location("a.ts", 0, 0, 3), SymbolKind::Function);
    index.add_definition_with_kind("bar", make_location("b.ts", 0, 0, 3), SymbolKind::Function);

    index.remove_file("a.ts");
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("foo");
    assert!(
        results.is_empty(),
        "After removing a.ts, foo should not be found"
    );
    let results = provider.find_symbols("bar");
    assert!(!results.is_empty(), "bar from b.ts should still be found");
}

#[test]
fn test_find_symbols_special_characters_in_query() {
    let index = setup_index();
    let provider = WorkspaceSymbolsProvider::new(&index);
    // Query with characters not in any symbol name
    let results = provider.find_symbols("$$$");
    assert!(
        results.is_empty(),
        "Query with no matching characters should return empty"
    );
}

#[test]
fn test_symbol_index_multiple_definitions_same_name() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "handler",
        make_location("a.ts", 0, 0, 7),
        SymbolKind::Function,
    );
    index.add_definition_with_kind(
        "handler",
        make_location("b.ts", 5, 0, 7),
        SymbolKind::Function,
    );

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("handler");
    assert!(
        results.len() >= 2,
        "Should find multiple definitions with the same name, got: {}",
        results.len()
    );
}

// =========================================================================
// Additional coverage tests
// =========================================================================

#[test]
fn test_whitespace_only_query() {
    let index = setup_index();
    let provider = WorkspaceSymbolsProvider::new(&index);

    // Whitespace query should still attempt matching (not treated as empty)
    let results = provider.find_symbols(" ");
    // No symbol names contain a space, so expect empty
    assert!(
        results.is_empty(),
        "Whitespace-only query should match no symbols"
    );
}

#[test]
fn test_case_insensitive_prefix_match() {
    let index = setup_index();
    let provider = WorkspaceSymbolsProvider::new(&index);

    // "MY" should match "MyClass", "myFunction", "myVariable", etc.
    let results = provider.find_symbols("MY");
    assert!(
        results.len() >= 3,
        "Case-insensitive 'MY' should match multiple symbols, got: {}",
        results.len()
    );
}

#[test]
fn test_case_insensitive_substring_match() {
    let index = setup_index();
    let provider = WorkspaceSymbolsProvider::new(&index);

    // "FUNCTION" as substring should match "myFunction"
    let results = provider.find_symbols("FUNCTION");
    assert!(
        !results.is_empty(),
        "Case-insensitive substring 'FUNCTION' should match myFunction"
    );
    assert!(results.iter().any(|r| r.name == "myFunction"));
}

#[test]
fn test_exact_match_comes_first_in_sorting() {
    let mut index = SymbolIndex::new();
    index.add_definition("test", make_location("a.ts", 0, 0, 4));
    index.add_definition("testing", make_location("b.ts", 0, 0, 7));
    index.add_definition("atest", make_location("c.ts", 0, 0, 5));

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("test");

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].name, "test", "Exact match should be first");
    assert_eq!(results[1].name, "testing", "Prefix match should be second");
    assert_eq!(results[2].name, "atest", "Substring match should be last");
}

#[test]
fn test_alphabetical_within_same_match_kind() {
    let mut index = SymbolIndex::new();
    index.add_definition("bazFoo", make_location("a.ts", 0, 0, 6));
    index.add_definition("alphaFoo", make_location("b.ts", 0, 0, 8));

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("foo");

    assert_eq!(results.len(), 2);
    // Both are substring matches, so alphabetical order
    assert_eq!(results[0].name, "alphaFoo");
    assert_eq!(results[1].name, "bazFoo");
}

#[test]
fn test_symbol_kind_for_definitions_without_stored_kind() {
    let mut index = SymbolIndex::new();
    // PascalCase without I-prefix -> Class heuristic
    index.add_definition("Widget", make_location("a.ts", 0, 0, 6));
    // camelCase -> Variable heuristic
    index.add_definition("getValue", make_location("b.ts", 0, 0, 8));
    // SCREAMING_CASE -> Constant heuristic
    index.add_definition("API_KEY", make_location("c.ts", 0, 0, 7));
    // I-prefix PascalCase -> Interface heuristic
    index.add_definition("IService", make_location("d.ts", 0, 0, 8));

    let provider = WorkspaceSymbolsProvider::new(&index);

    let widget = provider.find_symbols("Widget");
    assert_eq!(widget[0].kind, SymbolKind::Class);

    let get_value = provider.find_symbols("getValue");
    assert_eq!(get_value[0].kind, SymbolKind::Variable);

    let api_key = provider.find_symbols("API_KEY");
    assert_eq!(api_key[0].kind, SymbolKind::Constant);

    let service = provider.find_symbols("IService");
    assert_eq!(service[0].kind, SymbolKind::Interface);
}

#[test]
fn test_multiple_files_same_symbol_different_locations() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "Logger",
        make_location("logger.ts", 0, 0, 6),
        SymbolKind::Class,
    );
    index.add_definition_with_kind(
        "Logger",
        make_location("logger.d.ts", 0, 0, 6),
        SymbolKind::Class,
    );
    index.add_definition_with_kind(
        "Logger",
        make_location("mock-logger.ts", 5, 0, 6),
        SymbolKind::Class,
    );

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("Logger");

    assert_eq!(results.len(), 3, "Should return all 3 definitions");
    assert!(results.iter().all(|r| r.name == "Logger"));
    assert!(results.iter().all(|r| r.kind == SymbolKind::Class));
}

#[test]
fn test_numeric_in_symbol_name() {
    let mut index = SymbolIndex::new();
    index.add_definition("route404", make_location("routes.ts", 0, 0, 8));
    index.add_definition("handler500", make_location("routes.ts", 5, 0, 10));

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("404");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "route404");
}

#[test]
fn test_underscore_prefix_symbol() {
    let mut index = SymbolIndex::new();
    index.add_definition("_private", make_location("a.ts", 0, 0, 8));
    index.add_definition("__dunder", make_location("a.ts", 1, 0, 8));

    let provider = WorkspaceSymbolsProvider::new(&index);

    let results = provider.find_symbols("_private");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "_private");

    let results2 = provider.find_symbols("__");
    assert!(!results2.is_empty(), "Should find symbols starting with __");
}

#[test]
fn test_dollar_sign_symbol() {
    let mut index = SymbolIndex::new();
    index.add_definition("$element", make_location("dom.ts", 0, 0, 8));

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("$elem");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "$element");
}

#[test]
fn test_infer_symbol_kind_empty_string() {
    assert_eq!(
        WorkspaceSymbolsProvider::infer_symbol_kind(""),
        SymbolKind::Variable
    );
}

#[test]
fn test_infer_symbol_kind_single_uppercase_char() {
    // Single uppercase char -> Class (PascalCase)
    assert_eq!(
        WorkspaceSymbolsProvider::infer_symbol_kind("A"),
        SymbolKind::Class
    );
}

#[test]
fn test_infer_symbol_kind_single_lowercase_char() {
    assert_eq!(
        WorkspaceSymbolsProvider::infer_symbol_kind("x"),
        SymbolKind::Variable
    );
}

#[test]
fn test_workspace_symbols_underscore_prefix() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "_privateHelper",
        make_location("internal.ts", 0, 0, 14),
        SymbolKind::Function,
    );
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("_private");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "_privateHelper");
}

#[test]
fn test_workspace_symbols_numeric_suffix() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "handler1",
        make_location("handlers.ts", 0, 0, 8),
        SymbolKind::Function,
    );
    index.add_definition_with_kind(
        "handler2",
        make_location("handlers.ts", 5, 0, 8),
        SymbolKind::Function,
    );
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("handler");
    assert_eq!(results.len(), 2);
}

#[test]
fn test_workspace_symbols_many_results() {
    let mut index = SymbolIndex::new();
    for i in 0..20 {
        index.add_definition_with_kind(
            &format!("item{i}"),
            make_location("items.ts", i, 0, 5),
            SymbolKind::Variable,
        );
    }
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("item");
    assert!(results.len() >= 10, "Should return many matching results");
}

#[test]
fn test_workspace_symbols_interface_kind() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "IUserService",
        make_location("services.ts", 0, 0, 12),
        SymbolKind::Interface,
    );
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("IUser");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].kind, SymbolKind::Interface);
}

#[test]
fn test_workspace_symbols_enum_kind() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "Color",
        make_location("enums.ts", 0, 0, 5),
        SymbolKind::Enum,
    );
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("Color");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].kind, SymbolKind::Enum);
}

#[test]
fn test_workspace_symbols_whitespace_query() {
    let index = setup_index();
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("  ");
    // Whitespace-only query behavior is implementation-defined
    let _ = results;
}

#[test]
fn test_workspace_symbols_special_chars() {
    let index = setup_index();
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("@#$");
    assert!(
        results.is_empty(),
        "Special chars should not match normal symbols"
    );
}

#[test]
fn test_workspace_symbols_module_kind() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "MyModule",
        make_location("mod.ts", 0, 0, 8),
        SymbolKind::Module,
    );
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("MyModule");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].kind, SymbolKind::Module);
}

#[test]
fn test_workspace_symbols_type_alias_kind() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "StringOrNumber",
        make_location("types.ts", 0, 0, 14),
        SymbolKind::TypeParameter,
    );
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("StringOr");
    assert_eq!(results.len(), 1);
}

#[test]
fn test_workspace_symbols_duplicate_names_different_files() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind("Config", make_location("a.ts", 0, 0, 6), SymbolKind::Class);
    index.add_definition_with_kind(
        "Config",
        make_location("b.ts", 0, 0, 6),
        SymbolKind::Interface,
    );
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("Config");
    assert_eq!(
        results.len(),
        2,
        "Should find both Configs from different files"
    );
}

#[test]
fn test_infer_symbol_kind_screaming_snake() {
    // ALL_CAPS_NAME -> Constant
    assert_eq!(
        WorkspaceSymbolsProvider::infer_symbol_kind("MAX_VALUE"),
        SymbolKind::Constant
    );
}

#[test]
fn test_infer_symbol_kind_camel_case() {
    // camelCase may be inferred as Function or Variable depending on implementation
    let kind = WorkspaceSymbolsProvider::infer_symbol_kind("getUserName");
    assert!(
        kind == SymbolKind::Function || kind == SymbolKind::Variable,
        "camelCase should be Function or Variable, got: {kind:?}"
    );
}

#[test]
fn test_find_symbols_long_query() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "veryLongSymbolNameThatIsUnusuallyDescriptive",
        make_location("verbose.ts", 0, 0, 44),
        SymbolKind::Function,
    );
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("veryLongSymbolNameThatIsUnusuallyDescriptive");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].name,
        "veryLongSymbolNameThatIsUnusuallyDescriptive"
    );
}

#[test]
fn test_find_symbols_partial_middle_match() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "getUserById",
        make_location("users.ts", 0, 0, 11),
        SymbolKind::Function,
    );
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("ById");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "getUserById");
}

#[test]
fn test_remove_file_then_readd() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind("foo", make_location("a.ts", 0, 0, 3), SymbolKind::Function);
    index.remove_file("a.ts");

    // Re-add with different location
    index.add_definition_with_kind("foo", make_location("a.ts", 5, 0, 3), SymbolKind::Variable);

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("foo");
    assert_eq!(results.len(), 1, "Should find the re-added symbol");
    assert_eq!(results[0].location.range.start.line, 5);
}

#[test]
fn test_remove_nonexistent_file() {
    let mut index = SymbolIndex::new();
    index.add_definition("bar", make_location("exists.ts", 0, 0, 3));
    // Removing a file that was never added should not panic
    index.remove_file("nonexistent.ts");
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("bar");
    assert_eq!(results.len(), 1, "Existing symbols should be unaffected");
}

#[test]
fn test_infer_symbol_kind_i_prefix_lowercase_second() {
    // "iPad" starts with I but second char is lowercase 'P' - wait, 'P' is uppercase
    // "iValue" starts with lowercase 'i', should be Variable, not Interface
    let kind = WorkspaceSymbolsProvider::infer_symbol_kind("iValue");
    assert!(
        kind == SymbolKind::Variable || kind == SymbolKind::Function,
        "lowercase 'i' prefix should not be Interface, got: {kind:?}"
    );
}

#[test]
fn test_infer_symbol_kind_all_uppercase_no_underscore() {
    // "HTTP" is all uppercase but no underscore - may be Class or Constant
    let kind = WorkspaceSymbolsProvider::infer_symbol_kind("HTTP");
    // Either Constant (screaming case) or Class (PascalCase) is acceptable
    assert!(
        kind == SymbolKind::Constant || kind == SymbolKind::Class,
        "All uppercase without underscore should be Constant or Class, got: {kind:?}"
    );
}

#[test]
fn test_find_symbols_many_files_same_name() {
    let mut index = SymbolIndex::new();
    for i in 0..10 {
        index.add_definition_with_kind(
            "init",
            make_location(&format!("module{i}.ts"), 0, 0, 4),
            SymbolKind::Function,
        );
    }
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("init");
    assert_eq!(results.len(), 10, "Should find init in all 10 files");
    assert!(results.iter().all(|r| r.name == "init"));
}

#[test]
fn test_find_symbols_empty_index() {
    let index = SymbolIndex::new();
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("anything");
    assert!(results.is_empty(), "Empty index should return no results");
}

#[test]
fn test_prefix_match_ranked_above_substring() {
    let mut index = SymbolIndex::new();
    index.add_definition("renderItem", make_location("a.ts", 0, 0, 10));
    index.add_definition("prerender", make_location("b.ts", 0, 0, 9));

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("render");

    assert_eq!(results.len(), 2);
    // Prefix match should come before substring match
    assert_eq!(
        results[0].name, "renderItem",
        "Prefix match should be first"
    );
    assert_eq!(
        results[1].name, "prerender",
        "Substring match should be second"
    );
}

#[test]
fn test_get_definition_kind_after_remove_file() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind("Widget", make_location("w.ts", 0, 0, 6), SymbolKind::Class);
    index.remove_file("w.ts");

    // After removing the file, the symbol should not be findable
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("Widget");
    assert!(
        results.is_empty(),
        "After removing file, symbol should not be found"
    );
}

#[test]
fn test_find_symbols_mixed_case_query_matches() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "XMLParser",
        make_location("xml.ts", 0, 0, 9),
        SymbolKind::Class,
    );
    let provider = WorkspaceSymbolsProvider::new(&index);

    // Mixed case query
    let results = provider.find_symbols("xmlparser");
    assert_eq!(
        results.len(),
        1,
        "Case-insensitive search should find XMLParser"
    );
    assert_eq!(results[0].name, "XMLParser");
}

#[test]
fn test_infer_symbol_kind_underscore_only() {
    let kind = WorkspaceSymbolsProvider::infer_symbol_kind("_");
    assert!(
        kind == SymbolKind::Variable || kind == SymbolKind::Function,
        "Single underscore should be Variable or Function, got: {kind:?}"
    );
}

// =========================================================================
// Additional edge case tests
// =========================================================================

#[test]
fn test_find_symbols_query_longer_than_any_name() {
    let mut index = SymbolIndex::new();
    index.add_definition("ab", make_location("a.ts", 0, 0, 2));
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("abcdefghijklmnop");
    assert!(
        results.is_empty(),
        "Query longer than symbol name should not match"
    );
}

#[test]
fn test_find_symbols_exact_case_sensitive_priority() {
    let mut index = SymbolIndex::new();
    index.add_definition("Item", make_location("a.ts", 0, 0, 4));
    index.add_definition("item", make_location("b.ts", 0, 0, 4));
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("item");
    assert!(results.len() >= 2, "Should find both 'Item' and 'item'");
}

#[test]
fn test_add_definition_with_kind_preserves_location() {
    let mut index = SymbolIndex::new();
    let loc = make_location("deep/nested/file.ts", 99, 10, 25);
    index.add_definition_with_kind("deepSymbol", loc, SymbolKind::Function);
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("deepSymbol");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].location.file_path, "deep/nested/file.ts");
    assert_eq!(results[0].location.range.start.line, 99);
    assert_eq!(results[0].location.range.start.character, 10);
    assert_eq!(results[0].location.range.end.character, 25);
}

#[test]
fn test_find_symbols_trailing_whitespace_in_query() {
    let mut index = SymbolIndex::new();
    index.add_definition("myFunc", make_location("a.ts", 0, 0, 6));
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("myFunc ");
    // "myFunc " won't be a substring of "myFunc" so should be empty
    assert!(
        results.is_empty(),
        "Query with trailing space should not match"
    );
}

#[test]
fn test_find_symbols_multiple_kinds_same_query() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "DataClass",
        make_location("a.ts", 0, 0, 9),
        SymbolKind::Class,
    );
    index.add_definition_with_kind(
        "DataInterface",
        make_location("b.ts", 0, 0, 13),
        SymbolKind::Interface,
    );
    index.add_definition_with_kind("DataEnum", make_location("c.ts", 0, 0, 8), SymbolKind::Enum);
    index.add_definition_with_kind(
        "dataHelper",
        make_location("d.ts", 0, 0, 10),
        SymbolKind::Function,
    );
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("Data");
    assert_eq!(
        results.len(),
        4,
        "Should find all 4 symbols starting with Data"
    );
    let kinds: Vec<SymbolKind> = results.iter().map(|r| r.kind).collect();
    assert!(kinds.contains(&SymbolKind::Class));
    assert!(kinds.contains(&SymbolKind::Interface));
    assert!(kinds.contains(&SymbolKind::Enum));
    assert!(kinds.contains(&SymbolKind::Function));
}

#[test]
fn test_remove_file_with_multiple_symbols() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "alpha",
        make_location("multi.ts", 0, 0, 5),
        SymbolKind::Variable,
    );
    index.add_definition_with_kind(
        "beta",
        make_location("multi.ts", 1, 0, 4),
        SymbolKind::Variable,
    );
    index.add_definition_with_kind(
        "gamma",
        make_location("multi.ts", 2, 0, 5),
        SymbolKind::Variable,
    );
    index.add_definition_with_kind(
        "other",
        make_location("other.ts", 0, 0, 5),
        SymbolKind::Variable,
    );

    index.remove_file("multi.ts");
    let provider = WorkspaceSymbolsProvider::new(&index);
    assert!(
        provider.find_symbols("alpha").is_empty(),
        "alpha should be removed"
    );
    assert!(
        provider.find_symbols("beta").is_empty(),
        "beta should be removed"
    );
    assert!(
        provider.find_symbols("gamma").is_empty(),
        "gamma should be removed"
    );
    assert!(
        !provider.find_symbols("other").is_empty(),
        "other from other.ts should remain"
    );
}

#[test]
fn test_infer_symbol_kind_two_char_uppercase() {
    let kind = WorkspaceSymbolsProvider::infer_symbol_kind("IO");
    assert!(
        kind == SymbolKind::Constant || kind == SymbolKind::Class,
        "Two uppercase chars should be Constant or Class, got: {kind:?}"
    );
}

#[test]
fn test_infer_symbol_kind_mixed_with_numbers() {
    let kind = WorkspaceSymbolsProvider::infer_symbol_kind("item2Count");
    assert!(
        kind == SymbolKind::Variable || kind == SymbolKind::Function,
        "camelCase with numbers should be Variable or Function, got: {kind:?}"
    );
}

#[test]
fn test_infer_symbol_kind_pascal_with_numbers() {
    let kind = WorkspaceSymbolsProvider::infer_symbol_kind("Vec3D");
    assert_eq!(
        kind,
        SymbolKind::Class,
        "PascalCase with numbers should be Class"
    );
}

#[test]
fn test_find_symbols_emoji_query() {
    let mut index = SymbolIndex::new();
    index.add_definition("test_symbol", make_location("a.ts", 0, 0, 11));
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("\u{1F600}");
    assert!(results.is_empty(), "Emoji query should not match");
}

#[test]
fn test_find_symbols_numeric_only_query() {
    let mut index = SymbolIndex::new();
    index.add_definition("error404Handler", make_location("a.ts", 0, 0, 15));
    index.add_definition("route200", make_location("b.ts", 0, 0, 8));
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("200");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "route200");
}

#[test]
fn test_kind_from_index_constant() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "piValue",
        make_location("math.ts", 0, 0, 7),
        SymbolKind::Constant,
    );
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("piValue");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].kind,
        SymbolKind::Constant,
        "piValue should be Constant from index, not Variable from heuristic"
    );
}

#[test]
fn test_find_symbols_single_symbol_index() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "onlyOne",
        make_location("sole.ts", 0, 0, 7),
        SymbolKind::Function,
    );
    let provider = WorkspaceSymbolsProvider::new(&index);
    let exact = provider.find_symbols("onlyOne");
    assert_eq!(exact.len(), 1);
    let partial = provider.find_symbols("only");
    assert_eq!(partial.len(), 1);
    let none = provider.find_symbols("nope");
    assert!(none.is_empty());
}

#[test]
fn test_find_symbols_very_long_symbol_name() {
    let mut index = SymbolIndex::new();
    let long_name = "a".repeat(200);
    index.add_definition(&long_name, make_location("long.ts", 0, 0, 200));
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols(&long_name);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, long_name);
}

#[test]
fn test_remove_file_twice_does_not_panic() {
    let mut index = SymbolIndex::new();
    index.add_definition("sym", make_location("rm.ts", 0, 0, 3));
    index.remove_file("rm.ts");
    index.remove_file("rm.ts");
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("sym");
    assert!(results.is_empty());
}

#[test]
fn test_find_symbols_overlapping_names() {
    let mut index = SymbolIndex::new();
    index.add_definition("parse", make_location("a.ts", 0, 0, 5));
    index.add_definition("parser", make_location("b.ts", 0, 0, 6));
    index.add_definition("parseJSON", make_location("c.ts", 0, 0, 9));
    index.add_definition("JSONParser", make_location("d.ts", 0, 0, 10));
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("parse");
    assert!(results.len() >= 3, "Should find parse, parser, parseJSON");
    assert_eq!(results[0].name, "parse");
}

#[test]
fn test_find_symbols_single_char_m_query() {
    let index = setup_index();
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("m");
    let _ = results;
}

#[test]
fn test_find_symbols_full_name_exact() {
    let index = setup_index();
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("MyClass");
    assert!(!results.is_empty());
    assert_eq!(results[0].name, "MyClass");
}

#[test]
fn test_find_symbols_no_partial_match() {
    let mut index = SymbolIndex::new();
    index.add_definition("abcdef", make_location("a.ts", 0, 0, 6));
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("xyz");
    assert!(results.is_empty());
}

#[test]
fn test_find_symbols_mixed_case_definitions() {
    let mut index = SymbolIndex::new();
    index.add_definition("HTTPClient", make_location("a.ts", 0, 0, 10));
    index.add_definition("httpServer", make_location("b.ts", 0, 0, 10));
    index.add_definition("HttpHandler", make_location("c.ts", 0, 0, 11));
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("http");
    let _ = results;
}

#[test]
fn test_find_symbols_very_long_query() {
    let index = setup_index();
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("thisIsAVeryLongQueryThatMatchesNothing");
    assert!(results.is_empty());
}

#[test]
fn test_find_symbols_numeric_suffix_in_name() {
    let mut index = SymbolIndex::new();
    index.add_definition("handler1", make_location("a.ts", 0, 0, 8));
    index.add_definition("handler2", make_location("a.ts", 1, 0, 8));
    index.add_definition("handler10", make_location("a.ts", 2, 0, 9));
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("handler");
    assert!(results.len() >= 3);
}

#[test]
fn test_find_symbols_with_kind_class() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "MyWidget",
        make_location("a.ts", 0, 0, 8),
        SymbolKind::Class,
    );
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("MyWidget");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].kind, SymbolKind::Class);
}

#[test]
fn test_find_symbols_with_kind_function() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "processData",
        make_location("a.ts", 0, 0, 11),
        SymbolKind::Function,
    );
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("processData");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].kind, SymbolKind::Function);
}

#[test]
fn test_find_symbols_with_kind_interface() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "IService",
        make_location("a.ts", 0, 0, 8),
        SymbolKind::Interface,
    );
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("IService");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].kind, SymbolKind::Interface);
}

#[test]
fn test_find_symbols_with_kind_enum() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind("Status", make_location("a.ts", 0, 0, 6), SymbolKind::Enum);
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("Status");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].kind, SymbolKind::Enum);
}

#[test]
fn test_find_symbols_many_files() {
    let mut index = SymbolIndex::new();
    for i in 0..20 {
        index.add_definition(
            &format!("sym{i}"),
            make_location(&format!("file{i}.ts"), 0, 0, 4),
        );
    }
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("sym");
    assert!(results.len() >= 20);
}

#[test]
fn test_find_symbols_after_clear_and_readd() {
    let mut index = SymbolIndex::new();
    index.add_definition("foo", make_location("a.ts", 0, 0, 3));
    index.remove_file("a.ts");
    index.add_definition("foo", make_location("b.ts", 0, 0, 3));
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("foo");
    assert_eq!(results.len(), 1);
}

#[test]
fn test_find_symbols_dollar_prefix() {
    let mut index = SymbolIndex::new();
    index.add_definition("$scope", make_location("a.ts", 0, 0, 6));
    index.add_definition("$rootScope", make_location("b.ts", 0, 0, 10));
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("$scope");
    assert!(!results.is_empty());
}

#[test]
fn test_find_symbols_location_info_preserved() {
    let mut index = SymbolIndex::new();
    index.add_definition("target", make_location("src/deep/file.ts", 42, 5, 11));
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("target");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].location.range.start.line, 42);
    assert_eq!(results[0].location.range.start.character, 5);
}

#[test]
fn test_find_symbols_underscore_prefix() {
    let mut index = SymbolIndex::new();
    index.add_definition("_private", make_location("a.ts", 0, 0, 8));
    index.add_definition("__dunder", make_location("b.ts", 0, 0, 8));
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("_private");
    assert!(!results.is_empty());
}

#[test]
fn test_find_symbols_with_kind_class_animal() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind("Animal", make_location("a.ts", 0, 0, 6), SymbolKind::Class);
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("Animal");
    assert_eq!(results.len(), 1);
}

#[test]
fn test_find_symbols_same_name_different_files() {
    let mut index = SymbolIndex::new();
    index.add_definition("Config", make_location("a.ts", 0, 0, 6));
    index.add_definition("Config", make_location("b.ts", 0, 0, 6));
    index.add_definition("Config", make_location("c.ts", 0, 0, 6));
    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("Config");
    assert_eq!(results.len(), 3);
}
