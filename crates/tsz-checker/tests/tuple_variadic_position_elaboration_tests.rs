//! Variadic/rest tuple element-mismatch elaboration parity with `tsc`.
//!
//! When a tuple-to-tuple relation fails inside the region that aligns to a
//! target **rest** element, `tsc` reports the source *span* of positions
//! against the rest slot's single target position:
//!   - multi-element span -> `Type at positions <start> through <end> in source
//!     is not compatible with type at position <target> in target.` (`TS2627`)
//!   - single-element span -> `Type at position <start> in source is not
//!     compatible with type at position <target> in target.` (`TS2626`)
//!
//! Before this elaboration, the solver collapsed such a failure into a single
//! `index` and the renderer printed `position <index>` for *both* sides, which
//! is correct only for a fixed element (source position == target position).
//!
//! Binder/type-parameter names are varied across cases so the rendering is
//! proven structural, not keyed on a fixture identifier.

use tsz_checker::context::CheckerOptions;
use tsz_common::diagnostics::Diagnostic;

fn check_strict(source: &str) -> Vec<Diagnostic> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
}

fn one(diags: &[Diagnostic], code: u32) -> &Diagnostic {
    let matches: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one TS{code}, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    matches[0]
}

fn related_text(diag: &Diagnostic, code: u32) -> &str {
    diag.related_information
        .iter()
        .find(|r| r.code == code)
        .unwrap_or_else(|| {
            panic!(
                "expected related info TS{code}; related: {:?}",
                diag.related_information
                    .iter()
                    .map(|r| (r.code, &r.message_text))
                    .collect::<Vec<_>>()
            )
        })
        .message_text
        .as_str()
}

/// Multi-element source span aligned to a trailing rest slot -> plural `TS2627`,
/// with the failing element relation nested beneath. The target position (`1`,
/// the rest slot) differs from the source span end (`2`).
#[test]
fn assignment_multi_element_span_uses_plural_positions() {
    let source = r#"
type Src = [string, number, string];
const dst: [string, ...number[]] = (null as unknown as Src);
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2322);
    assert_eq!(
        related_text(diag, 2627),
        "Type at positions 1 through 2 in source is not compatible with type at position 1 in target."
    );
    assert!(
        diag.related_information
            .iter()
            .any(|r| r.message_text == "Type 'string' is not assignable to type 'number'."),
        "failing element relation must nest beneath the positional line; related: {:?}",
        diag.related_information
            .iter()
            .map(|r| (r.code, &r.message_text))
            .collect::<Vec<_>>()
    );
    // The collapsed single-position form must NOT be emitted.
    assert!(
        !diag.related_information.iter().any(|r| r.code == 2626),
        "a multi-element span must not emit the singular TS2626 line"
    );
}

/// A single-element source span aligned to the rest slot uses the singular
/// `TS2626` form (`position N`), not the plural `positions N through M`.
#[test]
fn assignment_single_element_span_uses_singular_position() {
    let source = r#"
type Pair = [string, string];
const out: [string, ...number[]] = (null as unknown as Pair);
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2322);
    assert_eq!(
        related_text(diag, 2626),
        "Type at position 1 in source is not compatible with type at position 1 in target."
    );
    assert!(
        !diag.related_information.iter().any(|r| r.code == 2627),
        "a single-element span must not emit the plural TS2627 line"
    );
}

/// A leading fixed element + trailing fixed element around the rest: the failure
/// is in the rest-aligned middle, so the span excludes the leading and trailing
/// fixed slots and the target position is the rest index `1`.
#[test]
fn span_excludes_leading_and_trailing_fixed_slots() {
    let source = r#"
type Wide = [boolean, string, number, string];
const narrow: [boolean, ...number[], string] = (null as unknown as Wide);
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2322);
    assert_eq!(
        related_text(diag, 2627),
        "Type at positions 1 through 2 in source is not compatible with type at position 1 in target."
    );
}

