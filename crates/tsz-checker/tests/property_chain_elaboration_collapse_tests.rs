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

/// A homomorphic mapped-type alias application (`Id<{ p: number }>` vs
/// `Id<{ p: string }>`) is NOT a nominal generic reference: `tsc` elaborates it
/// structurally (`Types of property 'p' are incompatible.` + leaf) rather than
/// collapsing to a single covariant type-argument line the way `Box<number>` vs
/// `Box<string>` does. tsz previously treated the mapped application like a
/// nominal generic and dropped the property header, leaving only the bare leaf.
/// Property names vary so the rule is structural, not a spelling match.
#[test]
fn single_level_mapped_alias_application_keeps_property_header() {
    for (prop, leaf) in [
        ("value", "Type 'number' is not assignable to type 'string'"),
        (
            "payload",
            "Type 'number' is not assignable to type 'string'",
        ),
    ] {
        let source = format!(
            "type Id<T> = {{ [K in keyof T]: Id<T[K]> }};\n\
             type S = Id<{{ {prop}: number }}>;\n\
             type D = Id<{{ {prop}: string }}>;\n\
             declare const src: S;\n\
             const t: D = src;\n"
        );
        let diag = single_ts2322(&source);
        let related = &diag.related_information;
        assert_eq!(related.len(), 2, "mapped chain for `{source}`: {related:?}");
        assert!(
            related[0]
                .message_text
                .contains(&format!("Types of property '{prop}'")),
            "mapped-alias property header must be present: {}",
            related[0].message_text
        );
        assert_eq!(related[0].depth, 0);
        assert!(
            related[1].message_text.contains(leaf),
            "leaf: {}",
            related[1].message_text
        );
        assert_eq!(related[1].depth, 1, "leaf one level under property header");
    }
}

/// A nested homomorphic mapped-type alias collapses its property run into a
/// single dotted path exactly like a plain nested object does — the mapped
/// applications at each level must not stop the collapse. tsz previously
/// truncated the chain at the first property (`Types of property 'a'`) with no
/// leaf, losing the root mismatch. Names vary so the collapse is structural.
#[test]
fn nested_mapped_alias_application_collapses_to_dotted_path() {
    for (a, b, c) in [("a", "b", "c"), ("one", "two", "three")] {
        let source = format!(
            "type Id<T> = {{ [K in keyof T]: Id<T[K]> }};\n\
             type S = Id<{{ {a}: {{ {b}: {{ {c}: number }} }} }}>;\n\
             type D = Id<{{ {a}: {{ {b}: {{ {c}: string }} }} }}>;\n\
             declare const src: S;\n\
             const t: D = src;\n"
        );
        let diag = single_ts2322(&source);
        let related = &diag.related_information;
        assert_eq!(
            related.len(),
            2,
            "collapsed chain for `{source}`: {related:?}"
        );
        let dotted = format!("'{a}.{b}.{c}'");
        assert!(
            related[0].message_text.contains("The types of")
                && related[0].message_text.contains(&dotted)
                && related[0]
                    .message_text
                    .contains("are incompatible between these types"),
            "collapsed header: {}",
            related[0].message_text
        );
        assert_eq!(related[0].depth, 0);
        assert!(
            related[1]
                .message_text
                .contains("Type 'number' is not assignable to type 'string'"),
            "leaf: {}",
            related[1].message_text
        );
        assert_eq!(related[1].depth, 1, "leaf one level under collapsed header");
    }
}

/// A non-recursive mapped alias with explicit modifiers (`+readonly`/`+?`) is
/// also structural: a nested mismatch collapses into a dotted path. Guards that
/// the fix keys on the mapped alias *body*, not a specific user-defined `Id`
/// spelling, and that the modifier-bearing form still elaborates structurally.
#[test]
fn modifier_mapped_alias_collapses_to_dotted_path() {
    let diag = single_ts2322(
        "type RO<T> = { +readonly [K in keyof T]: RO<T[K]> };\n\
         type S = RO<{ outer: { inner: number } }>;\n\
         type D = RO<{ outer: { inner: string } }>;\n\
         declare const src: S;\n\
         const t: D = src;\n",
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
    assert!(
        related[1]
            .message_text
            .contains("Type 'number' is not assignable to type 'string'"),
        "leaf: {}",
        related[1].message_text
    );
    assert_eq!(related[1].depth, 1);
}
