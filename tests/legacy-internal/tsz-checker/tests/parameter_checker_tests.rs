//! Unit tests extracted from `checkers/parameter_checker.rs`.
//!
//! Split out to keep that module under the 2000-line architecture ceiling; the
//! module contents are unchanged.

#[cfg(test)]
mod strict_parameter_name_tests {
    use crate::context::CheckerOptions;
    use crate::query_boundaries::common::TypeInterner;
    use crate::state::CheckerState;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;

    fn checker_codes_with_parse_health(source: &str) -> Vec<u32> {
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
            CheckerOptions::default(),
        );
        checker.ctx.has_parse_errors = !parse_diagnostics.is_empty();
        checker.ctx.has_syntax_parse_errors = !parse_diagnostics.is_empty();
        checker.ctx.syntax_parse_error_positions =
            parse_diagnostics.iter().map(|diag| diag.start).collect();
        checker.ctx.all_parse_error_positions =
            parse_diagnostics.iter().map(|diag| diag.start).collect();

        checker.check_source_file(root);
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|diag| diag.code)
            .collect()
    }

    #[test]
    fn recovered_rest_parameter_modifier_suppresses_class_strict_reserved_name() {
        let codes = checker_codes_with_parse_health(
            "class C {
                 constructor(...public rest: string[]) {}
             }",
        );
        assert!(
            !codes.contains(&1213),
            "recovered `...public rest` should not cascade into TS1213: {codes:?}"
        );
    }

    #[test]
    fn async_function_await_parameter_reports_ts1359() {
        let codes = checker_codes_with_parse_health("async function foo(await) {}");
        assert!(
            codes.contains(&1359),
            "async function `await` parameter should report TS1359: {codes:?}"
        );
    }

    #[test]
    fn async_arrow_await_parameter_reports_ts1359() {
        let codes = checker_codes_with_parse_health("var foo = async (await) => {};");
        assert!(
            codes.contains(&1359),
            "async arrow `await` parameter should report TS1359: {codes:?}"
        );
    }

    #[test]
    fn async_generator_await_parameter_reports_ts1359() {
        let codes = checker_codes_with_parse_health("async function* foo(await) {}");
        assert!(
            codes.contains(&1359),
            "async generator `await` parameter should report TS1359: {codes:?}"
        );
    }

    #[test]
    fn async_generator_method_await_parameter_reports_ts1359() {
        // Binder-name variation: the check keys off the `await` identifier and
        // the enclosing method's async modifier, not the enclosing class name.
        let codes = checker_codes_with_parse_health("class Widget { async *run(await) {} }");
        assert!(
            codes.contains(&1359),
            "async method `await` parameter should report TS1359: {codes:?}"
        );
    }

    #[test]
    fn non_async_function_await_parameter_stays_clean() {
        let codes = checker_codes_with_parse_health("function foo(await) {}");
        assert!(
            !codes.contains(&1359),
            "non-async function `await` parameter must not report TS1359: {codes:?}"
        );
    }

    #[test]
    fn sync_generator_await_parameter_stays_clean() {
        let codes = checker_codes_with_parse_health("function* foo(await) {}");
        assert!(
            !codes.contains(&1359),
            "sync generator `await` parameter must not report TS1359: {codes:?}"
        );
    }

    #[test]
    fn plain_function_in_static_block_await_parameter_stays_clean() {
        // The nested non-async function resets the Await context, so tsc emits
        // no TS1359 here even though the lexical scope is a class static block.
        let codes =
            checker_codes_with_parse_health("class C { static { function foo(await) {} } }");
        assert!(
            !codes.contains(&1359),
            "plain function nested in a static block must not report TS1359: {codes:?}"
        );
    }

    #[test]
    fn parse_error_suppresses_await_parameter_ts1359_program_wide() {
        // A real parse error anywhere in the program suppresses the TS1359
        // grammar check, mirroring tsc's program-wide grammar-check gate.
        let codes = checker_codes_with_parse_health(
            "async function foo(await) {}
             function bad( ",
        );
        assert!(
            !codes.contains(&1359),
            "parse error should suppress the await-parameter TS1359: {codes:?}"
        );
    }
}

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

