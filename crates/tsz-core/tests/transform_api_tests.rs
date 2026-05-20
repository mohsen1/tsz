use crate::Parser;
use crate::emitter::{ModuleKind, ScriptTarget};

#[test]
fn test_generate_transforms_and_emit_with_context() {
    let mut parser = Parser::new(
        "test.ts".to_string(),
        "export class Foo { constructor(x) { this.x = x; } }".to_string(),
    );
    parser.parse_source_file();

    let transforms =
        parser.generate_transforms(ScriptTarget::ES5 as u32, ModuleKind::CommonJS as u32);
    assert!(transforms.get_count() > 0);

    let output = parser.emit_with_transforms(&transforms);
    assert!(output.contains("exports.Foo"));
}

#[test]
fn test_es5_namespace_enum_merge_avoids_duplicate_var_declaration() {
    let source = r#"
namespace enumdule {
    export class Point {}
}
enum enumdule {
    Red,
    Blue
}
"#;
    let mut parser = Parser::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let transforms = parser.generate_transforms(ScriptTarget::ES5 as u32, ModuleKind::None as u32);
    let output = parser.emit_with_transforms(&transforms);

    let occurrences = output.matches("var enumdule;").count();
    assert_eq!(
        occurrences, 1,
        "expected exactly one `var enumdule;`, got output:\n{output}"
    );
}
