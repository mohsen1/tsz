use super::*;
use crate::construction::TypeInterner;
use crate::recursion::RecursionResult;
use crate::types::{MappedModifier, PropertyInfo, TupleElement, TypeParamInfo};

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

fn single_property_object(interner: &TypeInterner, name: &str) -> TypeId {
    interner.object(vec![PropertyInfo::new(
        interner.intern_string(name),
        TypeId::STRING,
    )])
}

#[test]
fn evaluate_keyof_or_constraint_defers_seeded_fifth_keyof_identity() {
    let interner = TypeInterner::new();
    let object = single_property_object(&interner, "seeded");
    let constraint = interner.keyof(object);
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(object, 4);
    let result = evaluator.evaluate_keyof_or_constraint(constraint);

    assert_eq!(
        result, constraint,
        "mapped keyof-constraint reduction should preserve the deferred KeyOf at the fifth same-identity hit"
    );
    assert!(
        evaluator.has_incomplete_request_verdict(),
        "mapped keyof-constraint identity bailout must mark the request incomplete"
    );
}

#[test]
fn evaluate_keyof_or_constraint_allows_seeded_fourth_keyof_identity_and_pops() {
    let interner = TypeInterner::new();
    let object = single_property_object(&interner, "finite");
    let constraint = interner.keyof(object);
    let expected = interner.literal_string("finite");
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(object, 3);

    assert_eq!(evaluator.evaluate_keyof_or_constraint(constraint), expected);
    assert_eq!(evaluator.evaluate_keyof_or_constraint(constraint), expected);
    assert!(
        !evaluator.has_incomplete_request_verdict(),
        "below-cutoff mapped keyof-constraint reductions must pop their temporary identity entry"
    );
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
/// final result is the original union (e.g. `M<string | "foo">` → `string | "foo"`).
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

    // Build Union(lit_0, Union(lit_1, Union(lit_2, ... )))
    let mut nested = interner.literal_string("leaf");
    for i in 0..50u32 {
        let lit = interner.literal_string(&i.to_string());
        nested = interner.union(vec![lit, nested]);
    }

    // Must not stack-overflow, must return a type (either the nested union or a simplified form)
    let result = evaluator.evaluate_keyof_or_constraint(nested);
    // The result is a valid TypeId (non-error).
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

/// Build the post-instantiation form of the identity homomorphic mapped
/// `type M<T> = { [<iter_name> in keyof T]: T[<iter_name>] }` with `T`
/// substituted by `concrete_source`. Used by the variadic-tuple tests
/// below.
fn build_identity_homomorphic_mapped(
    interner: &TypeInterner,
    iter_name: &str,
    concrete_source: TypeId,
) -> MappedType {
    let iter_atom = interner.intern_string(iter_name);
    let outer_t = interner.type_param(TypeParamInfo::simple(interner.intern_string("T")));
    let original_constraint = interner.keyof(outer_t);
    let iter_param = interner.type_param(TypeParamInfo {
        name: iter_atom,
        constraint: Some(original_constraint),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let template = interner.index_access(concrete_source, iter_param);
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

/// Issue #9694: `{ [K in keyof T]: T[K] }` over a variadic tuple
/// `[number, ...string[]]` must reproduce the same tuple structurally —
/// not a tuple whose rest element widened to `(number | string)[]`. The
/// pre-fix bug substituted `K = number` for the rest, evaluating
/// `tuple[number]` to the union of all element types.
#[test]
fn identity_homomorphic_mapped_over_trailing_rest_variadic_tuple_preserves_shape() {
    let interner = TypeInterner::new();
    let elements = vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: interner.array(TypeId::STRING),
            name: None,
            optional: false,
            rest: true,
        },
    ];
    let source = interner.tuple(elements.clone());
    let mapped = build_identity_homomorphic_mapped(&interner, "K", source);
    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate_mapped(&mapped);

    let expected = interner.tuple(elements);
    assert_eq!(
        result, expected,
        "identity homomorphic over `[number, ...string[]]` must reproduce the same tuple"
    );
}

/// The same shape with a renamed iteration variable (`P` instead of `K`)
/// must produce the same structural result. The fix must be name-blind.
#[test]
fn identity_homomorphic_mapped_over_trailing_rest_renamed_iter_var() {
    let interner = TypeInterner::new();
    // Same element types as the canonical twin; only the iter var name changes.
    let elements = vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: interner.array(TypeId::STRING),
            name: None,
            optional: false,
            rest: true,
        },
    ];
    let source = interner.tuple(elements.clone());
    let mapped = build_identity_homomorphic_mapped(&interner, "P", source);
    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate_mapped(&mapped);

    let expected = interner.tuple(elements);
    assert_eq!(
        result, expected,
        "identity homomorphic with iter `P` must produce the same tuple as iter `K`"
    );
}

