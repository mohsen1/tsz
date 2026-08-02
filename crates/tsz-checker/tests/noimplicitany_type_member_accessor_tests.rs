//! The `noImplicitAny` accessor family (`TS7033`/`TS7032`/`TS7006`) for *type
//! members* — the members of an `interface` or of a type literal. Issue #16186.
//!
//! Structural rule, one sentence: a `get`/`set` pair shares **one** property
//! type, taken from the getter's return type (annotated, or inferred from a
//! body) if there is one and otherwise from the setter's parameter annotation,
//! and when nothing supplies it `tsc` blames the **setter** with `TS7032` — the
//! getter is the blame site (`TS7033`) only when it has no paired setter at
//! all. tsz does this through `checkers/accessor_checker.rs`
//! (`check_type_member_accessor_implicit_any`), the same owner that already
//! holds the rule for class members and object-literal accessor elements.
//!
//! This is the type-member arm of the rule #16185 established for classes.
//! Class members are checked as *declarations*
//! (`check_accessor_declaration_with_request`); interface and type-literal
//! members are never declaration-checked, which is why the family was reported
//! for classes and silent here.
//!
//! Three properties of the rule are load-bearing and each is pinned in both
//! directions below, because a matrix built one axis at a time skips exactly
//! the cell where the rule lives:
//!
//! 1. The getter supplies the property type when it has an annotation **or a
//!    body** — not annotation alone.
//! 2. `TS7006` and `TS7032` need **separate** suppression flags. Any paired
//!    getter contextually types the setter's *parameter*; only an
//!    annotated-or-bodied one supplies the *property's* type. The both-missing
//!    pair (`get g(); set g(v);`) reports `TS7032` and **no** `TS7006`, which is
//!    the one row that separates the two flags.
//! 3. It is the annotation's **presence**, not its type — `set g(v: any)` is
//!    clean.
//!
//! Every expectation here was recorded from `typescript@7.0.2` under
//! `--noEmit --strict --lib es2022 --target es2022`. Anchors are pinned as
//! 0-based offsets (`tsc`'s 1-based column minus one) because the blame *site*
//! is half of this rule: `TS7032` lands on the setter's name, never on its
//! parameter.

use tsz_checker::test_utils::{check_source_non_strict_codes, check_source_strict};

/// Every diagnostic as `TS<code>@<0-based start>`, sorted — the exact shape the
/// oracle rows were recorded in.
fn sites(source: &str) -> Vec<String> {
    let mut out: Vec<String> = check_source_strict(source)
        .iter()
        .map(|d| format!("TS{}@{}", d.code, d.start))
        .collect();
    out.sort();
    out
}

fn assert_sites(source: &str, expected: &[&str]) {
    let actual = sites(source);
    let expected: Vec<String> = expected.iter().map(|s| (*s).to_string()).collect();
    assert_eq!(actual, expected, "source: {source}");
}

// ---------------------------------------------------------------------------
// Interface members: the getter axis.
// ---------------------------------------------------------------------------

#[test]
fn interface_lone_unannotated_getter_reports_ts7033_on_the_getter_name() {
    // interface I { get g(); }
    //                   ^18
    assert_sites("interface I { get g(); }", &["TS7033@18"]);
}

#[test]
fn interface_lone_annotated_getter_is_clean() {
    assert_sites("interface I { get g(): number; }", &[]);
}

#[test]
fn interface_getter_with_any_paired_setter_is_never_the_blame_site() {
    // The pair moves the blame to the setter even though the setter is itself
    // unannotated: property (2) above. Only TS7032, and it is on `g` at 27 —
    // the *setter's* name, not the getter's and not the parameter.
    assert_sites("interface I { get g(); set g(v); }", &["TS7032@27"]);
}

#[test]
fn interface_pair_with_annotated_getter_is_clean() {
    assert_sites("interface I { get g(): number; set g(v); }", &[]);
}

#[test]
fn interface_pair_with_annotated_setter_is_clean() {
    assert_sites("interface I { get g(); set g(v: number); }", &[]);
}

#[test]
fn interface_pair_with_both_annotated_is_clean() {
    assert_sites("interface I { get g(): number; set g(v: number); }", &[]);
}

// ---------------------------------------------------------------------------
// Interface members: the setter axis, and the TS7006/TS7032 flag split.
// ---------------------------------------------------------------------------

