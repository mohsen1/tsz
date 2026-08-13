//! `for...in` optional-chain head: TS2405 (must be `string`/`any`) vs TS2780
//! (may not be an optional property access).
//!
//! Structural rule: `tsc`'s `checkForInStatement` computes the head
//! expression's real type FIRST — via `checkExpression` (a READ, not a
//! write-target resolution) — and only calls `checkReferenceExpression` (the
//! source of TS2780) in the `else` branch of the type check:
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
//! wins whenever the head's type is not string/any, and TS2780 only fires once
//! that type check passes. The discriminating control is the **string-LHS**
//! row: precedence keys on the LHS *type*, not on "is an optional chain".
//!
//! `for...of`'s analogous TS2781 check is NOT gated this way — tsc's
//! `checkForOfStatement` calls `checkReferenceExpression` unconditionally — so
//! that family is a regression guard here, not something this fix changes.
//!
//! The head type is read through the value path precisely so the chain's
//! property/element lookups do not leak the spurious TS2339/TS7053 that the
//! write-target path produced (the leak that reverted #16660). These tests
//! therefore include element-access chains and deeper nesting as leak guards.
//!
//! All diagnostics oracle-verified against the pinned `typescript@7.0.2`
//! (`--strict --target es2022`). Binder names vary across cases per the repo's
//! anti-hardcoding discipline.

use tsz_checker::test_utils::check_source_strict_codes;

// =========================================================================
// TS2405 wins: the optional-chain head's real type is not string/any.
// =========================================================================

