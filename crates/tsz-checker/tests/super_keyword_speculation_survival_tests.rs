//! Locks tsc-parity for `super`-validity diagnostics that are emitted
//! during speculative type-computation contexts (e.g. inside the body of
//! a class method declared in a static field initializer of a derived
//! class). The diagnostic is a grammar/scope error — its validity does
//! not depend on which speculative branch is being probed — and must
//! survive `rollback_diagnostics`.
//!
//! Regression target: `typeOfThisInStaticMembers9.ts`.

use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_common::checker_options::CheckerOptions;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn checker_diagnostics(source: &str) -> Vec<(u32, String)> {
    let file_name = "test.ts";
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let options = CheckerOptions::default();

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn ts2335_emitted_for_super_in_inner_class_method_body_after_sibling_arrow_super() {
    // Minimal repro: derived class D has a static arrow boundary that
    // legitimately uses super; the static IIFE-init body declares an inner
    // non-derived class whose method body uses super. The inner method's
    // super check fires inside speculative type computation of the
    // surrounding static field initializer, so its diagnostic must survive
    // rollback. Without the fix, this emits 0 diagnostics; with the fix it
    // emits TS2335 on the method body's super.
    let source = r#"
class C { static f = 1 }
class D extends C {
    static arrowFunctionBoundary = () => super.f + 1;
    static functionAndClassDeclBoundary = (() => {
        class Inner {
            method () {
                return super.f + 6
            }
        }
    })();
}
"#;
    let diags = checker_diagnostics(source);
    let ts2335 = diags
        .iter()
        .filter(|(c, _)| *c == diagnostic_codes::SUPER_CAN_ONLY_BE_REFERENCED_IN_A_DERIVED_CLASS)
        .count();
    assert!(
        ts2335 >= 1,
        "Expected TS2335 on the inner method's `super.f`. Got: {diags:?}"
    );
}

#[test]
fn ts2335_emitted_for_super_in_inner_class_method_without_arrow_sibling() {
    // Control: same shape minus the arrow boundary. Already worked
    // pre-fix; this test guards against accidental regressions in the
    // baseline path.
    let source = r#"
class C { static f = 1 }
class D extends C {
    static functionAndClassDeclBoundary = (() => {
        class Inner {
            method () {
                return super.f + 6
            }
        }
    })();
}
"#;
    let diags = checker_diagnostics(source);
    let ts2335 = diags
        .iter()
        .filter(|(c, _)| *c == diagnostic_codes::SUPER_CAN_ONLY_BE_REFERENCED_IN_A_DERIVED_CLASS)
        .count();
    assert!(
        ts2335 >= 1,
        "Baseline path: expected TS2335 on the inner method's `super.f`. Got: {diags:?}"
    );
}

#[test]
fn ts2335_emitted_for_init_and_method_when_both_use_super() {
    // Both an instance-property initializer (non-speculative path) and a
    // method body (speculative path) reference `super` in the inner class.
    // Both must emit TS2335.
    let source = r#"
class C { static f = 1 }
class D extends C {
    static arrowFunctionBoundary = () => super.f + 1;
    static functionAndClassDeclBoundary = (() => {
        class Inner {
            init = super.f + 5
            method () {
                return super.f + 6
            }
        }
    })();
}
"#;
    let diags = checker_diagnostics(source);
    let ts2335 = diags
        .iter()
        .filter(|(c, _)| *c == diagnostic_codes::SUPER_CAN_ONLY_BE_REFERENCED_IN_A_DERIVED_CLASS)
        .count();
    assert!(
        ts2335 >= 2,
        "Expected at least 2 TS2335 (init + method super). Got: {diags:?}"
    );
}
