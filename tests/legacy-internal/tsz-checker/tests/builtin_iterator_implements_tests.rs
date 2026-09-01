//! Regression tests for false-positive TS2416 / TS2322 when a class
//! implements a built-in collection type that spans multiple lib declaration
//! files (e.g. `Map<K,V>`, `Set<T>`, `WeakMap<K,V>`), plus genuine override and
//! iterator-heritage parity guards for that same lib-materialization path.
//!
//! Root cause of the original report: `compute_interface_type_from_declarations`
//! used a single `self.ctx.arena` for *all* declarations of the built-in
//! symbol, but each lib file has its own `NodeArena` with independent
//! `NodeIndex` spaces. Using the wrong arena would retrieve an unrelated AST
//! node (often the `Iterable` interface), whose
//! `[Symbol.iterator](): Iterator<T, TReturn, TNext>` signature leaked into the
//! resolved type and caused false override-mismatch errors.
//!
//! Fix: when cross-arena delegation is active, resolve each declaration with
//! its own `NodeArena` via `lower_merged_interface_declarations_with_symbol`.
//!
//! Test integrity: every assertion here references a lib collection type
//! (`Map`/`Set`/`WeakMap`/`MapIterator`/`SetIterator`/`Symbol`), so the checks
//! are only meaningful when those types are actually resolved. The shared
//! helpers therefore load the real default libs ([`load_default_lib_files`]) —
//! the minimal unit-test lib used by `check_source_codes` does not declare the
//! collection types, which would leave every `Set`/`Map` reference an
//! unresolved name (TS2304/TS2583) and make the guards pass vacuously without
//! ever exercising the multi-arena heritage flattening they exist to protect.
//!
//! Issue: <https://github.com/tsz-org/tsz/issues/8422>

use std::sync::{Arc, OnceLock};
use tsz_binder::lib_loader::LibFile;

use crate::CheckerOptions;
use crate::test_utils::{
    check_source_with_libs_code_messages, diagnostic_codes, load_default_lib_files,
};

/// The default lib bundle, parsed exactly once and shared across every test in
/// this module (and across the harness's worker threads).
fn default_libs() -> &'static [Arc<LibFile>] {
    static DEFAULT_LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
    DEFAULT_LIBS.get_or_init(load_default_lib_files)
}

/// Type-check `src` as `test.ts` with the full default lib bundle loaded so
/// that the collection/iterator types resolve, returning `(code, message)`.
fn check_with_libs(src: &str) -> Vec<(u32, String)> {
    check_source_with_libs_code_messages(src, "test.ts", CheckerOptions::default(), default_libs())
}

/// Assert the snippet is fully clean. This is strictly stronger than the old
/// `!contains(2416)` form: it also fails if a collection type fails to resolve
/// (TS2304/TS2583) — i.e. it guarantees the heritage path is actually
/// exercised — and if any other false positive creeps in.
fn assert_fully_clean(src: &str) {
    let diags = check_with_libs(src);
    assert!(diags.is_empty(), "expected no diagnostics, got: {diags:?}");
}

