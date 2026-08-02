//! The `noImplicitAny` class-member family: the missing get-accessor arm
//! (`TS7033`) and the surface rule that decides when the family is suppressed.
//!
//! Structural rule, one sentence per half:
//!
//! 1. When a `get` accessor has no body and no return-type annotation, `tsc`
//!    reports `TS7033` — the accessor analogue of the bodyless-method `TS7010`
//!    arm — unless a paired `set` accessor's annotated parameter supplies the
//!    type (`isGetAccessorWithAnnotatedSetAccessor`). tsz does this through
//!    `state_checking_members/ambient_signature_checks.rs`
//!    (`check_accessor_declaration_with_request`).
//! 2. When a class member is hidden from the observable surface of an ambient
//!    declaration — `private`, or named by a private identifier, *and* inside a
//!    `declare class` or a `.d.ts` — `tsc` reports none of the family for it.
//!    tsz does this through `member_hidden_from_ambient_declaration_surface`.
//!
//! Neither condition of the conjunction in (2) suppresses on its own: an
//! ordinary-named ambient member still reports, and a private-identifier member
//! of a *non-ambient* class still reports. Both directions are pinned below,
//! because a guard keyed on either condition alone passes half of this file.
//!
//! Every expectation here was recorded from `typescript@7.0.2` under
//! `--noEmit --strict --lib es2022 --target es2022`.

use tsz_checker::test_utils::{check_source_codes_named, check_source_strict_codes};

const TS7008: u32 = 7008; // Member implicitly has an 'any' type.
const TS7010: u32 = 7010; // Lacks return-type annotation, implicitly 'any' return.
const TS7033: u32 = 7033; // Property implicitly 'any', get accessor lacks return type.
const TS7006: u32 = 7006; // Parameter implicitly has an 'any' type.
const TS7032: u32 = 7032; // Property implicitly 'any', set accessor lacks a param type.

fn has(codes: &[u32], code: u32) -> bool {
    codes.contains(&code)
}

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

// ---------------------------------------------------------------------------
// (1) TS7033 — the arm that did not exist. Issue #16179.
// ---------------------------------------------------------------------------

#[test]
fn ambient_class_bodyless_getter_reports_ts7033() {
    let codes = check_source_strict_codes("declare class J { get g(); }");
    assert!(has(&codes, TS7033), "expected TS7033: {codes:?}");
}

#[test]
fn ambient_class_annotated_getter_is_clean() {
    let codes = check_source_strict_codes("declare class J { get g(): number; }");
    assert!(
        !has(&codes, TS7033),
        "annotation supplies the type: {codes:?}"
    );
}

#[test]
fn abstract_getter_without_annotation_reports_ts7033() {
    // A bodyless getter is legal outside an ambient context too.
    let codes = check_source_strict_codes("abstract class A { abstract get g(); }");
    assert!(has(&codes, TS7033), "expected TS7033: {codes:?}");
}

#[test]
fn abstract_annotated_getter_is_clean() {
    let codes = check_source_strict_codes("abstract class A { abstract get g(): number; }");
    assert!(
        !has(&codes, TS7033),
        "annotation supplies the type: {codes:?}"
    );
}

#[test]
fn ambient_static_bodyless_getter_reports_ts7033() {
    let codes = check_source_strict_codes("declare class J { static get g(); }");
    assert!(
        has(&codes, TS7033),
        "static is not part of the rule: {codes:?}"
    );
}

#[test]
fn getter_paired_with_annotated_setter_is_clean() {
    // The exemption the bodyless-method TS7010 arm does not need, and the one
    // that matters most in practice: `get g(); set g(v: T);` is ordinary in real
    // declaration files, and tsc reports nothing for it.
    let codes = check_source_strict_codes("declare class J { get g(); set g(v: number); }");
    assert!(
        !has(&codes, TS7033),
        "paired annotated setter supplies the getter's type: {codes:?}"
    );
}

// --- the pairing rule: who gets blamed when neither side names a type -------
//
// A get/set pair shares ONE property type. It comes from the getter's return
// type (annotation, or inferred from a body) if there is one, else from the
// setter's parameter annotation. When nothing supplies it, `tsc` blames the
// *setter* with TS7032 — the getter is only the blame site when it has no
// paired setter at all. Issue #16183.

