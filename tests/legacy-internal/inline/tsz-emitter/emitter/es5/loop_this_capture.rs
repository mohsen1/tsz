//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/emitter/es5/loop_this_capture.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN e4accb7a4f8006def345c49a3f21d633506ad6e1127bcc7a02a7392b83b45b7e 139 converted_for_of_with_this_in_arrow_captures_this_1
    // A converted for-of loop whose body uses `this` (here through a captured
    // arrow) must capture the enclosing `this` into `var this_1 = this;` at the
    // function scope and rewrite the body reference to `this_1`. The rule is
    // keyed on the converted-loop body referencing lexical `this`, not on any
    // identifier spelling, so renaming the loop/binding vars must not change it.
    #[test]
    fn converted_for_of_with_this_in_arrow_captures_this_1() {
        for (loop_var, method) in [("x", "use"), ("element", "handle")] {
            let source = format!(
                "class C {{\n\
                    run(xs: any[]) {{\n\
                        for (const {loop_var} of xs) {{\n\
                            this.{method}(() => {loop_var});\n\
                        }}\n\
                    }}\n\
                    {method}(f: any) {{}}\n\
                }}\n"
            );

            let output = emit_es5(&source);

            assert!(
                output.contains("var this_1 = this;"),
                "Converted loop body referencing `this` must capture it at the function scope.\nOutput:\n{output}"
            );
            assert!(
                output.contains(&format!("this_1.{method}(")),
                "Body `this` must be rewritten to the captured `this_1` binding.\nOutput:\n{output}"
            );
            assert!(
                !output.contains(&format!("this.{method}(")),
                "No bare `this` should remain inside the converted-loop IIFE body.\nOutput:\n{output}"
            );
        }
    }
// TSZ_INLINE_TEST_END e4accb7a4f8006def345c49a3f21d633506ad6e1127bcc7a02a7392b83b45b7e

