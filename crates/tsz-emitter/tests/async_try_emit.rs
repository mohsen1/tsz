use tsz_common::common::ScriptTarget;
use tsz_emitter::output::printer::{PrintOptions, lower_and_print};
use tsz_parser::parser::ParserState;

fn emit_es5(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let opts = PrintOptions {
        target: ScriptTarget::ES5,
        remove_comments: true,
        ..PrintOptions::default()
    };
    lower_and_print(&parser.arena, root, opts).code
}

#[test]
fn async_catch_bindings_use_numbered_generator_temps() {
    let output = emit_es5(
        "declare var x, y: any;
        async function f(): Promise<Function> {
            try {
                await x;
            }
            catch (e) {
                return () => e;
            }
        }
        async function g() {
            try {
                x;
            }
            catch (e) {
                await y;
            }
        }",
    );

    assert!(
        output.contains("var e_1;"),
        "The first lowered async catch binding should hoist an `e_1` temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("e_1 = _a.sent();\n                    return [2 /*return*/, function () { return e_1; }];"),
        "Catch-body references should be rewritten to the generated catch temp without a trailing break after return.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(
            "return [2 /*return*/, function () { return e_1; }];\n                    return [3"
        ),
        "A terminating catch body should not receive a synthetic break.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var e_2;"),
        "Later lowered async catch bindings should continue the source-file ordinal.\nOutput:\n{output}"
    );
    assert!(
        output.contains("e_2 = _a.sent();\n                    return [4 /*yield*/, y];"),
        "The second lowered async catch should bind `_a.sent()` to `e_2` before its awaited body.\nOutput:\n{output}"
    );
}
