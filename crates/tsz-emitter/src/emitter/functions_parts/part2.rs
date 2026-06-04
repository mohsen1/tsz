#[cfg(test)]
mod tests {
    use crate::output::printer::{PrintOptions, Printer};
    use tsz_common::ScriptTarget;
    fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
        let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        (parser, root)
    }

    /// Async arrow functions must always have parenthesized parameters,
    /// matching tsc behavior. `async x => x` becomes `async (x) => x`.
    #[test]
    fn async_arrow_always_parenthesizes_params() {
        let source = "const f = async i => i;";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("async (i) =>"),
            "Async arrow with single param should always have parens.\nOutput:\n{output}"
        );
    }

    /// Non-async arrow functions with a single simple param preserve source parens.
    /// `x => x` stays as `x => x` (no forced parens).
    #[test]
    fn non_async_arrow_preserves_no_parens() {
        let source = "const f = x => x;";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        // Should NOT add parens for non-async single-param arrow
        assert!(
            !output.contains("(x) =>"),
            "Non-async arrow without source parens should not add parens.\nOutput:\n{output}"
        );
        assert!(
            output.contains("x =>"),
            "Non-async arrow should preserve no-paren form.\nOutput:\n{output}"
        );
    }

    /// Async arrow with parens in source should keep them.
    #[test]
    fn async_arrow_with_source_parens_keeps_them() {
        let source = "const f = async (x) => x;";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("async (x) =>"),
            "Async arrow with source parens should keep them.\nOutput:\n{output}"
        );
    }

    #[test]
    fn parenthesized_arrow_body_type_erasure_strips_parens_across_comment() {
        let source = "const x = (a: any[]) => (\n    // comment\n    undefined as number\n);";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(
            &parser.arena,
            PrintOptions {
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("const x = (a) => \n// comment\nundefined;"),
            "Arrow body type erasure should strip recovery parens and hoist the comment.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("=> ("),
            "Arrow body type erasure should not preserve the opening paren.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("undefined);"),
            "Arrow body type erasure should not preserve the closing paren.\nOutput:\n{output}"
        );
    }

    /// Async arrow inside a function body should pass `this` to __awaiter.
    /// Arrow functions lexically capture `this` from the enclosing scope.
    #[test]
    fn async_arrow_in_function_passes_this_to_awaiter() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "function f() { (async () => { return 10; })(); }";
        let (parser, root) = parse_test_source(source);
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result.code.contains("__awaiter(this,"),
            "Async arrow inside function should pass `this` to __awaiter.\nOutput:\n{}",
            result.code
        );
    }

    /// Async arrow at top level should pass `void 0` to __awaiter.
    #[test]
    fn async_arrow_at_top_level_passes_void_0_to_awaiter() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "const g = async () => { return 10; };";
        let (parser, root) = parse_test_source(source);
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result.code.contains("__awaiter(void 0,"),
            "Async arrow at top level should pass `void 0` to __awaiter.\nOutput:\n{}",
            result.code
        );
    }

    /// Async arrow inside a class method should pass `this` to __awaiter.
    #[test]
    fn async_arrow_in_class_method_passes_this_to_awaiter() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "class C { method() { return (async () => 42)(); } }";
        let (parser, root) = parse_test_source(source);
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result.code.contains("__awaiter(this,"),
            "Async arrow inside class method should pass `this` to __awaiter.\nOutput:\n{}",
            result.code
        );
    }

    /// Lowering `??=` on a member-access target below ES2020 introduces a
    /// read-cache temp (`(_a = obj.p) !== null && _a !== void 0 ? _a : ...`).
    /// That temp lives in `hoisted_assignment_value_temps` and must be declared
    /// as `var _a;` at the top of the enclosing function body — including for
    /// *single-line* bodies, where the prologue is injected inline. Without the
    /// declaration the output references an undeclared `_a` and throws
    /// `ReferenceError` at runtime under strict mode. These tests vary the object
    /// and property spellings so the fix is keyed on structure, not names.
    fn emit_es2015(source: &str) -> String {
        use crate::output::printer::{PrintOptions, lower_and_print};
        let (parser, root) = parse_test_source(source);
        lower_and_print(&parser.arena, root, PrintOptions::es6()).code
    }

    /// Every generated `(_x = ...)` read-cache temp must have a matching
    /// `var _x;` declaration in the output, otherwise the emit is non-runnable.
    fn assert_value_temp_declared(output: &str, temp: &str) {
        assert!(
            output.contains(&format!("({temp} = ")),
            "expected read-cache temp `{temp}` to be used.\nOutput:\n{output}"
        );
        assert!(
            output.contains(&format!("var {temp};"))
                || output.contains(&format!("var {temp},"))
                || output.contains(&format!(", {temp};"))
                || output.contains(&format!(", {temp},")),
            "read-cache temp `{temp}` is used but never declared (`var {temp};` missing).\nOutput:\n{output}"
        );
    }

    #[test]
    fn nullish_assign_in_single_line_function_declares_value_temp() {
        let output = emit_es2015("function f() { box.value ??= 1; }");
        assert_value_temp_declared(&output, "_a");
    }

    #[test]
    fn nullish_assign_in_single_line_method_declares_value_temp() {
        // Different object/property spelling than the function case.
        let output = emit_es2015("class Counter { bump() { this.count ??= 0; } }");
        assert_value_temp_declared(&output, "_a");
    }

    #[test]
    fn nullish_assign_on_super_property_declares_value_temp() {
        let source = "class Base { get slot() { return 0; } set slot(v: number) {} }\n\
            class Derived extends Base { run() { super.slot ??= 3; } }";
        let output = emit_es2015(source);
        assert_value_temp_declared(&output, "_a");
        assert!(
            output.contains("(_a = super.slot)"),
            "super-property read-cache temp must wrap the `super` read.\nOutput:\n{output}"
        );
    }

    #[test]
    fn nullish_assign_in_single_line_constructor_declares_value_temp() {
        let output = emit_es2015("class C { constructor() { store.flag ??= 1; } }");
        assert_value_temp_declared(&output, "_a");
    }

    #[test]
    fn nullish_assign_in_multiline_constructor_declares_value_temp() {
        let source = "class C {\n    constructor() {\n        const y = 1;\n        store.flag ??= y;\n    }\n}";
        let output = emit_es2015(source);
        assert_value_temp_declared(&output, "_a");
    }

    #[test]
    fn nullish_assign_in_field_initializer_declares_value_temp() {
        // Synthesized constructor path for a field initializer that lowers `??=`.
        let output = emit_es2015("class C { x = (bag.item ??= 2); }");
        assert_value_temp_declared(&output, "_a");
    }

    #[test]
    fn nullish_assign_complex_receiver_declares_both_temps() {
        // A non-simple receiver also allocates an assignment-target temp `_b`;
        // both the value temp `_a` and the receiver temp `_b` must be declared.
        let output = emit_es2015("function f() { make().value ??= 1; }");
        assert_value_temp_declared(&output, "_a");
        assert!(
            output.contains("var _b;")
                || output.contains(", _b;")
                || output.contains("var _a, _b;"),
            "receiver temp `_b` must be declared.\nOutput:\n{output}"
        );
    }

    #[test]
    fn nullish_assign_in_block_body_arrow_declares_value_temp() {
        let output = emit_es2015("const f = () => { box.value ??= 1; };");
        assert_value_temp_declared(&output, "_a");
    }

    #[test]
    fn async_arrow_inside_top_level_arrow_passes_void_0_to_awaiter() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "const outer = () => async () => 1;";
        let (parser, root) = parse_test_source(source);
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result.code.contains("() => __awaiter(void 0,"),
            "Async arrow nested only in top-level arrows should pass void 0.\nOutput:\n{}",
            result.code
        );
    }

    #[test]
    fn async_arrow_with_binding_pattern_params_forwards_arguments_to_generator() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "const f = async (dispatch: Dispatch, { foo }: OwnProps) => { return foo; };";
        let (parser, root) = parse_test_source(source);
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result.code.contains(
                "(dispatch_1, _a) => __awaiter(void 0, [dispatch_1, _a], void 0, function* (dispatch, { foo }) { return foo; })"
            ),
            "Async arrow with a binding pattern should forward temp parameters into the generator.\nOutput:\n{}",
            result.code
        );
    }

    #[test]
    fn async_arrow_object_rest_param_uses_generator_prologue() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "async ({ foo, bar, ...rest }) => bar(await foo);";
        let (parser, root) = parse_test_source(source);
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result
                .code
                .contains("__awaiter(void 0, void 0, void 0, function* () {"),
            "Async arrow with object rest should not forward the rest temp as generator args.\nOutput:\n{}",
            result.code
        );
        assert!(
            result
                .code
                .contains("var { foo, bar } = _a, rest = __rest(_a, [\"foo\", \"bar\"]);"),
            "Async arrow with object rest should emit a generator prologue.\nOutput:\n{}",
            result.code
        );
        assert!(
            result.code.contains("return bar(yield foo);"),
            "Async arrow body should still lower await to yield.\nOutput:\n{}",
            result.code
        );
    }

    #[test]
    fn function_with_empty_parameter_comment_preserves_comment() {
        let source = "function foo(/** nothing */) { return 1; }";

        let (parser, root) = parse_test_source(source);
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("function foo( /** nothing */) {"),
            "Comment inside empty parameter list should be emitted inside the parens for JS parity.\nOutput: {output}"
        );
        assert!(
            !output.contains("function foo() /** nothing */"),
            "Comment should not drift after closing paren.\nOutput: {output}"
        );
    }

    #[test]
    fn async_arrow_await_default_param_es2015_forwards_args() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "var foo = async (a = await): Promise<void> => {}";
        let (parser, root) = parse_test_source(source);
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result.code.contains("var foo = (...args_1) => __awaiter(void 0, [...args_1], void 0, function* (a = yield ) {"),
            "Async arrow await-default recovery should forward args in ES2015 emit.\nOutput:\n{}",
            result.code
        );
    }

    #[test]
    fn async_arrow_default_param_preserves_leading_args_and_reuses_arguments_capture() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "function f() { const a1 = async (x, y = z) => {}; const a2 = async (x = z) => { return async () => arguments; }; const a3 = async () => { return async (x = z) => arguments; }; }";
        let (parser, root) = parse_test_source(source);
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result.code.contains(
                "const a1 = (x_1, ...args_1) => __awaiter(this, [x_1, ...args_1], void 0, function* (x, y = z) { });"
            ),
            "Leading parameters before the first default should stay explicit and the default tail should be forwarded.\nOutput:\n{}",
            result.code
        );
        assert!(
            result.code.contains("var arguments_1 = arguments;"),
            "The first async arrow that needs lexical arguments should create a function-scoped capture.\nOutput:\n{}",
            result.code
        );
        assert!(
            result.code.contains(
                "const a3 = () => __awaiter(this, void 0, void 0, function* () { return (...args_1) => __awaiter(this, [...args_1], void 0, function* (x = z) { return arguments_1; }); });"
            ),
            "Sibling async arrows should reuse the existing function-scoped arguments capture.\nOutput:\n{}",
            result.code
        );
        assert!(
            !result.code.contains("var arguments_2 = arguments;"),
            "Sibling async arrows should not create redundant lexical arguments captures.\nOutput:\n{}",
            result.code
        );
    }

    #[test]
    fn async_function_es2015_single_line_moved_params_stays_inline() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "async function f(x = z) { return async () => arguments; }";
        let (parser, root) = parse_test_source(source);
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result.code.contains(
                "return __awaiter(this, arguments, void 0, function* (x = z) { return () => __awaiter(this, void 0, void 0, function* () { return arguments_1; }); });"
            ),
            "Single-line async function bodies with moved parameters should stay inline in the generator wrapper.\nOutput:\n{}",
            result.code
        );
    }

    #[test]
    fn async_function_await_arrow_param_recovery_native_keeps_await_param() {
        use crate::output::printer::lower_and_print;

        let source = "async function foo(a = await => await): Promise<void> {}";
        let (parser, root) = parse_test_source(source);
        let result = lower_and_print(
            &parser.arena,
            root,
            PrintOptions {
                target: ScriptTarget::ES2017,
                ..Default::default()
            },
        );

        assert!(
            result
                .code
                .contains("async function foo(a = await , await) {"),
            "Native async function recovery should preserve the trailing `await` parameter.\nOutput:\n{}",
            result.code
        );
    }

    #[test]
    fn async_function_await_arrow_param_recovery_es2015_keeps_await_param() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "async function foo(a = await => await): Promise<void> {}";
        let (parser, root) = parse_test_source(source);
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result.code.contains("function* (a = yield , await) {"),
            "Lowered async function recovery should preserve the trailing `await` parameter.\nOutput:\n{}",
            result.code
        );
    }

    #[test]
    fn async_function_es2015_destructured_param_preserves_outer_arity() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "async function foo({ foo = await bar }) {}";
        let (parser, root) = parse_test_source(source);
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result.code.contains("function foo(_a) {"),
            "Outer async function should keep a placeholder parameter.\nOutput:\n{}",
            result.code
        );
        assert!(
            result.code.contains("function* ({ foo = yield bar })"),
            "Moved generator parameter should preserve the destructuring pattern.\nOutput:\n{}",
            result.code
        );
    }

    #[test]
    fn async_function_es2015_moved_params_avoid_inner_name_collisions() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "async function h(a, { x }) {}";
        let (parser, root) = parse_test_source(source);
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result.code.contains("function h(a_1, _a) {"),
            "Outer async function placeholders should avoid colliding with inner generator parameters.\nOutput:\n{}",
            result.code
        );
        assert!(
            result.code.contains("function* (a, { x })"),
            "Moved generator parameters should preserve original names and patterns.\nOutput:\n{}",
            result.code
        );
    }

    #[test]
    fn malformed_rest_parameter_modifier_recovers_following_parameter() {
        let source = "class C { constructor(...public rest: string[]) {} }";

        let (parser, root) = parse_test_source(source);
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("constructor(...public, rest)"),
            "Malformed rest parameter should preserve the recovered parameter.\nOutput:\n{output}"
        );
    }

    #[test]
    fn parameter_leading_jsdoc_preserves_multiline_parameter_list_shape() {
        let source = r"class Type {
  constructor(
    /** a unique name for this codec */
    readonly name: string,
    /** a custom type guard */
    readonly is: boolean
  ) {}
}";

        let (parser, root) = parse_test_source(source);
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains(
                "constructor(\n    /** a unique name for this codec */\n    name, \n    /** a custom type guard */\n    is)"
            ),
            "Parameter JSDoc should keep the multiline parameter-list shape.\nOutput:\n{output}"
        );
    }

    /// Parameters with empty/missing identifier names (from parser error recovery)
    /// should be dropped, matching tsc behavior.
    #[test]
    fn empty_param_name_dropped() {
        let source = "function f(a,\u{00AC}) {}";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("function f(a)"),
            "Invalid character parameter should be dropped.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("(a, )"),
            "Should not have trailing comma for dropped param.\nOutput:\n{output}"
        );
    }

    /// Omitted arguments in call expressions should be dropped.
    #[test]
    fn omitted_call_args_dropped() {
        let source = "foo(a,,b);";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("foo(a, b)"),
            "Omitted argument should be dropped.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("foo(a, , b)"),
            "Should not have extra comma for omitted arg.\nOutput:\n{output}"
        );
    }
}