// TSZ_INLINE_TEST_BEGIN bb02baa4ed3ab5629b4b0dbbe952e77bdf9d19de2a4a844acbbbefe899a5e4f1 170 converted_for_of_coalesces_this_capture_and_body_var_hoists
    #[test]
    fn converted_for_of_coalesces_this_capture_and_body_var_hoists() {
        let output = emit_es5(
            "class C {\n\
                run(xs: any[]) {\n\
                    for (const item of xs) {\n\
                        var leaked;\n\
                        this.use(() => item);\n\
                    }\n\
                }\n\
                use(f: any) {}\n\
            }\n",
        );

        assert!(
            output.contains("var this_1 = this, leaked;"),
            "Converted for-of preamble should coalesce lexical `this` capture and body `var` hoists.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("var this_1 = this;\n        var leaked;"),
            "Body `var` hoist should not be emitted as a separate declaration after the lexical `this` capture.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END bb02baa4ed3ab5629b4b0dbbe952e77bdf9d19de2a4a844acbbbefe899a5e4f1

// TSZ_INLINE_TEST_BEGIN d06d971cf434f9be739f255d147cded2cd9b04b5fe7621f3744c440f9ca2ed0c 198 nested_loops_witness_shape_captures_this_after_loop_fn_decl
    // Reproduces the `nestedLoops` witness shape: the `this` site is at the
    // loop body level and the inner arrow only closes over the loop bindings.
    // The capture decl is emitted at the function scope (after the `_loop_N`
    // definition) and the body `this` is rewritten.
    #[test]
    fn nested_loops_witness_shape_captures_this_after_loop_fn_decl() {
        let source = "class Test {\n\
            constructor() {\n\
                let outerArray: number[] = [1, 2, 3];\n\
                let innerArray: number[] = [1, 2, 3];\n\
                for (let outer of outerArray)\n\
                    for (let inner of innerArray) {\n\
                        this.aFunction((n, o) => { let x = outer + inner + n; });\n\
                    }\n\
            }\n\
            aFunction(f: (n: any, o: any) => void): void {}\n\
        }\n";

        let output = emit_es5(source);

        // The capture is declared AFTER the outer `_loop_1` definition, at the
        // function scope, before the driving for-loop.
        let loop_decl = output
            .find("var _loop_1 = function")
            .expect("outer loop must convert");
        let capture_decl = output
            .find("var this_1 = this;")
            .expect("converted body referencing `this` must capture it");
        assert!(
            capture_decl > loop_decl,
            "`var this_1 = this;` must follow the `_loop_1` definition.\nOutput:\n{output}"
        );
        assert!(
            output.contains("this_1.aFunction("),
            "Inner-loop body `this` must be rewritten to the capture.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("this.aFunction("),
            "No bare `this` should remain inside the converted loops.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END d06d971cf434f9be739f255d147cded2cd9b04b5fe7621f3744c440f9ca2ed0c

// TSZ_INLINE_TEST_BEGIN 9c1d150233ae2907300ad57075cddac4352bbe674e56d9631eb429eeb67997c5 240 converted_for_of_direct_this_in_body_is_rewritten
    // A `this` that appears DIRECTLY in the converted-loop body (outside any
    // closure) is captured just like one inside an escaping arrow. Here the
    // loop converts because an escaping closure captures the loop variable,
    // and the direct `this.note(x)` in the body is rewritten too.
    #[test]
    fn converted_for_of_direct_this_in_body_is_rewritten() {
        for loop_var in ["x", "row"] {
            let source = format!(
                "class C {{\n\
                    run(xs: any[]) {{\n\
                        for (const {loop_var} of xs) {{\n\
                            this.note({loop_var});\n\
                            this.store(function () {{ return {loop_var}; }});\n\
                        }}\n\
                    }}\n\
                    note(x: any) {{}}\n\
                    store(f: any) {{}}\n\
                }}\n"
            );

            let output = emit_es5(&source);

            assert!(
                output.contains("var _loop_1 = function ("),
                "Loop must convert because the escaping closure captures the loop var.\nOutput:\n{output}"
            );
            assert!(
                output.contains("var this_1 = this;"),
                "Converted body with a direct `this` reference must capture it.\nOutput:\n{output}"
            );
            assert!(
                output.contains("this_1.note(") && output.contains("this_1.store("),
                "Both the direct and closure-passing `this` calls must use the capture.\nOutput:\n{output}"
            );
        }
    }
// TSZ_INLINE_TEST_END 9c1d150233ae2907300ad57075cddac4352bbe674e56d9631eb429eeb67997c5

// TSZ_INLINE_TEST_BEGIN cca231d49ccf5de0499add9dcf21a532295bca2b8250a5416a7f35390475420a 276 nested_converted_loops_share_single_this_capture
    // Nested converted loops share ONE capture allocated by the outermost loop.
    // The inner loop body's `this` resolves to the same `this_1`; no second
    // `this_2` capture is emitted. Renaming vars must not change this.
    #[test]
    fn nested_converted_loops_share_single_this_capture() {
        for (outer, inner) in [("outer", "inner"), ("p", "q")] {
            let source = format!(
                "class C {{\n\
                    run(a: any[], b: any[]) {{\n\
                        for (const {outer} of a)\n\
                            for (const {inner} of b) {{\n\
                                this.use(() => {outer} + {inner});\n\
                            }}\n\
                    }}\n\
                    use(f: any) {{}}\n\
                }}\n"
            );

            let output = emit_es5(&source);

            assert!(
                output.contains("var this_1 = this;"),
                "Outermost converted loop must own the single `this` capture.\nOutput:\n{output}"
            );
            assert!(
                !output.contains("var this_2 = this;"),
                "Nested converted loops must inherit, not re-capture, `this`.\nOutput:\n{output}"
            );
            assert!(
                output.contains("this_1.use("),
                "Inner loop body `this` must resolve to the inherited capture.\nOutput:\n{output}"
            );
        }
    }
// TSZ_INLINE_TEST_END cca231d49ccf5de0499add9dcf21a532295bca2b8250a5416a7f35390475420a

// TSZ_INLINE_TEST_BEGIN f711e51d931f648dd78a7f0f206a3c31fc3457ddeb5082f0fb3b1d17baa759c1 311 converted_loop_this_inside_nested_function_does_not_capture
    // A converted loop body whose `this` only appears inside a NESTED regular
    // function (which owns its own `this`) must NOT trigger a capture: that
    // `this` is not the enclosing lexical `this`.
    #[test]
    fn converted_loop_this_inside_nested_function_does_not_capture() {
        let source = "class C {\n\
            run(xs: any[]) {\n\
                for (const x of xs) {\n\
                    var f = function () { return this; };\n\
                    (() => x)();\n\
                }\n\
            }\n\
        }\n";

        let output = emit_es5(source);

        assert!(
            output.contains("var _loop_1 = function (x)"),
            "Loop must convert because `x` is captured.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("var this_1 = this;"),
            "A `this` owned by a nested regular function must not force a capture.\nOutput:\n{output}"
        );
        assert!(
            output.contains("return this;"),
            "The nested function keeps its own `this` unrewritten.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END f711e51d931f648dd78a7f0f206a3c31fc3457ddeb5082f0fb3b1d17baa759c1