#[test]
fn unannotated_pair_blames_the_setter_not_the_getter() {
    // tsc: TS7032 on the setter name, and no TS7033.
    let codes = check_source_strict_codes("declare class A { get g(); set g(v); }");
    assert!(
        has(&codes, TS7032),
        "expected TS7032 on the setter: {codes:?}"
    );
    assert!(
        !has(&codes, TS7033),
        "a getter with any paired setter is never the blame site: {codes:?}"
    );
}

#[test]
fn unannotated_pair_blames_the_setter_in_declaration_order_too() {
    // Same pair, setter declared first — pairing is by name, not by order.
    let codes = check_source_strict_codes("declare class A { set g(v); get g(); }");
    assert!(has(&codes, TS7032), "expected TS7032: {codes:?}");
    assert!(
        !has(&codes, TS7033),
        "no TS7033 on the paired getter: {codes:?}"
    );
}

#[test]
fn unannotated_static_pair_blames_the_setter() {
    let codes = check_source_strict_codes("declare class A { static get g(); static set g(v); }");
    assert!(has(&codes, TS7032), "expected TS7032: {codes:?}");
    assert!(!has(&codes, TS7033), "no TS7033: {codes:?}");
}

#[test]
fn unannotated_pair_does_not_report_ts7006_on_the_parameter() {
    // The paired getter still contextually types the parameter, so TS7006 stays
    // suppressed even though TS7032 fires. These two suppressions are different
    // questions and a single flag cannot answer both.
    let codes = check_source_strict_codes("declare class A { get g(); set g(v); }");
    assert!(
        !has(&codes, TS7006),
        "paired getter contextually types the param: {codes:?}"
    );
}

#[test]
fn annotated_getter_supplies_the_pair_type() {
    let codes = check_source_strict_codes("declare class A { get g(): number; set g(v); }");
    assert!(
        !has(&codes, TS7032),
        "getter annotation supplies it: {codes:?}"
    );
    assert!(
        !has(&codes, TS7033),
        "getter annotation supplies it: {codes:?}"
    );
}

#[test]
fn getter_with_a_body_supplies_the_pair_type() {
    // The common non-ambient shape. The getter's body infers `number`, so the
    // pair has a type and neither side reports — this is the row a naive
    // "any paired getter without an annotation means TS7032" rule would break.
    let codes = check_source_strict_codes("class B { get g() { return 1; } set g(v) {} }");
    assert!(!has(&codes, TS7032), "inferred from the body: {codes:?}");
    assert!(!has(&codes, TS7033), "inferred from the body: {codes:?}");
}

#[test]
fn lone_unannotated_setter_reports_both_ts7032_and_ts7006() {
    // No getter to contextually type the parameter, so both fire.
    let codes = check_source_strict_codes("declare class A { set s(v); }");
    assert!(has(&codes, TS7032), "expected TS7032: {codes:?}");
    assert!(has(&codes, TS7006), "expected TS7006: {codes:?}");
}

#[test]
fn any_annotation_on_the_setter_supplies_the_pair_type() {
    // It is the annotation's *presence* that matters, not its type — `any`
    // written explicitly is not an implicit any.
    let codes = check_source_strict_codes("declare class A { get g(); set g(v: any); }");
    assert!(!has(&codes, TS7032), "explicit annotation: {codes:?}");
    assert!(!has(&codes, TS7033), "explicit annotation: {codes:?}");
}

#[test]
fn hidden_ambient_pair_stays_clean_through_the_pairing_rule() {
    // The surface rule from #16178 still wins over the pairing rule.
    let codes = check_source_strict_codes("declare class A { get #g(); set #g(v); }");
    assert!(!has(&codes, TS7032), "hidden from the surface: {codes:?}");
    assert!(!has(&codes, TS7033), "hidden from the surface: {codes:?}");
}

#[test]
fn setter_without_paired_getter_never_reports_ts7033() {
    // TS7033 is a *get* accessor diagnostic; a lone setter is not in scope.
    let codes = check_source_strict_codes("declare class J { set s(v: number); }");
    assert!(!has(&codes, TS7033), "setter-only: {codes:?}");
}

