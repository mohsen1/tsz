use crate::emitter::{JsxEmit, ModuleKind, Printer, PrinterOptions};
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

#[test]
fn amd_preserve_jsx_keeps_import_equals_namespace_alias_dep() {
    let source = r#"import React = require("react");
import ReactRouter = require("react-router");
import Route = ReactRouter.Route;
var routes1 = <Route />;
"#;
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::AMD,
        jsx: JsxEmit::Preserve,
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains(
            "define([\"require\", \"exports\", \"react\", \"react-router\"], function (require, exports, React, ReactRouter) {"
        ),
        "AMD wrapper should keep the dependency that backs the JSX tag alias.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var Route = ReactRouter.Route;"),
        "The namespace import alias should still emit against the retained dependency variable.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var routes1 = <Route />;"),
        "JSX preserve output should retain the alias tag.\nOutput:\n{output}"
    );
}
