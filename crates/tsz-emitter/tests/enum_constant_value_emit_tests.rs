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

#[test]
fn const_enum_division_inlines_float_quotient() {
    // ECMAScript `/` is float division; const-enum inlining must emit the
    // exact fractional constant, not the truncated integer (regression: 10/4
    // inlined as 2 instead of 2.5). See issue: enum constant folding `/`.
    let output = emit_esnext(
        r#"
const enum CE { A = 10 / 4, B = 3.14, C = 7 / 2, D = 1 / 3, E = 0.5, F = 100 / 8 }
let v = [CE.A, CE.B, CE.C, CE.D, CE.E, CE.F];
"#,
    );
    assert!(
        output.contains(
            r#"[2.5 /* CE.A */, 3.14 /* CE.B */, 3.5 /* CE.C */, 0.3333333333333333 /* CE.D */, 0.5 /* CE.E */, 12.5 /* CE.F */]"#
        ),
        "const-enum division must inline true float quotients byte-for-byte with tsc.\nOutput:\n{output}"
    );
}

#[test]
fn regular_enum_division_object_member_is_float() {
    // The shared evaluator feeds the regular-enum object emit too, so the
    // member assignment must carry the float value.
    let output = emit_esnext(
        r#"
enum RE { A = 10 / 4, B = 7 / 2 }
"#,
    );
    assert!(
        output.contains(r#"RE[RE["A"] = 2.5] = "A";"#),
        "regular enum division member A must be 2.5.\nOutput:\n{output}"
    );
    assert!(
        output.contains(r#"RE[RE["B"] = 3.5] = "B";"#),
        "regular enum division member B must be 3.5.\nOutput:\n{output}"
    );
}

#[test]
fn enum_division_with_integral_quotient_stays_integer() {
    // Quotients that are integral re-narrow to integers, matching tsc (no
    // spurious `.0`).
    let output = emit_esnext(
        r#"
enum RE { A = 8 / 4, B = 9 / 3 }
"#,
    );
    assert!(
        output.contains(r#"RE[RE["A"] = 2] = "A";"#)
            && output.contains(r#"RE[RE["B"] = 3] = "B";"#),
        "integral quotients must emit as integers.\nOutput:\n{output}"
    );
}

#[test]
fn namespace_enum_division_is_float_at_es5_and_esnext() {
    // The namespace-nested enum IR path (`enum_es5_ir.rs`) must also honor
    // ECMAScript float division, at both ES5 (its own IR emitter) and ES2015+.
    let source = "namespace N { export enum E { A = 10 / 4, B = 7 / 2, C = 8 / 4 } }\n";
    for target in [ScriptTarget::ES5, ScriptTarget::ESNext] {
        let output = parse_lower_emit(
            source,
            PrinterOptions {
                target,
                ..Default::default()
            },
        );
        assert!(
            output.contains(r#"E[E["A"] = 2.5] = "A";"#)
                && output.contains(r#"E[E["B"] = 3.5] = "B";"#)
                && output.contains(r#"E[E["C"] = 2] = "C";"#),
            "namespace enum division must emit float quotients at {target:?}.\nOutput:\n{output}"
        );
    }
}

#[test]
fn enum_es5_float_member_values_use_number_tostring_text() {
    // tsc renders evaluated float member values through `Number::toString`
    // (`1e21` → `1e+21`, `1e-7` → `1e-7`), not Rust's `Display` shortest form.
    let output = parse_lower_emit(
        r#"
enum Big {
    A = 1e21,
    B = 1e-7,
}
"#,
        PrinterOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );

    assert!(
        output.contains(r#"Big[Big["A"] = 1e+21] = "A";"#),
        "ES5 enum float value should print as JS Number::toString text.\nOutput:\n{output}"
    );
    assert!(
        output.contains(r#"Big[Big["B"] = 1e-7] = "B";"#),
        "ES5 enum small float value should print as JS Number::toString text.\nOutput:\n{output}"
    );
}
