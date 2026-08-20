//! Locks in tsc's fresh-literal source widening for object-literal property
//! assignment (TS2322) displays.
//!
//! When a fresh string/number/bigint literal object-literal property is
//! assigned to a property whose declared type does not admit a literal of the
//! *source's* primitive domain, tsc widens the source to its primitive base in
//! the message — `{ configurable: "yes" }` against `{ configurable?: boolean }`
//! renders `Type 'string'`, not `Type '"yes"'`. The solver already widens the
//! source in its failure reason; the display had re-read the anchor
//! expression's literal text and un-widened it because the underlying
//! literal-sensitivity gate is domain-agnostic (`boolean` is stored as the
//! `true | false` union, numeric-literal unions are singleton-shaped).
//!
//! The source literal is preserved only when the target admits its domain: a
//! string source against a string-literal union keeps `"yes"`, a number source
//! against a numeric-literal union keeps `3`, and boolean literal sources
//! (`true` / `false`) are never widened. Every case below was verified against
//! `typescript@7.0.2`.
//!
//! Binder names (the interface, the property, the variable) are varied across
//! the matrix so the rule is proven structural, not keyed on a particular
//! spelling (`.claude/CLAUDE.md` §25).

use tsz_checker::test_utils::check_source_strict_messages;

fn ts2322_source_displays(source: &str) -> Vec<String> {
    check_source_strict_messages(source)
        .into_iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message.replace('\n', " | "))
        .collect()
}

#[track_caller]
fn assert_source_display(source: &str, expected_substring: &str) {
    let messages = ts2322_source_displays(source);
    assert!(
        messages.iter().any(|m| m.contains(expected_substring)),
        "expected a TS2322 containing {expected_substring:?}, got: {messages:#?}",
    );
}

#[track_caller]
fn assert_no_source_display(source: &str, forbidden_substring: &str) {
    let messages = ts2322_source_displays(source);
    assert!(
        !messages.iter().any(|m| m.contains(forbidden_substring)),
        "expected no TS2322 containing {forbidden_substring:?}, got: {messages:#?}",
    );
}

// --- string literal source, target rejects the string domain -> widen -------

#[test]
fn string_literal_against_boolean_property_widens_to_string() {
    assert_source_display(
        r#"interface Descriptor { configurable?: boolean }
           const flags: Descriptor = { configurable: "yes" };"#,
        "Type 'string' is not assignable to type 'boolean'",
    );
}

#[test]
fn string_literal_against_boolean_literal_property_widens_to_string() {
    assert_source_display(
        r#"interface Toggle { active?: true }
           const control: Toggle = { active: "yes" };"#,
        "Type 'string' is not assignable to type 'true'",
    );
}

#[test]
fn string_literal_against_numeric_literal_union_widens_to_string() {
    assert_source_display(
        r#"interface Tier { rank?: 1 | 2 }
           const account: Tier = { rank: "gold" };"#,
        "Type 'string' is not assignable to type",
    );
    assert_no_source_display(
        r#"interface Tier { rank?: 1 | 2 }
           const account: Tier = { rank: "gold" };"#,
        "'\"gold\"'",
    );
}

#[test]
fn string_literal_against_numeric_enum_widens_to_string() {
    assert_source_display(
        r#"enum Suit { Hearts, Spades }
           interface Hand { lead?: Suit }
           const round: Hand = { lead: "clubs" };"#,
        "Type 'string' is not assignable to type",
    );
    assert_no_source_display(
        r#"enum Suit { Hearts, Spades }
           interface Hand { lead?: Suit }
           const round: Hand = { lead: "clubs" };"#,
        "'\"clubs\"'",
    );
}

// --- number / bigint literal source, target rejects the domain -> widen -----

#[test]
fn number_literal_against_boolean_property_widens_to_number() {
    assert_source_display(
        r#"interface Switch { enabled?: boolean }
           const gate: Switch = { enabled: 7 };"#,
        "Type 'number' is not assignable to type 'boolean'",
    );
}

#[test]
fn bigint_literal_against_boolean_property_widens_to_bigint() {
    assert_source_display(
        r#"interface Meter { armed?: boolean }
           const sensor: Meter = { armed: 9n };"#,
        "Type 'bigint' is not assignable to type 'boolean'",
    );
}

// --- source literal preserved when the target admits its domain -------------

#[test]
fn string_literal_against_string_literal_union_is_preserved() {
    assert_source_display(
        r#"interface Palette { shade?: "warm" | "cool" }
           const theme: Palette = { shade: "neon" };"#,
        "Type '\"neon\"' is not assignable to type",
    );
}

#[test]
fn number_literal_against_numeric_literal_union_is_preserved() {
    assert_source_display(
        r#"interface Slot { index?: 1 | 2 }
           const cell: Slot = { index: 5 };"#,
        "Type '5' is not assignable to type",
    );
}

#[test]
fn string_literal_against_mixed_union_with_string_member_is_preserved() {
    // The target union carries a string literal (`"seed"`), so the source's
    // string domain is admitted and the literal is preserved.
    assert_source_display(
        r#"interface Source { value?: "seed" | number }
           const feed: Source = { value: "sprout" };"#,
        "Type '\"sprout\"' is not assignable to type",
    );
}

#[test]
fn boolean_literal_source_is_never_widened() {
    // tsc keeps `true` / `false` verbatim even against a numeric-literal union
    // the boolean domain cannot satisfy.
    assert_source_display(
        r#"interface Config { mode?: 1 | 2 }
           const setup: Config = { mode: true };"#,
        "Type 'true' is not assignable to type",
    );
}

// --- adjacency: nested property, and the plain (non-property) assignment -----

