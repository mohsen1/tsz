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
