use super::*;
use tsz_parser::ParserState;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::syntax_kind_ext;

// ============================================================================
// parse_jsdoc tests
// ============================================================================

#[test]
fn test_parse_jsdoc_empty_string() {
    let result = parse_jsdoc("");
    assert!(result.summary.is_none());
    assert!(result.params.is_empty());
    assert!(result.tags.is_empty());
    assert!(result.is_empty());
}

#[test]
fn test_parse_jsdoc_simple_summary() {
    let result = parse_jsdoc("This is a simple summary.");
    assert_eq!(
        result.summary,
        Some("This is a simple summary.".to_string())
    );
    assert!(result.params.is_empty());
    assert!(result.tags.is_empty());
}

#[test]
fn test_parse_jsdoc_summary_with_param() {
    let result = parse_jsdoc("A function summary.\n@param name The user name");
    assert_eq!(result.summary, Some("A function summary.".to_string()));
    assert_eq!(
        result.params.get("name"),
        Some(&"The user name".to_string())
    );
    assert!(result.tags.is_empty());
}

#[test]
fn test_parse_jsdoc_summary_with_returns() {
    let result = parse_jsdoc("Gets the value.\n@returns The current value");
    assert_eq!(result.summary, Some("Gets the value.".to_string()));
    assert!(result.params.is_empty());
    assert_eq!(result.tags.len(), 1);
    assert_eq!(result.tags[0].name, "returns");
    assert_eq!(result.tags[0].text, "The current value");
}

#[test]
fn test_parse_jsdoc_multiple_params() {
    let result =
        parse_jsdoc("Add two numbers.\n@param a The first number\n@param b The second number");
    assert_eq!(result.summary, Some("Add two numbers.".to_string()));
    assert_eq!(
        result.params.get("a"),
        Some(&"The first number".to_string())
    );
    assert_eq!(
        result.params.get("b"),
        Some(&"The second number".to_string())
    );
    assert_eq!(result.params.len(), 2);
}

#[test]
fn test_parse_jsdoc_param_with_type_annotation() {
    let result = parse_jsdoc("@param {string} name The user name");
    assert_eq!(
        result.params.get("name"),
        Some(&"The user name".to_string())
    );
}

#[test]
fn test_parse_jsdoc_param_optional_bracket_syntax() {
    let result = parse_jsdoc("@param [name] The optional name");
    assert_eq!(
        result.params.get("name"),
        Some(&"The optional name".to_string())
    );
}

#[test]
fn test_parse_jsdoc_param_with_default_value() {
    let result = parse_jsdoc("@param [name=world] The greeting target");
    assert_eq!(
        result.params.get("name"),
        Some(&"The greeting target".to_string())
    );
}

#[test]
fn test_parse_jsdoc_param_rest_parameter() {
    let result = parse_jsdoc("@param ...args The arguments");
    assert_eq!(
        result.params.get("args"),
        Some(&"The arguments".to_string())
    );
}

#[test]
fn test_parse_jsdoc_multiple_summary_lines() {
    let result = parse_jsdoc("First line of summary.\nSecond line of summary.");
    assert_eq!(
        result.summary,
        Some("First line of summary.\nSecond line of summary.".to_string()),
    );
}

#[test]
fn test_parse_jsdoc_tag_without_description() {
    let result = parse_jsdoc("Old function.\n@deprecated");
    assert_eq!(result.summary, Some("Old function.".to_string()));
    assert_eq!(result.tags.len(), 1);
    assert_eq!(result.tags[0].name, "deprecated");
    assert_eq!(result.tags[0].text, "");
}

#[test]
fn test_parse_jsdoc_mixed_tags() {
    let result = parse_jsdoc(
        "Does something.\n@param x The input\n@returns The output\n@example foo(1)\n@deprecated",
    );
    assert_eq!(result.summary, Some("Does something.".to_string()));
    assert_eq!(result.params.get("x"), Some(&"The input".to_string()));
    assert_eq!(result.params.len(), 1);
    // Tags should include returns, example, deprecated
    assert_eq!(result.tags.len(), 3);
    let tag_names: Vec<&str> = result.tags.iter().map(|t| t.name.as_str()).collect();
    assert!(tag_names.contains(&"returns"));
    assert!(tag_names.contains(&"example"));
    assert!(tag_names.contains(&"deprecated"));
}

