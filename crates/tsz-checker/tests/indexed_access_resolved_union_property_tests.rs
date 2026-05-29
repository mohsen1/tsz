//! TS2339 must fire on property access through an `IndexAccess` whose
//! evaluated receiver is a closed union, even though the display form
//! retains the `E[K]` shape.
//!
//! Pre-fix: `diagnostic_display_type_for_missing_property` returned the
//! `IndexAccess` `narrowed` form when the apparent type was a union, then
//! `error_property_not_exist_at` blanket-suppressed all `IndexAccess` types,
//! silencing the diagnostic on a genuine missing-property situation.
//!
//! Mirrors the `quickinfoTypeAtReturnPositionsInaccurate.ts` conformance
//! reproducer.

use tsz_common::diagnostics::Diagnostic;

fn check(source: &str) -> Vec<Diagnostic> {
    tsz_checker::test_utils::check_source_diagnostics(source)
}

/// `class Store<E extends { [k: string]: A | B }>` with a method that
/// reads `this.entries[k]: E[K]`. Property access on the resolved union
/// `A | B` should emit `TS2339` for properties absent on every member.
#[test]
fn ts2339_fires_on_indexed_access_resolving_to_closed_union_missing_property() {
    let source = r#"
class A { foo(): void {} }
class B { bar(): void {} }
class Store<E extends { [k: string]: A | B }> {
  private entries = {} as E;
  get<K extends keyof E>(k: K) {
    let entry = this.entries[k];
    entry.notexist();
  }
}
"#;
    let diags = check(source);
    assert!(
        diags.iter().any(|d| {
            d.code == 2339
                && d.message_text.contains("notexist")
                && d.message_text.contains("type 'A | B'")
        }),
        "Expected TS2339 for `entry.notexist()` on `A | B` (resolved E[K]), got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Bare type-parameter receivers — no `IndexAccess` wrapping — should keep
/// emitting TS2339 as before. Pin the unrelated branch.
#[test]
fn ts2339_fires_on_bare_type_parameter_constrained_to_union() {
    let source = r#"
class A { foo(): void {} }
class B { bar(): void {} }
function f<T extends A | B>(x: T) {
  x.notexist();
}
"#;
    let diags = check(source);
    assert!(
        diags
            .iter()
            .any(|d| { d.code == 2339 && d.message_text.contains("notexist") }),
        "Expected TS2339 on bare T extends A|B receiver, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn flow_narrowed_indexed_access_identifier_returns_to_declared_indexed_access() {
    let source = r#"
class NumBox<T extends number> {
  private value!: T;
  get(): T { return this.value; }
  onlyNum(): void {}
}
class StrBox<T extends string> {
  private value!: T;
  get(): T { return this.value; }
  onlyStr(): void {}
}
const isNumBox = <Item extends NumBox<number> | StrBox<string>>(
  item: Item
): item is Extract<Item, NumBox<any>> => item instanceof NumBox;

type Bag = { [index: string]: NumBox<number> | StrBox<string> };
class Store<Bags extends { [index: string]: Bag }> {
  private bags = {} as Bags;
  get<BagId extends keyof Bags, BagKey extends keyof Bags[BagId]>(
    bagId: BagId,
    bagKey: BagKey
  ): Bags[BagId][BagKey] {
    let item = this.bags[bagId][bagKey];
    if (isNumBox(item)) {
      item.onlyNum();
    }
    item.get();
    return item;
  }
}
"#;
    let diags = check(source);
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "Flow-narrowed generic indexed-access return should not emit TS2322, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn quickinfo_return_position_conformance_shape_keeps_only_expected_missing_property() {
    let source = r#"
class Alpha<T extends number> {
  private value!: T;
  get(): T { return this.value; }
  alpha(): void {}
}
class Beta<T extends string> {
  private value!: T;
  get(): T { return this.value; }
  beta(): void {}
}
const isAlpha = <Candidate extends Alpha<number> | Beta<string>>(
  candidate: Candidate
): candidate is Extract<Candidate, Alpha<any>> => candidate instanceof Alpha;

class Simple<Entries extends { [index: string]: Alpha<number> | Beta<string> }> {
  private entries = {} as Entries;
  get<EntryId extends keyof Entries>(entryId: EntryId): Entries[EntryId] {
    let entry = this.entries[entryId];
    entry.alpha();
    if (isAlpha(entry)) {
      return entry;
    }
    return entry;
  }
}

type Slice = { [index: string]: Alpha<number> | Beta<string> };
class Complex<Slices extends { [index: string]: Slice }> {
  private slices = {} as Slices;
  get<SliceId extends keyof Slices, SliceKey extends keyof Slices[SliceId]>(
    sliceId: SliceId,
    sliceKey: SliceKey
  ): Slices[SliceId][SliceKey] {
    let item = this.slices[sliceId][sliceKey];
    if (isAlpha(item)) {
      item.alpha();
    }
    item.get();
    return item;
  }
}
"#;
    let diags = check(source);
    let codes: Vec<_> = diags.iter().map(|d| d.code).collect();
    assert_eq!(
        codes,
        vec![2339],
        "Expected only TS2339 for the pre-guard missing property, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}
