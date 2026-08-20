//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/emitter/functions.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 0be60c17b858f00d22be1489dbf78dd2df798cc7e4e6e32faca8f50260ae5f6b 1459 async_arrow_always_parenthesizes_params
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
// TSZ_INLINE_TEST_END 0be60c17b858f00d22be1489dbf78dd2df798cc7e4e6e32faca8f50260ae5f6b

// TSZ_INLINE_TEST_BEGIN e75d46c2f0adaedd5270227ba5de3feae9b55a2adb51a3d02d1c69ecc98c4dc7 1478 non_async_arrow_preserves_no_parens
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
// TSZ_INLINE_TEST_END e75d46c2f0adaedd5270227ba5de3feae9b55a2adb51a3d02d1c69ecc98c4dc7

// TSZ_INLINE_TEST_BEGIN a74b7a153061d529acd75ac2ba8b90d448e3ca4cc65ca3c0e47ab6fc00adab39 1501 async_arrow_with_source_parens_keeps_them
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
// TSZ_INLINE_TEST_END a74b7a153061d529acd75ac2ba8b90d448e3ca4cc65ca3c0e47ab6fc00adab39

// TSZ_INLINE_TEST_BEGIN 291569ed4b138eee09d5b4452638a69d22e4eb21befa614db359f569e726a4c2 1518 parenthesized_arrow_body_type_erasure_strips_parens_across_comment
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
// TSZ_INLINE_TEST_END 291569ed4b138eee09d5b4452638a69d22e4eb21befa614db359f569e726a4c2

// TSZ_INLINE_TEST_BEGIN 1a4d52e73198a7fdcbad0e29b1d653b5e51a9d87d5e529b042e2cc1f28a29a66 1551 async_arrow_in_function_passes_this_to_awaiter
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
// TSZ_INLINE_TEST_END 1a4d52e73198a7fdcbad0e29b1d653b5e51a9d87d5e529b042e2cc1f28a29a66

// TSZ_INLINE_TEST_BEGIN 02042f7ad3f6e106cffb0cf0eb045bcbb11a7e81e90f312a5d1b52497c6561e5 1567 async_arrow_at_top_level_passes_void_0_to_awaiter
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
// TSZ_INLINE_TEST_END 02042f7ad3f6e106cffb0cf0eb045bcbb11a7e81e90f312a5d1b52497c6561e5

// TSZ_INLINE_TEST_BEGIN 5c98083599867194101d1343b75233d88adc71c36187b1522dd7fa8f03ebfe8e 1583 async_arrow_in_class_method_passes_this_to_awaiter
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
// TSZ_INLINE_TEST_END 5c98083599867194101d1343b75233d88adc71c36187b1522dd7fa8f03ebfe8e

// TSZ_INLINE_TEST_BEGIN d7a22b799cf1da33113900f9dea3d307b67eed7ae8061e0f591610c70d513015 1628 nullish_assign_in_single_line_function_declares_value_temp
    #[test]
    fn nullish_assign_in_single_line_function_declares_value_temp() {
        let output = emit_es2015("function f() { box.value ??= 1; }");
        assert_value_temp_declared(&output, "_a");
    }
// TSZ_INLINE_TEST_END d7a22b799cf1da33113900f9dea3d307b67eed7ae8061e0f591610c70d513015

// TSZ_INLINE_TEST_BEGIN 9b4e297a8b5c39a13ccede88bb34ff75be48f4842bacd9b6c9d535f045eb8a2c 1634 nullish_assign_in_single_line_method_declares_value_temp
    #[test]
    fn nullish_assign_in_single_line_method_declares_value_temp() {
        // Different object/property spelling than the function case.
        let output = emit_es2015("class Counter { bump() { this.count ??= 0; } }");
        assert_value_temp_declared(&output, "_a");
    }
// TSZ_INLINE_TEST_END 9b4e297a8b5c39a13ccede88bb34ff75be48f4842bacd9b6c9d535f045eb8a2c

