//! Regression tests for the assignability property-chain elaboration shape.
//!
//! Structural rule: when an object-to-object assignment fails through a chain
//! of plain property mismatches, `tsc` renders the elaboration the same way
//! `flattenDiagnosticMessageText` does — with progressive (2-space-per-level)
//! indentation, and with a run of >= 2 consecutive property links collapsed
//! into a single `The types of 'a.b.c' are incompatible between these types.`
//! line. A single property link keeps the `Types of property 'X' are
//! incompatible.` form. tsz previously rendered the chain flat (every entry at
//! one indentation level) and never collapsed multi-level property paths, so
//! the chain structure — and therefore the root relation — was obscured.
//!
//! The chain depth is carried on each `DiagnosticRelatedInformation` so the CLI
//! reporter can indent each level; these tests assert the structural depths and
//! collapsed messages directly, independent of the reporter, and vary property
//! names so the collapse cannot be name-hardcoded.

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source_strict;

fn single_ts2322(source: &str) -> Diagnostic {
    let mut diags: Vec<Diagnostic> = check_source_strict(source)
        .into_iter()
        .filter(|diag| diag.code == 2322)
        .collect();
    assert_eq!(
        diags.len(),
        1,
        "expected exactly one TS2322 for `{source}`, got: {diags:?}"
    );
    diags.remove(0)
}

/// The leaf relation line of a chain must sit one elaboration level deeper than
/// the property header above it. A single failing property keeps the
/// `Types of property 'X' are incompatible.` header (no collapse) with the leaf
/// indented beneath it.
#[test]
fn single_property_mismatch_indents_leaf_one_level_deeper() {
    let diag = single_ts2322(
        "declare const src: { a: number; b: string };\n\
         const t: { a: number; b: number } = src;\n",
    );
    let related = &diag.related_information;
    assert_eq!(related.len(), 2, "got: {related:?}");
    assert!(
        related[0].message_text.contains("Types of property 'b'"),
        "header: {}",
        related[0].message_text
    );
    assert_eq!(related[0].depth, 0, "header is the first elaboration level");
    assert!(
        related[1]
            .message_text
            .contains("Type 'string' is not assignable to type 'number'"),
        "leaf: {}",
        related[1].message_text
    );
    assert_eq!(
        related[1].depth, 1,
        "leaf must be one level deeper than its header"
    );
}

/// A run of >= 2 plain property links collapses into a single dotted-path line,
/// with the leaf relation one level deeper. Verified with two distinct name
/// choices so the rule is structural, not a spelling match.
#[test]
fn nested_property_chain_collapses_to_dotted_path() {
    for (source, dotted, leaf) in [
        (
            "declare const src: { a: { b: { c: string } } };\n\
             const t: { a: { b: { c: number } } } = src;\n",
            "'a.b.c'",
            "Type 'string' is not assignable to type 'number'",
        ),
        (
            "declare const src: { alpha: { beta: { gamma: boolean } } };\n\
             const t: { alpha: { beta: { gamma: string } } } = src;\n",
            "'alpha.beta.gamma'",
            "Type 'boolean' is not assignable to type 'string'",
        ),
    ] {
        let diag = single_ts2322(source);
        let related = &diag.related_information;
        assert_eq!(
            related.len(),
            2,
            "collapsed chain for `{source}`: {related:?}"
        );
        assert!(
            related[0].message_text.contains("The types of")
                && related[0].message_text.contains(dotted)
                && related[0]
                    .message_text
                    .contains("are incompatible between these types"),
            "collapsed header: {}",
            related[0].message_text
        );
        assert_eq!(related[0].depth, 0);
        assert!(
            related[1].message_text.contains(leaf),
            "leaf: {}",
            related[1].message_text
        );
        assert_eq!(related[1].depth, 1, "leaf one level under collapsed header");
    }
}

/// A two-level chain collapses (the threshold is >= 2 links), independent of the
/// chosen property names.
#[test]
fn two_level_property_chain_collapses() {
    let diag = single_ts2322(
        "declare const src: { outer: { inner: boolean } };\n\
         const t: { outer: { inner: string } } = src;\n",
    );
    let related = &diag.related_information;
    assert_eq!(related.len(), 2, "got: {related:?}");
    assert!(
        related[0].message_text.contains("'outer.inner'")
            && related[0]
                .message_text
                .contains("are incompatible between these types"),
        "collapsed header: {}",
        related[0].message_text
    );
    assert_eq!(related[0].depth, 0);
    assert_eq!(related[1].depth, 1);
}

/// Negative/fallback case: a property whose value types are a generic
/// application (`Box<string>` vs `Box<number>`) keeps the application boundary
/// visible rather than being folded into a dotted property path. The collapse
/// must stop at — and not absorb — the application-typed property. Uses a
/// user-defined generic so the test does not depend on lib types.
#[test]
fn generic_application_property_is_not_collapsed() {
    let diag = single_ts2322(
        "interface Box<T> { value: T; }\n\
         declare const src: { m: Box<string> };\n\
         const t: { m: Box<number> } = src;\n",
    );
    let related = &diag.related_information;
    assert!(
        !related.is_empty(),
        "expected an elaboration chain, got none"
    );
    // The header is the single-property form for `m`, never a dotted
    // `'m.value'` collapse across the `Box<_>` application boundary.
    assert!(
        related[0].message_text.contains("Types of property 'm'"),
        "header must stay the single-property form: {}",
        related[0].message_text
    );
    assert!(
        !related[0].message_text.contains("'m.value'"),
        "application-typed property must not be folded into a dotted path: {related:?}"
    );
}
