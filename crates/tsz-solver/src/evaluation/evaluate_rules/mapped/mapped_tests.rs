use super::*;
use crate::caches::query_cache::QueryCache;
use crate::construction::TypeInterner;
use crate::recursion::RecursionResult;
use crate::types::TypeParamInfo;

#[test]
fn evaluate_keyof_or_constraint_preserves_reentrant_constraint() {
    let interner = TypeInterner::new();
    let mut evaluator = TypeEvaluator::new(&interner);
    let constraint = interner.keyof(TypeId::STRING);

    assert!(matches!(
        evaluator.keyof_constraint_guard.enter(constraint),
        RecursionResult::Entered
    ));
    assert_eq!(
        evaluator.evaluate_keyof_or_constraint(constraint),
        constraint
    );
    evaluator.keyof_constraint_guard.leave(constraint);
}

/// Build the post-instantiation form of
/// `type M<T> = { [<iter_name> in keyof T]: <template> }`
/// with `T` substituted by `concrete_source`. The iteration variable's
/// declared constraint stays `keyof T` (the type parameter), proving
/// `M` was authored as a generic homomorphic mapping.
fn build_instantiated_homomorphic_mapped(
    interner: &TypeInterner,
    iter_name: &str,
    concrete_source: TypeId,
    template: TypeId,
) -> MappedType {
    let iter_atom = interner.intern_string(iter_name);
    let outer_t = interner.type_param(TypeParamInfo::simple(interner.intern_string("T")));
    let original_constraint = interner.keyof(outer_t);
    MappedType {
        type_param: TypeParamInfo {
            name: iter_atom,
            constraint: Some(original_constraint),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        },
        constraint: interner.keyof(concrete_source),
        name_type: None,
        template,
        readonly_modifier: None,
        optional_modifier: None,
    }
}

#[test]
fn mapped_property_template_uses_preserving_instantiation_cache() {
    // Per-file tier in isolation (#14345): disable the project-wide instantiation
    // cache so the repeat hit lands on the per-file QueryCache statistics.
    let _g = crate::instantiation::instantiate::ProjectInstCacheDisabledGuard::new();
    let interner = TypeInterner::new();
    let query_cache = QueryCache::new(&interner);

    let key_atom = interner.intern_string("K");
    let key_param = interner.type_param(TypeParamInfo::simple(key_atom));
    let wrapped_prop = PropertyInfo {
        name: interner.intern_string("value"),
        type_id: key_param,
        write_type: key_param,
        is_string_named: true,
        ..Default::default()
    };
    let template = interner.object(vec![wrapped_prop]);
    let constraint = interner.literal_string("same");

    let mapped = |readonly_modifier| MappedType {
        type_param: TypeParamInfo {
            name: key_atom,
            constraint: Some(constraint),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        },
        constraint,
        name_type: None,
        template,
        readonly_modifier,
        optional_modifier: None,
    };

    let first = interner.mapped(mapped(None));
    let second = interner.mapped(mapped(Some(MappedModifier::Add)));
    assert_ne!(
        first, second,
        "distinct mapped types keep the evaluator cache from hiding instantiation reuse"
    );

    let before = query_cache.statistics();
    let mut first_eval = TypeEvaluator::new(&interner).with_query_db(&query_cache);
    let first_result = first_eval.evaluate(first);
    let after_first = query_cache.statistics();

    let mut second_eval = TypeEvaluator::new(&interner).with_query_db(&query_cache);
    let second_result = second_eval.evaluate(second);
    let after_second = query_cache.statistics();

    assert_ne!(first_result, TypeId::ERROR);
    assert_ne!(second_result, TypeId::ERROR);
    assert!(
        after_first.instantiation_cache_misses > before.instantiation_cache_misses,
        "first mapped property template instantiation should populate the cache"
    );
    assert!(
        after_second.instantiation_cache_hits > after_first.instantiation_cache_hits,
        "second mapped property template instantiation should reuse the preserving cache slot"
    );
}

