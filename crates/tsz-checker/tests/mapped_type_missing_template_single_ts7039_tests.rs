//! Regression coverage for TS7039 ("Mapped object type implicitly has an
//! 'any' template type.") cardinality.
//!
//! A mapped type that omits its template type under `noImplicitAny` must emit
//! exactly one TS7039, anchored on the mapped-type node, regardless of the
//! syntactic position the mapped type appears in (top-level alias body,
//! object/interface member, conditional-nested, or with an `as` name clause).
//!
//! Previously the type-alias missing-name post-walk and the member missing-name
//! walk each contained two emission sites for this diagnostic, and only the
//! `(start, code)` diagnostic dedup collapsed the redundant pair at runtime.
//! That masking was span-coincidence-dependent and fragile. The emission is now
//! owned by a single helper (`report_mapped_type_missing_template`), so exactly
//! one diagnostic is produced for structural reasons rather than by dedup. This
//! test pins the cardinality and span so the collapse cannot silently regress.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_with_options;

fn no_implicit_any_options() -> CheckerOptions {
    CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    }
}

fn ts7039(source: &str) -> Vec<Diagnostic> {
    check_with_options(source, no_implicit_any_options())
        .into_iter()
        .filter(|diag| diag.code == 7039)
        .collect()
}

/// The mapped-type node text the diagnostic should anchor on, so a span
/// regression is caught alongside the cardinality regression.
fn anchor_text<'a>(source: &'a str, diagnostic: &Diagnostic) -> &'a str {
    let start = diagnostic.start as usize;
    let end = start.saturating_add(diagnostic.length as usize);
    source
        .get(start..end)
        .expect("TS7039 span must land on char boundaries within the source")
}

#[test]
fn top_level_alias_mapped_type_without_template_emits_single_ts7039() {
    // Binder names deliberately non-canonical (`Container`/`Slot`) so the rule
    // is structural and not keyed to any identifier text.
    let source = "type Container<Slot> = { [Member in keyof Slot] };";
    let diags = ts7039(source);
    assert_eq!(
        diags.len(),
        1,
        "expected exactly one TS7039 for a bare mapped-type alias body, got: {diags:#?}",
    );
    assert!(
        anchor_text(source, &diags[0]).starts_with('{'),
        "TS7039 should anchor on the mapped-type node, got {:?}",
        anchor_text(source, &diags[0]),
    );
}

#[test]
fn interface_member_mapped_type_without_template_emits_single_ts7039() {
    let source = "\
interface Wrapper<Payload> {
    field: { [Slot in keyof Payload] };
}";
    let diags = ts7039(source);
    assert_eq!(
        diags.len(),
        1,
        "expected exactly one TS7039 for a mapped type in interface-member position, got: {diags:#?}",
    );
}

#[test]
fn type_literal_member_mapped_type_without_template_emits_single_ts7039() {
    let source = "type Holder<Source> = { inner: { [Element in keyof Source] } };";
    let diags = ts7039(source);
    assert_eq!(
        diags.len(),
        1,
        "expected exactly one TS7039 for a mapped type nested in a type-literal member, got: {diags:#?}",
    );
}

#[test]
fn conditional_nested_mapped_type_without_template_emits_single_ts7039() {
    let source = "type Branch<Input> = Input extends object ? { [Field in keyof Input] } : never;";
    let diags = ts7039(source);
    assert_eq!(
        diags.len(),
        1,
        "expected exactly one TS7039 for a mapped type nested in a conditional branch, got: {diags:#?}",
    );
}

#[test]
fn as_clause_mapped_type_without_template_emits_single_ts7039() {
    let source = "type Renamed<Original> = { [Key in keyof Original as `p${string & Key}`] };";
    let diags = ts7039(source);
    assert_eq!(
        diags.len(),
        1,
        "expected exactly one TS7039 for a remapped (`as`-clause) mapped type without a template, got: {diags:#?}",
    );
}

#[test]
fn mapped_type_with_template_emits_no_ts7039() {
    // Negative control: a fully-formed mapped type must not trip the diagnostic.
    let source = "type Identity<Source> = { [Key in keyof Source]: Source[Key] };";
    let diags = ts7039(source);
    assert!(
        diags.is_empty(),
        "a mapped type with a template type must not emit TS7039, got: {diags:#?}",
    );
}

#[test]
fn mapped_type_without_template_is_silent_when_no_implicit_any_is_disabled() {
    let source = "type Result<Input> = ({[Slot in keyof Input]}) extends ({[key in Slot]: Input[Slot]}) ? number : never;";
    let diagnostics = check_with_options(
        source,
        CheckerOptions {
            strict: false,
            no_implicit_any: false,
            ..CheckerOptions::default()
        },
    );
    let relevant_codes: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| matches!(diagnostic.code, 2304 | 7039))
        .map(|diagnostic| diagnostic.code)
        .collect();

    assert_eq!(
        relevant_codes,
        vec![2304, 2304],
        "without noImplicitAny, the malformed mapped type should retain only the two unresolved-name diagnostics: {diagnostics:#?}",
    );
}
