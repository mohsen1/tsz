//! Subprocess cache-toggle agreement probes (#14351).
//!
//! The limit-result cache switch is latched through `OnceLock`, so this test
//! spawns the integration-test binary as a child process for each mode. The
//! child uses only public solver APIs and prints a stable summary line.

use std::process::Command;

use tsz_solver::construction::{QueryCache, RelationCacheProbe, TypeInterner};
use tsz_solver::def::DefId;
use tsz_solver::relations::relation_queries::{
    RelationContext, RelationKind, RelationPolicy, query_relation_with_resolver,
};
use tsz_solver::{PropertyInfo, RelationCacheKey, TypeId};

const CHILD_ENV: &str = "TSZ_LIMIT_RESULT_CACHE_PROBE_CHILD";
const LIMIT_RESULT_SWITCH: &str = "TSZ_DISABLE_LIMIT_RESULT_CACHE";
const SUMMARY_PREFIX: &str = "TSZ_LIMIT_RESULT_CACHE_PROBE ";

fn run_child(disable_limit_result_cache: bool) -> String {
    let current_exe = std::env::current_exe().expect("current test binary");
    let mut cmd = Command::new(current_exe);
    cmd.args([
        "--ignored",
        "--exact",
        "limit_result_cache_probe_child",
        "--nocapture",
    ])
    .env(CHILD_ENV, "1")
    .env_remove(LIMIT_RESULT_SWITCH);
    if disable_limit_result_cache {
        cmd.env(LIMIT_RESULT_SWITCH, "1");
    }

    let output = cmd.output().expect("run limit-result cache probe child");
    assert!(
        output.status.success(),
        "child probe failed; stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix(SUMMARY_PREFIX).map(str::to_owned))
        .unwrap_or_else(|| {
            panic!(
                "child probe did not print summary; stdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        })
}

#[test]
fn limit_result_cache_switch_changes_cache_evidence_not_semantics() {
    let enabled = run_child(false);
    let disabled = run_child(true);

    assert!(
        enabled.contains("outer=true:false;direct=true:false;"),
        "enabled run should complete the recursive relation without a relation-depth bail: {enabled}",
    );
    assert!(
        disabled.contains("outer=true:false;direct=true:false;"),
        "disabled run should keep the same recursive relation result: {disabled}",
    );
    assert!(
        enabled.contains("probe=hit_true;"),
        "cache-enabled run should promote the cycle-derived maybe key: {enabled}",
    );
    assert!(
        disabled.contains("probe=miss;"),
        "cache-disabled run should leave the cycle-derived maybe key uncached: {disabled}",
    );
}

#[test]
#[ignore = "spawned by limit_result_cache_switch_changes_cache_evidence_not_semantics"]
fn limit_result_cache_probe_child() {
    if std::env::var_os(CHILD_ENV).is_none() {
        return;
    }
    println!("{SUMMARY_PREFIX}{}", compute_limit_result_cache_probe());
}

fn compute_limit_result_cache_probe() -> String {
    let interner = TypeInterner::new();
    let s_def = DefId(1_435_101);
    let t_def = DefId(1_435_102);
    let lazy_s = interner.lazy(s_def);
    let lazy_t = interner.lazy(t_def);
    let arr_s = interner.array(lazy_s);
    let arr_t = interner.array(lazy_t);
    let next = interner.intern_string("next");
    let tag = interner.intern_string("tag");
    let body_s = interner.object(vec![
        PropertyInfo::new(next, arr_s),
        PropertyInfo::new(tag, TypeId::NUMBER),
    ]);
    let body_t = interner.object(vec![
        PropertyInfo::new(next, arr_t),
        PropertyInfo::new(tag, TypeId::NUMBER),
    ]);

    let mut env = tsz_solver::computation::TypeEnvironment::new();
    env.insert_def(s_def, body_s);
    env.insert_def(t_def, body_t);

    let db = QueryCache::new(&interner);
    let policy = RelationPolicy::default();
    let context = RelationContext {
        query_db: Some(&db),
        ..Default::default()
    };
    let outer = query_relation_with_resolver(
        &interner,
        &env,
        lazy_s,
        lazy_t,
        RelationKind::Subtype,
        policy,
        context,
    );
    let arr_key = RelationCacheKey::for_subtype(arr_s, arr_t, policy.cache_config());
    let probe = match db.probe_subtype_cache(arr_key) {
        RelationCacheProbe::Hit(true) => "hit_true",
        RelationCacheProbe::Hit(false) => "hit_false",
        RelationCacheProbe::MissNotCached => "miss",
    };

    db.reset_relation_cache_stats();
    let direct = query_relation_with_resolver(
        &interner,
        &env,
        arr_s,
        arr_t,
        RelationKind::Subtype,
        policy,
        context,
    );
    let stats = db.statistics().relation;

    format!(
        "outer={}:{};direct={}:{};probe={probe};hits={};misses={};entries={}",
        outer.is_related(),
        outer.depth_exceeded(),
        direct.is_related(),
        direct.depth_exceeded(),
        stats.subtype_hits,
        stats.subtype_misses,
        stats.subtype_entries,
    )
}