// TSZ_INLINE_TEST_BEGIN a7f6fa60bd859a85d3ff252aec7c85357721a777580133cd2fc1b794c8c8cec2 1641 nullish_assign_on_super_property_declares_value_temp
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
// TSZ_INLINE_TEST_END a7f6fa60bd859a85d3ff252aec7c85357721a777580133cd2fc1b794c8c8cec2

// TSZ_INLINE_TEST_BEGIN 81c9f965bb134904dbc29b9e19cca8ee84db8903a4de7922378854a1182c7d78 1653 nullish_assign_in_single_line_constructor_declares_value_temp
    #[test]
    fn nullish_assign_in_single_line_constructor_declares_value_temp() {
        let output = emit_es2015("class C { constructor() { store.flag ??= 1; } }");
        assert_value_temp_declared(&output, "_a");
    }
// TSZ_INLINE_TEST_END 81c9f965bb134904dbc29b9e19cca8ee84db8903a4de7922378854a1182c7d78

// TSZ_INLINE_TEST_BEGIN 11375d85dd805b96d1af11fe47deda4bcc12298f8148974e637231d0343c7383 1659 nullish_assign_in_multiline_constructor_declares_value_temp
    #[test]
    fn nullish_assign_in_multiline_constructor_declares_value_temp() {
        let source = "class C {\n    constructor() {\n        const y = 1;\n        store.flag ??= y;\n    }\n}";
        let output = emit_es2015(source);
        assert_value_temp_declared(&output, "_a");
    }
// TSZ_INLINE_TEST_END 11375d85dd805b96d1af11fe47deda4bcc12298f8148974e637231d0343c7383

// TSZ_INLINE_TEST_BEGIN caf660268c789c659b3505be2c93c26922395bab26388cded2ef12510440fcae 1666 nullish_assign_in_field_initializer_declares_value_temp
    #[test]
    fn nullish_assign_in_field_initializer_declares_value_temp() {
        // Synthesized constructor path for a field initializer that lowers `??=`.
        let output = emit_es2015("class C { x = (bag.item ??= 2); }");
        assert_value_temp_declared(&output, "_a");
    }
// TSZ_INLINE_TEST_END caf660268c789c659b3505be2c93c26922395bab26388cded2ef12510440fcae

// TSZ_INLINE_TEST_BEGIN 7b490fb0c4da1ac3a10d8e4fa0b71a28423db7a84a339091d10fa60af0cd16c1 1673 nullish_assign_complex_receiver_declares_both_temps
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
// TSZ_INLINE_TEST_END 7b490fb0c4da1ac3a10d8e4fa0b71a28423db7a84a339091d10fa60af0cd16c1

// TSZ_INLINE_TEST_BEGIN 5edf6e693253b773917a303c043d388c1a0c8888d55dc89f6aa2f66353c553ad 1687 nullish_assign_in_block_body_arrow_declares_value_temp
    #[test]
    fn nullish_assign_in_block_body_arrow_declares_value_temp() {
        let output = emit_es2015("const f = () => { box.value ??= 1; };");
        assert_value_temp_declared(&output, "_a");
    }
// TSZ_INLINE_TEST_END 5edf6e693253b773917a303c043d388c1a0c8888d55dc89f6aa2f66353c553ad

// TSZ_INLINE_TEST_BEGIN 8ffa85a8df38df01618ef892263dc2a0611adc1de4170fed0ae4e261fe5e9702 1693 async_arrow_inside_top_level_arrow_passes_void_0_to_awaiter
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
// TSZ_INLINE_TEST_END 8ffa85a8df38df01618ef892263dc2a0611adc1de4170fed0ae4e261fe5e9702

// TSZ_INLINE_TEST_BEGIN 4dcdb359bb3b04b4b22f299711abb66942a2288b31d429fe7c76a94ac8e16c65 1708 async_arrow_with_binding_pattern_params_forwards_arguments_to_generator
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
// TSZ_INLINE_TEST_END 4dcdb359bb3b04b4b22f299711abb66942a2288b31d429fe7c76a94ac8e16c65