/// Leading-rest variadic tuple `[...string[], number]` must round-trip
/// through the identity homomorphic mapped. Pre-fix this produced a
/// tuple whose tail and rest were both wrong because `tuple[1]` did not
/// uniquely resolve to a single element's type.
#[test]
fn identity_homomorphic_mapped_over_leading_rest_variadic_tuple_preserves_shape() {
    let interner = TypeInterner::new();
    let elements = vec![
        TupleElement {
            type_id: interner.array(TypeId::STRING),
            name: None,
            optional: false,
            rest: true,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ];
    let source = interner.tuple(elements.clone());
    let mapped = build_identity_homomorphic_mapped(&interner, "K", source);
    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate_mapped(&mapped);

    let expected = interner.tuple(elements);
    assert_eq!(
        result, expected,
        "identity homomorphic over `[...string[], number]` must reproduce the same tuple"
    );
}

/// Same leading-rest test with a renamed iteration variable (`P` instead of `K`).
/// The fix must be name-blind — changing the iteration variable must not affect
/// which branch handles suffix elements.
#[test]
fn identity_homomorphic_mapped_over_leading_rest_renamed_iter_var() {
    let interner = TypeInterner::new();
    let elements = vec![
        TupleElement::rest(interner.array(TypeId::BOOLEAN)),
        TupleElement::fixed(TypeId::STRING),
    ];
    let source = interner.tuple(elements.clone());
    let mapped = build_identity_homomorphic_mapped(&interner, "P", source);
    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate_mapped(&mapped);

    let expected = interner.tuple(elements);
    assert_eq!(
        result, expected,
        "identity homomorphic with iter `P` over `[...boolean[], string]` must preserve shape"
    );
}

/// Middle-rest tuple `[string, ...number[], boolean]` — the suffix element
/// follows a rest element that is itself preceded by a fixed prefix. The
/// proxy-based suffix rebinding must handle this shape too.
#[test]
fn identity_homomorphic_mapped_over_middle_rest_with_prefix_and_suffix() {
    let interner = TypeInterner::new();
    let elements = vec![
        TupleElement::fixed(TypeId::STRING),
        TupleElement::rest(interner.array(TypeId::NUMBER)),
        TupleElement::fixed(TypeId::BOOLEAN),
    ];
    let source = interner.tuple(elements.clone());
    let mapped = build_identity_homomorphic_mapped(&interner, "K", source);
    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate_mapped(&mapped);

    let expected = interner.tuple(elements);
    assert_eq!(
        result, expected,
        "identity homomorphic over `[string, ...number[], boolean]` must preserve shape"
    );
}

/// Multiple suffix elements after the rest: `[...string[], number, boolean]`.
/// Both suffix elements require the proxy rebind, and each must resolve to
/// its own type, not the union of all element types.
#[test]
fn identity_homomorphic_mapped_over_leading_rest_multiple_suffixes() {
    let interner = TypeInterner::new();
    let elements = vec![
        TupleElement::rest(interner.array(TypeId::STRING)),
        TupleElement::fixed(TypeId::NUMBER),
        TupleElement::fixed(TypeId::BOOLEAN),
    ];
    let source = interner.tuple(elements.clone());
    let mapped = build_identity_homomorphic_mapped(&interner, "K", source);
    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate_mapped(&mapped);

    let expected = interner.tuple(elements);
    assert_eq!(
        result, expected,
        "identity homomorphic over `[...string[], number, boolean]` must preserve all suffix types"
    );
}

/// Fixed (non-variadic) tuples are the negative control: the pre-fix
/// code path worked for them and the new structural fix must not
/// regress this case.
#[test]
fn identity_homomorphic_mapped_over_fixed_tuple_preserves_shape() {
    let interner = TypeInterner::new();
    let elements = vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
    ];
    let source = interner.tuple(elements.clone());
    let mapped = build_identity_homomorphic_mapped(&interner, "K", source);
    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate_mapped(&mapped);

    let expected = interner.tuple(elements);
    assert_eq!(
        result, expected,
        "identity homomorphic over `[number, string]` must reproduce the same tuple"
    );
}

