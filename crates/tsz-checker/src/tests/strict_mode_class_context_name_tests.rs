//! Regression tests for the class-context variant of the strict-mode
//! identifier-legality diagnostics.
//!
//! `tsc` selects between three messages for the same offence by asking
//! `getContainingClass(node)` — a pure walk up the identifier's own ancestor
//! chain — and only then falling back to "is this file a module?":
//!
//! | offence                    | in a class | in a module | otherwise |
//! | -------------------------- | ---------- | ----------- | --------- |
//! | strict-mode reserved word  | TS1213     | TS1214      | TS1212    |
//! | `eval` / `arguments`       | TS1210     | TS1215      | TS1100    |
//!
//! tsz used to answer "in a class" from `CheckerContext::enclosing_class`, the
//! ambient state of the member walk. That state is `None` on every path where
//! the identifier sits inside a *nested* function-like — a nested function
//! declaration, a property-initializer arrow, a function inside a static block —
//! so those all fell through to the non-class message even though they are
//! plainly code contained in a class. Two independent parameter-check paths
//! disagreeing about the same identifier also produced a *duplicate* report
//! (TS1100 from the path that missed the class plus TS1210 from the path that
//! saw it).
//!
//! Separately, set-accessor parameters and class-method *type* parameters never
//! reached the shared parameter-name check at all, so no diagnostic was reported
//! for them in any context.
//!
//! Every expectation below is pinned against `typescript@7.0.2`
//! (`--noEmit --pretty false --target es2022 --lib es2022`); the class-context
//! rows were verified under both `--strict` and `--strict false`, since class
//! bodies are auto-strict regardless of the flag.

use crate::test_utils::check_source_diagnostics;

