//! Tests for exact optional property type display in TS2375 diagnostic messages.
//!
//! When `exactOptionalPropertyTypes: true`, `foo?: T` means the property is
//! either absent or holds a value of type `T` — it does NOT implicitly include
//! `undefined`. Diagnostic messages must display the target type as `{ foo?: T }`
//! not `{ foo?: T | undefined }`.
//!
//! Conformance test: `strictOptionalProperties1.ts`
//! Root cause: `TypeFormatter` was appending `| undefined` to optional property
//! types even with `exactOptionalPropertyTypes: true`. Fixed by
//! `with_exact_optional_property_types(bool)` on `TypeFormatter`.

use crate::context::CheckerOptions;

fn check_with_options(source: &str, options: CheckerOptions) -> Vec<(u32, String)> {
    crate::test_utils::check_with_options(source, options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn check_strict_exact_optional(source: &str) -> Vec<(u32, String)> {
    check_with_options(
        source,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            exact_optional_property_types: true,
            ..Default::default()
        },
    )
}

fn check_strict_no_exact(source: &str) -> Vec<(u32, String)> {
    check_with_options(
        source,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            exact_optional_property_types: false,
            ..Default::default()
        },
    )
}

fn check_strict_exact_optional_no_unchecked(source: &str) -> Vec<(u32, String)> {
    check_with_options(
        source,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            exact_optional_property_types: true,
            no_unchecked_indexed_access: true,
            ..Default::default()
        },
    )
}

/// With `exactOptionalPropertyTypes: true`, assigning `{ foo: undefined }` to
/// `{ foo?: number }` should produce a TS2375 message that shows the target as
/// `{ foo?: number }`, not `{ foo?: number | undefined }`.
///
/// tsc: `Type '{ foo: undefined; }' is not assignable to type '{ foo?: number; }'
///       with 'exactOptionalPropertyTypes: true'. Consider adding 'undefined' to
///       the types of the target's properties.`
#[test]
fn ts2375_target_type_does_not_append_undefined_with_exact_optional() {
    let source = r#"
const x: { foo?: number } = { foo: undefined };
"#;
    let diags = check_strict_exact_optional(source);
    let ts2375: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2375).collect();
    assert!(
        !ts2375.is_empty(),
        "expected TS2375 for assigning undefined to an exact-optional property; got: {diags:?}"
    );
    let msg = &ts2375[0].1;
    assert!(
        msg.contains("foo?: number"),
        "TS2375 target type must display `foo?: number` (not `foo?: number | undefined`) with exactOptionalPropertyTypes. Got: {msg:?}"
    );
    assert!(
        !msg.contains("number | undefined"),
        "TS2375 must not append `| undefined` to optional property type when exactOptionalPropertyTypes is true. Got: {msg:?}"
    );
}

/// Without `exactOptionalPropertyTypes`, assigning `{ foo: undefined }` to
/// `{ foo?: number }` should NOT produce TS2375 (the property implicitly
/// includes undefined). Existing behavior must be preserved.
#[test]
fn ts2375_not_emitted_without_exact_optional_property_types() {
    let source = r#"
const x: { foo?: number } = { foo: undefined };
"#;
    let diags = check_strict_no_exact(source);
    let ts2375: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2375).collect();
    assert!(
        ts2375.is_empty(),
        "TS2375 must not be emitted without exactOptionalPropertyTypes. Got: {diags:?}"
    );
}

/// With `exactOptionalPropertyTypes: true`, a simple valid assignment must
/// not produce any diagnostic — ensures the fix doesn't cause false positives.
#[test]
fn no_false_positive_for_valid_optional_property_assignment() {
    let source = r#"
const x: { foo?: number } = { foo: 42 };
"#;
    let diags = check_strict_exact_optional(source);
    assert!(
        diags.is_empty(),
        "valid assignment to optional property must produce no diagnostics. Got: {diags:?}"
    );
}

/// With `exactOptionalPropertyTypes: true`, assigning `undefined` to a property
/// explicitly typed `string | undefined` (not just `string?`) must NOT produce
/// TS2375 — only explicitly-undefined-typed properties may hold undefined.
#[test]
fn ts2375_not_emitted_for_explicit_undefined_union_type() {
    let source = r#"
const x: { foo?: string | undefined } = { foo: undefined };
"#;
    let diags = check_strict_exact_optional(source);
    let ts2375: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2375).collect();
    assert!(
        ts2375.is_empty(),
        "TS2375 must not be emitted when the property type explicitly includes undefined. Got: {diags:?}"
    );
}

/// When an assignability failure involves a shared optional target property,
/// tsc uses TS2375 under `exactOptionalPropertyTypes` because the source-side
/// optional read can be `undefined` while the target optional slot excludes it.
/// This applies even when the immediate related-info property is a separate
/// required-property mismatch, as in `regexpExecAndMatchTypeUsages.ts`.
#[test]
fn ts2375_emitted_for_shared_optional_property_source_optional() {
    let source = r#"
interface A {
    required?: string;
    shared?: number;
}
interface B {
    required: string;
    shared?: number;
}
declare const a: A;
const b: B = a;
"#;
    let diags = check_strict_exact_optional(source);
    assert!(
        diags.iter().any(|(code, message)| {
            *code == 2375
                && message.contains("Type 'A' is not assignable to type 'B'")
                && message.contains("exactOptionalPropertyTypes")
        }),
        "expected TS2375 for shared exact-optional mismatch, got: {diags:#?}"
    );
}

