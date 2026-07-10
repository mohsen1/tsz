//! Coercion of captured template-literal segments to non-string primitives.
//!
//! A segment captured out of a template literal string is always raw text.
//! When the capture target (an `infer` placeholder or an inference type
//! variable) carries a constraint that admits a non-string primitive, tsc
//! re-interprets the text as that primitive: `"2"` becomes the `2` number
//! literal for `T extends number`, `"true"` the `true` literal for
//! `T extends boolean`, and so on (`inferToTemplateLiteralType` in
//! `checker.ts`). Both the conditional-type `infer` path and call-site type
//! parameter inference share this rule, so it lives here as the single owner.

use crate::caches::db::TypeDatabase;
use crate::relations::subtype::{SubtypeChecker, TypeResolver};
use crate::types::TypeId;

/// Re-interpret a captured segment as a number literal type.
///
/// Returns the number literal when the text round-trips through ECMAScript
/// `Number::toString` (tsc's `isValidNumberString(text, roundTripOnly:
/// true)`), the `number` intrinsic when the text parses but does not
/// round-trip (e.g. `"0x10"`, `"1e21"`), and `None` when it does not parse.
///
/// The radix parsing here deliberately does NOT reuse
/// `tsz_common::numeric::parse_numeric_literal_value`: that helper implements
/// the numeric *literal* grammar (numeric separators allowed), while a
/// captured segment is a runtime string coerced with `Number(string)`
/// semantics (`"1_0"` is `NaN`, hex/octal/binary strings are allowed).
fn parse_number_capture(db: &dyn TypeDatabase, captured: &str) -> Option<TypeId> {
    let value = if let Some(digits) = captured
        .strip_prefix("0x")
        .or_else(|| captured.strip_prefix("0X"))
    {
        u64::from_str_radix(digits, 16).ok().map(|n| n as f64)?
    } else if let Some(digits) = captured
        .strip_prefix("0o")
        .or_else(|| captured.strip_prefix("0O"))
    {
        u64::from_str_radix(digits, 8).ok().map(|n| n as f64)?
    } else if let Some(digits) = captured
        .strip_prefix("0b")
        .or_else(|| captured.strip_prefix("0B"))
    {
        u64::from_str_radix(digits, 2).ok().map(|n| n as f64)?
    } else {
        captured.parse::<f64>().ok()?
    };

    if !value.is_finite() {
        return None;
    }

    let round_trips = crate::utils::js_number_to_string(value) == captured;
    Some(if round_trips {
        db.literal_number(value)
    } else {
        TypeId::NUMBER
    })
}

/// Re-interpret a captured segment as a bigint literal type (digits with an
/// optional sign, no `n` suffix in the captured text).
fn parse_bigint_capture(db: &dyn TypeDatabase, captured: &str) -> Option<TypeId> {
    let (negative, digits) = captured
        .strip_prefix('-')
        .map_or((false, captured), |rest| (true, rest));
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }

    Some(db.literal_bigint_with_sign(negative, digits))
}

/// Non-string primitive coercions of a captured template segment.
///
/// The candidates are constraint-agnostic — the caller decides which one
/// satisfies the constraint via a structural subtype probe — so this covers
/// intrinsic, literal, and union-of-literal constraints uniformly, e.g.
/// `extends number`, `extends 5`, `extends 0 | 1`, or `extends bigint`.
fn capture_coercions(db: &dyn TypeDatabase, captured: &str) -> [Option<TypeId>; 5] {
    [
        parse_number_capture(db, captured),
        parse_bigint_capture(db, captured),
        match captured {
            "true" => Some(db.literal_boolean(true)),
            "false" => Some(db.literal_boolean(false)),
            _ => None,
        },
        (captured == "null").then_some(TypeId::NULL),
        (captured == "undefined").then_some(TypeId::UNDEFINED),
    ]
}

/// Pick the capture interpretation admitted by `constraint`.
///
/// The captured segment is used as-is (a string literal type) when it already
/// satisfies the constraint; otherwise the first non-string coercion the
/// constraint admits wins. Returns `None` when nothing satisfies the
/// constraint.
pub(crate) fn capture_for_constraint<R: TypeResolver>(
    db: &dyn TypeDatabase,
    checker: &mut SubtypeChecker<'_, R>,
    captured: &str,
    captured_type: TypeId,
    constraint: TypeId,
) -> Option<TypeId> {
    if checker.is_subtype_of(captured_type, constraint) {
        return Some(captured_type);
    }

    capture_coercions(db, captured)
        .into_iter()
        .flatten()
        .find(|&candidate| checker.is_subtype_of(candidate, constraint))
}
