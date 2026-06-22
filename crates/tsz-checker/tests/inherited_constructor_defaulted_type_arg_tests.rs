//! Regression tests for inherited-constructor instantiation of a base class
//! whose defaulted type parameter references an earlier parameter.
//!
//! Structural rule (owner: `class_type/constructor.rs`): when a base class has a
//! defaulted type parameter whose default references an earlier parameter
//! (`class Base<T, Items = T[]>`) and a subclass supplies only the earlier
//! argument (`class Sub extends Base<number>`), the inherited-constructor path
//! must instantiate the default through the substitution being built so the
//! earlier argument (`T = number`) propagates into the default (`Items =
//! number[]`). The three missing-type-arg fill loops previously pushed the raw
//! default (`T[]`) with `T` left free, so `new Sub([1, 2, 3])` checked
//! `number[]` against an abstract `T[]` and drew a spurious TS2322. This mirrors
//! the already-correct instance-side path in `class_type/instance_merge.rs`.

use tsz_checker::test_utils::check_source_codes;

fn codes(source: &str) -> Vec<u32> {
    let mut c = check_source_codes(source);
    c.sort_unstable();
    c.dedup();
    c
}

#[test]
fn inherited_ctor_array_default_resolves_earlier_param() {
    assert!(
        codes(
            r#"
class Base<T, Items = T[]> {
  constructor(public items: Items) {}
}
class IntList extends Base<number> {}
const il = new IntList([1, 2, 3]);
"#,
        )
        .is_empty(),
        "Items = T[] should resolve to number[] in the inherited constructor",
    );
}

#[test]
fn inherited_ctor_object_shape_default_resolves_earlier_param() {
    assert!(
        codes(
            r#"
class Base<T, S = { value: T; list: T[] }> {
  constructor(public s: S) {}
}
class C extends Base<number> {}
const c = new C({ value: 1, list: [2, 3] });
"#,
        )
        .is_empty(),
        "object-shape default referencing T should resolve to number",
    );
}

#[test]
fn inherited_ctor_default_resolves_earlier_param_renamed_binders() {
    // Not keyed on `T`/`Items` names.
    assert!(
        codes(
            r#"
class Base<A, B = A[]> {
  constructor(public b: B) {}
}
class Sub extends Base<string> {}
const s = new Sub(["a", "b"]);
"#,
        )
        .is_empty(),
        "renamed defaulted-param reference should resolve",
    );
}

#[test]
fn direct_instantiation_with_defaulted_param_still_clean() {
    // Control: direct instantiation (not via a subclass) already worked.
    assert!(
        codes(
            r#"
class Base<T, Items = T[]> {
  constructor(public items: Items) {}
}
const b = new Base<number>([1, 2, 3]);
"#,
        )
        .is_empty(),
        "direct instantiation should remain clean",
    );
}

#[test]
fn inherited_ctor_incompatible_argument_still_errors() {
    // Negative control: a genuinely wrong element type must still error — the
    // fix resolves the default, it does not suppress real mismatches.
    assert!(
        codes(
            r#"
class Base<T, Items = T[]> {
  constructor(public items: Items) {}
}
class IntList extends Base<number> {}
const il = new IntList(["a", "b"]);
"#,
        )
        .contains(&2322),
        "a string[] argument to a number[] parameter must still draw TS2322",
    );
}
