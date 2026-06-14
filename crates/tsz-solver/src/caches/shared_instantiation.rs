use crate::caches::shared_query_cache::ApplicationEvalCacheKey;
use crate::def::DefId;
use crate::types::TypeId;
use rustc_hash::FxHashSet;

pub(super) fn shared_instantiation_family_requested() -> bool {
    std::env::var_os("TSZ_SHARE_INSTANTIATION_CACHES").is_some_and(|value| value != "0")
}

pub(super) fn collect_application_eval_entry_def_dependencies(
    interner: &dyn crate::construction::TypeDatabase,
    key: &ApplicationEvalCacheKey,
    result: TypeId,
) -> Vec<DefId> {
    let (key_def, key_args, _, _) = key;
    let mut seen = FxHashSet::default();
    let mut deps = Vec::new();

    if seen.insert(*key_def) {
        deps.push(*key_def);
    }

    for &arg in key_args {
        for def_id in crate::visitors::visitor::collect_lazy_def_ids(interner, arg) {
            if seen.insert(def_id) {
                deps.push(def_id);
            }
        }
    }

    for def_id in crate::visitors::visitor::collect_lazy_def_ids(interner, result) {
        if seen.insert(def_id) {
            deps.push(def_id);
        }
    }

    deps
}
