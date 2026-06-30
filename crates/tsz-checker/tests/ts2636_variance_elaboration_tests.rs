//! TS2636 (variance annotation) nested relation-reason elaboration.
//!
//! When an explicit `in`/`out` annotation contradicts the parameter's actual
//! usage, tsc does not emit a flat message: it runs the assignability relation
//! over the declaration body with marker substitutions for the annotated
//! parameter and attaches the relation's failure reason as the nested tail
//! (`The types returned by 'f()' are incompatible…`, `Types of property 'x'
//! are incompatible…`). tsz reproduces that tail through the shared
//! assignability gateway.
//!
//! The decision is unchanged (still gated by the computed variance), so these
//! tests assert the elaboration is *attached* to the existing TS2636, not that
//! the decision flips. Binder names are varied so no name-string drives the
//! check, and the synthetic marker `could be instantiated` notes (TS5082 /
//! TS5075) must be absent — tsc omits them for the variance-marker relation.

use tsz_checker::test_utils::check_source_diagnostics;
use tsz_common::diagnostics::Diagnostic;

const TS2636: u32 = 2636;
const TS5082_ARBITRARY_NOTE: u32 = 5082;
const TS5075_SUBTYPE_NOTE: u32 = 5075;

fn ts2636_with_related<'a>(diags: &'a [Diagnostic]) -> &'a Diagnostic {
    diags
        .iter()
        .find(|d| d.code == TS2636)
        .unwrap_or_else(|| panic!("expected a TS2636 diagnostic; got {diags:?}"))
}

fn related_texts(diag: &Diagnostic) -> Vec<&str> {
    diag.related_information
        .iter()
        .map(|info| info.message_text.as_str())
        .collect()
}

/// `in T` (contravariant) used in a covariant method-return position: the tail
/// drills through the offending method return down to the marker leaf.
#[test]
fn in_annotation_method_return_attaches_return_reason_tail() {
    let diags = check_source_diagnostics("interface Box<in Elem> { read(): Elem; }");
    let diag = ts2636_with_related(&diags);
    let texts = related_texts(diag);
    assert!(
        texts
            .iter()
            .any(|t| *t == "The types returned by 'read()' are incompatible between these types."),
        "expected the method-return frame in the TS2636 tail; got {texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|t| *t == "Type 'super-Elem' is not assignable to type 'sub-Elem'."),
        "expected the marker leaf in the TS2636 tail; got {texts:?}"
    );
}

/// `out T` (covariant) used in a contravariant function-property parameter: the
/// tail drills through the property, signature, and parameter frames.
#[test]
fn out_annotation_property_param_attaches_parameter_reason_tail() {
    let diags = check_source_diagnostics("type Sink<out Item> = { write: (x: Item) => void };");
    let diag = ts2636_with_related(&diags);
    let texts = related_texts(diag);
    assert!(
        texts
            .iter()
            .any(|t| *t == "Types of property 'write' are incompatible."),
        "expected the property frame in the TS2636 tail; got {texts:?}"
    );
    assert!(
        texts.iter().any(|t| *t
            == "Type '(x: sub-Item) => void' is not assignable to type '(x: super-Item) => void'."),
        "expected the signature frame in the TS2636 tail; got {texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|t| *t == "Type 'super-Item' is not assignable to type 'sub-Item'."),
        "expected the marker leaf in the TS2636 tail; got {texts:?}"
    );
}

/// The synthetic markers are never instantiated, so the bare-type-parameter
/// "could be instantiated…" notes (TS5082 / TS5075) must not leak into the
/// variance elaboration — matching tsc.
#[test]
fn variance_tail_omits_marker_instantiation_notes() {
    for source in [
        "interface Box<in Elem> { read(): Elem; }",
        "type Sink<out Item> = { write: (x: Item) => void };",
    ] {
        let diags = check_source_diagnostics(source);
        let diag = ts2636_with_related(&diags);
        assert!(
            diag.related_information
                .iter()
                .all(|info| info.code != TS5082_ARBITRARY_NOTE && info.code != TS5075_SUBTYPE_NOTE),
            "TS2636 tail must omit the synthetic-marker instantiation note for `{source}`; got {:?}",
            related_texts(diag)
        );
    }
}

/// The elaboration is added without changing the decision: a sound annotation
/// stays clean, and an unsound one still reports TS2636.
#[test]
fn elaboration_does_not_change_the_decision() {
    // Sound: `out Elem` used covariantly.
    let sound = check_source_diagnostics("interface Box<out Elem> { read(): Elem; }");
    assert!(
        !sound.iter().any(|d| d.code == TS2636),
        "a sound `out` annotation must stay clean; got {sound:?}"
    );

    // Unsound: `in Elem` used covariantly still flags.
    let unsound = check_source_diagnostics("interface Box<in Elem> { read(): Elem; }");
    assert!(
        unsound.iter().any(|d| d.code == TS2636),
        "an unsound `in` annotation must still emit TS2636; got {unsound:?}"
    );
}
