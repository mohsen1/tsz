//! TS2447 on a *compound* bitwise assignment (`&=`, `|=`, `^=`) must anchor at
//! the operator token, exactly as it already does for the plain binary form.
//!
//! Structural rule: when a bitwise operator is applied to two boolean operands,
//! tsc anchors TS2447 at the operator token, not at the enclosing expression or
//! either operand. `operator_token_span` derives that span from the binary
//! expression node, so the compound-assignment caller has to hand it the
//! expression — passing the left operand made the lookup fail and silently fall
//! back to the operand's own position.
//!
//! Pins `compiler/bitwiseCompoundAssignmentOperators.ts`, whose oracle anchors
//! all three diagnostics one token right of where tsz used to put them.

use tsz_checker::test_utils::check_source_diagnostics;

/// Byte offset of `needle`'s first occurrence, as a `u32` diagnostic start.
fn offset_of(source: &str, needle: &str) -> u32 {
    u32::try_from(source.find(needle).expect("needle present in source")).expect("offset fits u32")
}

fn ts2447_starts(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .iter()
        .filter(|d| d.code == 2447)
        .map(|d| d.start)
        .collect()
}

#[test]
fn compound_bitwise_assignment_anchors_at_operator_token() {
    for (op, source) in [
        ("^=", "var a = true;\na ^= a;\n"),
        ("&=", "var a = true;\na &= a;\n"),
        ("|=", "var a = true;\na |= a;\n"),
    ] {
        let starts = ts2447_starts(source);
        assert_eq!(
            starts,
            vec![offset_of(source, op)],
            "TS2447 for `{op}` should anchor at the operator token, got: {:?}",
            check_source_diagnostics(source)
        );
    }
}

/// The plain binary form was already correct; lock it so the shared
/// `operator_token_span` path cannot regress in the other direction.
#[test]
fn plain_bitwise_binary_still_anchors_at_operator_token() {
    let source = "var a = true;\nvar r = a ^ a;\n";
    assert_eq!(
        ts2447_starts(source),
        vec![offset_of(source, "^")],
        "TS2447 for a plain `^` should anchor at the operator token"
    );
}

/// Renamed binders and a longer left operand: the anchor is computed from the
/// operand span, so it must not be a fixed offset from the statement start.
#[test]
fn compound_operator_anchor_survives_renamed_binders() {
    let source = "var someLongerFlagName = false;\nsomeLongerFlagName |= someLongerFlagName;\n";
    assert_eq!(
        ts2447_starts(source),
        vec![offset_of(source, "|=")],
        "TS2447 anchor must track the operator, not a fixed column"
    );
}

/// Non-boolean operands take the arithmetic path instead — no TS2447 at all.
#[test]
fn numeric_compound_bitwise_assignment_reports_no_ts2447() {
    let starts = ts2447_starts("var n = 1;\nn ^= n;\n");
    assert!(
        starts.is_empty(),
        "numeric operands must not produce TS2447, got starts: {starts:?}"
    );
}
