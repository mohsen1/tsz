//! Narrow transparent-alias exposure for shape-sensitive semantic queries.

use crate::construction::TypeDatabase;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::relations::subtype::TypeResolver;
use crate::types::{TypeData, TypeId};

/// Expose exactly one transparent type-alias boundary without general
/// evaluation.
pub(crate) fn expose_transparent_alias_once<R: TypeResolver + ?Sized>(
    db: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
) -> Option<TypeId> {
    match db.lookup(type_id)? {
        TypeData::Lazy(def_id) => resolver.resolve_lazy(def_id, db),
        TypeData::Application(application_id) => {
            let application = db.type_application(application_id);
            let TypeData::Lazy(def_id) = db.lookup(application.base)? else {
                return None;
            };
            let body = resolver.resolve_lazy(def_id, db)?;
            let type_params = resolver.get_lazy_type_params(def_id).unwrap_or_default();
            if type_params.is_empty() {
                return Some(body);
            }
            let substitution = TypeSubstitution::from_args(db, &type_params, &application.args);
            Some(instantiate_type(db, body, &substitution))
        }
        _ => None,
    }
}

/// Expose aliases and readonly wrappers until the outer rest-parameter shape
/// is visible, deliberately stopping at `NoInfer`.
pub(crate) fn expose_rest_alias_shape_preserving_no_infer<R: TypeResolver + ?Sized>(
    db: &dyn TypeDatabase,
    resolver: &R,
    mut type_id: TypeId,
) -> TypeId {
    for _ in 0..32 {
        match db.lookup(type_id) {
            Some(TypeData::ReadonlyType(inner)) => type_id = inner,
            Some(TypeData::Lazy(_) | TypeData::Application(_)) => {
                let Some(exposed) = expose_transparent_alias_once(db, resolver, type_id) else {
                    return type_id;
                };
                if exposed == type_id {
                    return type_id;
                }
                type_id = exposed;
            }
            _ => return type_id,
        }
    }
    type_id
}
