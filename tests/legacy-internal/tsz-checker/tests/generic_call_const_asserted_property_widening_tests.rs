//! A per-property `as const` assertion on a *fresh* object-literal argument
//! pins the inferred type parameter at the non-widening literal (issue #16725).
//!
//! Structural rule: when a fresh object literal has a property carrying an
//! `as const` assertion (`{ v: "x" as const }`), and that property is the
//! inference site for an unconstrained type parameter, tsc keeps the
//! non-widening literal type — `getWidenedLiteralType` widens only *fresh*
//! literals, and the assertion makes the property literal non-fresh. tsz
//! previously widened it to the base primitive because the inference candidate
//! took the whole object's freshness (`source_is_fresh`) and ignored the
//! property's own `non_widening` flag, producing a false `TS2322`.
//!
//! Owner: `crates/tsz-solver/src/operations/constraints/signatures.rs`
//! (`constrain_properties`), which now folds `source.non_widening` into the
//! per-property freshness passed to the inference candidate.
//!
//! Binder names are varied across rows: the rule is structural.

use tsz_common::options::checker::CheckerOptions;

fn codes_strict(source: &str) -> Vec<u32> {
    let opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    crate::test_utils::check_source(source, "test.ts", opts)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_clean(source: &str) {
    let codes = codes_strict(source);
    assert!(
        codes.is_empty(),
        "expected tsc-clean; got diagnostics: {codes:?}"
    );
}

/// The reported witness: a string literal `as const` per-property inference
/// site for an unconstrained `T`. tsc keeps `"x"`.
#[test]
fn per_property_string_as_const_pins_literal() {
    assert_clean(
        r#"
declare function pick<T>(o: { v: T }): T;
const r = pick({ v: "x" as const });
const ok: "x" = r;
"#,
    );
}

/// The numeric sibling — `as const` on a number literal property.
#[test]
fn per_property_number_as_const_pins_literal() {
    assert_clean(
        r#"
declare function pick<T>(o: { v: T }): T;
const r = pick({ v: 1 as const });
const ok: 1 = r;
"#,
    );
}

/// Renamed binders — the rule must not key on `pick`/`v`/`T`.
#[test]
fn per_property_as_const_does_not_depend_on_binder_spelling() {
    assert_clean(
        r#"
declare function grab<Elem>(box: { held: Elem }): Elem;
const out = grab({ held: "tag" as const });
const ok: "tag" = out;
"#,
    );
}

/// A boolean literal `as const` property is preserved too.
#[test]
fn per_property_boolean_as_const_pins_literal() {
    assert_clean(
        r#"
declare function pick<T>(o: { v: T }): T;
const r = pick({ v: true as const });
const ok: true = r;
"#,
    );
}

/// Two const-asserted properties feeding two distinct unconstrained params.
#[test]
fn two_const_asserted_properties_each_pin_their_literal() {
    assert_clean(
        r#"
declare function pair<A, B>(o: { a: A; b: B }): [A, B];
const r = pair({ a: "x" as const, b: 2 as const });
const okA: "x" = r[0];
const okB: 2 = r[1];
"#,
    );
}

/// A const-asserted property beside a plain (widening) one: the asserted
/// property keeps its literal, the plain one still widens.
#[test]
fn const_asserted_property_pins_while_sibling_plain_widens() {
    assert_clean(
        r#"
declare function pair<A, B>(o: { a: A; b: B }): [A, B];
const r = pair({ a: "x" as const, b: "y" });
const okA: "x" = r[0];
const okB: string = r[1];
"#,
    );
}

// ---------------------------------------------------------------------------
// Regression guards: the shapes the issue reports already MATCH tsc must stay
// matching, and a plain (non-asserted) fresh literal must still widen.
// ---------------------------------------------------------------------------

/// Whole-object `as const` (`{ v: 1 } as const`) already preserved; still does.
#[test]
fn whole_object_as_const_still_preserved() {
    assert_clean(
        r#"
declare function unbox<T>(o: { v: T }): T;
const r = unbox({ v: 1 } as const);
const ok: 1 = r;
"#,
    );
}

/// Direct unconstrained `id(1 as const)` — literal preserved (already matched).
#[test]
fn direct_as_const_argument_still_preserved() {
    assert_clean(
        r#"
declare function id<T>(x: T): T;
const r = id(1 as const);
const ok: 1 = r;
"#,
    );
}

/// A non-fresh annotated source keeps its literal (already matched).
#[test]
fn non_fresh_annotated_source_still_preserved() {
    assert_clean(
        r#"
declare function unbox<T>(o: { v: T }): T;
declare const src: { v: 1 };
const r = unbox(src);
const ok: 1 = r;
"#,
    );
}

/// Negative control: a PLAIN fresh literal property (no `as const`) still
/// widens — `getWidenedLiteralType` widens fresh literals. tsc infers `string`,
/// so the literal-typed target must error (the fix must not over-preserve).
#[test]
fn plain_fresh_literal_property_still_widens() {
    let codes = codes_strict(
        r#"
declare function pick<T>(o: { v: T }): T;
const r = pick({ v: "x" });
const bad: "x" = r;
"#,
    );
    assert!(
        codes.contains(&2322),
        "a plain fresh literal property widens to its primitive; the literal target must error (TS2322). Got: {codes:?}"
    );
}

/// The widened side of the same plain case is clean: `r` is `string`.
#[test]
fn plain_fresh_literal_property_widens_to_primitive() {
    assert_clean(
        r#"
declare function pick<T>(o: { v: T }): T;
const r = pick({ v: "x" });
const ok: string = r;
"#,
    );
}
