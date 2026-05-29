//! Regression tests for constrained-`infer` whole-candidate semantics.
//!
//! A constrained `infer U extends C` is a *whole-candidate* check in tsc: the
//! inferred candidate `X` is kept only when `X` (as a whole) is assignable to
//! the constraint `C`; otherwise the conditional takes its false branch. tsc
//! never keeps a matching subset of a union candidate while dropping the rest.
//!
//! The one wrinkle is optional positions (optional tuple element / optional
//! property): they contribute an extra `undefined` to the candidate (an absent
//! element reads as `undefined`). tsc strips that optionality-`undefined`
//! before the whole-candidate check, but still does not partially filter the
//! remaining union.
//!
//! Previously tsz partially filtered union candidates against the constraint
//! (e.g. `[1 | 2 | "x"] extends [infer U extends number]` wrongly produced
//! `1 | 2` instead of the false branch), which diverged from tsc.
//!
//! These tests assert the resolved type by feeding the conditional result into
//! assignments whose acceptance/rejection pins the type down, and they vary the
//! `infer`/type-parameter names so the fix cannot be name-specific.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn strict_options() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..Default::default()
    }
}

/// Count TS2322 "not assignable" diagnostics produced by `source`.
fn ts2322_count(source: &str) -> usize {
    check_source(source, "test.ts", strict_options())
        .iter()
        .filter(|d| d.code == 2322)
        .count()
}

/// Assert the conditional result type is exactly `"NO"` (the false branch):
/// assigning `"NO"` is accepted and assigning some other value is rejected.
fn assert_false_branch_no(type_decl: &str) {
    let ok = format!("{type_decl}\nconst ok: R = \"NO\";\n");
    let bad = format!("{type_decl}\nconst bad: R = \"other\";\n");
    assert_eq!(
        ts2322_count(&ok),
        0,
        "expected false branch (\"NO\") to accept \"NO\":\n{ok}"
    );
    assert_eq!(
        ts2322_count(&bad),
        1,
        "expected false branch (\"NO\") to reject \"other\":\n{bad}"
    );
}

// ---------------------------------------------------------------------------
// Required positions: whole-candidate check, no partial union filtering.
// ---------------------------------------------------------------------------

/// Reported repro: a union candidate with a non-conforming member must take the
/// false branch, not be filtered down to its matching members.
#[test]
fn required_tuple_union_partial_match_takes_false_branch() {
    assert_false_branch_no(
        "type C<T> = T extends [infer U extends number] ? U : \"NO\";\n\
         type R = C<[1 | 2 | \"x\"]>;",
    );
}

/// Same rule for an object property position (different surface, same rule).
#[test]
fn required_property_union_partial_match_takes_false_branch() {
    assert_false_branch_no(
        "type C<T> = T extends { a: infer U extends number } ? U : \"NO\";\n\
         type R = C<{ a: 1 | 2 | \"x\" }>;",
    );
}

/// Renamed binder variables (K instead of U, P instead of T) must behave
/// identically — proves the fix is structural, not name-specific.
#[test]
fn required_union_partial_match_renamed_params() {
    assert_false_branch_no(
        "type Cond<P> = P extends [infer K extends number] ? K : \"NO\";\n\
         type R = Cond<[1 | 2 | \"x\"]>;",
    );
}

/// A union candidate that *wholly* satisfies the constraint is kept in full.
#[test]
fn required_union_all_members_match_keeps_full_union() {
    // `R = 1 | 2`: accepts both members, rejects an outsider.
    let src_ok1 = "type C<T> = T extends [infer U extends number] ? U : \"NO\";\n\
                   type R = C<[1 | 2]>;\nconst a: R = 1;";
    let src_ok2 = "type C<T> = T extends [infer U extends number] ? U : \"NO\";\n\
                   type R = C<[1 | 2]>;\nconst b: R = 2;";
    let src_bad = "type C<T> = T extends [infer U extends number] ? U : \"NO\";\n\
                   type R = C<[1 | 2]>;\nconst c: R = 3;";
    assert_eq!(ts2322_count(src_ok1), 0, "1 should be assignable to 1 | 2");
    assert_eq!(ts2322_count(src_ok2), 0, "2 should be assignable to 1 | 2");
    assert_eq!(
        ts2322_count(src_bad),
        1,
        "3 should not be assignable to 1 | 2"
    );
}