fn diag_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_only_class_variant(codes: &[u32], expected: u32, other: u32, what: &str) {
    assert!(
        codes.contains(&expected),
        "{what}: expected TS{expected} (class-context variant). Got: {codes:?}"
    );
    assert!(
        !codes.contains(&other),
        "{what}: TS{other} (non-class variant) must not also be reported. Got: {codes:?}"
    );
    assert_eq!(
        codes.iter().filter(|&&c| c == expected).count(),
        1,
        "{what}: TS{expected} must be reported exactly once. Got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Reserved words: nested function-likes inside a class body -> TS1213
// ---------------------------------------------------------------------------

#[test]
fn reserved_word_param_of_nested_function_declaration_in_method_is_class_variant() {
    let codes = diag_codes("class H { m() { function g(yield) {} } }");
    assert_only_class_variant(&codes, 1213, 1212, "nested function declaration parameter");
}

#[test]
fn reserved_word_param_of_property_initializer_arrow_is_class_variant() {
    let codes = diag_codes("class H { p = (yield) => 0; }");
    assert_only_class_variant(&codes, 1213, 1212, "property-initializer arrow parameter");
}

#[test]
fn reserved_word_param_of_function_in_static_block_is_class_variant() {
    let codes = diag_codes("class H { static { function g(yield) {} } }");
    assert_only_class_variant(&codes, 1213, 1212, "static-block function parameter");
}

/// Anti-hardcoding cover: the rule is about the ancestor chain, not about the
/// spelling `yield`. Every ES5 strict-mode reserved word takes the same route.
#[test]
fn every_reserved_word_takes_the_class_variant_in_a_nested_function() {
    for word in [
        "implements",
        "interface",
        "let",
        "package",
        "private",
        "protected",
        "public",
        "static",
        "yield",
    ] {
        let codes = diag_codes(&format!("class H {{ m() {{ function g({word}) {{}} }} }}"));
        assert!(
            codes.contains(&1213),
            "expected TS1213 for reserved word `{word}`. Got: {codes:?}"
        );
        assert!(
            !codes.contains(&1212),
            "TS1212 must not accompany TS1213 for `{word}`. Got: {codes:?}"
        );
    }
}

/// Anti-hardcoding cover: renamed binders — the class, the method, the nested
/// function and the file all carry different names than the rows above.
#[test]
fn reserved_word_class_variant_survives_renamed_binders() {
    let codes =
        diag_codes("class ReportBuilder { renderSection() { function formatRow(package) {} } }");
    assert_only_class_variant(&codes, 1213, 1212, "renamed binders");
}

/// A class *expression* is class-like too, and so is a class nested inside a
/// function: the walk must not stop at the first function boundary in either
/// direction.
#[test]
fn reserved_word_class_variant_inside_class_expression() {
    let codes = diag_codes("const K = class { m() { function g(yield) {} } };");
    assert_only_class_variant(&codes, 1213, 1212, "class expression");
}

#[test]
fn reserved_word_class_variant_inside_class_nested_in_a_function() {
    let codes = diag_codes("function outer() { class H { m() { function g(yield) {} } } }");
    assert_only_class_variant(&codes, 1213, 1212, "class nested in a function");
}

// ---------------------------------------------------------------------------
// Negative controls: nothing above a class must be pulled into the class variant
// ---------------------------------------------------------------------------

/// The class here is a *sibling* of the offending parameter, not an ancestor.
/// `getContainingClass` walks parents only, so this stays TS1212.
#[test]
fn reserved_word_param_with_class_in_the_body_is_not_class_variant() {
    let codes = diag_codes("function f(yield) { class C {} }");
    assert!(
        codes.contains(&1212),
        "expected the plain strict-mode TS1212. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&1213),
        "a class in the body is not an enclosing class. Got: {codes:?}"
    );
}

/// A top-level function in a plain script file has neither a containing class
/// nor a module indicator.
#[test]
fn reserved_word_param_at_top_level_is_plain_strict_variant() {
    let codes = diag_codes("function f(yield) {}");
    assert!(
        codes.contains(&1212),
        "expected TS1212 outside any class. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&1213),
        "TS1213 must not fire outside a class. Got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// eval / arguments: TS1210, and reported exactly once
// ---------------------------------------------------------------------------

#[test]
fn eval_param_of_property_initializer_arrow_reports_class_variant_once() {
    let codes = diag_codes("class H { p = (eval) => eval; }");
    assert_only_class_variant(&codes, 1210, 1100, "property-initializer arrow `eval`");
}

#[test]
fn arguments_param_of_property_initializer_arrow_reports_class_variant_once() {
    let codes = diag_codes("class H { p = (arguments) => arguments; }");
    assert_only_class_variant(&codes, 1210, 1100, "property-initializer arrow `arguments`");
}

#[test]
fn eval_param_of_nested_function_in_static_block_is_class_variant() {
    let codes = diag_codes("class H { static { function g(eval) {} } }");
    assert_only_class_variant(&codes, 1210, 1100, "static-block function `eval`");
}

// ---------------------------------------------------------------------------
// Set-accessor parameters reached no parameter-name check at all
// ---------------------------------------------------------------------------

#[test]
fn set_accessor_parameter_named_reserved_word_reports_class_variant() {
    let codes = diag_codes("class H { set s(yield: number) {} }");
    assert_only_class_variant(&codes, 1213, 1212, "set-accessor reserved word");
}

#[test]
fn set_accessor_parameter_named_eval_reports_class_variant() {
    let codes = diag_codes("class H { set s(eval: number) {} }");
    assert_only_class_variant(&codes, 1210, 1100, "set-accessor `eval`");
}

#[test]
fn set_accessor_parameter_named_arguments_reports_class_variant() {
    let codes = diag_codes("class H { set s(arguments: number) {} }");
    assert_only_class_variant(&codes, 1210, 1100, "set-accessor `arguments`");
}

/// The get accessor takes no parameters, so nothing from this family may fire
/// on it — a guard against the new call site being wired to the wrong kind.
#[test]
fn get_accessor_reports_no_parameter_name_diagnostics() {
    let codes = diag_codes("class H { get yield(): number { return 1; } }");
    for code in [1100u32, 1210, 1212, 1213, 1214, 1215] {
        assert!(
            !codes.contains(&code),
            "TS{code} must not fire on a get accessor's own name. Got: {codes:?}"
        );
    }
}

/// Anti-hardcoding cover for the accessor rows: different accessor name,
/// different class name, different parameter type.
#[test]
fn set_accessor_class_variant_survives_renamed_binders() {
    let codes = diag_codes("class Viewport { set scrollOffset(interface: string) {} }");
    assert_only_class_variant(&codes, 1213, 1212, "renamed set accessor");
}

// ---------------------------------------------------------------------------
// Class-method type parameters reached no check at all
// ---------------------------------------------------------------------------

#[test]
fn method_type_parameter_named_reserved_word_reports_class_variant() {
    let codes = diag_codes("class H { m<yield>() {} }");
    assert_only_class_variant(&codes, 1213, 1212, "method type parameter");
}

#[test]
fn method_type_parameter_class_variant_survives_renamed_binders() {
    let codes = diag_codes("class TreeWalker { visitAll<package>() {} }");
    assert_only_class_variant(&codes, 1213, 1212, "renamed method type parameter");
}

/// A free function's type parameter already reported TS1212 before this change;
/// it must keep doing so rather than being pulled into the class variant.
#[test]
fn free_function_type_parameter_stays_plain_strict_variant() {
    let codes = diag_codes("function f<yield>() {}");
    assert!(
        codes.contains(&1212),
        "expected TS1212 for a free function's type parameter. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&1213),
        "TS1213 must not fire outside a class. Got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Already-correct rows, kept as a regression floor for the ambient-state path
// ---------------------------------------------------------------------------

#[test]
fn direct_method_parameter_rows_are_unchanged() {
    let reserved = diag_codes("class H { m(yield) {} }");
    assert_only_class_variant(&reserved, 1213, 1212, "direct method parameter");

    let eval_param = diag_codes("class H { m(eval) {} }");
    assert_only_class_variant(&eval_param, 1210, 1100, "direct method parameter `eval`");

    let ctor_param = diag_codes("class H { constructor(arguments) {} }");
    assert_only_class_variant(&ctor_param, 1210, 1100, "constructor parameter");
}