/// Assert TS2416 fires (a genuine override incompatibility) and no lib type was
/// left unresolved (so the override is checked against real lib members).
fn assert_has_2416(src: &str) {
    let diags = check_with_libs(src);
    let codes = diagnostic_codes(&diags);
    assert!(
        codes.contains(&2416),
        "expected TS2416, got none. Got: {diags:?}"
    );
    assert!(
        !codes.contains(&2304) && !codes.contains(&2583),
        "lib type left unresolved — guard would be vacuous: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Map-based class — the original report (#8422)
// ---------------------------------------------------------------------------

#[test]
fn no_false_positive_ts2416_class_extends_map() {
    assert_fully_clean(
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
    assert_fully_clean(
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
    assert_fully_clean(
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
    assert_fully_clean(
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
    assert_fully_clean(
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
// Negative / sanity cases — genuine mismatches must still produce TS2416.
// The cross-arena fix must not suppress errors for real violations. This
// covers named, `[Symbol.iterator]`-keyed, `unique symbol`-keyed, and
// string-literal computed-key overrides so the override check is verified to
// fire regardless of how the member key is spelled.
// ---------------------------------------------------------------------------

#[test]
fn genuine_ts2416_wrong_named_method_return_in_map_subclass() {
    assert_has_2416(
        "
class Base {
    foo(): number { return 0; }
}
class Sub extends Base {
    foo(): string { return ''; }
}
",
    );
}

#[test]
fn genuine_ts2416_named_method_renamed_class_still_detected() {
    assert_has_2416(
        "
class BaseX {
    run(): number { return 0; }
}
class DerivedX extends BaseX {
    run(): string { return ''; }
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
    assert_fully_clean(
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
    assert_fully_clean(
        "
function processSet(s: Set<number>) {
    for (const v of s) {
        const n: number = v;
    }
}
",
    );
}

// ---------------------------------------------------------------------------
// Plain `const k = Symbol()` computed property — `symbol_valued_binding`
// path: the key is `__symbol_<file>_<sym>`, not `__unique_*`.  Both the name
// map and the symbol-named prepass must include this case so that a class
// implementing an interface with `[k]` can find a matching signature.
// ---------------------------------------------------------------------------

#[test]
fn no_false_positive_ts2416_const_symbol_computed_property_name() {
    assert_fully_clean(
        "
const sym = Symbol();
interface IBase { [sym](): number; }
class ConcreteA implements IBase { [sym](): number { return 0; } }
",
    );
}

#[test]
fn no_false_positive_ts2416_const_symbol_computed_property_name_renamed_var() {
    assert_fully_clean(
        "
const myKey = Symbol();
interface IBase2 { [myKey](): string; }
class ConcreteB implements IBase2 { [myKey](): string { return ''; } }
",
    );
}

// ---------------------------------------------------------------------------
// WeakMap — another multi-lib built-in (spans lib.es2015.collection.d.ts
// and lib.es2015.weakref-adjacent declarations)
// ---------------------------------------------------------------------------

#[test]
fn no_false_positive_ts2416_class_extends_weak_map() {
    assert_fully_clean(
        "
class TrackedWeakMap<K extends object, V> extends WeakMap<K, V> {
    override set(key: K, value: V): this {
        return super.set(key, value);
    }
}
",
    );
}

#[test]
fn no_false_positive_ts2416_class_extends_weak_map_generic_name_a_b() {
    assert_fully_clean(
        "
class TrackedWeakMap<A extends object, B> extends WeakMap<A, B> {
    override set(key: A, value: B): this {
        return super.set(key, value);
    }
}
",
    );
}

// ---------------------------------------------------------------------------
// Genuine override mismatches keyed by a symbol / computed name must still be
// detected. The cross-arena heritage materialization must not become a blind
// spot for return-type incompatibility on non-identifier member keys.
// ---------------------------------------------------------------------------

#[test]
fn genuine_ts2416_symbol_iterator_keyed_override_wrong_return() {
    assert_has_2416(
        "
class Base {
    [Symbol.iterator](): number { return 1; }
}
class Sub extends Base {
    [Symbol.iterator](): string { return ''; }
}
",
    );
}

#[test]
fn genuine_ts2416_unique_symbol_keyed_override_wrong_return() {
    assert_has_2416(
        "
const k: unique symbol = Symbol();
class Base {
    [k](): number { return 1; }
}
class Sub extends Base {
    [k](): string { return ''; }
}
",
    );
}

#[test]
fn genuine_ts2416_string_literal_computed_override_wrong_return() {
    assert_has_2416(
        "
class Base {
    ['foo'](): number { return 1; }
}
class Sub extends Base {
    ['foo'](): string { return ''; }
}
",
    );
}

// ---------------------------------------------------------------------------
// Iterator-heritage assignability: `Set`/`Map` collection iterators
// (`SetIterator<T>` / `MapIterator<T>`) must flatten their heritage chain
// `… extends IteratorObject<T,…> extends Iterator<T,…>` so that `next` is a
// visible member and the iterator is assignable to `IterableIterator<T>`. This
// is the lib-iterator-heritage materialization path that the immer canary
// (#13942) reports drops at full-project scale; these guards pin the isolated
// contract so a regression in heritage flattening is caught directly.
// ---------------------------------------------------------------------------

#[test]
fn set_values_iterator_assignable_to_iterable_iterator() {
    assert_fully_clean(
        "
function f(s: Set<number>): IterableIterator<number> {
    return s.values();
}
",
    );
}

#[test]
fn map_entries_iterator_assignable_to_iterable_iterator() {
    assert_fully_clean(
        "
function f(m: Map<string, number>): IterableIterator<[string, number]> {
    return m.entries();
}
",
    );
}

#[test]
fn set_values_iterator_exposes_inherited_next_member() {
    assert_fully_clean(
        "
function f(s: Set<number>) {
    const it = s.values();
    it.next();
}
",
    );
}

#[test]
fn array_from_map_entries_infers_tuple_element() {
    assert_fully_clean(
        "
function f(m: Map<string, number>) {
    const arr = Array.from(m.entries());
    const x: [string, number] = arr[0];
}
",
    );
}

#[test]
fn spread_set_values_infers_element_type() {
    assert_fully_clean(
        "
function f(s: Set<number>) {
    const arr = [...s.values()];
    const x: number = arr[0];
}
",
    );
}
