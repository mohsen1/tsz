use crate::caches::query_cache::ApplicationEvalCacheKey;
use crate::def::DefId;
use crate::types::TypeId;

pub(super) fn shared_instantiation_family_requested() -> bool {
    std::env::var_os("TSZ_SHARE_INSTANTIATION_CACHES").is_some_and(|value| value != "0")
}

pub(super) fn application_eval_entry_references_def(
    interner: &dyn crate::construction::TypeDatabase,
    key: &ApplicationEvalCacheKey,
    result: TypeId,
    def_id: DefId,
) -> bool {
    let (key_def, key_args, _, _) = key;
    *key_def == def_id
        || key_args
            .iter()
            .any(|&arg| crate::visitors::visitor::contains_lazy_def_id(interner, arg, def_id))
        || crate::visitors::visitor::contains_lazy_def_id(interner, result, def_id)
}
