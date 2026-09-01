use crate::context::emit::EmitContext;
use crate::emitter::{ModuleKind, Printer, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit_system(source: &str, target: ScriptTarget) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        module: ModuleKind::System,
        target,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let emit_plan = LoweringPass::new(&parser.arena, &ctx).run_plan(root);
    let mut printer = Printer::with_emit_plan_and_options(&parser.arena, emit_plan, options);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

fn assert_ordered(output: &str, snippets: &[&str]) {
    let mut start = 0;
    for snippet in snippets {
        let Some(pos) = output[start..].find(snippet) else {
            panic!("Missing snippet `{snippet}` after byte {start}.\nOutput:\n{output}");
        };
        start += pos + snippet.len();
    }
}

#[test]
fn system_reexports_imported_bindings_in_import_binding_order() {
    let output = emit_system(
        r#"import n, { x } from "file1";
import n2 = require("file2");
export { x };
export { x as y };
export { n };
export { n as n1 };
export { n2 };
export { n2 as n3 };
"#,
        ScriptTarget::ES2015,
    );

    assert_ordered(
        &output,
        &[
            "execute: function () {",
            "exports_1(\"n\", file1_1.default);",
            "exports_1(\"n1\", file1_1.default);",
            "exports_1(\"x\", file1_1.x);",
            "exports_1(\"y\", file1_1.x);",
            "exports_1(\"n2\", n2);",
            "exports_1(\"n3\", n2);",
        ],
    );
    assert_eq!(output.matches("exports_1(\"x\",").count(), 1, "{output}");
    assert_eq!(output.matches("exports_1(\"n\",").count(), 1, "{output}");
}

#[test]
fn system_reexports_renamed_and_namespace_imports_by_import_binding_order() {
    let output = emit_system(
        r#"import d, { foo as bar, baz } from "dep";
import * as ns from "pkg";
export { baz as z };
export { ns as packageNamespace };
export { bar as b };
export { d as defaultAlias };
"#,
        ScriptTarget::ES2015,
    );

    assert_ordered(
        &output,
        &[
            "execute: function () {",
            "exports_1(\"defaultAlias\", dep_1.default);",
            "exports_1(\"b\", dep_1.foo);",
            "exports_1(\"z\", dep_1.baz);",
            "exports_1(\"packageNamespace\", ns);",
        ],
    );
    assert_eq!(output.matches("exports_1(\"b\",").count(), 1, "{output}");
    assert_eq!(
        output.matches("exports_1(\"defaultAlias\",").count(),
        1,
        "{output}"
    );
}
