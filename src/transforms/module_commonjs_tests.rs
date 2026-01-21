use super::module_commonjs::*;

#[test]
fn test_sanitize_module_name() {
    assert_eq!(sanitize_module_name("./foo"), "foo");
    assert_eq!(sanitize_module_name("./foo/bar"), "foo_bar");
    assert_eq!(sanitize_module_name("../utils"), "utils");
    assert_eq!(sanitize_module_name("@scope/pkg"), "_scope_pkg");
    assert_eq!(sanitize_module_name("./foo-bar/baz.qux"), "foo_bar_baz_qux");
}

#[test]
fn test_emit_commonjs_preamble() {
    let mut output = String::new();
    emit_commonjs_preamble(&mut output).unwrap();
    assert!(output.contains("\"use strict\";"));
    assert!(output.contains("Object.defineProperty(exports, \"__esModule\""));
}

#[test]
fn test_emit_exports_init() {
    let mut output = String::new();
    emit_exports_init(&mut output, &["foo".to_string(), "bar".to_string()]).unwrap();
    assert_eq!(output, "exports.foo = exports.bar = void 0;\n");
}

#[test]
fn test_emit_exports_init_empty() {
    let mut output = String::new();
    emit_exports_init(&mut output, &[]).unwrap();
    assert!(output.is_empty(), "Expected no output for empty exports");
}

#[test]
fn test_emit_export_assignment() {
    assert_eq!(emit_export_assignment("foo"), "exports.foo = foo;");
}

#[test]
fn test_emit_reexport_property() {
    let result = emit_reexport_property("foo", "module_1", "foo");
    assert!(result.contains("Object.defineProperty"));
    assert!(result.contains("enumerable: true"));
    assert!(result.contains("\"foo\""));
    assert!(result.contains("module_1.foo"));
}

#[test]
fn test_emit_reexport_property_alias() {
    let result = emit_reexport_property("foo", "module_1", "bar");
    assert!(result.contains("\"foo\""));
    assert!(result.contains("module_1.bar"));
}

#[test]
fn test_collect_export_names_with_parsed_ast() {
    use crate::parser::ParserState;
    use crate::parser::syntax_kind_ext;
    use crate::scanner::SyntaxKind;

    let source = "export class C {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(source_file) = parser
        .arena
        .get_source_file(parser.arena.get(root).unwrap())
    else {
        panic!("Failed to get source file");
    };

    // Debug: print the statements
    eprintln!(
        "Source file has {} statements",
        source_file.statements.nodes.len()
    );
    for (i, &stmt_idx) in source_file.statements.nodes.iter().enumerate() {
        if let Some(node) = parser.arena.get(stmt_idx) {
            eprintln!(
                "Statement {}: kind = {} (ClassDecl = {})",
                i,
                node.kind,
                syntax_kind_ext::CLASS_DECLARATION
            );

            if node.kind == syntax_kind_ext::CLASS_DECLARATION {
                if let Some(class) = parser.arena.get_class(node) {
                    eprintln!("  Found class, modifiers: {:?}", class.modifiers);
                    if let Some(modifiers) = &class.modifiers {
                        eprintln!("  Modifiers count: {}", modifiers.nodes.len());
                        for &mod_idx in &modifiers.nodes {
                            if let Some(mod_node) = parser.arena.get(mod_idx) {
                                eprintln!(
                                    "    Modifier kind: {} (Export = {})",
                                    mod_node.kind,
                                    SyntaxKind::ExportKeyword as u16
                                );
                            }
                        }
                    }
                    if let Some(name_node) = parser.arena.get(class.name) {
                        if let Some(ident) = parser.arena.get_identifier(name_node) {
                            eprintln!("  Class name: {}", ident.escaped_text);
                        }
                    }
                }
            }
        }
    }

    let export_names = collect_export_names(&parser.arena, &source_file.statements.nodes);

    eprintln!("Collected export names: {:?}", export_names);

    assert!(
        !export_names.is_empty(),
        "Expected to find exported class name"
    );
    assert_eq!(
        export_names,
        vec!["C"],
        "Expected to find class name 'C' in exports"
    );
}

#[test]
fn test_collect_export_names_with_destructuring() {
    use crate::parser::ParserState;

    let source = "export const { a, b: c } = obj;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(source_file) = parser
        .arena
        .get_source_file(parser.arena.get(root).unwrap())
    else {
        panic!("Failed to get source file");
    };

    let export_names = collect_export_names(&parser.arena, &source_file.statements.nodes);

    assert_eq!(
        export_names,
        vec!["a", "c"],
        "Expected destructured export names"
    );
}

#[test]
fn test_collect_export_names_with_default_export() {
    use crate::parser::ParserState;

    let source = "export default function () {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(source_file) = parser
        .arena
        .get_source_file(parser.arena.get(root).unwrap())
    else {
        panic!("Failed to get source file");
    };

    let export_names = collect_export_names(&parser.arena, &source_file.statements.nodes);

    assert_eq!(
        export_names,
        vec!["default"],
        "Expected default export name"
    );
}

#[test]
fn test_collect_export_names_with_default_class_export() {
    use crate::parser::ParserState;

    let source = "export default class Foo {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(source_file) = parser
        .arena
        .get_source_file(parser.arena.get(root).unwrap())
    else {
        panic!("Failed to get source file");
    };

    let export_names = collect_export_names(&parser.arena, &source_file.statements.nodes);

    assert_eq!(
        export_names,
        vec!["default"],
        "Expected default export name for class"
    );
}