#[cfg(test)]
mod parameter_decorator_grammar_tests {
    //! TS1206 for parameter decorators.
    //!
    //! Structural rule: a parameter decorator is legal only on a class
    //! constructor/method/set-accessor parameter, and only under
    //! `experimentalDecorators`. Every other function-like parameter position
    //! rejects it with TS1206 in both decorator modes. tsc reports it once per
    //! parameter (at the first decorator) and does not otherwise resolve the
    //! decorator expression of an invalidly-placed decorator.
    use crate::test_utils::{check_source_codes, check_source_codes_experimental_decorators};

    const DEC: &str = "declare function dec(...a: any[]): any;\n";

    fn count(codes: &[u32], code: u32) -> usize {
        codes.iter().filter(|&&c| c == code).count()
    }

    // --- Non-class function-like positions: TS1206 in both modes (false-negative fix) ---

    #[test]
    fn function_declaration_parameter_decorator_is_ts1206() {
        let src = format!("{DEC}function f(@dec a: number) {{}}");
        assert_eq!(count(&check_source_codes(&src), 1206), 1);
        assert_eq!(
            count(&check_source_codes_experimental_decorators(&src), 1206),
            1
        );
    }

    #[test]
    fn function_expression_parameter_decorator_is_ts1206() {
        let src = format!("{DEC}const h = function (@dec a: number) {{}};");
        assert_eq!(count(&check_source_codes(&src), 1206), 1);
    }

    #[test]
    fn object_literal_method_parameter_decorator_is_ts1206() {
        // Shares the MethodDeclaration node kind with a class method, but its
        // parent is an object literal, so it is never a valid decorator target.
        let src = format!("{DEC}const o = {{ m(@dec a: number) {{}} }};");
        assert_eq!(count(&check_source_codes(&src), 1206), 1);
        assert_eq!(
            count(&check_source_codes_experimental_decorators(&src), 1206),
            1
        );
    }

    #[test]
    fn object_literal_setter_parameter_decorator_is_ts1206() {
        let src = format!("{DEC}const o = {{ set y(@dec v: number) {{}} }};");
        assert_eq!(count(&check_source_codes(&src), 1206), 1);
    }

    #[test]
    fn interface_method_signature_parameter_decorator_is_ts1206() {
        let src = format!("{DEC}interface I {{ m(@dec a: number): void; }}");
        assert_eq!(count(&check_source_codes(&src), 1206), 1);
    }

    #[test]
    fn call_signature_parameter_decorator_is_ts1206() {
        let src = format!("{DEC}interface I {{ (@dec a: number): void; }}");
        assert_eq!(count(&check_source_codes(&src), 1206), 1);
    }

    #[test]
    fn construct_signature_parameter_decorator_is_ts1206() {
        let src = format!("{DEC}interface I {{ new (@dec a: number): void; }}");
        assert_eq!(count(&check_source_codes(&src), 1206), 1);
    }

    #[test]
    fn type_literal_method_parameter_decorator_is_ts1206() {
        let src = format!("{DEC}type T = {{ m(@dec a: number): void }};");
        assert_eq!(count(&check_source_codes(&src), 1206), 1);
    }

    #[test]
    fn function_nested_in_class_method_parameter_decorator_is_ts1206() {
        // The container is the inner function, not the enclosing class method,
        // so nearest-enclosing-class is not enough — the immediate parent must
        // be a class. The inner function's parent is a block.
        let src = format!("{DEC}class C {{ m() {{ function g(@dec a: number) {{}} }} }}");
        assert_eq!(count(&check_source_codes(&src), 1206), 1);
        assert_eq!(
            count(&check_source_codes_experimental_decorators(&src), 1206),
            1
        );
    }

    // --- Class-member positions: TS1206 only when experimentalDecorators is off ---

    #[test]
    fn class_method_parameter_decorator_ts1206_without_experimental() {
        let src = format!("{DEC}class C {{ m(@dec a: number) {{}} }}");
        assert_eq!(count(&check_source_codes(&src), 1206), 1);
        assert_eq!(
            count(&check_source_codes_experimental_decorators(&src), 1206),
            0
        );
    }

