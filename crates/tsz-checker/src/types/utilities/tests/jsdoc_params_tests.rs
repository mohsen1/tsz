use super::*;

#[test]
fn extract_nested_params_basic_object() {
    let jsdoc = r#"
 * @param {Object} opts doc
 * @param {string} opts.x doc2
 * @param {number} opts.y doc3
        "#;
    let result = CheckerState::extract_jsdoc_nested_param_properties(jsdoc, "opts");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], ("x".to_string(), "string".to_string(), false));
    assert_eq!(result[1], ("y".to_string(), "number".to_string(), false));
}

#[test]
fn extract_nested_params_optional_bracket() {
    let jsdoc = r#"
 * @param {Object} opts
 * @param {string} opts.x
 * @param {string=} opts.y
 * @param {string} [opts.z]
 * @param {string} [opts.w="hi"]
        "#;
    let result = CheckerState::extract_jsdoc_nested_param_properties(jsdoc, "opts");
    assert_eq!(result.len(), 4);
    assert_eq!(result[0].0, "x");
    assert!(!result[0].2); // not optional
    assert_eq!(result[1].0, "y");
    assert_eq!(result[1].1, "string="); // = suffix preserved for caller to handle
    assert!(!result[1].2);
    assert_eq!(result[2].0, "z");
    assert!(result[2].2); // bracket optional
    assert_eq!(result[3].0, "w");
    assert!(result[3].2); // bracket + default optional
}

#[test]
fn extract_nested_params_array_element() {
    let jsdoc = r#"
 * @param {Object[]} opts2
 * @param {string} opts2[].anotherX
 * @param {string=} opts2[].anotherY
        "#;
    let result = CheckerState::extract_jsdoc_nested_param_properties(jsdoc, "opts2");
    assert_eq!(result.len(), 2);
    assert_eq!(
        result[0],
        ("anotherX".to_string(), "string".to_string(), false)
    );
    assert_eq!(
        result[1],
        ("anotherY".to_string(), "string=".to_string(), false)
    );
}

#[test]
fn extract_nested_params_skips_deep_nesting() {
    let jsdoc = r#"
 * @param {object[]} opts5
 * @param {string} opts5[].help
 * @param {object} opts5[].what
 * @param {string} opts5[].what.a
 * @param {Object[]} opts5[].what.bad
 * @param {string} opts5[].what.bad[].idea
        "#;
    let result = CheckerState::extract_jsdoc_nested_param_properties(jsdoc, "opts5");
    // Only immediate children: help, what
    // Deeper nesting (what.a, what.bad, what.bad[].idea) should be skipped
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].0, "help");
    assert_eq!(result[1].0, "what");
}

#[test]
fn extract_nested_params_no_children() {
    let jsdoc = r#"
 * @param {string} name
 * @param {number} age
        "#;
    let result = CheckerState::extract_jsdoc_nested_param_properties(jsdoc, "name");
    assert!(result.is_empty());
}

#[test]
fn extract_nested_params_wrong_parent() {
    let jsdoc = r#"
 * @param {Object} opts1
 * @param {string} opts1.x
 * @param {Object} opts2
 * @param {number} opts2.y
        "#;
    let result = CheckerState::extract_jsdoc_nested_param_properties(jsdoc, "opts1");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].0, "x");
}

#[test]
fn extract_jsdoc_param_type_expr_with_span() {
    let (expr, offset) =
        CheckerState::extract_jsdoc_param_type_expr_with_span("@param {C} p", "p").unwrap();
    assert_eq!(expr, "C");
    assert_eq!(offset, 8);
}

#[test]
fn jsdoc_class_template_emits_ts2314_without_type_args() {
    let diags = crate::test_utils::check_js_source_diagnostics(
        r#"/**
 * @template T
 */
class C {}

/**
 * @param {C} p
 */
function f(p) {}
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2314),
        "Expected TS2314 for generic JSDoc class template without type arguments: codes={:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn is_plain_jsdoc_type_name_checks_identifier_shape() {
    assert!(CheckerState::is_plain_jsdoc_type_name("C"));
    assert!(CheckerState::is_plain_jsdoc_type_name("_Value2"));
    assert!(!CheckerState::is_plain_jsdoc_type_name("foo.bar"));
    assert!(!CheckerState::is_plain_jsdoc_type_name("Promise<T>"));
    assert!(!CheckerState::is_plain_jsdoc_type_name("C<T>[]"));
}

