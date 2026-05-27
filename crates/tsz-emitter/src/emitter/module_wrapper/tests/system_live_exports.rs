use crate::context::emit::EmitContext;
use crate::emitter::{ModuleKind, Printer, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit_system(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        module: ModuleKind::System,
        target: ScriptTarget::ES2015,
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
fn system_exported_var_mutations_use_system_export_calls() {
    let output = emit_system(
        r#"export var value;
value = 1;
value++;
value += 2;
function bump() {
    value = 3;
    value++;
}
"#,
    );

    assert_ordered(
        &output,
        &[
            "function bump() {",
            "exports_1(\"value\", value = 3);",
            "exports_1(\"value\", (value++, value));",
            "execute: function () {",
            "exports_1(\"value\", value = 1);",
            "exports_1(\"value\", (value++, value));",
            "exports_1(\"value\", value += 2);",
        ],
    );
    assert!(
        !output.contains("exports.value"),
        "System live exports must not use CommonJS export property writes.\nOutput:\n{output}"
    );
}

#[test]
fn system_exported_var_mutations_update_aliases_inside_same_call_chain() {
    let output = emit_system(
        r#"export var value;
export { value as alias };
value = 1;
value++;
++value;
function bump() {
    value = 3;
    value++;
}
"#,
    );

    assert_ordered(
        &output,
        &[
            "function bump() {",
            "exports_1(\"alias\", exports_1(\"value\", value = 3));",
            "exports_1(\"alias\", exports_1(\"value\", (value++, value)));",
            "execute: function () {",
            "exports_1(\"alias\", exports_1(\"value\", value = 1));",
            "exports_1(\"alias\", exports_1(\"value\", (value++, value)));",
            "exports_1(\"alias\", exports_1(\"value\", ++value));",
        ],
    );
}

#[test]
fn system_single_array_destructuring_export_indexes_literal_without_temp() {
    let output = emit_system(
        r#"export let [a] = [1];
export let [b, c] = [1, 2];
"#,
    );

    assert_ordered(
        &output,
        &[
            "var a, _a, b, c;",
            "exports_1(\"a\", a = [1][0]);",
            "_a = [1, 2], exports_1(\"b\", b = _a[0]), exports_1(\"c\", c = _a[1]);",
        ],
    );
}
