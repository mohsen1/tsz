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

fn emit_system_target(source: &str, target: ScriptTarget) -> String {
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

// --- Down-leveled compound assignment to a System-exported binding ---------
//
// `**=` (target < ES2016) and `&&=`/`||=`/`??=` (target < ES2021) lower to an
// inner value-producing assignment `x = <value>` that returns before the normal
// assignment dispatch. That inner write must still thread the live named export
// `exports_1("x", x = <value>)`, exactly as the non-lowered path does. Ground
// truth captured from `tsc` 6.0.2 `--module system`.

#[test]
fn system_downlevel_compound_assign_clause_export_wraps_es2015() {
    let output = emit_system_target(
        "let b = 1;\nb ??= 5;\nb ||= 2;\nb &&= 3;\nb **= 2;\nexport { b };\n",
        ScriptTarget::ES2015,
    );
    assert_ordered(
        &output,
        &[
            "b !== null && b !== void 0 ? b : (exports_1(\"b\", b = 5));",
            "b || (exports_1(\"b\", b = 2));",
            "b && (exports_1(\"b\", b = 3));",
            "exports_1(\"b\", b = Math.pow(b, 2));",
        ],
    );
    // The reads of the target stay bare — only the write threads the export.
    assert!(
        !output.contains("exports_1(\"b\", b) !== null"),
        "logical-assignment read must not be wrapped.\nOutput:\n{output}"
    );
}

#[test]
fn system_downlevel_compound_assign_clause_export_wraps_es5() {
    let output = emit_system_target(
        "let b = 1;\nb ??= 5;\nb ||= 2;\nb &&= 3;\nb **= 2;\nexport { b };\n",
        ScriptTarget::ES5,
    );
    assert_ordered(
        &output,
        &[
            "b !== null && b !== void 0 ? b : (exports_1(\"b\", b = 5));",
            "b || (exports_1(\"b\", b = 2));",
            "b && (exports_1(\"b\", b = 3));",
            "exports_1(\"b\", b = Math.pow(b, 2));",
        ],
    );
}

#[test]
fn system_downlevel_nullish_and_exponent_wrap_at_es2020() {
    // At ES2020 `??=` still lowers (to native `??` short-circuit) and must wrap;
    // native `**=` is kept and threads the export through the normal dispatch.
    let output = emit_system_target(
        "let b = 1;\nb ??= 5;\nb ||= 2;\nb &&= 3;\nb **= 2;\nexport { b };\n",
        ScriptTarget::ES2020,
    );
    assert_ordered(
        &output,
        &[
            "b ?? (exports_1(\"b\", b = 5));",
            "b || (exports_1(\"b\", b = 2));",
            "b && (exports_1(\"b\", b = 3));",
            "exports_1(\"b\", b **= 2);",
        ],
    );
}

#[test]
fn system_downlevel_native_logical_assignment_wraps_at_es2021() {
    // At ES2021 the operators are native; the normal assignment dispatch already
    // threads the export. This guards that the lowered/native paths agree.
    let output = emit_system_target(
        "let b = 1;\nb ??= 5;\nb ||= 2;\nb **= 2;\nexport { b };\n",
        ScriptTarget::ES2022,
    );
    assert_ordered(
        &output,
        &[
            "exports_1(\"b\", b ??= 5);",
            "exports_1(\"b\", b ||= 2);",
            "exports_1(\"b\", b **= 2);",
        ],
    );
}

#[test]
fn system_downlevel_compound_assign_renamed_reexport_wraps() {
    let output = emit_system_target(
        "let b = 1;\nb ??= 5;\nb ||= 2;\nb **= 2;\nexport { b as foo };\n",
        ScriptTarget::ES2015,
    );
    assert_ordered(
        &output,
        &[
            "b !== null && b !== void 0 ? b : (exports_1(\"foo\", b = 5));",
            "b || (exports_1(\"foo\", b = 2));",
            "exports_1(\"foo\", b = Math.pow(b, 2));",
        ],
    );
}

#[test]
fn system_downlevel_compound_assign_multi_alias_nests_call_chain() {
    let output = emit_system_target(
        "let c = 1;\nc &&= 3;\nc **= 2;\nexport { c, c as bar };\n",
        ScriptTarget::ES2015,
    );
    assert_ordered(
        &output,
        &[
            "c && (exports_1(\"bar\", exports_1(\"c\", c = 3)));",
            "exports_1(\"bar\", exports_1(\"c\", c = Math.pow(c, 2)));",
        ],
    );
}

#[test]
fn system_downlevel_chained_exponent_assign_nests_each_export() {
    let output = emit_system_target(
        "let p = 2, q = 3, r = 4;\np **= q **= r;\nexport { p, q, r };\n",
        ScriptTarget::ES2015,
    );
    assert!(
        output.contains("exports_1(\"p\", p = Math.pow(p, exports_1(\"q\", q = Math.pow(q, r))));"),
        "chained `**=` must wrap each exported write.\nOutput:\n{output}"
    );
}

#[test]
fn system_downlevel_non_exported_local_is_not_wrapped() {
    let output = emit_system_target(
        "let n = 0;\nlet b = 1;\nn ??= 5;\nn ||= 2;\nn &&= 3;\nn **= 2;\nb ||= 7;\nexport { b };\n",
        ScriptTarget::ES2015,
    );
    // The non-exported `n` writes stay bare in every lowered form...
    assert_ordered(
        &output,
        &[
            "n !== null && n !== void 0 ? n : (n = 5);",
            "n || (n = 2);",
            "n && (n = 3);",
            "n = Math.pow(n, 2);",
        ],
    );
    // ...while the exported `b` still threads its export.
    assert!(
        output.contains("b || (exports_1(\"b\", b = 7));"),
        "exported `b` must still wrap.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports_1(\"n\""),
        "non-exported `n` must never be wrapped.\nOutput:\n{output}"
    );
}