#[test]
fn interface_lone_unannotated_setter_reports_both_ts7006_and_ts7032() {
    // No paired getter: the parameter is not contextually typed (TS7006 on the
    // parameter at 20) *and* the property has no type (TS7032 on the name at
    // 18). This is the direction that proves the two flags are separate.
    assert_sites("interface I { set g(v); }", &["TS7006@20", "TS7032@18"]);
}

#[test]
fn interface_paired_setter_suppresses_ts7006_but_not_ts7032() {
    // The opposite direction of the same split: the paired getter contextually
    // types the parameter (no TS7006) but supplies no property type (TS7032).
    // Collapsing the two flags into one passes exactly one of these two tests.
    assert_sites("interface I { get g(); set g(v); }", &["TS7032@27"]);
}

#[test]
fn interface_lone_annotated_setter_is_clean() {
    assert_sites("interface I { set g(v: number); }", &[]);
}

#[test]
fn interface_setter_annotated_any_is_clean() {
    // Property (3): presence of the annotation, not its type.
    assert_sites("interface I { set g(v: any); }", &[]);
    assert_sites("interface I { get g(); set g(v: any); }", &[]);
}

#[test]
fn interface_pair_order_does_not_move_the_blame_site() {
    // Setter first. The blame is still the setter's name (18), because pairing
    // is by name, not by source order.
    assert_sites("interface I { set g(v); get g(); }", &["TS7032@18"]);
}

// ---------------------------------------------------------------------------
// Per-member, not per-container. A single interface holding several accessors
// must reach a different verdict for each — no "does this interface have a
// setter" rule gets this right by accident.
// ---------------------------------------------------------------------------

#[test]
fn interface_mixes_blame_sites_within_one_container() {
    // `g` is a pair (TS7032 on its setter at 27); `h` is a lone getter
    // (TS7033 at 37). Two different verdicts inside one member list.
    assert_sites(
        "interface I { get g(); set g(v); get h(); }",
        &["TS7032@27", "TS7033@37"],
    );
}

#[test]
fn interface_three_accessors_each_resolve_independently() {
    // a: both missing  -> TS7032 on a's setter (27)
    // b: setter annotated -> clean
    // c: getter annotated -> clean
    assert_sites(
        "interface I { get a(); set a(v); get b(); set b(w: string); get c(): boolean; set c(y); }",
        &["TS7032@27"],
    );
}

// ---------------------------------------------------------------------------
// Binder-name independence. The rule is structural; renaming everything the
// user chose must not move a single anchor's *relationship* to its member.
// ---------------------------------------------------------------------------

#[test]
fn interface_renamed_binders_report_identically() {
    assert_sites("interface Zqx { get wobble(); }", &["TS7033@20"]);
    assert_sites(
        "interface Zqx { get wobble(); set wobble(qq); }",
        &["TS7032@34"],
    );
}

#[test]
fn interface_string_literal_and_numeric_names_pair_by_property_name() {
    // `get 'g'()` pairs with `set g(v)` exactly as tsc pairs them, so the
    // getter is not the blame site and the setter is.
    assert_sites("interface I { get 'g'(); set g(v); }", &["TS7032@29"]);
    assert_sites("interface I { get 0(); set 0(v); }", &["TS7032@27"]);
    assert_sites("interface I { get 'a b'(); }", &["TS7033@18"]);
    assert_sites("interface I { get 0(); }", &["TS7033@18"]);
}

#[test]
fn interface_computed_unique_symbol_names_pair_by_symbol_identity() {
    // Pairing falls back to computed-name symbol identity, and the message
    // renders the name as `[k]` — so the blame site is the setter at 61.
    assert_sites(
        "declare const k: unique symbol; interface I { get [k](); set [k](v); }",
        &["TS7032@61"],
    );
    assert_sites(
        "declare const k: unique symbol; interface I { get [k](); }",
        &["TS7033@50"],
    );
}

#[test]
fn interface_computed_name_message_renders_the_bracketed_name() {
    let messages: Vec<String> = check_source_strict(
        "declare const k: unique symbol; interface I { get [k](); set [k](v); }",
    )
    .iter()
    .map(|d| d.message_text.clone())
    .collect();
    assert!(
        messages
            .iter()
            .any(|m: &String| m.contains("Property '[k]'")),
        "expected the bracketed computed name: {messages:?}"
    );
}

// ---------------------------------------------------------------------------
// Type literals. The other half of the type-member walk — a distinct call site
// from the interface one, so every container shape it is reached through is
// pinned rather than assumed.
// ---------------------------------------------------------------------------

