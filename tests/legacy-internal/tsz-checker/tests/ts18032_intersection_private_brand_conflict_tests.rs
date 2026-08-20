//! Regression tests for the `TS18032` related-info line `tsc` attaches to a
//! `TS2339` property access on an intersection reduced to `never` because of
//! a private-brand conflict — the sibling of `TS18031`
//! (`ts18031_intersection_conflicting_property_tests.rs`), which handles the
//! *literal-discriminant* reduction reason instead.
//!
//! Structural rule (pinned against `typescript@7.0.2`): when a *directly
//! written* object intersection (`declare const c: A & B`) has two or more
//! members that each declare a property of the same name, and at least one
//! of those declarations is modifier-`private`, `tsc`'s `getReducedType`
//! collapses the intersection to `never` (the private member's nominal brand
//! makes the property un-satisfiable across constituents, regardless of
//! whether the *types* agree) and any property access on it reports
//! `TS2339` with a `relatedInformation` entry: "The intersection '{0}' was
//! reduced to 'never' because property '{1}' exists in multiple constituents
//! and is private in some."
//!
//! Both modifier-`private` on both sides AND modifier-`private` on only one
//! side (paired with a structurally-identical `public` member) trigger this
//! — oracle-verified with an identical-type control, so the conflict is
//! genuinely about the private brand, not a type mismatch a different
//! diagnostic would already catch.
//!
//! Owned by the same `error_reporter/intersection_never_elaboration.rs`
//! helper as TS18031 (`intersection_reduced_to_never_related_info`), which
//! tries the literal-discriminant query first and falls back to
//! `find_private_brand_conflict_property`
//! (`tsz-solver/src/type_queries/data/intersection_conflict.rs`).
//!
//! Deliberately narrow scope, matching TS18031's own precedent: only a
//! directly-written intersection annotation (no alias/generic-application/
//! heritage indirection). Cases that don't reduce to `never` at all
//! (protected-only conflicts report `TS2445` instead; the same class
//! intersected with itself is not a conflict) are documented as negative
//! controls below — they never reach this helper's `type_id == NEVER` gate,
//! so they're regression coverage for the surrounding machinery, not for the
//! new query itself.
//!
//! ES `#`-private members are a separate case: `#x` on two different classes
//! is never the same name to `tsc` (each is lexically scoped to its own
//! class body), so real `tsc` neither reduces the intersection to `never`
//! nor elaborates. `find_private_brand_conflict_property` is scoped to never
//! attach the TS18032 line to an ES-private occurrence; three independent
//! never-reduction gates (`intersection_has_conflicting_private_brands` in
//! `crates/tsz-solver/src/intern/normalize.rs`,
//! `intersection_has_private_property_conflict` in
//! `crates/tsz-checker/src/state/state_checking_members/mixin_member_access.rs`,
//! and this file's own elaboration query) all now share the same
//! ES-private exclusion, so `A & B` with same-spelled `#x` members no
//! longer reduces to `never` at all.

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_strict;
use tsz_common::diagnostics::diagnostic_codes;

const TS2339: u32 = diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE;
const TS18032: u32 =
    diagnostic_codes::THE_INTERSECTION_WAS_REDUCED_TO_NEVER_BECAUSE_PROPERTY_EXISTS_IN_MULTIPLE_CONSTI;

fn only(diags: &[Diagnostic], code: u32) -> Diagnostic {
    let matching: Vec<_> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matching.len(),
        1,
        "expected exactly one TS{code}; got {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    matching[0].clone()
}

fn related(diagnostic: &Diagnostic, code: u32) -> Option<String> {
    diagnostic
        .related_information
        .iter()
        .find(|info| info.code == code)
        .map(|info| info.message_text.clone())
}

// ---------------------------------------------------------------------------
// Positive cases: a directly-written intersection with a private-brand
// conflict carries the TS18032 elaboration.
// ---------------------------------------------------------------------------

#[test]
fn both_sides_private_carry_ts18032_elaboration() {
    let diags = check_source_strict(
        r#"
class P1 { private x: string = ""; }
class P2 { private x: string = ""; }
declare const c: P1 & P2;
c.x;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18032).as_deref(),
        Some(
            "The intersection 'P1 & P2' was reduced to 'never' because property 'x' exists in multiple constituents and is private in some."
        ),
        "got {diags:?}"
    );
}

#[test]
fn one_side_private_one_side_public_still_conflicts() {
    // Identical property type on both sides — the conflict is the private
    // brand, not a structural type mismatch a different check would catch.
    let diags = check_source_strict(
        r#"
class P1 { private x: string = ""; }
class P2 { x: string = ""; }
declare const c: P1 & P2;
c.x;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18032).as_deref(),
        Some(
            "The intersection 'P1 & P2' was reduced to 'never' because property 'x' exists in multiple constituents and is private in some."
        ),
        "got {diags:?}"
    );
}

