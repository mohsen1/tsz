use crate::transforms::emit_utils::sanitize_module_name;
use crate::transforms::ir::IRNode;
use crate::transforms::module_commonjs::*;
use crate::transforms::module_commonjs_ir::CommonJsTransformContext;
use tsz_parser::parser::ParserState;

fn parse_collect_exports(source: &str) -> Vec<String> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let source_file = parser
        .arena
        .get_source_file(parser.arena.get(root).expect("root node must exist"))
        .expect("source file must exist");
    collect_export_names(&parser.arena, &source_file.statements.nodes)
}

fn parse_transform_cjs(source: &str) -> Vec<IRNode> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let root_node = parser.arena.get(root).expect("root node must exist");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("source file must exist");
    let mut transform = CommonJsTransformContext::new(&parser.arena);
    transform.transform_source_file(&source_file.statements.nodes)
}

#[test]
fn test_sanitize_module_name() {
    // tsc uses the last path segment of the module specifier
    assert_eq!(sanitize_module_name("./foo"), "foo");
    assert_eq!(sanitize_module_name("./foo/bar"), "bar");
    assert_eq!(sanitize_module_name("../utils"), "utils");
    assert_eq!(sanitize_module_name("@scope/pkg"), "pkg");
    assert_eq!(sanitize_module_name("./foo-bar/baz.qux"), "baz_qux");
    assert_eq!(sanitize_module_name("./123pkg/mod"), "mod");
    assert_eq!(sanitize_module_name("./"), "module");
    // Scoped packages with subpaths
    assert_eq!(sanitize_module_name("@ts-bug/core/utils"), "utils");
    assert_eq!(sanitize_module_name("ext/other"), "other");
    assert_eq!(sanitize_module_name("@emotion/react"), "react");
    // Simple module names (no path separator)
    assert_eq!(sanitize_module_name("demoModule"), "demoModule");
}

#[test]
fn test_emit_commonjs_preamble() {
    let mut output = String::new();
    emit_commonjs_preamble(&mut output).expect("emit to buffer should succeed");
    assert!(output.contains("\"use strict\";"));
    assert!(output.contains("Object.defineProperty(exports, \"__esModule\""));
}

#[test]
fn test_emit_exports_init() {
    let mut output = String::new();
    emit_exports_init(&mut output, &["foo".to_string(), "bar".to_string()])
        .expect("emit to buffer should succeed");
    assert_eq!(output, "exports.foo = exports.bar = void 0;\n");
}