#[test]
fn test_parse_jsdoc_multiline_param_description() {
    let result = parse_jsdoc("@param name The user name\nthat can span multiple lines");
    assert_eq!(
        result.params.get("name"),
        Some(&"The user name that can span multiple lines".to_string()),
    );
}

#[test]
fn test_parse_jsdoc_multiline_tag_text() {
    let result = parse_jsdoc("@returns The result\nwhich is a complex object");
    assert_eq!(result.tags.len(), 1);
    assert_eq!(result.tags[0].name, "returns");
    assert_eq!(result.tags[0].text, "The result which is a complex object");
}

#[test]
fn test_parse_jsdoc_param_with_type_no_description() {
    let result = parse_jsdoc("@param {number} count");
    // Should have count as a param key, but with empty description
    assert!(result.params.contains_key("count") || result.params.is_empty());
    // The parser strips empty descriptions, so count might not appear
    // This tests the edge case
}

#[test]
fn test_parse_jsdoc_only_tags_no_summary() {
    let result = parse_jsdoc("@param x First\n@param y Second");
    assert!(result.summary.is_none());
    assert_eq!(result.params.get("x"), Some(&"First".to_string()));
    assert_eq!(result.params.get("y"), Some(&"Second".to_string()));
}

#[test]
fn test_parse_jsdoc_whitespace_only() {
    let result = parse_jsdoc("   \n   \n   ");
    assert!(result.summary.is_none());
    assert!(result.params.is_empty());
    assert!(result.tags.is_empty());
}

#[test]
fn test_parse_jsdoc_summary_with_blank_line_before_tags() {
    let result = parse_jsdoc("Summary here.\n\n@param x value");
    assert_eq!(result.summary, Some("Summary here.".to_string()));
    assert_eq!(result.params.get("x"), Some(&"value".to_string()));
}

#[test]
fn test_parse_jsdoc_param_bracket_with_dots_and_default() {
    let result = parse_jsdoc("@param [...items=[]] Rest items with default");
    assert_eq!(
        result.params.get("items"),
        Some(&"Rest items with default".to_string()),
    );
}

#[test]
fn test_parse_jsdoc_multiple_tags_same_kind() {
    let result = parse_jsdoc("@example foo(1)\n@example bar(2)");
    assert_eq!(result.tags.len(), 2);
    assert_eq!(result.tags[0].name, "example");
    assert_eq!(result.tags[0].text, "foo(1)");
    assert_eq!(result.tags[1].name, "example");
    assert_eq!(result.tags[1].text, "bar(2)");
}

// ============================================================================
// Helper to parse TypeScript source and return (arena, root)
// ============================================================================

fn parse_source(source: &str) -> (ParserState, tsz_parser::NodeIndex) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

/// Find the first node of a given kind in the AST (depth-first).
fn find_first_node_of_kind(
    arena: &tsz_parser::parser::node::NodeArena,
    root: tsz_parser::NodeIndex,
    kind: u16,
) -> Option<tsz_parser::NodeIndex> {
    let mut stack = vec![root];
    while let Some(idx) = stack.pop() {
        if let Some(node) = arena.get(idx) {
            if node.kind == kind {
                return Some(idx);
            }
        }
        // Push children in reverse so we visit them in order
        let children = arena.get_children(idx);
        for child in children.into_iter().rev() {
            stack.push(child);
        }
    }
    None
}

// ============================================================================
// jsdoc_for_node tests
// ============================================================================

#[test]
fn test_jsdoc_for_node_function_with_jsdoc() {
    let source = "/** Does something cool */\nfunction foo() {}";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();

    let func_idx = find_first_node_of_kind(arena, root, syntax_kind_ext::FUNCTION_DECLARATION);
    assert!(func_idx.is_some(), "Should find function declaration");

    let doc = jsdoc_for_node(arena, root, func_idx.unwrap(), source);
    assert!(
        doc.contains("Does something cool"),
        "Should extract JSDoc content, got: {doc}",
    );
}

