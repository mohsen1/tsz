//! Indexed-access key-space construction boundary.
//!
//! Indexed-access validation owns source selection, diagnostics, and relation
//! policy. This module owns the solver surfaces those checks compare, such as
//! `keyof T`, `T[K]`, literal-key unions, and the `string | number` key space.

use tsz_common::Atom;
use tsz_solver::TypeId;
use tsz_solver::construction::TypeDatabase;

pub(crate) fn keyof_type(db: &dyn TypeDatabase, operand: TypeId) -> TypeId {
    db.keyof(operand)
}

pub(crate) fn indexed_access_type(
    db: &dyn TypeDatabase,
    object_type: TypeId,
    index_type: TypeId,
) -> TypeId {
    db.index_access(object_type, index_type)
}

pub(crate) fn literal_number_key(db: &dyn TypeDatabase, value: f64) -> TypeId {
    db.literal_number(value)
}

pub(crate) fn literal_string_key(db: &dyn TypeDatabase, atom: Atom) -> TypeId {
    db.literal_string_atom(atom)
}

pub(crate) fn literal_key_union(db: &dyn TypeDatabase, members: Vec<TypeId>) -> Option<TypeId> {
    (!members.is_empty()).then(|| db.union(members))
}

pub(crate) fn key_space_union(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.union(members)
}

pub(crate) fn string_or_number_key_space(db: &dyn TypeDatabase) -> TypeId {
    db.union2(TypeId::STRING, TypeId::NUMBER)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_solver::construction::TypeInterner;

    #[test]
    fn constructs_indexed_access_key_space_surfaces() {
        let db = TypeInterner::new();
        let atom = db.intern_string("field");
        let string_key = literal_string_key(&db, atom);
        let number_key = literal_number_key(&db, 1.0);

        assert_eq!(string_key, db.literal_string_atom(atom));
        assert_eq!(number_key, db.literal_number(1.0));
        assert_eq!(keyof_type(&db, TypeId::STRING), db.keyof(TypeId::STRING));
        assert_eq!(
            indexed_access_type(&db, TypeId::STRING, number_key),
            db.index_access(TypeId::STRING, number_key)
        );
        assert_eq!(literal_key_union(&db, vec![]), None);
        assert_eq!(
            literal_key_union(&db, vec![string_key, number_key]),
            Some(db.union(vec![string_key, number_key]))
        );
        assert_eq!(
            key_space_union(&db, vec![TypeId::STRING, TypeId::NUMBER]),
            db.union(vec![TypeId::STRING, TypeId::NUMBER])
        );
        assert_eq!(
            string_or_number_key_space(&db),
            db.union2(TypeId::STRING, TypeId::NUMBER)
        );
    }
}
