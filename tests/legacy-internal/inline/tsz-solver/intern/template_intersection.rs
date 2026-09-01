//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/intern/template_intersection.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN b8da8f9f42f7f2e6fa012e6459d9cec19af5bc3bc87c6d13bad43a7bb021f771 372 matching_string_literal_drops_redundant_numeric_template
    #[test]
    fn matching_string_literal_drops_redundant_numeric_template() {
        let interner = TypeInterner::new();
        let zero = interner.literal_string("0");
        let result = interner.intersection(vec![zero, numeric_template(&interner)]);
        assert_eq!(result, zero, "\"0\" & `${{number}}` should reduce to \"0\"");
    }
// TSZ_INLINE_TEST_END b8da8f9f42f7f2e6fa012e6459d9cec19af5bc3bc87c6d13bad43a7bb021f771

// TSZ_INLINE_TEST_BEGIN a063959eac10a59a2a91f94b8a428ef81e8d848d2ecea1617c36fc85117e50eb 380 nonmatching_string_literal_collapses_numeric_template_to_never
    #[test]
    fn nonmatching_string_literal_collapses_numeric_template_to_never() {
        let interner = TypeInterner::new();
        let length = interner.literal_string("length");
        let result = interner.intersection(vec![length, numeric_template(&interner)]);
        assert_eq!(
            result,
            TypeId::NEVER,
            "\"length\" & `${{number}}` should reduce to never"
        );
    }
// TSZ_INLINE_TEST_END a063959eac10a59a2a91f94b8a428ef81e8d848d2ecea1617c36fc85117e50eb

// TSZ_INLINE_TEST_BEGIN e0355a0876d5ee8e76b8846ce37b11c375f7983dae727988bebb831119ccaca5 392 numeric_template_filters_string_literal_union_keys
    #[test]
    fn numeric_template_filters_string_literal_union_keys() {
        let interner = TypeInterner::new();
        let zero = interner.literal_string("0");
        let one = interner.literal_string("1");
        let length = interner.literal_string("length");
        // Mirrors `keyof [string, string] & `${number}`` once `keyof` has produced
        // the literal index keys (plus the non-numeric `"length"` and the numeric
        // index intrinsic).
        let keys = interner.union(vec![zero, one, length, TypeId::NUMBER]);
        let result = interner.intersection(vec![keys, numeric_template(&interner)]);
        let expected = interner.union(vec![zero, one]);
        assert_eq!(
            result, expected,
            "numeric index keys survive, \"length\" and the number key drop out"
        );
    }
// TSZ_INLINE_TEST_END e0355a0876d5ee8e76b8846ce37b11c375f7983dae727988bebb831119ccaca5

// TSZ_INLINE_TEST_BEGIN cdc4039e38b379d11e1fe92362bfb423b04fbb7c50acd1afe161cfe9d6d936a5 410 prefix_wildcard_template_keeps_only_matching_literals
    #[test]
    fn prefix_wildcard_template_keeps_only_matching_literals() {
        let interner = TypeInterner::new();
        let foo = interner.literal_string("foo");
        let bar = interner.literal_string("bar");
        // `f${string}`
        let template = interner.template_literal(vec![
            TemplateSpan::Text(interner.intern_string("f")),
            TemplateSpan::Type(TypeId::STRING),
        ]);
        let result = interner.intersection(vec![interner.union(vec![foo, bar]), template]);
        assert_eq!(
            result, foo,
            "`f${{string}}` keeps \"foo\" and drops \"bar\""
        );
    }
// TSZ_INLINE_TEST_END cdc4039e38b379d11e1fe92362bfb423b04fbb7c50acd1afe161cfe9d6d936a5

// TSZ_INLINE_TEST_BEGIN a59197cd634615ba5e8b38ef56222eb5be216386775ca9ea8ac83ab79f3dce09 427 suffix_wildcard_template_keeps_only_matching_literals
    #[test]
    fn suffix_wildcard_template_keeps_only_matching_literals() {
        let interner = TypeInterner::new();
        let ax = interner.literal_string("ax");
        let bx = interner.literal_string("bx");
        let ay = interner.literal_string("ay");
        // `${string}x`
        let template = interner.template_literal(vec![
            TemplateSpan::Type(TypeId::STRING),
            TemplateSpan::Text(interner.intern_string("x")),
        ]);
        let result = interner.intersection(vec![interner.union(vec![ax, bx, ay]), template]);
        assert_eq!(result, interner.union(vec![ax, bx]));
    }