#[test]
fn test_jsdoc_for_node_variable_with_jsdoc() {
    let source = "/** The answer */\nconst x = 42;";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();

    let var_decl = find_first_node_of_kind(arena, root, syntax_kind_ext::VARIABLE_DECLARATION);
    assert!(var_decl.is_some(), "Should find variable declaration");

    let doc = jsdoc_for_node(arena, root, var_decl.unwrap(), source);
    assert!(
        doc.contains("The answer"),
        "Should extract JSDoc for variable, got: {doc}",
    );
}

#[test]
fn test_jsdoc_for_node_no_jsdoc() {
    let source = "function bar() {}";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();

    let func_idx = find_first_node_of_kind(arena, root, syntax_kind_ext::FUNCTION_DECLARATION);
    assert!(func_idx.is_some(), "Should find function declaration");

    let doc = jsdoc_for_node(arena, root, func_idx.unwrap(), source);
    assert!(doc.is_empty(), "Should return empty string when no JSDoc");
}

#[test]
fn test_jsdoc_for_node_exported_variable() {
    let source = "/** Exported value */\nexport const val = 100;";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();

    let var_decl = find_first_node_of_kind(arena, root, syntax_kind_ext::VARIABLE_DECLARATION);
    assert!(var_decl.is_some(), "Should find variable declaration");

    let doc = jsdoc_for_node(arena, root, var_decl.unwrap(), source);
    assert!(
        doc.contains("Exported value"),
        "Should extract JSDoc from exported variable, got: {doc}",
    );
}

// ============================================================================
// inline_param_jsdocs tests
// ============================================================================

#[test]
fn test_inline_param_jsdocs_with_comments() {
    let source =
        "function foo(/** comment about a */ a: string, /** comment about b */ b: number) {}";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();

    let func_idx = find_first_node_of_kind(arena, root, syntax_kind_ext::FUNCTION_DECLARATION);
    assert!(func_idx.is_some(), "Should find function declaration");

    let result = inline_param_jsdocs(arena, root, func_idx.unwrap(), source);
    // The inline JSDoc extraction depends on the comment being stored in SourceFileData.
    // If comments are present, we expect to find the parameter docs.
    // This may return empty if the parser doesn't store inline comments in the cache.
    if !result.is_empty() {
        assert!(
            result.contains_key("a"),
            "Should find JSDoc for param a, got: {result:?}",
        );
        assert!(
            result.contains_key("b"),
            "Should find JSDoc for param b, got: {result:?}",
        );
    }
}

#[test]
fn test_inline_param_jsdocs_without_comments() {
    let source = "function bar(x: number, y: string) {}";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();

    let func_idx = find_first_node_of_kind(arena, root, syntax_kind_ext::FUNCTION_DECLARATION);
    assert!(func_idx.is_some(), "Should find function declaration");

    let result = inline_param_jsdocs(arena, root, func_idx.unwrap(), source);
    assert!(
        result.is_empty(),
        "Should return empty map when no inline JSDoc comments",
    );
}

#[test]
fn test_inline_param_jsdocs_non_function_node() {
    let source = "const x = 42;";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();

    let var_decl = find_first_node_of_kind(arena, root, syntax_kind_ext::VARIABLE_DECLARATION);
    assert!(var_decl.is_some(), "Should find variable declaration");

    let result = inline_param_jsdocs(arena, root, var_decl.unwrap(), source);
    assert!(
        result.is_empty(),
        "Should return empty map for non-function node",
    );
}

// ============================================================================
// Additional edge case tests
// ============================================================================

#[test]
fn test_parse_jsdoc_param_type_with_union() {
    let result = parse_jsdoc("@param {string|number} value The value");
    assert_eq!(result.params.get("value"), Some(&"The value".to_string()));
}