#[test]
fn extract_param_names_basic() {
    let jsdoc = r#"
 * @param {string} name
 * @param {number} age
        "#;
    let names = CheckerState::extract_jsdoc_param_names(jsdoc);
    assert_eq!(names.len(), 2);
    assert_eq!(names[0].0, "name");
    assert_eq!(names[1].0, "age");
}

#[test]
fn extract_param_names_with_dotted_nested() {
    // Dotted names (nested params) should be filtered out — only top-level names
    let jsdoc = r#"
 * @param {Object} error
 * @param {string} error.reason
 * @param {string} error.code
        "#;
    let names = CheckerState::extract_jsdoc_param_names(jsdoc);
    assert_eq!(names.len(), 1);
    assert_eq!(names[0].0, "error");
}

#[test]
fn extract_param_names_multiple_with_nested() {
    // Multiple top-level params where one has nested properties
    let jsdoc = r#"
 * @param {Object} opts
 * @param {string} opts.name
 * @param {number} count
        "#;
    let names = CheckerState::extract_jsdoc_param_names(jsdoc);
    assert_eq!(names.len(), 2);
    assert_eq!(names[0].0, "opts");
    assert_eq!(names[1].0, "count");
}

#[test]
fn extract_param_names_rest_param() {
    let jsdoc = r#"
 * @param {...string} args
        "#;
    let names = CheckerState::extract_jsdoc_param_names(jsdoc);
    assert_eq!(names.len(), 1);
    assert_eq!(names[0].0, "args");
}

#[test]
fn extract_param_names_optional_bracket() {
    let jsdoc = r#"
 * @param {string} [name]
 * @param {number} [age=25]
        "#;
    let names = CheckerState::extract_jsdoc_param_names(jsdoc);
    assert_eq!(names.len(), 2);
    assert_eq!(names[0].0, "name");
    assert_eq!(names[1].0, "age");
}

/// Test that positional matching works for destructured params in class methods.
/// The `@param {Object} error` at index 0 should match a destructured `{reason, code}`
/// at parameter position 0.
#[test]
fn jsdoc_positional_matching_class_method_no_ts7031() {
    let diags = crate::test_utils::check_js_source_diagnostics(
        r#"class X {
    /**
     * @param {Object} error
     * @param {string} error.reason
     * @param {string} error.code
     */
    cancel({reason, code}) {}
}
"#,
    );
    let ts7031_diags: Vec<_> = diags.iter().filter(|d| d.code == 7031).collect();
    assert!(
        ts7031_diags.is_empty(),
        "Expected no TS7031 for destructured param with JSDoc @param, got: {ts7031_diags:?}"
    );
}

/// Test that positional matching works for standalone function declarations.
#[test]
fn jsdoc_positional_matching_function_decl_no_ts7031() {
    let diags = crate::test_utils::check_js_source_diagnostics(
        r#"/**
 * @param {Object} opts
 * @param {string} opts.name
 */
function f({name}) {}
"#,
    );
    let ts7031_diags: Vec<_> = diags.iter().filter(|d| d.code == 7031).collect();
    assert!(
        ts7031_diags.is_empty(),
        "Expected no TS7031 for destructured param with JSDoc @param, got: {ts7031_diags:?}"
    );
}

// =========================================================================
// jsdoc_has_param_type
// =========================================================================

#[test]
fn jsdoc_has_param_type_present() {
    let jsdoc = r#"
 * @param {string} name
        "#;
    assert!(CheckerState::jsdoc_has_param_type(jsdoc, "name"));
}

#[test]
fn jsdoc_has_param_type_absent() {
    let jsdoc = r#"
 * @param {string} name
        "#;
    assert!(!CheckerState::jsdoc_has_param_type(jsdoc, "age"));
}

