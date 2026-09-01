//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/flow/control_flow/flow_dp.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN f21186931b26bef4581322bb4f92add4f627d780282c3c529132d72334a3333d 302 chain_reachability_none_root_is_false
    #[test]
    fn chain_reachability_none_root_is_false() {
        let expansions = Cell::new(0);
        let (ants, flag) = graph_closures(&[], &[], &expansions);
        let mut memo = ChainReachabilityMemo::default();
        assert!(!resolve_chain_reachability(
            FlowNodeId::NONE,
            &mut memo,
            ants,
            flag
        ));
        assert_eq!(expansions.get(), 0);
    }
// TSZ_INLINE_TEST_END f21186931b26bef4581322bb4f92add4f627d780282c3c529132d72334a3333d

// TSZ_INLINE_TEST_BEGIN 23f4fedb36d26a38820563bc9ae53e947c7b27c23772669ee995f50198672eb1 316 chain_reachability_finds_flag_through_linear_chain
    #[test]
    fn chain_reachability_finds_flag_through_linear_chain() {
        // 1(flagged) <- 2 <- 3
        let edges: &[(u32, &[u32])] = &[(3, &[2]), (2, &[1]), (1, &[])];
        let expansions = Cell::new(0);
        let (ants, flag) = graph_closures(edges, &[1], &expansions);
        let mut memo = ChainReachabilityMemo::default();
        assert!(resolve_chain_reachability(
            FlowNodeId(3),
            &mut memo,
            &ants,
            &flag
        ));
        // The discovery path 3 -> 2 -> 1 is marked, so upstream worklist
        // queries are memo hits with no further graph expansion.
        let after_first = expansions.get();
        assert!(resolve_chain_reachability(
            FlowNodeId(2),
            &mut memo,
            &ants,
            &flag
        ));
        assert!(resolve_chain_reachability(
            FlowNodeId(1),
            &mut memo,
            &ants,
            &flag
        ));
        assert_eq!(expansions.get(), after_first);
    }
// TSZ_INLINE_TEST_END 23f4fedb36d26a38820563bc9ae53e947c7b27c23772669ee995f50198672eb1

// TSZ_INLINE_TEST_BEGIN 1f68adf8949968da51dfd2c330106e352bd0b09e8fc3180fdd8be3a03e066740 347 chain_reachability_exact_on_loop_back_edge
    #[test]
    fn chain_reachability_exact_on_loop_back_edge() {
        // The shape that breaks a fold-based OR DP: a switch upstream of a
        // loop. Loop header 2 has antecedents [3 (back-edge), 1 (entry)],
        // loop-body node 3 has antecedent [2], node 1 is flagged, and the
        // reference node 4 hangs off the header. A fold DP querying from 4
        // resolves 3 while 2 is still in progress and memoizes a wrong
        // `false` for 3; exact reachability must say `true` for every node.
        let edges: &[(u32, &[u32])] = &[(4, &[2]), (2, &[3, 1]), (3, &[2]), (1, &[])];
        let expansions = Cell::new(0);
        let (ants, flag) = graph_closures(edges, &[1], &expansions);
        let mut memo = ChainReachabilityMemo::default();
        // Downstream-most node first, mirroring `check_flow` worklist order.
        assert!(resolve_chain_reachability(
            FlowNodeId(4),
            &mut memo,
            &ants,
            &flag
        ));
        assert!(resolve_chain_reachability(
            FlowNodeId(2),
            &mut memo,
            &ants,
            &flag
        ));
        assert!(resolve_chain_reachability(
            FlowNodeId(3),
            &mut memo,
            &ants,
            &flag
        ));
        assert!(resolve_chain_reachability(
            FlowNodeId(1),
            &mut memo,
            &ants,
            &flag
        ));
    }
// TSZ_INLINE_TEST_END 1f68adf8949968da51dfd2c330106e352bd0b09e8fc3180fdd8be3a03e066740

// TSZ_INLINE_TEST_BEGIN 93cb4676df93757e6f4ff457adf5e4607ae2048d89f6670d976b3d28633eabba 386 chain_reachability_negative_chain_is_memoized
    #[test]
    fn chain_reachability_negative_chain_is_memoized() {
        // Diamond with a cycle and no flag anywhere:
        // 4 <- {2, 3}; 2 <- 1; 3 <- 1; 1 <- 4 (back-edge).
        let edges: &[(u32, &[u32])] = &[(4, &[2, 3]), (2, &[1]), (3, &[1]), (1, &[4])];
        let expansions = Cell::new(0);
        let (ants, flag) = graph_closures(edges, &[], &expansions);
        let mut memo = ChainReachabilityMemo::default();
        assert!(!resolve_chain_reachability(
            FlowNodeId(4),
            &mut memo,
            &ants,
            &flag
        ));
        let after_first = expansions.get();
        // Every node the first query visited is proven `false`; repeated
        // worklist queries are pure memo hits.
        for id in [4, 3, 2, 1] {
            assert!(!resolve_chain_reachability(
                FlowNodeId(id),
                &mut memo,
                &ants,
                &flag
            ));
        }
        assert_eq!(expansions.get(), after_first);
    }
// TSZ_INLINE_TEST_END 93cb4676df93757e6f4ff457adf5e4607ae2048d89f6670d976b3d28633eabba

// TSZ_INLINE_TEST_BEGIN 4d673a6fe6b70061d0b41cd384206d908cbb95396dacb83c51944a88d99d39c8 414 chain_reachability_flag_on_root_node
    #[test]
    fn chain_reachability_flag_on_root_node() {
        let edges: &[(u32, &[u32])] = &[(1, &[])];
        let expansions = Cell::new(0);
        let (ants, flag) = graph_closures(edges, &[1], &expansions);
        let mut memo = ChainReachabilityMemo::default();
        assert!(resolve_chain_reachability(
            FlowNodeId(1),
            &mut memo,
            &ants,
            &flag
        ));
    }
// TSZ_INLINE_TEST_END 4d673a6fe6b70061d0b41cd384206d908cbb95396dacb83c51944a88d99d39c8

// TSZ_INLINE_TEST_BEGIN 7e80dd41d21f1651f990fcee15f691bdc43fae40e3d0d6e899d4d1edbf7b5f29 428 chain_reachability_clear_resets_verdicts
    #[test]
    fn chain_reachability_clear_resets_verdicts() {
        let edges: &[(u32, &[u32])] = &[(2, &[1]), (1, &[])];
        let expansions = Cell::new(0);
        let (ants, flag) = graph_closures(edges, &[1], &expansions);
        let mut memo = ChainReachabilityMemo::default();
        assert!(resolve_chain_reachability(
            FlowNodeId(2),
            &mut memo,
            &ants,
            &flag
        ));
        memo.clear();
        let before = expansions.get();
        assert!(resolve_chain_reachability(
            FlowNodeId(2),
            &mut memo,
            &ants,
            &flag
        ));
        assert!(expansions.get() > before);
    }
// TSZ_INLINE_TEST_END 7e80dd41d21f1651f990fcee15f691bdc43fae40e3d0d6e899d4d1edbf7b5f29