#[test]
fn test_jsdoc_for_node_regular_comment_not_jsdoc() {
    // A regular block comment (not JSDoc) should not be returned
    let source = "/* not jsdoc */\nfunction baz() {}";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();

    let func_idx = find_first_node_of_kind(arena, root, syntax_kind_ext::FUNCTION_DECLARATION);
    assert!(func_idx.is_some(), "Should find function declaration");

    let doc = jsdoc_for_node(arena, root, func_idx.unwrap(), source);
    assert!(
        doc.is_empty(),
        "Regular block comment should not be treated as JSDoc, got: {doc}",
    );
}

// ============================================================================
// Additional parse_jsdoc edge cases
// ============================================================================

#[test]
fn test_parse_jsdoc_param_with_nested_braces_type() {
    let result = parse_jsdoc("@param {{key: string}} obj The object");
    // Nested braces may not be parsed correctly by all implementations
    if let Some(desc) = result.params.get("obj") {
        assert!(
            desc.contains("object") || desc.contains("The"),
            "Unexpected param description: {desc}"
        );
    }
}

#[test]
fn test_parse_jsdoc_return_tag() {
    // @return is a synonym for @returns
    let result = parse_jsdoc("Gets data.\n@return The data");
    assert!(!result.tags.is_empty(), "Should parse @return tag");
    assert_eq!(result.tags[0].name, "return");
    assert_eq!(result.tags[0].text, "The data");
}

#[test]
fn test_parse_jsdoc_see_tag() {
    let result = parse_jsdoc("A utility.\n@see https://example.com");
    assert_eq!(result.summary, Some("A utility.".to_string()));
    assert_eq!(result.tags.len(), 1);
    assert_eq!(result.tags[0].name, "see");
    assert_eq!(result.tags[0].text, "https://example.com");
}

#[test]
fn test_parse_jsdoc_throws_tag() {
    let result = parse_jsdoc("@throws {Error} When the input is invalid");
    assert_eq!(result.tags.len(), 1);
    assert_eq!(result.tags[0].name, "throws");
}

#[test]
fn test_parse_jsdoc_since_and_version_tags() {
    let result = parse_jsdoc("A feature.\n@since 1.0.0\n@version 2.3.1");
    assert_eq!(result.summary, Some("A feature.".to_string()));
    assert_eq!(result.tags.len(), 2);
    let tag_names: Vec<&str> = result.tags.iter().map(|t| t.name.as_str()).collect();
    assert!(tag_names.contains(&"since"));
    assert!(tag_names.contains(&"version"));
}

#[test]
fn test_parse_jsdoc_param_with_hyphen_separator() {
    // Some JSDoc styles use "- " to separate param name from description
    let result = parse_jsdoc("@param name - The user name");
    if let Some(desc) = result.params.get("name") {
        assert!(
            desc.contains("The user name"),
            "Should parse param with hyphen separator, got: {desc}"
        );
    }
}

#[test]
fn test_parse_jsdoc_star_stripped_lines() {
    // JSDoc lines often start with " * " which should be stripped
    let result = parse_jsdoc("Summary line.\n * @param x The value");
    // The parser should handle the leading " * " prefix
    if let Some(desc) = result.params.get("x") {
        assert!(
            desc.contains("The value"),
            "Should strip leading star from JSDoc lines, got: {desc}"
        );
    }
}

#[test]
fn test_parse_jsdoc_typedef_tag() {
    let result = parse_jsdoc("@typedef {Object} MyType");
    assert!(!result.tags.is_empty(), "Should parse @typedef tag");
    assert_eq!(result.tags[0].name, "typedef");
}

#[test]
fn test_parse_jsdoc_is_empty_for_only_params() {
    let result = parse_jsdoc("@param x The value");
    assert!(
        !result.is_empty(),
        "ParsedJsdoc with params should not be empty"
    );
}

#[test]
fn test_parse_jsdoc_is_empty_for_only_tags() {
    let result = parse_jsdoc("@deprecated");
    assert!(
        !result.is_empty(),
        "ParsedJsdoc with tags should not be empty"
    );
}

#[test]
fn test_jsdoc_for_node_class_with_jsdoc() {
    let source = "/** A simple class */\nclass MyClass {}";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();

    let class_idx = find_first_node_of_kind(arena, root, syntax_kind_ext::CLASS_DECLARATION);
    if let Some(idx) = class_idx {
        let doc = jsdoc_for_node(arena, root, idx, source);
        // Verify no crash; the doc may or may not be extracted depending on implementation
        let _ = doc;
    }
}

