//! Ambient/abstract bodiless method overrides still get the TS2416/TS2417
//! override-compat check (#17649).
//!
//! When a derived class member's type is not assignable to the base class
//! member's type, tsc reports TS2416 (instance) / TS2417 (static) regardless
//! of whether the classes are ambient (`declare class`, classes inside
//! `declare namespace`, any `.d.ts`) or the method is `abstract`. tsz
//! previously classified every bodiless method declaration as an overload
//! *signature* and deferred its compat check to an implementation node that
//! never exists in those contexts, silently skipping the check.
//!
//! Every expectation is pinned against `typescript@7.0.2`
//! (`--noEmit --strict --target es2017 --lib es2022`). Binder names are
//! varied across tests so the behavior keys on the ambient/abstract shape,
//! not any identifier.

use crate::test_utils::{check_source_codes, check_source_codes_named};

const TS2416: u32 = 2416; // Property in type is not assignable to the same property in base type.
const TS2417: u32 = 2417; // Class static side incorrectly extends base class static side.

/// Filter to the override-compat codes so assertions stay immune to
/// unrelated harness noise (the unit harness has no lib).
fn compat_codes(source: &str) -> Vec<u32> {
    check_source_codes(source)
        .into_iter()
        .filter(|c| *c == TS2416 || *c == TS2417)
        .collect()
}

fn compat_codes_named(source: &str, file_name: &str) -> Vec<u32> {
    check_source_codes_named(source, file_name)
        .into_iter()
        .filter(|c| *c == TS2416 || *c == TS2417)
        .collect()
}

// --- The #17649 witness family: `declare class` singleton method ----------

#[test]
fn ambient_class_mismatched_method_override_reports_ts2416() {
    let source = r#"
type Sealed<T> = { valid: true; value: T };
type Deferred<T> = { inner: Sealed<T> };
declare abstract class Basis<T> {
  _parse(data: unknown): Deferred<T>;
}
declare class Offshoot extends Basis<number> {
  _parse(data: unknown): Deferred<string>;
}
"#;
    assert_eq!(compat_codes(source), vec![TS2416]);
}

#[test]
fn ambient_class_mismatched_method_override_reports_ts2416_renamed_binders() {
    let source = r#"
type Boxed<V> = { held: V };
declare abstract class Trunk<V> {
  fetch(key: string): Boxed<V>;
}
declare class Branch extends Trunk<boolean> {
  fetch(key: string): Boxed<string>;
}
"#;
    assert_eq!(compat_codes(source), vec![TS2416]);
}

#[test]
fn ambient_namespace_class_mismatched_method_override_reports_ts2416() {
    let source = r#"
declare namespace Vault {
  abstract class Store<T> {
    lookup(key: string): T;
  }
  class Bin extends Store<number> {
    lookup(key: string): string;
  }
}
"#;
    assert_eq!(compat_codes(source), vec![TS2416]);
}

#[test]
fn declaration_file_class_mismatched_method_override_reports_ts2416() {
    let source = r#"
declare abstract class Gauge<T> {
  read(input: unknown): T;
}
declare class Meter extends Gauge<number> {
  read(input: unknown): string;
}
"#;
    assert_eq!(compat_codes_named(source, "probe.d.ts"), vec![TS2416]);
}

#[test]
fn ambient_class_mismatched_static_method_reports_ts2417() {
    let source = r#"
declare class Ledger {
  static tally(): number;
}
declare class Journal extends Ledger {
  static tally(): string;
}
"#;
    assert_eq!(compat_codes(source), vec![TS2417]);
}

// --- Abstract bodiless methods outside ambient contexts ------------------

#[test]
fn abstract_mismatched_method_override_reports_ts2416() {
    let source = r#"
abstract class Root {
  abstract gauge(): number;
}
abstract class Leaf extends Root {
  abstract gauge(): string;
}
"#;
    assert_eq!(compat_codes(source), vec![TS2416]);
}

// --- Negatives: compatible or any-instantiated overrides stay clean -------

#[test]
fn ambient_class_compatible_method_override_stays_clean() {
    let source = r#"
type Carton<T> = { load: T };
declare abstract class Origin<T> {
  pull(data: unknown): Carton<T>;
}
declare class Descend extends Origin<number> {
  pull(data: unknown): Carton<number>;
}
"#;
    assert_eq!(compat_codes(source), Vec::<u32>::new());
}

#[test]
fn ambient_class_any_instantiated_method_override_stays_clean() {
    let source = r#"
type Husk<T> = { seed: T };
type Yielded<T> = { pod: Husk<T> };
declare abstract class Stem<T> {
  _parse(data: unknown): Yielded<T>;
}
declare class Shoot extends Stem<any> {
  _parse(data: unknown): Yielded<string>;
}
"#;
    assert_eq!(compat_codes(source), Vec::<u32>::new());
}

// --- Fences: existing behavior around bodies and overload sets ------------

#[test]
fn real_class_mismatched_method_override_still_reports_ts2416() {
    let source = r#"
type Latch<T> = { pin: T };
abstract class Frame<T> {
  mount(data: unknown): Latch<T> { return null as any; }
}
class Panel extends Frame<number> {
  mount(data: unknown): Latch<string> { return null as any; }
}
"#;
    assert_eq!(compat_codes(source), vec![TS2416]);
}

#[test]
fn overload_signatures_with_implementation_keep_single_combined_ts2416() {
    // tsc reports one TS2416 per overload/implementation declaration in the
    // derived member's own declaration set, not one combined diagnostic for
    // the whole set (pinned against tsc 7.0.2: three declarations here, three
    // TS2416s, each anchored at its own declaration).
    let source = r#"
class Spool {
  wind(x: string): number { return 1; }
}
class Reel extends Spool {
  wind(x: string): string;
  wind(x: number): string;
  wind(x: unknown): string { return "s"; }
}
"#;
    assert_eq!(compat_codes(source), vec![TS2416, TS2416, TS2416]);
}

#[test]
fn ambient_overload_set_keeps_single_combined_ts2416() {
    // Same rule as above: tsc reports one TS2416 per declaration in the
    // ambient overload set (two declarations here, two TS2416s), not one
    // combined diagnostic (pinned against tsc 7.0.2).
    let source = r#"
declare class Crate {
  probe(x: string): number;
}
declare class Parcel extends Crate {
  probe(x: string): string;
  probe(x: number): string;
}
"#;
    assert_eq!(compat_codes(source), vec![TS2416, TS2416]);
}
