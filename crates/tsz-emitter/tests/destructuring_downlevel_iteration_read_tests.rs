//! ES5 `downlevelIteration` array destructuring read-helper coverage.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::context::emit::EmitContext;
use tsz_emitter::emitter::{Printer as EmitterPrinter, PrinterOptions};
use tsz_emitter::lowering::LoweringPass;
use tsz_parser::parser::ParserState;

fn emit_es5_downlevel_iteration(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let opts = PrinterOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::None,
        downlevel_iteration: true,
        remove_comments: true,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

#[test]
fn array_assignment_pattern_reads_simple_identifier_sources_when_downlevel_iteration_is_enabled() {
    let output = emit_es5_downlevel_iteration(
        "var a: any;\n\
         let a1, a2, a3;\n\
         ([] = [a1, a2, a3] = a);\n",
    );

    assert!(
        output.contains("__read(a, 3)"),
        "Array assignment destructuring should read iterable sources through `__read`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("a1 = _a[0]")
            && output.contains("a2 = _a[1]")
            && output.contains("a3 = _a[2]"),
        "Assignments should read from the `__read` temp.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("a1 = a[0]"),
        "Downlevel iterable assignment must not index the source directly.\nOutput:\n{output}"
    );
}

#[test]
fn empty_array_assignment_patterns_do_not_schedule_read_helpers() {
    let output = emit_es5_downlevel_iteration(
        "var a: any;\n\
         ({} = a);\n\
         ([] = a);\n",
    );

    assert!(
        !output.contains("var __read") && !output.contains("__read("),
        "Empty assignment patterns should evaluate the source without scheduling `__read`.\nOutput:\n{output}"
    );
    assert_eq!(
        output.matches("(a);").count(),
        2,
        "Both empty assignment patterns should emit as source evaluations.\nOutput:\n{output}"
    );
}

#[test]
fn empty_array_binding_patterns_without_initializers_still_schedule_read_helpers() {
    let output = emit_es5_downlevel_iteration(
        "(function () {\n\
             var [];\n\
             let [];\n\
             const [];\n\
         })();\n",
    );

    assert_eq!(
        output.matches("__read(void 0, 0)").count(),
        3,
        "Each empty array binding pattern should preserve the downlevel iterable read.\nOutput:\n{output}"
    );
}

#[test]
fn for_of_empty_array_assignment_target_advances_iterator_without_binding() {
    let output = emit_es5_downlevel_iteration(
        "var a: any;\n\
         for ([] of a) {}\n\
         for ({} of a) {}\n",
    );

    assert!(
        !output.contains("[] =") && !output.contains("{} ="),
        "Empty assignment patterns in for-of must not emit an assignment.\nOutput:\n{output}"
    );
    // The iterator value must still be accessed to advance the iterator.
    assert!(
        output.contains(".value;"),
        "Empty for-of assignment patterns must still access .value to advance the iterator.\nOutput:\n{output}"
    );
}

#[test]
fn for_of_sequential_empty_bindings_allocate_return_temps_before_body_temps() {
    let output = emit_es5_downlevel_iteration(
        "(function () {\n\
             var ns: number[][] = [];\n\
             for (var {} of ns) {}\n\
             for (var {} of ns) {}\n\
             for (var {} of ns) {}\n\
             for (var [] of ns) {}\n\
             for (var [] of ns) {}\n\
             for (var [] of ns) {}\n\
         })();\n",
    );

    // All 6 return temps must be pre-allocated before any body temps.
    // The hoisted var declaration must contain 12 names: e_1, _a, e_2, _b,
    // e_3, _c, e_4, _d, e_5, _e, e_6, _f in that order.
    assert!(
        output.contains("e_1, _a, e_2, _b, e_3, _c, e_4, _d, e_5, _e, e_6, _f"),
        "Return temps for sequential for-of loops must be allocated before body temps.\nOutput:\n{output}"
    );
}