#[test]
fn jsdoc_has_param_type_no_type_annotation() {
    // @param without {type} should not count as having a type
    let jsdoc = r#"
 * @param name
        "#;
    assert!(!CheckerState::jsdoc_has_param_type(jsdoc, "name"));
}

// =========================================================================
// jsdoc_has_required_param_tag
// =========================================================================

#[test]
fn jsdoc_has_required_param_tag_standard() {
    let jsdoc = r#"
 * @param {string} name
        "#;
    assert!(CheckerState::jsdoc_has_required_param_tag(jsdoc, "name"));
}

#[test]
fn jsdoc_has_required_param_tag_name_only() {
    // @param name (no type) is considered required
    let jsdoc = r#"
 * @param name
        "#;
    assert!(CheckerState::jsdoc_has_required_param_tag(jsdoc, "name"));
}

#[test]
fn jsdoc_has_required_param_tag_optional_bracket() {
    // [name] means optional
    let jsdoc = r#"
 * @param {string} [name]
        "#;
    assert!(!CheckerState::jsdoc_has_required_param_tag(jsdoc, "name"));
}

#[test]
fn jsdoc_has_required_param_tag_optional_bracket_with_default() {
    // [name=default] means optional
    let jsdoc = r#"
 * @param {string} [name="hello"]
        "#;
    assert!(!CheckerState::jsdoc_has_required_param_tag(jsdoc, "name"));
}

#[test]
fn jsdoc_has_required_param_tag_optional_type_suffix() {
    // {Type=} means optional
    let jsdoc = r#"
 * @param {string=} name
        "#;
    assert!(!CheckerState::jsdoc_has_required_param_tag(jsdoc, "name"));
}

#[test]
fn jsdoc_has_required_param_tag_wrong_name() {
    let jsdoc = r#"
 * @param {string} other
        "#;
    assert!(!CheckerState::jsdoc_has_required_param_tag(jsdoc, "name"));
}

// =========================================================================
// extract_jsdoc_param_type_string
// =========================================================================

#[test]
fn extract_param_type_string_basic() {
    let jsdoc = r#"
 * @param {string} name
        "#;
    let result = CheckerState::extract_jsdoc_param_type_string(jsdoc, "name");
    assert_eq!(result, Some("string".to_string()));
}

#[test]
fn extract_param_type_string_complex_type() {
    let jsdoc = r#"
 * @param {Object.<string, boolean>} map
        "#;
    let result = CheckerState::extract_jsdoc_param_type_string(jsdoc, "map");
    assert_eq!(result, Some("Object.<string, boolean>".to_string()));
}

#[test]
fn extract_param_type_string_not_found() {
    let jsdoc = r#"
 * @param {number} count
        "#;
    assert!(CheckerState::extract_jsdoc_param_type_string(jsdoc, "name").is_none());
}

#[test]
fn extract_param_type_string_union_type() {
    let jsdoc = r#"
 * @param {string|number} value
        "#;
    let result = CheckerState::extract_jsdoc_param_type_string(jsdoc, "value");
    assert_eq!(result, Some("string|number".to_string()));
}

#[test]
fn extract_param_type_string_multiple_params() {
    let jsdoc = r#"
 * @param {string} first
 * @param {number} second
 * @param {boolean} third
        "#;
    assert_eq!(
        CheckerState::extract_jsdoc_param_type_string(jsdoc, "first"),
        Some("string".to_string())
    );
    assert_eq!(
        CheckerState::extract_jsdoc_param_type_string(jsdoc, "second"),
        Some("number".to_string())
    );
    assert_eq!(
        CheckerState::extract_jsdoc_param_type_string(jsdoc, "third"),
        Some("boolean".to_string())
    );
}

// =========================================================================
// jsdoc_has_type_annotations
// =========================================================================

#[test]
fn jsdoc_has_type_annotations_with_param_type() {
    let jsdoc = "@param {string} name";
    assert!(CheckerState::jsdoc_has_type_annotations(jsdoc));
}

