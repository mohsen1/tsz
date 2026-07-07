fn named_lazy_conditional(
    interner: &TypeInterner,
    def_id: DefId,
    true_type: TypeId,
) -> TypeId {
    let lazy = interner.lazy(def_id);
    interner.conditional(ConditionalType {
        check_type: lazy,
        extends_type: lazy,
        true_type,
        false_type: TypeId::NEVER,
        is_distributive: false,
    })
}

fn concrete_true_conditional(interner: &TypeInterner, true_type: TypeId) -> TypeId {
    interner.conditional(ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type,
        false_type: TypeId::NEVER,
        is_distributive: false,
    })
}

fn object_prop(interner: &TypeInterner, name: &str, ty: TypeId) -> TypeId {
    interner.object(vec![PropertyInfo::new(interner.intern_string(name), ty)])
}

#[test]
fn conditional_tail_identity_defers_fifth_named_root() {
    let interner = TypeInterner::new();
    let root = DefId(143_510);
    let object = object_prop(&interner, "value", TypeId::STRING);
    let mut branch = object;
    for _ in 0..5 {
        branch = named_lazy_conditional(&interner, root, branch);
    }

    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate(branch);

    assert_ne!(result, TypeId::ERROR);
    assert!(
        matches!(interner.lookup(result), Some(TypeData::Conditional(_))),
        "fifth same-root tail conditional should stay deferred, got {:?}",
        interner.lookup(result)
    );
    assert!(
        evaluator.has_incomplete_request_verdict(),
        "conditional recursion-identity bailout must taint the request"
    );
}

#[test]
fn conditional_tail_identity_allows_fourth_named_root() {
    let interner = TypeInterner::new();
    let root = DefId(143_511);
    let object = object_prop(&interner, "value", TypeId::STRING);
    let mut branch = object;
    for _ in 0..4 {
        branch = named_lazy_conditional(&interner, root, branch);
    }

    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate(branch);

    assert_eq!(result, object);
    assert!(
        !evaluator.has_incomplete_request_verdict(),
        "four same-root tail conditional reductions are below the cutoff"
    );
}

#[test]
fn conditional_recurse_helper_defers_seeded_fifth_root() {
    let interner = TypeInterner::new();
    let object = object_prop(&interner, "value", TypeId::STRING);
    let conditional = concrete_true_conditional(&interner, object);
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(conditional, 4);
    let result = evaluator.recurse_conditional(conditional);

    assert_eq!(result, conditional);
    assert!(
        matches!(interner.lookup(result), Some(TypeData::Conditional(_))),
        "seeded fifth conditional re-reduce should preserve the deferred root"
    );
    assert!(
        evaluator.has_incomplete_request_verdict(),
        "seeded conditional bailout must mark the request partial"
    );
}

#[test]
fn conditional_recurse_helper_allows_seeded_fourth_root_and_pops() {
    let interner = TypeInterner::new();
    let object = object_prop(&interner, "value", TypeId::STRING);
    let conditional = concrete_true_conditional(&interner, object);
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(conditional, 3);
    assert_eq!(evaluator.recurse_conditional(conditional), object);
    assert_eq!(evaluator.recurse_conditional(conditional), object);
    assert!(
        !evaluator.has_incomplete_request_verdict(),
        "below-cutoff conditional helper calls must pop their stack entry"
    );
}

#[test]
fn direct_keyof_defers_seeded_fifth_identity() {
    let interner = TypeInterner::new();
    let object = object_prop(&interner, "value", TypeId::STRING);
    let expected = interner.keyof(object);
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(object, 4);
    let result = evaluator.evaluate_keyof(object);

    assert_eq!(result, expected);
    assert!(
        matches!(interner.lookup(result), Some(TypeData::KeyOf(inner)) if inner == object),
        "seeded fifth direct keyof should preserve the deferred root"
    );
    assert!(
        evaluator.has_incomplete_request_verdict(),
        "seeded direct keyof bailout must mark the request partial"
    );
}

#[test]
fn direct_keyof_allows_seeded_fourth_identity() {
    let interner = TypeInterner::new();
    let object = object_prop(&interner, "value", TypeId::STRING);
    let deferred = interner.keyof(object);
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(object, 3);
    let result = evaluator.evaluate_keyof(object);

    assert_ne!(result, deferred);
    assert!(
        !evaluator.has_incomplete_request_verdict(),
        "below-cutoff direct keyof must reduce and pop its stack entry"
    );
}

#[test]
fn recurse_keyof_allows_seeded_fourth_without_double_counting() {
    let interner = TypeInterner::new();
    let object = object_prop(&interner, "value", TypeId::STRING);
    let deferred = interner.keyof(object);
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(object, 3);
    let result = evaluator.recurse_keyof(object);

    assert_ne!(result, deferred);
    assert!(
        !evaluator.has_incomplete_request_verdict(),
        "helper-origin keyof evaluation must enter exactly one identity frame"
    );
}

