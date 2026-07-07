//! Numeric enum assignment diagnostics. Split into its own shard so the enum
//! matrix can grow without pushing the older `part_00` file over the line cap.

use super::*;

fn messages_with_code(source: &str, code: u32) -> Vec<String> {
    get_all_diagnostics(source)
        .into_iter()
        .filter_map(|(actual, message)| (actual == code).then_some(message))
        .collect()
}

#[test]
fn test_ts2322_numeric_enum_assignment_override_uses_member_values() {
    let messages = ts2322_messages(
        r#"
enum Status { Ready = 1, Done = 2 }
declare const num: number;

const wholeOk: Status = 1;
const wholeBad: Status = 3;
const memberOk: Status.Ready = 1;
const memberBad: Status.Ready = 2;
const wholeFromNumber: Status = num;
const memberFromNumber: Status.Ready = num;
"#,
    );

    assert_eq!(
        messages.len(),
        2,
        "only the out-of-domain numeric literals should report TS2322, got: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Type '3' is not assignable to type 'Status'.")),
        "expected whole enum target to reject numeric literal outside declared members, got: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Type '2' is not assignable to type 'Status.Ready'.")),
        "expected enum member target to reject a different numeric member value, got: {messages:#?}"
    );
}

#[test]
fn test_ts2345_numeric_enum_argument_override_uses_member_values() {
    let messages = messages_with_code(
        r#"
enum Status { Ready = 1, Done = 2 }
declare const num: number;

function takeStatus(value: Status) {}
function takeReady(value: Status.Ready) {}

takeStatus(1);
takeStatus(3);
takeReady(1);
takeReady(2);
takeStatus(num);
takeReady(num);
"#,
        2345,
    );

    assert_eq!(
        messages.len(),
        2,
        "only the out-of-domain numeric literal arguments should report TS2345, got: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| m
                .contains("Argument of type '3' is not assignable to parameter of type 'Status'.")),
        "expected whole enum parameter to reject numeric literal outside declared members, got: {messages:#?}"
    );
    assert!(
        messages.iter().any(|m| m.contains(
            "Argument of type '2' is not assignable to parameter of type 'Status.Ready'."
        )),
        "expected enum member parameter to reject a different numeric member value, got: {messages:#?}"
    );
}

#[test]
fn test_ts2322_numeric_enum_assignment_override_is_not_name_keyed() {
    let messages = ts2322_messages(
        r#"
enum Renamed { Alpha = 10, Beta = 20 }
enum Other { Alpha = 10, Beta = 20 }
enum Text { Alpha = "alpha" }

const renamedOk: Renamed.Alpha = 10;
const renamedBad: Renamed.Alpha = 20;
const otherBad: Other = 30;
const textBad: Text = "alpha";
"#,
    );

    assert_eq!(
        messages.len(),
        3,
        "renamed numeric enum failures plus string-enum control should report, got: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Type '20' is not assignable to type 'Renamed.Alpha'.")),
        "expected renamed enum member target to compare numeric values structurally, got: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Type '30' is not assignable to type 'Other'.")),
        "expected renamed whole enum target to reject undeclared numeric literal, got: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Type '\"alpha\"' is not assignable to type 'Text'.")),
        "string enums must not use the numeric assignment override, got: {messages:#?}"
    );
}

#[test]
fn test_ts2322_numeric_enum_member_union_literal_targets_subset_vs_full_display() {
    let messages = ts2322_messages(
        r#"
enum Size { Small = 1, Large = 2, Huge = 3 }

const okSubset: Size.Small | Size.Large = 1;
const badSubset: Size.Small | Size.Large = 3;
const okFull: Size.Small | Size.Large | Size.Huge = 2;
const badFull: Size.Small | Size.Large | Size.Huge = 4;
"#,
    );

    assert_eq!(
        messages.len(),
        2,
        "only subset/full out-of-domain literals should report TS2322, got: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Type '3' is not assignable to type 'Size.Small | Size.Large'.")),
        "proper subset enum-member union target must stay expanded, got: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Type '4' is not assignable to type 'Size'.")),
        "full enum-member union target should collapse to parent enum display, got: {messages:#?}"
    );
}

#[test]
fn test_ts2345_numeric_enum_member_union_literal_parameters_subset_vs_full_display() {
    let messages = messages_with_code(
        r#"
enum Size { Small = 1, Large = 2, Huge = 3 }

function takeSubset(value: Size.Small | Size.Large) {}
function takeFull(value: Size.Small | Size.Large | Size.Huge) {}

takeSubset(1);
takeSubset(3);
takeFull(2);
takeFull(4);
"#,
        2345,
    );

    assert_eq!(
        messages.len(),
        2,
        "only subset/full out-of-domain arguments should report TS2345, got: {messages:#?}"
    );
    assert!(
        messages.iter().any(|m| m.contains(
            "Argument of type '3' is not assignable to parameter of type 'Size.Small | Size.Large'."
        )),
        "proper subset enum-member union parameter must stay expanded, got: {messages:#?}"
    );
    assert!(
        messages.iter().any(|m| {
            m.contains("Argument of type '4' is not assignable to parameter of type 'Size'.")
        }),
        "full enum-member union parameter should collapse to parent enum display, got: {messages:#?}"
    );
}

#[test]
fn test_ts2322_numeric_enum_compatibility_resets_auto_values_per_merged_declaration() {
    let messages = ts2322_messages(
        r#"
namespace Left {
    export enum Status { Ready = 1 }
    export enum Status { Done }
}
namespace Right {
    export enum Status { Ready = 1, Done = 0 }
}
namespace Carry {
    export enum Status { Ready = 1, Done = 2 }
}

declare const left: Left.Status;
declare const right: Right.Status;
declare const carry: Carry.Status;

const okToRight: Right.Status = left;
const okToLeft: Left.Status = right;
const badCarryToRight: Right.Status = carry;
"#,
    );

    assert_eq!(
        messages.len(),
        1,
        "only the carried-value enum should fail numeric compatibility, got: {messages:#?}"
    );
    assert!(
        messages.iter().any(|message| message
            .contains("Type 'Carry.Status' is not assignable to type 'Right.Status'.")),
        "merged enum declaration auto-values should reset per declaration block, got: {messages:#?}"
    );
}