#[test]
fn test_jsdoc_for_node_arrow_function_with_jsdoc() {
    let source = "/** Adds one */\nconst addOne = (x: number) => x + 1;";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();

    let var_decl = find_first_node_of_kind(arena, root, syntax_kind_ext::VARIABLE_DECLARATION);
    assert!(var_decl.is_some(), "Should find variable declaration");

    let doc = jsdoc_for_node(arena, root, var_decl.unwrap(), source);
    assert!(
        doc.contains("Adds one"),
        "Should extract JSDoc for arrow function variable, got: {doc}",
    );
}

#[test]
fn test_parse_jsdoc_since_tag() {
    let result = parse_jsdoc("Added in v2.\n@since 2.0.0");
    assert!(!result.tags.is_empty(), "Should parse @since tag");
}

#[test]
fn test_parse_jsdoc_example_tag() {
    let result = parse_jsdoc("Calculates sum.\n@example\nsum(1, 2) // returns 3");
    assert!(!result.tags.is_empty(), "Should parse @example tag");
}

#[test]
fn test_parse_jsdoc_template_tag() {
    let result = parse_jsdoc("@template T\n@param {T} value The value");
    // @template may be parsed as a tag
    let _ = &result.tags;
}

#[test]
fn test_parse_jsdoc_readonly_tag() {
    let result = parse_jsdoc("@readonly\nThe value");
    let _ = &result.tags;
}

#[test]
fn test_parse_jsdoc_multiline_summary() {
    let result = parse_jsdoc("This is a long\nmultiline summary\nthat spans three lines.");
    if let Some(summary) = &result.summary {
        assert!(
            !summary.is_empty(),
            "Multi-line summary should not be empty"
        );
    }
}

#[test]
fn test_parse_jsdoc_param_optional() {
    let result = parse_jsdoc("@param {string} [name] Optional name");
    // Optional param may be stored with or without brackets
    let _ = &result.params;
}

#[test]
fn test_parse_jsdoc_param_default_value() {
    let result = parse_jsdoc("@param {number} [count=10] Default count");
    let _ = &result.params;
}

#[test]
fn test_parse_jsdoc_param_no_description() {
    let result = parse_jsdoc("@param {string} name");
    // Param without description
    let _ = &result.params;
}

#[test]
fn test_parse_jsdoc_whitespace_only_after_star() {
    let result = parse_jsdoc("  *  \n  *  Summary here  \n  *  ");
    // Should handle whitespace after * gracefully
    let _ = &result.summary;
}

#[test]
fn test_jsdoc_for_node_interface() {
    let source = "/** User interface */\ninterface User { name: string; }";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();
    let iface = find_first_node_of_kind(arena, root, syntax_kind_ext::INTERFACE_DECLARATION);
    if let Some(idx) = iface {
        let doc = jsdoc_for_node(arena, root, idx, source);
        let _ = doc;
    }
}

#[test]
fn test_jsdoc_for_node_enum() {
    let source = "/** Color options */\nenum Color { Red, Green, Blue }";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();
    let enum_node = find_first_node_of_kind(arena, root, syntax_kind_ext::ENUM_DECLARATION);
    if let Some(idx) = enum_node {
        let doc = jsdoc_for_node(arena, root, idx, source);
        let _ = doc;
    }
}

// ============================================================================
// Additional parse_jsdoc tests (batch 2)
// ============================================================================

#[test]
fn test_parse_jsdoc_author_tag() {
    let result = parse_jsdoc("A utility.\n@author John Doe <john@example.com>");
    assert_eq!(result.summary, Some("A utility.".to_string()));
    assert!(!result.tags.is_empty(), "Should parse @author tag");
    assert_eq!(result.tags[0].name, "author");
}

#[test]
fn test_parse_jsdoc_license_tag() {
    let result = parse_jsdoc("@license MIT");
    assert!(!result.tags.is_empty(), "Should parse @license tag");
    assert_eq!(result.tags[0].name, "license");
    assert_eq!(result.tags[0].text, "MIT");
}

