use crate::emitter::ModuleKind;
use crate::output::printer::{PrintOptions, Printer};
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

/// Regression test: type-only import-equals inside a namespace must not
/// leave a phantom blank line. The import `import T = M1.I;` produces no
/// JS output (type-only alias), but `emit_namespace_body_statements` used
/// to call `write_line()` unconditionally, inserting an empty line between
/// the IIFE opening brace and the first real statement.
#[test]
fn no_blank_line_for_type_only_import_alias_in_namespace() {
    let source = "namespace M1 {\n    export interface I {\n        foo();\n    }\n}\n\nnamespace M2 {\n    import T = M1.I;\n    class C implements T {\n        foo() {}\n    }\n}";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    // The IIFE body should NOT have a blank line after the opening brace.
    assert!(
        !output.contains("(function (M2) {\n\n"),
        "Should not have blank line after IIFE opening brace.\nOutput:\n{output}"
    );

    // The class should still be emitted correctly inside M2's IIFE
    assert!(
        output.contains("class C {"),
        "Class C should be emitted inside namespace M2.\nOutput:\n{output}"
    );
}

#[test]
fn namespace_exported_destructuring_uses_temp_in_esnext_path() {
    let source = "namespace M {\n    export var [a, b] = [1, 2];\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("var _a;\n    _a = [1, 2], M.a = _a[0], M.b = _a[1];"),
        "Exported namespace destructuring should use one temp.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("M.a = [1, 2]"),
        "Exported namespace destructuring should not repeat the initializer.\nOutput:\n{output}"
    );
}

#[test]
fn namespace_exported_destructuring_temp_hoists_before_class() {
    let source = "namespace m {\n    export class c {}\n    export var [x, y] = [10, new c()];\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    let temp_pos = output.find("var _a;").expect("expected temp hoist");
    let class_pos = output.find("class c").expect("expected class emit");
    assert!(
        temp_pos < class_pos,
        "Namespace destructuring temp should hoist before class declarations.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = [10, new c()], m.x = _a[0], m.y = _a[1];"),
        "Exported namespace destructuring should use the hoisted temp.\nOutput:\n{output}"
    );
}

#[test]
fn top_level_import_alias_to_ambient_namespace_value_emits_runtime_alias() {
    let source = "declare namespace foo { const await: any; }\n\n// await allowed in import=namespace when not a module\nimport await = foo.await;\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(
        &parser.arena,
        PrintOptions {
            module: ModuleKind::ESNext,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("var await = foo.await;"),
        "Ambient namespace value aliases should be preserved in JS emit.\nOutput:\n{output}"
    );
}

#[test]
fn top_level_import_alias_to_ambient_namespace_value_is_erased_in_modules() {
    let source = "export {};\ndeclare namespace foo { const await: any; }\n\n// await disallowed in import=namespace when in a module\nimport await = foo.await;\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(
        &parser.arena,
        PrintOptions {
            module: ModuleKind::ESNext,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        !output.contains("var await = foo.await;"),
        "Module-scoped ambient namespace aliases should still be erased when unused.\nOutput:\n{output}"
    );
    assert!(
        output.contains("export {};"),
        "Module marker should be preserved when the alias is erased.\nOutput:\n{output}"
    );
}

/// When a namespace body has a variable with the same name as the namespace,
/// the IIFE parameter must be renamed to avoid collision.
/// E.g., `namespace m { export var m = ''; }` should emit `(function (m_1) { m_1.m = ''; })`.
#[test]
fn namespace_iife_param_renamed_for_variable_conflict() {
    let source = "namespace m {\n  export var m = '';\n}";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("(function (m_1)"),
        "Namespace IIFE parameter should be renamed to m_1 when body has 'var m'.\nOutput:\n{output}"
    );
    assert!(
        output.contains("m_1.m = '';"),
        "Exported variable should use renamed parameter m_1.\nOutput:\n{output}"
    );
}

/// When a namespace body has an import-equals with the same name as the namespace,
/// the IIFE parameter must be renamed.
/// E.g., `namespace A.M { import M = Z.M; ... }` should emit `(function (M_1) { ... })`.
#[test]
fn namespace_iife_param_renamed_for_import_equals_conflict() {
    let source = "namespace Z {\n  export namespace M {\n    export function bar() { return ''; }\n  }\n}\nnamespace A {\n  export namespace M {\n    import M = Z.M;\n    export function bar() {}\n    M.bar();\n  }\n}";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    // The inner M namespace IIFE should have parameter renamed to M_1
    assert!(
        output.contains("(function (M_1)"),
        "Namespace IIFE parameter should be renamed to M_1 when body has 'import M = ...'.\nOutput:\n{output}"
    );
}

