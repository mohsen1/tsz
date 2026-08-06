use crate::caches::shared_query_cache::ApplicationEvalCacheKey;
use crate::def::{DefId, DefinitionStore};
use crate::types::TypeId;
use rustc_hash::FxHashSet;

pub(super) fn shared_instantiation_family_requested() -> bool {
    std::env::var_os("TSZ_SHARE_INSTANTIATION_CACHES").is_some_and(|value| value != "0")
}

pub(super) fn collect_application_eval_entry_def_dependencies(
    interner: &dyn crate::construction::TypeDatabase,
    definition_store: Option<&DefinitionStore>,
    key: &ApplicationEvalCacheKey,
    result: TypeId,
) -> Vec<DefId> {
    let (key_def, key_args, _, _) = key;
    collect_def_dependencies(
        interner,
        definition_store,
        [*key_def],
        key_args.iter().copied().chain(std::iter::once(result)),
    )
}

pub(super) fn collect_eval_entry_def_dependencies(
    interner: &dyn crate::construction::TypeDatabase,
    definition_store: Option<&DefinitionStore>,
    key: crate::evaluation::request::EvaluationCacheKey,
    result: TypeId,
) -> Vec<DefId> {
    collect_def_dependencies(interner, definition_store, [], [key.type_id(), result])
}

fn collect_def_dependencies(
    interner: &dyn crate::construction::TypeDatabase,
    definition_store: Option<&DefinitionStore>,
    direct_defs: impl IntoIterator<Item = DefId>,
    roots: impl IntoIterator<Item = TypeId>,
) -> Vec<DefId> {
    let mut seen = FxHashSet::default();
    let mut deps = Vec::new();
    let mut pending = Vec::new();

    for def_id in direct_defs {
        push_def_dependency(def_id, &mut seen, &mut deps, &mut pending);
    }

    for root in roots {
        // `push_def_dependency` already de-duplicates through `seen`, so feed
        // the walk straight in rather than materializing a deduped `Vec` from
        // `collect_lazy_def_ids` only to de-duplicate it a second time here.
        crate::visitors::visitor::for_each_lazy_def_id(interner, root, |def_id| {
            push_def_dependency(def_id, &mut seen, &mut deps, &mut pending);
        });
    }

    let Some(store) = definition_store else {
        return deps;
    };

    while let Some(def_id) = pending.pop() {
        let canonical = store.canonical_def_id(def_id);
        if canonical != def_id {
            push_def_dependency(canonical, &mut seen, &mut deps, &mut pending);
        }

        for body_def in [def_id, canonical] {
            let Some(body_deps) = store.body_dependency_defs(body_def) else {
                continue;
            };
            for &dependency in body_deps.iter() {
                push_def_dependency(dependency, &mut seen, &mut deps, &mut pending);
            }
        }
    }

    deps
}

fn push_def_dependency(
    def_id: DefId,
    seen: &mut FxHashSet<DefId>,
    deps: &mut Vec<DefId>,
    pending: &mut Vec<DefId>,
) {
    if seen.insert(def_id) {
        deps.push(def_id);
        pending.push(def_id);
    }
}
