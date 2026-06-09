//! Unit tests for `TypeInterner::tuple` inlining concrete tuple spreads
//! (`createNormalizedTupleType`).
//!
//! Structural rule: a rest element `...X` whose type is a concrete tuple
//! contributes a statically known run of elements, so `[A, ...[B, C]]` is
//! exactly `[A, B, C]`. A fixed inner tuple is always inlined; a variadic inner
//! tuple is inlined only as the parent's sole, last rest (keeping ≤ 1 rest); a
//! lone `[...X]` is left compressed; rest arrays are never inlined.

use crate::intern::TypeInterner;
use crate::types::{TupleElement, TypeData, TypeId};

fn fixed(type_id: TypeId) -> TupleElement {
    TupleElement::fixed(type_id)
}

fn rest(type_id: TypeId) -> TupleElement {
    TupleElement::rest(type_id)
}

fn tuple_elements(interner: &TypeInterner, type_id: TypeId) -> Vec<TupleElement> {
    match interner.lookup(type_id) {
        Some(TypeData::Tuple(list_id)) => interner.tuple_list(list_id).to_vec(),
        other => panic!("expected tuple, got {other:?}"),
    }
}

#[test]
fn fixed_tuple_spread_is_inlined() {
    let interner = TypeInterner::new();
    let inner = interner.tuple(vec![fixed(TypeId::NUMBER), fixed(TypeId::BOOLEAN)]);
    let outer = interner.tuple(vec![fixed(TypeId::STRING), rest(inner)]);

    let elements = tuple_elements(&interner, outer);
    let expected = interner.tuple(vec![
        fixed(TypeId::STRING),
        fixed(TypeId::NUMBER),
        fixed(TypeId::BOOLEAN),
    ]);
    assert_eq!(
        outer, expected,
        "[string, ...[number, boolean]] = [string, number, boolean]"
    );
    assert_eq!(elements.len(), 3);
    assert!(elements.iter().all(|elem| !elem.rest));
}

#[test]
fn readonly_fixed_tuple_spread_is_inlined() {
    let interner = TypeInterner::new();
    let inner = interner.tuple(vec![fixed(TypeId::NUMBER)]);
    let readonly_inner = interner.readonly_type(inner);
    let outer = interner.tuple(vec![fixed(TypeId::STRING), rest(readonly_inner)]);

    let expected = interner.tuple(vec![fixed(TypeId::STRING), fixed(TypeId::NUMBER)]);
    assert_eq!(
        outer, expected,
        "readonly is unwrapped before inlining the spread"
    );
}

#[test]
fn variadic_inner_tuple_inlines_as_sole_last_rest() {
    let interner = TypeInterner::new();
    // inner = [boolean, ...number[]]
    let inner = interner.tuple(vec![
        fixed(TypeId::BOOLEAN),
        rest(interner.array(TypeId::NUMBER)),
    ]);
    let outer = interner.tuple(vec![fixed(TypeId::STRING), rest(inner)]);

    let elements = tuple_elements(&interner, outer);
    // [string, ...[boolean, ...number[]]] = [string, boolean, ...number[]]
    assert_eq!(elements.len(), 3);
    assert_eq!(elements[0].type_id, TypeId::STRING);
    assert!(!elements[0].rest);
    assert_eq!(elements[1].type_id, TypeId::BOOLEAN);
    assert!(!elements[1].rest);
    assert!(
        elements[2].rest,
        "the inner array rest is preserved as the single trailing rest"
    );
}

#[test]
fn variadic_inner_tuple_not_last_is_left_compressed() {
    let interner = TypeInterner::new();
    // inner = [boolean, ...number[]] spliced would put a rest before a trailing
    // fixed element, creating a second rest — so it must stay un-inlined.
    let inner = interner.tuple(vec![
        fixed(TypeId::BOOLEAN),
        rest(interner.array(TypeId::NUMBER)),
    ]);
    let outer = interner.tuple(vec![rest(inner), fixed(TypeId::STRING)]);

    let elements = tuple_elements(&interner, outer);
    assert_eq!(
        elements.len(),
        2,
        "the variadic spread is kept compressed when not last"
    );
    assert!(elements[0].rest);
    assert_eq!(elements[0].type_id, inner);
}

#[test]
fn sole_rest_spread_is_left_compressed() {
    let interner = TypeInterner::new();
    // A large `[...inner]` must not eagerly expand — it already denotes `inner`.
    let inner = interner.tuple((0..512).map(|_| fixed(TypeId::NUMBER)).collect());
    let outer = interner.tuple(vec![rest(inner)]);

    let elements = tuple_elements(&interner, outer);
    assert_eq!(elements.len(), 1);
    assert!(elements[0].rest);
    assert_eq!(elements[0].type_id, inner);
}

#[test]
fn rest_array_spread_is_not_inlined() {
    let interner = TypeInterner::new();
    let outer = interner.tuple(vec![
        fixed(TypeId::STRING),
        rest(interner.array(TypeId::NUMBER)),
    ]);

    let elements = tuple_elements(&interner, outer);
    assert_eq!(elements.len(), 2);
    assert!(elements[1].rest);
}

#[test]
fn empty_tuple_spread_collapses_away() {
    let interner = TypeInterner::new();
    let empty = interner.tuple(vec![]);
    let outer = interner.tuple(vec![fixed(TypeId::STRING), rest(empty)]);

    let expected = interner.tuple(vec![fixed(TypeId::STRING)]);
    assert_eq!(outer, expected, "[string, ...[]] = [string]");
}