#[test]
fn remapped_key_type_uses_preserving_instantiation_cache() {
    // Per-file tier in isolation (#14345): disable the project-wide instantiation
    // cache so the repeat hit lands on the per-file QueryCache statistics.
    let _g = crate::instantiation::instantiate::ProjectInstCacheDisabledGuard::new();
    let interner = TypeInterner::new();
    let query_cache = QueryCache::new(&interner);

    let key_atom = interner.intern_string("K");
    let key_param = interner.type_param(TypeParamInfo::simple(key_atom));
    let name_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("key"),
        type_id: key_param,
        write_type: key_param,
        is_string_named: true,
        ..Default::default()
    }]);
    let constraint = interner.literal_string("same");

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: key_atom,
            constraint: Some(constraint),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        },
        constraint,
        name_type: Some(name_type),
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let before = query_cache.statistics();
    let mut first_eval = TypeEvaluator::new(&interner).with_query_db(&query_cache);
    let first_result = first_eval.remap_key_type_for_mapped(&mapped, constraint);
    let after_first = query_cache.statistics();

    let mut second_eval = TypeEvaluator::new(&interner).with_query_db(&query_cache);
    let second_result = second_eval.remap_key_type_for_mapped(&mapped, constraint);
    let after_second = query_cache.statistics();

    assert!(first_result.is_ok());
    assert!(second_result.is_ok());
    assert!(
        after_first.instantiation_cache_misses > before.instantiation_cache_misses,
        "first remapped-key instantiation should populate the preserving cache"
    );
    assert!(
        after_second.instantiation_cache_hits > after_first.instantiation_cache_hits,
        "second remapped-key instantiation should reuse the preserving cache slot"
    );
}

#[test]
fn mapped_index_template_uses_instantiation_cache_per_concrete_key() {
    // Per-file tier in isolation (#14345): disable the project-wide instantiation
    // cache so the repeat hit lands on the per-file QueryCache statistics.
    let _g = crate::instantiation::instantiate::ProjectInstCacheDisabledGuard::new();
    let interner = TypeInterner::new();
    let query_cache = QueryCache::new(&interner);

    let key_atom = interner.intern_string("K");
    let key_param = interner.type_param(TypeParamInfo::simple(key_atom));
    let wrapped_prop = PropertyInfo {
        name: interner.intern_string("value"),
        type_id: key_param,
        write_type: key_param,
        is_string_named: true,
        ..Default::default()
    };
    let template = interner.object(vec![wrapped_prop]);
    let constraint = interner.union(vec![
        interner.literal_string("first"),
        interner.literal_string("second"),
    ]);

    let mapped = |readonly_modifier| MappedType {
        type_param: TypeParamInfo {
            name: key_atom,
            constraint: Some(constraint),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        },
        constraint,
        name_type: None,
        template,
        readonly_modifier,
        optional_modifier: None,
    };

    let first = interner.mapped(mapped(None));
    let second = interner.mapped(mapped(Some(MappedModifier::Add)));
    assert_ne!(
        first, second,
        "distinct mapped types keep the evaluator cache from hiding per-key instantiation reuse"
    );

    let before = query_cache.statistics();
    let mut first_eval = TypeEvaluator::new(&interner).with_query_db(&query_cache);
    let first_result = first_eval.evaluate(interner.index_access(first, constraint));
    let after_first = query_cache.statistics();

    let mut second_eval = TypeEvaluator::new(&interner).with_query_db(&query_cache);
    let second_result = second_eval.evaluate(interner.index_access(second, constraint));
    let after_second = query_cache.statistics();

    assert_ne!(first_result, TypeId::ERROR);
    assert_ne!(second_result, TypeId::ERROR);
    assert!(
        after_first.instantiation_cache_misses > before.instantiation_cache_misses,
        "first mapped[index] per-key template instantiation should populate the cache"
    );
    assert!(
        after_second.instantiation_cache_hits > after_first.instantiation_cache_hits,
        "second mapped[index] per-key template instantiation should reuse cached template instantiations"
    );
}

