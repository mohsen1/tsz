//! Regression tests for false-positive TS2416 / TS2322 when a class
//! implements a built-in collection type that spans multiple lib declaration
//! files (e.g. `Map<K,V>`, `Set<T>`, `WeakMap<K,V>`).
//!
//! Root cause: `compute_interface_type_from_declarations` used a single
//! `self.ctx.arena` for *all* declarations of the built-in symbol, but each
//! lib file has its own `NodeArena` with independent `NodeIndex` spaces.
//! Using the wrong arena would retrieve an unrelated AST node (often the
//! `Iterable` interface), whose `[Symbol.iterator](): Iterator<T, TReturn, TNext>`
//! signature leaked into the resolved type and caused false override-mismatch
//! errors.
//!
//! Fix: when cross-arena delegation is active, resolve each declaration with
//! its own `NodeArena` via `lower_merged_interface_declarations_with_symbol`.
//!
//! Issue: <https://github.com/mohsen1/tsz/issues/8422>

use crate::test_utils::check_source_codes;

fn assert_no_2416(src: &str) {
    let codes = check_source_codes(src);
    assert!(!codes.contains(&2416), "unexpected TS2416. Got: {codes:?}");
}

fn assert_no_2322(src: &str) {
    let codes = check_source_codes(src);
    assert!(!codes.contains(&2322), "unexpected TS2322. Got: {codes:?}");
}

fn assert_has_2416(src: &str) {
    let codes = check_source_codes(src);
    assert!(
        codes.contains(&2416),
        "expected TS2416, got none. Got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Map-based class — the original report (#8422)
// ---------------------------------------------------------------------------

#[test]
fn no_false_positive_ts2416_class_extends_map() {
    assert_no_2416(
        "
class MyMap extends Map<string, number> {
    [Symbol.iterator](): MapIterator<[string, number]> {
        return super[Symbol.iterator]();
    }
}
",
    );
}

#[test]
fn no_false_positive_ts2416_class_implements_map_generic_name_k_v() {
    assert_no_2416(
        "
class KVMap<K, V> extends Map<K, V> {
    [Symbol.iterator](): MapIterator<[K, V]> {
        return super[Symbol.iterator]();
    }
}
",
    );
}

// ---------------------------------------------------------------------------
// Set-based class — different built-in that also spans multiple lib files
// ---------------------------------------------------------------------------

#[test]
fn no_false_positive_ts2416_class_extends_set() {
    assert_no_2416(
        "
class NumberSet extends Set<number> {
    [Symbol.iterator](): SetIterator<number> {
        return super[Symbol.iterator]();
    }
}
",
    );
}

#[test]
fn no_false_positive_ts2416_class_extends_set_generic_element() {
    assert_no_2416(
        "
class TypedSet<E> extends Set<E> {
    [Symbol.iterator](): SetIterator<E> {
        return super[Symbol.iterator]();
    }
}
",
    );
}

#[test]
fn no_false_positive_ts2416_merged_lib_interface_symbol_prepasses_use_decl_arenas() {
    assert_no_2416(
        "
interface Map<K, V> {
    [Symbol.toStringTag]: string;
}

class TaggedMap extends Map<string, number> {
    [Symbol.iterator](): MapIterator<[string, number]> {
        return super[Symbol.iterator]();
    }
}
",
    );
}

// ---------------------------------------------------------------------------
// Negative / sanity cases — wrong return type must still produce TS2416
// ---------------------------------------------------------------------------

#[test]
fn genuine_ts2416_wrong_iterator_return_type() {
    assert_has_2416(
        "
class BadMap extends Map<string, number> {
    // Wrong: [Symbol.iterator] should return MapIterator<[string,number]>,
    // but we return a plain Iterator<string>, which is incompatible.
    [Symbol.iterator](): Iterator<string> {
        return ([] as string[]).values();
    }
}
",
    );
}

// ---------------------------------------------------------------------------
// TS2322 narrowing case: iterating a Map/Set inside a function body must not
// produce false positives when the iterated value is non-nullable.
// ---------------------------------------------------------------------------

#[test]
fn no_false_positive_ts2322_map_entries_destructure() {
    assert_no_2322(
        "
function processMap(m: Map<string, number>) {
    for (const [k, v] of m) {
        const key: string = k;
        const val: number = v;
    }
}
",
    );
}

#[test]
fn no_false_positive_ts2322_set_values_destructure() {
    assert_no_2322(
        "
function processSet(s: Set<number>) {
    for (const v of s) {
        const n: number = v;
    }
}
",
    );
}
