use crate::construction::TypeDatabase;
use crate::def::resolver::TypeResolver;
use crate::def::{DefId, DefKind};
use crate::{TypeData, TypeId};

pub fn application_base_def_id<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    base: TypeId,
) -> Option<DefId> {
    crate::type_queries::get_lazy_def_id(db, base).or_else(|| match db.lookup(base) {
        Some(TypeData::TypeQuery(sym_ref)) => resolver.symbol_to_def_id(sym_ref),
        Some(TypeData::UnresolvedTypeName(atom)) => {
            let name = db.resolve_atom(atom);
            resolver.resolve_unresolved_type_name(&name)
        }
        _ => None,
    })
}

pub fn application_base_is_raw_conditional_alias<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    base: TypeId,
) -> bool {
    let Some(def_id) = application_base_def_id(db, resolver, base) else {
        return false;
    };
    resolver
        .get_def_raw_body(def_id, db)
        .is_some_and(|body| matches!(db.lookup(body), Some(TypeData::Conditional(_))))
}

pub fn application_base_uses_conditional_infer<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    base: TypeId,
) -> bool {
    let Some(def_id) = application_base_def_id(db, resolver, base) else {
        return false;
    };
    if resolver.get_def_kind(def_id) != Some(DefKind::TypeAlias) {
        return false;
    }
    resolver
        .get_def_raw_body(def_id, db)
        .or_else(|| resolver.resolve_lazy(def_id, db))
        .is_some_and(|body| {
            matches!(
                crate::type_queries::classify_body_for_arg_preservation(db, body),
                crate::type_queries::BodyArgPreservation::ConditionalInfer
                    | crate::type_queries::BodyArgPreservation::ConditionalApplicationInfer
            ) || crate::type_queries::contains_infer_types_db(db, body)
        })
}