#[test]
fn ambient_class_reports_all_three_family_codes_together() {
    // tsc: TS7008 (x), TS7010 (m), TS7033 (g) — the three arms are independent
    // and co-emit on one class.
    let codes = check_source_strict_codes("declare class D { x; m(); get g(); }");
    assert!(has(&codes, TS7008), "expected TS7008: {codes:?}");
    assert!(has(&codes, TS7010), "expected TS7010: {codes:?}");
    assert!(has(&codes, TS7033), "expected TS7033: {codes:?}");
}

#[test]
fn ts7033_is_reported_once_per_getter() {
    let codes = check_source_strict_codes("declare class D { get g(); }");
    assert_eq!(count(&codes, TS7033), 1, "exactly one TS7033: {codes:?}");
}

// ---------------------------------------------------------------------------
// (1b) Computed names — the class arm did not resolve a display name for a
// computed getter/setter whose expression is not a literal, so the TS7033
// site's `get_property_name` returned `None` and silently suppressed the
// diagnostic. Issue #16186 (the residual left by #16188, which fixed the
// interface/type-literal arm the same way). tsc still reports, using the
// raw `[expr]` source text as the display name (`declarationNameToString`).
// ---------------------------------------------------------------------------

#[test]
fn computed_unique_symbol_getter_reports_ts7033() {
    let codes =
        check_source_strict_codes("declare const k: unique symbol; declare class C { get [k](); }");
    assert!(has(&codes, TS7033), "expected TS7033: {codes:?}");
}

#[test]
fn computed_unique_symbol_pair_still_blames_the_setter() {
    // Control: the TS7032 sibling site already used the computed-aware
    // helper, so this direction must be unaffected by the TS7033 fix.
    let codes = check_source_strict_codes(
        "declare const k: unique symbol; declare class C { get [k](); set [k](v); }",
    );
    assert!(has(&codes, TS7032), "expected TS7032: {codes:?}");
    assert!(!has(&codes, TS7033), "setter is the blame site: {codes:?}");
}

#[test]
fn computed_non_literal_typed_getter_reports_ts7033() {
    // `k`'s type is plain `string`, not a literal or unique symbol — tsc
    // cannot resolve a concrete property key either, and still reports
    // using the raw source text as the display name.
    let codes =
        check_source_strict_codes("declare const k: string; declare class C { get [k](); }");
    assert!(has(&codes, TS7033), "expected TS7033: {codes:?}");
}

#[test]
fn computed_well_known_symbol_getter_reports_ts7033() {
    let codes = check_source_strict_codes("declare class C { get [Symbol.iterator](); }");
    assert!(has(&codes, TS7033), "expected TS7033: {codes:?}");
}

#[test]
fn computed_getter_in_abstract_class_reports_ts7033() {
    let codes = check_source_strict_codes(
        "declare const k: unique symbol; abstract class C { abstract get [k](); }",
    );
    assert!(has(&codes, TS7033), "expected TS7033: {codes:?}");
}

#[test]
fn computed_string_literal_getter_still_reports_ts7033() {
    // Control: a computed name whose expression IS a string literal already
    // resolved through `get_property_name`'s literal fast path before this
    // fix — must keep working unchanged.
    let codes = check_source_strict_codes("declare class C { get [\"foo\"](); }");
    assert!(has(&codes, TS7033), "expected TS7033: {codes:?}");
}

#[test]
fn computed_private_hidden_getter_stays_clean() {
    // The surface rule (private/`.d.ts`-hidden) must still win over the
    // computed-name display fix — a hidden member reports nothing at all,
    // so the computed-name display fallback must never be reached for it.
    let codes = check_source_strict_codes(
        "declare const k: unique symbol; declare class C { private get [k](); }",
    );
    assert!(!has(&codes, TS7033), "hidden from the surface: {codes:?}");
}

#[test]
fn malformed_computed_getter_name_does_not_report_ts7033() {
    // `[]` and `[1+]` are parse errors (empty / incomplete computed-name
    // expression) — tsc reports only the syntax error and stays silent on
    // `TS7033`. The fix must resolve computed names through the *structured*
    // display helper (identifier / property-access / literal), not through a
    // raw source-text slice — a slice would render `[]`/`[1+]` as a "name"
    // and start reporting a semantic diagnostic tsc never emits here.
    let codes = check_source_strict_codes("declare class C { get [](); }");
    assert!(!has(&codes, TS7033), "parse-error name: {codes:?}");
    let codes = check_source_strict_codes("declare class C { get [1+](); }");
    assert!(!has(&codes, TS7033), "parse-error name: {codes:?}");
}

