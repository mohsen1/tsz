//! TS7027 reachability inside namespace bodies.
//!
//! tsc treats a module block as its own control-flow container: the body
//! starts a fresh, reachable flow, the outer walk resumes its own state
//! afterwards, and unreachable statements *inside* a namespace report their
//! own TS7027 ranges (`reachabilityChecks11.ts`). Two refinements ride along:
//!
//! - An unreachable `MODULE_DECLARATION` statement reports only when the
//!   module is instantiated (`isSourceElementUnreachable` ->
//!   `IsInstantiatedModule`), with const-enum-only namespaces gated on
//!   `preserveConstEnums`. Ambient modules get no exemption.
//! - Once a statement is covered by a reported unreachable range, everything
//!   beneath it is TS7027-silent (`withinUnreachableCode`) while normal
//!   checking continues — so a namespace or class inside a reported range
//!   never re-reports from its own body.
//!
//! All expectations in this file were pinned against `tsc` 7.0.2.

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

/// 1-based lines of every TS7027 in `source`, in emission order.
fn unreachable_lines(source: &str, preserve_const_enums: bool) -> Vec<u32> {
    let diagnostics = check_source(
        source,
        "test.ts",
        CheckerOptions {
            allow_unreachable_code: Some(false),
            preserve_const_enums,
            ..CheckerOptions::default()
        },
    );
    diagnostics
        .iter()
        .filter(|diag| diag.code == 7027)
        .map(|diag| {
            let upto = &source[..diag.start as usize];
            upto.bytes().filter(|&b| b == b'\n').count() as u32 + 1
        })
        .collect()
}

#[test]
fn unreachable_statement_inside_namespace_reports() {
    // reachabilityChecks11 `namespace A` shape, binder names varied.
    let lines = unreachable_lines(
        "namespace Outer {\n    while (true);\n    let marker;\n}\n",
        false,
    );
    assert_eq!(lines, vec![3], "let after while(true) inside a namespace");
}

#[test]
fn unreachable_nested_instantiated_namespace_reports_at_declaration() {
    let lines = unreachable_lines(
        "namespace Wrap {\n    while (true);\n    namespace Inner {\n        var value = 1;\n    }\n}\n",
        false,
    );
    assert_eq!(
        lines,
        vec![3],
        "instantiated nested namespace reports at its own declaration, body stays silent"
    );
}

#[test]
fn unreachable_nested_type_only_namespace_is_silent() {
    let lines = unreachable_lines(
        "namespace Wrap {\n    do {} while (true);\n    namespace TypesOnly {\n        interface Shape {}\n    }\n}\n",
        false,
    );
    assert_eq!(
        lines,
        Vec::<u32>::new(),
        "non-instantiated namespace never reports"
    );
}

#[test]
fn unreachable_const_enum_only_namespace_gated_on_preserve_const_enums() {
    let source = "namespace Wrap {\n    while (true);\n    namespace Enums {\n        const enum Flag { On }\n    }\n}\n";
    assert_eq!(
        unreachable_lines(source, true),
        vec![3],
        "const-enum-only namespace reports when preserveConstEnums keeps it"
    );
    assert_eq!(
        unreachable_lines(source, false),
        Vec::<u32>::new(),
        "erased const-enum-only namespace never reports"
    );
}

#[test]
fn namespace_body_ending_unreachable_does_not_leak_to_outer_statements() {
    // probe1: outer statements after the namespace stay reachable.
    let lines = unreachable_lines(
        "namespace Contained { while (true); }\nvar after = 1;\nnamespace Sibling { var q = 2; }\nvar last = 3;\n",
        false,
    );
    assert_eq!(
        lines,
        Vec::<u32>::new(),
        "module block flow is contained on exit"
    );
}

