//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/flow/control_flow/core/flow_cache_policy.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 7b132937fd40fa6aa27e3b77d5b75d3de08bef6c8eaa368444a5b2c272dbbea3 114 concrete_stable_flow_allows_cache_read_and_write
    #[test]
    fn concrete_stable_flow_allows_cache_read_and_write() {
        let policy = FlowCachePolicy::new(TypeId::NUMBER, false, false);

        assert!(policy.allows_read(FlowCacheRead {
            is_switch_clause: false,
            is_loop_label_node: false,
            bypass: FlowCacheBypass::none(),
        }));
        assert!(policy.allows_write(FlowCacheWrite {
            is_loop_label_node: false,
            bypass: FlowCacheBypass::none(),
            final_type: TypeId::STRING,
            final_has_type_params: false,
            unreachable_never: TypeId::NEVER,
        }));
    }
// TSZ_INLINE_TEST_END 7b132937fd40fa6aa27e3b77d5b75d3de08bef6c8eaa368444a5b2c272dbbea3

// TSZ_INLINE_TEST_BEGIN b44245d60c4a235b9badb8b55f6db4a212efb5d8881b1399dec1e14c699603cf 132 generic_initial_or_final_type_blocks_shared_writes
    #[test]
    fn generic_initial_or_final_type_blocks_shared_writes() {
        let generic_initial = FlowCachePolicy::new(TypeId::NUMBER, true, false);
        let concrete_initial = FlowCachePolicy::new(TypeId::NUMBER, false, false);

        assert!(!generic_initial.allows_write(FlowCacheWrite {
            is_loop_label_node: true,
            bypass: FlowCacheBypass::none(),
            final_type: TypeId::STRING,
            final_has_type_params: false,
            unreachable_never: TypeId::NEVER,
        }));
        assert!(!concrete_initial.allows_write(FlowCacheWrite {
            is_loop_label_node: false,
            bypass: FlowCacheBypass::none(),
            final_type: TypeId::STRING,
            final_has_type_params: true,
            unreachable_never: TypeId::NEVER,
        }));
    }
// TSZ_INLINE_TEST_END b44245d60c4a235b9badb8b55f6db4a212efb5d8881b1399dec1e14c699603cf

// TSZ_INLINE_TEST_BEGIN 15669f4653cd548acbab5f85d014d7f69762e4524a6c1c6a8e4ab63fd3b65efe 153 provisional_walk_blocks_pending_writes
    #[test]
    fn provisional_walk_blocks_pending_writes() {
        let mut policy = FlowCachePolicy::new(TypeId::NUMBER, false, false);

        policy.mark_provisional();

        assert_eq!(policy.stability(), FlowCacheStability::Provisional);
        assert!(!policy.allows_pending_writes());
        assert!(!policy.allows_write(FlowCacheWrite {
            is_loop_label_node: false,
            bypass: FlowCacheBypass::none(),
            final_type: TypeId::STRING,
            final_has_type_params: false,
            unreachable_never: TypeId::NEVER,
        }));
    }
// TSZ_INLINE_TEST_END 15669f4653cd548acbab5f85d014d7f69762e4524a6c1c6a8e4ab63fd3b65efe

// TSZ_INLINE_TEST_BEGIN aea02304ecc938bbe06aac7d53d64a0e41f7b09c86223b58ffc44ded9699ba61 170 loop_label_can_read_recursion_guard_cache_for_generic_or_any_walks
    #[test]
    fn loop_label_can_read_recursion_guard_cache_for_generic_or_any_walks() {
        let generic_policy = FlowCachePolicy::new(TypeId::NUMBER, true, false);
        let control_flow_any_policy = FlowCachePolicy::new(TypeId::ANY, false, true);

        let loop_read = FlowCacheRead {
            is_switch_clause: false,
            is_loop_label_node: true,
            bypass: FlowCacheBypass::none(),
        };

        assert!(generic_policy.allows_read(loop_read));
        assert!(control_flow_any_policy.allows_read(loop_read));
    }
// TSZ_INLINE_TEST_END aea02304ecc938bbe06aac7d53d64a0e41f7b09c86223b58ffc44ded9699ba61

// TSZ_INLINE_TEST_BEGIN bbeb3cb5609519161fa39d15afc2220e427de8310be9cbea803f15d304ac3dc1 185 explicit_unknown_paths_skip_cache_without_marking_walk_provisional
    #[test]
    fn explicit_unknown_paths_skip_cache_without_marking_walk_provisional() {
        let policy = FlowCachePolicy::new(TypeId::UNKNOWN, false, false);

        assert!(!policy.allows_read(FlowCacheRead {
            is_switch_clause: false,
            is_loop_label_node: false,
            bypass: FlowCacheBypass::new(true, false),
        }));
        assert!(!policy.allows_write(FlowCacheWrite {
            is_loop_label_node: false,
            bypass: FlowCacheBypass::new(false, true),
            final_type: TypeId::STRING,
            final_has_type_params: false,
            unreachable_never: TypeId::NEVER,
        }));
        assert_eq!(policy.stability(), FlowCacheStability::Stable);
    }
// TSZ_INLINE_TEST_END bbeb3cb5609519161fa39d15afc2220e427de8310be9cbea803f15d304ac3dc1
