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
