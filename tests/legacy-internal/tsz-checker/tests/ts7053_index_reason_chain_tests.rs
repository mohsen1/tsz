//! Regression tests for the nested reason `tsc` chains beneath a `TS7053`
//! ("Element implicitly has an 'any' type ... can't be used to index type 'X'")
//! element-access diagnostic.
//!
//! Structural rule (one sentence): `tsc`'s `getPropertyTypeForIndexType` renders
//! `TS7053` as a message chain whose nested reason is
//! `Property '<name>' does not exist on type '<T>'.` for a string/number literal
//! key, and `No index signature with a parameter of type '<kind>' was found on
//! type '<T>'.` for a general `string`/`number` key; `symbol`/`any`/template
//! keys carry no nested reason.
//!
//! Witness family: any element access that reports implicit-any under
//! `noImplicitAny` (e.g. the kysely `TS7053` long tail) previously emitted only
//! the bare head, dropping the reason line `tsc` renders.

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_diagnostics;

const TS7053: u32 = 7053;
const TS2339: u32 = 2339;
const TS7054: u32 = 7054;

/// The single `TS7053` diagnostic in `source`, or a panic listing what was found.
fn ts7053_diagnostic(source: &str) -> Diagnostic {
    let mut hits: Vec<Diagnostic> = check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == TS7053)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly one TS7053 diagnostic; got: {hits:?}",
    );
    hits.pop().unwrap()
}

/// The `(code, message)` of a diagnostic's single elaboration reason line, or
/// `None` when it carries no `related_information` entries.
fn only_reason(diag: &Diagnostic) -> Option<(u32, &str)> {
    match diag.related_information.as_slice() {
        [] => None,
        [reason] => Some((reason.code, reason.message_text.as_str())),
        many => panic!("expected at most one elaboration reason; got: {many:?}"),
    }
}

#[test]
fn string_literal_key_chains_property_does_not_exist() {
    let diag = ts7053_diagnostic("declare const o: { a: number }; const x = o[\"missing\"];");
    assert_eq!(
        only_reason(&diag),
        Some((
            TS2339,
            "Property 'missing' does not exist on type '{ a: number; }'.",
        )),
        "string-literal key must chain the TS2339 property reason",
    );
}

#[test]
fn number_literal_key_chains_property_does_not_exist() {
    let diag = ts7053_diagnostic("declare const o: { a: number }; const x = o[5];");
    assert_eq!(
        only_reason(&diag),
        Some((
            TS2339,
            "Property '5' does not exist on type '{ a: number; }'."
        )),
        "number-literal key must chain the TS2339 property reason with the JS number name",
    );
}

#[test]
fn general_string_key_chains_no_index_signature() {
    let diag = ts7053_diagnostic(
        "declare const o: { a: number }; declare const k: string; const x = o[k];",
    );
    assert_eq!(
        only_reason(&diag),
        Some((
            TS7054,
            "No index signature with a parameter of type 'string' was found on type '{ a: number; }'.",
        )),
        "general `string` key must chain the TS7054 no-index-signature reason",
    );
}

#[test]
fn general_number_key_chains_no_index_signature() {
    let diag = ts7053_diagnostic(
        "declare const o: { a: number }; declare const k: number; const x = o[k];",
    );
    assert_eq!(
        only_reason(&diag),
        Some((
            TS7054,
            "No index signature with a parameter of type 'number' was found on type '{ a: number; }'.",
        )),
        "general `number` key must chain the TS7054 no-index-signature reason",
    );
}

#[test]
fn symbol_key_carries_no_reason() {
    let diag = ts7053_diagnostic(
        "declare const o: { a: number }; declare const k: symbol; const x = o[k];",
    );
    assert_eq!(
        only_reason(&diag),
        None,
        "a `symbol` key emits the bare TS7053 head with no chained reason, matching tsc",
    );
}

/// The chained reason and the head must name the *same* receiver type. For a
/// template-literal index signature the key does not match, so `tsc` reports
/// `TS7053` on the whole named interface `T` (not a structural shape) in both
/// lines.
#[test]
fn template_index_signature_non_matching_key_names_the_interface() {
    let source = r#"
interface T { [k: `data-${string}`]: string; }
declare const t: T;
const bad = t["other"];
"#;
    let diag = ts7053_diagnostic(source);
    assert!(
        diag.message_text
            .contains("can't be used to index type 'T'"),
        "head must name the nominal interface `T`; got: {:?}",
        diag.message_text,
    );
    assert_eq!(
        only_reason(&diag),
        Some((TS2339, "Property 'other' does not exist on type 'T'.")),
        "the chained reason must name the same nominal type as the head",
    );
}

/// Write-context receiver-display: an annotated variable renders its declared
/// (annotation) type in both the head and the chained reason, not the widened
/// object-literal initializer type. Distinct receiver-display facet fixed
/// alongside the reason chain.
#[test]
fn annotated_initializer_receiver_uses_the_annotation() {
    let source = r#"
interface T { [k: `data-${string}`]: string; }
const t: T = { "data-x": "1" };
t["other"] = "3";
"#;
    let diag = ts7053_diagnostic(source);
    assert!(
        diag.message_text
            .contains("can't be used to index type 'T'")
            && !diag.message_text.contains("data-x"),
        "annotated receiver must display the annotation `T`, not the initializer shape; got: {:?}",
        diag.message_text,
    );
    assert_eq!(
        only_reason(&diag),
        Some((TS2339, "Property 'other' does not exist on type 'T'.")),
    );
}

/// Control: with *no* annotation the declared type is the widened initializer,
/// so the receiver still renders the anonymous object shape (the display guard
/// only suppresses the initializer shape when an annotation exists).
#[test]
fn unannotated_initializer_receiver_still_uses_the_literal_shape() {
    let diag = ts7053_diagnostic("const o = { a: 1 }; o[\"b\"] = 2;");
    assert!(
        diag.message_text
            .contains("can't be used to index type '{ a: number; }'"),
        "unannotated receiver keeps the inferred object-literal shape; got: {:?}",
        diag.message_text,
    );
    assert_eq!(
        only_reason(&diag),
        Some((
            TS2339,
            "Property 'b' does not exist on type '{ a: number; }'."
        )),
    );
}

/// Anti-hardcoding: the reason chain is derived from the type shape, not from
/// the user's binder or property spellings. Renaming every identifier keeps the
/// same chain shape (only the rendered names track the new spellings).
#[test]
fn reason_chain_is_binder_name_agnostic() {
    let renamed = ts7053_diagnostic(
        "declare const container: { alpha: number }; const picked = container[\"beta\"];",
    );
    assert_eq!(
        only_reason(&renamed),
        Some((
            TS2339,
            "Property 'beta' does not exist on type '{ alpha: number; }'.",
        )),
        "the reason must follow the renamed shape, proving it is structural not name-keyed",
    );
}