/// When a dotted namespace `Y.Y` collides at every level (outer renamed to
/// `Y_1`, inner to `Y_2` because the body declares `enum Y`), the inner
/// IIFE's argument expression must reference the outer's renamed binding,
/// not the original name. The original name is shadowed inside the outer's
/// body by the `var Y;` we emit for the inner namespace.
#[test]
fn dotted_namespace_inner_iife_uses_outer_renamed_param_in_argument() {
    let source = "namespace Y.Y {\n  export enum Y { Red, Blue }\n}";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("})(Y = Y_1.Y || (Y_1.Y = {}));"),
        "Inner IIFE argument should reference the outer's renamed param Y_1.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("})(Y = Y.Y || (Y.Y = {}));"),
        "Inner IIFE argument must not reference the shadowed original Y.\nOutput:\n{output}"
    );
}

#[test]
fn dotted_namespace_reference_to_sibling_qualifies_parent_namespace() {
    let source = "function foo(title: string) {}\nnamespace foo.Bar {\n  export function f() {}\n}\nnamespace foo.Baz {\n  export function g() {\n    Bar.f();\n  }\n}";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("foo.Bar.f();"),
        "Sibling namespace reference should be qualified through the parent namespace.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("    Bar.f();"),
        "Sibling namespace reference should not be emitted as a bare identifier.\nOutput:\n{output}"
    );
}

#[test]
fn dotted_namespace_reopen_qualifies_prior_value_exports() {
    let source = "namespace X.Y {\n  class A {\n    m(Y: any) {\n      new B();\n    }\n  }\n}\nnamespace X.Y {\n  export class B {}\n}\nnamespace my.data {\n  export function buz() {}\n}\nnamespace my.data.foo {\n  function data(my, foo) {\n    buz();\n  }\n}";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("new Y_1.B();"),
        "Reopened dotted namespace should qualify prior class exports through the namespace object.\nOutput:\n{output}"
    );
    assert!(
        output.contains("data_1.buz();"),
        "Nested dotted namespace should qualify parent exports through the renamed IIFE parameter.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("\n            buz();"),
        "Parent namespace export should not remain a bare identifier.\nOutput:\n{output}"
    );
}

#[test]
fn namespace_exported_var_rewrites_computed_class_method_name() {
    let source = "namespace M {\n    export var Symbol;\n\n    class C {\n        [Symbol.iterator]() { }\n    }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("[M.Symbol.iterator]()"),
        "Computed method name should qualify exported namespace value.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("[Symbol.iterator]()"),
        "Computed method name should not keep the bare exported name.\nOutput:\n{output}"
    );

    let mut es5_printer = Printer::new(
        &parser.arena,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );
    es5_printer.set_source_text(source);
    es5_printer.print(root);
    let es5_output = es5_printer.finish().code;
    assert!(
        es5_output.contains("M.Symbol.iterator"),
        "ES5 namespace class lowering should also qualify computed method names.\nOutput:\n{es5_output}"
    );
}

#[test]
fn reopened_dotted_namespace_qualifies_merged_exports_by_source_path() {
    let source = "namespace my.data.foo {\n  export function child() {}\n}\nnamespace my.data {\n  export function buz() {}\n}\nnamespace my.data {\n  function data(my) {\n    foo.child();\n  }\n}\nnamespace my.data.foo {\n  function data(my, foo) {\n    buz();\n  }\n}";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("data_1.foo.child();"),
        "Reopened parent namespace should qualify dotted child namespace references.\nOutput:\n{output}"
    );
    assert!(
        output.contains("data_2.buz();"),
        "Nested namespace should qualify merged parent exports through the parent IIFE parameter.\nOutput:\n{output}"
    );
}

#[test]
fn nested_namespace_does_not_qualify_own_leaf_name_from_parent_exports() {
    let source = "namespace X.Y {\n  export namespace Point {\n    export var Origin = new Point(0, 0);\n  }\n}\nnamespace X.Y {\n  export class Point {}\n}";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("Point.Origin = new Point(0, 0);"),
        "Nested namespace should keep references to its own IIFE parameter bare.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("Point.Origin = new Y.Point(0, 0);"),
        "Parent exports must not reintroduce the current namespace leaf as a qualifier.\nOutput:\n{output}"
    );
}

#[test]
fn nested_namespace_uses_parent_current_class_lexically() {
    let source = "namespace A {\n  export class Point {\n    constructor(public x: number, public y: number) {}\n  }\n\n  export namespace B {\n    export var Origin: Point = new Point(0, 0);\n  }\n}";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("B.Origin = new Point(0, 0);"),
        "Nested namespace should use parent current-block classes through lexical scope.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("B.Origin = new A.Point(0, 0);"),
        "Current-block parent class should not be treated as a prior namespace-object export.\nOutput:\n{output}"
    );
}
