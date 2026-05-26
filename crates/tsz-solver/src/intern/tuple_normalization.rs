use crate::caches::db::TypeDatabase;
use crate::types::{TupleElement, TypeData, TypeId};

/// Intern a tuple after merging adjacent concrete rest elements.
///
/// This is intentionally used by instantiation paths only. Raw tuple
/// construction for explicit annotations must keep adjacent rest elements as
/// written so downstream diagnostics match `tsc`.
pub(crate) fn tuple_normalized(db: &dyn TypeDatabase, elements: Vec<TupleElement>) -> TypeId {
    db.tuple(merge_adjacent_rest_arrays(db, elements))
}

/// Merge consecutive concrete rest elements into a single rest element whose
/// type is a union array. `[...X[], ...Y[]]` becomes `[...(X | Y)[]]`.
pub(crate) fn merge_adjacent_rest_arrays(
    db: &dyn TypeDatabase,
    elements: Vec<TupleElement>,
) -> Vec<TupleElement> {
    if elements.len() < 2 {
        return elements;
    }

    let mut result: Vec<TupleElement> = Vec::with_capacity(elements.len());
    let mut changed = false;
    let mut i = 0;
    while i < elements.len() {
        let elem = elements[i];
        if elem.rest
            && let Some(first_elem_type) = concrete_rest_elem_type(db, elem.type_id)
        {
            let (run_types, consumed) =
                collect_concrete_rest_run(db, &elements[i + 1..], first_elem_type);
            i += 1 + consumed;
            if run_types.len() == 1 {
                result.push(elem);
            } else {
                changed = true;
                result.push(TupleElement {
                    type_id: db.array(db.union(run_types)),
                    name: elem.name,
                    optional: elem.optional,
                    rest: true,
                });
            }
            continue;
        }
        result.push(elem);
        i += 1;
    }

    if changed { result } else { elements }
}

fn collect_concrete_rest_run(
    db: &dyn TypeDatabase,
    tail: &[TupleElement],
    first_elem_type: TypeId,
) -> (Vec<TypeId>, usize) {
    let mut types = vec![first_elem_type];
    let mut consumed = 0;
    for next in tail {
        if next.rest
            && let Some(t) = concrete_rest_elem_type(db, next.type_id)
        {
            types.push(t);
            consumed += 1;
        } else {
            break;
        }
    }
    (types, consumed)
}

fn concrete_rest_elem_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    if let Some(TypeData::Array(elem)) = db.lookup(type_id) {
        return Some(elem);
    }
    if let Some(TypeData::ReadonlyType(inner)) = db.lookup(type_id)
        && let Some(TypeData::Array(elem)) = db.lookup(inner)
    {
        return Some(elem);
    }
    if type_id.is_intrinsic() {
        return Some(type_id);
    }

    match db.lookup(type_id) {
        Some(
            TypeData::TypeParameter(_)
            | TypeData::BoundParameter(_)
            | TypeData::Lazy(_)
            | TypeData::Application(_)
            | TypeData::Conditional(_)
            | TypeData::Mapped(_)
            | TypeData::IndexAccess(_, _)
            | TypeData::KeyOf(_)
            | TypeData::TypeQuery(_)
            | TypeData::TemplateLiteral(_)
            | TypeData::StringIntrinsic { .. }
            | TypeData::Infer(_)
            | TypeData::Recursive(_)
            | TypeData::NoInfer(_),
        )
        | None => None,
        Some(_) => Some(type_id),
    }
}
