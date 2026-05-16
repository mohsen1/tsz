//! Regression coverage for malformed `try`/`catch`/`finally` recovery.
//!
//! TSC keeps several invalid surfaces in emitted JavaScript: orphan `catch`
//! and `finally` clauses synthesize a missing `try`, `try { };` synthesizes a
//! missing `finally` that replays the semicolon's trailing comment, and
//! `catch ()` preserves its invalid empty binding rather than downleveling as
//! optional catch binding.

use tsz_common::common::ScriptTarget;
use tsz_emitter::context::emit::EmitContext;
use tsz_emitter::emitter::{Printer as EmitterPrinter, PrinterOptions};
use tsz_emitter::lowering::LoweringPass;

#[path = "test_support.rs"]
mod test_support;

fn parse_lower_emit(source: &str, opts: PrinterOptions) -> String {
    let (parser, root) = test_support::parse_source(source);
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

fn emit_es2015(source: &str) -> String {
    parse_lower_emit(
        source,
        PrinterOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    )
}

#[test]
fn orphan_catch_and_finally_synthesize_multiline_try_and_keep_trailing_comments() {
    let output = emit_es2015(
        r#"
function fn() {
    catch (x) { } // missing try

    finally { } // absorbed finally
}
"#,
    );

    assert!(
        output.contains(
            "    try {\n    }\n    catch (x) { } // missing try\n    finally { } // absorbed finally"
        ),
        "Orphan catch/finally should synthesize a multiline empty try and keep clause trailing comments.\nOutput:\n{output}"
    );
}

#[test]
fn try_without_catch_or_finally_replays_following_semicolon_comment() {
    let output = emit_es2015(
        r#"
function fn() {
    try { }; // missing finally
}
"#,
    );

    assert!(
        output.contains(
            "    try { }\n    finally { // missing finally\n     } // missing finally\n    ; // missing finally"
        ),
        "Recovered missing finally should replay the following semicolon comment like tsc.\nOutput:\n{output}"
    );
}

#[test]
fn invalid_empty_catch_binding_parens_are_preserved() {
    let output = emit_es2015(
        r#"
function fn() {
    try { } catch () { } // invalid binding
}
"#,
    );

    assert!(
        output.contains("    catch () { } // invalid binding"),
        "Invalid `catch ()` should preserve empty parens instead of generating a temp binding.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("catch (_"),
        "Invalid `catch ()` must not be treated as optional catch binding.\nOutput:\n{output}"
    );
}

#[test]
fn valid_optional_catch_binding_still_downlevels_for_es2015() {
    let output = emit_es2015(
        r#"
function fn() {
    try { } catch { } // optional binding
}
"#,
    );

    assert!(
        output.contains("    catch (_"),
        "Valid optional catch binding should still get an ES2015 temp binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains(") { } // optional binding"),
        "Optional catch binding trailing comment should stay on the catch block line.\nOutput:\n{output}"
    );
}
