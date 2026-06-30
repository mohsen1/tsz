use std::fs;
use std::path::PathBuf;

fn checker_src_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(relative)
}

#[test]
fn flow_cache_stability_decisions_use_policy_owner() {
    let policy_rs = fs::read_to_string(checker_src_path(
        "flow/control_flow/core/flow_cache_policy.rs",
    ))
    .expect("failed to read flow_cache_policy.rs");
    let traversal_rs =
        fs::read_to_string(checker_src_path("flow/control_flow/core/flow_traversal.rs"))
            .expect("failed to read flow_traversal.rs");

    assert!(
        policy_rs.contains("struct FlowCachePolicy")
            && policy_rs.contains("enum FlowCacheStability")
            && policy_rs.contains("mark_provisional"),
        "flow cache read/write stability must be owned by a named policy"
    );
    assert!(
        traversal_rs.contains("FlowCachePolicy::new")
            && traversal_rs.contains(".mark_provisional(")
            && !traversal_rs.contains("let mut cacheable_walk = true")
            && !traversal_rs.contains("cacheable_walk = false"),
        "flow traversal should ask FlowCachePolicy for stability instead of \
         carrying an inline cacheable_walk boolean"
    );
}
