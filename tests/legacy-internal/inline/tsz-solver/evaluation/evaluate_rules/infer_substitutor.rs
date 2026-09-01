//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/evaluation/evaluate_rules/infer_substitutor.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN b6bdd50bc731eeb804884d920907dd2d3b1022d2aa53ea59e95430f2d7cea8f2 794 budget_at_or_above_node_count_substitutes_fully
    /// A budget at least as large as the node count substitutes every element —
    /// byte-identical to the default (calibrated, effectively unbounded) path.
    #[test]
    fn budget_at_or_above_node_count_substitutes_fully() {
        let interner = TypeInterner::new();
        let (input, bindings, values) = bound_name_tuple(&interner, 5);
        let expected = interner.tuple(values.iter().copied().map(TupleElement::fixed).collect());

        // The tuple node plus its five elements are six distinct visits.
        let bounded =
            InferSubstitutor::with_visit_budget(&interner, &bindings, 6).substitute(input);
        let default = InferSubstitutor::new(&interner, &bindings).substitute(input);

        assert_eq!(bounded, expected, "full budget substitutes every element");
        assert_eq!(
            bounded, default,
            "an at-capacity budget matches the calibrated default path"
        );
    }
// TSZ_INLINE_TEST_END b6bdd50bc731eeb804884d920907dd2d3b1022d2aa53ea59e95430f2d7cea8f2

// TSZ_INLINE_TEST_BEGIN 4def031c14bfe402afbfc86c54c22391dcda35b8eefc89008f9f0c0ed0b38d89 815 exhausted_budget_leaves_remaining_nodes_opaque
    /// Once the budget is spent the remaining elements are left opaque
    /// (identity) rather than substituted — a relation-preserving partial
    /// result, the same bail shape the depth guard takes.
    #[test]
    fn exhausted_budget_leaves_remaining_nodes_opaque() {
        let interner = TypeInterner::new();
        let (input, bindings, values) = bound_name_tuple(&interner, 5);

        // Budget 3: the tuple (1) and the first two elements (2) are visited;
        // the last three elements see an empty budget and stay unsubstituted.
        let bounded =
            InferSubstitutor::with_visit_budget(&interner, &bindings, 3).substitute(input);

        let full = interner.tuple(values.iter().copied().map(TupleElement::fixed).collect());
        assert_ne!(bounded, full, "a spent budget must not fully substitute");

        let Some(TypeData::Tuple(list)) = interner.lookup(bounded) else {
            panic!("substitution result is still a tuple");
        };
        let elements = interner.tuple_list(list);
        assert_eq!(elements[0].type_id, values[0], "element 0 substituted");
        assert_eq!(elements[1].type_id, values[1], "element 1 substituted");
        for (i, original) in [2usize, 3, 4].into_iter().enumerate() {
            // Untouched elements equal the original `UnresolvedTypeName(Name_i)`.
            let name = interner.intern_string(&format!("Name{original}"));
            assert_eq!(
                elements[original].type_id,
                interner.unresolved_type_name(name),
                "element {original} (index past budget {i}) is left opaque",
            );
        }
    }
// TSZ_INLINE_TEST_END 4def031c14bfe402afbfc86c54c22391dcda35b8eefc89008f9f0c0ed0b38d89

// TSZ_INLINE_TEST_BEGIN 68c482333a500501978b20f70fece0d13a6051d2ec36cd5796f7d9677e40a6c3 847 budget_truncation_is_deterministic
    /// The bail point is a deterministic function of the input and the budget,
    /// so the same inputs always truncate identically (no schedule sensitivity).
    #[test]
    fn budget_truncation_is_deterministic() {
        let interner = TypeInterner::new();
        let (input, bindings, _) = bound_name_tuple(&interner, 8);
        let first = InferSubstitutor::with_visit_budget(&interner, &bindings, 4).substitute(input);
        let second = InferSubstitutor::with_visit_budget(&interner, &bindings, 4).substitute(input);
        assert_eq!(
            first, second,
            "identical input + budget truncates identically"
        );
    }
// TSZ_INLINE_TEST_END 68c482333a500501978b20f70fece0d13a6051d2ec36cd5796f7d9677e40a6c3

// TSZ_INLINE_TEST_BEGIN 561eff149b53f6a2db66cfe79cfbf38a14226364d256de45f4de9c1a9ac456f1 860 zero_budget_returns_input_unchanged
    /// A zero budget is a hard stop: the top-level type is returned unchanged.
    #[test]
    fn zero_budget_returns_input_unchanged() {
        let interner = TypeInterner::new();
        let (input, bindings, _) = bound_name_tuple(&interner, 3);
        let bounded =
            InferSubstitutor::with_visit_budget(&interner, &bindings, 0).substitute(input);
        assert_eq!(bounded, input, "a zero budget substitutes nothing");
    }
// TSZ_INLINE_TEST_END 561eff149b53f6a2db66cfe79cfbf38a14226364d256de45f4de9c1a9ac456f1