#[test]
fn jsdoc_has_type_annotations_with_returns_type() {
    let jsdoc = "@returns {number}";
    assert!(CheckerState::jsdoc_has_type_annotations(jsdoc));
}

#[test]
fn jsdoc_has_type_annotations_with_return_type() {
    // @return is also valid (alias for @returns)
    let jsdoc = "@return {void}";
    assert!(CheckerState::jsdoc_has_type_annotations(jsdoc));
}

#[test]
fn jsdoc_has_type_annotations_with_type_tag() {
    let jsdoc = "@type {Function}";
    assert!(CheckerState::jsdoc_has_type_annotations(jsdoc));
}

#[test]
fn jsdoc_has_type_annotations_with_template() {
    let jsdoc = "@template T";
    assert!(CheckerState::jsdoc_has_type_annotations(jsdoc));
}

#[test]
fn jsdoc_has_type_annotations_none() {
    let jsdoc = "Just a description of the function";
    assert!(!CheckerState::jsdoc_has_type_annotations(jsdoc));
}

#[test]
fn jsdoc_has_type_annotations_param_without_type() {
    // @param without {type} does NOT count
    let jsdoc = "@param name";
    assert!(!CheckerState::jsdoc_has_type_annotations(jsdoc));
}

// =========================================================================
// jsdoc_extract_type_tag_expr
// =========================================================================

#[test]
fn jsdoc_extract_type_tag_expr_braceless_function_type() {
    let jsdoc = "@type function (number): string";
    assert_eq!(
        CheckerState::jsdoc_extract_type_tag_expr_braceless(jsdoc),
        Some("function (number): string".to_string())
    );
}

#[test]
fn jsdoc_extract_type_tag_expr_braceless_missing_type() {
    let jsdoc = "@type";
    assert_eq!(
        CheckerState::jsdoc_extract_type_tag_expr_braceless(jsdoc),
        None
    );
}

#[test]
fn jsdoc_extract_type_tag_expr_simple() {
    let jsdoc = "* @type {Cb}";
    assert_eq!(
        CheckerState::jsdoc_extract_type_tag_expr(jsdoc),
        Some("Cb".to_string())
    );
}

#[test]
fn jsdoc_extract_type_tag_expr_complex() {
    let jsdoc = "* @type {(x: string) => number}";
    assert_eq!(
        CheckerState::jsdoc_extract_type_tag_expr(jsdoc),
        Some("(x: string) => number".to_string())
    );
}

#[test]
fn jsdoc_extract_type_tag_expr_absent() {
    let jsdoc = "* @param {string} name";
    assert!(CheckerState::jsdoc_extract_type_tag_expr(jsdoc).is_none());
}

#[test]
fn jsdoc_extract_type_tag_expr_no_closing_brace() {
    let jsdoc = "* @type {unclosed";
    assert!(CheckerState::jsdoc_extract_type_tag_expr(jsdoc).is_none());
}

// =========================================================================
// jsdoc_template_type_params
// =========================================================================

/// Helper to extract just names from template params (for backward-compat tests).
fn names_only(params: &[(String, bool)]) -> Vec<&str> {
    params.iter().map(|(n, _)| n.as_str()).collect()
}

#[test]
fn jsdoc_template_single() {
    let jsdoc = "* @template T";
    let params = CheckerState::jsdoc_template_type_params(jsdoc);
    assert_eq!(names_only(&params), vec!["T"]);
    assert!(!params[0].1); // not const
}

#[test]
fn jsdoc_template_multiple_comma() {
    let jsdoc = "* @template T, U";
    let params = CheckerState::jsdoc_template_type_params(jsdoc);
    assert_eq!(names_only(&params), vec!["T", "U"]);
}

#[test]
fn jsdoc_template_multiple_space() {
    let jsdoc = "* @template T U";
    let params = CheckerState::jsdoc_template_type_params(jsdoc);
    assert_eq!(names_only(&params), vec!["T", "U"]);
}

