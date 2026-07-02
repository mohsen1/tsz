//! The error sentinel (`TypeId::ERROR`, tsc's `errorType`) must render as
//! `any` in every structural display position, never the compiler-internal
//! "error" token.
//!
//! `tsc` models a failed type resolution as `errorType` — an `Any`-flagged
//! intrinsic whose internal name is "error" — but its printer always renders
//! it as `any`. So a function whose body failed to type-check surfaces in a
//! diagnostic as `(e: {...}) => any`, an errored array element as `any[]`, an
//! errored property value as `{ r: any; }`, and so on. tsz previously leaked
//! the raw "error" spelling into these positions.

use super::*;
use crate::construction::TypeInterner;
use crate::types::{FunctionShape, ParamInfo, PropertyInfo, TupleElement};

#[test]
fn error_sentinel_renders_as_any() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);
    assert_eq!(fmt.format(TypeId::ERROR), "any");
}

#[test]
fn none_placeholder_renders_as_any() {
    // `TypeId::NONE` shares `TypeData::Error`; if it ever reaches the printer
    // it must still read as `any`, exercising the key-dispatch branch that is
    // kept in lock-step with the `TypeId::ERROR` fast path.
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);
    assert_eq!(fmt.format(TypeId::NONE), "any");
}

#[test]
fn error_element_array_renders_as_any_array() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);
    let arr = db.array(TypeId::ERROR);
    assert_eq!(fmt.format(arr), "any[]");
}

#[test]
fn error_return_type_renders_as_arrow_to_any() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);
    let f = db.function(FunctionShape::new(
        vec![ParamInfo {
            name: Some(db.intern_string("e")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        TypeId::ERROR,
    ));
    assert_eq!(fmt.format(f), "(e: number) => any");
}

#[test]
fn error_property_value_renders_as_any() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);
    let obj = db.object(vec![PropertyInfo::new(
        db.intern_string("r"),
        TypeId::ERROR,
    )]);
    assert_eq!(fmt.format(obj), "{ r: any; }");
}

#[test]
fn error_tuple_element_renders_as_any() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);
    let tup = db.tuple(vec![
        TupleElement::fixed(TypeId::NUMBER),
        TupleElement::fixed(TypeId::ERROR),
    ]);
    assert_eq!(fmt.format(tup), "[number, any]");
}
