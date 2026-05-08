use super::constructors::should_cache_base_expr_result;

#[test]
fn base_expr_cache_predicate_only_caches_non_generic_paths() {
    assert!(should_cache_base_expr_result(0, false));
    assert!(!should_cache_base_expr_result(0, true));
    assert!(!should_cache_base_expr_result(1, false));
    assert!(!should_cache_base_expr_result(3, false));
}