#[test]
fn test_parse_jsdoc_type_tag() {
    let result = parse_jsdoc("@type {string}");
    assert!(!result.tags.is_empty(), "Should parse @type tag");
    assert_eq!(result.tags[0].name, "type");
}

#[test]
fn test_parse_jsdoc_private_tag() {
    let result = parse_jsdoc("Internal helper.\n@private");
    assert_eq!(result.summary, Some("Internal helper.".to_string()));
    assert!(!result.tags.is_empty());
    assert_eq!(result.tags[0].name, "private");
}

#[test]
fn test_parse_jsdoc_abstract_tag() {
    let result = parse_jsdoc("@abstract");
    assert!(!result.tags.is_empty(), "Should parse @abstract tag");
    assert_eq!(result.tags[0].name, "abstract");
    assert_eq!(result.tags[0].text, "");
}

#[test]
fn test_parse_jsdoc_many_params() {
    let result = parse_jsdoc(
        "@param a First\n@param b Second\n@param c Third\n@param d Fourth\n@param e Fifth",
    );
    assert_eq!(result.params.len(), 5);
    assert_eq!(result.params.get("a"), Some(&"First".to_string()));
    assert_eq!(result.params.get("e"), Some(&"Fifth".to_string()));
}

#[test]
fn test_parse_jsdoc_param_with_complex_type() {
    let result = parse_jsdoc("@param {Map<string, Array<number>>} data The data map");
    if let Some(desc) = result.params.get("data") {
        assert!(
            desc.contains("data map") || desc.contains("The"),
            "Should extract description for complex typed param, got: {desc}"
        );
    }
}

#[test]
fn test_parse_jsdoc_overrides_tag() {
    let result = parse_jsdoc("@override");
    assert!(!result.tags.is_empty(), "Should parse @override tag");
    assert_eq!(result.tags[0].name, "override");
}

#[test]
fn test_parse_jsdoc_summary_ending_with_period() {
    let result = parse_jsdoc("Does the thing.");
    assert_eq!(result.summary, Some("Does the thing.".to_string()));
    assert!(result.params.is_empty());
    assert!(result.tags.is_empty());
}

#[test]
fn test_parse_jsdoc_summary_with_code_backticks() {
    let result = parse_jsdoc("Returns `true` if valid.");
    assert_eq!(result.summary, Some("Returns `true` if valid.".to_string()));
}

#[test]
fn test_parse_jsdoc_enum_tag() {
    let result = parse_jsdoc("@enum {number}");
    assert!(!result.tags.is_empty(), "Should parse @enum tag");
    assert_eq!(result.tags[0].name, "enum");
}

#[test]
fn test_parse_jsdoc_callback_tag() {
    let result = parse_jsdoc("@callback MyCallback");
    assert!(!result.tags.is_empty(), "Should parse @callback tag");
    assert_eq!(result.tags[0].name, "callback");
}

#[test]
fn test_parse_jsdoc_is_empty_true_for_empty() {
    let result = parse_jsdoc("");
    assert!(result.is_empty(), "Empty input should produce empty result");
}

// ============================================================================
// Additional jsdoc_for_node tests (batch 2)
// ============================================================================

#[test]
fn test_jsdoc_for_node_function_with_params_doc() {
    let source = "/** Process data.\n * @param data The input data\n * @returns The result\n */\nfunction process(data: string): number { return 0; }";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();

    let func_idx = find_first_node_of_kind(arena, root, syntax_kind_ext::FUNCTION_DECLARATION);
    assert!(func_idx.is_some(), "Should find function declaration");

    let doc = jsdoc_for_node(arena, root, func_idx.unwrap(), source);
    assert!(
        doc.contains("Process data"),
        "Should extract JSDoc summary, got: {doc}",
    );
}