/// Call-argument context: a typed tuple argument failing in the rest region must
/// surface the argument-level `TS2345` with the positional sub-message, not an
/// element-level `TS2322`.
#[test]
fn call_argument_typed_tuple_defers_to_positional_message() {
    let source = r#"
declare function consume(items: [string, ...number[]]): void;
const payload: [string, number, string] = (null as unknown as [string, number, string]);
consume(payload);
"#;
    let diags = check_strict(source);
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "rest-region call-argument mismatch must not drill into an element TS2322; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    let diag = one(&diags, 2345);
    assert_eq!(
        related_text(diag, 2627),
        "Type at positions 1 through 2 in source is not compatible with type at position 1 in target."
    );
}

/// A nested object-property mismatch inside the rest region keeps the positional
/// line and then drills into the property failure.
#[test]
fn span_drills_into_nested_property_failure() {
    let source = r#"
type Box = [string, { id: string }];
const sink: [string, ...{ id: number }[]] = (null as unknown as Box);
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2322);
    assert_eq!(
        related_text(diag, 2626),
        "Type at position 1 in source is not compatible with type at position 1 in target."
    );
    assert!(
        diag.related_information
            .iter()
            .any(|r| r.message_text == "Type 'string' is not assignable to type 'number'."),
        "nested property leaf must be rendered; related: {:?}",
        diag.related_information
            .iter()
            .map(|r| (r.code, &r.message_text))
            .collect::<Vec<_>>()
    );
}

/// A variadic-span mismatch nested inside a single-element tuple must get the
/// element-type header before its positional line (the single-element wrapper
/// omits its own positional line), so the variant is classified like a fixed
/// element by the header gate.
#[test]
fn nested_inside_single_element_tuple_emits_element_header() {
    let source = r#"
type Inner = [string, number, string];
const wrapped: [Inner] = [(null as unknown as Inner)];
const reshaped: [[string, ...number[]]] = wrapped;
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2322);
    // Single-element wrapper relates the element types directly (no position
    // line for the outer), then the inner variadic span line appears.
    assert!(
        diag.related_information.iter().any(|r| r.code == 2627
            && r.message_text
                == "Type at positions 1 through 2 in source is not compatible with type at position 1 in target."),
        "inner variadic span line must be present; related: {:?}",
        diag.related_information
            .iter()
            .map(|r| (r.code, &r.message_text))
            .collect::<Vec<_>>()
    );
    assert!(
        diag.related_information.iter().any(|r| r.code == 2322
            && r.message_text == "Type 'Inner' is not assignable to type '[string, ...number[]]'."),
        "the inner element-type header must be emitted before the span line; related: {:?}",
        diag.related_information
            .iter()
            .map(|r| (r.code, &r.message_text))
            .collect::<Vec<_>>()
    );
}

/// A union source whose failing member fails inside a rest region must keep the
/// union context: the member header line (`Type '<member>' is not assignable to
/// type '<target>'.`) precedes the positional span line. This guards the
/// union-source self-heading classifier, which must treat the variadic-span
/// reason exactly like a fixed-element one.
#[test]
fn union_source_member_keeps_header_above_span() {
    let source = r#"
type Variants = [string, number, string] | [boolean, boolean];
const collapsed: [string, ...number[]] = (null as unknown as Variants);
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2322);
    assert!(
        diag.related_information.iter().any(|r| r.code == 2322
            && r.message_text
                == "Type '[string, number, string]' is not assignable to type '[string, ...number[]]'."),
        "the failing union member header must precede the span line; related: {:?}",
        diag.related_information
            .iter()
            .map(|r| (r.code, &r.message_text))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        related_text(diag, 2627),
        "Type at positions 1 through 2 in source is not compatible with type at position 1 in target."
    );
}

/// Anti-regression: a leading fixed element mismatch (outside any rest region)
/// must keep the fixed-element rendering — `position 0` on both sides — and must
/// NOT be reported as a variadic span.
#[test]
fn leading_fixed_element_mismatch_is_not_a_span() {
    let source = r#"
type Lead = [number, ...string[]];
const want: [string, ...string[]] = (null as unknown as Lead);
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2322);
    assert!(
        !diag.related_information.iter().any(|r| r.code == 2627),
        "a leading fixed element mismatch must not emit the plural span line; related: {:?}",
        diag.related_information
            .iter()
            .map(|r| (r.code, &r.message_text))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        related_text(diag, 2626),
        "Type at position 0 in source is not compatible with type at position 0 in target."
    );
}
