//! Adjacent-case coverage for #13088: the canonical computed-property-name
//! owner (`types_domain::computed_names`) shared by the checker
//! symbol-resolution layer and the type-node lowering layer.
//!
//! Structural rules pinned here (verified against tsc 6.0):
//! - A binding annotated `unique symbol` late-binds `[k]` keys regardless of
//!   the binder's chosen name.
//! - A `<base>.<member>` access late-binds when the base variable's
//!   type-literal annotation declares `<member>: unique symbol`, including
//!   when the base is a local `Symbol` shadow and in external modules, and
//!   the key stays consistent between `keyof` (lowering layer) and
//!   `typeof <base>.<member>` (symbol-resolution layer).
//! - The two layers agree on the key, so reads and writes through either
//!   spelling stay assignable.

use crate::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_clean(source: &str) {
    let diags = check_source_diagnostics(source);
    assert!(
        diags.is_empty(),
        "expected no diagnostics, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

// ── unique-symbol annotated binding keys (renamed binders) ────────────────────

#[test]
fn unique_symbol_annotation_key_resolves_member_read() {
    assert_clean(
        r#"
declare const kAlpha: unique symbol;
interface Boxed { [kAlpha]: number; }
declare const b: Boxed;
const n: number = b[kAlpha];
"#,
    );
}

#[test]
fn unique_symbol_annotation_key_with_different_name_resolves_member_read() {
    assert_clean(
        r#"
declare const wireFormatTag: unique symbol;
interface Packet { [wireFormatTag]: string; }
declare const p: Packet;
const s: string = p[wireFormatTag];
"#,
    );
}

// ── declared unique-symbol member late-binding through a shadowed base ────────

#[test]
fn module_scope_symbol_shadow_unique_member_keyof_typeof_consistent() {
    // tsc 6.0: clean. The module-local `Symbol` genuinely shadows the global;
    // `[Symbol.iterator]` late-binds through the shadow's `unique symbol`
    // member and the `keyof` key matches `typeof Symbol.iterator`.
    assert_clean(
        r#"
export {};
declare const Symbol: { readonly iterator: unique symbol };
type Keys = keyof { [Symbol.iterator]: number };
declare let key: Keys;
const it: typeof Symbol.iterator = key;
"#,
    );
}

#[test]
fn script_scope_symbol_shadow_unique_member_interface_element_access() {
    // With no lib in this harness, the declared `Symbol` is the only Symbol
    // in the compilation; its `unique symbol` member late-binds the key for
    // both the interface member (lowering layer) and the element access
    // (symbol-resolution layer).
    assert_clean(
        r#"
declare const Symbol: { readonly tagA: unique symbol };
interface Marked { [Symbol.tagA]: string; }
declare const m: Marked;
const s: string = m[Symbol.tagA];
"#,
    );
}

#[test]
fn renamed_base_unique_member_interface_element_access() {
    // The late-binding rule keys off the structure (base variable whose
    // type-literal annotation declares `<member>: unique symbol`), not the
    // base being spelled `Symbol`.
    assert_clean(
        r#"
declare const WireTags: { readonly kk: unique symbol };
interface Tagged { [WireTags.kk]: number; }
declare const t: Tagged;
const n: number = t[WireTags.kk];
"#,
    );
}

// ── negative / fallback cases ─────────────────────────────────────────────────

#[test]
fn parenthesized_member_access_is_not_late_bindable() {
    // tsc 6.0: TS1169 — parentheses break the entity-name form even though
    // the inner type is a unique symbol.
    let codes = codes(
        r#"
declare const Wrapped: { readonly kk: unique symbol };
interface ViaParens { [(Wrapped.kk)]: string; }
"#,
    );
    assert!(
        codes.contains(&1169),
        "expected TS1169 for parenthesized computed member access, got: {codes:?}"
    );
}

#[test]
fn shadowed_symbol_literal_member_keeps_literal_key() {
    // tsc 6.0: clean. A shadowing const object with a string-literal member
    // names the property by the literal, not by a symbol key: reading the
    // literal key succeeds.
    assert_clean(
        r#"
export {};
const Symbol = { tag: "name" } as const;
interface Bag { [Symbol.tag]: string; }
declare const bag: Bag;
const v: string = bag["name"];
"#,
    );
}

#[test]
fn plain_symbol_annotation_key_still_matches_by_binding_identity() {
    // `: symbol` (non-unique) bindings key members by binding identity so
    // element access through the same binding resolves (existing tsz
    // behavior shared by both layers; tsc 6.0 is also clean here).
    assert_clean(
        r#"
declare const looseKey: symbol;
interface WithLoose { [looseKey]: number; }
declare const w: WithLoose;
const n: number = w[looseKey];
"#,
    );
}
