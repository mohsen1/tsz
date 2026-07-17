//! Regression tests for TS1038 ("A 'declare' modifier cannot be used in an
//! already ambient context.") inside ambient namespace bodies.
//!
//! Background
//! ----------
//! `tsc`'s `checkGrammarModifiers` reports TS1038 for any `declare` modifier on
//! a declaration whose enclosing context is already ambient. tsz mirrors this in
//! `check_declare_modifiers_in_ambient_body`. Two coverage gaps motivated these
//! tests (witnessed by `compiler/giant.ts`):
//!   1. `export declare X` parses as an `EXPORT_DECLARATION` wrapping the real
//!      declaration; the modifier check must unwrap it (the wrapper's own
//!      modifiers are always `None`).
//!   2. a plain (non-`declare`) namespace nested inside a `declare namespace`
//!      is still an ambient context, so redundant `declare` modifiers inside it
//!      must be reported — the check must recurse into nested ambient bodies.
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

fn count_ts1038(diags: &[tsz_checker::diagnostics::Diagnostic]) -> usize {
    diags.iter().filter(|d| d.code == 1038).count()
}

#[test]
fn export_declare_direct_children_of_declare_namespace_report_ts1038() {
    // Each `export declare X` is EXPORT_DECLARATION-wrapped; the `declare` is
    // redundant because `Outer` is already ambient.
    let source = "\
declare namespace Outer {\n\
    export declare var alpha;\n\
    export declare function beta(): void;\n\
    export declare class Gamma { }\n\
    export declare namespace Delta { }\n\
}\n";
    let diags = check(source);
    assert_eq!(
        count_ts1038(&diags),
        4,
        "expected one TS1038 per redundant `export declare`, got {diags:?}"
    );
}

#[test]
fn export_declare_nested_in_plain_namespace_reports_ts1038() {
    // `Middle` has no `declare` of its own but is ambient by inheritance, so
    // the check must recurse into it to reach the redundant modifiers.
    let source = "\
declare namespace Root {\n\
    namespace Middle {\n\
        export declare var epsilon;\n\
        export declare function zeta(): void;\n\
    }\n\
}\n";
    let diags = check(source);
    assert_eq!(
        count_ts1038(&diags),
        2,
        "expected TS1038 for each redundant `declare` in the nested ambient namespace, got {diags:?}"
    );
}

#[test]
fn nested_own_declare_namespace_reports_ts1038_without_double_counting() {
    // Regression guard: an own-`declare` nested namespace is visited by the
    // own-declare callback AND is a child of the outer body — the recursion
    // must not double-report. Expect exactly two: one for the inner
    // namespace's `declare`, one for the variable's `declare`.
    let source = "\
declare namespace Level1 {\n\
    declare namespace Level2 {\n\
        declare var theta;\n\
    }\n\
}\n";
    let diags = check(source);
    assert_eq!(
        count_ts1038(&diags),
        2,
        "expected exactly two TS1038 (no double-report), got {diags:?}"
    );
}

#[test]
fn ambient_namespace_without_redundant_declare_reports_no_ts1038() {
    // Plain declarations (no `declare` modifier) inside a `declare namespace`
    // are correct — the outer `declare` supplies the ambient meaning.
    let source = "\
declare namespace Clean {\n\
    var iota;\n\
    function kappa(): void;\n\
    export var lambda;\n\
}\n";
    let diags = check(source);
    assert_eq!(
        count_ts1038(&diags),
        0,
        "plain ambient members must not report TS1038, got {diags:?}"
    );
}

#[test]
fn top_level_export_declare_reports_no_ts1038() {
    // At top level there is no enclosing ambient context, so `export declare`
    // is meaningful and must not report TS1038.
    let source = "\
export declare var mu;\n\
export declare function nu(): void;\n";
    let diags = check(source);
    assert_eq!(
        count_ts1038(&diags),
        0,
        "top-level `export declare` must not report TS1038, got {diags:?}"
    );
}