#[test]
fn test_collect_export_names_with_named_exports() {
    use crate::parser::ParserState;

    let source = "const foo = 1; export { foo as bar };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(source_file) = parser
        .arena
        .get_source_file(parser.arena.get(root).unwrap())
    else {
        panic!("Failed to get source file");
    };

    let export_names = collect_export_names(&parser.arena, &source_file.statements.nodes);

    assert_eq!(
        export_names,
        vec!["bar"],
        "Expected exported name from named export"
    );
}

#[test]
fn test_collect_export_names_ignores_type_only_specifiers() {
    use crate::parser::ParserState;

    let source = "type Foo = number; const foo = 1; export { foo, type Foo };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(source_file) = parser
        .arena
        .get_source_file(parser.arena.get(root).unwrap())
    else {
        panic!("Failed to get source file");
    };

    let export_names = collect_export_names(&parser.arena, &source_file.statements.nodes);

    assert_eq!(
        export_names,
        vec!["foo"],
        "Expected type-only specifiers to be ignored"
    );
}

#[test]
fn test_collect_export_names_ignores_type_only_named_exports() {
    use crate::parser::ParserState;

    let source = "type Foo = number; export type { Foo };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(source_file) = parser
        .arena
        .get_source_file(parser.arena.get(root).unwrap())
    else {
        panic!("Failed to get source file");
    };

    let export_names = collect_export_names(&parser.arena, &source_file.statements.nodes);

    assert!(
        export_names.is_empty(),
        "Expected type-only named exports to be ignored"
    );
}

#[test]
fn test_collect_export_names_with_multiple_named_exports() {
    use crate::parser::ParserState;

    let source = "const foo = 1; const bar = 2; export { foo, bar as baz };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(source_file) = parser
        .arena
        .get_source_file(parser.arena.get(root).unwrap())
    else {
        panic!("Failed to get source file");
    };

    let export_names = collect_export_names(&parser.arena, &source_file.statements.nodes);

    assert_eq!(
        export_names,
        vec!["foo", "baz"],
        "Expected multiple exported names"
    );
}

#[test]
fn test_collect_export_names_with_export_import_equals() {
    use crate::parser::ParserState;

    let source = "export import Foo = require(\"./bar\");";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(source_file) = parser
        .arena
        .get_source_file(parser.arena.get(root).unwrap())
    else {
        panic!("Failed to get source file");
    };

    let export_names = collect_export_names(&parser.arena, &source_file.statements.nodes);

    assert_eq!(
        export_names,
        vec!["Foo"],
        "Expected export name from export import equals"
    );
}

#[test]
fn test_collect_export_names_ignores_type_only_declarations() {
    use crate::parser::ParserState;

    let source = "export type Foo = number; export interface Bar { x: number; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(source_file) = parser
        .arena
        .get_source_file(parser.arena.get(root).unwrap())
    else {
        panic!("Failed to get source file");
    };

    let export_names = collect_export_names(&parser.arena, &source_file.statements.nodes);

    assert!(
        export_names.is_empty(),
        "Expected no runtime exports for type-only declarations"
    );
}

#[test]
fn test_collect_export_names_ignores_declare_exports() {
    use crate::parser::ParserState;

    let source = "export declare const foo: number; export declare function bar(): void;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(source_file) = parser
        .arena
        .get_source_file(parser.arena.get(root).unwrap())
    else {
        panic!("Failed to get source file");
    };

    let export_names = collect_export_names(&parser.arena, &source_file.statements.nodes);

    assert!(
        export_names.is_empty(),
        "Expected no runtime exports for declare-only exports"
    );
}

#[test]
fn test_collect_export_names_ignores_reexports() {
    use crate::parser::ParserState;

    let source = "export * from \"./foo\"; export { bar } from \"./bar\";";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(source_file) = parser
        .arena
        .get_source_file(parser.arena.get(root).unwrap())
    else {
        panic!("Failed to get source file");
    };

    let export_names = collect_export_names(&parser.arena, &source_file.statements.nodes);

    assert!(
        export_names.is_empty(),
        "Expected no runtime exports for re-exports"
    );
}

#[test]
fn test_collect_export_names_ignores_default_reexport() {
    use crate::parser::ParserState;

    let source = "export { default } from \"./foo\";";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(source_file) = parser
        .arena
        .get_source_file(parser.arena.get(root).unwrap())
    else {
        panic!("Failed to get source file");
    };

    let export_names = collect_export_names(&parser.arena, &source_file.statements.nodes);

    assert!(
        export_names.is_empty(),
        "Expected no runtime exports for default re-export"
    );
}

#[test]
fn test_collect_export_names_ignores_const_enum() {
    use crate::parser::ParserState;

    let source = "export const enum Foo { A }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(source_file) = parser
        .arena
        .get_source_file(parser.arena.get(root).unwrap())
    else {
        panic!("Failed to get source file");
    };

    let export_names = collect_export_names(&parser.arena, &source_file.statements.nodes);

    assert!(
        export_names.is_empty(),
        "Expected no runtime exports for const enums"
    );
}
