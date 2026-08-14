//! Tests that `tsc`'s bare-type-parameter-target elaboration
//! (`TS5082`/`TS5075`, "`'T'` could be instantiated with an arbitrary type
//! …" / "… could be instantiated with a different subtype of constraint …") is
//! attached at *every* nesting level the type-parameter target fails at, not
//! only the top-level mismatch.
//!
//! Structural rule: when a concrete source fails to relate to a bare
//! type-parameter target, `tsc`'s `reportRelationError` appends the
//! type-parameter note beneath the failing `Type '{src}' is not assignable to
//! type '{T}'.` line — whether that line is the top-level diagnostic or a
//! nested `Types of property 'x' are incompatible.` elaboration child. Before
//! the fix tsz gated the note on `depth == 0`, dropping it for the nested
//! object-literal / tuple / array-element forms that real generic code (e.g.
//! the valibot row, #13212) hits.
//!
//! The note's related-info `depth` tracks the failing line's chain depth so it
//! renders one indentation level deeper, mirroring tsc's progressive indent.
//!
//! The trailing `ts2345_call_argument_*` tests cover the same note on the
//! call-argument (`TS2345`) surface, which builds its diagnostic through a
//! separate "preserve the parameter display" fallback rather than
//! `render_failure_reason` and previously dropped the note entirely (#17449).

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_diagnostics;

/// `TS5082` — "`'{T}'` could be instantiated with an arbitrary type …".
const COULD_BE_INSTANTIATED_ARBITRARY: u32 = 5082;
/// `TS5075` — "… could be instantiated with a different subtype of constraint …".
const COULD_BE_INSTANTIATED_DIFFERENT_SUBTYPE: u32 = 5075;

fn ts2322(source: &str) -> Diagnostic {
    check_source_diagnostics(source)
        .into_iter()
        .find(|d| d.code == 2322)
        .unwrap_or_else(|| panic!("expected a TS2322 for source:\n{source}"))
}

fn ts2345(source: &str) -> Diagnostic {
    check_source_diagnostics(source)
        .into_iter()
        .find(|d| d.code == 2345)
        .unwrap_or_else(|| panic!("expected a TS2345 for source:\n{source}"))
}

/// Find the type-parameter elaboration note (either constraint variant) and
/// return its rendering `depth`.
fn type_param_note_depth(diag: &Diagnostic, code: u32) -> u8 {
    diag.related_information
        .iter()
        .find(|r| r.code == code)
        .unwrap_or_else(|| {
            panic!(
                "expected related note TS{code}; got: {:?}",
                diag.related_information
            )
        })
        .depth
}

#[test]
fn direct_type_parameter_return_note_at_depth_zero() {
    // A direct mismatch against a bare type-parameter target: the failing
    // `Type 'number' is not assignable to type 'T'.` line is the diagnostic
    // header itself, so the note is its first child at related-depth 0.
    let diag = ts2322(
        r#"
function make<T>(value: number): T {
    return value;
}
"#,
    );
    assert_eq!(
        type_param_note_depth(&diag, COULD_BE_INSTANTIATED_ARBITRARY),
        0,
        "direct type-param note must sit at depth 0, got: {:?}",
        diag.related_information
    );
}

#[test]
fn nested_object_property_note_indents_below_chain() {
    // `{ field: T }` reached through a non-fresh source builds the structural
    // chain `Type '{…}' …` → `Types of property 'field' are incompatible.` →
    // `Type 'string' is not assignable to type 'T'.`, so the note sits two
    // levels deeper than the header (related-depth 2). Before the fix this note
    // was dropped entirely for the nested form.
    let diag = ts2322(
        r#"
function make<T>(): { field: T } {
    const built = { field: "value" };
    return built;
}
"#,
    );
    assert_eq!(
        type_param_note_depth(&diag, COULD_BE_INSTANTIATED_ARBITRARY),
        2,
        "nested object-property type-param note must indent below the chain, got: {:?}",
        diag.related_information
    );
}

#[test]
fn array_element_type_parameter_note_indents_deepest() {
    // An array-element relation adds one more chain frame
    // (`Type '{…}[]' …` → `Type '{…}' …` → `Types of property 'x' …` →
    // `Type 'string' … 'T'.`), so the note lands at related-depth 3.
    let diag = ts2322(
        r#"
function make<T>(): { x: T }[] {
    const built = [{ x: "value" }];
    return built;
}
"#,
    );
    assert_eq!(
        type_param_note_depth(&diag, COULD_BE_INSTANTIATED_ARBITRARY),
        3,
        "array-element type-param note must indent past every chain frame, got: {:?}",
        diag.related_information
    );
}