#[test]
fn jsdoc_template_multiple_lines() {
    let jsdoc = "* @template T\n* @template U";
    let params = CheckerState::jsdoc_template_type_params(jsdoc);
    assert_eq!(names_only(&params), vec!["T", "U"]);
}

#[test]
fn jsdoc_template_no_duplicates() {
    let jsdoc = "* @template T\n* @template T";
    let params = CheckerState::jsdoc_template_type_params(jsdoc);
    assert_eq!(names_only(&params), vec!["T"]);
}

#[test]
fn jsdoc_template_none() {
    let jsdoc = "* @param {string} name";
    let params = CheckerState::jsdoc_template_type_params(jsdoc);
    assert!(params.is_empty());
}

#[test]
fn jsdoc_template_with_underscores() {
    let jsdoc = "* @template _T, $U";
    let params = CheckerState::jsdoc_template_type_params(jsdoc);
    assert_eq!(names_only(&params), vec!["_T", "$U"]);
}

#[test]
fn jsdoc_template_ignores_brace_form_for_binding() {
    let jsdoc = "* @template {T}";
    let params = CheckerState::jsdoc_template_type_params(jsdoc);
    assert!(params.is_empty());
}

#[test]
fn jsdoc_template_const_modifier() {
    let jsdoc = "* @template const T";
    let params = CheckerState::jsdoc_template_type_params(jsdoc);
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].0, "T");
    assert!(params[0].1); // is_const
}

#[test]
fn jsdoc_template_const_modifier_multiple() {
    // `@template const T, U` — both T and U are const per tsc
    let jsdoc = "* @template const T, U";
    let params = CheckerState::jsdoc_template_type_params(jsdoc);
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].0, "T");
    assert!(params[0].1); // const
    assert_eq!(params[1].0, "U");
    assert!(params[1].1); // const
}

#[test]
fn jsdoc_template_const_on_separate_line_not_shared() {
    // const on line 1 does NOT affect line 2
    let jsdoc = "* @template const T\n* @template U";
    let params = CheckerState::jsdoc_template_type_params(jsdoc);
    assert_eq!(params.len(), 2);
    assert!(params[0].1); // T is const
    assert!(!params[1].1); // U is not const
}

// =========================================================================
// jsdoc_returns_type_name
// =========================================================================

#[test]
fn jsdoc_returns_type_name_simple() {
    let jsdoc = "* @returns {number}";
    assert_eq!(
        CheckerState::jsdoc_returns_type_name(jsdoc),
        Some("number".to_string())
    );
}

#[test]
fn jsdoc_returns_type_name_return_alias() {
    // @return is accepted as well as @returns
    let jsdoc = "* @return {string}";
    assert_eq!(
        CheckerState::jsdoc_returns_type_name(jsdoc),
        Some("string".to_string())
    );
}

#[test]
fn jsdoc_returns_type_name_complex_returns_none() {
    // Complex types (containing non-identifier chars) return None
    let jsdoc = "* @returns {Promise<string>}";
    assert!(CheckerState::jsdoc_returns_type_name(jsdoc).is_none());
}

#[test]
fn jsdoc_returns_type_name_union_returns_none() {
    let jsdoc = "* @returns {string|number}";
    assert!(CheckerState::jsdoc_returns_type_name(jsdoc).is_none());
}

#[test]
fn jsdoc_returns_type_name_absent() {
    let jsdoc = "* @param {string} name";
    assert!(CheckerState::jsdoc_returns_type_name(jsdoc).is_none());
}

#[test]
fn jsdoc_returns_type_name_void() {
    let jsdoc = "* @returns {void}";
    assert_eq!(
        CheckerState::jsdoc_returns_type_name(jsdoc),
        Some("void".to_string())
    );
}

// =========================================================================
// jsdoc_returns_type_predicate
// =========================================================================

#[test]
fn jsdoc_returns_type_predicate_basic() {
    let jsdoc = "* @returns {x is string}";
    let result = CheckerState::jsdoc_returns_type_predicate(jsdoc);
    let (is_asserts, param_name, type_str) = result.unwrap();
    assert!(!is_asserts);
    assert_eq!(param_name, "x");
    assert_eq!(type_str, Some("string".to_string()));
}

