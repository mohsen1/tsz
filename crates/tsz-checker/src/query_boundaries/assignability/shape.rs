//! Indexed-access surface normalization shape probes.
//!
//! These helpers own the low-level type-shape questions asked while normalizing
//! indexed-access surfaces *before* an assignability relation runs (the TS2322 /
//! TS2345 pipeline). Routing them through this boundary — rather than the
//! catch-all `query_boundaries::common` module — keeps the relation-adjacent
//! normalization steps visibly owned by the assignability boundary, so
//! reviewers can tell which shape probes are part of the relation pipeline from
//! generic type queries. They delegate to the existing solver queries
//! internally; the point is ownership and ratcheting, not new semantics.

use tsz_solver::TypeId;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::relations::subtype::TypeResolver;
use tsz_solver::type_queries::TypeIdList;

fn free_decl_origins_and_names(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> rustc_hash::FxHashSet<(tsz_solver::TypeParamOrigin, tsz_common::Atom)> {
    tsz_solver::visitor::free_decl_scoped_type_parameter_origins_in(db, [type_id])
}

/// Whether `source` and `target` carry different authoritative declarations in
/// their free type-parameter sets.
///
/// A declaration-scoped parameter nested in an object/alias body remains free
/// at an ordinary value assignment. It is not alpha-renamable there: only an
/// enclosing generic signature introduces an alpha-equivalence scope. Legacy
/// `User` origins carry no authoritative declaration key and intentionally do
/// not make this predicate true.
pub(crate) fn have_distinct_decl_scoped_free_type_parameters(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
) -> bool {
    if source == target {
        return false;
    }
    let source_entries = free_decl_origins_and_names(db, source);
    let target_entries = free_decl_origins_and_names(db, target);
    let source_origins: rustc_hash::FxHashSet<_> =
        source_entries.iter().map(|(origin, _)| *origin).collect();
    let target_origins: rustc_hash::FxHashSet<_> =
        target_entries.iter().map(|(origin, _)| *origin).collect();

    !source_origins.is_empty() && !target_origins.is_empty() && source_origins != target_origins
}

/// Whether a pair with different authoritative free-binder identities has the
/// same structural surface after those identically named free binders are put
/// into a shared comparison scope.
///
/// This is the semantic TS2719 selector for reduced alias/application bodies;
/// callers do not infer unrelated duplicate declarations from rendered text.
pub(crate) fn have_same_surface_distinct_decl_scoped_free_type_parameters<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    source: TypeId,
    target: TypeId,
) -> bool {
    let source_entries = free_decl_origins_and_names(db, source);
    let target_entries = free_decl_origins_and_names(db, target);
    let source_origins: rustc_hash::FxHashSet<_> =
        source_entries.iter().map(|(origin, _)| *origin).collect();
    let target_origins: rustc_hash::FxHashSet<_> =
        target_entries.iter().map(|(origin, _)| *origin).collect();
    if source_origins.is_empty() || target_origins.is_empty() || source_origins == target_origins {
        return false;
    }

    let source_names: rustc_hash::FxHashSet<_> =
        source_entries.iter().map(|(_, name)| *name).collect();
    let target_names: rustc_hash::FxHashSet<_> =
        target_entries.iter().map(|(_, name)| *name).collect();
    if source_names != target_names
        || source_names.len() != source_entries.len()
        || target_names.len() != target_entries.len()
    {
        return false;
    }
    let mut param_names: Vec<_> = source_names.into_iter().collect();
    param_names.sort_unstable();
    tsz_solver::computation::are_types_structurally_identical_in_param_scope(
        db,
        resolver,
        source,
        target,
        &param_names,
    )
}

/// Detect an indexed-access type (`T[K]`) during assignability normalization.
///
/// Used to decide whether a surface should be driven through
/// `evaluate_type_for_assignability` before the relation runs. Identical in
/// behavior to the shared solver predicate; the dedicated name marks it as the
/// assignability-pipeline entry point.
pub(crate) fn is_index_access_for_assignability(db: &dyn TypeDatabase, ty: TypeId) -> bool {
    tsz_solver::type_queries::is_index_access_type(db, ty)
}

