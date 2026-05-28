//! Homomorphic mapped-chain flattening for subtype relations.

use crate::types::{MappedModifier, MappedTypeId, TypeId};
use crate::visitor::{index_access_parts, keyof_inner_type, mapped_type_id, type_param_info};

/// Result of flattening a chain of nested homomorphic mapped types.
pub(crate) struct FlattenedMapped {
    /// The ultimate source type (e.g., `T` in `Partial<Readonly<T>>`).
    pub source: TypeId,
    /// The outer mapped key constraint (e.g., `keyof T` or `K`).
    pub key_constraint: TypeId,
    /// Whether any mapped type in the chain adds optional (`?`).
    pub has_optional: bool,
    /// Whether any mapped type in the chain adds readonly.
    pub has_readonly: bool,
}

/// Flatten a chain of nested homomorphic mapped types into a single descriptor.
///
/// For `Partial<Readonly<T>>`, this produces:
/// source = `T`, `has_optional = true`, `has_readonly = true`.
///
/// For `Required<Partial<T>>`, this produces:
/// source = `T`, `has_optional = false`, `has_readonly = false`.
///
/// Returns `None` if the mapped type is not in homomorphic form.
pub(crate) fn flatten_mapped_chain(
    interner: &dyn crate::construction::TypeDatabase,
    mapped_id: MappedTypeId,
) -> Option<FlattenedMapped> {
    let mapped = interner.mapped_type(mapped_id);

    // Can't flatten mapped types with name remapping (`as` clause).
    if mapped.name_type.is_some() {
        return None;
    }

    let has_optional = mapped.optional_modifier == Some(MappedModifier::Add);
    let has_readonly = mapped.readonly_modifier == Some(MappedModifier::Add);
    let removes_optional = mapped.optional_modifier == Some(MappedModifier::Remove);
    let removes_readonly = mapped.readonly_modifier == Some(MappedModifier::Remove);

    // Check if template is `X[K]` where `K` is the iteration param.
    let (obj, idx) = index_access_parts(interner, mapped.template)?;
    let param = type_param_info(interner, idx)?;
    if param.name != mapped.type_param.name {
        return None;
    }
    if !mapped_constraint_matches_template_source(interner, mapped.constraint, obj) {
        return None;
    }

    // Try to flatten through the source object if it's itself a mapped type.
    let obj_eval = crate::evaluation::evaluate::evaluate_type(interner, obj);
    if let Some(inner_mapped_id) = mapped_type_id(interner, obj_eval)
        && let Some(inner) = flatten_mapped_chain(interner, inner_mapped_id)
    {
        return Some(FlattenedMapped {
            source: inner.source,
            key_constraint: mapped.constraint,
            has_optional: if removes_optional {
                false
            } else {
                has_optional || inner.has_optional
            },
            has_readonly: if removes_readonly {
                false
            } else {
                has_readonly || inner.has_readonly
            },
        });
    }

    Some(FlattenedMapped {
        source: obj,
        key_constraint: mapped.constraint,
        has_optional,
        has_readonly,
    })
}

fn mapped_constraint_matches_template_source(
    interner: &dyn crate::construction::TypeDatabase,
    constraint: TypeId,
    source: TypeId,
) -> bool {
    if keyof_inner_type(interner, constraint)
        .is_some_and(|operand| homomorphic_source_identity_matches(interner, operand, source))
    {
        return true;
    }

    if let Some(param) = type_param_info(interner, constraint)
        && let Some(param_constraint) = param.constraint
    {
        return keyof_inner_type(interner, param_constraint)
            .is_some_and(|operand| homomorphic_source_identity_matches(interner, operand, source));
    }

    false
}

fn homomorphic_source_identity_matches(
    interner: &dyn crate::construction::TypeDatabase,
    left: TypeId,
    right: TypeId,
) -> bool {
    if left == right {
        return true;
    }

    match (
        type_param_info(interner, left),
        type_param_info(interner, right),
    ) {
        (Some(left_param), Some(right_param)) => left_param.name == right_param.name,
        _ => false,
    }
}