// ---------------------------------------------------------------------------
// (1c) Adjacent-case matrix for issue #16190 (the residual #16201 left
// unverified): `static`, a real `.d.ts` container, and a substituted template
// literal computed name all reached the same `check_accessor_declaration_with_request`
// bodyless-getter arm — none of them gate on the name's *kind*, so the first
// two already passed before this change (pinned here as controls). The
// template-literal case did not: a `TemplateExpression` computed name is not
// a literal `get_property_name` can resolve, and — unlike `[k]` or
// `[Symbol.iterator]` — `computed_name_expression_display_text` had no arm
// for it either, so the whole `member_name_for_diagnostic` call failed and
// silently suppressed TS7033. Fixed by rendering the template expression's
// verbatim source text, matching tsc's `declarationNameToString` fallback.
// Verified against the pinned `typescript@7.0.2` oracle.
// ---------------------------------------------------------------------------

#[test]
fn static_computed_unique_symbol_getter_reports_ts7033() {
    let codes = check_source_strict_codes(
        "declare const k: unique symbol; declare class C { static get [k](); }",
    );
    assert!(has(&codes, TS7033), "expected TS7033: {codes:?}");
}

#[test]
fn dts_container_computed_unique_symbol_getter_reports_ts7033() {
    // A real `.d.ts` file is implicitly ambient — no `declare` keyword needed
    // on the class — exercising the `is_declaration_file()` half of the
    // ambient-context check rather than `enclosing_class.is_declared`.
    let codes = check_source_codes_named(
        "declare const k: unique symbol; class C { get [k](); }",
        "test.d.ts",
    );
    assert!(has(&codes, TS7033), "expected TS7033: {codes:?}");
}

#[test]
fn computed_template_expression_getter_reports_ts7033() {
    let codes =
        check_source_strict_codes("declare const x: string; declare class C { get [`a${x}`](); }");
    assert!(has(&codes, TS7033), "expected TS7033: {codes:?}");
}

#[test]
fn computed_template_expression_getter_and_setter_both_report_independently() {
    // Unlike `[k]` (a `unique symbol` reference, paired by resolved symbol
    // identity) or a string literal (paired by resolved key), tsc's binder
    // does not pair two computed accessors whose name is a *non-constant*
    // template expression — each is bound as its own unrelated member, so
    // both the getter's TS7033 and the setter's TS7032/TS7006 fire together,
    // confirmed against the `typescript@7.0.2` oracle. This is the inverse
    // control of `computed_unique_symbol_pair_still_blames_the_setter`: the
    // pairing helpers (`paired_setter_in_members`/`paired_getter_in_members`)
    // correctly fail to match here since neither `get_property_name` nor
    // `resolve_computed_name_symbol` can key a template expression, and that
    // failure is the *correct* answer, not a gap this fix needs to close.
    let codes = check_source_strict_codes(
        "declare const x: string; declare class C { get [`a${x}`](); set [`a${x}`](v); }",
    );
    assert!(
        has(&codes, TS7033),
        "expected TS7033 on the getter: {codes:?}"
    );
    assert!(
        has(&codes, TS7032),
        "expected TS7032 on the setter: {codes:?}"
    );
    assert!(
        has(&codes, TS7006),
        "expected TS7006 on the parameter: {codes:?}"
    );
}

#[test]
fn computed_template_expression_property_reports_ts7008() {
    // Bonus reach of the same shared display-name fix: `check_class_property`'s
    // TS7008 site (line ~550 in ambient_signature_checks.rs) uses the same
    // `get_member_name_display_text` helper for the identical reason — a
    // template-expression computed name is not a `get_property_name` literal.
    // Confirmed against the `typescript@7.0.2` oracle: tsc reports both TS1166
    // (computed class-property name needs a literal/`unique symbol` type) and
    // TS7008 here; this test pins only the noImplicitAny half this fix owns.
    let codes =
        check_source_strict_codes("declare const x: string; declare class C { [`a${x}`]; }");
    assert!(has(&codes, TS7008), "expected TS7008: {codes:?}");
}