#[test]
fn for_in_optional_receiver_required_member_number_leaf_emits_only_ts2405() {
    // `mid` is optional, `inner`/`leaf` required: the chain's real type is
    // `number | undefined`, not assignable to `string` — TS2405, no TS2780.
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
fn for_in_renamed_binders_number_leaf_emits_only_ts2405() {
    // Same shape, completely different identifiers — the decision must be
    // structural, never keyed on any particular name.
    let codes = check_source_strict_codes(
        "declare const zq: { al?: { be: { ga: number } } };\nfor (zq.al?.be.ga in {}) {}\n",
    );
    assert!(codes.contains(&2405), "expected TS2405; got: {codes:?}");
    assert!(!codes.contains(&2780), "expected no TS2780; got: {codes:?}");
}

#[test]
fn for_in_top_level_optional_access_number_typed_emits_only_ts2405() {
    let codes = check_source_strict_codes(
        "declare const rec: { count?: number };\nfor (rec?.count in {}) {}\n",
    );
    assert!(codes.contains(&2405), "expected TS2405; got: {codes:?}");
    assert!(!codes.contains(&2780), "expected no TS2780; got: {codes:?}");
}

#[test]
fn for_in_optional_element_access_number_leaf_emits_only_ts2405() {
    // Element-access chain form (mirrors elementAccessChain.3.ts): the leak
    // guard for spurious TS7053 on the value read of a bracketed access.
    let codes = check_source_strict_codes(
        "declare const store: { slot?: { cell: { value: number } } };\nfor (store.slot?.[\"cell\"][\"value\"] in {}) {}\n",
    );
    assert!(codes.contains(&2405), "expected TS2405; got: {codes:?}");
    assert!(!codes.contains(&2780), "expected no TS2780; got: {codes:?}");
    assert!(
        !codes.contains(&7053),
        "value-path read must not leak TS7053 for a valid element-access chain; got: {codes:?}"
    );
}

// =========================================================================
// TS2780 wins: the head's real type IS string/any, so the reference-validity
// check runs and reports the chain itself.
// =========================================================================

#[test]
fn for_in_optional_receiver_string_leaf_emits_only_ts2780() {
    let codes = check_source_strict_codes(
        "declare const holder: { maybe?: { text: string } };\nfor (holder.maybe?.text in {}) {}\n",
    );
    assert!(
        codes.contains(&2780),
        "string-typed optional-chain head must emit TS2780; got: {codes:?}"
    );
    assert!(
        !codes.contains(&2405),
        "a string-typed head passes the TS2405 check; got: {codes:?}"
    );
    assert!(
        !codes.contains(&2339),
        "value-path read must not leak TS2339 for a valid chain; got: {codes:?}"
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
fn for_in_optional_element_access_string_leaf_emits_only_ts2780() {
    let codes = check_source_strict_codes(
        "declare const dict: { entry?: { name: string } };\nfor (dict.entry?.[\"name\"] in {}) {}\n",
    );
    assert!(codes.contains(&2780), "expected TS2780; got: {codes:?}");
    assert!(!codes.contains(&2405), "expected no TS2405; got: {codes:?}");
    assert!(
        !codes.contains(&7053),
        "value-path read must not leak TS7053; got: {codes:?}"
    );
}

#[test]
fn for_in_optional_any_leaf_emits_only_ts2780() {
    // An `any`-typed leaf passes the TS2405 type check — TS2780 owns the head.
    let codes = check_source_strict_codes(
        "declare const anyBag: { blob?: { payload: any } };\nfor (anyBag.blob?.payload in {}) {}\n",
    );
    assert!(codes.contains(&2780), "expected TS2780; got: {codes:?}");
    assert!(!codes.contains(&2405), "expected no TS2405; got: {codes:?}");
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
        "a non-optional string-typed head is valid; got: {codes:?}"
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
        "for-of optional-chain head must still emit TS2781; got: {codes:?}"
    );
    assert!(
        !codes.contains(&2405),
        "for-of never routes through the TS2405 type gate; got: {codes:?}"
    );
}

#[test]
fn for_of_optional_chain_string_leaf_still_emits_ts2781() {
    // The string-LHS control on the for...of side: still TS2781 (unconditional),
    // never TS2780/TS2405, confirming the gating change is for...in-only.
    let codes = check_source_strict_codes(
        "declare const holder: { maybe?: { text: string } };\nfor (holder.maybe?.text of []) {}\n",
    );
    assert!(codes.contains(&2781), "expected TS2781; got: {codes:?}");
    assert!(!codes.contains(&2780), "expected no TS2780; got: {codes:?}");
    assert!(!codes.contains(&2405), "expected no TS2405; got: {codes:?}");
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
    assert!(
        !codes.contains(&2405) && !codes.contains(&2780),
        "an assignment target is neither a for-in nor a for-of head; got: {codes:?}"
    );
}

#[test]
fn for_in_head_ignores_narrowing_from_prior_invalid_optional_chain_assignment() {
    // `obj?.a = 1` is a hard error (TS2779) and records no narrowing, so the
    // later `for (obj?.a in {})` head keeps `obj?.a`'s declared type (`any`),
    // selecting TS2780 — not TS2405 off a bogus `1`-narrowed value. This is the
    // discriminating shape of `propertyAccessChain.3.ts`. `obj` is `any`, so a
    // correct read is `any`; only a spurious assignment-narrowing turns it to a
    // number.
    let codes =
        check_source_strict_codes("declare const obj: any;\nobj?.a = 1;\nfor (obj?.a in {}) {}\n");
    assert!(
        codes.contains(&2780),
        "an `any` head must select TS2780, ignoring the invalid prior assignment; got: {codes:?}"
    );
    assert!(
        !codes.contains(&2405),
        "the invalid optional-chain assignment must not narrow the head to a number; got: {codes:?}"
    );
    assert!(
        codes.contains(&2779),
        "the invalid optional-chain assignment target itself still emits TS2779; got: {codes:?}"
    );
}

#[test]
fn for_in_head_ignores_narrowing_from_prior_deep_optional_chain_assignment() {
    // The continuation-chain spelling (`obj?.a.b`) is likewise not narrowed by
    // its own invalid assignment — both `propertyAccessChain.3.ts` for-in heads
    // must land on TS2780.
    let codes = check_source_strict_codes(
        "declare const obj: any;\nobj?.a.b = 1;\nfor (obj?.a.b in {}) {}\n",
    );
    assert!(codes.contains(&2780), "expected TS2780; got: {codes:?}");
    assert!(!codes.contains(&2405), "expected no TS2405; got: {codes:?}");
}

#[test]
fn for_in_optional_chain_increment_target_still_emits_ts2777() {
    // The increment/decrement family (`++`) is likewise unconditional and
    // unaffected by the for-in type gate.
    let codes = check_source_strict_codes(
        "declare const holder: { maybe?: { text: number } };\nholder.maybe?.text++;\n",
    );
    assert!(
        codes.contains(&2777),
        "optional-chain increment target must still emit TS2777; got: {codes:?}"
    );
}