// TSZ_INLINE_TEST_END a59197cd634615ba5e8b38ef56222eb5be216386775ca9ea8ac83ab79f3dce09

// TSZ_INLINE_TEST_BEGIN 947fc2df5fa138ad96adb613e00c7f6224acd121acb919048541697b8ed64941 442 bigint_template_filters_union_literals
    #[test]
    fn bigint_template_filters_union_literals() {
        let interner = TypeInterner::new();
        let one = interner.literal_string("1");
        let x = interner.literal_string("x");
        let template = interner.template_literal(vec![TemplateSpan::Type(TypeId::BIGINT)]);
        let result = interner.intersection(vec![interner.union(vec![one, x]), template]);
        assert_eq!(result, one);
    }
// TSZ_INLINE_TEST_END 947fc2df5fa138ad96adb613e00c7f6224acd121acb919048541697b8ed64941

// TSZ_INLINE_TEST_BEGIN b015d0b3d5566a7a61975b6fe27a85388171f6cb8bda6d1448fc4355d8745365 452 literal_must_satisfy_every_pattern_template
    #[test]
    fn literal_must_satisfy_every_pattern_template() {
        let interner = TypeInterner::new();
        // `"a1"` matches `a${string}` but not `${number}` -> never.
        let a1 = interner.literal_string("a1");
        let a_prefix = interner.template_literal(vec![
            TemplateSpan::Text(interner.intern_string("a")),
            TemplateSpan::Type(TypeId::STRING),
        ]);
        let result = interner.intersection(vec![a1, a_prefix, numeric_template(&interner)]);
        assert_eq!(result, TypeId::NEVER);
    }
// TSZ_INLINE_TEST_END b015d0b3d5566a7a61975b6fe27a85388171f6cb8bda6d1448fc4355d8745365

// TSZ_INLINE_TEST_BEGIN c4c267b438cd3e02cb5d4c7ab1e820c85463d57e6849282a582a7e2e5ca76979 465 number_literal_member_drops_out_of_template_intersection
    #[test]
    fn number_literal_member_drops_out_of_template_intersection() {
        let interner = TypeInterner::new();
        let zero = interner.literal_string("0");
        let two = interner.literal_number(2.0);
        // `(2 | "0") & `${number}`` -> "0" (the number literal cannot inhabit a
        // string-domain template).
        let result = interner.intersection(vec![
            interner.union(vec![two, zero]),
            numeric_template(&interner),
        ]);
        assert_eq!(result, zero);
    }
// TSZ_INLINE_TEST_END c4c267b438cd3e02cb5d4c7ab1e820c85463d57e6849282a582a7e2e5ca76979

// TSZ_INLINE_TEST_BEGIN 9ae0416bb5f572dd3076c2342d9415778279122cfb964b1d144a7533ea771742 479 bare_string_with_template_is_left_for_other_passes
    #[test]
    fn bare_string_with_template_is_left_for_other_passes() {
        let interner = TypeInterner::new();
        // `string & `${number}`` is not modeled by this reduction (the non-literal
        // `string` member is undecidable here); it must be returned unchanged so
        // the surrounding normalization keeps both members.
        let template = numeric_template(&interner);
        let result = interner.intersection(vec![TypeId::STRING, template]);
        assert!(
            matches!(interner.lookup(result), Some(TypeData::Intersection(_))),
            "string & `${{number}}` should stay an intersection"
        );
    }
// TSZ_INLINE_TEST_END 9ae0416bb5f572dd3076c2342d9415778279122cfb964b1d144a7533ea771742

