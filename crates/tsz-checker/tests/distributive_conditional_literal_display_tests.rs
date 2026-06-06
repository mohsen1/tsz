//! Literal preservation when a non-fresh object reaches display widening
//! (issue #10815: distributive conditional over tuple-like unions).
//!
//! Structural rule: when tsc renders an object type for a diagnostic, it only
//! widens literal property types of *fresh* object literals (types carrying the
//! widening flag in `getWidenedType`). A declared or computed object — including
//! every member produced by distributing a conditional over a union — keeps its
//! literal property types (`{ kind: "other" }`), it is NOT widened to the
//! primitive (`{ kind: string }`).
//!
//! tsz previously routed the displayed type through `widen_type_for_display`,
//! which widened literal properties of *every* object regardless of freshness,
//! so a distributed union member lost per-variant precision in the
//! `Property 'X' is missing in type 'S' but required in type 'T'` elaboration.
//! The display-widening path now respects freshness, matching tsc.
//!
//! Tests vary the alias, type-parameter, property, and variant-tag spellings so
//! a fix keyed to a particular identifier would not satisfy them.

use tsz_checker::test_utils::check_source_diagnostics;
use tsz_common::diagnostics::Diagnostic;

/// All message text carried by a diagnostic: the headline plus every nested
/// related-information line.
fn all_text(diag: &Diagnostic) -> String {
    let mut text = diag.message_text.clone();
    for info in &diag.related_information {
        text.push('\n');
        text.push_str(&info.message_text);
    }
    text
}

/// True when some TS2322 mentions `needle` anywhere in its message chain.
fn ts2322_mentions(diags: &[Diagnostic], needle: &str) -> bool {
    diags
        .iter()
        .filter(|d| d.code == 2322)
        .any(|d| all_text(d).contains(needle))
}

#[test]
fn distributed_tuple_union_member_keeps_literal_tag() {
    // `Classify<U>` distributes over the tuple union; each member is a
    // non-fresh object `{ tag: "other"; value: <tuple> }`. The missing-property
    // elaboration must show the literal tag `"other"`, never `string`.
    let source = r#"
        type Classify<T> = T extends unknown
            ? T extends string
                ? { tag: "string"; value: T }
                : T extends number
                    ? { tag: "number"; value: T }
                    : { tag: "other"; value: T }
            : never;
        type Variants = [string, string] | [number, number] | [];
        type Result = Classify<Variants>;
        declare const r: Result;
        const sink: { extra: boolean } = r;
    "#;
    let diags = check_source_diagnostics(source);
    assert!(
        ts2322_mentions(&diags, r#"tag: "other""#),
        "expected the distributed member to keep its literal tag, got: {:#?}",
        diags.iter().map(all_text).collect::<Vec<_>>()
    );
    assert!(
        !ts2322_mentions(&diags, "tag: string"),
        "literal tag must not be widened to `string`, got: {:#?}",
        diags.iter().map(all_text).collect::<Vec<_>>()
    );
}

#[test]
fn plain_union_member_keeps_literal_property() {
    // Same precision rule without any conditional: a declared union of
    // non-fresh objects keeps each member's literal property in the
    // missing-property elaboration. Names differ from the test above.
    let source = r#"
        declare const value:
            | { label: "alpha"; payload: [] }
            | { label: "beta"; payload: [string, string] };
        const out: { marker: number } = value;
    "#;
    let diags = check_source_diagnostics(source);
    assert!(
        ts2322_mentions(&diags, r#"label: "alpha""#) || ts2322_mentions(&diags, r#"label: "beta""#),
        "expected a union member rendered with its literal label, got: {:#?}",
        diags.iter().map(all_text).collect::<Vec<_>>()
    );
    assert!(
        !ts2322_mentions(&diags, "label: string"),
        "literal label must not be widened to `string`, got: {:#?}",
        diags.iter().map(all_text).collect::<Vec<_>>()
    );
}
