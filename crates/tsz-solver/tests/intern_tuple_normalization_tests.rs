use super::*;

// =============================================================================
// Adjacent rest-array normalization in tuple_normalized()
// tuple()         → raw construction, no adjacent-rest merging
// tuple_normalized() → instantiation path, merges adjacent concrete rest arrays
// =============================================================================

fn rest_elem(type_id: TypeId) -> TupleElement {
    TupleElement {
        type_id,
        name: None,
        optional: false,
        rest: true,
    }
}

fn fixed_elem(type_id: TypeId) -> TupleElement {
    TupleElement {
        type_id,
        name: None,
        optional: false,
        rest: false,
    }
}

/// `[...number[], ...string[]]` → `(number | string)[]` via `tuple_normalized`.
/// Two adjacent unbounded rest arrays collapse to a plain array with a union element type.
#[test]
fn tuple_two_adjacent_rest_arrays_collapse_to_array() {
    let db = TypeInterner::new();
    let num_arr = db.array(TypeId::NUMBER);
    let str_arr = db.array(TypeId::STRING);

    let result = db.tuple_normalized(vec![rest_elem(num_arr), rest_elem(str_arr)]);

    let expected_elem = db.union(vec![TypeId::NUMBER, TypeId::STRING]);
    let expected = db.array(expected_elem);
    assert_eq!(
        result, expected,
        "[...number[], ...string[]] should normalize to (number | string)[]"
    );
}

/// Same structural rule with renamed element types (`boolean[]` and `number[]`).
/// The fix must be structural, not tied to specific type names.
#[test]
fn tuple_adjacent_rest_arrays_renamed_types() {
    let db = TypeInterner::new();
    let bool_arr = db.array(TypeId::BOOLEAN);
    let num_arr = db.array(TypeId::NUMBER);

    let result = db.tuple_normalized(vec![rest_elem(bool_arr), rest_elem(num_arr)]);

    let expected_elem = db.union(vec![TypeId::BOOLEAN, TypeId::NUMBER]);
    let expected = db.array(expected_elem);
    assert_eq!(
        result, expected,
        "[...boolean[], ...number[]] should normalize to (boolean | number)[]"
    );
}

/// Three adjacent rest arrays → `(A | B | C)[]`.
#[test]
fn tuple_three_adjacent_rest_arrays_collapse_to_array() {
    let db = TypeInterner::new();
    let result = db.tuple_normalized(vec![
        rest_elem(db.array(TypeId::NUMBER)),
        rest_elem(db.array(TypeId::STRING)),
        rest_elem(db.array(TypeId::BOOLEAN)),
    ]);

    let expected_elem = db.union(vec![TypeId::NUMBER, TypeId::STRING, TypeId::BOOLEAN]);
    let expected = db.array(expected_elem);
    assert_eq!(
        result, expected,
        "[...number[], ...string[], ...boolean[]] should normalize to (number | string | boolean)[]"
    );
}

/// CONTROL: a single trailing rest array must stay as-is.
/// `[number, ...string[]]` must remain `[number, ...string[]]`.
#[test]
fn tuple_single_trailing_rest_array_unchanged() {
    let db = TypeInterner::new();
    let str_arr = db.array(TypeId::STRING);

    let result = db.tuple(vec![fixed_elem(TypeId::NUMBER), rest_elem(str_arr)]);

    let Some(TypeData::Tuple(list_id)) = db.lookup(result) else {
        panic!("expected a Tuple, got {:?}", db.lookup(result));
    };
    let elems = db.tuple_list(list_id);
    assert_eq!(
        elems.len(),
        2,
        "[number, ...string[]] must remain a 2-element tuple"
    );
    assert!(!elems[0].rest, "first element must be fixed");
    assert!(elems[1].rest, "second element must be rest");
}

/// Fixed prefix + two adjacent rest arrays → `[X, ...(A | B)[]]`.
#[test]
fn tuple_fixed_prefix_plus_two_rest_arrays_merges_rests() {
    let db = TypeInterner::new();
    let result = db.tuple_normalized(vec![
        fixed_elem(TypeId::BOOLEAN),
        rest_elem(db.array(TypeId::NUMBER)),
        rest_elem(db.array(TypeId::STRING)),
    ]);

    let Some(TypeData::Tuple(list_id)) = db.lookup(result) else {
        panic!("expected a Tuple, got {:?}", db.lookup(result));
    };
    let elems = db.tuple_list(list_id);
    assert_eq!(
        elems.len(),
        2,
        "[boolean, ...(number|string)[]] should be 2 elements"
    );
    assert!(!elems[0].rest, "first element must be fixed (boolean)");
    assert_eq!(elems[0].type_id, TypeId::BOOLEAN);
    assert!(elems[1].rest, "second element must be a rest");
    let expected_arr = db.array(db.union(vec![TypeId::NUMBER, TypeId::STRING]));
    assert_eq!(
        elems[1].type_id, expected_arr,
        "rest element should be (number | string)[]"
    );
}

/// CONTROL: `tuple()` (raw path) must NOT merge adjacent concrete rest arrays.
/// Only `tuple_normalized()` (instantiation path) may collapse them.
#[test]
fn tuple_raw_keeps_adjacent_rest_arrays() {
    let db = TypeInterner::new();
    let num_arr = db.array(TypeId::NUMBER);
    let str_arr = db.array(TypeId::STRING);

    let result = db.tuple(vec![rest_elem(num_arr), rest_elem(str_arr)]);

    // Raw tuple() must keep two separate rest elements
    let Some(TypeData::Tuple(list_id)) = db.lookup(result) else {
        panic!(
            "expected Tuple from raw tuple(), got {:?}",
            db.lookup(result)
        );
    };
    let elems = db.tuple_list(list_id);
    assert_eq!(
        elems.len(),
        2,
        "raw tuple() must not merge adjacent rest arrays"
    );
}

/// CONTROL: type-parameter rest elements must NOT be merged even via `tuple_normalized`.
#[test]
fn tuple_type_param_rest_elements_not_merged() {
    let db = TypeInterner::new();
    let a_param = db.type_param(TypeParamInfo {
        name: db.intern_string("A"),
        constraint: None,
        default: None,
        is_const: false,
    });
    let b_param = db.type_param(TypeParamInfo {
        name: db.intern_string("B"),
        constraint: None,
        default: None,
        is_const: false,
    });

    let result = db.tuple(vec![rest_elem(a_param), rest_elem(b_param)]);

    let Some(TypeData::Tuple(list_id)) = db.lookup(result) else {
        panic!("expected a Tuple, got {:?}", db.lookup(result));
    };
    let elems = db.tuple_list(list_id);
    assert_eq!(
        elems.len(),
        2,
        "[...A, ...B] with type params must remain a 2-element tuple"
    );
}

/// Adjacent rest elements with bare (non-array-wrapped) element types are merged
/// via `tuple_normalized` (the evaluate.rs / instantiation path).
#[test]
fn tuple_adjacent_rest_elements_with_bare_element_types_merged() {
    let db = TypeInterner::new();
    let result = db.tuple_normalized(vec![rest_elem(TypeId::NUMBER), rest_elem(TypeId::STRING)]);

    let expected_elem = db.union(vec![TypeId::NUMBER, TypeId::STRING]);
    let expected = db.array(expected_elem);
    assert_eq!(
        result, expected,
        "rest elements with bare element types should also be merged"
    );
}
