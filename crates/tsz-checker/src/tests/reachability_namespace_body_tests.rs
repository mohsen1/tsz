//! Reachability (TS7027) inside namespace bodies.
//!
//! Structural rule: a module block is a control-flow container — its
//! statement list starts a fresh reachable flow regardless of the
//! surrounding list — so unreachable code *inside* a reachable namespace
//! reports its own TS7027. When the module declaration itself is
//! unreachable, tsc reports at the declaration only when the module is
//! instantiated (`isInstantiatedModule(node, preserveConstEnums)`), and
//! checks the body with `withinUnreachableCode` set, so nothing inside an
//! unreachable namespace ever reports again. Non-instantiated namespaces
//! (only interfaces/type aliases, or only const enums without
//! `preserveConstEnums`) neither report nor extend an unreachable range.
//!
//! Oracle: typescript@7.0.2 with `--allowUnreachableCode false`
//! (`compiler/reachabilityChecks1.ts` / `compiler/reachabilityChecks11.ts`
//! plus the hand-run matrix below).

use crate::context::CheckerOptions;
use crate::test_utils::{DiagnosticShape, assert_diagnostic_shapes_exactly, check_source};

fn options() -> CheckerOptions {
    CheckerOptions {
        allow_unreachable_code: Some(false),
        ..CheckerOptions::default()
    }
}

fn check(source: &str, options: CheckerOptions) -> Vec<crate::diagnostics::Diagnostic> {
    check_source(source, "test.ts", options)
}

#[test]
fn namespace_body_reports_unreachable_after_infinite_loop() {
    let source = "namespace Zeta {
    while (true);
    let qq;
}
";
    let diagnostics = check(source, options());
    assert_diagnostic_shapes_exactly(
        source,
        &diagnostics,
        &[DiagnosticShape::code(7027).at(3, 5)],
    );
}

#[test]
fn nested_instantiated_namespace_reports_at_its_declaration() {
    let source = "namespace Wrap {
    while (true);
    namespace Inner {
        var runtime = 1;
    }
}
";
    let diagnostics = check(source, options());
    assert_diagnostic_shapes_exactly(
        source,
        &diagnostics,
        &[DiagnosticShape::code(7027).at(3, 5)],
    );
}

#[test]
fn nested_interface_only_namespace_is_not_reported() {
    let source = "namespace Iota {
    do {} while (true);
    namespace TypesOnly {
        interface Face {}
    }
}
";
    let diagnostics = check(source, options());
    assert_diagnostic_shapes_exactly(source, &diagnostics, &[]);
}

#[test]
fn type_alias_after_infinite_loop_is_not_reported() {
    let source = "namespace Tau {
    while (true);
    type Alias = string;
}
";
    let diagnostics = check(source, options());
    assert_diagnostic_shapes_exactly(source, &diagnostics, &[]);
}

#[test]
fn const_enum_only_nested_namespace_not_reported_without_preserve() {
    let source = "namespace Kappa {
    while (true);
    namespace Inner {
        const enum Flags { A }
    }
}
";
    let diagnostics = check(source, options());
    assert_diagnostic_shapes_exactly(source, &diagnostics, &[]);
}

#[test]
fn const_enum_only_nested_namespace_reported_with_preserve() {
    let source = "namespace Kappa {
    while (true);
    namespace Inner {
        const enum Flags { A }
    }
}
";
    let diagnostics = check(
        source,
        CheckerOptions {
            preserve_const_enums: true,
            ..options()
        },
    );
    assert_diagnostic_shapes_exactly(
        source,
        &diagnostics,
        &[DiagnosticShape::code(7027).at(3, 5)],
    );
}

#[test]
fn unreachable_namespace_suppresses_all_inner_reports() {
    // The namespace itself is the first reportable statement of the
    // top-level unreachable range; its body (which would report on its own
    // if the namespace were reachable) must stay silent.
    let source = "while (true);
namespace Omega {
    while (true);
    let inner;
}
";
    let diagnostics = check(source, options());
    assert_diagnostic_shapes_exactly(
        source,
        &diagnostics,
        &[DiagnosticShape::code(7027).at(2, 1)],
    );
}

#[test]
fn function_decl_inside_unreachable_namespace_does_not_resurrect_reporting() {
    // A function declaration resets the "already reported" marker inside a
    // reachable unreachable-range walk; under an unreachable namespace that
    // reset must not let the following statement report again.
    let source = "while (true);
namespace Quux {
    function gg() {}
    while (1);
}
";
    let diagnostics = check(source, options());
    assert_diagnostic_shapes_exactly(
        source,
        &diagnostics,
        &[DiagnosticShape::code(7027).at(2, 1)],
    );
}

#[test]
fn function_body_inside_unreachable_namespace_is_not_reported() {
    // tsc's `withinUnreachableCode` covers the whole subtree, including
    // nested function bodies: one diagnostic at the namespace, nothing on
    // the dead code inside `hh`.
    let source = "while (true);
namespace Sigma {
    function hh() {
        return;
        let zz;
    }
}
";
    let diagnostics = check(source, options());
    assert_diagnostic_shapes_exactly(
        source,
        &diagnostics,
        &[DiagnosticShape::code(7027).at(2, 1)],
    );
}

#[test]
fn function_body_inside_reachable_namespace_still_reports() {
    let source = "namespace Rho {
    function hh() {
        return;
        let zz;
    }
}
";
    let diagnostics = check(source, options());
    assert_diagnostic_shapes_exactly(
        source,
        &diagnostics,
        &[DiagnosticShape::code(7027).at(4, 9)],
    );
}

#[test]
fn dotted_namespace_body_reports_unreachable_code() {
    let source = "namespace Dot.Ted {
    while (true);
    let ww;
}
";
    let diagnostics = check(source, options());
    assert_diagnostic_shapes_exactly(
        source,
        &diagnostics,
        &[DiagnosticShape::code(7027).at(3, 5)],
    );
}

#[test]
fn non_instantiated_namespace_breaks_the_unreachable_range() {
    // The type-only namespace is skipped (not reported), and — like other
    // non-executable statements — it splits the range so the next runtime
    // statement reports its own TS7027.
    let source = "while (true);
namespace TypeOnly {
    type XX = string;
}
var vv = 1;
";
    let diagnostics = check(source, options());
    assert_diagnostic_shapes_exactly(
        source,
        &diagnostics,
        &[DiagnosticShape::code(7027).at(5, 1)],
    );
}

#[test]
fn hoisted_var_without_initializer_is_not_reported() {
    let source = "namespace Vee {
    while (true);
    var yy;
}
";
    let diagnostics = check(source, options());
    assert_diagnostic_shapes_exactly(source, &diagnostics, &[]);
}

#[test]
fn deeply_nested_namespace_gets_its_own_fresh_flow() {
    // The inner namespace's unreachable `var` reports; the statement after
    // the nested namespace in the outer body stays reachable (a module
    // declaration falls through).
    let source = "namespace Outer1 {
    namespace Mid2 {
        do {} while (true);
        var deep = 1;
    }
    let after = 2;
}
";
    let diagnostics = check(source, options());
    assert_diagnostic_shapes_exactly(
        source,
        &diagnostics,
        &[DiagnosticShape::code(7027).at(4, 9)],
    );
}
