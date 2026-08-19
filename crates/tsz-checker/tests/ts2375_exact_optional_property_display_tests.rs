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
use crate::diagnostics::Diagnostic;

fn check_with_options(source: &str, options: CheckerOptions) -> Vec<(u32, String)> {
    crate::test_utils::check_with_options(source, options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

/// Full diagnostics (including `related_information`) under strict +
/// `exactOptionalPropertyTypes`. The `(code, message_text)` projection used by
/// most tests is derived from this so the option set lives in one place.
fn full_strict_exact_optional(source: &str) -> Vec<Diagnostic> {
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
}

fn check_strict_exact_optional(source: &str) -> Vec<(u32, String)> {
    full_strict_exact_optional(source)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
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

// ── TS2375 nested elaboration on the assignment / array-element paths ─────────
//
// tsc attaches the `Types of property 'X' are incompatible. / Type 'S' is not
// assignable to type 'T'.` relation-reason chain beneath the TS2375 top line,
// exactly as it does for the sibling TS2379 argument path. The assignment and
// array-element contexts previously dropped that elaboration (flat single line).
//
// These assert the chain structurally (depth, code, leaf relation) and vary the
// binder spellings / property positions so a fix keyed to a specific name would
// not pass.

/// Collect the nested elaboration of the first TS2375 as `(depth, code, text)`.
fn ts2375_chain(diags: &[Diagnostic]) -> Vec<(u8, u32, String)> {
    diags
        .iter()
        .find(|d| d.code == 2375)
        .map(|d| {
            d.related_information
                .iter()
                .map(|r| (r.depth, r.code, r.message_text.clone()))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn ts2375_variable_assignment_carries_property_incompatible_elaboration() {
    // Vary the optional property's name and primitive type to prove the chain is
    // structural, not keyed on a spelling.
    for (prop, ty) in [("a", "number"), ("value", "string"), ("flag", "boolean")] {
        let source = format!(
            "interface Opt {{ {prop}?: {ty}; }}\nconst o: Opt = {{ {prop}: undefined }};\n"
        );
        let diags = full_strict_exact_optional(&source);
        let chain = ts2375_chain(&diags);
        assert!(
            chain.iter().any(|(depth, code, msg)| *depth == 0
                && *code == 2326
                && msg.contains(&format!("Types of property '{prop}' are incompatible"))),
            "expected `Types of property '{prop}' are incompatible.` header at depth 0; got {chain:?}"
        );
        assert!(
            chain.iter().any(|(depth, _, msg)| *depth == 1
                && msg.contains("Type 'undefined' is not assignable to type")
                && msg.contains(&format!("'{ty}'"))),
            "expected the `undefined`→`{ty}` leaf relation at depth 1; got {chain:?}"
        );
    }
}

#[test]
fn ts2375_array_element_assignment_carries_elaboration() {
    let source = r#"
interface O { a?: number; }
const arr: O[] = [{ a: undefined }];
"#;
    let chain = ts2375_chain(&full_strict_exact_optional(source));
    assert!(
        chain.iter().any(|(depth, code, msg)| *depth == 0
            && *code == 2326
            && msg.contains("Types of property 'a' are incompatible")),
        "array-element TS2375 must carry the property-incompatibility header; got {chain:?}"
    );
    assert!(
        chain.iter().any(|(depth, _, msg)| *depth == 1
            && msg.contains("Type 'undefined' is not assignable to type 'number'")),
        "array-element TS2375 must carry the leaf relation; got {chain:?}"
    );
}

#[test]
fn ts2375_nested_optional_object_property_carries_elaboration() {
    let source = r#"
interface Nested { inner?: { x: number } }
const n: Nested = { inner: undefined };
"#;
    let chain = ts2375_chain(&full_strict_exact_optional(source));
    assert!(
        chain.iter().any(|(depth, _, msg)| *depth == 0
            && msg.contains("Types of property 'inner' are incompatible")),
        "nested-object TS2375 must name the offending property; got {chain:?}"
    );
    assert!(
        chain.iter().any(|(depth, _, msg)| *depth == 1
            && msg.contains("Type 'undefined' is not assignable to type '{ x: number; }'")),
        "nested-object TS2375 must drill to the object leaf type; got {chain:?}"
    );
}
