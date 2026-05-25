//! Regression tests for `keyof` preserving precise well-known-symbol keys.
//!
//! When an object/interface/type-literal has a member keyed by a well-known
//! symbol (`Symbol.iterator`, `Symbol.toPrimitive`, …), `keyof T` must
//! contribute that symbol's precise `typeof Symbol.xxx` (`unique symbol`
//! identity) key type, not widen it to the generic `symbol`.  Previously the
//! solver could not recover the symbol identity from the canonical
//! `[Symbol.xxx]` property-name key (the well-known-symbol name map was never
//! populated), so `keyof` widened to `symbol` and identity checks such as
//! `Equal<keyof typeof o, typeof Symbol.iterator>` resolved to `false`,
//! emitting a spurious TS2344.
//!
//! The structural rule under test: a well-known-symbol-keyed member's `keyof`
//! key type equals `typeof Symbol.<name>`, independent of which well-known
//! symbol it is, the bound iteration-variable name, or the surrounding shape
//! (value object, interface, or type literal).

use tsz_checker::test_utils::check_source_code_messages as compile_and_get_diagnostics;

/// `Equal<X, Y>` is the standard tsc identity probe; `Expect<true>` forces a
/// TS2344 when the two arguments are not mutually identical.  A clean compile
/// means `keyof` produced exactly the expected key type.
const PRELUDE: &str = r#"
type Equal<X, Y> =
  (<T>() => T extends X ? 1 : 2) extends (<T>() => T extends Y ? 1 : 2) ? true : false;
type Expect<T extends true> = T;
"#;

fn no_ts2344(source: &str) {
    let full = format!("{PRELUDE}{source}");
    let diagnostics = compile_and_get_diagnostics(&full);
    let ts2344: Vec<&(u32, String)> = diagnostics.iter().filter(|(c, _)| *c == 2344).collect();
    assert!(
        ts2344.is_empty(),
        "expected keyof to preserve the precise well-known symbol key (no TS2344); got: {diagnostics:#?}"
    );
}

#[test]
fn keyof_value_object_well_known_iterator_is_precise() {
    no_ts2344(
        r#"
const o = { [Symbol.iterator]: 2 };
type K = keyof typeof o;
type _ = Expect<Equal<K, typeof Symbol.iterator>>;
"#,
    );
}

/// Proves the rule is not `Symbol.iterator`-specific.
#[test]
fn keyof_value_object_well_known_to_primitive_is_precise() {
    no_ts2344(
        r#"
const o = { [Symbol.toPrimitive]: () => 1 };
type K = keyof typeof o;
type _ = Expect<Equal<K, typeof Symbol.toPrimitive>>;
"#,
    );
}

/// A different well-known symbol again, mixed with a string key so the keyof
/// is a union of the precise symbol key and the literal `"name"`.
#[test]
fn keyof_value_object_well_known_has_instance_mixed_with_string_key() {
    no_ts2344(
        r#"
const o = { [Symbol.hasInstance]: 0, name: "x" };
type K = keyof typeof o;
type _ = Expect<Equal<K, typeof Symbol.hasInstance | "name">>;
"#,
    );
}

#[test]
fn keyof_interface_well_known_symbol_key_is_precise() {
    no_ts2344(
        r#"
interface I { [Symbol.iterator]: number; a: string; }
type K = keyof I;
type _ = Expect<Equal<K, typeof Symbol.iterator | "a">>;
"#,
    );
}

/// Type-literal shape: assert the symbol-keyed `keyof` does not collapse to
/// the generic `symbol`.  (The precise `typeof Symbol.iterator` equality is
/// covered by the value-object and interface cases above; this case uses the
/// `symbol` discriminator so it does not depend on the reduced test lib
/// resolving a bare `typeof Symbol.xxx` expression.)
#[test]
fn keyof_type_literal_well_known_symbol_key_is_not_generic_symbol() {
    let full = format!(
        "{PRELUDE}{}",
        r#"
type TL = { [Symbol.iterator]: number };
type K = keyof TL;
type _ = Expect<Equal<K, symbol>>;
"#
    );
    let diagnostics = compile_and_get_diagnostics(&full);
    let ts2344 = diagnostics.iter().filter(|(c, _)| *c == 2344).count();
    assert_eq!(
        ts2344, 1,
        "type-literal keyof of a well-known symbol key must differ from generic `symbol`; got: {diagnostics:#?}"
    );
}

/// Control from the issue: a value object keyed by a *user* `unique symbol`
/// already round-trips precisely and must remain precise.  Uses the variable
/// name `p` (not `s`) to confirm the behaviour is identifier-independent.
#[test]
fn keyof_value_object_user_unique_symbol_remains_precise() {
    no_ts2344(
        r#"
declare const p: unique symbol;
const o = { [p]: 1 };
type K = keyof typeof o;
type _ = Expect<Equal<K, typeof p>>;
"#,
    );
}

/// Negative guard: `keyof` over a well-known symbol key must NOT collapse to
/// the generic `symbol`.  If it widened, `Equal<keyof, symbol>` would be
/// `true` and this `Expect` would compile clean; we require the TS2344 here.
#[test]
fn keyof_well_known_symbol_key_is_not_generic_symbol() {
    let full = format!(
        "{PRELUDE}{}",
        r#"
const o = { [Symbol.iterator]: 2 };
type K = keyof typeof o;
type _ = Expect<Equal<K, symbol>>;
"#
    );
    let diagnostics = compile_and_get_diagnostics(&full);
    let ts2344 = diagnostics.iter().filter(|(c, _)| *c == 2344).count();
    assert_eq!(
        ts2344, 1,
        "keyof of a well-known symbol key must differ from the generic `symbol`; got: {diagnostics:#?}"
    );
}
