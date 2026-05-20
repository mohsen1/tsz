//! Regression tests for enum value planning across declarations.
//!
//! These cover the emit facts used by `isolatedDeclarationErrorsEnums`: enum
//! lowering must fold top-level `const` numeric values and carry string enum
//! member values forward for later enum initializers.

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

fn emit_esnext(source: &str) -> String {
    parse_lower_emit(
        source,
        PrinterOptions {
            target: ScriptTarget::ESNext,
            ..Default::default()
        },
    )
}

#[test]
fn enum_initializer_folds_top_level_const_numeric_value() {
    let output = emit_esnext(
        r#"
const EV = 1;
enum ExtFlags {
    D = 4 >> 1,
    E = EV,
}
"#,
    );

    assert!(
        output.contains(r#"ExtFlags[ExtFlags["D"] = 2] = "D";"#),
        "Enum constant expression should still fold local arithmetic.\nOutput:\n{output}"
    );
    assert!(
        output.contains(r#"ExtFlags[ExtFlags["E"] = 1] = "E";"#),
        "Enum initializer should fold top-level const numeric values.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(r#"ExtFlags["E"] = EV"#),
        "Enum initializer should not emit the top-level const identifier once folded.\nOutput:\n{output}"
    );
}

#[test]
fn enum_string_values_fold_across_prior_enum_property_and_element_access() {
    let output = emit_esnext(
        r#"
enum Str {
    A = "A",
    B = "B",
    AB = A + B,
}
enum StrExt {
    D = "D",
    ABD = Str.AB + D,
    AD = Str["A"] + D,
}
"#,
    );

    assert!(
        output.contains(r#"Str["AB"] = "AB";"#),
        "First enum should fold local string concatenation.\nOutput:\n{output}"
    );
    assert!(
        output.contains(r#"StrExt["ABD"] = "ABD";"#),
        "Later enum should fold prior enum string values through property access.\nOutput:\n{output}"
    );
    assert!(
        output.contains(r#"StrExt["AD"] = "AD";"#),
        "Later enum should fold prior enum string values through element access.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(r#"StrExt[StrExt["AD"]"#),
        "Folded string enum members must not receive numeric reverse mappings.\nOutput:\n{output}"
    );
}
