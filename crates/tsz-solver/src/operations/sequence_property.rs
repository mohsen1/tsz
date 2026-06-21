//! Pure tuple/array index-property resolution shared by the property-access
//! evaluator and the conditional `infer`-pattern resolver.
//!
//! These helpers read a numeric-index (or fixed-length) property off a tuple
//! type using only the interner, so they can run in evaluation contexts that do
//! not have a `QueryDatabase` wired up (for example, conditional `infer`-pattern
//! matching during type-alias evaluation). Keeping them as free functions over
//! `&dyn TypeDatabase` lets both call sites share one implementation instead of
//! diverging.

use crate::construction::TypeDatabase;
use crate::types::{TupleElement, TupleListId, TypeData, TypeId};

/// Resolve a type to a tuple element-list id, transparently unwrapping a
/// `readonly` wrapper.
///
/// A spread element `...X` where `X` is a `readonly` tuple (`...readonly [a, b]`)
/// contributes the same statically known run of elements as a mutable tuple
/// spread, so the index walk and fixed-length count must descend through it.
/// Without unwrapping, `tuple_fixed_slot_inner` bailed to `None` on a
/// `ReadonlyType(Tuple)` rest element and the caller fell back to a bogus
/// element-union (`head | readonly [tail]`) for an in-bounds numeric read.
fn tuple_list_id_through_readonly(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TupleListId> {
    let unwrapped = crate::type_queries::data::unwrap_readonly(db, type_id);
    match db.lookup(unwrapped) {
        Some(TypeData::Tuple(id)) => Some(id),
        _ => None,
    }
}

/// Upper bound on rest-spread recursion while walking nested variadic tuples.
const MAX_TUPLE_SPREAD_DEPTH: usize = 64;
/// Upper bound on the fixed length we will count out of a tuple.
const MAX_FIXED_LENGTH: usize = 1000;

/// Widen an element type to include `undefined`, matching the reading of an
/// optional tuple slot.
pub(crate) fn element_type_with_undefined(db: &dyn TypeDatabase, element_type: TypeId) -> TypeId {
    db.union2(element_type, TypeId::UNDEFINED)
}

/// Parse a property name as a tuple/array element index, applying the same
/// numeric-name canonicalization (`"01"`/`"1.0"` → `1`) used for indexed access.
/// Returns `None` for non-numeric keys (e.g. array-prototype method names).
pub(crate) fn parse_numeric_index(prop_name: &str) -> Option<usize> {
    crate::utils::canonicalize_numeric_name(prop_name)?
        .parse::<usize>()
        .ok()
}

/// Resolve the type at a fixed numeric `index` of a tuple's element list,
/// descending through single-rest variadic spreads. Returns `None` when the
/// index is not a guaranteed fixed slot (e.g. it falls inside an unbounded rest
/// element or beyond the tuple's fixed length).
///
/// An optional slot is widened to `T | undefined`, matching how an indexed read
/// (`tuple[index]`) surfaces the slot's value type. Callers that care about the
/// distinction between a guaranteed slot and a merely-optional one (for example
/// structural presence in a conditional `infer` pattern) should use
/// [`tuple_fixed_slot`] instead.
pub(crate) fn tuple_fixed_element_type(
    db: &dyn TypeDatabase,
    elements: &[TupleElement],
    index: usize,
) -> Option<TypeId> {
    let (type_id, optional) = tuple_fixed_slot(db, elements, index)?;
    Some(if optional {
        element_type_with_undefined(db, type_id)
    } else {
        type_id
    })
}

/// Resolve the fixed slot at numeric `index` of a tuple's element list,
/// returning the slot's declared element type together with whether the slot is
/// optional. Descends through single-rest variadic spreads. Returns `None` when
/// the index is not a fixed slot (it falls inside an unbounded rest element or
/// beyond the tuple's fixed length).
///
/// Unlike [`tuple_fixed_element_type`], the element type is returned *without*
/// the optionality `undefined` so a caller can decide how optionality affects
/// its own semantics (an indexed read widens, structural presence rejects).
pub(crate) fn tuple_fixed_slot(
    db: &dyn TypeDatabase,
    elements: &[TupleElement],
    index: usize,
) -> Option<(TypeId, bool)> {
    tuple_fixed_slot_inner(db, elements, index, 0)
}

fn tuple_fixed_slot_inner(
    db: &dyn TypeDatabase,
    elements: &[TupleElement],
    index: usize,
    depth: usize,
) -> Option<(TypeId, bool)> {
    if depth > MAX_TUPLE_SPREAD_DEPTH {
        return None;
    }

    let mut position = 0usize;
    for elem in elements {
        if elem.rest {
            let rest_id = elem.type_id;
            if rest_id.is_intrinsic() {
                return None;
            }
            let inner_list_id = tuple_list_id_through_readonly(db, rest_id)?;
            let inner = db.tuple_list(inner_list_id);
            let rest_index = index.checked_sub(position)?;
            if let Some(slot) = tuple_fixed_slot_inner(db, &inner, rest_index, depth + 1) {
                return Some(slot);
            }
            let inner_len = compute_tuple_fixed_length(db, rest_id)?;
            position = position.checked_add(inner_len)?;
        } else {
            if position == index {
                return Some((elem.type_id, elem.optional));
            }
            position = position.checked_add(1)?;
        }

        if position > index {
            return None;
        }
    }

    None
}

/// Compute the fixed length of a tuple type, if it has one. Returns `None` for
/// arrays, variable-length tuples (with an unbounded rest), or non-tuple types.
pub(crate) fn compute_tuple_fixed_length(db: &dyn TypeDatabase, type_id: TypeId) -> Option<usize> {
    if type_id.is_intrinsic() {
        return None;
    }
    let list_id = tuple_list_id_through_readonly(db, type_id)?;

    let elements = db.tuple_list(list_id);
    let mut total = 0usize;
    let mut rest_type: Option<TypeId> = None;
    let mut rest_count = 0;

    for elem in elements.iter() {
        if elem.rest {
            rest_count += 1;
            if rest_count > 1 {
                return None;
            }
            rest_type = Some(elem.type_id);
        } else {
            total += 1;
            if total > MAX_FIXED_LENGTH {
                return None;
            }
        }
    }

    // Iteratively descend into single-rest chains (e.g., [T, ...Acc])
    while let Some(rest_id) = rest_type.take() {
        if rest_id.is_intrinsic() {
            return None;
        }
        // A rest that spreads a non-tuple → variable length.
        let inner_list_id = tuple_list_id_through_readonly(db, rest_id)?;
        let inner_elements = db.tuple_list(inner_list_id);
        let mut inner_rest_count = 0;
        for elem in inner_elements.iter() {
            if elem.rest {
                inner_rest_count += 1;
                if inner_rest_count > 1 {
                    return None;
                }
                rest_type = Some(elem.type_id);
            } else {
                total += 1;
                if total > MAX_FIXED_LENGTH {
                    return None;
                }
            }
        }
    }

    Some(total)
}
