//! Required `never` properties do not make an object union member impossible.
//!
//! TypeScript preserves `{ marker: never; value: T }` as an object type: the
//! property makes ordinary structural construction difficult, but values can
//! still exist through declarations, class instances, assertions, and generic
//! constraints. Only a conflicting discriminant intersection is reduced to
//! `never`. Union pruning must therefore retain required-`never` object and
//! class members while continuing to remove provably disjoint intersections.

use tsz_checker::test_utils::check_source_strict_codes;

fn codes(source: &str) -> Vec<u32> {
    check_source_strict_codes(source)
}

#[test]
fn object_union_retains_member_with_required_never_property() {
    let source = r#"
declare const value:
  | { marker: never; common: string }
  | { common: number };

const mustReject: number = value.common;
"#;

    assert_eq!(
        codes(source),
        vec![2322],
        "the read type must remain `string | number`"
    );
}

#[test]
fn generic_nullable_class_with_required_never_property_remains_callable() {
    let source = r#"
abstract class Schema {
  readonly marker!: never;
  abstract parse(): number;
}

function read<Value extends Schema | null>(value: Value) {
  if (value) {
    const result: number = value.parse();
  }
}
"#;

    assert!(
        codes(source).is_empty(),
        "truthiness must retain the class-constrained receiver"
    );
}

#[test]
fn constrained_tuple_elements_with_required_never_property_remain_callable() {
    let source = r#"
abstract class Validator {
  readonly brand!: never;
  abstract validate(): string;
}

function validateAll<Items extends [Validator, ...Validator[]]>(items: Items) {
  for (const item of items) {
    const result: string = item.validate();
  }
}
"#;

    assert!(
        codes(source).is_empty(),
        "tuple element constraints must retain the class methods"
    );
}

#[test]
fn conflicting_discriminant_intersection_is_still_impossible() {
    let source = r#"
type Impossible = { kind: "left"; common: string }
  & { kind: "right"; common: boolean };

declare const value: Impossible | { common: number };
const result: number = value.common;
"#;

    assert!(
        codes(source).is_empty(),
        "disjoint literal discriminants must still remove the impossible branch"
    );
}