/// Peel union members during assignability normalization.
///
/// Returns the interned member list (a zero-copy [`TypeIdList`] view) when `ty`
/// is a union so each member can be normalized independently, or `None`
/// otherwise. Mirrors `query_boundaries::common::union_members` but is owned by
/// the assignability boundary for the relation-preparation path.
pub(crate) fn union_members_for_assignability(
    db: &dyn TypeDatabase,
    ty: TypeId,
) -> Option<TypeIdList> {
    tsz_solver::type_queries::get_union_members(db, ty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_solver::construction::TypeInterner;
    use tsz_solver::{PropertyInfo, TypeParamInfo, TypeParamOrigin};

    fn declared_param(db: &TypeInterner, file: tsz_common::Atom, name: &str, node: u32) -> TypeId {
        db.type_param(TypeParamInfo {
            origin: TypeParamOrigin::DeclScoped { file, node },
            ..TypeParamInfo::simple(db.intern_string(name))
        })
    }

    fn boxed(db: &TypeInterner, value: TypeId) -> TypeId {
        db.object(vec![PropertyInfo::new(db.intern_string("value"), value)])
    }

    #[test]
    fn declaration_origin_queries_distinguish_identity_from_surface() {
        let db = TypeInterner::new();
        let file = db.intern_string("identity.js");
        let first = boxed(&db, declared_param(&db, file, "Value", 10));
        let second = boxed(&db, declared_param(&db, file, "Value", 20));
        let renamed = boxed(&db, declared_param(&db, file, "Other", 30));

        assert!(have_distinct_decl_scoped_free_type_parameters(
            &db, first, second
        ));
        assert!(have_same_surface_distinct_decl_scoped_free_type_parameters(
            &db, &db, first, second
        ));
        assert!(have_distinct_decl_scoped_free_type_parameters(
            &db, first, renamed
        ));
        assert!(
            !have_same_surface_distinct_decl_scoped_free_type_parameters(&db, &db, first, renamed)
        );
        assert!(!have_distinct_decl_scoped_free_type_parameters(
            &db, first, first
        ));
    }

    #[test]
    fn declaration_origin_queries_ignore_legacy_and_reminted_identity() {
        let db = TypeInterner::new();
        let file = db.intern_string("identity.js");
        let name = db.intern_string("Value");
        let origin = TypeParamOrigin::DeclScoped { file, node: 40 };
        let first_param = db.type_param(TypeParamInfo {
            origin,
            ..TypeParamInfo::simple(name)
        });
        let reminted_param = db.type_param(TypeParamInfo {
            constraint: Some(TypeId::STRING),
            origin,
            ..TypeParamInfo::simple(name)
        });
        assert_ne!(first_param, reminted_param);
        assert!(!have_distinct_decl_scoped_free_type_parameters(
            &db,
            boxed(&db, first_param),
            boxed(&db, reminted_param),
        ));

        let legacy_left = boxed(
            &db,
            db.type_param(TypeParamInfo {
                constraint: Some(TypeId::STRING),
                ..TypeParamInfo::simple(name)
            }),
        );
        let legacy_right = boxed(
            &db,
            db.type_param(TypeParamInfo {
                constraint: Some(TypeId::NUMBER),
                ..TypeParamInfo::simple(name)
            }),
        );
        assert!(!have_distinct_decl_scoped_free_type_parameters(
            &db,
            legacy_left,
            legacy_right,
        ));
    }

    #[test]
    fn declaration_origin_surface_query_traverses_application_wrappers() {
        let db = TypeInterner::new();
        let file = db.intern_string("application.js");
        let base = db.lazy(tsz_solver::DefId(1));
        let first = db.application(
            base,
            vec![boxed(&db, declared_param(&db, file, "Element", 50))],
        );
        let second = db.application(
            base,
            vec![boxed(&db, declared_param(&db, file, "Element", 60))],
        );

        assert!(have_same_surface_distinct_decl_scoped_free_type_parameters(
            &db, &db, first, second
        ));
    }

    #[test]
    fn declaration_origin_surface_query_rejects_ambiguous_same_name_sets() {
        let db = TypeInterner::new();
        let file = db.intern_string("ambiguous.js");
        let name = "Repeated";
        let pair = |direct_node, nested_node| {
            db.object(vec![
                PropertyInfo::new(
                    db.intern_string("direct"),
                    declared_param(&db, file, name, direct_node),
                ),
                PropertyInfo::new(
                    db.intern_string("nested"),
                    boxed(&db, declared_param(&db, file, name, nested_node)),
                ),
            ])
        };
        let source = pair(70, 80);
        let target = pair(90, 100);

        assert!(have_distinct_decl_scoped_free_type_parameters(
            &db, source, target
        ));
        assert!(
            !have_same_surface_distinct_decl_scoped_free_type_parameters(&db, &db, source, target)
        );
    }
}
