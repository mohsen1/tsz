//! Non-default relation-kind query helpers split out of the assignability
//! boundary parent to satisfy the source-file line cap: bivariant-callback
//! assignability, subtype, and redeclaration-identity relation queries that
//! share the boundary's policy/context construction but do not participate
//! in the failure-analysis path.

use tsz_solver::TypeId;
use tsz_solver::classes::inheritance::InheritanceGraph;
use tsz_solver::construction::QueryDatabase;

use super::{assignability_cache_key, is_relation_cacheable};
use crate::query_boundaries::relation_policy;

pub(crate) fn is_assignable_bivariant_with_resolver<
    R: tsz_solver::relations::subtype::TypeResolver,
>(
    db: &dyn QueryDatabase,
    resolver: &R,
    source: TypeId,
    target: TypeId,
    flags: u16,
    inheritance_graph: &InheritanceGraph,
    sound_mode: bool,
) -> tsz_solver::relations::relation_queries::RelationResult {
    let policy = relation_policy::from_checker_flags_u16(flags)
        .with_strict_subtype_checking(sound_mode)
        .with_strict_any_propagation(sound_mode);
    let context = tsz_solver::relations::relation_queries::RelationContext {
        query_db: Some(db),
        inheritance_graph: Some(inheritance_graph),
        class_check: None,
    };
    tsz_solver::relations::relation_queries::query_relation_with_resolver(
        db,
        resolver,
        source,
        target,
        tsz_solver::relations::relation_queries::RelationKind::AssignableBivariantCallbacks,
        policy,
        context,
    )
}

pub(crate) fn cached_bivariant_assignability_with_resolver<
    R: tsz_solver::relations::subtype::TypeResolver,
>(
    db: &dyn QueryDatabase,
    resolver: &R,
    source: TypeId,
    target: TypeId,
    flags: u16,
    inheritance_graph: &InheritanceGraph,
    sound_mode: bool,
) -> tsz_solver::relations::relation_queries::RelationResult {
    let is_cacheable = is_relation_cacheable(db.as_type_database(), source, target);
    if is_cacheable {
        let cache_key = assignability_cache_key(source, target, flags);
        if let Some(cached) = db.lookup_assignability_cache(cache_key) {
            return tsz_solver::relations::relation_queries::RelationResult::complete(
                tsz_solver::relations::relation_queries::RelationKind::AssignableBivariantCallbacks,
                cached,
            );
        }
    }

    let relation_result = is_assignable_bivariant_with_resolver(
        db,
        resolver,
        source,
        target,
        flags,
        inheritance_graph,
        sound_mode,
    );

    if is_cacheable {
        let cache_key = assignability_cache_key(source, target, flags);
        db.insert_assignability_cache(cache_key, relation_result.is_related());
    }

    relation_result
}

pub(crate) fn is_subtype_with_resolver<R: tsz_solver::relations::subtype::TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    source: TypeId,
    target: TypeId,
    flags: u16,
    inheritance_graph: &InheritanceGraph,
    class_check: Option<&dyn Fn(tsz_solver::SymbolRef) -> bool>,
) -> tsz_solver::relations::relation_queries::RelationResult {
    let policy = relation_policy::from_checker_flags_u16(flags);
    let context = tsz_solver::relations::relation_queries::RelationContext {
        query_db: Some(db),
        inheritance_graph: Some(inheritance_graph),
        class_check,
    };
    tsz_solver::relations::relation_queries::query_relation_with_resolver(
        db,
        resolver,
        source,
        target,
        tsz_solver::relations::relation_queries::RelationKind::Subtype,
        policy,
        context,
    )
}

pub(crate) fn is_redeclaration_identical_with_resolver<
    R: tsz_solver::relations::subtype::TypeResolver,
>(
    db: &dyn QueryDatabase,
    resolver: &R,
    source: TypeId,
    target: TypeId,
    flags: u16,
    inheritance_graph: &InheritanceGraph,
    sound_mode: bool,
) -> bool {
    let policy = relation_policy::from_checker_flags_u16(flags)
        .with_strict_subtype_checking(sound_mode)
        .with_strict_any_propagation(sound_mode);
    let context = tsz_solver::relations::relation_queries::RelationContext {
        query_db: Some(db),
        inheritance_graph: Some(inheritance_graph),
        class_check: None,
    };
    tsz_solver::relations::relation_queries::query_relation_with_resolver(
        db,
        resolver,
        source,
        target,
        tsz_solver::relations::relation_queries::RelationKind::RedeclarationIdentical,
        policy,
        context,
    )
    .is_related()
}
