//! Regression test for the `_default` tracker on the *TC39 (non-legacy)
//! ES decorators* path inside a System top-level `using` block.
//!
//! When a using statement precedes a TC39-decorated anonymous default class
//! export under `module=system, target=es5`, the inline export must hop
//! through the closure's `_default` tracker — i.e.
//! `exports_1("default", _default = default_1)` — so re-exports observe
//! the post-decorator value. This mirrors the legacy-decorators path; the
//! TC39 ES5 branch in `emit_top_level_using_class_assignment` was missing
//! the prefix.
//!
//! Source:
//! `crates/tsz-emitter/src/emitter/source_file/top_level_using.rs`
//! (the `in_system_execute_body && export_name == "default" &&
//! class.name.is_none()` branch reached via
//! `render_simple_tc39_decorated_class_es5`).

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
fn system_using_es5_threads_default_tracker_for_tc39_decorated_anonymous_default_class() {
    let source = "declare var dec: any;\nusing before = null;\n@dec\nexport default class {}\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::System,
        // legacy_decorators = false -> TC39 (non-legacy) decorator path
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        output.contains("exports_1(\"default\", _default = default_1)"),
        "TC39-decorated anonymous default class inside a system using-block must thread \
         `_default` so live-binding re-exports observe the post-decorator value.\n\
         Output:\n{output}"
    );
}
