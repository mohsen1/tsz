/// `import Foo, * as ns from "x"; Foo();` - an unused namespace binding beside
/// a used default must drop only the namespace, mirroring the unused-default
/// elision rule.
#[test]
fn esnext_unused_namespace_beside_used_default_is_elided() {
    let output = emit_esnext(
        "import Foo, * as ns from \"./dep\";\nFoo();\nexport {};\n",
        |_| {},
    );
    assert!(
        output.contains("import Foo from \"./dep\""),
        "Used default binding must be preserved without the unused namespace.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("* as ns"),
        "Unused namespace binding should be elided.\nOutput:\n{output}"
    );
}

/// Negative: a USED namespace binding beside a used default must be kept.
#[test]
fn esnext_used_namespace_beside_used_default_is_kept() {
    let output = emit_esnext(
        "import Foo, * as ns from \"./dep\";\nFoo();\nns.bar();\nexport {};\n",
        |_| {},
    );
    assert!(
        output.contains("import Foo, * as ns from \"./dep\""),
        "Both used bindings must be preserved.\nOutput:\n{output}"
    );
}

/// Bound-name choice must not matter - the namespace elision is name-agnostic.
#[test]
fn esnext_unused_namespace_elision_is_name_agnostic() {
    let output = emit_esnext(
        "import Defaulted, * as everything from \"./dep\";\nDefaulted();\nexport {};\n",
        |_| {},
    );
    assert!(
        output.contains("import Defaulted from \"./dep\""),
        "Used default must be preserved under a renamed namespace.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("everything"),
        "Unused namespace binding should be elided regardless of its name.\nOutput:\n{output}"
    );
}

/// Negative: verbatimModuleSyntax must keep the source clause exactly - the
/// unused namespace binding must NOT be elided.
#[test]
fn esnext_verbatim_module_syntax_keeps_unused_namespace() {
    let output = emit_esnext(
        "import Foo, * as ns from \"./dep\";\nFoo();\nexport {};\n",
        |o| o.verbatim_module_syntax = true,
    );
    assert!(
        output.contains("import Foo, * as ns from \"./dep\""),
        "verbatimModuleSyntax must preserve the original import clause.\nOutput:\n{output}"
    );
}

/// Negative: a namespace binding named like the classic JSX factory root is
/// referenced implicitly by JSX and must NOT be elided when the file has JSX.
#[test]
fn esnext_jsx_factory_namespace_binding_is_kept() {
    let source = "import * as React from \"react\";\nconst el = <div />;\nexport {};\n";
    // JSX requires a `.tsx` source for the parser to enable JSX parsing.
    let mut parser = ParserState::new("main.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        module: ModuleKind::ESNext,
        jsx: crate::emitter::JsxEmit::Preserve,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();
    assert!(
        output.contains("import * as React from \"react\""),
        "JSX-factory namespace binding must be preserved when the file uses JSX.\nOutput:\n{output}"
    );
}

#[test]
fn esnext_custom_jsx_factory_elides_unused_default_fragment_import_without_fragments() {
    let source = r#"import React from "react";

declare const jsx: typeof React.createElement;
declare const Comp: (p: { css?: string }) => null;
<Comp css="color:hotpink;" />;
"#;

    let mut parser = ParserState::new("main.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        module: ModuleKind::ESNext,
        jsx: crate::emitter::JsxEmit::React,
        jsx_factory: Some("jsx".to_string()),
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        !output.contains("from \"react\""),
        "Default fragment factory root must not keep an otherwise type-only import alive when the file has no fragments.\nOutput:\n{output}"
    );
    assert!(
        output.contains("jsx(Comp, { css: \"color:hotpink;\" });"),
        "Custom element factory should still emit the JSX call.\nOutput:\n{output}"
    );
    assert!(
        output.contains("export {};"),
        "Eliding the only import should keep the file as an external module.\nOutput:\n{output}"
    );
}

#[test]
fn esnext_custom_jsx_factory_keeps_default_fragment_import_for_fragments() {
    let source = r#"import React from "react";

declare const jsx: typeof React.createElement;
<></>;
"#;

    let mut parser = ParserState::new("main.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        module: ModuleKind::ESNext,
        jsx: crate::emitter::JsxEmit::React,
        jsx_factory: Some("jsx".to_string()),
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("import React from \"react\""),
        "Default fragment factory root should keep the import when fragments emit React.Fragment.\nOutput:\n{output}"
    );
    assert!(
        output.contains("jsx(React.Fragment"),
        "Fragment JSX should still reference the configured element factory and default fragment factory.\nOutput:\n{output}"
    );
}