// TSZ_INLINE_TEST_BEGIN e921d3912e649d949ab8eec1ecfcdb52fa87f3aa68fe88a3b60fd3451264c338 493 unrelated_intersections_are_left_untouched
    #[test]
    fn unrelated_intersections_are_left_untouched() {
        let interner = TypeInterner::new();
        // No pattern template present: ordinary string literal intersection is
        // governed by the existing disjoint-literal logic, not this reduction.
        let a = interner.literal_string("a");
        let b = interner.literal_string("b");
        assert_eq!(interner.intersection(vec![a, b]), TypeId::NEVER);
        // A single string literal with no template is returned unchanged.
        assert_eq!(interner.intersection(vec![a]), a);
    }
// TSZ_INLINE_TEST_END e921d3912e649d949ab8eec1ecfcdb52fa87f3aa68fe88a3b60fd3451264c338

// TSZ_INLINE_TEST_BEGIN e7528d98030bc7f3ef7000323fb0a7feb6895a8c99aead6876bd6b610f9c1567 505 template_filters_intersection_of_two_key_unions
    #[test]
    fn template_filters_intersection_of_two_key_unions() {
        let interner = TypeInterner::new();
        let zero = interner.literal_string("0");
        let one = interner.literal_string("1");
        let two = interner.literal_string("2");
        let length = interner.literal_string("length");
        // Mirrors `keyof [a,b,c] & keyof [a,b] & `${number}`` — two large key
        // unions intersected with the numeric pattern. The size-gated union
        // distribution skips unions this wide, so the reduction must filter the
        // multi-member intersection directly. Only the numeric keys common to both
        // unions survive: `"0" | "1"`.
        let keys_abc = interner.union(vec![zero, one, two, length, TypeId::NUMBER]);
        let keys_ab = interner.union(vec![zero, one, length, TypeId::NUMBER]);
        let result = interner.intersection(vec![keys_abc, keys_ab, numeric_template(&interner)]);
        assert_eq!(result, interner.union(vec![zero, one]));
    }
// TSZ_INLINE_TEST_END e7528d98030bc7f3ef7000323fb0a7feb6895a8c99aead6876bd6b610f9c1567

// TSZ_INLINE_TEST_BEGIN b3bcd3d52489f1a9115d0824e16cbe50612f06d4ef19ea5163af29afc8bace8e 523 template_filters_three_way_key_union_intersection
    #[test]
    fn template_filters_three_way_key_union_intersection() {
        let interner = TypeInterner::new();
        let zero = interner.literal_string("0");
        let one = interner.literal_string("1");
        let two = interner.literal_string("2");
        let three = interner.literal_string("3");
        let length = interner.literal_string("length");
        let a = interner.union(vec![zero, one, two, three, length, TypeId::NUMBER]);
        let b = interner.union(vec![zero, one, two, length, TypeId::NUMBER]);
        let c = interner.union(vec![zero, one, length, TypeId::NUMBER]);
        let result = interner.intersection(vec![a, b, c, numeric_template(&interner)]);
        assert_eq!(result, interner.union(vec![zero, one]));
    }
// TSZ_INLINE_TEST_END b3bcd3d52489f1a9115d0824e16cbe50612f06d4ef19ea5163af29afc8bace8e

// TSZ_INLINE_TEST_BEGIN 6eedf7952e82c340515db92d123d4543c13ae76227a67530b2eb9da002921571 538 multi_member_intersection_with_no_common_numeric_key_is_never
    #[test]
    fn multi_member_intersection_with_no_common_numeric_key_is_never() {
        let interner = TypeInterner::new();
        let zero = interner.literal_string("0");
        let one = interner.literal_string("1");
        let two = interner.literal_string("2");
        // `("0" | "1") & ("2") & `${number}`` — the literal sets are disjoint, so
        // the numeric filter leaves nothing.
        let result = interner.intersection(vec![
            interner.union(vec![zero, one]),
            two,
            numeric_template(&interner),
        ]);
        assert_eq!(result, TypeId::NEVER);
    }
// TSZ_INLINE_TEST_END 6eedf7952e82c340515db92d123d4543c13ae76227a67530b2eb9da002921571

