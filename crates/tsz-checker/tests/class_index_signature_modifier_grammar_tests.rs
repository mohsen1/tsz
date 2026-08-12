//! Regression tests for TS1071 ("'{0}' modifier cannot appear on an index
//! signature.") on a class index signature.
//!
//! Background
//! ----------
//! `tsc`'s `checkGrammarModifiers` reports TS1071 for any modifier other than
//! `readonly`/`static` on a class index signature, at the FIRST offending
//! modifier, then returns — one diagnostic per index signature, not one per
//! offending modifier. Before this fix,
//! `member_declaration_checks.rs`'s `INDEX_SIGNATURE` branch hardcoded the
//! TS1071 set to `public`/`private`/`protected`/`export`, so `declare`,
//! `abstract`, `async`, `override`, `accessor`, `in`, `out`, and `const` were
//! silently accepted, and multiple offending modifiers on one signature were
//! each reported (issue #17280).
//!
//! Binder names are varied across cases so no fix can key on an identifier.

use tsz_checker::context::CheckerOptions;

fn check(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    let lib_files =
        tsz_checker::test_utils::load_compiled_lib_files(&["lib.es5.d.ts", "lib.es2015.d.ts"]);
    tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions::default(),
        &lib_files,
    )
}

fn count_ts1071(diags: &[tsz_checker::diagnostics::Diagnostic]) -> usize {
    diags.iter().filter(|d| d.code == 1071).count()
}

#[test]
fn declare_modifier_on_class_index_signature_reports_ts1071() {
    let source = "\
class Alpha {\n\
    declare [beta: string]: number;\n\
}\n";
    let diags = check(source);
    assert_eq!(count_ts1071(&diags), 1, "got {diags:?}");
}

#[test]
fn abstract_modifier_on_class_index_signature_reports_ts1071() {
    let source = "\
abstract class Gamma {\n\
    abstract [delta: string]: number;\n\
}\n";
    let diags = check(source);
    assert_eq!(count_ts1071(&diags), 1, "got {diags:?}");
}

#[test]
fn async_modifier_on_class_index_signature_reports_ts1071() {
    let source = "\
class Epsilon {\n\
    async [zeta: string]: number;\n\
}\n";
    let diags = check(source);
    assert_eq!(count_ts1071(&diags), 1, "got {diags:?}");
}

#[test]
fn override_modifier_on_class_index_signature_reports_ts1071() {
    let source = "\
class Eta {\n\
    override [theta: string]: number;\n\
}\n";
    let diags = check(source);
    assert_eq!(count_ts1071(&diags), 1, "got {diags:?}");
}

#[test]
fn accessor_modifier_on_class_index_signature_reports_ts1071() {
    let source = "\
class Iota {\n\
    accessor [kappa: string]: number;\n\
}\n";
    let diags = check(source);
    assert_eq!(count_ts1071(&diags), 1, "got {diags:?}");
}

#[test]
fn in_modifier_on_class_index_signature_reports_ts1071() {
    let source = "\
class Lambda {\n\
    in [mu: string]: number;\n\
}\n";
    let diags = check(source);
    assert_eq!(count_ts1071(&diags), 1, "got {diags:?}");
}

#[test]
fn const_modifier_on_class_index_signature_reports_ts1071() {
    let source = "\
class Nu {\n\
    const [xi: string]: number;\n\
}\n";
    let diags = check(source);
    assert_eq!(count_ts1071(&diags), 1, "got {diags:?}");
}

#[test]
fn accessibility_modifier_on_class_index_signature_still_reports_ts1071() {
    // Pre-existing coverage: guard against a regression on the modifiers that
    // were already handled before this fix.
    let source = "\
class Omicron {\n\
    public [pi: string]: number;\n\
}\n";
    let diags = check(source);
    assert_eq!(count_ts1071(&diags), 1, "got {diags:?}");
}

#[test]
fn readonly_modifier_on_class_index_signature_reports_no_ts1071() {
    let source = "\
class Rho {\n\
    readonly [sigma: string]: number;\n\
}\n";
    let diags = check(source);
    assert_eq!(count_ts1071(&diags), 0, "got {diags:?}");
}

#[test]
fn static_modifier_on_class_index_signature_reports_no_ts1071() {
    let source = "\
class Tau {\n\
    static [upsilon: string]: number;\n\
}\n";
    let diags = check(source);
    assert_eq!(count_ts1071(&diags), 0, "got {diags:?}");
}

#[test]
fn multiple_offending_modifiers_report_exactly_one_ts1071() {
    // tsc's checkGrammarModifiers stops at the FIRST offending modifier and
    // returns; a signature with several illegal modifiers still reports one.
    let source = "\
class Phi {\n\
    public private [chi: string]: number;\n\
}\n";
    let diags = check(source);
    assert_eq!(
        count_ts1071(&diags),
        1,
        "tsc reports exactly one TS1071 per index signature, got {diags:?}"
    );
}

#[test]
fn readonly_and_static_together_report_no_ts1071() {
    let source = "\
class Psi {\n\
    static readonly [omega: string]: number;\n\
}\n";
    let diags = check(source);
    assert_eq!(count_ts1071(&diags), 0, "got {diags:?}");
}
