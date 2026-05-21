//! Tests for object literal types with computed property names whose key
//! expression has the wide `symbol` intrinsic type.
//!
//! Structural rule: when a computed property name in an object literal has
//! the wide `symbol` type (not `unique symbol`, not a well-known symbol like
//! `Symbol.iterator`), tsc synthesises a `[s: symbol]: V` index signature on
//! the resulting object type rather than a named member. Therefore:
//!
//! - `(typeof o)[symbol]` resolves to the value type
//! - `keyof typeof o` includes `symbol`
//! - `o[wideSym]` resolves to the value type (no TS2536)
//!
//! Negative coverage: `unique symbol` and well-known symbol (`Symbol.iterator`)
//! computed keys keep their named-member behaviour.
use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn has_ts(source: &str, code: u32) -> bool {
    codes(source).contains(&code)
}

// ── Issue-9755 repro: indexed access via wide `symbol` type ──────────────────

#[test]
fn issue_9755_indexed_access_with_wide_symbol_key_type_no_false_ts2536() {
    // `(typeof o)[symbol]` must resolve to `number`, not raise TS2536.
    assert!(!has_ts(
        r#"
declare const sym: symbol;
const o = { [sym]: 1 };
type V = (typeof o)[symbol];
const _check: number = (null as unknown as V);
"#,
        diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE
    ));
}

#[test]
fn issue_9755_indexed_access_value_assignable_to_number() {
    // Beyond TS2536, the resolved indexed access must actually be `number`
    // (or compatible). A wrong widening to `unknown`/`any` would still pass
    // the TS2536 check above, so we also require an assignability gate.
    assert!(!has_ts(
        r#"
declare const sym: symbol;
const o = { [sym]: 1 };
const probe: number = (null as unknown as (typeof o)[symbol]);
"#,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

// ── keyof on object literal with mixed wide-symbol + string-named members ────

#[test]
fn issue_9755_keyof_with_mixed_symbol_and_string_named_members() {
    // `keyof typeof o2` should be `symbol | "a"`. We verify by checking that:
    //   - `"a"` is assignable to keyof (named member is kept)
    //   - `symbol` is assignable to keyof (symbol index signature is present)
    assert!(!has_ts(
        r#"
declare const sym: symbol;
const o2 = { [sym]: 1, a: 2 };
type K = keyof typeof o2;
const probe_a: K = "a";
const probe_sym: K = (null as unknown as symbol);
"#,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

// ── Rule generalisation: different binding name ──────────────────────────────

#[test]
fn wide_symbol_index_works_with_renamed_binding() {
    // The rule must be structural — the binding name is irrelevant.
    assert!(!has_ts(
        r#"
declare const myKey: symbol;
const o = { [myKey]: "hi" };
const _check: string = (null as unknown as (typeof o)[symbol]);
"#,
        diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE
    ));
}

#[test]
fn wide_symbol_index_works_with_third_binding_name() {
    assert!(!has_ts(
        r#"
declare const anotherSym: symbol;
const o = { [anotherSym]: true };
const _check: boolean = (null as unknown as (typeof o)[symbol]);
"#,
        diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE
    ));
}

// ── Value-level access by the same wide-symbol binding ────────────────────────

#[test]
fn value_level_access_via_same_wide_symbol_binding_no_ts2536() {
    // `o[sym]` (value-level) must resolve to the value type.
    assert!(!has_ts(
        r#"
declare const sym: symbol;
const o = { [sym]: 42 };
const probe: number = o[sym];
"#,
        diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE
    ));
}

// ── Negative control: unique symbol stays a named member ──────────────────────

#[test]
fn unique_symbol_computed_key_still_named_member_not_index_signature() {
    // For `unique symbol`, the value-level `o[usym]` must still work (named
    // member path). This is the boundary that distinguishes the fix from a
    // blanket "all symbol-typed keys are index signatures" rule.
    assert!(!has_ts(
        r#"
declare const usym: unique symbol;
const o = { [usym]: 7 };
const probe: number = o[usym];
"#,
        diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE
    ));
}

#[test]
fn unique_symbol_keyof_does_not_include_wide_symbol() {
    // For `{ [usym]: ... }` with `usym: unique symbol`, `keyof` should yield
    // `typeof usym` (the unique-symbol type), not the wide `symbol`. We probe
    // by asserting the wide `symbol` type is NOT assignable to the keyof.
    assert!(has_ts(
        r#"
declare const usym: unique symbol;
const o = { [usym]: 7 };
type K = keyof typeof o;
declare const widesym: symbol;
const probe: K = widesym;
"#,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

// ── Negative control: well-known symbols stay named ───────────────────────────

#[test]
fn well_known_symbol_iterator_stays_a_named_member() {
    // Well-known symbols like `Symbol.iterator` get a stable `[Symbol.iterator]`
    // named key and must not be folded into a `[s: symbol]` index signature.
    // Verify that `o[Symbol.iterator]` resolves correctly.
    assert!(!has_ts(
        r#"
const o = { [Symbol.iterator]: () => null };
const fn = o[Symbol.iterator];
"#,
        diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE
    ));
}

// ── Wide-symbol parameter (not a const binding) ───────────────────────────────

#[test]
fn wide_symbol_parameter_keyed_object_literal_no_silent_drop() {
    // A parameter typed `s: symbol` (not a const binding) must also produce
    // a symbol-keyed index signature. Without the fix, tsz silently dropped
    // the property and `o[s]` would not return the value type.
    assert!(!has_ts(
        r#"
function f(s: symbol) {
    const o = { [s]: 99 };
    const probe: number = o[s];
    return probe;
}
"#,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

// ── Method form: `{ [sym]() {} }` ─────────────────────────────────────────────

#[test]
fn wide_symbol_keyed_method_becomes_callable_symbol_index() {
    // A method declared with a wide-symbol computed key should still be
    // callable via that key — and importantly should not crash or emit
    // TS2536 on `(typeof o)[symbol]` access.
    assert!(!has_ts(
        r#"
declare const sym: symbol;
const o = { [sym]() { return 1; } };
type V = (typeof o)[symbol];
"#,
        diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE
    ));
}
