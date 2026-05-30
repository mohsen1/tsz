//! `this`-capture for ES5 converted loops (`_loop_N` IIFE pattern).
//!
//! When a loop is converted to a `_loop_N` IIFE (because its body captures a
//! block-scoped binding) and the loop body lexically references `this`, the
//! body is now inside a `function` and a bare `this` would resolve to the
//! IIFE's own receiver rather than the enclosing function's `this`. tsc
//! captures the enclosing `this` into a `var this_N = this;` temp declared at
//! the real function scope and rewrites the body references to `this_N`:
//!
//! ```typescript
//! for (const x of xs) {
//!     this.use(() => x);
//! }
//! ```
//! becomes (ES5):
//! ```javascript
//! var _loop_1 = function (x) {
//!     this_1.use(function () { return x; });
//! };
//! var this_1 = this;
//! for (var _i = 0, xs_1 = xs; _i < xs_1.length; _i++) {
//!     var x = xs_1[_i];
//!     _loop_1(x);
//! }
//! ```
//!
//! Only the OUTERMOST converted loop owns the capture: nested converted loops
//! inherit the same `this_N` binding (the structural condition is "a converted
//! loop whose body lexically references `this`, with no enclosing converted
//! loop already capturing `this`"). This mirrors the existing `arguments_N`
//! capture machinery for async lowering.

use super::super::Printer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::syntax::transform_utils::contains_this_reference;

/// Token returned by [`Printer::begin_loop_iife_this_capture`] that records
/// whether this converted loop owns a freshly-allocated `this_N` capture and
/// the previous capture name to restore once the IIFE body has been emitted.
pub(in crate::emitter) struct LoopThisCaptureScope {
    /// Capture name to declare (`var this_N = this;`) once the IIFE body is
    /// emitted, when this loop owns the capture. `None` when an enclosing
    /// converted loop already captured `this`.
    owned_capture_name: Option<String>,
    /// Previous `loop_this_capture_name` to restore after the body.
    previous: Option<String>,
}

impl<'a> Printer<'a> {
    fn next_loop_this_capture_name(&mut self) -> String {
        loop {
            self.ctx.loop_this_capture_counter += 1;
            let candidate = format!("this_{}", self.ctx.loop_this_capture_counter);
            if !self.file_identifiers.contains(&candidate) {
                return candidate;
            }
        }
    }

    /// Activate `this` -> `this_N` substitution for a converted-loop IIFE body
    /// that lexically references `this`. Must be paired with
    /// [`Printer::end_loop_iife_this_capture`] after the body is emitted.
    ///
    /// The outermost converted loop allocates a fresh `this_N` binding; nested
    /// converted loops inherit the active binding. Returns a scope token whose
    /// owned capture name (if any) should be declared by the caller via
    /// [`Printer::emit_loop_this_capture_decl`] after the IIFE definition.
    pub(in crate::emitter) fn begin_loop_iife_this_capture(
        &mut self,
        body_idx: NodeIndex,
    ) -> LoopThisCaptureScope {
        let previous = self.ctx.loop_this_capture_name.clone();

        // An enclosing converted loop already captured `this`; the body simply
        // inherits that binding. Bodies that do not reference lexical `this`
        // need no capture at all.
        if previous.is_some() || !contains_this_reference(self.arena, body_idx) {
            return LoopThisCaptureScope {
                owned_capture_name: None,
                previous,
            };
        }

        let capture_name = self.next_loop_this_capture_name();
        self.ctx.loop_this_capture_name = Some(capture_name.clone());
        LoopThisCaptureScope {
            owned_capture_name: Some(capture_name),
            previous,
        }
    }

    /// Restore the substitution state saved by
    /// [`Printer::begin_loop_iife_this_capture`] once the IIFE body has been
    /// emitted, returning the owned capture name (if this loop owns one) so the
    /// caller can declare `var this_N = this;` at the function scope.
    pub(in crate::emitter) fn end_loop_iife_this_capture(
        &mut self,
        scope: LoopThisCaptureScope,
    ) -> Option<String> {
        self.ctx.loop_this_capture_name = scope.previous;
        scope.owned_capture_name
    }

    /// Emit `var this_N = this;` at the current (function-scope) indentation
    /// when the converted loop owns a freshly-allocated capture.
    pub(in crate::emitter) fn emit_loop_this_capture_decl(&mut self, owned_capture_name: &str) {
        self.write("var ");
        self.write(owned_capture_name);
        self.write(" = this;");
        self.write_line();
    }
}

#[cfg(test)]
mod tests {
    use crate::output::printer::{PrintOptions, lower_and_print};
    use tsz_common::ScriptTarget;
    use tsz_parser::ParserState;

    fn emit_es5(source: &str) -> String {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        lower_and_print(
            &parser.arena,
            root,
            PrintOptions {
                target: ScriptTarget::ES5,
                ..Default::default()
            },
        )
        .code
    }

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
}