/// Mixed optional and rest: `[number, string?, ...boolean[]]`. Optional
/// flags on fixed elements must be preserved, and the rest's inner type
/// must remain `boolean` (not widened to `number | string | boolean`).
#[test]
fn identity_homomorphic_mapped_over_optional_and_rest_tuple_preserves_shape() {
    let interner = TypeInterner::new();
    let elements = vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: true,
            rest: false,
        },
        TupleElement {
            type_id: interner.array(TypeId::BOOLEAN),
            name: None,
            optional: false,
            rest: true,
        },
    ];
    let source = interner.tuple(elements.clone());
    let mapped = build_identity_homomorphic_mapped(&interner, "K", source);
    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate_mapped(&mapped);

    let expected = interner.tuple(elements);
    assert_eq!(
        result, expected,
        "identity homomorphic must preserve optional flags and the rest's inner type"
    );
}

/// Non-identity homomorphic mapped over a variadic tuple. For
/// `Boxified<T> = { [K in keyof T]: Box<T[K]> }` applied to
/// `[number, ...string[]]`, the result must be
/// `[Box<number>, ...Box<string>[]]` — the rest's inner is `Box<string>`,
/// not `Box<number | string>` (which would be the pre-fix output).
#[test]
fn non_identity_homomorphic_mapped_over_trailing_rest_tuple_applies_per_element() {
    use crate::def::DefId;

    let interner = TypeInterner::new();
    // Build a Box<T_arg> wrapper around T_arg using an Application over a
    // Lazy(DefId) base. `substitute_exact_type` substitutes the source in
    // the template; evaluation of the substituted index access then yields
    // the per-element inner type, which the wrapper carries through.
    let box_base = interner.lazy(DefId(9001));

    let iter_atom = interner.intern_string("K");
    let outer_t = interner.type_param(TypeParamInfo::simple(interner.intern_string("T")));
    let original_constraint = interner.keyof(outer_t);
    let iter_param = interner.type_param(TypeParamInfo {
        name: iter_atom,
        constraint: Some(original_constraint),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });

    let source_elements = vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: interner.array(TypeId::STRING),
            name: None,
            optional: false,
            rest: true,
        },
    ];
    let source = interner.tuple(source_elements);

    // Template: `Box<source[K]>` — the source is baked in by the outer
    // M<source> instantiation.
    let index_access = interner.index_access(source, iter_param);
    let template = interner.application(box_base, vec![index_access]);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: iter_atom,
            constraint: Some(original_constraint),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        },
        constraint: interner.keyof(source),
        name_type: None,
        template,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate_mapped(&mapped);

    let expected = interner.tuple(vec![
        TupleElement {
            type_id: interner.application(box_base, vec![TypeId::NUMBER]),
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: interner.array(interner.application(box_base, vec![TypeId::STRING])),
            name: None,
            optional: false,
            rest: true,
        },
    ]);
    assert_eq!(
        result, expected,
        "non-identity homomorphic over `[number, ...string[]]` must produce \
             `[Box<number>, ...Box<string>[]]`, not widen the rest's inner to a union"
    );
}