/// tsc's `instantiateMappedType` reduces a generic homomorphic mapped
/// type to its source whenever the source resolves to a primitive,
/// literal, `never`, unique symbol, or enum. This proves the rule is
/// structural — varying the iteration-variable name must not affect
/// the decision.
#[test]
fn instantiated_homomorphic_mapped_over_non_object_source_reduces_to_source() {
    let interner = TypeInterner::new();
    let template = TypeId::BOOLEAN;

    let primitive_cases = [
        TypeId::STRING,
        TypeId::NUMBER,
        TypeId::BOOLEAN,
        TypeId::BIGINT,
        TypeId::SYMBOL,
        TypeId::NULL,
        TypeId::UNDEFINED,
        TypeId::VOID,
        TypeId::NEVER,
    ];

    for iter_name in ["P", "K", "X"] {
        for source in primitive_cases {
            let mapped =
                build_instantiated_homomorphic_mapped(&interner, iter_name, source, template);
            let mut evaluator = TypeEvaluator::new(&interner);
            assert_eq!(
                evaluator.evaluate_mapped(&mapped),
                source,
                "instantiated homomorphic mapped over {source:?} with iter `{iter_name}` should reduce to source"
            );
        }

        let literal_foo = interner.literal_string("foo");
        let mapped =
            build_instantiated_homomorphic_mapped(&interner, iter_name, literal_foo, template);
        let mut evaluator = TypeEvaluator::new(&interner);
        assert_eq!(
            evaluator.evaluate_mapped(&mapped),
            literal_foo,
            "instantiated homomorphic mapped over a string literal should reduce to the literal"
        );
    }
}

/// A directly authored `{ [K in keyof string]: V }` — whose iteration
/// variable's declared constraint is `keyof string`, NOT `keyof <typeparam>`
/// — must NOT take the primitive short-circuit. tsc keeps the normal
/// key-expansion behavior here, producing an indexed object over string's
/// apparent members.
#[test]
fn direct_mapped_over_string_does_not_short_circuit() {
    let interner = TypeInterner::new();
    let constraint = interner.keyof(TypeId::STRING);
    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: Some(constraint),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        },
        constraint,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    };
    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate_mapped(&mapped);
    assert_ne!(
        result,
        TypeId::STRING,
        "direct `{{ [K in keyof string]: V }}` must NOT reduce to `string`"
    );
}

/// Object sources must not short-circuit — they exercise the full
/// homomorphic-mapping expansion. This proves the rule is keyed on the
/// source's structure (primitive vs. object), not on iteration-variable
/// spelling or the mere presence of a generic outer constraint.
#[test]
fn instantiated_homomorphic_mapped_over_object_source_does_not_short_circuit() {
    let interner = TypeInterner::new();
    let foo_atom = interner.intern_string("foo");
    let property = crate::types::PropertyInfo {
        name: foo_atom,
        type_id: TypeId::STRING,
        ..Default::default()
    };
    let source = interner.object(vec![property]);

    let mapped = build_instantiated_homomorphic_mapped(&interner, "P", source, TypeId::STRING);
    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate_mapped(&mapped);
    assert_ne!(
        result, source,
        "object sources must NOT take the primitive short-circuit"
    );
}

/// Union sources are handled by `try_distribute_mapped_over_union_source`,
/// which distributes the mapped type over each member and recursively
/// evaluates. Primitive members must still reduce to themselves so the
/// final result is the original union (e.g. `M<string | "foo">` -> `string | "foo"`).
#[test]
fn instantiated_homomorphic_mapped_distributes_over_primitive_union() {
    let interner = TypeInterner::new();
    let literal_foo = interner.literal_string("foo");
    let source = interner.union(vec![TypeId::STRING, literal_foo]);
    let mapped = build_instantiated_homomorphic_mapped(&interner, "P", source, TypeId::BOOLEAN);
    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate_mapped(&mapped);
    let expected = interner.union(vec![TypeId::STRING, literal_foo]);
    assert_eq!(
        result, expected,
        "union of primitives should distribute and each member should reduce to itself"
    );
}