#[test]
fn direct_index_access_defers_seeded_fifth_identity() {
    let interner = TypeInterner::new();
    let object = object_prop(&interner, "value", TypeId::STRING);
    let key = interner.literal_string("value");
    let expected = interner.index_access(object, key);
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(expected, 4);
    let result = evaluator.evaluate_index_access(object, key);

    assert_eq!(result, expected);
    assert!(
        matches!(interner.lookup(result), Some(TypeData::IndexAccess(obj, idx)) if obj == object && idx == key),
        "seeded fifth direct indexed access should preserve the deferred root"
    );
    assert!(
        evaluator.has_incomplete_request_verdict(),
        "seeded direct indexed-access bailout must mark the request partial"
    );
}

#[test]
fn direct_index_access_allows_seeded_fourth_identity() {
    let interner = TypeInterner::new();
    let object = object_prop(&interner, "value", TypeId::STRING);
    let key = interner.literal_string("value");
    let deferred = interner.index_access(object, key);
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(deferred, 3);
    let result = evaluator.evaluate_index_access(object, key);

    assert_eq!(result, TypeId::STRING);
    assert!(
        !evaluator.has_incomplete_request_verdict(),
        "below-cutoff direct indexed access must reduce and pop its stack entry"
    );
}

#[test]
fn recurse_index_access_allows_seeded_fourth_without_double_counting() {
    let interner = TypeInterner::new();
    let object = object_prop(&interner, "value", TypeId::STRING);
    let key = interner.literal_string("value");
    let deferred = interner.index_access(object, key);
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(deferred, 3);
    let result = evaluator.recurse_index_access(object, key);

    assert_eq!(result, TypeId::STRING);
    assert!(
        !evaluator.has_incomplete_request_verdict(),
        "helper-origin indexed access must enter exactly one identity frame"
    );
}

#[test]
fn conditional_recurse_helper_reset_clears_seeded_bailout_state() {
    let interner = TypeInterner::new();
    let object = object_prop(&interner, "value", TypeId::STRING);
    let conditional = concrete_true_conditional(&interner, object);
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(conditional, 4);
    assert_eq!(evaluator.recurse_conditional(conditional), conditional);
    assert!(evaluator.has_incomplete_request_verdict());

    evaluator.reset();
    assert_eq!(evaluator.recurse_conditional(conditional), object);
    assert!(
        !evaluator.has_incomplete_request_verdict(),
        "reset must clear both the meta identity stack and request verdict"
    );
}

#[test]
fn conditional_identity_counts_canonical_def_roots() {
    use crate::construction::TypeDatabase;
    use crate::relations::subtype::TypeResolver;

    struct CanonicalResolver {
        primary: DefId,
        alias: DefId,
    }

    impl TypeResolver for CanonicalResolver {
        fn resolve_ref(
            &self,
            _symbol: SymbolRef,
            _interner: &dyn TypeDatabase,
        ) -> Option<TypeId> {
            None
        }

        fn canonical_def_id(&self, def_id: DefId) -> DefId {
            if def_id == self.alias {
                self.primary
            } else {
                def_id
            }
        }

        fn defs_are_equivalent(&self, left: DefId, right: DefId) -> bool {
            self.canonical_def_id(left) == self.canonical_def_id(right)
        }
    }

    let interner = TypeInterner::new();
    let primary = DefId(143_512);
    let alias = DefId(143_513);
    let object = object_prop(&interner, "value", TypeId::STRING);
    let primary_conditional = named_lazy_conditional(&interner, primary, object);
    let alias_conditional = named_lazy_conditional(&interner, alias, object);
    let resolver = CanonicalResolver { primary, alias };
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &resolver);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(alias_conditional, 4);
    let result = evaluator.recurse_conditional(primary_conditional);

    assert_eq!(result, primary_conditional);
    assert!(
        evaluator.has_incomplete_request_verdict(),
        "canonical/equivalent DefIds must count as one conditional recursion root"
    );
}

#[test]
fn conditional_identity_keeps_distinct_roots_separate() {
    let interner = TypeInterner::new();
    let object = object_prop(&interner, "value", TypeId::STRING);
    let seeded = concrete_true_conditional(&interner, object);
    let distinct = named_lazy_conditional(&interner, DefId(143_514), object);
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(seeded, 4);
    let result = evaluator.recurse_conditional(distinct);

    assert_eq!(result, object);
    assert!(
        !evaluator.has_incomplete_request_verdict(),
        "allocation-distinct conditionals without a shared named root must not collide"
    );
}