/// Opaque variadic rests like `...T` are not the same as concrete `...E[]`
/// rests. Mapping `[string, number, ...T]` through
/// `{ [K in keyof T]: T[K][] }` must preserve the tuple-position indexed access
/// for the opaque rest; collapsing it to `T[number][]` loses reverse-inference
/// provenance for `T`.
#[test]
fn non_identity_homomorphic_mapped_over_opaque_variadic_rest_keeps_positional_access() {
    let interner = TypeInterner::new();

    let iter_atom = interner.intern_string("K");
    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(interner.array(TypeId::UNKNOWN)),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let original_constraint = interner.keyof(t_param);
    let iter_param = interner.type_param(TypeParamInfo {
        name: iter_atom,
        constraint: Some(original_constraint),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });

    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let template = interner.array(interner.index_access(source, iter_param));
    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: iter_atom,
            constraint: Some(original_constraint),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        },
        constraint: interner.keyof(source),
        name_type: None,
        template,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate_mapped(&mapped);
    let Some(TypeData::Tuple(tuple_id)) = interner.lookup(result) else {
        panic!("expected mapped tuple, got {:?}", interner.lookup(result));
    };
    let elements = interner.tuple_list(tuple_id);
    assert_eq!(elements.len(), 3);
    assert!(elements[2].rest, "opaque variadic rest must stay a rest");

    let collapsed = interner.array(interner.index_access(t_param, TypeId::NUMBER));
    assert_ne!(
        elements[2].type_id, collapsed,
        "opaque variadic rest must not collapse to `T[number][]`"
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

    // Build two overlapping unions that share a member so the guard is exercised
    // across recursive member evaluation: U1 = (lit_x | U2), U2 = (lit_y | lit_z)
    // evaluate_keyof_or_constraint on U1 recurses into both lit_x and U2;
    // evaluating U2 recurses into lit_y and lit_z. The guard must handle all
    // levels without hanging.
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

    // A constraint that evaluates to itself must terminate immediately (the
    // `step != current` guard short-circuits before re-entering the loop).
    let plain_union = interner.union(vec![lit_x, lit_y]);
    let result2 = evaluator.evaluate_keyof_or_constraint(plain_union);
    assert_ne!(
        result2,
        TypeId::ERROR,
        "self-stable union must terminate without ERROR"
    );
}