#[test]
fn type_alias_literal_reports_the_family() {
    assert_sites("type T = { get g(); };", &["TS7033@15"]);
    assert_sites("type T = { get g(); set g(v); };", &["TS7032@24"]);
    assert_sites("type T = { get g(): number; set g(v); };", &[]);
    assert_sites("type T = { get g(); set g(v: number); };", &[]);
    assert_sites("type T = { set g(v); };", &["TS7006@17", "TS7032@15"]);
}

#[test]
fn type_literal_in_a_variable_annotation_reports_the_family() {
    assert_sites("declare const x: { get g(); };", &["TS7033@23"]);
    assert_sites("declare const x: { get g(); set g(v); };", &["TS7032@32"]);
}

#[test]
fn type_literal_nested_inside_an_interface_property_reports_the_family() {
    assert_sites("interface I { p: { get g(); }; }", &["TS7033@23"]);
    assert_sites("interface I { p: { get g(); set g(v); }; }", &["TS7032@32"]);
}

#[test]
fn type_literal_in_signature_positions_reports_the_family() {
    assert_sites("declare function f(a: { get g(); }): void;", &["TS7033@28"]);
    assert_sites(
        "declare function f(): { get g(); set g(v); };",
        &["TS7032@37"],
    );
}

#[test]
fn type_literal_inside_composite_types_reports_the_family() {
    assert_sites("type T = { get g(); } & { a: number };", &["TS7033@15"]);
    assert_sites("type T = { get g(); } | { a: number };", &["TS7033@15"]);
}

#[test]
fn interface_with_heritage_and_type_parameters_reports_the_family() {
    assert_sites(
        "interface B { x: number } interface I extends B { get g(); set g(v); }",
        &["TS7032@63"],
    );
    assert_sites("interface I<T> { get g(); set g(v); }", &["TS7032@30"]);
}

// ---------------------------------------------------------------------------
// Negative controls: shapes that must stay exactly as they were.
// ---------------------------------------------------------------------------

#[test]
fn the_family_is_governed_by_no_implicit_any() {
    // Nothing in this family fires with noImplicitAny off.
    assert!(
        check_source_non_strict_codes("interface I { get g(); }").is_empty(),
        "TS7033 must be governed by noImplicitAny"
    );
    assert!(
        check_source_non_strict_codes("interface I { get g(); set g(v); }").is_empty(),
        "TS7032 must be governed by noImplicitAny"
    );
    assert!(
        check_source_non_strict_codes("interface I { set g(v); }").is_empty(),
        "TS7006/TS7032 must be governed by noImplicitAny"
    );
}

#[test]
fn neighbouring_type_member_kinds_are_untouched() {
    // The property-signature (TS7008) and method-signature (TS7010/TS7006)
    // arms of the same walk keep their existing verdicts and anchors.
    assert_sites("interface I { p; }", &["TS7008@14"]);
    assert_sites("interface I { m(); }", &["TS7010@14"]);
    assert_sites("interface I { m(a); }", &["TS7006@16", "TS7010@14"]);
    assert_sites("type T = { m(a); };", &["TS7006@13", "TS7010@11"]);
    assert_sites("interface I { m(a: number); }", &["TS7010@14"]);
    assert_sites("interface I { (a): void; }", &["TS7006@15"]);
    assert_sites("interface I { [k: string]: number; }", &[]);
}

#[test]
fn accessor_shaped_parse_recovery_still_lands_on_the_method_arm() {
    // `get ()` is not an accessor — it parses as a method signature *named*
    // `get`, so it keeps TS7010 from the method arm and must not acquire a
    // TS7033 from the accessor arm.
    assert_sites("interface I { get (); }", &["TS7010@14"]);
    assert_sites("type T = { get (); };", &["TS7010@11"]);
    assert_sites("interface I { get; }", &["TS7008@14"]);
    assert_sites("interface I { set (v); }", &["TS7006@19", "TS7010@14"]);
}

#[test]
fn class_and_object_literal_arms_are_unchanged() {
    // The class arm (#16179/#16183) and the object-literal arm own the same
    // rule through different entry points; this change is additive to them.
    assert_sites("declare class C { get g(); set g(v); }", &["TS7032@31"]);
    assert_sites("declare class C { get g(); }", &["TS7033@22"]);
    assert_sites("declare class C { set g(v); }", &["TS7006@24", "TS7032@22"]);
    assert_sites("const o = { get g() { return 1; } };", &[]);
}
