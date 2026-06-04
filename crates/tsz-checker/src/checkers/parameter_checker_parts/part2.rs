#[cfg(test)]
mod binding_pattern_defaults_tests {
    use crate::context::CheckerOptions;
    use crate::query_boundaries::common::TypeInterner;
    use crate::state::CheckerState;
    use crate::test_utils::{check_js_source_diagnostics, check_source_codes};
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;

    /// Positive test: arrow function default correctly typed via contextual type.
    /// `v => v.toString()` returns string, matching `(x: number) => string`.
    #[test]
    fn arrow_default_matching_signature_no_error() {
        let codes = check_source_codes(
            "interface Show { show: (x: number) => string; }
             function f({ show = v => v.toString() }: Show) {}",
        );
        assert!(
            !codes.contains(&2322),
            "Should not emit TS2322 for matching arrow default: {codes:?}"
        );
    }

    /// Positive test: renamed property with arrow default, correct return type.
    #[test]
    fn renamed_property_arrow_default_no_error() {
        let codes = check_source_codes(
            r#"interface Show { show: (x: number) => string; }
               function f2({ "show": showRename = v => v.toString() }: Show) {}"#,
        );
        assert!(
            !codes.contains(&2322),
            "Should not emit TS2322 for matching renamed arrow default: {codes:?}"
        );
    }

    /// Positive test: string literal default matches union type.
    #[test]
    fn string_literal_default_matches_union_no_error() {
        let codes = check_source_codes(
            r#"interface StringUnion { prop: "foo" | "bar"; }
               function h({ prop = "foo" }: StringUnion) {}"#,
        );
        assert!(
            !codes.contains(&2322),
            "Should not emit TS2322 for matching string literal default: {codes:?}"
        );
    }

