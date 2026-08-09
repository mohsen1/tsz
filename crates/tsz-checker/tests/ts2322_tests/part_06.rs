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

// ---------------------------------------------------------------------------
// `object` (the non-primitive intrinsic) as an assignment/argument source.
//
// `object` is not a `TypeFlags.StructuredType`, so tsc never drills into a
// target's members to elaborate a missing-property line for it. A failed
// `object <: <object-like>` surfaces the generic TS2322 (naming `object`
// verbatim) — never TS2741/TS2739/TS2740 rendered against a `{}` apparent
// shape. Regression witness:
// conformance/types/nonPrimitive/nonPrimitiveAssignError.ts.
// ---------------------------------------------------------------------------

/// Assert an `object` (non-primitive intrinsic) source produces exactly one
/// generic TS2322 equal to `expected` and no missing-property elaboration
/// (TS2739/TS2740/TS2741). Collects the diagnostics once.
fn assert_object_source_single_ts2322(source: &str, expected: &str) {
    let diagnostics = get_all_diagnostics(source);
    for missing_property_code in [2739_u32, 2740, 2741] {
        assert!(
            !has_diagnostic_code(&diagnostics, missing_property_code),
            "`object` source must not emit TS{missing_property_code}, got: {diagnostics:?}"
        );
    }
    let ts2322: Vec<&str> = diagnostics
        .iter()
        .filter_map(|(code, message)| (*code == 2322).then_some(message.as_str()))
        .collect();
    assert_eq!(
        ts2322,
        vec![expected],
        "expected exactly one generic TS2322 naming `object` verbatim (not a \
         `{{}}`-rendered apparent shape), got: {diagnostics:?}"
    );
}

/// A single required target property must NOT turn an `object` source into a
/// TS2741 "property missing" line; tsc reports the generic TS2322 naming
/// `object`.
#[test]
fn test_object_intrinsic_source_reports_ts2322_not_missing_property() {
    assert_object_source_single_ts2322(
        r#"
declare var src: object;
var dst: { foo: string } = src;
"#,
        "Type 'object' is not assignable to type '{ foo: string; }'.",
    );
}

/// Multiple missing target properties must NOT produce TS2739/TS2740 for an
/// `object` source either — still the generic TS2322.
#[test]
fn test_object_intrinsic_source_multi_property_target_reports_ts2322() {
    assert_object_source_single_ts2322(
        r#"
declare var payload: object;
var sink: { foo: string; bar: number } = payload;
"#,
        "Type 'object' is not assignable to type '{ foo: string; bar: number; }'.",
    );
}

/// An index-signature target follows the same rule (renamed binder to keep the
/// check binder-name independent).
#[test]
fn test_object_intrinsic_source_index_signature_target_reports_ts2322() {
    assert_object_source_single_ts2322(
        r#"
declare var bag: object;
var lookup: { [key: string]: number } = bag;
"#,
        "Type 'object' is not assignable to type '{ [key: string]: number; }'.",
    );
}

/// The same discipline holds in argument position (TS2345): an `object`
/// argument against an object-typed parameter reports the generic argument
/// mismatch, not a nested missing-property elaboration.
#[test]
fn test_object_intrinsic_argument_source_reports_ts2345_not_missing_property() {
    let source = r#"
declare function sink(target: { foo: string }): void;
declare var src: object;
sink(src);
"#;
    let diagnostics = get_all_diagnostics(source);

    assert!(
        !has_diagnostic_code(&diagnostics, 2741),
        "`object` argument must not emit TS2741, got: {diagnostics:?}"
    );
    let ts2345 = messages_with_code(source, 2345);
    assert_eq!(
        ts2345,
        vec![
            "Argument of type 'object' is not assignable to parameter of type '{ foo: string; }'."
                .to_string()
        ],
        "expected generic TS2345 naming `object`, got: {diagnostics:?}"
    );
}

/// Regression guard: the empty object type `{}` and a members-less interface
/// ARE structured object sources, so a missing required target property still
/// elaborates as TS2741 (tsc parity). The `object` fix must not disturb them.
#[test]
fn test_empty_object_and_empty_interface_sources_keep_ts2741() {
    let empty_object = r#"
declare var src: {};
var dst: { foo: string } = src;
"#;
    assert!(
        has_diagnostic_code(&get_all_diagnostics(empty_object), 2741),
        "empty object literal `{{}}` source must keep TS2741"
    );

    let empty_interface = r#"
interface Blank {}
declare var src: Blank;
var dst: { foo: string } = src;
"#;
    assert!(
        has_diagnostic_code(&get_all_diagnostics(empty_interface), 2741),
        "members-less interface source must keep TS2741"
    );
}