    #[test]
    fn class_constructor_parameter_decorator_ts1206_without_experimental() {
        // A constructor implementation skips check_parameter_properties, so this
        // pins the direct decorator-grammar call on the bodied-constructor path.
        let src = format!("{DEC}class C {{ constructor(@dec a: number) {{}} }}");
        assert_eq!(count(&check_source_codes(&src), 1206), 1);
        assert_eq!(
            count(&check_source_codes_experimental_decorators(&src), 1206),
            0
        );
    }

    #[test]
    fn class_setter_parameter_decorator_ts1206_without_experimental() {
        let src = format!("{DEC}class C {{ set x(@dec v: number) {{}} }}");
        assert_eq!(count(&check_source_codes(&src), 1206), 1);
        assert_eq!(
            count(&check_source_codes_experimental_decorators(&src), 1206),
            0
        );
    }

    #[test]
    fn class_getter_parameter_decorator_is_ts1206_even_with_experimental() {
        // A getter cannot carry a parameter (TS1054), and tsc rejects a
        // decorator on that illegal parameter too — a get accessor is never a
        // valid parameter-decorator target, so TS1206 fires in both modes.
        let src = format!("{DEC}class C {{ get x(@dec a: number) {{ return 1; }} }}");
        assert_eq!(count(&check_source_codes(&src), 1206), 1);
        assert_eq!(
            count(&check_source_codes_experimental_decorators(&src), 1206),
            1
        );
    }

    // --- Multiplicity: one TS1206 per parameter, not per decorator ---

    #[test]
    fn multiple_decorators_on_one_parameter_report_once() {
        let src = format!("{DEC}function f(@dec @dec a: number) {{}}");
        assert_eq!(count(&check_source_codes(&src), 1206), 1);
    }

    #[test]
    fn class_method_multiple_decorators_on_one_parameter_report_once() {
        let src = format!("{DEC}class C {{ m(@dec @dec a: number) {{}} }}");
        assert_eq!(count(&check_source_codes(&src), 1206), 1);
    }

    #[test]
    fn each_decorated_parameter_reports_its_own_ts1206() {
        let src = format!("{DEC}function f(@dec a: number, @dec b: number) {{}}");
        assert_eq!(count(&check_source_codes(&src), 1206), 2);
    }

    // --- No spurious semantic checking of an invalidly-placed decorator ---

    #[test]
    fn invalid_class_parameter_decorator_does_not_resolve_expression() {
        // Standard mode: TS1206 fires for the invalid position and the
        // (undefined) decorator expression is not resolved, so there is no
        // cascaded TS2304. With experimentalDecorators the position becomes
        // valid and TS1206 disappears.
        let src = "class C { m(@nope a: number) {} }";
        let std = check_source_codes(src);
        assert_eq!(count(&std, 1206), 1);
        assert_eq!(count(&std, 2304), 0);

        let exp = check_source_codes_experimental_decorators(src);
        assert_eq!(count(&exp, 1206), 0);
    }

    #[test]
    fn invalid_function_parameter_decorator_does_not_resolve_expression() {
        let src = "function f(@nope a: number) {}";
        let codes = check_source_codes(src);
        assert_eq!(count(&codes, 1206), 1);
        assert_eq!(count(&codes, 2304), 0);
    }

    #[test]
    fn this_parameter_decorator_is_not_ts1206() {
        // A decorator on a `this` parameter is owned by TS1433 ("Neither
        // decorators nor modifiers may be applied to 'this' parameters."), which
        // tsc reports *instead of* the generic TS1206. The end-to-end TS1433
        // emission is covered by the conformance suite
        // (`decoratorOnFunctionParameter.ts`); here we pin the suppression this
        // fix owns: the generic TS1206 must not fire on a `this` parameter.
        let src =
            format!("{DEC}class C {{ n = true; }}\nfunction f(@dec this: C) {{ return this.n; }}");
        assert_eq!(count(&check_source_codes(&src), 1206), 0);
    }
}
