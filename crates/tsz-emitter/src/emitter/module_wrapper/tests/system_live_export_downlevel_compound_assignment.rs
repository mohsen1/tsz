//! Down-leveled compound assignments (`**=` / `&&=` / `||=` / `??=`) to a
//! System-exported binding must keep the live named export in sync by wrapping
//! their innermost `x = <value>` write in the `exports_1("x", ...)` call chain,
//! exactly as the non-lowered assignment path does. See issue #15291.
//!
//! Expectations are byte-checked against `tsc` 6.0.2 output.

use crate::context::emit::EmitContext;
use crate::emitter::{ModuleKind, Printer, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

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

const COMPOUND_SOURCE: &str = r#"let b = 1;
b ??= 5;
b ||= 2;
b &&= 3;
b **= 2;
export { b };
"#;

#[test]
fn es2015_lowered_compound_assignments_wrap_the_write_in_exports_call() {
    let output = emit_system_target(COMPOUND_SOURCE, ScriptTarget::ES2015);
    assert_ordered(
        &output,
        &[
            "b !== null && b !== void 0 ? b : (exports_1(\"b\", b = 5));",
            "b || (exports_1(\"b\", b = 2));",
            "b && (exports_1(\"b\", b = 3));",
            "exports_1(\"b\", b = Math.pow(b, 2));",
        ],
    );
    // The short-circuit read positions stay unwrapped, and no bare `(b = N)`
    // write escapes the live-export mirror.
    assert!(
        !output.contains("(b = 5)") && !output.contains("(b = 2)") && !output.contains("(b = 3)"),
        "Lowered writes must be wrapped in exports_1.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.b"),
        "System live exports must not use CommonJS export property writes.\nOutput:\n{output}"
    );
}

#[test]
fn es2020_still_lowers_logical_assignments_and_wraps_them() {
    // At ES2020 the logical-assignment operators still lower (native `&&=`/`||=`/`??=`
    // are ES2021), so the wrap must be preserved; native `**=` is kept and handled
    // by the non-lowered System gateway.
    let output = emit_system_target(COMPOUND_SOURCE, ScriptTarget::ES2020);
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
fn renamed_reexport_wraps_the_write_under_the_export_alias() {
    let source = r#"let b = 1;
b ??= 5;
b **= 2;
export { b as foo };
"#;
    let output = emit_system_target(source, ScriptTarget::ES2015);
    assert_ordered(
        &output,
        &[
            "b !== null && b !== void 0 ? b : (exports_1(\"foo\", b = 5));",
            "exports_1(\"foo\", b = Math.pow(b, 2));",
        ],
    );
}

#[test]
fn multi_alias_clause_nests_the_export_calls_around_the_write() {
    let source = r#"let b = 1;
b ??= 5;
b **= 2;
export { b, b as foo };
"#;
    let output = emit_system_target(source, ScriptTarget::ES2015);
    assert_ordered(
        &output,
        &[
            "b !== null && b !== void 0 ? b : (exports_1(\"foo\", exports_1(\"b\", b = 5)));",
            "exports_1(\"foo\", exports_1(\"b\", b = Math.pow(b, 2)));",
        ],
    );
}

#[test]
fn non_exported_local_gets_no_export_wrap() {
    // A non-exported binding must lower with no `exports_1` wrap, proving the
    // decision keys on the exported-binding origin, not on the operator.
    let source = r#"export let e = 0;
let b = 1;
b ??= 5;
b ||= 2;
b &&= 3;
b **= 2;
"#;
    let output = emit_system_target(source, ScriptTarget::ES2015);
    assert_ordered(
        &output,
        &[
            "b !== null && b !== void 0 ? b : (b = 5);",
            "b || (b = 2);",
            "b && (b = 3);",
            "b = Math.pow(b, 2);",
        ],
    );
    assert!(
        !output.contains("exports_1(\"b\""),
        "Non-exported local must not be threaded through exports_1.\nOutput:\n{output}"
    );
}
