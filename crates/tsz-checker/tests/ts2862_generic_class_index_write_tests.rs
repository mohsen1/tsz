//! Regression tests for TS2862 false positive on indexed writes inside a
//! generic class whose own body declares a concrete index signature
//! (issue #6190).
//!
//! Structural rule:
//!
//!   TS2862 ("generic and can only be indexed for reading") fires for a
//!   broad indexed write `obj[k] = v` only when the receiver's *key space*
//!   is genuinely deferred — i.e. after evaluation in the current
//!   environment the receiver is still a generic mapped type whose key
//!   constraint contains a free type parameter (e.g. `{ [K in keyof T]: V }`,
//!   `Record<keyof T, V>`).
//!
//! When the receiver has a *concretely declared* index signature — a class
//! body, an interface body, or a mapped form with a non-generic key
//! argument such as `Record<string, T>` — the write flows through
//! ordinary assignability against the declared value type. Any real
//! mismatch is reported by TS2322, not TS2862.

use tsz_checker::test_utils::check_source_codes;

#[test]
fn ts2862_not_emitted_for_generic_class_self_index_write() {
    // Reported repro from #6190: generic class with declared `[k: string]: T`
    // and a method that writes `this[key] = value`.
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
        "TS2862 must not fire for `this[k] = v` inside a generic class with a declared string index signature; got {codes:?}",
    );
    // TS2411 (method shape vs declared index signature) is the expected
    // diagnostic — preserving it ensures the fix didn't go too wide.
    assert!(
        codes.contains(&2411),
        "TS2411 must still report the `set` method conflicting with the declared index signature; got {codes:?}",
    );
}

#[test]
fn ts2862_not_emitted_for_generic_class_self_index_write_renamed_type_parameter() {
    // Same structural rule, but the type parameter is `V` rather than `T`,
    // and the method is `put` rather than `set`. Per §25, the fix must not
    // be keyed by the spelling of the bound name. The behaviour must be
    // identical to the canonical repro.
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
        "TS2862 must not fire regardless of the type-parameter or method name; got {codes:?}",
    );
}

#[test]
fn ts2862_not_emitted_for_generic_interface_string_index_write() {
    // The same rule for interfaces: an explicit `[k: string]: T` is a
    // concretely declared index signature even when T is free in the
    // declaring scope.
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
        "TS2862 must not fire for writes through an interface's declared string index signature; got {codes:?}",
    );
}

#[test]
fn ts2862_not_emitted_for_generic_interface_number_index_write() {
    // Concrete numeric index signature is also writable through a number
    // key. Generic value type is fine because the key space is non-generic.
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
        "TS2862 must not fire for writes through a declared numeric index signature; got {codes:?}",
    );
}

#[test]
fn ts2862_not_emitted_for_inline_mapped_alias_with_concrete_string_key() {
    // Same shape as `Record<string, T>` (Record is `{ [P in K]: T }`) but
    // expressed without depending on a lib alias. `K = string` means the
    // mapped key space is *non-generic*, so the receiver reduces to an
    // `ObjectWithIndex` with a declared string index signature. The write
    // is writable through assignability; TS2862 must not fire.
    let source = r#"
type MapLike<K extends string | number | symbol, V> = { [P in K]: V };
function set<T>(obj: MapLike<string, T>, key: string, value: T): void {
    obj[key] = value;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2862),
        "TS2862 must not fire for an inline mapped alias with a concrete `string` key; got {codes:?}",
    );
}

#[test]
fn ts2862_still_emitted_for_mapped_alias_with_generic_keyof_key() {
    // Positive regression guard: the same alias with a *generic keyof*
    // argument has a deferred mapped key space (`keyof Shape` contains a
    // free type parameter `Shape`). tsc emits TS2862 for broad string
    // writes here; tsz must continue to do the same.
    //
    // Per §25, the iteration variable is renamed and the type-parameter
    // is `Shape` (rather than `T`) to keep the fix structurally keyed and
    // not name-keyed.
    let source = r#"
type MapLike<K extends string, V> = { [P in K]: V };
function f<Shape>(_shape: Shape): void {
    const obj: MapLike<keyof Shape & string, number> =
        {} as MapLike<keyof Shape & string, number>;
    obj["" as string] = 4;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2862),
        "TS2862 must still fire for broad writes through `MapLike<keyof T & string, V>`; got {codes:?}",
    );
}

#[test]
fn ts2862_still_emitted_for_free_type_parameter_indexed_write() {
    // Negative regression guard: a write through a *bare* type parameter
    // `T` (constrained or not) continues to emit TS2862, since the
    // receiver itself is generic and its concrete instance shape is
    // unknown at the write site.
    let source = r#"
function f<T extends { [key: string]: number }>(obj: T, k: string, v: number): void {
    obj[k] = v;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2862),
        "TS2862 must still fire for writes through a bare type parameter; got {codes:?}",
    );
}