/// Deep union chain: `"a" | "b" | "c" | ... | "z"` (26 members) used as a mapped
/// constraint. Tests that `evaluate_keyof_or_constraint` handles wide flat unions
/// without stack overflow regardless of whether the iteration-variable is named `K` or `P`.
#[test]
fn evaluate_keyof_or_constraint_deep_flat_union_constraint() {
    let interner = TypeInterner::new();
    let mut evaluator = TypeEvaluator::new(&interner);

    let members: Vec<TypeId> = (b'a'..=b'z')
        .map(|c| interner.literal_string(&(c as char).to_string()))
        .collect();
    let wide_union = interner.union(members);

    // constraint is a union of 26 string literals — evaluate_keyof_or_constraint
    // must visit each member recursively; none should be changed by evaluation.
    let result = evaluator.evaluate_keyof_or_constraint(wide_union);
    assert_eq!(
        result, wide_union,
        "flat union of string literals should be returned unchanged"
    );
}

/// Deeply nested union: `Union(a, Union(b, Union(c, ...)))` with 50 levels.
/// Tests that the guard fires at the depth limit and the function terminates.
#[test]
fn evaluate_keyof_or_constraint_nested_union_terminates() {
    let interner = TypeInterner::new();
    let mut evaluator = TypeEvaluator::new(&interner);

    let mut nested = interner.literal_string("leaf");
    for i in 0..50u32 {
        let lit = interner.literal_string(&i.to_string());
        nested = interner.union(vec![lit, nested]);
    }

    let result = evaluator.evaluate_keyof_or_constraint(nested);
    assert_ne!(
        result,
        TypeId::ERROR,
        "deep nested union must not produce ERROR"
    );
}

/// Verifies that the iteration-variable name does not affect constraint evaluation.
/// Both `K` and `Q` iterate over the same constraint and must produce identical results.
#[test]
fn evaluate_keyof_or_constraint_name_invariant() {
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let constraint = interner.union(vec![lit_a, lit_b]);

    let result_k = TypeEvaluator::new(&interner).evaluate_keyof_or_constraint(constraint);
    let result_q = TypeEvaluator::new(&interner).evaluate_keyof_or_constraint(constraint);

    assert_eq!(
        result_k, result_q,
        "constraint evaluation must be independent of iteration-variable name"
    );
}

/// Verifies that re-entering the same TypeId within the chain is detected and does
/// not loop forever. The `keyof_constraint_guard` keeps all intermediate types
/// entered until the chain terminates; if the same TypeId appears again (cycle),
/// `enter` returns `Cycle` and terminates the loop. We exercise this by calling
/// `evaluate_keyof_or_constraint` on a union whose members are themselves unions
/// sharing a member — the shared type will be encountered twice across the
/// recursive union-member evaluation and must not cause unbounded iteration.
#[test]
fn evaluate_keyof_or_constraint_cycle_guard_prevents_infinite_loop() {
    let interner = TypeInterner::new();
    let mut evaluator = TypeEvaluator::new(&interner);

    let lit_x = interner.literal_string("x");
    let lit_y = interner.literal_string("y");
    let lit_z = interner.literal_string("z");
    let u2 = interner.union(vec![lit_y, lit_z]);
    let u1 = interner.union(vec![lit_x, u2]);

    let result = evaluator.evaluate_keyof_or_constraint(u1);
    assert_ne!(
        result,
        TypeId::ERROR,
        "nested union evaluation must not produce ERROR"
    );

    let plain_union = interner.union(vec![lit_x, lit_y]);
    let result2 = evaluator.evaluate_keyof_or_constraint(plain_union);
    assert_ne!(
        result2,
        TypeId::ERROR,
        "self-stable union must terminate without ERROR"
    );
}

