//! TS2739/TS2740/TS2741 missing-property list parity for well-known-symbol
//! members and the implicit `Object.prototype` fallback.
//!
//! Structural rule (owner: solver `explain_object_failure` missing pass +
//! checker display sort): tsc's `getUnmatchedProperties` resolves every
//! target member through `getPropertyOfType(source, …)`, whose lookup falls
//! back to the global `Object` interface — so `toString`/`toLocaleString`
//! (and any other Object-interface member a target redeclares) never appear
//! in a missing list, by *presence* alone (an own incompatible member still
//! counts as present). Well-known-symbol members (`[Symbol.iterator]`,
//! `[Symbol.unscopables]`, …) get no such exemption: they are counted and
//! listed like any other member, and tsc appends late-bound (symbol-keyed)
//! members after every early-bound (string-keyed) member in the rendered
//! list.
//!
//! Witnesses: `conformance/es6/destructuring/iterableArrayPattern18.ts` /
//! `19.ts` pinned against `typescript@7.0.2` (`Type 'FooIterator' is missing
//! the following properties from type 'Bar[]': length, pop, push, concat,
//! and 24 more.` — 24, not 25, because the source's own `[Symbol.iterator]`
//! matches while `[Symbol.unscopables]` stays missing), cross-checked
//! against a local `tsc` 6.0.2 for every shape below.
//!
//! The exact totals against `Array<T>` depend on which lib files are loaded,
//! so the array-target tests pin the *invariants* rather than one absolute
//! count: supplying a symbol member reduces the count by exactly one;
//! supplying `toString` changes nothing (already exempt via the fallback);
//! `toString`/`toLocaleString` never render. Short-list tests against
//! source-declared interface targets pin exact strings. The conformance
//! suite pins the absolute lib-dependent counts.
//!
//! The decision is structural, never keyed on a binder's name: the renamed-
//! binder cases write the same shapes with different identifiers and expect
//! the same behavior.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

/// All diagnostic `(code, message_text)` rows for `source` checked
/// non-strict with the default lib bundle.
fn rows(source: &str) -> Vec<(u32, String)> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: false,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

/// The single missing-properties message ("… is missing the following
/// properties from type …") in `source`'s diagnostics.
fn missing_list_message(source: &str) -> String {
    let all = rows(source);
    let mut hits = all
        .iter()
        .filter(|(_, m)| m.contains("is missing the following properties from type"));
    let first = hits
        .next()
        .unwrap_or_else(|| panic!("expected a missing-properties diagnostic, got {all:?}"))
        .1
        .clone();
    assert!(
        hits.next().is_none(),
        "expected exactly one missing-properties diagnostic, got {all:?}"
    );
    first
}

/// The `N` of a trailing "and N more." in a missing-properties message.
fn more_count(message: &str) -> usize {
    let tail = message
        .rsplit_once("and ")
        .unwrap_or_else(|| panic!("no 'and N more.' tail in {message:?}"))
        .1;
    tail.strip_suffix(" more.")
        .unwrap_or_else(|| panic!("no 'and N more.' tail in {message:?}"))
        .parse()
        .unwrap_or_else(|_| panic!("non-numeric 'and N more.' tail in {message:?}"))
}

const PLAIN_SOURCE: &str = r#"
class Elem { marker = 1 }
class Plain { other = 2 }
var xs: Elem[] = new Plain;
"#;

const ITERATOR_SOURCE: &str = r#"
class Elem { marker = 1 }
class WithIter {
    next() { return { value: new Elem, done: false }; }
    [Symbol.iterator]() { return this; }
}
var xs: Elem[] = new WithIter;
"#;

const BOTH_SYMBOLS_SOURCE: &str = r#"
class Elem { marker = 1 }
class WithBoth {
    next() { return { value: new Elem, done: false }; }
    [Symbol.iterator]() { return this; }
    [Symbol.unscopables]() { return {} as any; }
}
var xs: Elem[] = new WithBoth;
"#;

const OWN_TOSTRING_SOURCE: &str = r#"
class Elem { marker = 1 }
class WithToString { toString(): number { return 1 } }
var xs: Elem[] = new WithToString;
"#;

