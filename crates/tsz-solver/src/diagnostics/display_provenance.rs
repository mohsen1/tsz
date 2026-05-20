//! Diagnostic display provenance facade.
//!
//! This module is the named policy boundary for diagnostic-only provenance
//! records. Storage still lives behind [`TypeDatabase`] for now, but solver
//! algorithms should record provenance through these typed functions instead
//! of calling interner side-table methods directly.

use crate::caches::db::TypeDatabase;
use crate::types::{PropertyInfo, TypeId};

#[derive(Debug, Clone, Copy)]
pub(crate) struct AliasApplicationProvenance {
    pub(crate) evaluated: TypeId,
    pub(crate) application: TypeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AliasApplicationPriority {
    PreserveExisting,
    PreferApplication,
}

#[derive(Debug)]
pub(crate) struct FreshObjectLiteralDisplayProvenance {
    pub(crate) type_id: TypeId,
    pub(crate) properties: Vec<PropertyInfo>,
}

#[derive(Debug)]
pub(crate) struct UnionOriginProvenance {
    pub(crate) union_type_id: TypeId,
    pub(crate) origin_members: Vec<TypeId>,
}

pub(crate) fn record_alias_application(
    db: &dyn TypeDatabase,
    provenance: AliasApplicationProvenance,
    priority: AliasApplicationPriority,
) {
    match priority {
        AliasApplicationPriority::PreserveExisting => {
            db.store_display_alias(provenance.evaluated, provenance.application);
        }
        AliasApplicationPriority::PreferApplication => {
            db.store_display_alias_preferring_application(
                provenance.evaluated,
                provenance.application,
            );
        }
    }
}

pub(crate) fn display_alias(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    db.get_display_alias(type_id)
}

pub(crate) fn record_fresh_object_literal_display(
    db: &dyn TypeDatabase,
    provenance: FreshObjectLiteralDisplayProvenance,
) {
    db.store_display_properties(provenance.type_id, provenance.properties);
}

pub(crate) fn record_union_origin(db: &dyn TypeDatabase, provenance: UnionOriginProvenance) {
    db.store_union_origin(provenance.union_type_id, provenance.origin_members);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::TypeInterner;
    use crate::types::TypeData;

    #[test]
    fn alias_application_records_display_alias() {
        let interner = TypeInterner::new();
        let evaluated = interner.object(vec![]);
        let application = interner.application(TypeId::STRING, vec![TypeId::NUMBER]);

        record_alias_application(
            &interner,
            AliasApplicationProvenance {
                evaluated,
                application,
            },
            AliasApplicationPriority::PreserveExisting,
        );

        assert_eq!(display_alias(&interner, evaluated), Some(application));
    }

    #[test]
    fn fresh_object_display_records_properties() {
        let interner = TypeInterner::new();
        let property_name = interner.intern_string("value");
        let ty = interner.object(vec![]);

        record_fresh_object_literal_display(
            &interner,
            FreshObjectLiteralDisplayProvenance {
                type_id: ty,
                properties: vec![PropertyInfo::new(property_name, TypeId::STRING)],
            },
        );

        let props = interner
            .get_display_properties(ty)
            .expect("display properties");
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].name, property_name);
        assert_eq!(props[0].type_id, TypeId::STRING);
    }

    #[test]
    fn flattened_union_origin_records_source_members() {
        let interner = TypeInterner::new();
        let inner = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
        let union = interner.union(vec![inner, TypeId::BOOLEAN]);

        record_union_origin(
            &interner,
            UnionOriginProvenance {
                union_type_id: union,
                origin_members: vec![inner, TypeId::BOOLEAN],
            },
        );

        assert!(matches!(interner.lookup(union), Some(TypeData::Union(_))));
        assert_eq!(
            interner
                .get_union_origin(union)
                .as_deref()
                .map(Vec::as_slice),
            Some([inner, TypeId::BOOLEAN].as_slice())
        );
    }
}
