//! Regression test for the JSX excess-attribute TS2322 source-type display
//! when a spread attribute precedes the offending explicit attribute.
//!
//! Background: in `<AnotherComponent {...props} Property1/>` where
//! `Property1` is an excess attr, tsc renders the source object as the
//! merged shape `{ Property1: true; property1: string; property2: number; }`
//! — including the spread-merged properties. tsz previously rendered
//! only `{ Property1: true; }` because the excess-property emit path at
//! `crates/tsz-checker/src/checkers/jsx/props/resolution.rs` (the
//! `has_string_index` excess-property branch) hardcoded the source string
//! to the offending attr alone, ignoring everything already pushed into
//! `provided_attrs` from prior spreads.
//!
//! Conformance: `conformance/jsx/tsxSpreadAttributesResolution14.tsx` flips
//! FAIL → PASS with this fix.

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

#[test]
fn jsx_excess_attr_after_spread_includes_spread_props_in_message() {
    let diags = check_source(
        r#"
declare namespace JSX {
    interface IntrinsicAttributes {}
    interface IntrinsicElements {}
    interface ElementAttributesProperty { props: {}; }
    interface ElementChildrenAttribute {}
}

interface ComponentProps {
    property1: string;
    property2: number;
}

interface AnotherComponentProps {
    property1: string;
}

declare const props: ComponentProps;
declare function AnotherComponent(p: AnotherComponentProps): null;

const _x = <AnotherComponent {...props} Property1/>;
"#,
        "test.tsx",
        CheckerOptions::default(),
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one TS2322. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );

    let msg = &ts2322[0].message_text;
    assert!(
        msg.contains("property1: string") && msg.contains("property2: number"),
        "Source-type display must include spread-merged props (property1, property2). Got: {msg:?}"
    );
    assert!(
        msg.contains("Property1: true"),
        "Source-type display must still include the offending attribute Property1. Got: {msg:?}"
    );
}

/// Regression: the synthesized JSX-attributes source-type display for a
/// spread + excess attr (TS2322) must include the spread's literal
/// properties with their value types, not just the offending attribute.
///
/// Pairs with the deterministic invariant in
/// `tsz-solver`'s `shape_properties_atom_sorted_yet_recover_source_order_via_declaration_order`,
/// which guarantees that sorting `shape.properties` by `declaration_order`
/// recovers source order. The JSX synthesized-source path in
/// `crates/tsz-checker/src/checkers/jsx/props/resolution.rs` uses that
/// sort to mirror tsc.
///
/// Conformance: `conformance/jsx/tsxSpreadAttributesResolution2.tsx` flips
/// fingerprint-only failures from `{ Z; y; x; }` to `{ Z; x; y; }` for the
/// 26:40 anchor.
#[test]
fn jsx_excess_attr_after_spread_preserves_spread_source_order() {
    let diags = check_source(
        r#"
declare namespace JSX {
    interface IntrinsicAttributes {}
    interface IntrinsicElements {}
    interface ElementAttributesProperty { props: {}; }
    interface ElementChildrenAttribute {}
}

interface Target { x: string; y: "2"; }
declare function F(p: Target): null;

const _x = <F {...{x: 5, y: "2"}} Z="hi" />;
"#,
        "test.tsx",
        CheckerOptions::default(),
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "Expected at least one TS2322 for the excess Z attribute. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );

    let synthesized = ts2322
        .iter()
        .map(|d| &d.message_text)
        .find(|m| m.contains("Z: string"))
        .unwrap_or_else(|| {
            panic!(
                "Expected a TS2322 with synthesized source-type containing 'Z: string'. Got: {:?}",
                ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
            );
        });

    // Both spread-derived properties must appear with their value types.
    assert!(
        synthesized.contains("x: number"),
        "synthesized source-type display must include the spread's x: number; got: {synthesized:?}"
    );
    assert!(
        synthesized.contains("y: \"2\""),
        "synthesized source-type display must include the spread's y: \"2\"; got: {synthesized:?}"
    );

    // The explicit attribute Z="hi" still leads the synthesized literal —
    // tsc emits explicit attrs (in source order) before spread-derived props.
    let z_pos = synthesized
        .find("Z: string")
        .expect("Z: string must be in the synthesized literal");
    let x_pos = synthesized
        .find("x: number")
        .expect("x: number must be in the synthesized literal");
    let y_pos = synthesized
        .find("y: \"2\"")
        .expect("y: \"2\" must be in the synthesized literal");
    assert!(
        z_pos < x_pos && z_pos < y_pos,
        "Explicit attribute Z must precede spread-derived props; got: {synthesized:?}"
    );
}

/// Regression: the JSX spread missing-properties list (TS2739/TS2740/TS2741)
/// must include each required property of the target props type when a
/// spread of `{}` provides nothing.
///
/// Pairs with the deterministic invariant in `tsz-solver`'s
/// `shape_properties_atom_sorted_yet_recover_source_order_via_declaration_order`,
/// which guarantees that sorting `props_shape.properties` by
/// `declaration_order` recovers source order — so the missing list reads
/// `x, y` (declaration order) rather than the atom-sorted order.
#[test]
fn jsx_spread_missing_props_listed_for_each_required_target_property() {
    let diags = check_source(
        r#"
declare namespace JSX {
    interface IntrinsicAttributes {}
    interface IntrinsicElements {}
    interface ElementAttributesProperty { props: {}; }
    interface ElementChildrenAttribute {}
}

interface P { x: string; y: "2"; }
declare function F(p: P): null;

declare const obj: {};
const _r = <F {...obj} />;
"#,
        "test.tsx",
        CheckerOptions::default(),
    );

    let missing_msgs: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2739 || d.code == 2740 || d.code == 2741)
        .map(|d| &d.message_text)
        .collect();
    assert!(
        !missing_msgs.is_empty(),
        "Expected a missing-properties diagnostic (TS2739/TS2740/TS2741). Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );

    let listing = missing_msgs
        .iter()
        .find(|m| m.contains("missing the following properties"))
        .unwrap_or_else(|| {
            panic!("Expected a 'missing the following properties' message. Got: {missing_msgs:?}");
        });

    // Both required properties must show up, after the post-colon list
    // (i.e., not as part of the surrounding template like 'type').
    let after_colon = listing
        .rsplit_once(": ")
        .map(|(_, rest)| rest)
        .unwrap_or(listing);
    assert!(
        after_colon.contains('x'),
        "missing-properties list must mention x; got: {listing:?}"
    );
    assert!(
        after_colon.contains('y'),
        "missing-properties list must mention y; got: {listing:?}"
    );
}