#[test]
fn array_target_counts_a_missing_well_known_symbol_member() {
    // The iterator-bearing source matches `Array<T>`'s `[Symbol.iterator]`,
    // so exactly one fewer member is missing than for the plain source —
    // `[Symbol.unscopables]` alone stays in the count
    // (iterableArrayPattern18/19's 25-vs-24 drift).
    let plain = more_count(&missing_list_message(PLAIN_SOURCE));
    let with_iter = more_count(&missing_list_message(ITERATOR_SOURCE));
    assert_eq!(with_iter + 1, plain);
}

#[test]
fn array_target_counts_each_symbol_member_separately() {
    let plain = more_count(&missing_list_message(PLAIN_SOURCE));
    let with_both = more_count(&missing_list_message(BOTH_SYMBOLS_SOURCE));
    assert_eq!(with_both + 2, plain);
}

#[test]
fn own_incompatible_tostring_changes_nothing() {
    // `toString` is exempt through the `Object.prototype` presence fallback
    // whether or not the source declares its own (even an incompatible one),
    // so the count is identical to the plain source.
    let plain = more_count(&missing_list_message(PLAIN_SOURCE));
    let with_tostring = more_count(&missing_list_message(OWN_TOSTRING_SOURCE));
    assert_eq!(with_tostring, plain);
}

#[test]
fn object_prototype_members_never_render_in_the_list() {
    for source in [
        PLAIN_SOURCE,
        ITERATOR_SOURCE,
        BOTH_SYMBOLS_SOURCE,
        OWN_TOSTRING_SOURCE,
    ] {
        let message = missing_list_message(source);
        assert!(
            message.contains("length, pop, push, concat"),
            "expected tsc's canonical array head in {message:?}"
        );
        assert!(
            !message.contains("toString") && !message.contains("toLocaleString"),
            "Object.prototype members must not render as missing in {message:?}"
        );
    }
}

#[test]
fn short_list_orders_symbol_members_last() {
    // tsc resolves early-bound members first and appends late-bound
    // (symbol-keyed) members, regardless of declaration order.
    let source = r#"
interface HasIter { [Symbol.iterator](): any; alpha: number; beta: string; }
class Bare { }
var h: HasIter = new Bare;
"#;
    let message = missing_list_message(source);
    assert!(
        message.ends_with("alpha, beta, [Symbol.iterator]"),
        "expected symbol member listed last, got {message:?}"
    );
}

#[test]
fn short_list_orders_symbol_members_last_renamed_binders() {
    let source = r#"
interface Wants { [Symbol.asyncIterator](): any; first: boolean; second: object; }
class Empty2 { }
var w: Wants = new Empty2;
"#;
    let message = missing_list_message(source);
    assert!(
        message.ends_with("first, second, [Symbol.asyncIterator]"),
        "expected symbol member listed last, got {message:?}"
    );
}

#[test]
fn lone_missing_symbol_member_reports_ts2741() {
    let source = r#"
interface OnlyIter { [Symbol.iterator](): any; }
class Src3 { z: number = 1 }
var o: OnlyIter = new Src3;
"#;
    let all = rows(source);
    assert!(
        all.iter().any(|(code, m)| *code == 2741
            && m.contains("Property '[Symbol.iterator]' is missing in type 'Src3'")),
        "expected TS2741 for the lone missing symbol member, got {all:?}"
    );
}

#[test]
fn redeclared_object_member_is_exempt_on_plain_interface_targets() {
    // A non-array target redeclaring `valueOf` still gets the fallback:
    // only `alpha`/`beta` are missing.
    let source = r#"
interface Wants { valueOf(): Object; alpha: number; beta: string; }
class Src { }
var w: Wants = new Src;
"#;
    let all = rows(source);
    assert!(
        all.iter()
            .any(|(code, m)| *code == 2739 && m.ends_with("alpha, beta") && !m.contains("valueOf")),
        "expected TS2739 listing exactly alpha, beta, got {all:?}"
    );
}

#[test]
fn compatible_iterable_source_stays_clean() {
    // Negative control: the same iterator-bearing class satisfies
    // `Iterable<T>` — the symbol member matches, nothing is reported.
    let source = r#"
class YieldsNum {
    next() { return { value: 1, done: false }; }
    [Symbol.iterator]() { return this; }
}
var i: Iterable<number> = new YieldsNum;
"#;
    let all = rows(source);
    assert!(
        all.iter()
            .all(|(code, _)| *code != 2322 && *code != 2739 && *code != 2740 && *code != 2741),
        "expected no assignability diagnostics, got {all:?}"
    );
}
