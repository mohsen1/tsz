//! A computed `symbol` key in an object binding pattern resolves through the
//! source's `symbol` index signature (issue #17528).
//!
//! `{ [s]: v } = obj` desugars to `v = obj[s]`, so a symbol key indexing a
//! receiver with a `[k: symbol]: V` index signature is valid and binds `v: V` —
//! no TS2538. tsz previously fell through to a false TS2538 for any symbol key
//! that did not match a declared property, ignoring the symbol index signature.
//!
//! Every row is oracle-verified against `typescript@7.0.2` under `--strict false`
//! (so the tests run non-strict). Binder names vary across rows so the behaviour
//! cannot ride on a spelling. The negative controls (no symbol index) must keep
//! reporting TS2538.

use tsz_checker::test_utils::check_source_non_strict_codes;

const TS2538: u32 = 2538;
const TS2322: u32 = 2322;

/// The core case: an inline `[k: symbol]: V` index signature. No TS2538, and the
/// bound value carries the index's value type (a `number` assigned to `string`
/// is the witnessing TS2322).
#[test]
fn symbol_key_resolves_through_symbol_index_signature() {
    let source = r#"
declare const s: unique symbol;
interface Bag { [k: symbol]: number }
declare const bag: Bag;
const { [s]: v } = bag;
const bad: string = v;
"#;
    let codes = check_source_non_strict_codes(source);
    assert!(
        !codes.contains(&TS2538),
        "a symbol key indexing a symbol index signature must not report TS2538"
    );
    assert!(
        codes.contains(&TS2322),
        "the bound value must carry the symbol index's value type (number here)"
    );
}

/// Same shape, different binder names and a function-parameter binding pattern —
/// the behaviour is structural, not tied to `s`/`v`/`Bag`.
#[test]
fn symbol_key_index_resolution_is_binder_name_independent() {
    let source = r#"
declare const marker: unique symbol;
interface Store { [key: symbol]: boolean }
declare function take({ [marker]: flag }: Store): void;
declare const store: Store;
take(store);
const check: number = (({ [marker]: flag }: Store) => flag)(store);
"#;
    let codes = check_source_non_strict_codes(source);
    assert!(
        !codes.contains(&TS2538),
        "renamed symbol-key destructuring must not report TS2538"
    );
    assert!(
        codes.contains(&TS2322),
        "the bound value must carry the index value type (boolean here)"
    );
}

/// A type alias (`Lazy`) to a symbol-index object must be resolved before the
/// index is consulted.
#[test]
fn symbol_key_resolves_through_aliased_symbol_index() {
    let source = r#"
declare const s: unique symbol;
type Rec = { [k: symbol]: number };
declare const r: Rec;
const { [s]: v } = r;
const bad: string = v;
"#;
    let codes = check_source_non_strict_codes(source);
    assert!(!codes.contains(&TS2538));
    assert!(codes.contains(&TS2322));
}

/// A generic-alias *application* (`Rec<boolean>`) must be evaluated before the
/// index is consulted — the `Record<symbol, V>` shape the row uses.
#[test]
fn symbol_key_resolves_through_generic_symbol_index_application() {
    let source = r#"
declare const s: unique symbol;
type Rec<V> = { [k: symbol]: V };
declare const r: Rec<boolean>;
const { [s]: v } = r;
const bad: number = v;
"#;
    let codes = check_source_non_strict_codes(source);
    assert!(!codes.contains(&TS2538));
    assert!(codes.contains(&TS2322));
}

/// Union source where *every* member has a symbol index: valid, and the bound
/// value is the union of the members' value types.
#[test]
fn symbol_key_resolves_when_every_union_member_has_symbol_index() {
    let source = r#"
declare const s: unique symbol;
declare const u: { [k: symbol]: number } | { [k: symbol]: string };
const { [s]: v } = u;
const bad: boolean = v;
"#;
    let codes = check_source_non_strict_codes(source);
    assert!(!codes.contains(&TS2538));
    assert!(codes.contains(&TS2322));
}

/// Negative control: a union member without a symbol index means the key is not
/// universally valid — TS2538 must still fire.
#[test]
fn symbol_key_still_errors_when_a_union_member_lacks_symbol_index() {
    let source = r#"
declare const s: unique symbol;
declare const u: { [k: symbol]: number } | { a: number };
const { [s]: v } = u;
"#;
    assert!(
        check_source_non_strict_codes(source).contains(&TS2538),
        "a symbol key not accepted by every union member must still report TS2538"
    );
}

/// Negative control: a plain object with no symbol member keeps TS2538.
#[test]
fn symbol_key_still_errors_without_a_symbol_index() {
    let source = r#"
declare const s: unique symbol;
declare const obj: { a: number };
const { [s]: v } = obj;
"#;
    assert!(
        check_source_non_strict_codes(source).contains(&TS2538),
        "a symbol key on a source without a symbol index must still report TS2538"
    );
}

/// Negative control: a source with only a *string* index signature does not
/// accept a symbol key — TS2538 must still fire (the string-index fallthrough is
/// exactly what TS2538 is reserved for).
#[test]
fn symbol_key_still_errors_with_string_index_only() {
    let source = r#"
declare const s: unique symbol;
declare const obj: { [k: string]: number };
const { [s]: v } = obj;
"#;
    assert!(
        check_source_non_strict_codes(source).contains(&TS2538),
        "a symbol key on a string-index-only source must still report TS2538"
    );
}
