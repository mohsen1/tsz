//! TS2517 elaboration: "Cannot assign an abstract constructor type to a
//! non-abstract constructor type." must accompany the top-level TS2322/TS2345
//! when an abstract constructor type is assigned to a concrete one.
//!
//! Structural rule: when the source resolves to an abstract construct
//! signature and the target is a concrete construct signature, tsc rejects the
//! relation and explains it with the TS2517 elaboration line. The rule is
//! independent of class/alias name and of how the constructor target is
//! spelled (`new () => T` vs `{ new(): T }`).

use tsz_checker::test_utils::check_source_diagnostics;
use tsz_common::diagnostics::Diagnostic;

const ABSTRACT_ELABORATION: &str =
    "Cannot assign an abstract constructor type to a non-abstract constructor type.";

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source_diagnostics(source)
}

fn has_abstract_elaboration(diagnostics: &[Diagnostic], code: u32) -> bool {
    diagnostics.iter().any(|diag| {
        diag.code == code
            && diag
                .related_information
                .iter()
                .any(|info| info.message_text == ABSTRACT_ELABORATION)
    })
}

#[test]
fn assignment_emits_ts2322_with_abstract_elaboration() {
    let diags = diagnostics(
        r#"
abstract class A {}
const c: new () => A = A;
"#,
    );
    assert!(
        has_abstract_elaboration(&diags, 2322),
        "expected TS2322 with TS2517 elaboration; got {diags:?}"
    );
}

#[test]
fn argument_emits_ts2345_with_abstract_elaboration() {
    let diags = diagnostics(
        r#"
abstract class A { abstract m(): void; }
declare function need(c: new () => A): void;
need(A);
"#,
    );
    assert!(
        has_abstract_elaboration(&diags, 2345),
        "expected TS2345 with TS2517 elaboration; got {diags:?}"
    );
}

#[test]
fn object_type_constructor_target_also_elaborates() {
    // Boundary control from the issue: a `{ new(): A }` target (not the arrow
    // `new () => A` form) must produce the same elaboration.
    let diags = diagnostics(
        r#"
abstract class A { abstract m(): void; }
type C = { new (): A };
const c: C = A;
"#,
    );
    assert!(
        has_abstract_elaboration(&diags, 2322),
        "expected TS2322 with TS2517 elaboration for object-type constructor target; got {diags:?}"
    );
}

#[test]
fn elaboration_is_not_class_name_specific() {
    // Same shape, a differently named class — proves the rule is structural,
    // not keyed on any particular identifier.
    let diags = diagnostics(
        r#"
abstract class Widget { abstract render(): void; }
declare function build(ctor: new () => Widget): void;
build(Widget);
"#,
    );
    assert!(
        has_abstract_elaboration(&diags, 2345),
        "expected TS2345 with TS2517 elaboration for renamed class; got {diags:?}"
    );
}

#[test]
fn abstract_target_does_not_elaborate() {
    // Negative/fallback case: an abstract target accepts an abstract source,
    // so neither the error nor the elaboration should appear.
    let diags = diagnostics(
        r#"
abstract class A {}
const c: abstract new () => A = A;
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "abstract→abstract constructor assignment must not error; got {diags:?}"
    );
    assert!(
        !diags
            .iter()
            .flat_map(|d| d.related_information.iter())
            .any(|info| info.message_text == ABSTRACT_ELABORATION),
        "abstract→abstract constructor assignment must not elaborate; got {diags:?}"
    );
}

#[test]
fn concrete_source_does_not_elaborate() {
    // Negative case: a concrete class is assignable to a concrete constructor
    // target, so there is no failure and no elaboration.
    let diags = diagnostics(
        r#"
class A {}
const c: new () => A = A;
"#,
    );
    assert!(
        !diags
            .iter()
            .flat_map(|d| d.related_information.iter())
            .any(|info| info.message_text == ABSTRACT_ELABORATION),
        "concrete→concrete constructor assignment must not elaborate; got {diags:?}"
    );
}
