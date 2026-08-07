//! Structural rule: `tsc`'s `checkForInStatement` computes the LHS expression's
//! real type FIRST and only calls `checkReferenceExpression` (the source of
//! TS2780, "may not be an optional property access") when that type passes the
//! `isTypeAssignableTo(indexType, leftType)` check (TS2405, "must be of type
//! 'string' or 'any'") — see typescript-go's `checker.go`:
//!
//! ```go
//! } else if !c.isTypeAssignableTo(c.getIndexTypeOrString(rightType), leftType) {
//!     c.error(varExpr, diagnostics.The_left_hand_side_of_a_for_in_statement_must_be_of_type_string_or_any)
//! } else {
//!     c.checkReferenceExpression(varExpr, ..., diagnostics.The_left_hand_side_of_a_for_in_statement_may_not_be_an_optional_property_access)
//! }
//! ```
//!
//! So TS2405 and TS2780 are mutually exclusive for a `for...in` head: TS2405
//! wins whenever it fires, and TS2780 only fires once the LHS type already
//! passed. `for...of`'s analogous TS2781 check is NOT gated this way — tsc's
//! `checkForOfStatement` calls `checkReferenceExpression` unconditionally — so
//! that family is a regression guard here, not something this fix changes.
//!
//! The precedence decision computes the head's type as a plain read (matching
//! tsc's `checkExpression`), not through the assignment-target/write-target
//! path — that path short-circuits an optional-chain receiver to `any` for a
//! genuine write and carries its own nullish-receiver diagnostic, neither of
//! which belongs to this probe. The read is speculative: any diagnostic the
//! type computation itself would raise along the chain (e.g. on a receiver
//! whose own type is `any`, or on a non-optional continuation past an
//! optional link) is not this check's diagnostic to emit — the unconditional
//! real read later in `check_for_in_of_expression_initializer` owns that.
//!
//! All diagnostics oracle-verified against the pinned `typescript@7.0.2`
//! (`--noEmit --strict --lib es2022 --target es2022`). Binder names vary
//! across cases per the repo's anti-hardcoding discipline.

use crate::test_utils::check_source_strict_codes;

// =========================================================================
// TS2405 wins: the optional-chain LHS's real type is not string/any.
// =========================================================================

#[test]
fn for_in_optional_receiver_required_member_number_leaf_emits_only_ts2405() {
    // `b` is optional, `c` is required: the chain's real type is
    // `number | undefined`, not assignable to `string | any` — TS2405, no TS2780.
    let codes = check_source_strict_codes(
        "declare const outer: { mid?: { inner: { leaf: number } } };\nfor (outer.mid?.inner.leaf in {}) {}\n",
    );
    assert!(
        codes.contains(&2405),
        "possibly-undefined non-string LHS must emit TS2405; got: {codes:?}"
    );
    assert!(
        !codes.contains(&2780),
        "TS2405 must suppress TS2780 for a for-in optional-chain head; got: {codes:?}"
    );
}

#[test]
fn for_in_top_level_optional_access_number_typed_emits_only_ts2405() {
    let codes = check_source_strict_codes(
        "declare const rec: { count?: number };\nfor (rec?.count in {}) {}\n",
    );
    assert!(codes.contains(&2405), "expected TS2405; got: {codes:?}");
    assert!(!codes.contains(&2780), "expected no TS2780; got: {codes:?}");
}

// =========================================================================
// TS2780 wins: the optional-chain LHS's real type IS string/any, so the
// reference-validity check runs and reports the chain itself.
// =========================================================================

#[test]
fn for_in_optional_receiver_string_leaf_emits_only_ts2780() {
    let codes = check_source_strict_codes(
        "declare const holder: { maybe?: { text: string } };\nfor (holder.maybe?.text in {}) {}\n",
    );
    assert!(
        codes.contains(&2780),
        "string-typed optional-chain LHS must emit TS2780; got: {codes:?}"
    );
    assert!(
        !codes.contains(&2405),
        "a string-typed LHS passes the TS2405 check; got: {codes:?}"
    );
}

#[test]
fn for_in_top_level_optional_access_string_typed_emits_only_ts2780() {
    let codes = check_source_strict_codes(
        "declare const bag: { label?: string };\nfor (bag?.label in {}) {}\n",
    );
    assert!(codes.contains(&2780), "expected TS2780; got: {codes:?}");
    assert!(!codes.contains(&2405), "expected no TS2405; got: {codes:?}");
}

