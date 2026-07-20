//! Recognizers for bare foreign/outer type-parameter shapes used by generic
//! call inference.
//!
//! These helpers classify whether an inferred type is a naked outer type
//! parameter (or a homogeneous array/tuple/union built only from such naked
//! parameters). Direct-parameter inference for naked-type-parameter targets
//! uses them to decide when tsc keeps a distributed union rather than collapsing
//! to the first candidate.

use crate::inference::infer::InferenceVar;
use crate::types::{TypeData, TypeId, TypeParamInfo};
use rustc_hash::FxHashMap;

pub(super) fn is_bare_foreign_type_param(
    interner: &dyn crate::construction::TypeDatabase,
    ty: TypeId,
    local_type_params: &[TypeParamInfo],
    local_placeholders: &FxHashMap<TypeId, InferenceVar>,
) -> bool {
    if ty.is_intrinsic() {
        return false;
    }
    match interner.lookup(ty) {
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
            !is_local_placeholder(interner, ty, &info, local_placeholders)
                && !local_type_params
                    .iter()
                    .any(|type_param| type_param.is_same_binder(info))
        }
        _ => false,
    }
}

fn is_local_placeholder(
    interner: &dyn crate::construction::TypeDatabase,
    ty: TypeId,
    info: &TypeParamInfo,
    local_placeholders: &FxHashMap<TypeId, InferenceVar>,
) -> bool {
    if local_placeholders.contains_key(&ty) {
        return true;
    }
    if !info.is_current_infer_placeholder() {
        return false;
    }
    local_placeholders.keys().any(|local_type| {
        matches!(
            interner.lookup(*local_type),
            Some(TypeData::TypeParameter(local_info) | TypeData::Infer(local_info))
                if local_info.origin == info.origin
        )
    })
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
    local_type_params: &[TypeParamInfo],
    local_placeholders: &FxHashMap<TypeId, InferenceVar>,
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
    local_type_params: &[TypeParamInfo],
    local_placeholders: &FxHashMap<TypeId, InferenceVar>,
) -> bool {
    !ty.is_any_unknown_or_error()
        && !is_bare_foreign_type_param(interner, ty, local_type_params, local_placeholders)
        && !crate::visitor::contains_type_parameters(interner, ty)
        && !crate::type_queries::contains_infer_types_db(interner, ty)
}

#[cfg(test)]
mod tests {
    use super::is_bare_foreign_type_param;
    use crate::construction::TypeInterner;
    use crate::inference::infer::InferenceVar;
    use crate::types::{TypeParamInfo, TypeParamOrigin};
    use rustc_hash::FxHashMap;

    #[test]
    fn reconstructed_local_placeholder_is_not_foreign() {
        let interner = TypeInterner::new();
        let name = interner.intern_string("__local_placeholder");
        let info = TypeParamInfo {
            name,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::InferPlaceholder { id: 7 },
        };
        let original = interner.fresh_type_param(info);
        let reconstructed = interner.fresh_type_param(info);
        assert_ne!(original, reconstructed);
        let local_placeholders = FxHashMap::from_iter([(original, InferenceVar(0))]);

        assert!(!is_bare_foreign_type_param(
            &interner,
            reconstructed,
            &[],
            &local_placeholders,
        ));

        for origin in [
            TypeParamOrigin::User,
            TypeParamOrigin::DeclScoped {
                file: interner.intern_string("user-source.ts"),
                node: 1,
            },
        ] {
            let user_param = interner.fresh_type_param(TypeParamInfo { origin, ..info });
            assert!(
                is_bare_foreign_type_param(&interner, user_param, &[], &local_placeholders),
                "a same-spelled user binder must remain foreign"
            );
        }

        let unrelated_placeholder = interner.fresh_type_param(TypeParamInfo {
            origin: TypeParamOrigin::InferPlaceholder { id: 8 },
            ..info
        });
        assert!(is_bare_foreign_type_param(
            &interner,
            unrelated_placeholder,
            &[],
            &local_placeholders,
        ));
    }
}