#[test]
fn renamed_binders_still_emit_nested_note() {
    // Anti-hardcoding: a different type-parameter spelling and property name
    // must produce the identical note (structural, not name-keyed).
    let diag = ts2322(
        r#"
function build<Elem>(): { entry: Elem } {
    const made = { entry: "value" };
    return made;
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
fn deeper_nested_object_property_note_indents_further() {
    // `{ outer: { inner: T } }` collapses to a single `The types of
    // 'outer.inner' are incompatible …` line at related-depth 0, so the failing
    // `… type 'T'.` line is at depth 1 and the note sits one deeper, at depth 2.
    let diag = ts2322(
        r#"
function make<Held>(): { outer: { inner: Held } } {
    const built = { outer: { inner: "value" } };
    return built;
}
"#,
    );
    assert_eq!(
        type_param_note_depth(&diag, COULD_BE_INSTANTIATED_ARBITRARY),
        2,
        "deeply nested type-param note must indent past the collapse line, got: {:?}",
        diag.related_information
    );
}

#[test]
fn satisfiable_constraint_emits_subtype_variant_nested() {
    // The source satisfies the constraint, so tsc reports the TS5075 "different
    // subtype of constraint" variant — still emitted at the nested leaf.
    let diag = ts2322(
        r#"
function make<T extends string | number>(): { field: T } {
    const built = { field: "value" };
    return built;
}
"#,
    );
    assert_eq!(
        type_param_note_depth(&diag, COULD_BE_INSTANTIATED_DIFFERENT_SUBTYPE),
        2,
        "constraint-satisfied source must use the TS5075 subtype variant nested, got: {:?}",
        diag.related_information
    );
}

// The call-argument (`TS2345`) surface builds its "preserve the parameter
// display" fallback diagnostic directly
// (`error_argument_not_assignable_preserving_param_display`) instead of
// through `render_failure_reason`, so it needs its own explicit call to
// `unrelated_type_parameter_target_related_info` — without it the note was
// silently dropped for every call-argument mismatch against a bare
// type-parameter target (#17449), even though the structurally identical
// direct-assignment (`TS2322`) case above already carried it.

#[test]
fn ts2345_call_argument_unconstrained_type_parameter_target_gets_arbitrary_note() {
    // The target parameter is an unconstrained bare type parameter fixed by
    // an explicit type argument (`takesU<U>`) from the enclosing generic
    // function, so `5` cannot be shown related to it: TS5082.
    let diag = ts2345(
        r#"
declare function takesU<U>(x: U): void;
function outer<U>() {
    takesU<U>(5);
}
"#,
    );
    let note = diag
        .related_information
        .iter()
        .find(|r| r.code == COULD_BE_INSTANTIATED_ARBITRARY)
        .unwrap_or_else(|| panic!("expected TS5082; got: {:?}", diag.related_information));
    assert!(
        note.message_text.contains('U'),
        "note must name the actual type parameter, got: {}",
        note.message_text
    );
}

#[test]
fn ts2345_call_argument_constrained_type_parameter_target_gets_subtype_note() {
    // The argument satisfies the target type parameter's own `extends`
    // constraint but is not provably `T` itself: TS5075.
    let diag = ts2345(
        r#"
declare function takesT<T extends string | number>(x: T): void;
function outer<T extends string | number>() {
    takesT<T>("value");
}
"#,
    );
    let note = diag
        .related_information
        .iter()
        .find(|r| r.code == COULD_BE_INSTANTIATED_DIFFERENT_SUBTYPE)
        .unwrap_or_else(|| panic!("expected TS5075; got: {:?}", diag.related_information));
    assert!(
        note.message_text.contains("string | number"),
        "note must name the actual constraint, got: {}",
        note.message_text
    );
}

#[test]
fn ts2345_call_argument_renamed_binder_still_emits_note() {
    // Anti-hardcoding: a different type-parameter spelling must still
    // produce the note (structural, not name-keyed).
    let diag = ts2345(
        r#"
declare function acceptsElem<Elem>(x: Elem): void;
function wrapper<Elem>() {
    acceptsElem<Elem>(true);
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
fn ts2345_call_argument_concrete_target_has_no_type_parameter_note() {
    // Control: an ordinary concrete-target argument mismatch (not a bare type
    // parameter) must not gain the type-parameter elaboration.
    let diag = ts2345(
        r#"
function take(x: string): void {}
take(5);
"#,
    );
    assert!(
        diag.related_information
            .iter()
            .all(|r| r.code != COULD_BE_INSTANTIATED_ARBITRARY
                && r.code != COULD_BE_INSTANTIATED_DIFFERENT_SUBTYPE),
        "concrete-target argument mismatch must not emit a type-param note, got: {:?}",
        diag.related_information
    );
}

#[test]
fn non_type_parameter_nested_mismatch_has_no_note() {
    // Control: a nested mismatch whose target is a concrete primitive (not a
    // bare type parameter) must NOT gain the type-parameter elaboration.
    let diag = ts2322(
        r#"
function make(): { field: { value: number } } {
    const built = { field: { value: "text" } };
    return built;
}
"#,
    );
    assert!(
        diag.related_information
            .iter()
            .all(|r| r.code != COULD_BE_INSTANTIATED_ARBITRARY
                && r.code != COULD_BE_INSTANTIATED_DIFFERENT_SUBTYPE),
        "concrete-target nested mismatch must not emit a type-param note, got: {:?}",
        diag.related_information
    );
}
