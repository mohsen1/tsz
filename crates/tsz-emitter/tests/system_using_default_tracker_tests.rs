//! Regression tests for the `_default` tracker that System modules thread
//! through legacy-decorated default class exports emitted inside a top-level
//! `using` block.
//!
//! Two related contracts:
//!
//!   1. When a `using` statement precedes a legacy-decorated default class
//!      export, the class is emitted *inside* the System try/catch. The
//!      live-binding `exports_1("default", ...)` call must thread a
//!      `_default` tracker so re-exports can observe the latest value
//!      (`exports_1("default", _default = C)` and the anonymous form
//!      `exports_1("default", _default = default_1)`).
//!
//!   2. When the default-export class is emitted *before* any `using`
//!      statement, it lives at System closure scope rather than inside the
//!      using block. There is no live-binding tracker to thread, and the
//!      closure's `var ...` hoist list must not include a phantom `_default`.
//!
//! Sources:
//!   - `crates/tsz-emitter/src/emitter/source_file/top_level_using.rs`
//!     (the inline export-binding rewrite for ES5 + legacy decorators +
//!     anonymous default class)
//!   - `crates/tsz-emitter/src/emitter/module_wrapper/wrapper_entry.rs`
//!     (the `_default` hoist gating in `collect_system_hoisted_names`)

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::context::emit::EmitContext;
use tsz_emitter::emitter::{Printer as EmitterPrinter, PrinterOptions};
use tsz_emitter::lowering::LoweringPass;
use tsz_parser::parser::ParserState;

fn parse_lower_emit(source: &str, opts: PrinterOptions) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

#[test]
fn system_using_block_threads_default_tracker_for_anonymous_decorated_default_class() {
    let source = "declare var dec: any;\nusing before = null;\n@dec\nexport default class {}\n";
    for target in [ScriptTarget::ES5, ScriptTarget::ES2015] {
        let opts = PrinterOptions {
            target,
            module: ModuleKind::System,
            legacy_decorators: true,
            ..Default::default()
        };
        let output = parse_lower_emit(source, opts);

        assert!(
            output.contains("exports_1(\"default\", _default = default_1)"),
            "target={target:?}: anonymous decorated default class inside system using-block \
             must thread `_default`.\nOutput:\n{output}"
        );
        assert!(
            output.contains("_default"),
            "target={target:?}: `_default` must be hoisted in the System closure var list.\n\
             Output:\n{output}"
        );
    }
}

#[test]
fn system_default_class_before_using_does_not_hoist_default_tracker() {
    let source = "declare var dec: any;\n@dec\nexport default class C {}\nusing after = null;\n";
    for target in [ScriptTarget::ES5, ScriptTarget::ES2015] {
        let opts = PrinterOptions {
            target,
            module: ModuleKind::System,
            legacy_decorators: true,
            ..Default::default()
        };
        let output = parse_lower_emit(source, opts);

        assert!(
            !output.contains("_default"),
            "target={target:?}: pre-using default class must not synthesize a `_default` \
             tracker.\nOutput:\n{output}"
        );
        assert!(
            output.contains("exports_1(\"default\", C)"),
            "target={target:?}: pre-using default class must export the class binding \
             directly.\nOutput:\n{output}"
        );
    }
}
