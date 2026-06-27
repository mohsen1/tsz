//! Regression guards: a literal inferred for a type parameter through an
//! **object-literal property** is widened to its primitive (matching tsc's
//! `getInferredType`), so a sibling `NoInfer<T>` position is checked against the
//! widened type — not the raw literal candidate.
//!
//! Before the fix tsz suppressed literal widening of the inferred type argument
//! whenever a `NoInfer<T>` occurrence was present, so
//! `create<T>(o: { value: T; default: NoInfer<T> })` called with
//! `{ value: 1, default: 2 }` kept `T = 1` and rejected `default: 2`
//! (`TS2322 '2' is not assignable to '1'`). tsc widens `T` to `number` (the
//! literal came from an object-literal property), so `NoInfer<T> = number`
//! accepts `2`. The widening distinction is structural: a *direct* argument
//! (`value: T`) and a `const` / literal-preserving-constrained type parameter
//! keep the literal in both compilers, so those still report the mismatch.
//!
//! Witnesses verified against `tsc` 6.0.2 (`--strict --target es2022`). Binder
//! names (type parameter, property, and function identifiers) vary across cases
//! so the guard stays structural per the anti-hardcoding contract.

use tsz_checker::test_utils::check_source_codes;

fn assert_clean(source: &str, ctx: &str) {
    let codes = check_source_codes(source);
    assert!(
        codes.is_empty(),
        "{ctx}: expected no diagnostics, got {codes:?}"
    );
}

fn assert_has_error(source: &str, ctx: &str) {
    let codes = check_source_codes(source);
    assert!(!codes.is_empty(), "{ctx}: expected a diagnostic, got none");
}

// ── Fix: object-property inference widens T for the NoInfer<T> sibling ───────

/// Number literal through an object-literal property: `Item` widens `10 ->
/// number`, so the `NoInfer<Item>` sibling accepts a different number literal.
#[test]
fn object_property_number_literal_widens_for_noinfer_sibling() {
    assert_clean(
        r#"
declare function build<Item>(cfg: { seed: Item; spare: NoInfer<Item> }): Item;
build({ seed: 10, spare: 99 });
"#,
        "number object-property + NoInfer sibling",
    );
}

/// String literal variant, different identifiers.
#[test]
fn object_property_string_literal_widens_for_noinfer_sibling() {
    assert_clean(
        r#"
declare function compose<Token>(parts: { primary: Token; secondary: NoInfer<Token> }): Token;
compose({ primary: "red", secondary: "blue" });
"#,
        "string object-property + NoInfer sibling",
    );
}

/// Boolean literal variant.
#[test]
fn object_property_boolean_literal_widens_for_noinfer_sibling() {
    assert_clean(
        r#"
declare function gate<Flag>(opts: { enabled: Flag; otherwise: NoInfer<Flag> }): Flag;
gate({ enabled: true, otherwise: false });
"#,
        "boolean object-property + NoInfer sibling",
    );
}

/// Nested object property: the widening site is two object levels deep.
#[test]
fn nested_object_property_literal_widens_for_noinfer_sibling() {
    assert_clean(
        r#"
declare function wire<Cell>(cfg: { inner: { value: Cell }; fallback: NoInfer<Cell> }): Cell;
wire({ inner: { value: 1 }, fallback: 2 });
"#,
        "nested object-property + NoInfer sibling",
    );
}

/// The original valibot-adjacent shape (return position is the bare parameter).
#[test]
fn create_with_default_noinfer_accepts_distinct_literal() {
    assert_clean(
        r#"
declare function create<T>(opts: { value: T; default: NoInfer<T> }): T;
const x = create({ value: 1, default: 2 });
"#,
        "create value/default NoInfer",
    );
}

// ── No regression: positions that genuinely preserve the literal still error ─

/// A *direct* argument keeps the literal in tsc, so the `NoInfer<V>` backup
/// must match it exactly — a different literal stays an error.
#[test]
fn direct_argument_literal_is_preserved_for_noinfer_sibling() {
    assert_has_error(
        r#"
declare function pick<V>(chosen: V, backup: NoInfer<V>): V;
pick(7, 8);
"#,
        "direct argument NoInfer preserves literal",
    );
}

/// A `const` type parameter preserves the literal even through an object
/// property, so a mismatched sibling is still rejected.
#[test]
fn const_type_parameter_preserves_object_property_literal() {
    assert_has_error(
        r#"
declare function freeze<const C>(o: { lead: C; trail: NoInfer<C> }): C;
freeze({ lead: 1, trail: 2 });
"#,
        "const type parameter preserves literal",
    );
}

/// Widening still rejects a genuinely-incompatible default: a string default
/// against a number-widened `T` is a real type error in both compilers.
#[test]
fn object_property_noinfer_rejects_incompatible_default_type() {
    assert_has_error(
        r#"
declare function attach<T>(opts: { value: T; fallback: NoInfer<T> }): T;
attach({ value: 1, fallback: "x" });
"#,
        "incompatible default type still errors",
    );
}

/// Plain object-property inference (no `NoInfer`) already widened the inferred
/// type argument — guard that the result is `number`, not the literal `1`.
#[test]
fn plain_object_property_inference_widens_inferred_argument() {
    assert_has_error(
        r#"
declare function read<T>(o: { value: T }): T;
const r = read({ value: 1 });
const one: 1 = r;
"#,
        "plain object-property inference widens to number",
    );
}
