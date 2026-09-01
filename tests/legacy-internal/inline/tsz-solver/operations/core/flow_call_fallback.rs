//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/operations/core/flow_call_fallback.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN b3c9dda0868aa6a09680e013ea1fdb7e2ac97af213fd83f96d4119b7a7407712 221 resolves_one_generic_signature_through_an_intersection_wrapper
    #[test]
    fn resolves_one_generic_signature_through_an_intersection_wrapper() {
        let db = TypeInterner::new();
        let callable = generic_identity(&db, "T");
        let wrapped = db.intersection2(callable, db.object(Vec::new()));
        let result = resolve_single_non_rest_generic_call_with_compat_checker(
            &db,
            &TypeEnvironment::new(),
            wrapped,
            &[TypeId::STRING],
            |_| {},
        );
        assert_eq!(result, Some(TypeId::STRING));
    }
// TSZ_INLINE_TEST_END b3c9dda0868aa6a09680e013ea1fdb7e2ac97af213fd83f96d4119b7a7407712

// TSZ_INLINE_TEST_BEGIN 70f3fe54520fb4e3f967c922094e3efcc61457b3aab58ca21ef3e4ad208a20ab 236 rejects_an_intersection_with_two_generic_call_signatures
    #[test]
    fn rejects_an_intersection_with_two_generic_call_signatures() {
        let db = TypeInterner::new();
        let overloaded = db.intersection2(generic_identity(&db, "T"), generic_identity(&db, "U"));
        let result = resolve_single_non_rest_generic_call_with_compat_checker(
            &db,
            &TypeEnvironment::new(),
            overloaded,
            &[TypeId::STRING],
            |_| {},
        );
        assert_eq!(result, None);
    }
// TSZ_INLINE_TEST_END 70f3fe54520fb4e3f967c922094e3efcc61457b3aab58ca21ef3e4ad208a20ab

// TSZ_INLINE_TEST_BEGIN a37cc6fff36a6a1e8f28b7e720e33b0a732c352978bc3081988f171722447579 250 ignores_a_construct_only_member_in_an_intersection_wrapper
    #[test]
    fn ignores_a_construct_only_member_in_an_intersection_wrapper() {
        let db = TypeInterner::new();
        let callable = generic_identity(&db, "T");
        let constructor = db.function(FunctionShape {
            type_params: Vec::new(),
            params: Vec::new(),
            this_type: None,
            return_type: TypeId::UNKNOWN,
            type_predicate: None,
            is_constructor: true,
            is_method: false,
        });
        let wrapped = db.intersection2(callable, constructor);
        let result = resolve_single_non_rest_generic_call_with_compat_checker(
            &db,
            &TypeEnvironment::new(),
            wrapped,
            &[TypeId::STRING],
            |_| {},
        );
        assert_eq!(result, Some(TypeId::STRING));
    }
// TSZ_INLINE_TEST_END a37cc6fff36a6a1e8f28b7e720e33b0a732c352978bc3081988f171722447579

// TSZ_INLINE_TEST_BEGIN 8aae56ef52f98f3aed4224a64673d3eaf00d4bf39ea1e542575e885d2ec729cb 274 rejects_an_intersection_with_an_unresolved_member
    #[test]
    fn rejects_an_intersection_with_an_unresolved_member() {
        let db = TypeInterner::new();
        let wrapped = db.intersection2(generic_identity(&db, "T"), db.lazy(DefId(17)));
        let result = resolve_single_non_rest_generic_call_with_compat_checker(
            &db,
            &TypeEnvironment::new(),
            wrapped,
            &[TypeId::STRING],
            |_| {},
        );
        assert_eq!(result, None);
    }
// TSZ_INLINE_TEST_END 8aae56ef52f98f3aed4224a64673d3eaf00d4bf39ea1e542575e885d2ec729cb

// TSZ_INLINE_TEST_BEGIN c5c676b60da9e5203f4a3e0f63ef58b727cab621ecd06e4a33278e8a012460bd 288 accepts_an_acyclic_constraint_chain_at_the_walk_cap
    #[test]
    fn accepts_an_acyclic_constraint_chain_at_the_walk_cap() {
        let db = TypeInterner::new();
        let callable = generic_identity(&db, "Value");
        let wrapped = constrained_chain(&db, callable, MAX_SINGLE_GENERIC_CALL_WALK_STEPS - 1);
        let result = resolve_single_non_rest_generic_call_with_compat_checker(
            &db,
            &TypeEnvironment::new(),
            wrapped,
            &[TypeId::STRING],
            |_| {},
        );
        assert_eq!(result, Some(TypeId::STRING));
    }
// TSZ_INLINE_TEST_END c5c676b60da9e5203f4a3e0f63ef58b727cab621ecd06e4a33278e8a012460bd

// TSZ_INLINE_TEST_BEGIN 993c978c6f994dc923195c5e62319712d7abc82905f3e33abe8677ba4ffa90cf 303 rejects_an_acyclic_constraint_chain_over_the_walk_cap
    #[test]
    fn rejects_an_acyclic_constraint_chain_over_the_walk_cap() {
        let db = TypeInterner::new();
        let callable = generic_identity(&db, "Element");
        let wrapped = constrained_chain(&db, callable, MAX_SINGLE_GENERIC_CALL_WALK_STEPS);
        let result = resolve_single_non_rest_generic_call_with_compat_checker(
            &db,
            &TypeEnvironment::new(),
            wrapped,
            &[TypeId::STRING],
            |_| {},
        );
        assert_eq!(result, None);
    }
// TSZ_INLINE_TEST_END 993c978c6f994dc923195c5e62319712d7abc82905f3e33abe8677ba4ffa90cf
