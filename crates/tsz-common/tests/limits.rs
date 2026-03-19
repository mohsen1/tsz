use super::*;

const _: () = {
    assert!(MAX_INSTANTIATION_DEPTH == 100);
    assert!(MAX_CALL_DEPTH == 20);
    assert!(MAX_SUBTYPE_DEPTH == 100);
    assert!(MAX_TREE_WALK_ITERATIONS == 10_000);
    assert!(MAX_IN_PROGRESS_PAIRS == 10_000);
    assert!(MAX_PARSER_RECURSION_DEPTH == 1_000);
    #[cfg(target_arch = "wasm32")]
    assert!(MAX_TYPE_RESOLUTION_OPS == 20_000);
    #[cfg(not(target_arch = "wasm32"))]
    assert!(MAX_TYPE_RESOLUTION_OPS == 100_000);
};
