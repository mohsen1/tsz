//! Env-eval cache architecture tests split out to satisfy the source-file line
//! cap.

use super::*;

/// Guard: solver `SymbolRef` to binder `SymbolId` reinterpretation should stay
/// behind `query_boundaries::definition_identity::symbol_ref_to_symbol_id`.
///
/// Existing legacy callsites are a migration budget, not a pattern for new
/// code. Lower this budget whenever a PR moves another callsite behind the
/// bridge.
#[test]
fn test_symbol_ref_to_symbol_id_cast_budget() {
    const RAW_SYMBOL_REF_CAST_BUDGET: usize = 21;
    const BRIDGE_PATH: &str = "src/query_boundaries/definition_identity.rs";

    fn is_raw_symbol_ref_cast(line: &str) -> bool {
        let trimmed = line.trim_start();
        !trimmed.starts_with("//")
            && (line.contains("SymbolId(sym_ref.0)")
                || line.contains("SymbolId(symbol_ref.0)")
                || line.contains("SymbolId(symbol.0)"))
    }

    let mut files = Vec::new();
    collect_checker_rs_files_recursive(Path::new("src"), &mut files);

    let mut hits = Vec::new();
    for path in files {
        if path
            .components()
            .any(|component| component.as_os_str() == "tests")
        {
            continue;
        }
        let rel_path = path.display().to_string();
        if rel_path == BRIDGE_PATH {
            continue;
        }
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_idx, line) in source.lines().enumerate() {
            if is_raw_symbol_ref_cast(line) {
                hits.push(format!("{}:{}: {}", rel_path, line_idx + 1, line.trim()));
            }
        }
    }

    assert!(
        hits.len() <= RAW_SYMBOL_REF_CAST_BUDGET,
        "raw SymbolRef -> SymbolId casts must go through \
         query_boundaries::definition_identity::symbol_ref_to_symbol_id; \
         found {} casts, budget {}:\n{}",
        hits.len(),
        RAW_SYMBOL_REF_CAST_BUDGET,
        hits.join("\n")
    );
}

#[test]
fn test_module_augmentation_publishes_merged_defs_through_context_authority() {
    let mut source = fs::read_to_string("src/types/module_augmentation.rs")
        .expect("failed to read module_augmentation.rs");
    source.push_str(
        &fs::read_to_string("src/types/module_augmentation_redirect.rs")
            .expect("failed to read module_augmentation_redirect.rs"),
    );
    let non_comment_source = source
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .collect::<Vec<_>>()
        .join("\n");
    let compact_source = non_comment_source
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();

    assert!(
        compact_source.contains("register_augmented_def_in_envs(def_id,result,false)"),
        "global augmentation merged bodies must publish through CheckerContext"
    );
    assert!(
        compact_source.contains("register_augmented_def_in_envs(aug_def_id,merged_type,false)"),
        "augmentation-local self-reference bodies must publish through CheckerContext"
    );
    assert!(
        compact_source.contains("register_def_in_envs(home_def_id,merged_type)"),
        "augmented base-body redirects must publish through CheckerContext"
    );
    let raw_type_env_mut_borrows = non_comment_source.lines().filter(|line| {
        line.contains("type_env")
            && (line.contains("try_borrow_mut()") || line.contains("borrow_mut()"))
    });
    assert!(
        raw_type_env_mut_borrows.count() == 0,
        "module augmentation must not write type_env directly; route DefId bodies \
         through the deferred dual-env authority"
    );
    assert!(
        !non_comment_source.contains("env.insert_def("),
        "module augmentation must not publish DefId bodies with raw env.insert_def"
    );
}

#[test]
fn test_lazy_type_env_symbol_publication_uses_context_authority_on_contention() {
    let source =
        fs::read_to_string("src/state/type_environment/lazy.rs").expect("failed to read lazy.rs");
    let insert_type_env_symbol_src = source
        .split("pub(crate) fn insert_type_env_symbol")
        .nth(1)
        .and_then(|tail| tail.split("/// Resolve a `DefId`").next())
        .expect("failed to isolate insert_type_env_symbol");

    assert!(
        insert_type_env_symbol_src.contains("register_symbol_type_in_envs(")
            && insert_type_env_symbol_src.contains("register_def_auto_params_in_envs("),
        "insert_type_env_symbol must queue symbol/def writes through CheckerContext on evaluator-env borrow contention"
    );
    assert!(
        source.contains("register_class_instance_in_envs(def_id, resolved)")
            && source.contains("register_def_in_envs(def_id, resolved)"),
        "lazy import-alias def remaps must publish through CheckerContext instead of raw env writes"
    );
}

