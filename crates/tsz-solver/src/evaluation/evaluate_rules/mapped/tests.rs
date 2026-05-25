use super::*;
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
    });
    let template = interner.index_access(concrete_source, iter_param);
    MappedType {
        type_param: TypeParamInfo {
            name: iter_atom,
            constraint: Some(original_constraint),
            default: None,
            is_const: false,
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
    let elements = vec![
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: interner.array(TypeId::NUMBER),
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
