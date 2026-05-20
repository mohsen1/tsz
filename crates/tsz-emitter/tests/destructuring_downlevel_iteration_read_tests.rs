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
fn empty_assignment_patterns_evaluate_rhs_without_read_helper() {
    let output = emit_es5_downlevel_iteration(
        "var a: any;\n\
         ({} = a);\n\
         ([] = a);\n",
    );

    assert!(
        output.contains("var a;\n(a);\n(a);"),
        "Empty object and array assignment patterns should evaluate only their RHS.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("__read"),
        "Empty array assignment patterns should not schedule the read helper.\nOutput:\n{output}"
    );
}

#[test]
fn hole_only_assignment_pattern_still_advances_iterable() {
    let output = emit_es5_downlevel_iteration(
        "var a: any;\n\
         ([,] = a);\n",
    );

    assert!(
        output.contains("__read(a, 1)"),
        "Array assignment holes are not empty patterns; they must still advance the iterable.\nOutput:\n{output}"
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
