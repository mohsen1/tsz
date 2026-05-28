//! Regression test for continuation-line indentation of a TC39-decorated
//! class whose `extends` base is itself a multi-line-emitting expression.
//!
//! When emit lowering captures the heritage (super) expression's rendered text
//! and reinserts it after `let _classSuper = `, every *continuation* line of
//! that captured text must be re-based to the indentation of the generated
//! `_classSuper` statement. tsz previously trimmed only the first captured line
//! and left interior lines at the column they happened to be emitted at,
//! producing flush-left `});` for an empty class-expression base instead of
//! tsc's indented `    });`.
//!
//! The rule is structural — it keys on the captured text spanning multiple
//! lines, not on the kind of base expression or the chosen decorator name — so
//! these tests vary the base members, vary nesting depth, and use a renamed
//! decorator.
//!
//! Source: `crates/tsz-emitter/src/emitter/transform_dispatch.rs`
//! (`seed_tc39_decorator_extends_text`, routed through
//! `Printer::reindent_captured_block`).

use tsz_common::common::ScriptTarget;
use tsz_emitter::context::emit::EmitContext;
use tsz_emitter::emitter::{Printer as EmitterPrinter, PrinterOptions};
use tsz_emitter::lowering::LoweringPass;

#[path = "test_support.rs"]
mod test_support;

fn parse_lower_emit(source: &str) -> String {
    let opts = PrinterOptions {
        target: ScriptTarget::ES2022,
        no_emit_helpers: true,
        ..Default::default()
    };
    let (parser, root) = test_support::parse_source(source);
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

#[test]
fn empty_class_expression_base_close_brace_is_indented_to_class_super() {
    // The captured base is `class {\n}`. Its continuation `}` line must land at
    // the `let _classSuper = ` indent (one level), matching tsc:
    //   let _classSuper = (0, class {
    //   });
    let source = concat!(
        "declare var deco: any;\n",
        "(@deco\n",
        "class C1 extends class { } {\n",
        "    static { super.name; }\n",
        "});\n",
    );
    let output = parse_lower_emit(source);

    assert!(
        output.contains("    let _classSuper = (0, class {\n    });"),
        "Empty class-expression base close `}}` must be indented to the `_classSuper` \
         statement indent (4 spaces).\nOutput:\n{output}"
    );
    assert!(
        !output.contains("\n});\n    var C1"),
        "The class-body close must not be left flush-left.\nOutput:\n{output}"
    );
}

#[test]
fn class_expression_base_with_member_preserves_relative_indentation() {
    // Member body sits one level deeper than `_classSuper`; the close brace
    // returns to the `_classSuper` level.
    let source = concat!(
        "declare var deco: any;\n",
        "(@deco\n",
        "class C1 extends class { m() { return 1; } } {\n",
        "    static { super.name; }\n",
        "});\n",
    );
    let output = parse_lower_emit(source);

    assert!(
        output.contains("    let _classSuper = (0, class {\n        m() { return 1; }\n    });"),
        "A class-expression base with members must keep its body one level deeper \
         than `_classSuper` and close at the `_classSuper` indent.\nOutput:\n{output}"
    );
}

#[test]
fn nested_class_expression_base_reindents_to_local_class_super_level() {
    // Inside a function body the whole lowering is one level deeper, so the
    // re-based continuation lines must follow the *local* `_classSuper` indent,
    // proving the fix is keyed on indent levels, not the top-level column.
    let source = concat!(
        "declare var deco: any;\n",
        "function wrap() {\n",
        "    (@deco\n",
        "    class C2 extends class { m() { return 5; } } {\n",
        "        static { super.name; }\n",
        "    });\n",
        "}\n",
    );
    let output = parse_lower_emit(source);

    assert!(
        output.contains(
            "        let _classSuper = (0, class {\n            m() { return 5; }\n        });"
        ),
        "Nested class-expression base must re-base continuation lines to the local \
         `_classSuper` indent (8 spaces), with the member one level deeper.\nOutput:\n{output}"
    );
}

#[test]
fn single_line_base_is_unchanged() {
    // An identifier base spans a single captured line: reindenting is a no-op
    // and the `extends` capture remains on the `_classSuper` line.
    let source = concat!(
        "declare var deco: any;\n",
        "declare class Base {}\n",
        "(@deco\n",
        "class C3 extends Base {\n",
        "    static { super.name; }\n",
        "});\n",
    );
    let output = parse_lower_emit(source);

    assert!(
        output.contains("let _classSuper = Base;"),
        "A single-line identifier base must be spliced verbatim with no reindent.\nOutput:\n{output}"
    );
}