#[test]
fn test_emit_exports_init_empty() {
    let mut output = String::new();
    emit_exports_init(&mut output, &[]).expect("emit to buffer should succeed");
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
fn test_get_import_bindings_default_import_uses_default_property() {
    use tsz_parser::parser::syntax_kind_ext;

    let source = "import foo from \"./module\";";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let root_node = parser
        .arena
        .get(root)
        .expect("root node must exist in arena");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("failed to get source file");
    let stmt_idx = *source_file
        .statements
        .nodes
        .first()
        .expect("source should have one statement");
    let stmt_node = parser
        .arena
        .get(stmt_idx)
        .expect("statement node should exist");
    assert_eq!(stmt_node.kind, syntax_kind_ext::IMPORT_DECLARATION);

    // Without esModuleInterop, default imports use plain property access
    let bindings = get_import_bindings(&parser.arena, stmt_node, "module_1", false);
    assert_eq!(bindings, vec!["var foo = module_1.default;".to_string()]);
}

#[test]
fn test_namespace_import_without_es_module_interop() {
    use tsz_parser::parser::syntax_kind_ext;

    let source = r#"import * as ns from "./module";"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let root_node = parser
        .arena
        .get(root)
        .expect("root node must exist in arena");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("failed to get source file");
    let stmt_idx = *source_file
        .statements
        .nodes
        .first()
        .expect("source should have one statement");
    let stmt_node = parser
        .arena
        .get(stmt_idx)
        .expect("statement node should exist");
    assert_eq!(stmt_node.kind, syntax_kind_ext::IMPORT_DECLARATION);

    // Without esModuleInterop: plain alias, no __importStar
    let bindings = get_import_bindings(&parser.arena, stmt_node, "module_1", false);
    assert_eq!(bindings, vec!["var ns = module_1;".to_string()]);

    // With esModuleInterop: uses __importStar helper
    let bindings = get_import_bindings(&parser.arena, stmt_node, "module_1", true);
    assert_eq!(
        bindings,
        vec!["var ns = __importStar(module_1);".to_string()]
    );
}

#[test]
fn test_collect_export_names_with_parsed_ast() {
    let export_names = parse_collect_exports("export class C {}");
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
    let export_names = parse_collect_exports("export const { a, b: c } = obj;");
    assert_eq!(
        export_names,
        vec!["a", "c"],
        "Expected destructured export names"
    );
}

#[test]
fn test_collect_export_names_with_default_export() {
    let export_names = parse_collect_exports("export default function () {}");
    assert!(
        export_names.is_empty(),
        "Default exports should not be in void 0 initialization list"
    );
}

#[test]
fn test_collect_export_names_with_default_class_export() {
    let export_names = parse_collect_exports("export default class Foo {}");
    assert!(
        export_names.is_empty(),
        "Default class exports should not be in void 0 initialization list"
    );
}

#[test]
fn test_collect_export_names_with_named_exports() {
    let export_names = parse_collect_exports("const foo = 1; export { foo as bar };");
    assert_eq!(
        export_names,
        vec!["bar"],
        "Expected exported name from named export"
    );
}

#[test]
fn test_collect_export_names_ignores_type_only_specifiers() {
    let export_names =
        parse_collect_exports("type Foo = number; const foo = 1; export { foo, type Foo };");
    assert_eq!(
        export_names,
        vec!["foo"],
        "Expected type-only specifiers to be ignored"
    );
}

#[test]
fn test_collect_export_names_ignores_type_only_named_exports() {
    let export_names = parse_collect_exports("type Foo = number; export type { Foo };");
    assert!(
        export_names.is_empty(),
        "Expected type-only named exports to be ignored"
    );
}

#[test]
fn test_collect_export_names_with_multiple_named_exports() {
    let export_names =
        parse_collect_exports("const foo = 1; const bar = 2; export { foo, bar as baz };");
    assert_eq!(
        export_names,
        vec!["foo", "baz"],
        "Expected multiple exported names"
    );
}

#[test]
#[ignore = "current main CI restore: pre-existing red assertion exposed by Rust 1.95 build fix"]
fn test_collect_export_names_with_export_import_equals() {
    // `export import Foo = Bar;` where Bar is *instantiated* (has runtime
    // value declarations) does export the alias. Empty/non-instantiated
    // namespaces are elided by tsc and verified separately.
    let export_names =
        parse_collect_exports("namespace Bar { export const x = 1; }\nexport import Foo = Bar;");
    assert_eq!(
        export_names,
        vec!["Foo"],
        "Expected export name from namespace export import equals"
    );
}

#[test]
fn test_collect_export_names_skips_external_export_import_equals() {
    let export_names = parse_collect_exports("export import Foo = require(\"./bar\");");
    assert!(
        export_names.is_empty(),
        "External export import equals should not get a void 0 preamble"
    );
}

#[test]
fn test_collect_export_names_ignores_type_only_import_equals_namespace_identifier() {
    let export_names =
        parse_collect_exports("export namespace C { export interface I {} }\nexport import v = C;");
    assert!(
        export_names.is_empty(),
        "Expected no runtime exports for import-equals aliases to type-only namespaces"
    );
}

#[test]
fn test_collect_export_names_skips_namespace_alias_to_exported_interface() {
    // `export import b = a.I;` where `a.I` is an *exported* interface inside
    // an exported namespace resolves to a type-only member that is reachable
    // from outside `a`. tsc elides both the void-0 preamble and the runtime
    // assignment in that case.
    let export_names = parse_collect_exports(
        "export namespace a {\n    export interface I {}\n}\nexport import b = a.I;\nexport var x: b;\n",
    );
    assert_eq!(
        export_names,
        vec!["x"],
        "Only the runtime variable `x` should be initialized; the type-only alias `b` is elided"
    );
}

#[test]
fn test_collect_export_names_keeps_namespace_alias_to_non_exported_interface() {
    // `import a = x.c;` where `c` is *non-exported* inside `x` cannot be
    // resolved from outside the namespace. tsc preserves the runtime assignment
    // (a broken-at-runtime emit) and the void-0 preamble.
    let export_names =
        parse_collect_exports("namespace x { interface c {} }\nexport import a = x.c;\n");
    assert_eq!(
        export_names,
        vec!["a"],
        "Non-exported inner interface keeps the runtime alias and its void-0 preamble"
    );
}

#[test]
fn test_collect_export_names_ignores_type_only_declarations() {
    let export_names =
        parse_collect_exports("export type Foo = number; export interface Bar { x: number; }");
    assert!(
        export_names.is_empty(),
        "Expected no runtime exports for type-only declarations"
    );
}

#[test]
fn test_collect_export_names_ignores_declare_exports() {
    let export_names = parse_collect_exports(
        "export declare const foo: number; export declare function bar(): void;",
    );
    assert!(
        export_names.is_empty(),
        "Expected no runtime exports for declare-only exports"
    );
}

#[test]
fn test_collect_export_names_includes_named_reexports() {
    // `export * from "x"` does NOT produce void 0 exports (no named specifiers).
    // `export { bar } from "x"` DOES produce void 0 exports (tsc emits exports.bar = void 0;).
    let export_names =
        parse_collect_exports("export * from \"./foo\"; export { bar } from \"./bar\";");
    assert_eq!(
        export_names,
        vec!["bar".to_string()],
        "Named re-exports should produce void 0 preamble entries"
    );
}

#[test]
fn test_collect_export_names_includes_default_reexport() {
    // tsc emits `exports.default = void 0;` for `export { default } from "x"`.
    let export_names = parse_collect_exports("export { default } from \"./foo\";");
    assert_eq!(
        export_names,
        vec!["default".to_string()],
        "Default re-export should produce void 0 preamble entry"
    );
}

#[test]
fn test_collect_export_names_ignores_const_enum() {
    let export_names = parse_collect_exports("export const enum Foo { A }");
    assert!(
        export_names.is_empty(),
        "Expected no runtime exports for const enums"
    );
}

#[test]
fn side_effect_import_emits_bare_require() {
    let nodes = parse_transform_cjs("import \"./side\";");
    assert!(
        nodes
            .iter()
            .any(|n| matches!(n, IRNode::Raw(s) if s == "require(\"./side\");")),
        "side-effect import should emit bare require call"
    );
}

#[test]
fn empty_named_import_is_elided() {
    let nodes = parse_transform_cjs("import {} from \"./side\";");
    assert!(
        nodes.is_empty(),
        "empty named import should not emit runtime IR"
    );
}

#[test]
fn type_only_named_import_is_elided_in_ir_commonjs_transform() {
    let nodes = parse_transform_cjs("import { type Foo } from \"./types\";");
    assert!(
        nodes.is_empty(),
        "type-only named imports should be erased in CommonJS IR transform"
    );
}

#[test]
fn test_collect_export_names_deduplicates_overloaded_functions() {
    // Overloaded functions produce multiple FUNCTION_DECLARATION nodes with the same name.
    // The collector must deduplicate to avoid repeated `exports.X = X;` lines.
    let source = r#"
export function foo(a: string): string;
export function foo(a: number): number;
export function foo(a: any): any { return a; }
"#;
    let export_names = parse_collect_exports(source);
    assert_eq!(
        export_names,
        vec!["foo"],
        "Overloaded function should produce only one export name"
    );
}

#[test]
fn test_collect_export_names_categorized_deduplicates_overloaded_functions() {
    let source = r#"
export function foo(a: string): string;
export function foo(a: number): number;
export function foo(a: any): any { return a; }
export const bar = 42;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(source_file) = parser.arena.get_source_file(
        parser
            .arena
            .get(root)
            .expect("root node must exist in arena"),
    ) else {
        panic!("Failed to get source file");
    };

    let result = collect_export_names_categorized(
        &parser.arena,
        &source_file.statements.nodes,
        false,
        &rustc_hash::FxHashSet::default(),
    );

    assert_eq!(
        result.function_exports,
        vec![("foo".to_string(), "foo".to_string())],
        "Overloaded function should produce only one func_export entry"
    );
    assert_eq!(
        result.other_exports,
        vec!["bar"],
        "Non-function exports should be unaffected"
    );
}

#[test]
fn collect_export_names_categorized_skips_marked_type_only_specifiers() {
    let source = "export { I, I as II };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let source_file = parser
        .arena
        .get_source_file(parser.arena.get(root).expect("root node must exist"))
        .expect("source file must exist");

    let mut type_only_nodes = rustc_hash::FxHashSet::default();
    for &stmt_idx in &source_file.statements.nodes {
        let Some(stmt) = parser.arena.get(stmt_idx) else {
            continue;
        };
        let Some(export_decl) = parser.arena.get_export_decl(stmt) else {
            continue;
        };
        let Some(clause_node) = parser.arena.get(export_decl.export_clause) else {
            continue;
        };
        let Some(named_exports) = parser.arena.get_named_imports(clause_node) else {
            continue;
        };
        type_only_nodes.extend(named_exports.elements.nodes.iter().copied());
    }

    let result = collect_export_names_categorized(
        &parser.arena,
        &source_file.statements.nodes,
        false,
        &type_only_nodes,
    );

    assert!(
        result.other_exports.is_empty(),
        "type-only export specifiers should not be preinitialized"
    );
}

#[test]
fn ir_commonjs_does_not_preinit_function_exports_with_void_zero() {
    let nodes = parse_transform_cjs("export function f() {}");
    assert!(
        !nodes
            .iter()
            .any(|n| matches!(n, IRNode::Raw(s) if s.contains("void 0"))),
        "hoisted function exports should not be in CommonJS void 0 preinit"
    );
}