/// `{ [K in keyof T as K]: T[K] }` applied to `{ readonly a?: number; b?: string; c: boolean }`
/// must preserve optional and readonly modifiers from the source type — matching tsc behavior.
///
/// Covers the structural rule: when a homomorphic mapped type has an identity `as K` remap
/// clause (where K is the same as the iteration variable), tsz must treat the type as
/// homomorphic and inherit source property modifiers, the same as with no `as` clause.
///
/// Verified for multiple iteration variable names to prove the rule is structural, not
/// keyed on the spelling `K`.
#[test]
fn identity_as_clause_mapped_over_object_preserves_optional_and_readonly_modifiers() {
    let interner = TypeInterner::new();

    let a_atom = interner.intern_string("a");
    let b_atom = interner.intern_string("b");
    let c_atom = interner.intern_string("c");

    // source: { readonly a?: number; b?: string; c: boolean }
    let source = interner.object(vec![
        PropertyInfo {
            name: a_atom,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true,
            readonly: true,
            ..Default::default()
        },
        PropertyInfo {
            name: b_atom,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: true,
            readonly: false,
            ..Default::default()
        },
        PropertyInfo::new(c_atom, TypeId::BOOLEAN),
    ]);

    // The same structural fix must apply regardless of iteration-variable spelling.
    for iter_name in ["K", "P", "X", "Key"] {
        let iter_atom = interner.intern_string(iter_name);
        let outer_t = interner.type_param(TypeParamInfo::simple(interner.intern_string("T")));
        let original_constraint = interner.keyof(outer_t);
        let iter_param = interner.type_param(TypeParamInfo {
            name: iter_atom,
            constraint: Some(original_constraint),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let template = interner.index_access(source, iter_param);
        // name_type is the iteration variable itself — identity `as K` remap.
        let name_type_param = interner.type_param(TypeParamInfo::simple(iter_atom));

        let mapped = MappedType {
            type_param: TypeParamInfo {
                name: iter_atom,
                constraint: Some(original_constraint),
                default: None,
                is_const: false,
                origin: crate::types::TypeParamOrigin::User,
            },
            constraint: interner.keyof(source),
            name_type: Some(name_type_param),
            template,
            readonly_modifier: None,
            optional_modifier: None,
        };

        let mut evaluator = TypeEvaluator::new(&interner);
        let result = evaluator.evaluate_mapped(&mapped);

        assert_ne!(
            result,
            TypeId::ERROR,
            "iter `{iter_name}`: identity as-clause mapped type must not produce ERROR"
        );
        assert_ne!(
            result,
            TypeId::NEVER,
            "iter `{iter_name}`: identity as-clause mapped type must not produce NEVER"
        );

        let shape_id = match interner.lookup(result) {
            Some(TypeData::Object(id)) => id,
            other => panic!(
                "iter `{iter_name}`: expected Object result, got {other:?} (TypeId={result:?})"
            ),
        };
        let shape = interner.object_shape(shape_id);

        let find_prop = |atom: Atom| shape.properties.iter().find(|p| p.name == atom).cloned();

        let prop_a = find_prop(a_atom)
            .unwrap_or_else(|| panic!("iter `{iter_name}`: property 'a' must exist"));
        assert!(
            prop_a.optional,
            "iter `{iter_name}`: property 'a' must be optional (source was `a?: number`)"
        );
        assert!(
            prop_a.readonly,
            "iter `{iter_name}`: property 'a' must be readonly (source was `readonly a?`)"
        );

        let prop_b = find_prop(b_atom)
            .unwrap_or_else(|| panic!("iter `{iter_name}`: property 'b' must exist"));
        assert!(
            prop_b.optional,
            "iter `{iter_name}`: property 'b' must be optional (source was `b?: string`)"
        );
        assert!(
            !prop_b.readonly,
            "iter `{iter_name}`: property 'b' must not be readonly"
        );

        let prop_c = find_prop(c_atom)
            .unwrap_or_else(|| panic!("iter `{iter_name}`: property 'c' must exist"));
        assert!(
            !prop_c.optional,
            "iter `{iter_name}`: property 'c' must not be optional (source was `c: boolean`)"
        );
        assert!(
            !prop_c.readonly,
            "iter `{iter_name}`: property 'c' must not be readonly"
        );
    }
}

/// When the source type has been evaluated to a concrete object through an alias,
/// the identity `as K` mapped type must still preserve modifiers.
///
/// Structural rule: modifier preservation through identity `as K` must hold even
/// when the source is accessed indirectly (e.g., through an alias-like evaluation).
#[test]
fn identity_as_clause_with_renamed_iter_var_is_name_agnostic() {
    let interner = TypeInterner::new();
    let x_atom = interner.intern_string("x");

    // source: { readonly x?: boolean }
    let source = interner.object(vec![PropertyInfo {
        name: x_atom,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: true,
        readonly: true,
        ..Default::default()
    }]);

    // Build with two different variable names to confirm name-agnostic behaviour.
    for (iter_name, outer_name) in [("K", "T"), ("Prop", "Source"), ("Item", "Obj")] {
        let iter_atom = interner.intern_string(iter_name);
        let outer_t =
            interner.type_param(TypeParamInfo::simple(interner.intern_string(outer_name)));
        let original_constraint = interner.keyof(outer_t);
        let iter_param = interner.type_param(TypeParamInfo {
            name: iter_atom,
            constraint: Some(original_constraint),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let template = interner.index_access(source, iter_param);
        let name_type_param = interner.type_param(TypeParamInfo::simple(iter_atom));

        let mapped = MappedType {
            type_param: TypeParamInfo {
                name: iter_atom,
                constraint: Some(original_constraint),
                default: None,
                is_const: false,
                origin: crate::types::TypeParamOrigin::User,
            },
            constraint: interner.keyof(source),
            name_type: Some(name_type_param),
            template,
            readonly_modifier: None,
            optional_modifier: None,
        };

        let mut evaluator = TypeEvaluator::new(&interner);
        let result = evaluator.evaluate_mapped(&mapped);

        let shape_id = match interner.lookup(result) {
            Some(TypeData::Object(id)) => id,
            other => panic!("iter `{iter_name}`: expected Object, got {other:?}"),
        };
        let shape = interner.object_shape(shape_id);
        let prop = shape
            .properties
            .iter()
            .find(|p| p.name == x_atom)
            .unwrap_or_else(|| panic!("iter `{iter_name}`: property 'x' must exist"));

        assert!(
            prop.optional,
            "iter `{iter_name}`: property 'x' must remain optional through identity as-clause"
        );
        assert!(
            prop.readonly,
            "iter `{iter_name}`: property 'x' must remain readonly through identity as-clause"
        );
    }
}

/// Build `{ a: number } & { b: string }` and return the intersection source
/// plus the two disjoint key atoms, the shared setup for the distribution tests
/// below.
fn build_disjoint_object_intersection(interner: &TypeInterner) -> (TypeId, Atom, Atom) {
    let a_atom = interner.intern_string("a");
    let b_atom = interner.intern_string("b");
    let obj_a = interner.object(vec![PropertyInfo::new(a_atom, TypeId::NUMBER)]);
    let obj_b = interner.object(vec![PropertyInfo::new(b_atom, TypeId::STRING)]);
    (interner.intersection(vec![obj_a, obj_b]), a_atom, b_atom)
}

/// Intersection sources are distributed by `try_distribute_mapped_over_composite_source`
/// → `distribute_mapped_over_members`: a generic homomorphic `M<A & B>` becomes
/// `M<A> & M<B>`. The distributed result must carry every key contributed by
/// each member object (here `a` from `A` and `b` from `B`), proving the
/// distribution iterated both members rather than collapsing the intersection.
#[test]
fn instantiated_homomorphic_mapped_distributes_over_object_intersection() {
    let interner = TypeInterner::new();
    let (source, a_atom, b_atom) = build_disjoint_object_intersection(&interner);

    // Constant `boolean` template: every produced property is `boolean`, so we
    // can assert purely on the *key set* surviving distribution.
    let mapped = build_instantiated_homomorphic_mapped(&interner, "P", source, TypeId::BOOLEAN);
    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate(interner.mapped(mapped));

    // Collect the keys reachable on the distributed result.
    let mut names = std::collections::BTreeSet::new();
    let collect = |obj: TypeId, names: &mut std::collections::BTreeSet<Atom>| {
        if let Some(TypeData::Object(shape_id)) = interner.lookup(obj) {
            for prop in &interner.object_shape(shape_id).properties {
                names.insert(prop.name);
                assert_eq!(
                    prop.type_id,
                    TypeId::BOOLEAN,
                    "distributed property must carry the mapped template"
                );
            }
        }
    };
    match interner.lookup(result) {
        Some(TypeData::Intersection(list_id)) => {
            for member in interner.type_list(list_id).to_vec() {
                collect(member, &mut names);
            }
        }
        Some(TypeData::Object(_)) => collect(result, &mut names),
        other => panic!("expected object/intersection result, got {other:?}"),
    }
    assert!(
        names.contains(&a_atom) && names.contains(&b_atom),
        "distributed result must keep keys from every intersection member, got {names:?}"
    );
}

/// Routing distribution through the cached `evaluate` makes evaluation
/// idempotent: re-evaluating the same interned mapped id returns the identical
/// `TypeId` (the evaluator memo / interner collapse repeats). This guards the
/// over-instantiation fix — structurally-identical member instantiations must
/// not produce divergent fresh ids on re-evaluation.
#[test]
fn distributed_mapped_over_intersection_is_idempotent() {
    let interner = TypeInterner::new();
    let (source, _a, _b) = build_disjoint_object_intersection(&interner);
    let mapped = build_instantiated_homomorphic_mapped(&interner, "P", source, TypeId::BOOLEAN);
    let mapped_id = interner.mapped(mapped);

    let mut evaluator = TypeEvaluator::new(&interner);
    let first = evaluator.evaluate(mapped_id);
    let second = evaluator.evaluate(mapped_id);
    assert_eq!(
        first, second,
        "re-evaluating the same distributed mapped id must be stable (cached)"
    );
}

/// Build the identity homomorphic mapped `{ [K in keyof T]: T[K] }` over
/// `source`, optionally adding an identity `as K` remap and a readonly modifier.
/// Reuses [`build_identity_homomorphic_mapped`] for the shared base shape.
fn build_homomorphic_mapped_over(
    interner: &TypeInterner,
    iter_name: &str,
    source: TypeId,
    as_clause: bool,
    readonly_modifier: Option<MappedModifier>,
) -> MappedType {
    let mut mapped = build_identity_homomorphic_mapped(interner, iter_name, source);
    if as_clause {
        let iter_atom = interner.intern_string(iter_name);
        mapped.name_type = Some(interner.type_param(TypeParamInfo::simple(iter_atom)));
    }
    mapped.readonly_modifier = readonly_modifier;
    mapped
}

/// A homomorphic mapped type over `readonly T[]` / `ReadonlyArray<T>` must
/// preserve the readonly array shape rather than collapsing to a plain object
/// (which dropped the `readonly` modifier and synthesized mutable-array methods
/// like `push`). Readonly is kept for a plain `{ [K in keyof T]: T[K] }`, an
/// identity `as K` remap, and an explicit `+readonly`; a no-`as` `-readonly`
/// yields a mutable array, matching tsc's `instantiateMappedArrayType`.
#[test]
fn homomorphic_mapped_over_readonly_array_preserves_readonly() {
    let interner = TypeInterner::new();
    // source: readonly number[]
    let source = interner.readonly_type(interner.array(TypeId::NUMBER));

    // (as_clause, readonly_modifier, expect_readonly_result, label)
    let cases = [
        (false, None, true, "plain identity"),
        (true, None, true, "identity as K"),
        (false, Some(MappedModifier::Add), true, "+readonly"),
        (
            false,
            Some(MappedModifier::Remove),
            false,
            "-readonly (no as)",
        ),
    ];

    for iter_name in ["K", "P", "Item"] {
        for &(as_clause, readonly_modifier, expect_readonly, label) in &cases {
            let mapped = build_homomorphic_mapped_over(
                &interner,
                iter_name,
                source,
                as_clause,
                readonly_modifier,
            );
            let mut evaluator = TypeEvaluator::new(&interner);
            let result = evaluator.evaluate_mapped(&mapped);
            let inner = match interner.lookup(result) {
                Some(TypeData::ReadonlyType(inner)) if expect_readonly => inner,
                Some(TypeData::Array(_)) if !expect_readonly => result,
                other => panic!(
                    "iter `{iter_name}`: {label}: expected a {} array, got {other:?}",
                    if expect_readonly {
                        "readonly"
                    } else {
                        "mutable"
                    }
                ),
            };
            assert!(
                matches!(interner.lookup(inner), Some(TypeData::Array(_))),
                "iter `{iter_name}`: {label}: result must wrap an Array, inner was {:?}",
                interner.lookup(inner)
            );
        }
    }
}
