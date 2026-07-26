//! An inaccessible constructor still yields the instance type.
//!
//! tsc's `resolveNewExpression` reports TS2673/TS2674 for a private/protected
//! constructor and suppresses the *call's* subsequent arg-count and
//! type-mismatch diagnostics — but the new-expression's type is still the
//! instance type. tsz returned `any` instead, which poisoned everything
//! downstream: `var c = new C()` made `c` `any`, so any later misuse of `c`
//! silently passed.
//!
//! Pins `conformance/types/members/typesWithPrivateConstructor.ts` and
//! `typesWithProtectedConstructor.ts`, whose oracle expects the accessibility
//! error AND a TS2322 on `var r: () => void = c.constructor` (an instance's
//! `.constructor` is `Function`, which is not assignable to a specific
//! function type).

use tsz_checker::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    let mut c: Vec<u32> = check_source_diagnostics(source)
        .iter()
        .map(|d| d.code)
        .collect();
    c.sort_unstable();
    c
}

#[test]
fn private_constructor_keeps_instance_type_for_downstream_checks() {
    let source = "class C { private constructor() { } }\n\
                  var c = new C();\n\
                  var r: () => void = c.constructor;\n";
    assert_eq!(
        codes(source),
        vec![2322, 2673],
        "expected the accessibility error AND the downstream assignability error"
    );
}

#[test]
fn protected_constructor_keeps_instance_type_for_downstream_checks() {
    let source = "class C { protected constructor() { } }\n\
                  var c = new C();\n\
                  var r: () => void = c.constructor;\n";
    assert_eq!(codes(source), vec![2322, 2674]);
}

/// The instance type must be the class, not `any` — a genuinely wrong member
/// access on it still has to be caught.
#[test]
fn inaccessible_constructor_instance_still_reports_unknown_members() {
    let source = "class C { private constructor() { } x: number = 1; }\n\
                  var c = new C();\n\
                  c.notAMember;\n";
    assert_eq!(
        codes(source),
        vec![2339, 2673],
        "the instance must still be typed `C`, so TS2339 fires"
    );
}

/// ...and a correct member access on it must NOT error.
#[test]
fn inaccessible_constructor_instance_allows_real_members() {
    let source = "class C { private constructor() { } x: number = 1; }\n\
                  var c = new C();\n\
                  var n: number = c.x;\n";
    assert_eq!(
        codes(source),
        vec![2673],
        "only the accessibility error is due"
    );
}

/// The public-constructor path was already correct and must not shift.
#[test]
fn public_constructor_is_unchanged() {
    let source = "class C { constructor() { } }\n\
                  var c = new C();\n\
                  var r: () => void = c.constructor;\n";
    assert_eq!(codes(source), vec![2322]);
}

/// Renamed binders, so the rule is not keyed to a particular class name.
#[test]
fn rule_holds_for_renamed_binders() {
    let source = "class Holder { private constructor() { } value: string = \"\"; }\n\
                  var held = new Holder();\n\
                  var n: number = held.value;\n";
    assert_eq!(
        codes(source),
        vec![2322, 2673],
        "instance type survives, so string-to-number is still caught"
    );
}