    /// Positive test: tuple default matches tuple type.
    #[test]
    fn tuple_default_matches_tuple_type_no_error() {
        let codes = check_source_codes(
            "interface Tuples { prop: [string, number]; }
             function g({ prop = [\"hello\", 1234] }: Tuples) {}",
        );
        assert!(
            !codes.contains(&2322),
            "Should not emit TS2322 for matching tuple default: {codes:?}"
        );
    }

    /// Optional property default — `check_binding_element` validates when
    /// element type includes undefined.
    #[test]
    fn optional_property_default_assignable_no_error() {
        let codes = check_source_codes(
            "interface Opts { name?: string; }
             function f({ name = \"default\" }: Opts) {}",
        );
        assert!(
            !codes.contains(&2322),
            "Should not emit TS2322 for assignable optional property default: {codes:?}"
        );
    }

    /// The `check_parameter_binding_pattern_defaults` infrastructure is called
    /// for function declarations with binding pattern parameters.
    #[test]
    fn parameter_binding_check_called_for_function_decl() {
        // This should not panic or crash — verifies the call path works.
        let codes = check_source_codes(
            "interface Config { debug?: boolean; }
             function init({ debug = false }: Config) {}",
        );
        assert!(
            !codes.contains(&2322),
            "Should not emit TS2322 for boolean default: {codes:?}"
        );
    }

    /// Nested object binding pattern with defaults.
    #[test]
    fn nested_object_binding_no_error_when_matching() {
        let codes = check_source_codes(
            "interface Show { show: (x: number) => string; }
             interface Nested { nested: Show }
             function ff({ nested = { show: v => v.toString() } }: Nested) {}",
        );
        assert!(
            !codes.contains(&2322),
            "Should not emit TS2322 for matching nested default: {codes:?}"
        );
    }

    #[test]
    fn optional_binding_pattern_parameter_reports_ts2463_without_ts2339() {
        let codes = check_source_codes(
            "function f({ x }?: { x: number }) {
                 return x;
             }",
        );
        assert!(
            codes.contains(&2463),
            "Expected TS2463 for optional binding-pattern parameter, got: {codes:?}"
        );
        assert!(
            !codes.contains(&2339),
            "Optional binding-pattern parameter should not cascade into TS2339: {codes:?}"
        );
    }

    #[test]
    fn arrow_optional_binding_pattern_parameter_reports_ts2463_without_ts2339() {
        let codes = check_source_codes("const f = ({ x }?: { x: number }) => x;");
        assert!(
            codes.contains(&2463),
            "Expected TS2463 for arrow optional binding-pattern parameter, got: {codes:?}"
        );
        assert!(
            !codes.contains(&2339),
            "Arrow optional binding-pattern parameter should not cascade into TS2339: {codes:?}"
        );
    }

    #[test]
    fn typed_binding_pattern_parameter_default_object_literal_suppresses_ts2339() {
        let codes = check_source_codes(
            "function f({ x }: { x?: number } = {}) {
                 return x;
             }",
        );
        assert!(
            !codes.contains(&2339),
            "Typed parameter default object literal should not trigger TS2339: {codes:?}"
        );
    }

    #[test]
    fn jsdoc_optional_binding_pattern_parameter_reports_ts2463_without_ts2339() {
        let diagnostics = check_js_source_diagnostics(
            "/**
              * @typedef Foo
              * @property {string} a
              */
             /**
              * @param {Foo} [options]
              */
             function f({ a = \"a\" }) {}",
        );
        let codes: Vec<u32> = diagnostics.iter().map(|diag| diag.code).collect();
        assert!(
            codes.contains(&2463),
            "Expected TS2463 for JSDoc-optional binding-pattern parameter, got: {codes:?}"
        );
        assert!(
            !codes.contains(&2339),
            "JSDoc-optional binding-pattern parameter should not cascade into TS2339: {codes:?}"
        );
    }

    /// Regression: nested binding-pattern check must run for class
    /// constructors too. Previously, `check_parameter_binding_pattern_defaults`
    /// was only invoked for function declarations, so missing properties
    /// inside a constructor's binding-pattern parameter (e.g., `{ x1, x2 }`
    /// extracted from `ObjType1` which has `{ x; y; z }`) silently passed.
    /// tsc emits TS2339 in this position.
    #[test]
    fn constructor_binding_pattern_emits_ts2339_for_missing_properties() {
        let codes = check_source_codes(
            "type ObjType1 = { x: number; y: string; z: boolean }
             type TupleType1 = [ObjType1, number, string]
             class C1 {
                 constructor([{ x1, x2, x3 }, y, z]: TupleType1) {}
             }",
        );
        let ts2339_count = codes.iter().filter(|&&c| c == 2339).count();
        assert!(
            ts2339_count >= 3,
            "Constructor binding pattern with missing properties should \
             emit TS2339 for x1, x2, x3 (>=3 occurrences). Got codes: {codes:?}"
        );
    }

    #[test]
    fn misplaced_decorator_after_parameter_property_modifier_suppresses_ts1213() {
        let source = "declare var dec: any;
             class C {
                 constructor(public @dec p: number) {}
             }";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let parse_diagnostics = parser.get_diagnostics().to_vec();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            CheckerOptions {
                experimental_decorators: true,
                ..CheckerOptions::default()
            },
        );
        checker.ctx.set_lib_contexts(Vec::new());
        checker.ctx.has_parse_errors = !parse_diagnostics.is_empty();
        checker.ctx.all_parse_error_positions =
            parse_diagnostics.iter().map(|diag| diag.start).collect();
        checker.check_source_file(root);

        let codes: Vec<u32> = parse_diagnostics
            .iter()
            .map(|diag| diag.code)
            .chain(checker.ctx.diagnostics.iter().map(|diag| diag.code))
            .collect();
        assert!(
            codes.contains(&1005),
            "Expected misplaced parameter decorator to keep TS1005, got: {codes:?}"
        );
        assert!(
            !codes.contains(&1213),
            "Expected no TS1213 reserved-word cascade for recovered parameter, got: {codes:?}"
        );
    }
}

#[cfg(test)]
mod jsdoc_optional_param_tests {
    use crate::state::CheckerState;

    // Note: is_jsdoc_param_optional processes raw JSDoc comment text which
    // includes the `/** */` delimiters. Lines are split by '\n' and each line
    // is trimmed then stripped of leading '*'. For single-line JSDoc like
    // `/** @param ... */`, the leading `/**` starts with `/` so the `*` strip
    // doesn't reach the content. Use multiline format in tests to match real usage.

    #[test]
    fn bracket_syntax_marks_optional() {
        let jsdoc = "/**\n * @param {number} [x]\n */";
        assert!(CheckerState::is_jsdoc_param_optional(jsdoc, "x"));
    }

    #[test]
    fn bracket_with_default_marks_optional() {
        let jsdoc = "/**\n * @param {number} [x=0]\n */";
        assert!(CheckerState::is_jsdoc_param_optional(jsdoc, "x"));
    }

    #[test]
    fn type_equals_suffix_marks_optional() {
        let jsdoc = "/**\n * @param {number=} x\n */";
        assert!(CheckerState::is_jsdoc_param_optional(jsdoc, "x"));
    }

    #[test]
    fn plain_param_not_optional() {
        let jsdoc = "/**\n * @param {number} x\n */";
        assert!(!CheckerState::is_jsdoc_param_optional(jsdoc, "x"));
    }

    #[test]
    fn backtick_quoted_name_with_type_equals() {
        let jsdoc = "/**\n * @param {number=} `x`\n */";
        assert!(CheckerState::is_jsdoc_param_optional(jsdoc, "x"));
    }

    #[test]
    fn name_first_format_with_type_equals() {
        let jsdoc = "/**\n * @param x {number=}\n */";
        assert!(CheckerState::is_jsdoc_param_optional(jsdoc, "x"));
    }

    #[test]
    fn wrong_name_not_matched() {
        let jsdoc = "/**\n * @param {number} [y]\n */";
        assert!(!CheckerState::is_jsdoc_param_optional(jsdoc, "x"));
    }

    #[test]
    fn multiline_jsdoc_finds_correct_param() {
        let jsdoc = "/**\n * @param {number} a\n * @param {string} [b]\n */";
        assert!(!CheckerState::is_jsdoc_param_optional(jsdoc, "a"));
        assert!(CheckerState::is_jsdoc_param_optional(jsdoc, "b"));
    }
}

#[cfg(test)]
mod jsdoc_diagnostic_integration_tests {
    use crate::test_utils::check_js_source_diagnostics;

    /// TS1016: required param after JSDoc optional bracket param.
    #[test]
    fn ts1016_jsdoc_optional_bracket_then_required() {
        let diags = check_js_source_diagnostics(
            "/**\n * @param {number} [x]\n * @param {number} y\n */\nfunction f(x, y) {}",
        );
        // y is required after optional x — should NOT emit TS1016 since y is also required
        // Actually, x is optional (bracket), y is required after optional → TS1016 on y
        assert!(
            diags.iter().any(|d| d.code == 1016),
            "Expected TS1016 for required param after JSDoc optional: {diags:?}"
        );
    }

    /// No TS1016 when all params are required.
    #[test]
    fn no_ts1016_when_all_required() {
        let diags = check_js_source_diagnostics(
            "/**\n * @param {number} x\n * @param {number} y\n */\nfunction f(x, y) {}",
        );
        assert!(
            !diags.iter().any(|d| d.code == 1016),
            "Should not emit TS1016 when all params are required: {diags:?}"
        );
    }
}
