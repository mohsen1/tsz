use crate::emitter::ModuleKind;
use crate::output::printer::{PrintOptions, Printer};
use tsz_common::ScriptTarget;
fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

#[test]
fn namespace_recovers_malformed_export_function_arrow_body() {
    let source = "namespace M {\n    export namespace N {\n        export function f(x:number)=>2*x;\n    }\n}";
    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("2 * x;"),
        "Recovered malformed function arrow body should emit as a statement.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("function f"),
        "Recovered malformed function arrow body should not emit a function declaration.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("N.f = f"),
        "Recovered malformed function arrow body should not emit a namespace export assignment.\nOutput:\n{output}"
    );
}

#[test]
fn namespace_recovers_malformed_export_function_arrow_object_literal_body() {
    let source = "namespace M {\n    export namespace N {\n        export function f()=>({ a: 1 });\n    }\n}";
    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("({ a: 1 });"),
        "Recovered object-literal arrow body must stay parenthesized in statement position.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("\n        { a: 1 };"),
        "Recovered object-literal arrow body must not emit as a bare block statement.\nOutput:\n{output}"
    );
}

/// Regression test: type-only import-equals inside a namespace must not
/// leave a phantom blank line. The import `import T = M1.I;` produces no
/// JS output (type-only alias), but `emit_namespace_body_statements` used
/// to call `write_line()` unconditionally, inserting an empty line between
/// the IIFE opening brace and the first real statement.
#[test]
fn no_blank_line_for_type_only_import_alias_in_namespace() {
    let source = "namespace M1 {\n    export interface I {\n        foo();\n    }\n}\n\nnamespace M2 {\n    import T = M1.I;\n    class C implements T {\n        foo() {}\n    }\n}";

    let (parser, root) = parse_test_source(source);

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
fn namespace_anonymous_default_class_gets_synthetic_export_binding() {
    let source = "namespace ns_class {\n    export default class {}\n}\n\nnamespace ns_abstract_class {\n    export default abstract class {}\n}";
    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::new(
        &parser.arena,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("class default_1"),
        "First anonymous default class should get default_1 binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("ns_class.default_1 = default_1;"),
        "Namespace should export the first synthetic class binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("class default_2"),
        "Second anonymous default class should get default_2 binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("ns_abstract_class.default_2 = default_2;"),
        "Namespace should export the second synthetic class binding.\nOutput:\n{output}"
    );
}

#[test]
fn namespace_exported_destructuring_uses_temp_in_esnext_path() {
    let source = "namespace M {\n    export var [a, b] = [1, 2];\n}";
    let (parser, root) = parse_test_source(source);

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
fn namespace_single_exported_destructuring_reads_initializer_directly() {
    let source =
        "namespace M {\n    export let [bar5] = [1];\n    export const { a: bar7 } = { a: 1 };\n}";
    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("M.bar5 = [1][0];"),
        "Single array binding export should read by element index without a temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("M.bar7 = { a: 1 }.a;"),
        "Single object binding export should read by property name without a temp.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var _a;"),
        "Single binding exports should not reserve a namespace destructuring temp.\nOutput:\n{output}"
    );
}

#[test]
fn namespace_exported_destructuring_temp_hoists_before_class() {
    let source = "namespace m {\n    export class c {}\n    export var [x, y] = [10, new c()];\n}";
    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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
    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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
fn same_block_class_extends_class_uses_unqualified_name() {
    // Regression for `genericRecursiveImplicitConstructorErrors3` and
    // `collisionCodeGenModuleWithModuleReopening`: when a class declared in
    // the SAME namespace block extends another class declared in that same
    // block, the `extends` clause must reference the bare class name. tsc
    // emits `class B extends A` (not `class B extends X.A`) because A is
    // lexically in scope inside the IIFE.
    //
    // The `namespace_exported_names` set, populated for reopened blocks,
    // can include names that are also locally declared in the current
    // block (the merge logic in `collect_namespace_exported_names` adds
    // prior-block class exports). Without ordering the
    // `namespace_current_class_fn_enum_names` check first, the qualifier
    // branch wins and the output incorrectly becomes `extends X.A`.
    let source = "namespace X {\n  export class A {}\n}\nnamespace X {\n  export class A {}\n  export class B extends A {}\n}";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("class B extends A {"),
        "Same-block class reference in extends should be unqualified.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("extends X.A"),
        "Same-block class reference must not be qualified through the namespace name.\nOutput:\n{output}"
    );
}

#[test]
fn nested_namespace_does_not_qualify_own_leaf_name_from_parent_exports() {
    let source = "namespace X.Y {\n  export namespace Point {\n    export var Origin = new Point(0, 0);\n  }\n}\nnamespace X.Y {\n  export class Point {}\n}";

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

#[test]
fn nested_namespace_uses_parent_current_namespace_lexically() {
    let source = "namespace A {\n  export declare namespace BB {\n    export var Elephant: any;\n  }\n  export namespace B {\n    export class C {\n      x = BB.Elephant.X;\n    }\n  }\n}";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::new(
        &parser.arena,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("this.x = BB.Elephant.X;"),
        "Nested namespace should use parent current-block namespaces through lexical scope.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("this.x = A.BB.Elephant.X;"),
        "Current-block parent namespace should not be qualified through the parent object.\nOutput:\n{output}"
    );
}

#[test]
fn namespace_default_function_recovery_emits_default_assignment() {
    let source = "namespace ns_function {\n    export default function () {}\n}\n\nnamespace ns_async_function {\n    export default async function () {}\n}";
    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::new(
        &parser.arena,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("default function () { }\n    ns_function.default_1 = default_1;"),
        "Recovered namespace default function should keep tsc's default-function recovery and export assignment.\nOutput:\n{output}"
    );
    assert!(
        output.contains("default function () {\n        return __awaiter(this, void 0, void 0, function* () { });\n    }\n    ns_async_function.default_2 = default_2;"),
        "Recovered async namespace default function should lower async body and export assignment.\nOutput:\n{output}"
    );
}
