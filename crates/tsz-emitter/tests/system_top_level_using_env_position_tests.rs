//! Regression tests for the `env_1` placement in the System closure's
//! `var ...` list when a top-level `using` block coexists with other
//! top-level declarations.
//!
//! tsc places the using-block helper `env_1` either:
//!   1. *Immediately before* a `var` hoisted from inside an `if` / `for` /
//!      `try` block (the nested-hoisted case), or
//!   2. *At the trailing end* of the closure's `var` list when no
//!      nested-hoisted vars appear after the using.
//!
//! tsz was unconditionally inserting `env_1` directly after the using
//! statement's binding name, so for sources with subsequent top-level
//! `const` / `let` / `var` / `export default` declarations the helper
//! ended up wedged in the middle:
//!
//!   `var x, z, env_1, y, _default, w;`   (tsz, before fix)
//!   `var x, z, y, _default, w, env_1;`   (tsc, expected)
//!
//! Source: `crates/tsz-emitter/src/emitter/module_wrapper/wrapper_entry.rs`
//! (`collect_system_hoisted_names` — removed the eager push and added a
//! single trailing push when the nested-walk insertion never fired).

use tsz_common::common::{ModuleKind, ScriptTarget};
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

#[test]
fn system_top_level_using_followed_by_top_level_const_places_env_1_at_end() {
    // No nested-hoisted vars after the using → env_1 lands at the trailing
    // position of the closure's var list.
    let source = "export const x = 1;\nexport { y };\n\nawait using z = { async [Symbol.asyncDispose]() {} };\n\nconst y = 2;\n\nexport const w = 3;\n\nexport default 4;\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2022,
        module: ModuleKind::System,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        output.contains("var x, z, y, _default, w, env_1;"),
        "env_1 must trail when only top-level (non-nested-hoisted) vars follow the using.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var x, z, env_1,"),
        "env_1 must not be wedged immediately after the using's binding name.\nOutput:\n{output}"
    );
}

#[test]
fn system_top_level_using_followed_by_nested_hoisted_var_places_env_1_before_it() {
    // A `var y` inside an `if` block hoists to the System closure scope and
    // tsc inserts env_1 *between* the using's binding and the nested var.
    let source = "export {};\nusing z = null as any;\nif (false) { var y = 1; }\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2022,
        module: ModuleKind::System,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        output.contains("var z, env_1, y;"),
        "env_1 must be placed *before* a nested-hoisted var that follows the using.\nOutput:\n{output}"
    );
}
