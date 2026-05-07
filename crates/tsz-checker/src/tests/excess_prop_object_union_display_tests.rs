//! Regression for the `nonObjectUnionNestedExcessPropertyCheck`
//! conformance failure: TS2353's diagnostic target should display only
//! the object-like member of a union (e.g. `IProps`), not the full
//! union (`IProps | number`). Primitive members aren't subject to
//! excess-property checking, so including them is noise.

use crate::test_utils::check_source_diagnostics;

#[test]
fn ts2353_strips_primitive_union_member_from_target_display() {
    let diags = check_source_diagnostics(
        r#"
interface IProps {
    iconProp?: string;
}
const propB1: IProps | number = { INVALID_PROP_NAME: 'share', iconProp: 'test' };
"#,
    );

    let ts2353: Vec<_> = diags.iter().filter(|d| d.code == 2353).collect();
    assert!(
        !ts2353.is_empty(),
        "expected TS2353 excess-property diagnostic; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    let msg = &ts2353[0].message_text;
    assert!(
        msg.contains("'IProps'"),
        "TS2353 should display target as 'IProps' (object member only); got: {msg}"
    );
    assert!(
        !msg.contains("IProps | number"),
        "TS2353 should not display the full union 'IProps | number'; got: {msg}"
    );
}
