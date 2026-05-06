use crate::output::printer::{PrintOptions, Printer};
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
