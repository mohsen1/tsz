//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/type_queries/data/exact_property_keys.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 21b01f74888f353523ab75a57ae1ebf6934d7452c60a790650e64d8d638316d1 755 keyof_deferred_remapped_intersection_collects_only_output_keys
    #[test]
    fn keyof_deferred_remapped_intersection_collects_only_output_keys() {
        let interner = TypeInterner::new();
        let retained = interner.literal_string("retainedKey");
        let payload = interner.literal_string("payloadKey");
        let filtered = interner.literal_string("filteredKey");
        let numeric = interner.literal_number(7.0);
        let numeric_string = interner.literal_string("7");
        let symbol = interner.unique_symbol(crate::types::SymbolRef(7001));
        let constraint = interner.union(vec![
            retained,
            payload,
            filtered,
            numeric,
            numeric_string,
            symbol,
        ]);
        let recursive_template = interner.object(vec![PropertyInfo::new(
            interner.intern_string("next"),
            interner.recursive(0),
        )]);
        let first =
            deferred_filter_map(&interner, "Entry", constraint, retained, recursive_template);
        let second =
            deferred_filter_map(&interner, "Slot", constraint, payload, recursive_template);
        let third =
            deferred_filter_map(&interner, "Index", constraint, numeric, recursive_template);
        let fourth =
            deferred_filter_map(&interner, "Marker", constraint, symbol, recursive_template);
        let fifth = deferred_filter_map(
            &interner,
            "QuotedIndex",
            constraint,
            numeric_string,
            recursive_template,
        );
        for mapped in [first, second, third, fourth, fifth] {
            let evaluated = crate::evaluation::evaluate::evaluate_type(&interner, mapped);
            assert!(
                matches!(interner.lookup(evaluated), Some(TypeData::Mapped(_))),
                "the witness must reach the deferred mapped-operand path"
            );
        }
        let operand = interner.intersection(vec![first, second, third, fourth, fifth]);

        let mut keys = FxHashSet::default();
        let (success, steps) = with_exact_key_scratch(|scratch| {
            let mut traversal = ExactKeyTraversal::new(scratch);
            let success = collect_exact_literal_property_keys_from_keyof_operand_with_symbol_info(
                &interner,
                operand,
                &mut keys,
                &mut traversal,
            );
            (success, traversal.steps)
        });
        success.expect("the remapped intersection has a finite exact key set");
        assert!(
            steps < 256,
            "finite key-only traversal should stay linear in this small graph; used {steps} steps"
        );
        let observed: Vec<_> = keys
            .into_iter()
            .map(|key| {
                (
                    interner.resolve_atom_ref(key.name).to_string(),
                    key.is_symbol_named,
                    key.is_string_named,
                )
            })
            .collect();
        assert_eq!(observed.len(), 5);
        assert!(
            observed
                .iter()
                .any(|key| key == &("retainedKey".into(), false, true))
        );
        assert!(
            observed
                .iter()
                .any(|key| key == &("payloadKey".into(), false, true))
        );
        assert!(
            observed
                .iter()
                .any(|key| key == &("7".into(), false, false))
        );
        assert!(observed.iter().any(|key| key == &("7".into(), false, true)));
        assert!(observed.iter().any(|key| key.1));
        assert!(!observed.iter().any(|key| key.0 == "filteredKey"));
    }
// TSZ_INLINE_TEST_END 21b01f74888f353523ab75a57ae1ebf6934d7452c60a790650e64d8d638316d1

