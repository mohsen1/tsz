use tsz_common::common::ScriptTarget;
use tsz_emitter::output::printer::{PrintOptions, lower_and_print};
use tsz_parser::parser::ParserState;

fn emit_with_target(source: &str, target: ScriptTarget) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    lower_and_print(
        &parser.arena,
        root,
        PrintOptions {
            target,
            ..PrintOptions::default()
        },
    )
    .code
}

#[test]
fn es2015_arrow_binding_param_class_static_uses_native_prologue() {
    let output = emit_with_target(
        "(({ [class { static x = 1 }.x]: b = \"\" }) => {})();",
        ScriptTarget::ES2015,
    );

    assert!(
        output.contains("((_a) => { var _b; var { [(_b = class {"),
        "Arrow binding parameter should be moved to a native body prologue.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_b.x = 1,"),
        "Class static field side effect should stay inside the binding key expression.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_b).x]: b = \"\" } = _a; })();"),
        "Native destructuring should read from the forwarded parameter temp.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("(({ [("),
        "Original destructuring parameter should not remain in the arrow head.\nOutput:\n{output}"
    );
}