#[test]
fn namespace_in_reported_range_suppresses_its_whole_body() {
    // probe2: `fnDecl` would reset the reported flag and `banner` would
    // re-report if the covered namespace's body were not suppressed.
    let lines = unreachable_lines(
        "while (true);\nvar first = 1;\nnamespace Covered {\n    function fnDecl() {}\n    var banner = 2;\n}\nvar tail = 3;\n",
        false,
    );
    assert_eq!(
        lines,
        vec![2],
        "one range at top level; covered namespace body and tail stay silent"
    );
}

#[test]
fn deeply_nested_namespace_under_reported_range_stays_silent() {
    let lines = unreachable_lines(
        "while (true);\nvar first = 1;\nnamespace Outer {\n    namespace Deep {\n        while (true);\n        var buried = 2;\n    }\n}\n",
        false,
    );
    assert_eq!(
        lines,
        vec![2],
        "suppression covers arbitrarily deep namespace bodies"
    );
}

#[test]
fn class_in_reported_range_suppresses_member_bodies() {
    // probe10: tsc does not re-report from a method body when the class is
    // already covered by the outer unreachable range.
    let lines = unreachable_lines(
        "while (true);\nvar first = 1;\nclass Covered {\n    method() {\n        while (true);\n        var buried = 2;\n    }\n}\n",
        false,
    );
    assert_eq!(lines, vec![2], "covered class member bodies stay silent");
}

#[test]
fn function_declaration_in_unreachable_code_still_checks_its_body() {
    // probe11: a hoisted function declaration is not part of the range, so
    // its body reports its own unreachable code.
    let lines = unreachable_lines(
        "while (true);\nvar first = 1;\nfunction hoisted() {\n    while (true);\n    var buried = 2;\n}\n",
        false,
    );
    assert_eq!(
        lines,
        vec![2, 5],
        "hoisted function bodies keep their own reachability reporting"
    );
}

#[test]
fn hoisted_function_separates_ranges_inside_namespace_body() {
    // probe7: the reset-on-hoisted-declaration behavior applies inside a
    // namespace body exactly as it does at top level.
    let lines = unreachable_lines(
        "namespace Ranges {\n    while (true);\n    var head = 1;\n    var mid = 2;\n    function divider() {}\n    var tail = 3;\n}\n",
        false,
    );
    assert_eq!(
        lines,
        vec![3, 6],
        "two ranges split by the hoisted function"
    );
}

#[test]
fn ambient_instantiated_namespace_in_unreachable_code_reports() {
    // probe8: `declare namespace` gets no ambient exemption in tsc 7.
    let lines = unreachable_lines(
        "while (true);\nvar first = 1;\nfunction divider() {}\ndeclare namespace Ambient { let q: number; }\n",
        false,
    );
    assert_eq!(lines, vec![2, 4], "ambient instantiated namespace reports");
}

#[test]
fn type_only_export_namespace_breaks_range_and_next_statement_reports() {
    // probe9: a non-instantiated namespace behaves like a hoisted
    // declaration — it is skipped and separates unreachable ranges.
    let lines = unreachable_lines(
        "while (true);\nvar first = 1;\nfunction divider() {}\nnamespace TypeExports { export type Alias = string; }\nvar tail = 2;\n",
        false,
    );
    assert_eq!(
        lines,
        vec![2, 5],
        "range restarts after the erased namespace"
    );
}

#[test]
fn reachable_namespace_body_reports_nothing_without_terminator() {
    let lines = unreachable_lines(
        "namespace Fine {\n    var a = 1;\n    function helper() {}\n    var b = 2;\n}\nvar after = 3;\n",
        false,
    );
    assert_eq!(
        lines,
        Vec::<u32>::new(),
        "no false positives in reachable bodies"
    );
}

#[test]
fn dotted_namespace_body_threads_reachability() {
    let lines = unreachable_lines(
        "namespace Dot.Ted {\n    while (true);\n    let marker;\n}\n",
        false,
    );
    assert_eq!(
        lines,
        vec![3],
        "dotted namespace bodies thread like plain ones"
    );
}
