use crate::query_boundaries::common::instantiate_generic;
use tsz_solver::construction::{QueryCache, TypeInterner};
use tsz_solver::{PropertyInfo, TypeId, TypeParamInfo, Visibility};

fn param_info(name: tsz_common::interner::Atom) -> TypeParamInfo {
    TypeParamInfo {
        name,
        constraint: None,
        default: None,
        is_const: false,
        origin: tsz_solver::TypeParamOrigin::User,
    }
}

fn object_with(interner: &TypeInterner, type_id: TypeId) -> TypeId {
    let name = interner.intern_string("value");
    interner.object(vec![PropertyInfo {
        name,
        type_id,
        write_type: type_id,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: true,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }])
}

#[test]
fn instantiate_generic_boundary_uses_query_cache() {
    // Per-file tier in isolation (#14345): disable the project-wide instantiation
    // cache so the repeat hit lands on the per-file QueryCache statistics.
    let _g = tsz_solver::computation::ProjectInstCacheDisabledGuard::new();
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let param_name = interner.intern_string("T");
    let param_type = interner.type_param(param_info(param_name));
    let body = object_with(&interner, param_type);
    let param = param_info(param_name);

    let before = db.statistics();
    let first = instantiate_generic(&db, body, &[param], &[TypeId::STRING]);
    let second = instantiate_generic(&db, body, &[param], &[TypeId::STRING]);
    let after = db.statistics();

    assert_eq!(first, second);
    assert!(after.instantiation_cache_misses > before.instantiation_cache_misses);
    assert!(after.instantiation_cache_hits > before.instantiation_cache_hits);
}
