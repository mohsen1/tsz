//! Tests for class `implements` with index signatures (TS2420/TS2411 interaction).
//!
//! When a class declares a compatible index signature and implements an interface
//! whose only contract is that index signature, tsz must not emit TS2420.
//! Extra class properties that violate the class's own index constraint produce
//! TS2411 separately — they do not constitute a failure to implement.

use tsz_checker::test_utils::check_source_code_messages as get_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    get_diagnostics(source).iter().map(|(c, _)| *c).collect()
}

#[test]
fn fbounded_implements_not_corrupted_by_recursive_alias_context() {
    // Regression for issue #6557: recursive interface and recursive alias
    // context must not corrupt the F-bounded Comparable<T> member lookup.
    let source = r#"
interface TreeNode2 {
  value: number;
  left?: TreeNode2;
  right?: TreeNode2;
}

const tree: TreeNode2 = { value: 1, left: { value: 2 }, right: { value: 3, left: { value: 4 } } };

type Json = string | number | boolean | null | Json[] | { [key: string]: Json };

const json: Json = { name: "test", values: [1, 2, { nested: true }], active: true };

interface Comparable<T extends Comparable<T>> {
  compareTo(other: T): number;
}

class MyNumber implements Comparable<MyNumber> {
  constructor(public value: number) {}
  compareTo(other: MyNumber): number { return this.value - other.value; }
}
"#;
    let cs = codes(source);
    assert!(
        !cs.contains(&2420),
        "expected no TS2420 for valid F-bounded implements; got codes: {cs:?}"
    );
}

// ── Regression: issue #6370 ──────────────────────────────────────────────

#[test]
fn class_with_matching_string_index_sig_no_ts2420() {
    // tsc emits only TS2411 for the `get` method violating the index constraint;
    // it does NOT emit TS2420 because the class declares the matching index sig.
    let source = r#"
interface Dictionary {
  [key: string]: string;
}

class StringDict implements Dictionary {
  [key: string]: string;
  get(key: string): string {
    return this[key] ?? "";
  }
}
"#;
    let cs = codes(source);
    assert!(
        !cs.contains(&2420),
        "expected no TS2420 when class declares a matching index signature; got codes: {cs:?}"
    );
    assert!(
        cs.contains(&2411),
        "expected TS2411 for `get` violating the string index constraint; got codes: {cs:?}"
    );
}

#[test]
fn class_with_matching_index_sig_and_extra_properties_no_ts2420() {
    // Any number of extra named properties that violate the index constraint should
    // produce TS2411 (one per property) but never TS2420.
    let source = r#"
interface Storage {
  [key: string]: string;
}

class AppStorage implements Storage {
  [key: string]: string;
  serialize(): string { return ""; }
  deserialize(): string { return ""; }
}
"#;
    let cs = codes(source);
    assert!(
        !cs.contains(&2420),
        "expected no TS2420 with multiple extra methods; got codes: {cs:?}"
    );
}

#[test]
fn class_with_number_index_sig_implements_number_indexed_interface() {
    let source = r#"
interface NumericStore {
  [index: number]: number;
}

class NumberStore implements NumericStore {
  [index: number]: number;
  byName(key: string): number { return 0; }
}
"#;
    let cs = codes(source);
    assert!(
        !cs.contains(&2420),
        "expected no TS2420 for class with matching number index sig; got codes: {cs:?}"
    );
}

// ── Correct errors preserved ─────────────────────────────────────────────

#[test]
fn class_without_index_sig_gets_ts2420() {
    // Class has no index signature at all → should still get TS2420.
    let source = r#"
interface Dictionary {
  [key: string]: string;
}

class NoIndexSig implements Dictionary {
  foo: string = "bar";
}
"#;
    let cs = codes(source);
    assert!(
        cs.contains(&2420),
        "expected TS2420 when class has no index signature; got codes: {cs:?}"
    );
}

#[test]
fn class_with_incompatible_index_sig_value_type_gets_ts2420() {
    // Class declares `[key: string]: number` but interface requires `[key: string]: string`.
    let source = r#"
interface StringDict {
  [key: string]: string;
}

class NumberDict implements StringDict {
  [key: string]: number;
}
"#;
    let cs = codes(source);
    assert!(
        cs.contains(&2420) || cs.iter().any(|&c| c == 2415 || c == 2416),
        "expected a compatibility error when class index sig value type is incompatible; got codes: {cs:?}"
    );
}

// ── Interaction: interface with both named properties and an index sig ────

#[test]
fn class_with_index_sig_plus_named_props_satisfies_mixed_interface() {
    // When the interface has both named properties and an index sig, the class
    // must satisfy all of them.  No TS2420 when everything lines up.
    let source = r#"
interface Named {
  [key: string]: string;
  name: string;
}

class Person implements Named {
  [key: string]: string;
  name: string = "Alice";
  greet(): string { return "hi"; }
}
"#;
    let cs = codes(source);
    assert!(
        !cs.contains(&2420),
        "expected no TS2420 when class satisfies both named props and index sig; got codes: {cs:?}"
    );
}

#[test]
fn class_missing_named_prop_from_mixed_interface_gets_ts2420() {
    // The class has the index sig but is missing the required named property.
    let source = r#"
interface Named {
  [key: string]: string;
  name: string;
}

class NoName implements Named {
  [key: string]: string;
}
"#;
    let cs = codes(source);
    assert!(
        cs.contains(&2420),
        "expected TS2420 when class is missing a required named property; got codes: {cs:?}"
    );
}
