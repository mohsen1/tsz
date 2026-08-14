//! `TS2345` call-argument diagnostics against a bare type-parameter target owe
//! the same `TS5082`/`TS5075` elaboration note the direct-assignment `TS2322`
//! surface attaches (`nested_type_parameter_target_elaboration_tests.rs`,
//! #17445/#17446).
//!
//! Structural rule: when a call argument fails to relate to a target that
//! carries a *free* (caller-scope) type parameter, `check_call_result`
//! (`types/computation/call_result.rs`) routes the diagnostic through
//! `error_argument_not_assignable_preserving_param_display` instead of the
//! general assignability gateway (`check_argument_assignable_or_report`), so
//! the written `T` name survives in the rendered message rather than being
//! resolved/substituted away. That bypass built only the bare `TS2345` head
//! and never attached the elaboration
//! (`unrelated_type_parameter_target_related_info`, gated on the target
//! genuinely being a bare type parameter — a no-op for `T[]`/`Foo<T>`
//! targets, which keep their own, unrelated elaboration story).
//!
//! #17449.

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_diagnostics;

/// `TS5082` — "`'{T}'` could be instantiated with an arbitrary type …".
const COULD_BE_INSTANTIATED_ARBITRARY: u32 = 5082;
/// `TS5075` — "… could be instantiated with a different subtype of constraint …".
const COULD_BE_INSTANTIATED_DIFFERENT_SUBTYPE: u32 = 5075;

fn ts2345(source: &str) -> Diagnostic {
    check_source_diagnostics(source)
        .into_iter()
        .find(|d| d.code == 2345)
        .unwrap_or_else(|| panic!("expected a TS2345 for source:\n{source}"))
}

#[test]
fn unconstrained_bare_type_parameter_argument_gets_arbitrary_note() {
    // `T` carries no `extends` clause of its own, so a failing argument gets
    // the TS5082 "could be instantiated with an arbitrary type" note.
    let diag = ts2345(
        r#"
declare function takesT<T extends unknown>(x: T): void;
function c<T>() {
    takesT<T>(null);
}
"#,
    );
    let note = diag
        .related_information
        .iter()
        .find(|r| r.code == COULD_BE_INSTANTIATED_ARBITRARY)
        .unwrap_or_else(|| panic!("expected TS5082; got: {:?}", diag.related_information));
    assert_eq!(
        note.message_text,
        "'T' could be instantiated with an arbitrary type which could be unrelated to 'null'."
    );
}

#[test]
fn constrained_bare_type_parameter_argument_gets_subtype_note() {
    // The caller's `T extends string` constraint is satisfied by the
    // argument, so tsc reports the TS5075 "different subtype of constraint"
    // variant instead of the unconstrained TS5082 fallback.
    let diag = ts2345(
        r#"
declare function takesT<T extends unknown>(x: T): void;
function e<T extends string>(x: string) {
    takesT<T>(x);
}
"#,
    );
    let note = diag
        .related_information
        .iter()
        .find(|r| r.code == COULD_BE_INSTANTIATED_DIFFERENT_SUBTYPE)
        .unwrap_or_else(|| panic!("expected TS5075; got: {:?}", diag.related_information));
    assert_eq!(
        note.message_text,
        "'string' is assignable to the constraint of type 'T', but 'T' could be instantiated with a different subtype of constraint 'string'."
    );
}

#[test]
fn renamed_binders_still_emit_the_note() {
    // Anti-hardcoding: a different type-parameter spelling and function name
    // must produce the identical (structural, not name-keyed) note.
    let diag = ts2345(
        r#"
declare function accepts<Elem extends unknown>(value: Elem): void;
function outer<Elem>() {
    accepts<Elem>(undefined);
}
"#,
    );
    let note = diag
        .related_information
        .iter()
        .find(|r| r.code == COULD_BE_INSTANTIATED_ARBITRARY)
        .unwrap_or_else(|| panic!("expected TS5082; got: {:?}", diag.related_information));
    assert!(
        note.message_text.contains("Elem"),
        "note must name the actual type parameter, got: {}",
        note.message_text
    );
}

#[test]
fn array_of_type_parameter_target_has_no_bare_note() {
    // Control: `T[]` contains a free type parameter (so the call still takes
    // the param-display-preserving path) but is not itself a bare type
    // parameter, so it must not gain the bare-target elaboration.
    let diag = ts2345(
        r#"
declare function takesArr<T extends unknown>(x: T[]): void;
function h<T>(v: number[]) {
    takesArr<T>(v);
}
"#,
    );
    assert!(
        diag.related_information
            .iter()
            .all(|r| r.code != COULD_BE_INSTANTIATED_ARBITRARY
                && r.code != COULD_BE_INSTANTIATED_DIFFERENT_SUBTYPE),
        "T[] target must not emit the bare type-parameter note, got: {:?}",
        diag.related_information
    );
}

#[test]
fn concrete_target_has_no_bare_note() {
    // Control: an entirely concrete target (no free type parameter anywhere)
    // never takes the param-display-preserving path at all, so the ordinary
    // assignability gateway handles it and no type-parameter note is
    // attached.
    let diag = ts2345(
        r#"
declare function takesNum(x: number): void;
takesNum("str");
"#,
    );
    assert!(
        diag.related_information
            .iter()
            .all(|r| r.code != COULD_BE_INSTANTIATED_ARBITRARY
                && r.code != COULD_BE_INSTANTIATED_DIFFERENT_SUBTYPE),
        "concrete target must not emit the free type-parameter note, got: {:?}",
        diag.related_information
    );
}