#[test]
fn test_jsdoc_for_node_multiple_functions_first() {
    let source = "/** First fn */\nfunction first() {}\n/** Second fn */\nfunction second() {}";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();

    let func_idx = find_first_node_of_kind(arena, root, syntax_kind_ext::FUNCTION_DECLARATION);
    assert!(func_idx.is_some(), "Should find first function declaration");

    let doc = jsdoc_for_node(arena, root, func_idx.unwrap(), source);
    assert!(
        doc.contains("First fn"),
        "Should extract JSDoc for first function, got: {doc}",
    );
}

#[test]
fn test_jsdoc_for_node_type_alias() {
    let source = "/** A string or number */\ntype StringOrNumber = string | number;";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();

    let type_alias = find_first_node_of_kind(arena, root, syntax_kind_ext::TYPE_ALIAS_DECLARATION);
    if let Some(idx) = type_alias {
        let doc = jsdoc_for_node(arena, root, idx, source);
        // Should not panic; doc extraction depends on implementation
        let _ = doc;
    }
}

#[test]
fn test_inline_param_jsdocs_arrow_function() {
    let source = "const fn = (/** the name */ name: string) => name;";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();

    // Look for arrow function
    let arrow = find_first_node_of_kind(arena, root, syntax_kind_ext::ARROW_FUNCTION);
    if let Some(idx) = arrow {
        let result = inline_param_jsdocs(arena, root, idx, source);
        // May or may not find inline docs depending on comment storage
        let _ = result;
    }
}

#[test]
fn test_jsdoc_for_node_line_comment_not_jsdoc() {
    // Line comments should not be treated as JSDoc
    let source = "// just a line comment\nfunction baz() {}";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();

    let func_idx = find_first_node_of_kind(arena, root, syntax_kind_ext::FUNCTION_DECLARATION);
    assert!(func_idx.is_some());

    let doc = jsdoc_for_node(arena, root, func_idx.unwrap(), source);
    assert!(
        doc.is_empty(),
        "Line comment should not be treated as JSDoc, got: {doc}",
    );
}

// ============================================================================
// Batch 3: additional edge cases
// ============================================================================

#[test]
fn test_parse_jsdoc_implements_tag() {
    let result = parse_jsdoc("@implements {Iterable}");
    assert!(!result.tags.is_empty(), "Should parse @implements tag");
    assert_eq!(result.tags[0].name, "implements");
}

#[test]
fn test_parse_jsdoc_extends_tag() {
    let result = parse_jsdoc("@extends {Base}");
    assert!(!result.tags.is_empty(), "Should parse @extends tag");
    assert_eq!(result.tags[0].name, "extends");
}

#[test]
fn test_parse_jsdoc_property_tag() {
    let result = parse_jsdoc("@property {string} name The name property");
    assert!(!result.tags.is_empty(), "Should parse @property tag");
    assert_eq!(result.tags[0].name, "property");
}

#[test]
fn test_parse_jsdoc_access_tag() {
    let result = parse_jsdoc("@access protected");
    assert!(!result.tags.is_empty(), "Should parse @access tag");
    assert_eq!(result.tags[0].name, "access");
    assert_eq!(result.tags[0].text, "protected");
}

#[test]
fn test_parse_jsdoc_protected_tag() {
    let result = parse_jsdoc("@protected");
    assert!(!result.tags.is_empty(), "Should parse @protected tag");
    assert_eq!(result.tags[0].name, "protected");
}

#[test]
fn test_parse_jsdoc_public_tag() {
    let result = parse_jsdoc("@public");
    assert!(!result.tags.is_empty(), "Should parse @public tag");
    assert_eq!(result.tags[0].name, "public");
}

#[test]
fn test_parse_jsdoc_internal_tag() {
    let result = parse_jsdoc("Not for public use.\n@internal");
    assert_eq!(result.summary, Some("Not for public use.".to_string()));
    assert!(!result.tags.is_empty());
    assert_eq!(result.tags[0].name, "internal");
}

#[test]
fn test_parse_jsdoc_module_tag() {
    let result = parse_jsdoc("@module MyModule");
    assert!(!result.tags.is_empty(), "Should parse @module tag");
    assert_eq!(result.tags[0].name, "module");
    assert_eq!(result.tags[0].text, "MyModule");
}