// TSZ_INLINE_TEST_BEGIN 22fb99b4b5f79e979b5523ad6614d5f38c7f5d67936422cc3f8b8b72654e2364 554 prefix_template_filters_multi_member_intersection
    #[test]
    fn prefix_template_filters_multi_member_intersection() {
        let interner = TypeInterner::new();
        let a1 = interner.literal_string("a-1");
        let a2 = interner.literal_string("a-2");
        let b1 = interner.literal_string("b-1");
        // `("a-1" | "a-2" | "b-1") & ("a-1" | "b-1" | "a-2") & `a-${number}``
        let template = interner.template_literal(vec![
            TemplateSpan::Text(interner.intern_string("a-")),
            TemplateSpan::Type(TypeId::NUMBER),
        ]);
        let result = interner.intersection(vec![
            interner.union(vec![a1, a2, b1]),
            interner.union(vec![a1, b1, a2]),
            template,
        ]);
        // `b-1` fails the `a-${number}` pattern; `a-1` and `a-2` survive in both.
        assert_eq!(result, interner.union(vec![a1, a2]));
    }
// TSZ_INLINE_TEST_END 22fb99b4b5f79e979b5523ad6614d5f38c7f5d67936422cc3f8b8b72654e2364

// TSZ_INLINE_TEST_BEGIN 954edfe810e5a7b7489bfebebcb8272ddcbde639560f05cc5ae598002b14182b 574 infinite_member_without_finite_bound_is_left_for_other_passes
    #[test]
    fn infinite_member_without_finite_bound_is_left_for_other_passes() {
        let interner = TypeInterner::new();
        // `(string | "0") & `${number}`` has an infinite value set (`${number}`),
        // so there is no finite member to bound enumeration: the reduction must
        // bail rather than collapse to the enumerated literal `"0"`.
        let zero = interner.literal_string("0");
        let su = interner.union(vec![TypeId::STRING, zero]);
        let result = interner.intersection(vec![su, numeric_template(&interner)]);
        // Whatever the other passes choose, it must not be the over-narrow `"0"`.
        assert_ne!(
            result, zero,
            "must not collapse `(string | \"0\") & `${{number}}`` to \"0\""
        );
    }
// TSZ_INLINE_TEST_END 954edfe810e5a7b7489bfebebcb8272ddcbde639560f05cc5ae598002b14182b

// TSZ_INLINE_TEST_BEGIN 760c4c044c33eb17e773ff4d15c1db1e29903c65f2b8fd7d484cdc45863891ff 590 finite_bound_lets_string_member_act_as_universal_filter
    #[test]
    fn finite_bound_lets_string_member_act_as_universal_filter() {
        let interner = TypeInterner::new();
        // `("0" | "1") & string & `${number}`` — the finite `"0" | "1"` bounds the
        // result and the universal `string` member keeps every candidate; the
        // numeric pattern is already satisfied. Result: `"0" | "1"`.
        let zero = interner.literal_string("0");
        let one = interner.literal_string("1");
        let result = interner.intersection(vec![
            interner.union(vec![zero, one]),
            TypeId::STRING,
            numeric_template(&interner),
        ]);
        assert_eq!(result, interner.union(vec![zero, one]));
    }
// TSZ_INLINE_TEST_END 760c4c044c33eb17e773ff4d15c1db1e29903c65f2b8fd7d484cdc45863891ff

// TSZ_INLINE_TEST_BEGIN 5f56687ab2b95ba126377770020c6ab7d1a7a8e92a07274c339036f1c34b83ed 606 undecidable_object_member_bails_without_collapsing
    #[test]
    fn undecidable_object_member_bails_without_collapsing() {
        let interner = TypeInterner::new();
        // An object member is not a string filter; the reduction must bail (return
        // `None`) and leave the intersection for the structural passes rather than
        // dropping the object or the literals.
        let zero = interner.literal_string("0");
        let reduced = interner.reduce_pattern_template_intersection(&[
            zero,
            TypeId::OBJECT,
            numeric_template(&interner),
        ]);
        assert!(
            reduced.is_none(),
            "object member must bail, leaving the intersection to other passes"
        );
    }
// TSZ_INLINE_TEST_END 5f56687ab2b95ba126377770020c6ab7d1a7a8e92a07274c339036f1c34b83ed