/// A single non-conforming candidate still takes the false branch.
#[test]
fn required_single_non_matching_takes_false_branch() {
    assert_false_branch_no(
        "type C<T> = T extends [infer U extends number] ? U : \"NO\";\n\
         type R = C<[\"x\"]>;",
    );
}

// ---------------------------------------------------------------------------
// Optional positions: strip optionality-`undefined`, then whole-candidate check.
// ---------------------------------------------------------------------------

/// Optional property whose value matches the constraint: the optionality
/// `undefined` is stripped and the real type is inferred.
#[test]
fn optional_property_matching_infers_stripped_type() {
    // `R = string`: a string is accepted, a non-string is rejected.
    let ok = "type C<T> = T extends { a?: infer U extends string } ? U : \"NO\";\n\
              type R = C<{ a?: string }>;\nconst a: R = \"hi\";";
    let bad = "type C<T> = T extends { a?: infer U extends string } ? U : \"NO\";\n\
               type R = C<{ a?: string }>;\nconst b: R = 123;";
    assert_eq!(
        ts2322_count(ok),
        0,
        "optional matching prop should infer string"
    );
    assert_eq!(ts2322_count(bad), 1, "result should be string, not number");
}

/// Optional property with a union that only *partly* matches: after stripping
/// `undefined`, the remaining union does not satisfy the constraint as a whole,
/// so the conditional takes the false branch (no partial filtering to `"x"`).
#[test]
fn optional_property_partial_union_takes_false_branch() {
    assert_false_branch_no(
        "type C<T> = T extends { a?: infer U extends string } ? U : \"NO\";\n\
         type R = C<{ a?: \"x\" | 1 }>;",
    );
}

/// Optional property union that *wholly* matches keeps the full union (still no
/// stray `undefined`).
#[test]
fn optional_property_full_union_keeps_union_without_undefined() {
    let ok1 = "type C<T> = T extends { a?: infer U extends string } ? U : \"NO\";\n\
               type R = C<{ a?: \"x\" | \"y\" }>;\nconst a: R = \"x\";";
    let ok2 = "type C<T> = T extends { a?: infer U extends string } ? U : \"NO\";\n\
               type R = C<{ a?: \"x\" | \"y\" }>;\nconst b: R = \"y\";";
    let bad_other = "type C<T> = T extends { a?: infer U extends string } ? U : \"NO\";\n\
                     type R = C<{ a?: \"x\" | \"y\" }>;\nconst c: R = \"z\";";
    let bad_undef = "type C<T> = T extends { a?: infer U extends string } ? U : \"NO\";\n\
                     type R = C<{ a?: \"x\" | \"y\" }>;\nconst d: R = undefined;";
    assert_eq!(ts2322_count(ok1), 0);
    assert_eq!(ts2322_count(ok2), 0);
    assert_eq!(ts2322_count(bad_other), 1, "result is \"x\" | \"y\"");
    assert_eq!(
        ts2322_count(bad_undef),
        1,
        "optionality undefined must be stripped from the result"
    );
}

// ---------------------------------------------------------------------------
// Distributive conditionals: each member is evaluated independently, so a
// distributive union still yields per-member results (not a silent filter).
// ---------------------------------------------------------------------------

/// `T extends infer U extends number ? U : "NO"` distributed over
/// `1 | 2 | "x"` yields `1 | 2 | "NO"` — the `"x"` member maps to the false
/// branch rather than being dropped.
#[test]
fn distributive_union_maps_failing_member_to_false_branch() {
    let accepts_no = "type C<T> = T extends infer U extends number ? U : \"NO\";\n\
                      type R = C<1 | 2 | \"x\">;\nconst a: R = \"NO\";";
    let accepts_1 = "type C<T> = T extends infer U extends number ? U : \"NO\";\n\
                     type R = C<1 | 2 | \"x\">;\nconst b: R = 1;";
    let rejects_str = "type C<T> = T extends infer U extends number ? U : \"NO\";\n\
                       type R = C<1 | 2 | \"x\">;\nconst c: R = \"x\";";
    assert_eq!(
        ts2322_count(accepts_no),
        0,
        "\"NO\" is a member of 1 | 2 | \"NO\""
    );
    assert_eq!(
        ts2322_count(accepts_1),
        0,
        "1 is a member of 1 | 2 | \"NO\""
    );
    assert_eq!(
        ts2322_count(rejects_str),
        1,
        "\"x\" maps to the false branch, so it is not in the result"
    );
}