#[test]
fn renamed_binders_carry_ts18032_elaboration() {
    // Same rule, different identifiers — structural, not name-keyed.
    let diags = check_source_strict(
        r#"
class Alpha { private val: number = 0; }
class Beta { private val: number = 0; }
declare const combined: Alpha & Beta;
combined.val;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18032).as_deref(),
        Some(
            "The intersection 'Alpha & Beta' was reduced to 'never' because property 'val' exists in multiple constituents and is private in some."
        ),
        "got {diags:?}"
    );
}

#[test]
fn accessing_a_different_property_still_carries_ts18032() {
    let diags = check_source_strict(
        r#"
class WithSecret { private secret: string = ""; }
class AlsoSecret { private secret: string = ""; }
declare const anything: WithSecret & AlsoSecret;
anything.other;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18032).as_deref(),
        Some(
            "The intersection 'WithSecret & AlsoSecret' was reduced to 'never' because property 'secret' exists in multiple constituents and is private in some."
        ),
        "got {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Multi-conflict ordering: when more than one property qualifies (declared in
// two or more members, private in at least one), tsc names the first one by a
// combined declaration order — the same walk `elaborateNeverIntersection`
// uses for TS18031: members left to right, and within each member its own
// properties in declaration order, positioning each name at its *first*
// occurrence. The pick is independent of alphabetical order, of which property
// was accessed, and of a later member's own declaration order.
// ---------------------------------------------------------------------------

#[test]
fn multi_conflict_names_first_declared_private_property() {
    // Both `x` and `y` are private in both members; `x` is declared first, so
    // `x` wins even though the access is on `x` itself.
    let diags = check_source_strict(
        r#"
class A { private x: string = ""; private y: string = ""; }
class B { private x: string = ""; private y: string = ""; }
declare const c: A & B;
c.x;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18032).as_deref(),
        Some(
            "The intersection 'A & B' was reduced to 'never' because property 'x' exists in multiple constituents and is private in some."
        ),
        "got {diags:?}"
    );
}

#[test]
fn multi_conflict_order_follows_declaration_order_not_alphabetical() {
    // Source order is `zz` then `aa` (reverse of alphabetical); the winner is
    // `zz`, proving the pick is declaration order, not name order — the same
    // guarantee the sibling TS18031 rule makes.
    let diags = check_source_strict(
        r#"
class P { private zz: string = ""; private aa: string = ""; }
class Q { private zz: string = ""; private aa: string = ""; }
declare const r: P & Q;
r.zz;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18032).as_deref(),
        Some(
            "The intersection 'P & Q' was reduced to 'never' because property 'zz' exists in multiple constituents and is private in some."
        ),
        "got {diags:?}"
    );
}

#[test]
fn multi_conflict_order_ignores_second_members_declaration_order() {
    // `A` declares `y` before `x`; `B` declares `x` before `y`. tsc uses the
    // FIRST member's order, so `y` wins even though the access is on `x`.
    let diags = check_source_strict(
        r#"
class A { private y: string = ""; private x: string = ""; }
class B { private x: string = ""; private y: string = ""; }
declare const c: A & B;
c.x;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18032).as_deref(),
        Some(
            "The intersection 'A & B' was reduced to 'never' because property 'y' exists in multiple constituents and is private in some."
        ),
        "got {diags:?}"
    );
}

