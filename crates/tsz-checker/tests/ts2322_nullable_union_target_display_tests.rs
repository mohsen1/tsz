//! Tests for TS2322 / TS2345 nullable-union *target* display.
//!
//! tsc collapses a nullable union target (`T | null`, `T | undefined`,
//! `T | null | undefined`) to its non-nullish part in an assignability message
//! only when a *single* real member survives the strip — e.g.
//! `string | undefined` renders as `string`. When two or more non-nullish
//! members remain, tsc keeps the full union — nullish members included — on the
//! target line (`string | number | undefined`, `A | B | null`,
//! `"a" | "b" | undefined` all stay intact), regardless of whether the surviving
//! members are primitive or object-like. This holds uniformly across variable
//! declarations, assignment expressions, return positions, property targets, and
//! call arguments.
//!
//! Regression coverage for the nullable-union over-reduction fixed in
//! `strip_nullish_for_assignability_display` (issue: nullable-union target strips
//! nullish member when >=2 real members remain). Binder names are varied so the
//! assertions cannot be satisfied by any identifier fast-path.

fn strict(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_strict_messages(source)
}

fn assignability_msgs(diags: &[(u32, String)]) -> Vec<&str> {
    diags
        .iter()
        .filter_map(|(code, msg)| matches!(code, 2322 | 2345).then_some(msg.as_str()))
        .collect()
}

/// A union target with two or more non-nullish members plus `undefined`
/// keeps the full union — including `undefined` — on the target line.
#[test]
fn keeps_full_union_with_two_primitive_members_and_undefined() {
    let diags = strict("const alpha: string | number | undefined = true;");
    let msgs = assignability_msgs(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("type 'string | number | undefined'")),
        "expected full nullable union on the target line, got: {msgs:?}"
    );
    assert!(
        !msgs.iter().any(|m| m.contains("type 'string | number'.")),
        "target must not drop the undefined member, got: {msgs:?}"
    );
}

/// Same rule for a `null`-bearing union (not just `undefined`).
#[test]
fn keeps_full_union_with_two_members_and_null() {
    let diags = strict("let beta: string | number | null;\nbeta = true;");
    let msgs = assignability_msgs(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("type 'string | number | null'")),
        "expected full null-bearing union on the target line, got: {msgs:?}"
    );
}

/// Object-like members are treated the same as primitives: two or more object
/// members plus `undefined` keep the full union.
#[test]
fn keeps_full_union_with_two_object_members_and_undefined() {
    let diags = strict("const gamma: { a: number } | { b: string } | undefined = 5;");
    let msgs = assignability_msgs(&diags);
    assert!(
        msgs.iter()
            .any(|m| { m.contains("{ a: number; } | { b: string; } | undefined") }),
        "expected full object union including undefined, got: {msgs:?}"
    );
}

/// Property-target position (through a named interface) obeys the same rule.
#[test]
fn keeps_full_union_for_interface_property_target() {
    let diags = strict(
        "interface Holder { field: string | number | undefined }\nconst delta: Holder = { field: true };",
    );
    let msgs = assignability_msgs(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("type 'string | number | undefined'")),
        "expected full union for interface property target, got: {msgs:?}"
    );
}

/// An optional property whose declared type is a two-member union expands to
/// `T1 | T2 | undefined` and keeps every member.
#[test]
fn keeps_full_union_for_optional_property_target() {
    let diags =
        strict("interface Box { slot?: string | number }\nconst epsilon: Box = { slot: true };");
    let msgs = assignability_msgs(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("type 'string | number | undefined'")),
        "expected optional property to keep undefined in the union, got: {msgs:?}"
    );
}

/// Return position obeys the same rule.
#[test]
fn keeps_full_union_for_return_target() {
    let diags = strict("const zeta = (): string | number | undefined => true;");
    let msgs = assignability_msgs(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("type 'string | number | undefined'")),
        "expected full union for return target, got: {msgs:?}"
    );
}

// ----- Negative controls: single-member reduction must be preserved. -----

/// A union with exactly one non-nullish member still collapses to that member,
/// matching tsc (`string | undefined` -> `string`).
#[test]
fn single_primitive_member_still_collapses() {
    let diags = strict("const eta: string | undefined = 5;");
    let msgs = assignability_msgs(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("is not assignable to type 'string'.")),
        "expected single-member collapse to 'string', got: {msgs:?}"
    );
    assert!(
        !msgs.iter().any(|m| m.contains("string | undefined")),
        "single-member target must collapse, got: {msgs:?}"
    );
}

/// A union with no nullish member is never touched.
#[test]
fn no_nullish_union_unchanged() {
    let diags = strict("const iota: string | number = true;");
    let msgs = assignability_msgs(&diags);
    assert!(
        msgs.iter().any(|m| m.contains("type 'string | number'")),
        "expected untouched non-nullable union, got: {msgs:?}"
    );
}

/// Argument-position multi-member nullable union keeps the full union too.
#[test]
fn argument_multi_member_keeps_full_union() {
    let diags = strict("function kappa(param: string | number | undefined) {}\nkappa(true);");
    let msgs = assignability_msgs(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("parameter of type 'string | number | undefined'")),
        "expected full union on the parameter line, got: {msgs:?}"
    );
}
