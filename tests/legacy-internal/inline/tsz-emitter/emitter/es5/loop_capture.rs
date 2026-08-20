//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/emitter/es5/loop_capture.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 92d119eb498bc52f06cce62b07c6177b46569ecb29c59687ad203f5657df9212 989 do_loop_capture_renames_body_let_that_shadows_parameter
    #[test]
    fn do_loop_capture_renames_body_let_that_shadows_parameter() {
        let source = "function use(v: number) {}\n\
function foo(x: number) {\n\
  var v = 1;\n\
  do {\n\
    let x = v;\n\
    var v;\n\
    var v = 2;\n\
    () => x + v;\n\
  } while (false);\n\
\n\
  use(v);\n\
}\n";

        let output = emit_es5(source);

        assert!(
            output.contains("var x_1 = v;"),
            "Loop IIFE body let should be renamed when it shadows a parameter.\nOutput:\n{output}"
        );
        assert!(
            output.contains("return x_1 + v;"),
            "Captured arrow should reference the renamed body let.\nOutput:\n{output}"
        );
        assert!(
            output.contains("var v, v;"),
            "Loop body var hoist should preserve duplicate var declarations.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 92d119eb498bc52f06cce62b07c6177b46569ecb29c59687ad203f5657df9212

// TSZ_INLINE_TEST_BEGIN 450fb7f93f174c17028d5f4e3aa5e0acdad7b13a65b08df6d7b6ef68cff32a8c 1020 loop_capture_helper_body_uses_renamed_captured_parameter
    #[test]
    fn loop_capture_helper_body_uses_renamed_captured_parameter() {
        let source = "declare function keep(v: number): void;\n\
function foo() {\n\
  let i = 0;\n\
  for (let i = 0; i < 2; i++) {\n\
    (() => i)();\n\
  }\n\
  keep(i);\n\
}\n";

        let output = emit_es5(source);

        assert!(
            output.contains("var _loop_1 = function (i_1)"),
            "Loop helper signature should use the block-scoped emitted name.\nOutput:\n{output}"
        );
        assert!(
            output.contains("return i_1;"),
            "Helper body references should resolve through the same emitted parameter.\nOutput:\n{output}"
        );
        assert!(
            output.contains("_loop_1(i_1);"),
            "Loop helper call should pass the same emitted captured variable.\nOutput:\n{output}"
        );
        assert!(
            output.contains("keep(i);"),
            "Outer lexical binding should keep its own emitted name.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("return i;"),
            "Helper body must not drift back to the outer binding name.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 450fb7f93f174c17028d5f4e3aa5e0acdad7b13a65b08df6d7b6ef68cff32a8c

// TSZ_INLINE_TEST_BEGIN 0996a52844bec4afed49b0545d0aaac8c22058ced1ff638c9745a24d7b2f2928 1055 for_of_loop_capture_preserves_multiline_arrow_block_spacing
    #[test]
    fn for_of_loop_capture_preserves_multiline_arrow_block_spacing() {
        let source = "function foo() {\n\
    for (const i of [0, 1]) {\n\
        if (i === 0) {\n\
            continue;\n\
        }\n\
\n\
        (() => {\n\
            return i;\n\
        })();\n\
    }\n\
}\n";

        let output = emit_es5(source);

        assert!(
            output.contains("(function () {\n            return i;\n        })();"),
            "Captured loop arrow block should preserve its multiline block body.\nOutput:\n{output}"
        );
        assert!(
            output.contains("_loop_1(i);\n    }\n}"),
            "Captured for-of loop call should not leave an extra blank line before the loop closes.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("_loop_1(i);\n\n    }"),
            "Captured for-of loop call should emit exactly one line break before the closing brace.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 0996a52844bec4afed49b0545d0aaac8c22058ced1ff638c9745a24d7b2f2928

// TSZ_INLINE_TEST_BEGIN b31de73f9b30cd621567e51d3b4bdfcf345b848d4a5fc6bed1f8330d127a8b9d 1085 single_line_arrow_block_stays_compact_in_loop_capture
    #[test]
    fn single_line_arrow_block_stays_compact_in_loop_capture() {
        let source = "function foo() {\n\
    for (const i of [0, 1]) {\n\
        (() => { return i; })();\n\
    }\n\
}\n";

        let output = emit_es5(source);

        assert!(
            output.contains("(function () { return i; })();"),
            "Single-line arrow block should keep the compact ES5 function body.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END b31de73f9b30cd621567e51d3b4bdfcf345b848d4a5fc6bed1f8330d127a8b9d

// TSZ_INLINE_TEST_BEGIN 6e20c7ea330e06e098a307f8dfa83f50dd2b966aacb602aa250d78b6c8b773f7 1101 captured_initializerless_for_let_uses_void0_without_leaking_scope
    #[test]
    fn captured_initializerless_for_let_uses_void0_without_leaking_scope() {
        let source = "declare function use(a: any);\n\
var x;\n\
for (let x = 10; ;) {\n\
    use(x);\n\
}\n\
use(x);\n\
for (; ;) {\n\
    let x;\n\
    use(x);\n\
}\n";

        let output = emit_es5(source);

        assert!(
            output.contains("for (var x_1 = 10;;) {\n    use(x_1);\n}\nuse(x);"),
            "For-header lexical scope should not leak to the following statement.\nOutput:\n{output}"
        );
        assert!(
            output.contains("var x_2 = void 0;\n    use(x_2);"),
            "Initializerless block-scoped body declarations downlevel to `void 0`.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 6e20c7ea330e06e098a307f8dfa83f50dd2b966aacb602aa250d78b6c8b773f7

// TSZ_INLINE_TEST_BEGIN 935291c597eb587431a46dd683ec3e98a68949738e0e310cb0ed14aae44be660 1126 initializerless_lexical_declarations_reset_in_nested_es5_blocks
    #[test]
    fn initializerless_lexical_declarations_reset_in_nested_es5_blocks() {
        let source = "function plain() {\n\
    let x;\n\
    { let y; }\n\
    if (true) { let q; }\n\
}\n\
while (true) {\n\
    let z;\n\
    function nested() { let w; }\n\
}\n";

        let output = emit_es5(source);

        assert!(
            output
                .contains("function plain() {\n    var x;\n    {\n        var y = void 0;\n    }"),
            "Initializerless lexical declarations in nested ES5 blocks should reset on entry.\nOutput:\n{output}"
        );
        assert!(
            output.contains("if (true) {\n        var q = void 0;\n    }"),
            "Control-flow blocks should use the same ES5 lexical reset policy.\nOutput:\n{output}"
        );
        assert!(
            output.contains("while (true) {\n    var z = void 0;"),
            "Loop body blocks should keep resetting initializerless lexical declarations.\nOutput:\n{output}"
        );
        assert!(
            output.contains("function nested() { var w; }"),
            "Nested function bodies should not inherit the outer loop reset policy.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 935291c597eb587431a46dd683ec3e98a68949738e0e310cb0ed14aae44be660

// TSZ_INLINE_TEST_BEGIN 8d6c20bafae227c018a1914ab0988a3d678f607252ad76e74390b2f05571f014 1159 object_literal_methods_capture_for_initializer_let
    #[test]
    fn object_literal_methods_capture_for_initializer_let() {
        let source = "for (let x; ;) {\n\
    ({ foo() { x } });\n\
}\n\
for (let x; ;) {\n\
    ({ get foo() { return x } });\n\
}\n\
for (let x; ;) {\n\
    ({ set foo(v) { x } });\n\
}\n";

        let output = emit_es5(source);

        assert!(
            output.contains(
                "var _loop_1 = function (x) {\n    ({ foo: function () { x; } });\n};\nfor (var x = void 0;;) {\n    _loop_1(x);\n}"
            ),
            "Object literal methods should trigger loop capture and initialize the loop binding.\nOutput:\n{output}"
        );
        assert!(
            output.contains("var _loop_2 = function (x) {\n    ({ get foo() { return x; } });\n};"),
            "Object literal getters should trigger loop capture.\nOutput:\n{output}"
        );
        assert!(
            output.contains("var _loop_3 = function (x) {\n    ({ set foo(v) { x; } });\n};"),
            "Object literal setters should trigger loop capture.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 8d6c20bafae227c018a1914ab0988a3d678f607252ad76e74390b2f05571f014

// TSZ_INLINE_TEST_BEGIN 2753053943d2791be06be318e07b2d942b376bd8d783cf67e50d0b929f66a535 1194 nested_for_of_outer_loop_var_capture_converts_outer_loop
    // A closure that captures an OUTER for-of loop variable from inside a nested
    // for-of body must convert the outer loop too. Capture analysis must descend
    // into nested for-of statements (not only `for`/`while`/`do`). Renaming the
    // loop variables must not change the result — capture is by binding, not by
    // a hardcoded name.
    #[test]
    fn nested_for_of_outer_loop_var_capture_converts_outer_loop() {
        for (outer_var, inner_var) in [("outer", "inner"), ("p", "q")] {
            let source = format!(
                "function f(xs: any[], ys: any[]) {{\n\
                    for (const {outer_var} of xs)\n\
                        for (const {inner_var} of ys)\n\
                            (() => {outer_var} + {inner_var});\n\
                }}\n"
            );

            let output = emit_es5(&source);

            assert!(
                output.contains(&format!("var _loop_1 = function ({outer_var}) {{")),
                "Outer loop must convert with its own var as the helper parameter.\nOutput:\n{output}"
            );
            assert!(
                output.contains(&format!("var _loop_2 = function ({inner_var}) {{")),
                "Inner loop must convert with its own var as the helper parameter.\nOutput:\n{output}"
            );
            assert!(
                output.contains(&format!("_loop_1({outer_var});")),
                "Outer converted loop must be invoked with its iteration variable.\nOutput:\n{output}"
            );
        }
    }
// TSZ_INLINE_TEST_END 2753053943d2791be06be318e07b2d942b376bd8d783cf67e50d0b929f66a535

// TSZ_INLINE_TEST_BEGIN a76f13dca22d06c0161553fac1e2add83b0259542d85c90f2d5e4e5576c15614 1225 nested_loop_shadowing_same_name_does_not_convert_outer_loop
    // When a nested loop re-binds the same name as an enclosing loop variable, a
    // closure referencing that name inside the nested loop captures the INNER
    // binding, so the OUTER loop must NOT convert. This holds for any name.
    #[test]
    fn nested_loop_shadowing_same_name_does_not_convert_outer_loop() {
        for var_name in ["v", "z"] {
            let source = format!(
                "function f(xs: any[], ys: any[]) {{\n\
                    for (const {var_name} of xs)\n\
                        for (const {var_name} of ys)\n\
                            (() => {var_name});\n\
                }}\n"
            );

            let output = emit_es5(&source);

            assert!(
                output.contains("var _loop_1 = function"),
                "The inner loop (whose own binding is captured) must convert.\nOutput:\n{output}"
            );
            assert!(
                !output.contains("var _loop_2 = function"),
                "The outer loop must stay a plain for-loop because its binding is shadowed.\nOutput:\n{output}"
            );
        }
    }
// TSZ_INLINE_TEST_END a76f13dca22d06c0161553fac1e2add83b0259542d85c90f2d5e4e5576c15614

// TSZ_INLINE_TEST_BEGIN 6cf73f76b1b7df11489f74989e7d40490ccf340ed9e5e007aac2ae7eb79fc2a9 1253 converted_for_of_threads_all_binding_vars_even_when_uncaptured
    // Once a for-of loop is converted (because a BODY binding is captured), all
    // of the loop's own binding variables — including destructured names that
    // are NOT themselves captured — are threaded as helper parameters so each
    // iteration receives a fresh copy.
    #[test]
    fn converted_for_of_threads_all_binding_vars_even_when_uncaptured() {
        for (a, b) in [("value", "i"), ("first", "second")] {
            let source = format!(
                "declare function pairs(): any[];\n\
                function f() {{\n\
                    for (const [{a}, {b}] of pairs()) {{\n\
                        const bar: any = [];\n\
                        (() => bar);\n\
                    }}\n\
                }}\n"
            );

            let output = emit_es5(&source);

            assert!(
                output.contains(&format!("var _loop_1 = function ({a}, {b}) {{")),
                "Both for-of binding vars must be helper parameters, even though only `bar` is captured.\nOutput:\n{output}"
            );
            assert!(
                output.contains(&format!("_loop_1({a}, {b});")),
                "The converted loop call must pass both binding vars.\nOutput:\n{output}"
            );
        }
    }
// TSZ_INLINE_TEST_END 6cf73f76b1b7df11489f74989e7d40490ccf340ed9e5e007aac2ae7eb79fc2a9

// TSZ_INLINE_TEST_BEGIN 655a4d470824c7fcc22e37251d09d4b9af2c3bc987440be951cfb4bfe2f91f9c 1282 converted_loop_spread_method_call_captures_non_simple_receiver_in_iife_temp
    // A spread method call whose receiver is a non-simple expression captures the
    // receiver once into a hoisted temp to avoid double evaluation. Inside a
    // converted-loop IIFE body, that `var _a;` belongs to the IIFE body.
    #[test]
    fn converted_loop_spread_method_call_captures_non_simple_receiver_in_iife_temp() {
        for fn_var in ["value", "k"] {
            let source = format!(
                "declare function pairs(): any[];\n\
                function f(set: any) {{\n\
                    for (const {fn_var} of pairs()) {{\n\
                        const bar: any = [];\n\
                        (() => bar);\n\
                        set.values.push(...[]);\n\
                    }}\n\
                }}\n"
            );

            let output = emit_es5(&source);

            // Receiver captured once and reused; never evaluated twice.
            assert!(
                output.contains(").push.apply("),
                "Spread method call should lower to `.push.apply(...)`.\nOutput:\n{output}"
            );
            assert!(
                !output.contains("set.values.push.apply(set.values"),
                "Non-simple receiver must not be emitted twice in the apply call.\nOutput:\n{output}"
            );
            // The hoisted receiver temp declaration lives inside the IIFE body,
            // not at the enclosing function top.
            assert!(
                output.contains("var _loop_1 = function ("),
                "Loop must convert because `bar` is captured.\nOutput:\n{output}"
            );
            let iife_start = output.find("var _loop_1 = function (").unwrap();
            let iife_prefix = &output[..iife_start];
            assert!(
                !iife_prefix.contains("var _a;") && !iife_prefix.contains("var _b;"),
                "Receiver temp must not leak to the enclosing function before the IIFE.\nOutput:\n{output}"
            );
        }
    }
// TSZ_INLINE_TEST_END 655a4d470824c7fcc22e37251d09d4b9af2c3bc987440be951cfb4bfe2f91f9c

// TSZ_INLINE_TEST_BEGIN 7f9bde61e17b269282e8af617585fbb053dd3ef1d27ffa9eb72eaa15332e2af7 1325 non_loop_spread_method_call_captures_non_simple_receiver
    // Non-loop spread method call with a non-simple receiver also captures into a
    // hoisted temp at the enclosing function body top (the rule is general, not
    // loop-specific).
    #[test]
    fn non_loop_spread_method_call_captures_non_simple_receiver() {
        let source = "function f(set: any) {\n\
            set.values.push(...[]);\n\
        }\n";

        let output = emit_es5(source);

        assert!(
            output.contains("var _a;"),
            "Receiver temp should be hoisted at the function body top.\nOutput:\n{output}"
        );
        assert!(
            output.contains("(_a = set.values).push.apply(_a, [])"),
            "Non-simple receiver should be captured once and reused.\nOutput:\n{output}"
        );
        // A simple identifier receiver needs no temp.
        let simple = emit_es5("function g(arr: any[]) {\n    arr.push(...[1]);\n}\n");
        assert!(
            simple.contains("arr.push.apply(arr, [1])"),
            "Simple identifier receiver must not be captured into a temp.\nOutput:\n{simple}"
        );
    }
// TSZ_INLINE_TEST_END 7f9bde61e17b269282e8af617585fbb053dd3ef1d27ffa9eb72eaa15332e2af7
