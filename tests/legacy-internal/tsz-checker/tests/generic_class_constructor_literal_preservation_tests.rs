//! When a generic class has a type parameter `T extends P` where P is a
//! primitive (string/number/boolean/bigint/symbol), tsc preserves literal
//! argument types as the inferred `T` value. The same class-based inference
//! must match the function-call path.

use crate::test_utils::check_source_diagnostics;

#[test]
fn generic_class_k_extends_string_preserves_literal_key_name() {
    let diags = check_source_diagnostics(
        r#"
class KeyValue<K extends string, V> {
    constructor(public key: K, public value: V) {}
    getEntry(): [K, V] { return [this.key, this.value]; }
}
const kv = new KeyValue("name", "Alice");
const entry = kv.getEntry();
const k: "name" = entry[0];
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322: K extends string should preserve \"name\" literal; got: {ts2322:?}"
    );
}

#[test]
fn generic_class_k_extends_string_preserves_literal_different_name() {
    let diags = check_source_diagnostics(
        r#"
class Pair<K extends string, V> {
    constructor(public first: K, public second: V) {}
    getKey(): K { return this.first; }
}
const p = new Pair("status", 42);
const k: "status" = p.getKey();
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322: K extends string should preserve \"status\" literal; got: {ts2322:?}"
    );
}

#[test]
fn generic_class_n_extends_number_preserves_literal() {
    let diags = check_source_diagnostics(
        r#"
class NumBox<N extends number> {
    constructor(public val: N) {}
    get(): N { return this.val; }
}
const box = new NumBox(42);
const n: 42 = box.get();
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322: N extends number should preserve 42 literal; got: {ts2322:?}"
    );
}

#[test]
fn generic_class_unconstrained_t_widens_boolean_literal() {
    let diags = check_source_diagnostics(
        r#"
class Wrap<T> {
    constructor(public val: T) {}
    get(): T { return this.val; }
}
const w = new Wrap(true);
const b: boolean = w.get();
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322: unconstrained T widens true to boolean; got: {ts2322:?}"
    );
}

#[test]
fn generic_class_tuple_return_preserves_k_extends_string_literal() {
    let diags = check_source_diagnostics(
        r#"
class Entry<K extends string, V> {
    constructor(public key: K, public value: V) {}
    asTuple(): [K, V] { return [this.key, this.value]; }
}
const e = new Entry("role", true);
const t = e.asTuple();
const k: "role" = t[0];
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322: tuple return [K, V] should preserve K=\"role\" literal; got: {ts2322:?}"
    );
}

#[test]
fn generic_class_literal_preserved_regardless_of_param_name() {
    let diags = check_source_diagnostics(
        r#"
class Item<X extends string, Y> {
    constructor(public id: X, public data: Y) {}
    getId(): X { return this.id; }
}
const item = new Item("widget", 100);
const x: "widget" = item.getId();
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322: literal preservation must not depend on type-param name; got: {ts2322:?}"
    );
}

#[test]
fn function_call_k_extends_string_preserves_literal_parity() {
    let diags = check_source_diagnostics(
        r#"
function pair<K extends string, V>(key: K, value: V): [K, V] {
    return [key, value];
}
const p = pair("name", "Alice");
const k: "name" = p[0];
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322: function call K extends string also preserves literal; got: {ts2322:?}"
    );
}