#[test]
fn jsdoc_returns_type_predicate_this() {
    let jsdoc = "* @returns {this is Entry}";
    let result = CheckerState::jsdoc_returns_type_predicate(jsdoc);
    let (is_asserts, param_name, type_str) = result.unwrap();
    assert!(!is_asserts);
    assert_eq!(param_name, "this");
    assert_eq!(type_str, Some("Entry".to_string()));
}

#[test]
fn jsdoc_returns_type_predicate_asserts() {
    let jsdoc = "* @returns {asserts x is string}";
    let result = CheckerState::jsdoc_returns_type_predicate(jsdoc);
    let (is_asserts, param_name, type_str) = result.unwrap();
    assert!(is_asserts);
    assert_eq!(param_name, "x");
    assert_eq!(type_str, Some("string".to_string()));
}

#[test]
fn jsdoc_returns_type_predicate_asserts_only() {
    // "asserts x" without " is Type"
    let jsdoc = "* @returns {asserts x}";
    let result = CheckerState::jsdoc_returns_type_predicate(jsdoc);
    let (is_asserts, param_name, type_str) = result.unwrap();
    assert!(is_asserts);
    assert_eq!(param_name, "x");
    assert!(type_str.is_none());
}

#[test]
fn jsdoc_returns_type_predicate_none_for_regular_type() {
    let jsdoc = "* @returns {string}";
    assert!(CheckerState::jsdoc_returns_type_predicate(jsdoc).is_none());
}

#[test]
fn jsdoc_returns_type_predicate_return_alias() {
    // @return works too
    let jsdoc = "* @return {x is number}";
    let result = CheckerState::jsdoc_returns_type_predicate(jsdoc);
    assert!(result.is_some());
    let (_, param_name, _) = result.unwrap();
    assert_eq!(param_name, "x");
}

// =========================================================================
// skip_backtick_quoted
// =========================================================================

#[test]
fn skip_backtick_quoted_no_backticks() {
    assert_eq!(
        CheckerState::skip_backtick_quoted("@param {string} x"),
        "@param {string} x"
    );
}

#[test]
fn skip_backtick_quoted_single_pair() {
    assert_eq!(
        CheckerState::skip_backtick_quoted("`code` @param {string} x"),
        "@param {string} x"
    );
}

#[test]
fn skip_backtick_quoted_multiple_pairs() {
    assert_eq!(CheckerState::skip_backtick_quoted("`a` `b` rest"), "rest");
}

#[test]
fn skip_backtick_quoted_unclosed() {
    // Unclosed backtick should just stop
    let result = CheckerState::skip_backtick_quoted("`unclosed");
    assert_eq!(result, "`unclosed");
}

// =========================================================================
// is_plain_jsdoc_type_name - additional tests
// =========================================================================

#[test]
fn is_plain_jsdoc_type_name_empty() {
    assert!(!CheckerState::is_plain_jsdoc_type_name(""));
}

#[test]
fn is_plain_jsdoc_type_name_single_char() {
    assert!(CheckerState::is_plain_jsdoc_type_name("T"));
    assert!(CheckerState::is_plain_jsdoc_type_name("_"));
    assert!(CheckerState::is_plain_jsdoc_type_name("$"));
}

#[test]
fn is_plain_jsdoc_type_name_starts_with_digit() {
    assert!(!CheckerState::is_plain_jsdoc_type_name("1Foo"));
}

#[test]
fn is_plain_jsdoc_type_name_with_generics() {
    assert!(!CheckerState::is_plain_jsdoc_type_name("Array<string>"));
}

#[test]
fn is_plain_jsdoc_type_name_with_parens() {
    assert!(!CheckerState::is_plain_jsdoc_type_name("(string)"));
}

#[test]
fn is_plain_jsdoc_type_name_with_spaces() {
    assert!(!CheckerState::is_plain_jsdoc_type_name("foo bar"));
}

// =========================================================================
// extract_jsdoc_param_names - additional tests
// =========================================================================