#[test]
fn multi_conflict_skips_non_conflicting_private_and_names_first_shared_private() {
    // `A`'s `solo` is private but declared in only one member — it never
    // conflicts, so it is skipped. `shared` (private in both) is the first
    // name that qualifies, even though `solo` is declared before it.
    let diags = check_source_strict(
        r#"
class A { private solo: string = ""; private shared: string = ""; }
class B { private shared: string = ""; }
declare const c: A & B;
c.shared;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18032).as_deref(),
        Some(
            "The intersection 'A & B' was reduced to 'never' because property 'shared' exists in multiple constituents and is private in some."
        ),
        "got {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative / control cases: no TS18032 elaboration.
// ---------------------------------------------------------------------------

#[test]
fn protected_only_conflict_does_not_reduce_to_never() {
    // tsc reports TS2445 (protected access) against the un-reduced
    // intersection type here, not TS2339 against `never` — this shape never
    // reaches the never-elaboration helper at all.
    let diags = check_source_strict(
        r#"
class P1 { protected x: string = ""; }
class P2 { protected x: string = ""; }
declare const c: P1 & P2;
c.x;
"#,
    );
    assert!(
        diags.iter().all(|d| d.code != TS2339),
        "protected-only conflicts must not reduce to never, got {diags:?}"
    );
}

#[test]
fn es_private_same_spelled_fields_do_not_carry_ts18032() {
    // ES `#`-private fields are lexically scoped to their declaring class —
    // `#x` on `A` and `#x` on `B` are never the same name to tsc even though
    // they share identical source text, so this is not a naming collision
    // tsc elaborates (tsc: `Property 'foo' does not exist on type 'A & B'.`,
    // no relatedInformation, no `never` reduction at all). This test pins
    // only the elaboration's own scope: no TS2339 here may carry the
    // TS18032 related-info line. `es_private_same_name_does_not_reduce_to_never`
    // below covers the reduction itself no longer firing for this shape.
    let diags = check_source_strict(
        r#"
class A { #x = 1; }
class B { #x = 1; }
declare const v: A & B;
v.foo;
"#,
    );
    for diag in diags.iter().filter(|d| d.code == TS2339) {
        assert!(related(diag, TS18032).is_none(), "got {diags:?}");
    }
}

#[test]
fn same_class_intersected_with_itself_has_no_conflict() {
    let diags = check_source_strict(
        r#"
class P1 { private x: string = ""; }
declare const c: P1 & P1;
c.x;
"#,
    );
    assert!(
        diags.iter().all(|d| d.code != TS2339),
        "a class intersected with itself must not reduce to never, got {diags:?}"
    );
}

#[test]
fn compatible_public_members_report_nothing() {
    let diags = check_source_strict(
        r#"
class Pub1 { x: string = ""; }
class Pub2 { x: string = ""; }
declare const c: Pub1 & Pub2;
c.x;
"#,
    );
    assert!(diags.is_empty(), "got {diags:?}");
}

#[test]
fn es_private_same_name_does_not_reduce_to_never() {
    // `#x` in `A` and `#x` in `B` are different, per-class-scoped names —
    // structurally nothing to conflict, unlike modifier-`private` `x`/`x`
    // above. Oracle-verified (`typescript@7.0.2`): 0 errors.
    let diags = check_source_strict(
        r#"
class A { #x = 1; m() { return 1; } }
class B { #x = 1; n() { return 2; } }
declare const v: A & B;
v.m();
v.n();
"#,
    );
    assert!(diags.is_empty(), "got {diags:?}");
}

#[test]
fn es_private_different_names_does_not_reduce_to_never() {
    let diags = check_source_strict(
        r#"
class A { #x = 1; m() { return 1; } }
class B { #y = 2; n() { return 2; } }
declare const v: A & B;
v.m();
v.n();
"#,
    );
    assert!(diags.is_empty(), "got {diags:?}");
}

#[test]
fn modifier_private_and_es_private_mixed_forms_do_not_conflict() {
    // `private x` (modifier) and `#x` (ES private) are different property
    // names at the type level (`x` vs `#x`) regardless of the shared
    // spelling after the sigil, so there is no shared name to conflict on.
    let diags = check_source_strict(
        r#"
class A { private x: number = 1; m() { return 1; } }
class B { #x = 1; n() { return 2; } }
declare const v: A & B;
v.m();
v.n();
"#,
    );
    assert!(diags.is_empty(), "got {diags:?}");
}

#[test]
fn es_private_shared_with_subclass_does_not_conflict() {
    let diags = check_source_strict(
        r#"
class A { #x = 1; m() { return 1; } }
class D extends A { n() { return 2; } }
declare const v: A & D;
v.m();
v.n();
"#,
    );
    assert!(diags.is_empty(), "got {diags:?}");
}

#[test]
fn three_way_intersection_mixing_private_kinds_does_not_conflict() {
    // A private modifier member, an ES-private member, and a protected
    // member with three distinct names: no shared name across any pair, so
    // no reduction — each kind must be excluded from the coarse brand check
    // independently of the others.
    let diags = check_source_strict(
        r#"
class A { private p: number = 1; m() { return 1; } }
class B { #x = 1; n() { return 2; } }
class C { protected r: number = 1; o() { return 3; } }
declare const v: A & B & C;
v.m();
v.n();
v.o();
"#,
    );
    assert!(diags.is_empty(), "got {diags:?}");
}

#[test]
fn modifier_private_conflict_still_reduces_with_es_private_present() {
    // The fix must not weaken the genuine modifier-`private` same-name
    // conflict just because an unrelated ES-private member is also present
    // in the intersection.
    let diags = check_source_strict(
        r#"
class P1 { private x: string = ""; }
class P2 { private x: string = ""; }
class B { #y = 1; }
declare const c: P1 & P2 & B;
c.x;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18032).as_deref(),
        Some(
            "The intersection 'P1 & P2 & B' was reduced to 'never' because property 'x' exists in multiple constituents and is private in some."
        ),
        "got {diags:?}"
    );
}

#[test]
fn alias_to_private_brand_intersection_carries_ts18032_naming_the_alias() {
    // Same alias support as TS18031: the walk follows the receiver's alias to
    // the private-brand-conflicting intersection behind it and names the alias
    // (`Combined`), matching `tsc`.
    let diags = check_source_strict(
        r#"
class P1 { private x: string = ""; }
class P2 { private x: string = ""; }
type Combined = P1 & P2;
declare const value: Combined;
value.x;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18032).as_deref(),
        Some(
            "The intersection 'Combined' was reduced to 'never' because property 'x' exists in multiple constituents and is private in some."
        ),
        "got {diags:?}"
    );
}