#[test]
fn nested_object_literal_property_widens_string_literal() {
    assert_source_display(
        r#"interface Outer { inner: { flag: boolean } }
           const shell: Outer = { inner: { flag: "yes" } };"#,
        "Type 'string' is not assignable to type 'boolean'",
    );
}

#[test]
fn plain_boolean_annotation_still_widens_string_literal() {
    assert_source_display(
        r#"const direct: boolean = "yes";"#,
        "Type 'string' is not assignable to type 'boolean'",
    );
}

// --- #17782: a nested object-literal value inside a fresh union-fold head keeps
// its literal surface when the arm admits it -----------------------------------
// tsc renders the head as `{ kind: "a"; v: { x: 2; }; }` — the nested `v.x`
// literal survives because a discriminated-union arm carries `v: { x: 2 }`, a
// same-domain literal. tsz dropped the contextual target when recursing into the
// nested object literal, so every nested value widened to its primitive base
// (`v: { x: number; }`). The fix threads the nested property's contextual target
// (the property's type unioned across the target's arms) into the recursion.
// Binder names varied so the rule is structural.

#[test]
fn nested_object_literal_number_literal_preserved_in_union_fold_head() {
    let src = r#"type Shape =
           | { tag: "lo"; box: { size: 1 } }
           | { tag: "hi"; box: { size: 2 } };
       const picked: Shape = { tag: "lo", box: { size: 2 } };"#;
    assert_source_display(src, "box: { size: 2; }");
    assert_no_source_display(src, "box: { size: number; }");
}

#[test]
fn nested_object_literal_string_literal_preserved_in_union_fold_head() {
    let src = r#"type Route =
           | { via: "road"; edge: { name: "north" } }
           | { via: "rail"; edge: { name: "south" } };
       const trip: Route = { via: "road", edge: { name: "south" } };"#;
    assert_source_display(src, r#"edge: { name: "south"; }"#);
    assert_no_source_display(src, "edge: { name: string; }");
}

#[test]
fn nested_object_literal_boolean_literal_preserved_in_union_fold_head() {
    let src = r#"type Mode =
           | { key: "on"; state: { live: true } }
           | { key: "off"; state: { live: false } };
       const run: Mode = { key: "on", state: { live: false } };"#;
    assert_source_display(src, "state: { live: false; }");
    assert_no_source_display(src, "state: { live: boolean; }");
}

#[test]
fn deeply_nested_object_literal_preserved_in_union_fold_head() {
    // Three levels: the recursion threads the contextual target at every depth.
    let src = r#"type Deep =
           | { sel: "a"; outer: { middle: { leaf: 1 } } }
           | { sel: "b"; outer: { middle: { leaf: 2 } } };
       const value: Deep = { sel: "a", outer: { middle: { leaf: 2 } } };"#;
    assert_source_display(src, "outer: { middle: { leaf: 2; }; }");
    assert_no_source_display(src, "leaf: number");
}

#[test]
fn nested_object_literal_domain_mismatch_is_not_over_preserved() {
    // The negative control: when no arm's nested property admits the source's
    // domain, the nested literal still widens (threading a target must not
    // over-preserve). A string source against numeric-literal arms widens to
    // `string`, and the `"many"` literal never survives — the bedrock
    // fresh-literal domain rule this file encodes, here through a nested value.
    let src = r#"type Bucket =
           | { id: "x"; cell: { count: 1 } }
           | { id: "y"; cell: { count: 2 } };
       const item: Bucket = { id: "x", cell: { count: "many" } };"#;
    assert_source_display(src, "Type 'string' is not assignable");
    assert_no_source_display(src, r#""many""#);
}

// --- `NoInfer<T>` targets are transparent for the literal-domain decision ----
// `NoInfer<>` only suppresses inference, never assignability, so a same-domain
// literal source must be preserved when the wrapped target admits its domain
// (#17491: `noInfer.ts` regressed by #17488, which widened `"bar"` to `string`
// against a `NoInfer<"foo">` property target). Binder names are varied.

#[test]
fn string_literal_against_noinfer_string_literal_property_is_preserved() {
    // `{ x: NoInfer<T> }` with `T` fixed to `"foo"` — the wrapped target is the
    // string literal `"foo"`, same domain as the source, so `"bar"` survives.
    assert_source_display(
        r#"declare function pick<T extends string>(a: T, b: { x: NoInfer<T> }): void;
           pick("foo", { x: "bar" });"#,
        "Type '\"bar\"' is not assignable to type '\"foo\"'",
    );
    assert_no_source_display(
        r#"declare function pick<T extends string>(a: T, b: { x: NoInfer<T> }): void;
           pick("foo", { x: "bar" });"#,
        "Type 'string' is not assignable",
    );
}

#[test]
fn number_literal_against_noinfer_number_literal_property_is_preserved() {
    // Renamed binders + numeric domain: same-domain preservation through NoInfer.
    assert_source_display(
        r#"declare function choose<K extends number>(lead: K, rest: { slot: NoInfer<K> }): void;
           choose(1, { slot: 2 });"#,
        "Type '2' is not assignable to type '1'",
    );
}

#[test]
fn string_literal_against_noinfer_boolean_property_still_widens() {
    // The wrapped target admits only the boolean domain, so a string source is a
    // genuine domain rejection and still widens — stripping NoInfer must not
    // over-preserve.
    assert_source_display(
        r#"declare function gate<F extends boolean>(seed: unknown, opts: { on: NoInfer<F> }): void;
           gate(0, { on: "yes" });"#,
        "Type 'string' is not assignable to type 'boolean'",
    );
}