/// The same required-property mismatch should stay TS2322 when the target's
/// shared optional property explicitly accepts `undefined`.
#[test]
fn shared_optional_explicit_undefined_keeps_ts2322() {
    let source = r#"
interface A {
    required?: string;
    shared?: number;
}
interface B {
    required: string;
    shared?: number | undefined;
}
declare const a: A;
const b: B = a;
"#;
    let diags = check_strict_exact_optional(source);
    assert!(
        diags.iter().any(|(code, _)| *code == 2322),
        "expected TS2322 for required-property mismatch, got: {diags:#?}"
    );
    assert!(
        diags.iter().all(|(code, _)| *code != 2375),
        "must not emit TS2375 when target optional property includes undefined, got: {diags:#?}"
    );
}

#[test]
fn identical_optional_properties_do_not_emit_ts2375() {
    let source = r#"
interface A {
    shared?: number;
}
interface B {
    shared?: number;
}
declare const a: A;
const b: B = a;
"#;
    let diags = check_strict_exact_optional(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2375),
        "identical optional properties are assignable and must not emit TS2375, got: {diags:#?}"
    );
}

// ── TS2375 nested relation-reason elaboration (#14830) ──────────────────────
//
// Structural rule: when an assignment-context `exactOptionalPropertyTypes`
// mismatch emits TS2375, tsc appends the same per-property relation
// elaboration the call-argument TS2379 path already shows — `Types of
// property 'X' are incompatible.` (TS2326) followed by `Type 'undefined' is
// not assignable to type '<base>'.` (TS2322). tsz previously dropped this tail
// on the variable-assignment and array-element paths. The assertions below
// vary the property spelling and the path (variable, nested object, array
// element) so a hardcoded-spelling fix would not pass.

fn ts2375_diag(source: &str) -> tsz_common::diagnostics::Diagnostic {
    crate::test_utils::check_with_options(
        source,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            exact_optional_property_types: true,
            ..Default::default()
        },
    )
    .into_iter()
    .find(|d| d.code == 2375)
    .expect("expected a TS2375 diagnostic")
}

/// True when `diag` carries the two-line per-property elaboration tail for
/// `prop`: `Types of property '<prop>' are incompatible.` (TS2326) and a
/// nested `Type 'undefined' is not assignable to type '<base>'.` (TS2322).
fn has_property_incompatible_elaboration(
    diag: &tsz_common::diagnostics::Diagnostic,
    prop: &str,
    base: &str,
) -> bool {
    let head = format!("Types of property '{prop}' are incompatible.");
    let leaf = format!("Type 'undefined' is not assignable to type '{base}'.");
    let has_head = diag
        .related_information
        .iter()
        .any(|info| info.code == 2326 && info.message_text == head);
    let has_leaf = diag
        .related_information
        .iter()
        .any(|info| info.code == 2322 && info.message_text == leaf);
    has_head && has_leaf
}

#[test]
fn ts2375_variable_assignment_carries_property_incompatible_elaboration() {
    let diag = ts2375_diag("interface Opt { a?: number; }\nconst o1: Opt = { a: undefined };\n");
    assert!(
        has_property_incompatible_elaboration(&diag, "a", "number"),
        "TS2375 on a variable assignment must carry the 'Types of property a are \
         incompatible / Type undefined is not assignable to number' tail; got {:?}",
        diag.related_information
    );
}

#[test]
fn ts2375_elaboration_is_property_name_independent() {
    // Rename the property to prove the elaboration is structural, not keyed on
    // the spelling 'a'.
    let diag = ts2375_diag(
        "interface Opt { greeting?: string; }\nconst o: Opt = { greeting: undefined };\n",
    );
    assert!(
        has_property_incompatible_elaboration(&diag, "greeting", "string"),
        "TS2375 elaboration must follow a renamed property; got {:?}",
        diag.related_information
    );
}

#[test]
fn ts2375_nested_object_property_elaboration_names_base_object_type() {
    // The nested optional object property: the leaf names the property's base
    // object type, matching tsc.
    let diag = ts2375_diag(
        "interface Nested { inner?: { x: number } }\nconst n: Nested = { inner: undefined };\n",
    );
    assert!(
        has_property_incompatible_elaboration(&diag, "inner", "{ x: number; }"),
        "nested-object TS2375 must elaborate to the base object type; got {:?}",
        diag.related_information
    );
}

#[test]
fn ts2375_array_element_carries_property_incompatible_elaboration() {
    // tsc drills array-literal assignments to the offending element, reporting
    // the element-level mismatch with the same property elaboration.
    let diag = ts2375_diag("interface O { a?: number; }\nconst arr: O[] = [{ a: undefined }];\n");
    assert!(
        has_property_incompatible_elaboration(&diag, "a", "number"),
        "array-element TS2375 must carry the property-incompatible tail; got {:?}",
        diag.related_information
    );
}

#[test]
fn element_access_names_optional_property_receiver_in_ts18048() {
    let source = r#"
declare const matchResult: { groups?: { [key: string]: string } };
matchResult.groups["someVariable"].length;
"#;
    let diags = check_strict_exact_optional_no_unchecked(source);
    assert!(
        diags.iter().any(|(code, message)| {
            *code == 18048 && message.contains("'matchResult.groups' is possibly 'undefined'")
        }),
        "expected TS18048 to name optional property receiver, got: {diags:#?}"
    );
    assert!(
        diags.iter().any(|(code, message)| {
            *code == 2532 && message.contains("Object is possibly 'undefined'")
        }),
        "expected TS2532 for noUncheckedIndexedAccess result before `.length`, got: {diags:#?}"
    );
}