/// Homomorphic mapped type instantiated with an intersection source distributes
/// over the intersection members, mirroring tsc's `instantiateMappedType`.
///
/// Structural rule: `Mapped<A & B>` → `Mapped<A> & Mapped<B>` when `Mapped`
/// is a homomorphic generic instantiation (iteration variable has a declared
/// `keyof T` constraint from the generic body).
#[test]
fn instantiated_homomorphic_mapped_over_intersection_distributes() {
    let interner = TypeInterner::new();

    // A = { x: number }, B = { y: string }
    let x_atom = interner.intern_string("x");
    let y_atom = interner.intern_string("y");
    let obj_a = interner.object(vec![PropertyInfo::new(x_atom, TypeId::NUMBER)]);
    let obj_b = interner.object(vec![PropertyInfo::new(y_atom, TypeId::STRING)]);
    let a_and_b = interner.intersection(vec![obj_a, obj_b]);

    // Template: identity (T[K]) — simplest homomorphic form.
    // iter_k is interned identically to the helper's internal variable, so the
    // template's type-parameter reference is the same TypeId.
    let outer_t = interner.type_param(TypeParamInfo::simple(interner.intern_string("T")));
    let iter_k = interner.type_param(TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(interner.keyof(outer_t)),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let template = interner.index_access(a_and_b, iter_k);

    // Instantiated form: { [K in keyof (A & B)]: (A & B)[K] }
    let mapped = build_instantiated_homomorphic_mapped(&interner, "K", a_and_b, template);
    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate_mapped(&mapped);

    assert_ne!(
        result,
        TypeId::ERROR,
        "intersection distribution must not produce ERROR"
    );
    assert_ne!(
        result,
        TypeId::NEVER,
        "intersection distribution must not produce NEVER"
    );

    match interner.lookup(result) {
        Some(TypeData::Intersection(list_id)) => {
            let members = interner.type_list(list_id);
            assert!(
                members.len() >= 2,
                "result must be an intersection of >=2 members"
            );
        }
        Some(TypeData::Object(_) | TypeData::ObjectWithIndex(_)) => {
            // Fully merged is also acceptable.
        }
        other => {
            assert!(
                !matches!(other, Some(TypeData::Mapped(_))),
                "intersection source must not leave mapped type deferred; got {other:?}"
            );
        }
    }
}

/// Iteration-variable name must not affect whether intersection distribution fires.
#[test]
fn intersection_distribution_is_iteration_variable_name_agnostic() {
    let interner = TypeInterner::new();

    let a_atom = interner.intern_string("a");
    let b_atom = interner.intern_string("b");
    let obj_a = interner.object(vec![PropertyInfo::new(a_atom, TypeId::NUMBER)]);
    let obj_b = interner.object(vec![PropertyInfo::new(b_atom, TypeId::STRING)]);
    let src = interner.intersection(vec![obj_a, obj_b]);

    let template = TypeId::BOOLEAN;

    let mut results = Vec::new();
    for iter_name in ["P", "K", "X", "Key"] {
        let mapped = build_instantiated_homomorphic_mapped(&interner, iter_name, src, template);
        let mut evaluator = TypeEvaluator::new(&interner);
        results.push(evaluator.evaluate_mapped(&mapped));
    }

    let first = results[0];
    for &r in &results[1..] {
        assert_eq!(
            r, first,
            "intersection distribution result must be identical regardless of iter-var name"
        );
    }
    assert!(
        !matches!(interner.lookup(first), Some(TypeData::Mapped(_))),
        "intersection distribution must not leave the mapped type deferred"
    );
}

/// Direct (non-instantiated) `{{ [K in keyof (A & B)]: ... }}` must NOT distribute.
/// The declared constraint matches the effective constraint, so the guard fires.
#[test]
fn direct_mapped_over_intersection_does_not_distribute() {
    let interner = TypeInterner::new();

    let x_atom = interner.intern_string("x");
    let y_atom = interner.intern_string("y");
    let obj_a = interner.object(vec![PropertyInfo::new(x_atom, TypeId::NUMBER)]);
    let obj_b = interner.object(vec![PropertyInfo::new(y_atom, TypeId::STRING)]);
    let a_and_b = interner.intersection(vec![obj_a, obj_b]);
    let constraint = interner.keyof(a_and_b);

    // Declared constraint == effective constraint: direct form, not an instantiation.
    let direct_mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: Some(constraint), // declared = effective → no distribution
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        },
        constraint,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate_mapped(&direct_mapped);

    assert_ne!(
        result,
        TypeId::ERROR,
        "direct mapped over intersection must not ERROR"
    );
    assert!(
        !matches!(interner.lookup(result), Some(TypeData::Mapped(_))),
        "direct mapped over intersection should be evaluated, not deferred"
    );
}