#[test]
fn test_env_eval_cache_def_invalidation_is_targeted() {
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let types = TypeInterner::new();
    let ctx = CheckerContext::new(
        &arena,
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    let stale_def = DefId(10_001);
    let unrelated_def = DefId(10_002);
    let stale_key = types.lazy(stale_def);
    let stale_result = types.lazy(stale_def);
    let unrelated_result = types.lazy(unrelated_def);

    ctx.cache_env_eval_result(stale_key, TypeId::STRING, false);
    ctx.cache_env_eval_result(TypeId::NUMBER, stale_result, false);
    ctx.cache_env_eval_result(TypeId::BOOLEAN, unrelated_result, false);

    assert_eq!(
        ctx.env_eval_cache_indexed_key_count_for_def(stale_def),
        2,
        "the reverse index should track key-side and result-side stale entries",
    );

    ctx.clear_type_evaluation_caches_for_def(stale_def);

    assert!(ctx.lookup_env_eval_cache(stale_key).is_none());
    assert!(ctx.lookup_env_eval_cache(TypeId::NUMBER).is_none());
    assert_eq!(
        ctx.env_eval_cache_indexed_key_count_for_def(stale_def),
        0,
        "def invalidation must prune reverse-index edges for removed entries",
    );
    assert_eq!(
        ctx.lookup_env_eval_cache(TypeId::BOOLEAN)
            .map(|entry| entry.result),
        Some(unrelated_result)
    );
}

#[test]
fn test_invalidate_env_eval_for_targets_single_entry() {
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let types = TypeInterner::new();
    let ctx = CheckerContext::new(
        &arena,
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    let target = types.lazy(DefId(11_001));
    let neighbor = types.lazy(DefId(11_002));

    ctx.cache_env_eval_result(target, TypeId::STRING, false);
    ctx.cache_env_eval_result(neighbor, TypeId::NUMBER, false);

    assert_eq!(
        ctx.env_eval_cache_indexed_key_count_for_def(DefId(11_001)),
        1
    );

    assert!(ctx.invalidate_env_eval_for(target));
    assert!(ctx.lookup_env_eval_cache(target).is_none());
    assert_eq!(
        ctx.env_eval_cache_indexed_key_count_for_def(DefId(11_001)),
        0,
        "single-key invalidation must remove the key from the reverse index",
    );
    assert_eq!(
        ctx.lookup_env_eval_cache(neighbor)
            .map(|entry| entry.result),
        Some(TypeId::NUMBER),
        "unrelated entries must survive a targeted single-key invalidation",
    );

    assert!(!ctx.invalidate_env_eval_for(target));
}

#[test]
fn test_invalidate_env_eval_reachable_from_clears_structural_closure() {
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let types = TypeInterner::new();
    let ctx = CheckerContext::new(
        &arena,
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    let member_a = types.lazy(DefId(12_001));
    let member_b = types.lazy(DefId(12_002));
    let composite = types.union(vec![member_a, member_b]);
    let outsider = types.lazy(DefId(12_003));
    let keep_key = types.lazy(DefId(12_004));
    let keep_result = types.lazy(DefId(12_005));

    ctx.cache_env_eval_result(composite, TypeId::STRING, false);
    ctx.cache_env_eval_result(member_a, TypeId::NUMBER, false);
    ctx.cache_env_eval_result(outsider, member_b, false);
    ctx.cache_env_eval_result(keep_key, keep_result, false);

    assert_eq!(
        ctx.env_eval_cache_indexed_key_count_for_def(DefId(12_001)),
        2
    );
    assert_eq!(
        ctx.env_eval_cache_indexed_key_count_for_def(DefId(12_002)),
        2
    );

    let removed = ctx.invalidate_env_eval_reachable_from(composite);
    assert_eq!(
        removed, 3,
        "root, reachable sub-term key, and result-side mention should be cleared",
    );

    assert!(ctx.lookup_env_eval_cache(composite).is_none());
    assert!(ctx.lookup_env_eval_cache(member_a).is_none());
    assert!(ctx.lookup_env_eval_cache(outsider).is_none());
    assert_eq!(
        ctx.env_eval_cache_indexed_key_count_for_def(DefId(12_001)),
        0,
        "reachable invalidation must prune all index edges for removed keys",
    );
    assert_eq!(
        ctx.env_eval_cache_indexed_key_count_for_def(DefId(12_002)),
        0,
        "result-side reachable invalidation must prune reverse-index edges",
    );
    assert_eq!(
        ctx.lookup_env_eval_cache(keep_key)
            .map(|entry| entry.result),
        Some(keep_result),
        "entries outside the reachable closure must survive",
    );
}

#[test]
fn test_env_eval_cache_index_tracks_overwrite_and_clear() {
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let types = TypeInterner::new();
    let ctx = CheckerContext::new(
        &arena,
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    let old_def = DefId(12_101);
    let new_def = DefId(12_102);
    let key = TypeId::NUMBER;

    ctx.cache_env_eval_result(key, types.lazy(old_def), false);
    assert_eq!(ctx.env_eval_cache_indexed_key_count_for_def(old_def), 1);
    assert_eq!(ctx.env_eval_cache_indexed_key_count_for_def(new_def), 0);

    ctx.cache_env_eval_result(key, types.lazy(new_def), false);
    assert_eq!(
        ctx.env_eval_cache_indexed_key_count_for_def(old_def),
        0,
        "overwriting an entry must drop stale reverse edges for the old result",
    );
    assert_eq!(ctx.env_eval_cache_indexed_key_count_for_def(new_def), 1);

    ctx.clear_type_evaluation_caches_for_def(old_def);
    assert_eq!(
        ctx.lookup_env_eval_cache(key).map(|entry| entry.result),
        Some(types.lazy(new_def)),
        "stale reverse edges must not let old-def invalidation evict the replacement entry",
    );

    ctx.clear_env_eval_cache();
    assert_eq!(ctx.env_eval_cache_indexed_key_count_for_def(new_def), 0);
    assert!(ctx.lookup_env_eval_cache(key).is_none());
}

#[test]
fn test_env_eval_cache_def_invalidation_follows_registered_alias_bodies() {
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let types = TypeInterner::new();
    let ctx = CheckerContext::new(
        &arena,
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    let alias_def = DefId(12_201);
    let dep_def = DefId(12_202);
    let unrelated_def = DefId(12_203);
    let alias_ref = types.lazy(alias_def);
    let dep_ref = types.lazy(dep_def);
    let unrelated_key = types.lazy(unrelated_def);
    let key = types.application(alias_ref, vec![TypeId::STRING]);

    ctx.definition_store.set_body(alias_def, dep_ref);
    ctx.cache_env_eval_result(key, TypeId::NUMBER, false);
    ctx.cache_env_eval_result(TypeId::BOOLEAN, alias_ref, false);
    ctx.cache_env_eval_result(unrelated_key, TypeId::STRING, false);

    assert_eq!(
        ctx.env_eval_cache_indexed_key_count_for_def(dep_def),
        2,
        "key-side and result-side alias refs should index their registered body dependency",
    );

    ctx.clear_type_evaluation_caches_for_def(dep_def);

    assert!(
        ctx.lookup_env_eval_cache(key).is_none(),
        "rewriting a def must invalidate entries whose key depends on it through an alias body",
    );
    assert!(
        ctx.lookup_env_eval_cache(TypeId::BOOLEAN).is_none(),
        "rewriting a def must invalidate entries whose result depends on it through an alias body",
    );
    assert_eq!(
        ctx.lookup_env_eval_cache(unrelated_key)
            .map(|entry| entry.result),
        Some(TypeId::STRING),
        "unrelated entries must survive transitive def invalidation",
    );
}
