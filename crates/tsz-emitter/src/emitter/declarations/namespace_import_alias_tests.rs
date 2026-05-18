use crate::output::printer::{PrintOptions, Printer};
use tsz_common::common::ScriptTarget;
use tsz_parser::ParserState;

/// When the same `import X = ...` alias is re-declared (e.g., a duplicate
/// in error-recovery code), the SECOND `import X = ...` was being treated
/// as a binding that "shadows" the first, causing both to be elided -
/// even when the first one is value-bearing and `X` is referenced later.
/// tsc treats the duplicate as a TS2300 diagnostic but still emits the
/// first value-bearing import. Lock that behavior so both imports survive
/// when there's a downstream use.
#[test]
fn namespace_import_alias_redeclared_keeps_value_emit() {
    let source = "namespace Z {\n    export namespace M {\n        export function bar() {}\n    }\n    export interface I {}\n}\nnamespace A.M {\n    import M = Z.M;\n    import M = Z.I;\n\n    export function bar() {}\n    M.bar();\n}";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("var M = Z.M;"),
        "First value-bearing `import M = Z.M;` must survive when followed by a duplicate redeclaration.\nOutput:\n{output}"
    );
    // The second (type-only) import should still be elided: Z.I has no
    // runtime value, and even with the redeclaration logic the
    // `import_decl_has_runtime_value` gate stops it.
    assert!(
        !output.contains("var M = Z.I;"),
        "Type-only `import M = Z.I;` must remain elided.\nOutput:\n{output}"
    );
}

#[test]
fn namespace_import_alias_elided_when_shadowed_before_use() {
    let source = "namespace X {\n  export class Y {}\n}\nnamespace Z {\n  import Y = X.Y;\n  var Y = 12;\n}\nnamespace r {\n  export const Q = {};\n}\nnamespace s {\n  import Q = r.Q;\n  const Q = 0;\n}";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        !output.contains("var Y = X.Y;"),
        "Namespace import alias should be elided when a local var shadows it before use.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var Q = r.Q;"),
        "Namespace import alias should be elided when a local const shadows it before use.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var Y = 12;") && output.contains("const Q = 0;"),
        "Shadowing declarations should still emit.\nOutput:\n{output}"
    );
}

#[test]
fn namespace_relative_const_enum_aliases_inline_and_elide() {
    let source = "namespace Root.Child {\n    export const enum E { A, B }\n    export interface I {}\n}\nnamespace Root {\n    import Local = Child.E;\n    export import ReExported = Child.E;\n    export import TypeAlias = Child.I;\n    export class Use {\n        m() {\n            const a = Local.B;\n            const b = ReExported.A;\n        }\n    }\n}";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        !output.contains("var Local =") && !output.contains("Root.ReExported ="),
        "Const-enum import aliases inside merged namespaces should be erased.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("Root.TypeAlias ="),
        "Interface import aliases inside merged namespaces should be erased.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const a = 1 /* Local.B */;")
            && output.contains("const b = 0 /* ReExported.A */;"),
        "Const enum aliases should still inline at usage sites.\nOutput:\n{output}"
    );
}

#[test]
fn namespace_relative_const_enum_aliases_inline_and_elide_es5() {
    let source = "namespace Root.Child {\n    export const enum E { A, B }\n    export interface I {}\n}\nnamespace Root {\n    import Local = Child.E;\n    export import ReExported = Child.E;\n    export import TypeAlias = Child.I;\n    export class Use {\n        m() {\n            const a = Local.B;\n            const b = ReExported.A;\n        }\n    }\n}";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrintOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, options);
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        !output.contains("var Local =") && !output.contains("Root.ReExported ="),
        "ES5 const-enum import aliases inside merged namespaces should be erased.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("Root.TypeAlias ="),
        "ES5 interface import aliases inside merged namespaces should be erased.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var a = 1 /* Local.B */;")
            && output.contains("var b = 0 /* ReExported.A */;"),
        "ES5 const enum aliases should still inline at usage sites.\nOutput:\n{output}"
    );
}

#[test]
fn sibling_namespaces_keep_local_const_enum_alias_scope() {
    let source = "namespace A {\n  export const enum E { X = 1 }\n  import Local = E;\n  export const a = Local.X;\n}\nnamespace B {\n  export const enum E { X = 2 }\n  import Local = E;\n  export const b = Local.X;\n}";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("A.a = 1 /* Local.X */;"),
        "Alias `Local` in namespace A should resolve to A.E.\nOutput:\n{output}"
    );
    assert!(
        output.contains("B.b = 2 /* Local.X */;"),
        "Alias `Local` in namespace B should resolve to B.E.\nOutput:\n{output}"
    );
}

#[test]
fn sibling_namespaces_keep_local_const_enum_alias_scope_es5() {
    let source = "namespace A {\n  export const enum E { X = 1 }\n  import Local = E;\n  export const a = Local.X;\n}\nnamespace B {\n  export const enum E { X = 2 }\n  import Local = E;\n  export const b = Local.X;\n}";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrintOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, options);
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("A.a = 1 /* Local.X */;"),
        "ES5 alias `Local` in namespace A should resolve to A.E.\nOutput:\n{output}"
    );
    assert!(
        output.contains("B.b = 2 /* Local.X */;"),
        "ES5 alias `Local` in namespace B should resolve to B.E.\nOutput:\n{output}"
    );
}

#[test]
fn chained_import_alias_to_const_enum_is_elided_es5() {
    let source = "namespace Root.Child {\n  export const enum E { A = 1 }\n}\nnamespace Root {\n  import A = Child.E;\n  import B = A;\n  export const value = B.A;\n}";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrintOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, options);
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        !output.contains("var A =") && !output.contains("var B ="),
        "ES5 import aliases chained to a const enum should be erased.\nOutput:\n{output}"
    );
    assert!(
        output.contains("Root.value = 1 /* B.A */;"),
        "ES5 chained const enum alias should still inline at usage sites.\nOutput:\n{output}"
    );
}

#[test]
fn erased_import_alias_preserves_trailing_standalone_comment_es5() {
    let source = "namespace Root {\n  export interface I {}\n  import Local = I;\n  // keep me\n  export const value = 1;\n}";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrintOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, options);
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("// keep me"),
        "Standalone trailing comments after erased import aliases should be preserved.\nOutput:\n{output}"
    );
}
