//! Regression coverage for TS2345 suppression on nominal-looking application
//! surfaces. The suppression may only apply when the full application surface
//! matches structurally, not merely when both sides share the same generic base.

use tsz_checker::test_utils::check_source_code_messages as check;
use tsz_common::diagnostics::diagnostic_codes;

const TS2345: u32 = diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE;

#[test]
fn same_generic_base_different_type_arg_still_reports_ts2345() {
    let diagnostics = check(
        r#"
interface Carrier<T> {
    value: T;
}
declare function takeNumber(input: Carrier<number>): void;
declare const stringCarrier: Carrier<string>;
takeNumber(stringCarrier);
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, _)| *code == TS2345),
        "different type arguments for the same generic base must not be suppressed: {diagnostics:?}"
    );
}

#[test]
fn renamed_generic_base_different_type_arg_still_reports_ts2345() {
    let diagnostics = check(
        r#"
interface RenamedBox<Item> {
    payload: Item;
}
declare function takeBoolean(input: RenamedBox<boolean>): void;
declare const numberBox: RenamedBox<number>;
takeBoolean(numberBox);
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, _)| *code == TS2345),
        "suppression must compare type arguments structurally, independent of user-chosen names: {diagnostics:?}"
    );
}