// ---------------------------------------------------------------------------
// (2) The surface rule. Issue #16178 — TS7010 fired where tsc is silent.
// ---------------------------------------------------------------------------

#[test]
fn ambient_private_named_method_is_clean() {
    // The #16178 witness.
    let codes = check_source_strict_codes("declare class C { #m(); }");
    assert!(!has(&codes, TS7010), "hidden from the surface: {codes:?}");
}

#[test]
fn ambient_private_named_property_is_clean() {
    let codes = check_source_strict_codes("declare class C { #x; }");
    assert!(!has(&codes, TS7008), "hidden from the surface: {codes:?}");
}

#[test]
fn ambient_private_named_getter_is_clean() {
    // The new TS7033 arm must respect the same surface rule it was added under,
    // or fixing #16179 would have re-opened #16178 through a different code.
    let codes = check_source_strict_codes("declare class C { get #g(); }");
    assert!(!has(&codes, TS7033), "hidden from the surface: {codes:?}");
}

#[test]
fn ambient_static_private_named_method_is_clean() {
    let codes = check_source_strict_codes("declare class C { static #m(); }");
    assert!(!has(&codes, TS7010), "hidden from the surface: {codes:?}");
}

#[test]
fn ambient_private_modifier_members_stay_clean() {
    // Pre-existing behavior, kept as a control: the `private` keyword is the
    // other way a member leaves the surface, and it must keep working.
    let codes = check_source_strict_codes("declare class F { private m(); private get g(); }");
    assert!(!has(&codes, TS7010), "private keyword: {codes:?}");
    assert!(!has(&codes, TS7033), "private keyword: {codes:?}");
}

#[test]
fn ambient_whole_private_named_class_is_clean() {
    // Renamed binders throughout — no identifier string drives the rule.
    let codes = check_source_strict_codes("declare class Zebra { #alpha; #beta(); get #gamma(); }");
    assert!(!has(&codes, TS7008), "renamed binders: {codes:?}");
    assert!(!has(&codes, TS7010), "renamed binders: {codes:?}");
    assert!(!has(&codes, TS7033), "renamed binders: {codes:?}");
}

// --- negative controls: neither half of the conjunction suppresses alone -----

#[test]
fn non_ambient_private_named_method_still_reports_ts7010() {
    // A `#m()` outside an ambient context is NOT hidden — its implicit `any`
    // still affects the enclosing class body's inferred type. A guard keyed on
    // the private-identifier name alone would wrongly silence this.
    let codes = check_source_strict_codes("class E { #m(); }");
    assert!(has(&codes, TS7010), "non-ambient private name: {codes:?}");
}

#[test]
fn non_ambient_abstract_private_named_method_still_reports_ts7010() {
    let codes = check_source_strict_codes("abstract class G { abstract #m(); }");
    assert!(has(&codes, TS7010), "non-ambient private name: {codes:?}");
}

#[test]
fn ambient_ordinary_named_members_still_report() {
    // An ambient declaration *is* the public API for ordinary names. A guard
    // keyed on the ambient context alone would wrongly silence all of these.
    let codes = check_source_strict_codes("declare class D { m(); }");
    assert!(has(&codes, TS7010), "ordinary ambient name: {codes:?}");
    let codes = check_source_strict_codes("declare class D { x; }");
    assert!(has(&codes, TS7008), "ordinary ambient name: {codes:?}");
    let codes = check_source_strict_codes("declare class D { get g(); }");
    assert!(has(&codes, TS7033), "ordinary ambient name: {codes:?}");
}

#[test]
fn ambient_private_named_annotated_members_are_clean_either_way() {
    // Annotated hidden members are clean for two independent reasons; pinned so
    // a later change to either reason does not silently start reporting.
    let codes = check_source_strict_codes("declare class C { get #g(): number; #m(): void; }");
    assert!(!has(&codes, TS7033), "annotated + hidden: {codes:?}");
    assert!(!has(&codes, TS7010), "annotated + hidden: {codes:?}");
}
