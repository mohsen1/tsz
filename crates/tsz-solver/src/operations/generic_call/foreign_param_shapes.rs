//! Recognizers for bare foreign/outer type-parameter shapes used by generic
//! call inference.
//!
//! These helpers classify whether an inferred type is a naked outer type
//! parameter (or a homogeneous array/tuple/union built only from such naked
//! parameters). Direct-parameter inference for naked-type-parameter targets
//! uses them to decide when tsc keeps a distributed union rather than collapsing
//! to the first candidate.

use crate::types::{TypeData, TypeId};
use rustc_hash::FxHashSet;

pub(super) fn is_bare_foreign_type_param(
    interner: &dyn crate::construction::TypeDatabase,
    ty: TypeId,
    local_type_params: &FxHashSet<tsz_common::Atom>,
    local_placeholders: &[tsz_common::Atom],
) -> bool {
    if ty.is_intrinsic() {
        return false;
    }
    match interner.lookup(ty) {
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
            !local_type_params.contains(&info.name) && !local_placeholders.contains(&info.name)
        }
        _ => false,
    }
}

/// Returns `true` when `ty` is a bare foreign/outer type parameter, or a
/// homogeneous array / readonly-array / tuple built (recursively) only from such
/// bare foreign type parameters (e.g. `T`, `T[]`, `readonly T[]`, `[T, T]`).
///
/// These are the shapes a union argument distributes into when its members are
/// naked outer type parameters (e.g. `T | T[]`). When direct-parameter inference
/// for a naked-type-parameter target accumulates only such shapes, tsc keeps the
/// distributed union as the inferred type rather than collapsing to the first
/// candidate, so the post-inference assignability re-check still matches. This
/// recognizes the array/tuple wrappers `is_bare_foreign_type_param` alone does
/// not, without treating any concrete candidate (e.g. `string`) as bare.
pub(super) fn is_bare_foreign_type_param_shape(
    interner: &dyn crate::construction::TypeDatabase,
    ty: TypeId,
    local_type_params: &FxHashSet<tsz_common::Atom>,
    local_placeholders: &[tsz_common::Atom],
) -> bool {
    if is_bare_foreign_type_param(interner, ty, local_type_params, local_placeholders) {
        return true;
    }
    if ty.is_intrinsic() {
        return false;
    }
    if let Some(elem) = crate::type_queries::get_array_element_type(interner, ty) {
        return is_bare_foreign_type_param_shape(
            interner,
            elem,
            local_type_params,
            local_placeholders,
        );
    }
    if let Some(elements) = crate::type_queries::get_tuple_elements(interner, ty) {
        return !elements.is_empty()
            && elements.iter().all(|element| {
                is_bare_foreign_type_param_shape(
                    interner,
                    element.type_id,
                    local_type_params,
                    local_placeholders,
                )
            });
    }
    // A union whose members are themselves bare-foreign-type-param shapes (e.g.
    // `T | T[]`) is itself such a shape: it is exactly the distributed result a
    // naked union argument produces, accumulated as a single lower bound.
    if let Some(TypeData::Union(members)) = interner.lookup(ty) {
        let members = interner.type_list(members);
        return !members.is_empty()
            && members.iter().all(|&member| {
                is_bare_foreign_type_param_shape(
                    interner,
                    member,
                    local_type_params,
                    local_placeholders,
                )
            });
    }
    false
}

pub(super) fn is_substantive_inference_candidate(
    interner: &dyn crate::construction::TypeDatabase,
    ty: TypeId,
    local_type_params: &FxHashSet<tsz_common::Atom>,
    local_placeholders: &[tsz_common::Atom],
) -> bool {
    !ty.is_any_unknown_or_error()
        && !is_bare_foreign_type_param(interner, ty, local_type_params, local_placeholders)
        && !crate::visitor::contains_type_parameters(interner, ty)
        && !crate::type_queries::contains_infer_types_db(interner, ty)
}