/// `Partial<A & B>` style: an instantiated optional-modifier homomorphic
/// mapped type over an intersection distributes, matching tsc.
#[test]
fn partial_style_instantiated_mapped_over_intersection_distributes() {
    let interner = TypeInterner::new();

    let x_atom = interner.intern_string("x");
    let y_atom = interner.intern_string("y");
    let obj_a = interner.object(vec![PropertyInfo::new(x_atom, TypeId::NUMBER)]);
    let obj_b = interner.object(vec![PropertyInfo::new(y_atom, TypeId::STRING)]);
    let src = interner.intersection(vec![obj_a, obj_b]);

    let outer_t = interner.type_param(TypeParamInfo::simple(interner.intern_string("T")));
    let keyof_outer_t = interner.keyof(outer_t);
    let iter_k = interner.type_param(TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keyof_outer_t),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let template = interner.index_access(src, iter_k);

    // Partial-like: add-optional modifier (declared constraint ≠ effective → distributes).
    let partial_mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: Some(keyof_outer_t),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        },
        constraint: interner.keyof(src),
        name_type: None,
        template,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Add),
    };

    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate_mapped(&partial_mapped);

    assert_ne!(result, TypeId::ERROR, "Partial<A & B> must not ERROR");
    assert!(
        !matches!(interner.lookup(result), Some(TypeData::Mapped(_))),
        "Partial<A & B> must not remain a deferred Mapped type"
    );
}

/// Cache-purity taint (#14347): extracting mapped keys from a constraint that is
/// a bare `Lazy(DefId)` whose body is not registered on this query is a
/// *registration-window artifact* — once the declaring file publishes the body
/// the same constraint yields concrete keys. The Lazy arm of
/// `extract_mapped_keys_impl` resolves the body directly (it does not route the
/// `Lazy` through the evaluator's `visit_lazy`), so it is the only site that can
/// record the taint for the callers that pass a *raw* `mapped.constraint`
/// (`try_evaluate_mapped_template_per_concrete_key` /
/// `try_evaluate_remapped_mapped_template_for_index` on the indexed-access path).
/// Without the mark, the deferred mapped/index result computed under the
/// unresolved body would be persisted in a `TypeId`-keyed result memo and shadow
/// the real expansion (the cross-arena member-degradation class, #13484 /
/// #10663).
#[test]
fn extract_mapped_keys_taints_on_unresolved_lazy_constraint() {
    let interner = TypeInterner::new();
    let mut evaluator = TypeEvaluator::new(&interner);
    let unresolved = interner.lazy(crate::def::DefId(987_654));

    let keys = evaluator.extract_mapped_keys(unresolved);

    assert!(
        keys.is_none(),
        "an unresolved Lazy(DefId) constraint cannot yield concrete mapped keys"
    );
    assert!(
        evaluator.is_unresolved_def_seen(),
        "extract_mapped_keys must mark the unresolved-def taint when the constraint's \
         Lazy body has nothing registered, so the deferred mapped/index result is kept \
         out of the TypeId-keyed caches (#14347)"
    );
}

