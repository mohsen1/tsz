//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/query_boundaries/assignability/cache_key.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 695bb58c133073a0d37cc426cc994b21aeeb2c326528e58cc2d867c481295454 124 checker_relation_cache_keys_partition_by_inheritance_graph_generation
    #[test]
    fn checker_relation_cache_keys_partition_by_inheritance_graph_generation() {
        let graph = InheritanceGraph::new();
        let before_assignability =
            assignability_cache_key(TypeId::STRING, TypeId::NUMBER, 0, &graph);
        let before_final =
            checker_final_assignability_cache_key(TypeId::STRING, TypeId::NUMBER, 0, &graph);
        let before_subtype = subtype_cache_key(TypeId::STRING, TypeId::NUMBER, 0, &graph);

        assert_eq!(before_assignability.inheritance_graph_id, graph.identity());
        assert_eq!(before_final.inheritance_graph_id, graph.identity());
        assert_eq!(before_subtype.inheritance_graph_id, graph.identity());
        assert_eq!(
            before_assignability.inheritance_graph_generation,
            graph.generation()
        );

        graph.add_inheritance(SymbolId(1), &[SymbolId(2)]);

        let after_assignability =
            assignability_cache_key(TypeId::STRING, TypeId::NUMBER, 0, &graph);
        let after_final =
            checker_final_assignability_cache_key(TypeId::STRING, TypeId::NUMBER, 0, &graph);
        let after_subtype = subtype_cache_key(TypeId::STRING, TypeId::NUMBER, 0, &graph);

        assert_eq!(after_assignability.inheritance_graph_id, graph.identity());
        assert_ne!(before_assignability, after_assignability);
        assert_ne!(before_final, after_final);
        assert_ne!(before_subtype, after_subtype);
    }
// TSZ_INLINE_TEST_END 695bb58c133073a0d37cc426cc994b21aeeb2c326528e58cc2d867c481295454

// TSZ_INLINE_TEST_BEGIN 4dcd6fdd7b0458f00f03c890d38c75e5b607f718682733ad84f3fc8b93ce1d05 155 checker_relation_cache_keys_partition_typed_rest_and_any_policies
    #[test]
    fn checker_relation_cache_keys_partition_typed_rest_and_any_policies() {
        let graph = InheritanceGraph::new();
        let base_policy =
            relation_policy::from_checker_flags_u16(RelationFlags::STRICT_FUNCTION_TYPES);
        let provisional_policy = base_policy.with_provisional_rest_union(true);
        let overload_policy = base_policy.with_any_propagation_mode(
            tsz_solver::relations::subtype::AnyPropagationMode::AnySourceNotRelated,
        );
        let overload_provisional_policy = provisional_policy.with_any_propagation_mode(
            tsz_solver::relations::subtype::AnyPropagationMode::AnySourceNotRelated,
        );

        let base =
            assignability_cache_key_for_policy(TypeId::STRING, TypeId::NUMBER, base_policy, &graph);
        let provisional = assignability_cache_key_for_policy(
            TypeId::STRING,
            TypeId::NUMBER,
            provisional_policy,
            &graph,
        );
        let overload = assignability_cache_key_for_policy(
            TypeId::STRING,
            TypeId::NUMBER,
            overload_policy,
            &graph,
        );
        let overload_provisional = assignability_cache_key_for_policy(
            TypeId::STRING,
            TypeId::NUMBER,
            overload_provisional_policy,
            &graph,
        );

        assert_ne!(base, provisional);
        assert_ne!(base, overload);
        assert_ne!(provisional, overload_provisional);
        assert_ne!(overload, overload_provisional);
        assert_ne!(provisional, overload);
        assert_ne!(base, overload_provisional);
    }
// TSZ_INLINE_TEST_END 4dcd6fdd7b0458f00f03c890d38c75e5b607f718682733ad84f3fc8b93ce1d05
