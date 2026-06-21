use crate::caches::db::TypeDatabase;
use crate::types::{TypeData, TypeId};

pub(super) fn constraint_is_nullable_union(db: &dyn TypeDatabase, constraint: TypeId) -> bool {
    let Some(TypeData::Union(members)) = db.lookup(constraint) else {
        return false;
    };
    db.type_list(members)
        .iter()
        .any(|&member| member.is_nullable())
}