#[test]
fn for_in_any_typed_optional_chain_receiver_emits_only_ts2780() {
    // Regression guard for the exact shape that broke the first reland
    // attempt (#16660/#16667): a receiver typed `any` (`declare const obj:
    // any;`) makes every link of the chain `any`, so the precedence probe
    // must not synthesize a TS2339/TS7053 anywhere along the chain — it must
    // see `any`, pass the TS2405 check, and land on TS2780 alone.
    let codes = check_source_strict_codes("declare const obj: any;\nfor (obj?.a in {}) {}\n");
    assert!(codes.contains(&2780), "expected TS2780; got: {codes:?}");
    assert!(!codes.contains(&2405), "expected no TS2405; got: {codes:?}");
    assert!(
        !codes.contains(&2339) && !codes.contains(&7053),
        "an `any`-typed receiver must not leak property-lookup diagnostics \
         from the precedence probe; got: {codes:?}"
    );
}

#[test]
fn for_in_any_typed_optional_chain_with_trailing_access_emits_only_ts2780() {
    let codes = check_source_strict_codes("declare const obj: any;\nfor (obj?.a.b in {}) {}\n");
    assert!(codes.contains(&2780), "expected TS2780; got: {codes:?}");
    assert!(!codes.contains(&2405), "expected no TS2405; got: {codes:?}");
    assert!(
        !codes.contains(&2339) && !codes.contains(&7053),
        "an `any`-typed receiver must not leak property-lookup diagnostics \
         from the precedence probe; got: {codes:?}"
    );
}

#[test]
fn for_in_any_typed_optional_chain_element_access_emits_only_ts2780() {
    let codes = check_source_strict_codes("declare const obj: any;\nfor (obj?.[\"a\"] in {}) {}\n");
    assert!(codes.contains(&2780), "expected TS2780; got: {codes:?}");
    assert!(!codes.contains(&2405), "expected no TS2405; got: {codes:?}");
    assert!(
        !codes.contains(&2339) && !codes.contains(&7053),
        "an `any`-typed receiver must not leak property-lookup diagnostics \
         from the precedence probe; got: {codes:?}"
    );
}

// =========================================================================
// Regression guards: unaffected shapes.
// =========================================================================

#[test]
fn for_in_non_optional_property_access_string_leaf_reports_neither() {
    let codes = check_source_strict_codes(
        "declare const plain: { nested: { name: string } };\nfor (plain.nested.name in {}) {}\n",
    );
    assert!(
        !codes.contains(&2405) && !codes.contains(&2780),
        "a non-optional string-typed LHS is valid; got: {codes:?}"
    );
}

#[test]
fn for_in_non_optional_property_access_number_leaf_emits_only_ts2405() {
    // No optional chain at all: TS2405 alone must still fire for a non-string type.
    let codes = check_source_strict_codes(
        "declare const box: { field: { count: number } };\nfor (box.field.count in {}) {}\n",
    );
    assert!(codes.contains(&2405), "expected TS2405; got: {codes:?}");
    assert!(!codes.contains(&2780), "expected no TS2780; got: {codes:?}");
}

#[test]
fn for_of_optional_chain_still_emits_ts2781_unconditionally() {
    // for...of's analogous check is NOT gated behind an assignability check —
    // tsc reports TS2781 regardless of the chain's element type. This fix only
    // changes for...in's ordering, so for...of keeps its prior behavior.
    let codes = check_source_strict_codes(
        "declare const outer: { mid?: { inner: { leaf: number } } };\nfor (outer.mid?.inner.leaf of []) {}\n",
    );
    assert!(
        codes.contains(&2781),
        "for-of optional-chain LHS must still emit TS2781; got: {codes:?}"
    );
}

#[test]
fn for_of_any_typed_optional_chain_still_emits_ts2781_only() {
    let codes = check_source_strict_codes("declare const obj: any;\nfor (obj?.a of []) {}\n");
    assert!(
        codes.contains(&2781),
        "for-of optional-chain LHS must still emit TS2781; got: {codes:?}"
    );
    assert!(
        !codes.contains(&2339) && !codes.contains(&7053),
        "for-of's unconditional check must not leak property-lookup \
         diagnostics either; got: {codes:?}"
    );
}

#[test]
fn plain_optional_chain_assignment_target_still_emits_ts2779() {
    // Regression guard for the write-target family (`=`), which is unrelated
    // to for-in/for-of and must keep short-circuiting to `any` unconditionally.
    let codes = check_source_strict_codes(
        "declare const holder: { maybe?: { text: string } };\nholder.maybe?.text = \"x\";\n",
    );
    assert!(
        codes.contains(&2779),
        "plain optional-chain assignment target must still emit TS2779; got: {codes:?}"
    );
}

#[test]
fn renamed_binders_do_not_change_the_outcome() {
    let codes = check_source_strict_codes(
        "declare const zq: { al?: { be: { ga: number } } };\nfor (zq.al?.be.ga in {}) {}\n",
    );
    assert!(codes.contains(&2405), "expected TS2405; got: {codes:?}");
    assert!(!codes.contains(&2780), "expected no TS2780; got: {codes:?}");
}