// TSZ_INLINE_TEST_BEGIN 67c9b8ffb4d8f62e0e0aeb71b54f1be711df3aa11e0aff1aee8e65486df65757 1725 async_arrow_object_rest_param_uses_generator_prologue
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
// TSZ_INLINE_TEST_END 67c9b8ffb4d8f62e0e0aeb71b54f1be711df3aa11e0aff1aee8e65486df65757

// TSZ_INLINE_TEST_BEGIN 6c470df8cfb45b20577dfcadefa79e4341394452221866bfe09c9ed3b028bb5b 1759 object_rest_param_keeps_single_line_function_body
    /// An ES2018 object-rest parameter synthesizes a `var { a } = _a, rest =
    /// __rest(_a, [...])` preamble into the function body. tsc keeps a
    /// single-line source body on one line and writes that preamble inline;
    /// tsz previously forced the body multi-line whenever such a preamble was
    /// present. The body must stay single-line to match tsc byte-for-byte.
    #[test]
    fn object_rest_param_keeps_single_line_function_body() {
        let source = "function f({ a, b = 2, ...rest }: any) { return a + b; }";
        let (parser, root) = parse_test_source(source);
        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains(
                "function f(_a) { var { a, b = 2 } = _a, rest = __rest(_a, [\"a\", \"b\"]); return a + b; }"
            ),
            "Object-rest parameter must keep a single-line body on one line (matching tsc).\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 6c470df8cfb45b20577dfcadefa79e4341394452221866bfe09c9ed3b028bb5b

// TSZ_INLINE_TEST_BEGIN 4f72d31df9b0c0f778a0d566a3d8a2b6e47d6569a54a7864b7ef27aae794d201 1779 object_rest_param_single_line_body_hoists_temps_before_preamble
    /// When a single-line body also hoists optional-chaining / logical-assignment
    /// temps (`var _b, _c;`), tsc emits those temp declarations BEFORE the
    /// object-rest destructuring preamble. Lock that ordering.
    #[test]
    fn object_rest_param_single_line_body_hoists_temps_before_preamble() {
        let source = "function f({ a, ...rest }: any) { return g.h?.() ?? a; }";
        let (parser, root) = parse_test_source(source);
        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("var _b, _c; var { a } = _a, rest = __rest(_a, [\"a\"]);"),
            "Hoisted optional-chaining temps must precede the object-rest preamble on the single line.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 4f72d31df9b0c0f778a0d566a3d8a2b6e47d6569a54a7864b7ef27aae794d201

// TSZ_INLINE_TEST_BEGIN 2be27ca785c7d4b7343b92f5c10f9dc917ec266a187ca24d9167d7de501a6821 1800 object_rest_param_keeps_single_line_function_body_es5
    /// At ES5 the object-rest parameter preamble is lowered to property-access
    /// reads (`var a = _a.a, rest = __rest(_a, ["a"])`) instead of staying a
    /// binding pattern, and that lowering previously routed exclusively through
    /// the multi-line `emit_block_with_param_prologue` path. tsc still keeps a
    /// single-line source body single-line here (the object-rest prologue is not
    /// `startOnNewLine`), so the ES5 path must match.
    #[test]
    fn object_rest_param_keeps_single_line_function_body_es5() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "function f({ a, ...rest }: { a: number; b: string }) { return rest; }";
        let (parser, root) = parse_test_source(source);
        let output = lower_and_print(&parser.arena, root, PrintOptions::es5()).code;

        assert!(
            output.contains(
                "function f(_a) { var a = _a.a, rest = __rest(_a, [\"a\"]); return rest; }"
            ),
            "ES5 object-rest parameter must keep a single-line body on one line (matching tsc).\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 2be27ca785c7d4b7343b92f5c10f9dc917ec266a187ca24d9167d7de501a6821

// TSZ_INLINE_TEST_BEGIN 6d790c8415f48aeeca631b22f737d47914ce0f0240ab79a33e895ea08ed66e6c 1819 two_object_rest_params_keep_single_line_function_body_es5
    /// Two object-rest parameters at ES5 produce two combined `var` statements;
    /// tsc keeps the single-line body single-line, the statements separated by a
    /// single space (`var a = _a.a, r1 = __rest(...); var c = _b.c, r2 = ...;`).
    #[test]
    fn two_object_rest_params_keep_single_line_function_body_es5() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "function f({ a, ...r1 }: { a: number; b: string }, { c, ...r2 }: { c: number; d: string }) { return r1; }";
        let (parser, root) = parse_test_source(source);
        let output = lower_and_print(&parser.arena, root, PrintOptions::es5()).code;

        assert!(
            output.contains(
                "function f(_a, _b) { var a = _a.a, r1 = __rest(_a, [\"a\"]); var c = _b.c, r2 = __rest(_b, [\"c\"]); return r1; }"
            ),
            "Two ES5 object-rest parameters must keep a single-line body on one line.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 6d790c8415f48aeeca631b22f737d47914ce0f0240ab79a33e895ea08ed66e6c

// TSZ_INLINE_TEST_BEGIN c0396337a18fe658572ea5af092d71709f91c250c51511be939c43176e8b53c8 1840 simple_destructure_param_stays_multi_line_es5
    /// A non-object-rest transformed parameter (a plain destructure, an array
    /// pattern, or a default value) goes through the ES2015 transform, whose
    /// statements ARE `startOnNewLine`, so tsc emits the body multi-line. The
    /// single-line fast path must not fire for those, including when mixed with
    /// an object-rest parameter.
    #[test]
    fn simple_destructure_param_stays_multi_line_es5() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "function f({ a }: { a: number }) { return a; }";
        let (parser, root) = parse_test_source(source);
        let output = lower_and_print(&parser.arena, root, PrintOptions::es5()).code;

        assert!(
            output.contains("function f(_a) {\n    var a = _a.a;\n    return a;\n}"),
            "ES5 simple destructure parameter must keep the body multi-line (matching tsc).\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END c0396337a18fe658572ea5af092d71709f91c250c51511be939c43176e8b53c8

// TSZ_INLINE_TEST_BEGIN e3a0560295d44db9bbe3944c12233717caa112eef96d88ffb853462a1ee0d77e 1857 rest_param_with_object_rest_stays_multi_line_es5
    /// A `...args` rest parameter contributes an `arguments`-copy loop (a
    /// `startOnNewLine` prologue), so tsc keeps the body multi-line even when an
    /// object-rest parameter is also present.
    #[test]
    fn rest_param_with_object_rest_stays_multi_line_es5() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "function f({ a, ...rest }: { a: number; b: string }, ...args: number[]) { return rest; }";
        let (parser, root) = parse_test_source(source);
        let output = lower_and_print(&parser.arena, root, PrintOptions::es5()).code;

        assert!(
            output.contains("function f(_a) {\n"),
            "An ES5 `...args` rest parameter must keep the body multi-line.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END e3a0560295d44db9bbe3944c12233717caa112eef96d88ffb853462a1ee0d77e

// TSZ_INLINE_TEST_BEGIN e1241ea38b09af66dd2ae99870f4e89128cae844f5c89ad2f2318458a7e696df 1871 function_with_empty_parameter_comment_preserves_comment
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
// TSZ_INLINE_TEST_END e1241ea38b09af66dd2ae99870f4e89128cae844f5c89ad2f2318458a7e696df

// TSZ_INLINE_TEST_BEGIN 6bcec9895f1034d3a90c974a94744bd4b34582c9847a8788de89c0f95b36222f 1891 async_arrow_await_default_param_es2015_forwards_args
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
// TSZ_INLINE_TEST_END 6bcec9895f1034d3a90c974a94744bd4b34582c9847a8788de89c0f95b36222f

// TSZ_INLINE_TEST_BEGIN 22443f6c1d662d8c36dd726d162fd7c61d3158ad9287b25c69cb91d6db539b30 1906 async_arrow_default_param_preserves_leading_args_and_reuses_arguments_capture
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
// TSZ_INLINE_TEST_END 22443f6c1d662d8c36dd726d162fd7c61d3158ad9287b25c69cb91d6db539b30

// TSZ_INLINE_TEST_BEGIN 2ac9639c62857c879e690052db458d29de19da6a4586f2c102364c9007e07601 1940 async_function_es2015_single_line_moved_params_stays_inline
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
// TSZ_INLINE_TEST_END 2ac9639c62857c879e690052db458d29de19da6a4586f2c102364c9007e07601

// TSZ_INLINE_TEST_BEGIN 52ccda05a803bd15f6d97bb7792c31a4f30bbbeb7c1406ba5b3b9d2ac1064ee9 1957 async_function_await_arrow_param_recovery_native_keeps_await_param
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
// TSZ_INLINE_TEST_END 52ccda05a803bd15f6d97bb7792c31a4f30bbbeb7c1406ba5b3b9d2ac1064ee9

// TSZ_INLINE_TEST_BEGIN 1f2245a9fe1dae6b1c45d2189213872d5ae02cc9f6c1daebc94087db524e0968 1981 async_function_await_arrow_param_recovery_es2015_keeps_await_param
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
// TSZ_INLINE_TEST_END 1f2245a9fe1dae6b1c45d2189213872d5ae02cc9f6c1daebc94087db524e0968

// TSZ_INLINE_TEST_BEGIN b4134214de2abf469381f1068cdf6e13da32aab112feb94e50401fccf4362ce9 1996 async_function_es2015_destructured_param_preserves_outer_arity
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
// TSZ_INLINE_TEST_END b4134214de2abf469381f1068cdf6e13da32aab112feb94e50401fccf4362ce9

// TSZ_INLINE_TEST_BEGIN 81a393b804f775436b14031bea8baade05b817e8f0a4c45d7dbbce7acdaf01c7 2016 async_function_es2015_moved_params_avoid_inner_name_collisions
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
// TSZ_INLINE_TEST_END 81a393b804f775436b14031bea8baade05b817e8f0a4c45d7dbbce7acdaf01c7

// TSZ_INLINE_TEST_BEGIN 0aa545cde6c7c6af7365d36222037d143f145c931dd805d3e9a2dc473df2739d 2036 malformed_rest_parameter_modifier_recovers_following_parameter
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
// TSZ_INLINE_TEST_END 0aa545cde6c7c6af7365d36222037d143f145c931dd805d3e9a2dc473df2739d

// TSZ_INLINE_TEST_BEGIN ea941139b4fb5b1e1ef08424b5afcb6886e066880f32bcec08ab854d4ab9a446 2052 parameter_leading_jsdoc_preserves_multiline_parameter_list_shape
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
// TSZ_INLINE_TEST_END ea941139b4fb5b1e1ef08424b5afcb6886e066880f32bcec08ab854d4ab9a446

// TSZ_INLINE_TEST_BEGIN 408924d2abf2286bb6de49c8c8a0709e5645ebb0e98cdd8a7a5772a193d558af 2079 empty_param_name_dropped
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
// TSZ_INLINE_TEST_END 408924d2abf2286bb6de49c8c8a0709e5645ebb0e98cdd8a7a5772a193d558af

// TSZ_INLINE_TEST_BEGIN 3b6d918ce9e272a0e2fc8ce361cb4c2ca9e61bb95c1608e29f9de3a2807c4255 2101 omitted_call_args_dropped
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
// TSZ_INLINE_TEST_END 3b6d918ce9e272a0e2fc8ce361cb4c2ca9e61bb95c1608e29f9de3a2807c4255