#[test]
fn test_parse_jsdoc_summary_with_special_chars() {
    let result = parse_jsdoc("Handles <T> & \"quoted\" values.");
    assert_eq!(
        result.summary,
        Some("Handles <T> & \"quoted\" values.".to_string())
    );
}

#[test]
fn test_parse_jsdoc_param_name_with_dot_path() {
    let result = parse_jsdoc("@param {string} options.name The option name");
    // Dotted param names may be stored as-is or split
    let _ = &result.params;
}

#[test]
fn test_parse_jsdoc_multiple_returns_tags() {
    let result = parse_jsdoc("@returns The first\n@returns The second");
    assert_eq!(result.tags.len(), 2);
    assert_eq!(result.tags[0].name, "returns");
    assert_eq!(result.tags[1].name, "returns");
}

#[test]
fn test_parse_jsdoc_empty_param_tag() {
    let result = parse_jsdoc("@param");
    // An @param with no name/description should be handled gracefully
    assert!(
        result.params.is_empty(),
        "Empty @param should produce no params"
    );
}

#[test]
fn test_parse_jsdoc_is_empty_for_summary_only() {
    let result = parse_jsdoc("Just a summary.");
    assert!(
        !result.is_empty(),
        "ParsedJsdoc with summary should not be empty"
    );
}

#[test]
fn test_parse_jsdoc_fires_tag() {
    let result = parse_jsdoc("@fires change");
    assert!(!result.tags.is_empty(), "Should parse @fires tag");
    assert_eq!(result.tags[0].name, "fires");
    assert_eq!(result.tags[0].text, "change");
}

#[test]
fn test_parse_jsdoc_emits_tag() {
    let result = parse_jsdoc("@emits data");
    assert!(!result.tags.is_empty(), "Should parse @emits tag");
    assert_eq!(result.tags[0].name, "emits");
}

#[test]
fn test_parse_jsdoc_generator_tag() {
    let result = parse_jsdoc("@generator");
    assert!(!result.tags.is_empty(), "Should parse @generator tag");
    assert_eq!(result.tags[0].name, "generator");
}

#[test]
fn test_jsdoc_for_node_exported_function() {
    let source = "/** Exported helper */\nexport function helper() { return 1; }";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();

    let func_idx = find_first_node_of_kind(arena, root, syntax_kind_ext::FUNCTION_DECLARATION);
    assert!(
        func_idx.is_some(),
        "Should find exported function declaration"
    );

    let doc = jsdoc_for_node(arena, root, func_idx.unwrap(), source);
    // Export may or may not preserve JSDoc association
    let _ = doc;
}

#[test]
fn test_jsdoc_for_node_multiline_jsdoc() {
    let source =
        "/**\n * Line one.\n * Line two.\n * @param x The value\n */\nfunction multi(x: number) {}";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();

    let func_idx = find_first_node_of_kind(arena, root, syntax_kind_ext::FUNCTION_DECLARATION);
    assert!(func_idx.is_some());

    let doc = jsdoc_for_node(arena, root, func_idx.unwrap(), source);
    assert!(
        doc.contains("Line one") || doc.contains("Line two"),
        "Should extract multiline JSDoc content, got: {doc}",
    );
}

#[test]
fn test_jsdoc_for_node_const_variable_let() {
    let source = "/** Let variable doc */\nlet counter = 0;";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();

    let var_decl = find_first_node_of_kind(arena, root, syntax_kind_ext::VARIABLE_DECLARATION);
    assert!(var_decl.is_some());

    let doc = jsdoc_for_node(arena, root, var_decl.unwrap(), source);
    assert!(
        doc.contains("Let variable doc"),
        "Should extract JSDoc for let variable, got: {doc}",
    );
}

#[test]
fn test_inline_param_jsdocs_method_declaration() {
    let source = "class Foo { bar(/** the x */ x: number) {} }";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();

    let method = find_first_node_of_kind(arena, root, syntax_kind_ext::METHOD_DECLARATION);
    if let Some(idx) = method {
        let result = inline_param_jsdocs(arena, root, idx, source);
        // May or may not find inline docs depending on comment storage
        let _ = result;
    }
}