#[test]
fn extract_param_names_empty_jsdoc() {
    let names = CheckerState::extract_jsdoc_param_names("");
    assert!(names.is_empty());
}

#[test]
fn extract_param_names_no_params() {
    let jsdoc = r#"
 * @returns {number}
 * @description Something
        "#;
    let names = CheckerState::extract_jsdoc_param_names(jsdoc);
    assert!(names.is_empty());
}

#[test]
fn extract_param_names_preserves_order() {
    let jsdoc = r#"
 * @param {string} alpha
 * @param {number} beta
 * @param {boolean} gamma
        "#;
    let names = CheckerState::extract_jsdoc_param_names(jsdoc);
    assert_eq!(names.len(), 3);
    assert_eq!(names[0].0, "alpha");
    assert_eq!(names[1].0, "beta");
    assert_eq!(names[2].0, "gamma");
}

#[test]
fn extract_param_names_byte_offsets() {
    let jsdoc = r#"
 * @param {string} first
 * @param {number} second
        "#;
    let names = CheckerState::extract_jsdoc_param_names(jsdoc);
    // The second element is a byte offset into the jsdoc string
    // pointing to where the @param tag starts
    assert!(names[0].1 > 0);
    assert!(names[1].1 > names[0].1);
    // Both offsets should point to a "@param" in the source
    assert!(jsdoc[names[0].1..].starts_with("@param"));
    assert!(jsdoc[names[1].1..].starts_with("@param"));
}

// =========================================================================
// JSDoc @type {function(...)} on object property — checkJsdocTypeTagOnObjectProperty2
// =========================================================================

/// Method shorthand inside an object literal whose property carries
/// `/** @type {function(number): number} */` should have its block-body
/// returns checked against the contextual return type. tsc reports TS2322
/// at `return "42";` because the JSDoc-declared return is `number`.
///
/// Before the fix, `body_return_type` for a sync contextually-typed function
/// fell back to the inferred body type (`string`), which made the return
/// statement check trivially succeed.
#[test]
fn jsdoc_type_function_on_method_shorthand_checks_block_body_return_type() {
    let diags = crate::test_utils::check_js_source_diagnostics(
        r#"// @ts-check
const obj = {
  /** @type {function(number): number} */
  method1(n1) {
      return "42";
  },
};
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one TS2322 (string -> number) on the method body return; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    let msg = &ts2322[0].message_text;
    assert!(
        msg.contains("string") && msg.contains("number"),
        "Expected TS2322 message to mention string and number, got: {msg}"
    );
}

/// Arrow function with a parameter default whose value type does not match
/// the JSDoc-declared callable parameter type should anchor TS2322 at the
/// parameter name, not at the enclosing arrow function's `(` token.
///
/// Before the fix, `assignment_anchor_node` walked up from the Parameter to
/// the `ArrowFunction` and returned the function's start position (`(`), which
/// shifted the diagnostic column one to the left of tsc's anchor.
#[test]
fn jsdoc_type_function_param_default_anchors_at_parameter_name() {
    let diags = crate::test_utils::check_js_source_diagnostics(
        r#"// @ts-check
/** @type {function(number): number} */
const f = (num="0") => num + 42;
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one TS2322 for the param default mismatch; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );

    // The diagnostic must anchor at `num`, not at the preceding `(`.
    // `f = (num` — the parameter name `num` follows `f = (`. The diagnostic
    // start should match the byte offset of `n` in `num`, and its length
    // should be 3 (the identifier).
    let source = "// @ts-check\n/** @type {function(number): number} */\nconst f = (num=\"0\") => num + 42;\n";
    let num_pos = source.find("(num=").expect("paren-num present") + 1; // skip '('
    let diag = ts2322[0];
    assert_eq!(
        diag.start as usize, num_pos,
        "TS2322 must anchor at the parameter name `num` (offset {num_pos}); got diagnostic at offset {}",
        diag.start
    );
    assert_eq!(
        diag.length, 3,
        "TS2322 length must equal len(`num`) = 3; got {}",
        diag.length
    );
}
