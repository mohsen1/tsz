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
fn generator_object_literal_prefix_before_yield_omits_source_trailing_comma() {
    let output = emit_es5(
        "function* f() {
            const x = {
                a: 1,
                [g()]: 2,
                b: yield 3,
                c: 4,
            };
        }",
    );

    assert!(
        output.contains("_a = {\n                        a: 1\n                    },"),
        "Object-literal prefixes split before a generator yield should not inherit the full source trailing comma or extra computed-property indentation.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("a: 1,"),
        "The synthesized prefix object is not the full source literal, so it must not keep the source trailing comma.\nOutput:\n{output}"
    );
}

#[test]
fn async_object_literal_computed_suffix_reuses_materialized_object() {
    let output = emit_es5(
        "declare var x, y, z, b;
        async function f() {
            x = {
                a: await y,
                [b]: z
            };
        }",
    );

    assert!(
        output.contains(
            "_a.a = _b.sent(),\n                        _a[b] = z,\n                        _a)"
        ),
        "Async object literals should assign computed suffix properties on the materialized object without a result temp.\nOutput:\n{output}"
    );
}

#[test]
fn async_object_literal_multiple_awaited_properties_resume_sequentially() {
    let output = emit_es5(
        "async function f() {
            return {
                a: await Promise.resolve(0),
                b: await Promise.resolve(1),
                c: await Promise.resolve(2),
            };
        }",
    );

    assert!(
        output.contains(
            "_a.a = _b.sent();\n                    return [4 /*yield*/, Promise.resolve(1)];"
        ),
        "The first awaited property should resume into an assignment before yielding for the next property.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "_a.b = _b.sent();\n                    return [4 /*yield*/, Promise.resolve(2)];"
        ),
        "The second awaited property should also get its own resume case.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_a.a = _b.sent(),\n                        _a.b = _b.sent()"),
        "Later awaited properties must not collapse into one comma expression after the first yield.\nOutput:\n{output}"
    );
}
