//! Regression tests for TS2862 false positives on broad indexed writes
//! through *concrete* declared index signatures (issue #6190).
//!
//! Structural rule: TS2862 ("generic and can only be indexed for reading")
//! fires for a broad indexed write `obj[k] = v` only when the object's
//! *key space* is generic — i.e. the object's structure embeds a `keyof T`
//! whose inner contains a free type parameter, producing a deferred mapped
//! key space (e.g. `Record<keyof Shape | "k", V>`).
//!
//! When the object has an explicit, concrete declared index signature —
//! whether through a class body, an interface body, or a mapped-type alias
//! with a concrete key argument such as `Record<string, T>` — the write
//! flows through ordinary assignability against the declared value type
//! and TS2862 does not apply.

use tsz_checker::test_utils::check_source_codes;

#[test]
fn ts2862_not_emitted_for_generic_class_string_index_signature_write() {
    // Reported repro: generic class with declared `[k: string]: T`.
    let source = r#"
class Dict<T> {
    [key: string]: T;

    set(key: string, value: T): void {
        this[key] = value;
    }
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2862),
        "TS2862 must not fire for writes through a class's own declared string index signature; got {codes:?}"
    );
    // TS2411 is the expected diagnostic for the method shape vs index signature.
    assert!(
        codes.contains(&2411),
        "TS2411 must still report the `set` method conflicting with the index signature; got {codes:?}"
    );
}

#[test]
fn ts2862_not_emitted_for_generic_class_string_index_signature_write_renamed_type_param() {
    // Same structural rule, different type parameter name — proves the fix
    // is not keyed by spelling. `T` -> `V`.
    let source = r#"
class Store<V> {
    [key: string]: V;

    put(key: string, value: V): void {
        this[key] = value;
    }
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2862),
        "TS2862 must not fire regardless of type parameter spelling; got {codes:?}"
    );
}

#[test]
fn ts2862_not_emitted_for_generic_interface_string_index_signature_write() {
    let source = r#"
interface Container<T> {
    [key: string]: T;
}
function set<T>(obj: Container<T>, key: string, value: T): void {
    obj[key] = value;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2862),
        "TS2862 must not fire for writes through an interface's declared string index signature; got {codes:?}"
    );
}

#[test]
fn ts2862_not_emitted_for_generic_interface_number_index_signature_write() {
    // Concrete numeric index signature: writing through a number key is safe.
    let source = r#"
interface NumKey<T> {
    [key: number]: T;
}
function set<T>(obj: NumKey<T>, key: number, value: T): void {
    obj[key] = value;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2862),
        "TS2862 must not fire for writes through a concrete numeric index signature; got {codes:?}"
    );
}

#[test]
fn ts2862_not_emitted_for_inline_mapped_alias_with_concrete_string_key() {
    // Same shape as `Record<string, T>` (Record is `{ [P in K]: T }`) but
    // expressed without depending on lib: the K argument is the concrete
    // `string` primitive, so the resulting mapped form has a non-generic
    // key space. Writes through it are safe.
    let source = r#"
type Map<K extends string | number | symbol, V> = { [P in K]: V };
function set<T>(obj: Map<string, T>, key: string, value: T): void {
    obj[key] = value;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2862),
        "TS2862 must not fire for inline mapped alias with concrete `string` key; got {codes:?}"
    );
}

#[test]
fn ts2862_still_emitted_for_mapped_alias_with_generic_keyof_key() {
    // Positive regression guard: the same alias with a *generic keyof*
    // argument — `Map<keyof Shape | "k", V>` — has a deferred mapped key
    // space (contains `keyof Shape` where Shape is a free type parameter).
    // tsc emits TS2862 for broad string writes here, and so must tsz.
    let source = r#"
type Map<K extends string | number | symbol, V> = { [P in K]: V };
function f<Shape extends { a: string }>(_shape: Shape) {
    const obj = {} as Map<keyof Shape | "knownLiteralKey", number>;
    obj["" as string] = 4;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2862),
        "TS2862 must still fire for broad writes through `Map<keyof T | ..., V>`; got {codes:?}"
    );
}

#[test]
fn ts2862_still_emitted_for_free_type_parameter_write() {
    // Negative regression guard: writing through a bare type parameter T
    // (constrained or not) keeps emitting TS2862, since the receiver itself
    // is generic.
    let source = r#"
function f<T extends { [key: string]: number }>(obj: T, k: string, v: number): void {
    obj[k] = v;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2862),
        "TS2862 must still fire for writes through a free type parameter; got {codes:?}"
    );
}