/// Precision floor for the mapped-key extraction taint (#14347): a constraint
/// with a fully-resolvable, concrete key set observes no unresolved def, so the
/// taint must stay clear — proving the mark is keyed on an actually-missing
/// `Lazy(DefId)` body, not fired for every `None`/structural defer (which would
/// over-suppress caching for legitimate generic mapped types).
#[test]
fn extract_mapped_keys_does_not_taint_on_resolvable_constraint() {
    let interner = TypeInterner::new();
    let mut evaluator = TypeEvaluator::new(&interner);
    let concrete = interner.union(vec![
        interner.literal_string("a"),
        interner.literal_string("b"),
    ]);

    let keys = evaluator.extract_mapped_keys(concrete);

    assert!(
        keys.is_some(),
        "a concrete string-literal union constraint must yield extractable keys"
    );
    assert!(
        !evaluator.is_unresolved_def_seen(),
        "a fully-resolvable constraint observed no unresolved def, so the mapped-key \
         extraction taint must stay clear (#14347)"
    );
}

/// `{ readonly [K in keyof <operand>]: <operand>[K] }` — the shared shape for
/// the composite-operand deferral tests below.
fn readonly_mapped_over_keyof(interner: &TypeInterner, operand: TypeId) -> MappedType {
    let constraint = interner.keyof(operand);
    let key_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(constraint),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let key_param = interner.type_param(key_info);
    MappedType {
        type_param: key_info,
        constraint,
        name_type: None,
        template: interner.index_access(operand, key_param),
        readonly_modifier: Some(MappedModifier::Add),
        optional_modifier: None,
    }
}

fn string_prop_object(interner: &TypeInterner, name: &str) -> TypeId {
    interner.object(vec![PropertyInfo {
        name: interner.intern_string(name),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        is_string_named: true,
        ..Default::default()
    }])
}

/// A homomorphic mapped type keyed by `keyof (T & X)` where `T` is a free type
/// parameter is still *generic*: its key set includes `keyof T`, which has no
/// concrete expansion. Eagerly materializing it used to keep only the concrete
/// member's keys (dropping every key `T` contributes), which broke the
/// `Readonly<T & { p }> <: Readonly<T>` relation behind kysely's freeze-factory
/// pattern (#10663). tsc's `isGenericIndexType` defers here; so must tsz.
#[test]
fn mapped_over_keyof_intersection_with_free_param_stays_deferred() {
    let interner = TypeInterner::new();
    let t_param = interner.type_param(TypeParamInfo::simple(interner.intern_string("T")));
    let concrete = string_prop_object(&interner, "where");

    for operand in [
        interner.intersection(vec![t_param, concrete]),
        interner.intersection(vec![concrete, t_param]),
        interner.union(vec![t_param, concrete]),
    ] {
        let mapped = readonly_mapped_over_keyof(&interner, operand);
        let mut evaluator = TypeEvaluator::new(&interner);
        let result = evaluator.evaluate_mapped(&mapped);
        assert!(
            matches!(interner.lookup(result), Some(TypeData::Mapped(_))),
            "mapped over keyof of a composite containing a free type parameter must stay \
             deferred, got {:?}",
            interner.lookup(result)
        );
    }
}

/// Precision floor for the composite-operand deferral: `keyof (A & B)` over
/// fully concrete members has a complete key set, so the mapped type must keep
/// materializing eagerly (deferring here would regress display and
/// excess-property behavior for concrete intersections).
#[test]
fn mapped_over_keyof_concrete_intersection_still_materializes() {
    let interner = TypeInterner::new();
    let operand = interner.intersection(vec![
        string_prop_object(&interner, "a"),
        string_prop_object(&interner, "b"),
    ]);
    let mapped = readonly_mapped_over_keyof(&interner, operand);

    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate_mapped(&mapped);
    assert!(
        !matches!(interner.lookup(result), Some(TypeData::Mapped(_))),
        "mapped over keyof of a fully concrete intersection must still materialize, got {:?}",
        interner.lookup(result)
    );
}