// TSZ_INLINE_TEST_BEGIN 570bb3c3a236696f01e10971a7598612acb22c7b51abf328ec9e5e3c2bfd2399 847 keyof_identity_mapped_intersection_preserves_well_known_symbol_atom
    #[test]
    fn keyof_identity_mapped_intersection_preserves_well_known_symbol_atom() {
        let interner = TypeInterner::new();
        let iterator_name = interner.intern_string("[Symbol.iterator]");
        let mut iterator_property = PropertyInfo::new(iterator_name, TypeId::STRING);
        iterator_property.is_symbol_named = true;
        let source = interner.object(vec![iterator_property]);
        let source_keys = interner.keyof(source);
        let first = identity_map(&interner, "Entry", source_keys, TypeId::STRING);
        let second = identity_map(&interner, "Slot", source_keys, TypeId::NUMBER);
        let operand = interner.intersect_types_raw(vec![first, second]);

        for mapped in [first, second] {
            let mut mapped_keys = FxHashSet::default();
            let mapped_success = with_exact_key_scratch(|scratch| {
                let mut traversal = ExactKeyTraversal::new(scratch);
                collect_exact_literal_property_keys_from_keyof_operand_with_symbol_info(
                    &interner,
                    mapped,
                    &mut mapped_keys,
                    &mut traversal,
                )
            });
            mapped_success.expect("each raw identity mapped operand has exact keys");
            assert_eq!(mapped_keys.len(), 1);
        }

        let mut keys = FxHashSet::default();
        let success = with_exact_key_scratch(|scratch| {
            let mut traversal = ExactKeyTraversal::new(scratch);
            collect_exact_literal_property_keys_from_keyof_operand_with_symbol_info(
                &interner,
                operand,
                &mut keys,
                &mut traversal,
            )
        });

        success.expect("raw identity mapped operands have an exact key set");
        assert_eq!(
            keys,
            FxHashSet::from_iter([ExactLiteralPropertyKey {
                name: iterator_name,
                is_symbol_named: true,
                is_string_named: false,
            }])
        );
    }
// TSZ_INLINE_TEST_END 570bb3c3a236696f01e10971a7598612acb22c7b51abf328ec9e5e3c2bfd2399

// TSZ_INLINE_TEST_BEGIN 69c193adceaae4078e0351aad262231eeb5ed7f180dac9a64a8f72919191c4cd 896 keyof_union_inside_deferred_mapped_intersection_keeps_only_common_keys
    #[test]
    fn keyof_union_inside_deferred_mapped_intersection_keeps_only_common_keys() {
        let interner = TypeInterner::new();
        let common = interner.intern_string("common");
        let branch_a = interner.object(vec![
            PropertyInfo::new(common, TypeId::STRING),
            PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        ]);
        let branch_b = interner.object(vec![
            PropertyInfo::new(common, TypeId::STRING),
            PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
        ]);
        let union = interner.union_preserve_members(vec![branch_a, branch_b]);
        let recursive_template = interner.recursive(0);
        let common_map = deferred_identity_remap(
            &interner,
            "BranchKey",
            interner.keyof(union),
            recursive_template,
        );
        let marker_key = interner.literal_string("marker");
        let marker_map =
            deferred_identity_remap(&interner, "MarkerKey", marker_key, recursive_template);
        let operand = interner.intersection(vec![common_map, marker_map]);

        assert!(matches!(
            interner.lookup(common_map),
            Some(TypeData::Mapped(_))
        ));
        assert!(matches!(
            interner.lookup(marker_map),
            Some(TypeData::Mapped(_))
        ));
        let mut keys = FxHashSet::default();
        let success = with_exact_key_scratch(|scratch| {
            let mut traversal = ExactKeyTraversal::new(scratch);
            collect_exact_literal_property_keys_from_keyof_operand_with_symbol_info(
                &interner,
                operand,
                &mut keys,
                &mut traversal,
            )
        });
        success.expect("the deferred mapped intersection has finite exact keys");
        let names: FxHashSet<_> = keys.into_iter().map(|key| key.name).collect();

        assert_eq!(
            names,
            FxHashSet::from_iter([common, interner.intern_string("marker")])
        );
        assert!(!names.contains(&interner.intern_string("a")));
        assert!(!names.contains(&interner.intern_string("b")));
    }
// TSZ_INLINE_TEST_END 69c193adceaae4078e0351aad262231eeb5ed7f180dac9a64a8f72919191c4cd
