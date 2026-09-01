//! Narrowing of an object reference survives a member/element assignment to that
//! reference when a control-flow join sits between the guard and the assignment.
//!
//! Structural rule: a member/element assignment `m.p = …` (or `m[k] = …`,
//! `m.a.b = …`) is a definition of the *property*, not of the reference `m`, so
//! it must not reset `m`'s flow-narrowed type back to its declared type. When a
//! control-flow join (`BRANCH_LABEL`) sits between a truthiness guard `if (m)`
//! and the property mutation — an empty `if`, an optional-chain expression
//! statement, or a `??` expression statement — the flow worklist's
//! property-mutation arm must still *defer* to the merged antecedent so the
//! guard's narrowing is preserved. Owner layer: checker flow narrowing
//! (`flow/control_flow/core/flow_traversal.rs`).
//!
//! A direct reassignment follows the right-hand side's current flow type. It
//! un-narrows `m` only when that type still includes `undefined`; an annotated
//! local initialized from the narrowed `m` remains flow-narrowed to `M`.

use crate::context::CheckerOptions;
use crate::test_utils::{check_with_options, diagnostic_count};

fn strict() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
}

const DECLS: &str = "interface M { p?: number; id: string; a: { b: string }; }\n\
    declare const cond: boolean;\n\
    declare const opt: { y(): void } | undefined;\n";

fn check(body: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let src = format!("{DECLS}function f(m: M | undefined): void {{ {body} }}\n");
    check_with_options(&src, strict())
}

fn check_null(body: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let src = format!("{DECLS}function f(m: M | null): void {{ {body} }}\n");
    check_with_options(&src, strict())
}

// --- positives: narrowing preserved across the join ---

#[test]
fn member_assign_after_empty_if_join_no_ts18048() {
    let diags = check("if (m) { if (cond) {} m.p = 1; m.id; }");
    assert_eq!(
        diagnostic_count(&diags, 18048),
        0,
        "member assignment after an empty-if join must not un-narrow m: {diags:?}"
    );
}

#[test]
fn member_assign_after_optional_chain_join_no_ts18048() {
    let diags = check("if (m) { opt?.y(); m.p = 1; m.id; }");
    assert_eq!(
        diagnostic_count(&diags, 18048),
        0,
        "member assignment after an optional-chain join must not un-narrow m: {diags:?}"
    );
}

#[test]
fn member_assign_after_nullish_join_no_ts18048() {
    let diags = check("if (m) { (opt ?? undefined); m.p = 1; m.id; }");
    assert_eq!(
        diagnostic_count(&diags, 18048),
        0,
        "member assignment after a ?? join must not un-narrow m: {diags:?}"
    );
}

#[test]
fn element_assign_after_join_no_ts18048() {
    let diags = check("if (m) { if (cond) {} m[\"p\"] = 1; m.id; }");
    assert_eq!(
        diagnostic_count(&diags, 18048),
        0,
        "element-access assignment after a join must not un-narrow m: {diags:?}"
    );
}

#[test]
fn nested_member_assign_after_join_no_ts18048() {
    let diags = check("if (m) { if (cond) {} m.a.b = \"x\"; m.id; }");
    assert_eq!(
        diagnostic_count(&diags, 18048),
        0,
        "nested member assignment m.a.b after a join must not un-narrow m: {diags:?}"
    );
}

#[test]
fn member_assign_after_join_null_union_no_ts18047() {
    let diags = check_null("if (m) { if (cond) {} m.p = 1; m.id; }");
    assert_eq!(
        diagnostic_count(&diags, 18047),
        0,
        "member assignment after a join must not re-introduce possibly-null on m: {diags:?}"
    );
}

// --- negatives: a direct reassignment of m DOES un-narrow it ---

#[test]
fn direct_reassign_undefined_after_join_keeps_ts18048() {
    let diags = check("if (m) { if (cond) {} m = undefined; m.id; }");
    assert_eq!(
        diagnostic_count(&diags, 18048),
        1,
        "directly reassigning m = undefined after the guard must still report TS18048: {diags:?}"
    );
}

#[test]
fn direct_reassign_maybe_after_join_keeps_ts18048() {
    let diags = check(
        "if (m) { if (cond) {} let maybe: M | undefined = cond ? m : undefined; m = maybe; m.id; }",
    );
    assert_eq!(
        diagnostic_count(&diags, 18048),
        1,
        "reassigning m from a flow type that includes undefined must still report TS18048: {diags:?}"
    );
}

#[test]
fn direct_reassign_flow_narrowed_alias_after_join_stays_clean() {
    let diags = check("if (m) { if (cond) {} let maybe: M | undefined = m; m = maybe; m.id; }");
    assert_eq!(
        diagnostic_count(&diags, 18048),
        0,
        "an annotated alias initialized from narrowed m has current flow type M: {diags:?}"
    );
}

#[test]
fn direct_reassign_other_after_join_reflects_new_value() {
    let diags = check("declare const other: M; if (m) { if (cond) {} m = other; m.id; }");
    assert_eq!(
        diagnostic_count(&diags, 18048),
        0,
        "reassigning m to a definite M after the guard must reflect the new value (no TS18048): {diags:?}"
    );
}

// --- no-join control: already correct, must stay correct ---

#[test]
fn member_assign_without_join_no_ts18048() {
    let diags = check("if (m) { m.p = 1; m.id; }");
    assert_eq!(
        diagnostic_count(&diags, 18048),
        0,
        "member assignment with no intervening join must keep m narrowed: {diags:?}"
    );
}
