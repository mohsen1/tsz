#[test]
fn test_mapped_type_mixed_string_and_symbol_keys() {
    // { [K in "str" | typeof sym1]: number }
    // → { str: number; [sym1]: number }
    let interner = TypeInterner::new();
    let sym1 = crate::types::SymbolRef(504);
    let sym1_type = interner.unique_symbol(sym1);
    let str_key = interner.literal_string("str");
    let constraint = interner.union(vec![str_key, sym1_type]);

    let k_name = interner.intern_string("K");
    let k_info = TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let mapped = MappedType {
        type_param: k_info,
        constraint,
        name_type: None,
        template: TypeId::NUMBER,
        optional_modifier: None,
        readonly_modifier: None,
    };
    let result = evaluate_type(&interner, interner.mapped(mapped));

    // String key should be present
    let has_str_prop = match interner.lookup(result) {
        Some(TypeData::Object(sid) | TypeData::ObjectWithIndex(sid)) => interner
            .object_shape(sid)
            .properties
            .iter()
            .any(|p| !p.is_symbol_named && interner.resolve_atom_ref(p.name).as_ref() == "str"),
        _ => false,
    };
    assert!(
        has_str_prop,
        "mixed-key mapped type should preserve string key 'str'"
    );
    assert_eq!(
        find_symbol_property(&interner, result, sym1),
        Some(TypeId::NUMBER),
        "mixed-key mapped type should preserve symbol key sym1"
    );
}

#[test]
fn test_mapped_type_symbol_filter_as_clause() {
    // type SymbolProps<T> = { [K in keyof T as K extends symbol ? K : never]: T[K] }
    // Applied to { str: string, [sym1]: number }
    // → { [sym1]: number }
    //
    // We simulate this by using "str" | UniqueSymbol(sym1) as the constraint and
    // a conditional as the name_type.
    let interner = TypeInterner::new();
    let sym1 = crate::types::SymbolRef(505);
    let sym1_type = interner.unique_symbol(sym1);
    let str_key = interner.literal_string("str");
    let constraint = interner.union(vec![str_key, sym1_type]);

    let k_name = interner.intern_string("K");
    let k_info = TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let k_type = interner.type_param(crate::types::TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    });

    // as K extends symbol ? K : never
    let name_type = interner.conditional(ConditionalType {
        check_type: k_type,
        extends_type: TypeId::SYMBOL,
        true_type: k_type,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    let mapped = MappedType {
        type_param: k_info,
        constraint,
        name_type: Some(name_type),
        template: TypeId::NUMBER,
        optional_modifier: None,
        readonly_modifier: None,
    };
    let result = evaluate_type(&interner, interner.mapped(mapped));

    // "str" should be filtered out by the as clause
    let has_str_prop = match interner.lookup(result) {
        Some(TypeData::Object(sid) | TypeData::ObjectWithIndex(sid)) => interner
            .object_shape(sid)
            .properties
            .iter()
            .any(|p| !p.is_symbol_named && interner.resolve_atom_ref(p.name).as_ref() == "str"),
        _ => false,
    };
    assert!(
        !has_str_prop,
        "as clause `K extends symbol ? K : never` should filter out string key 'str'"
    );

    // [sym1] should be kept
    assert_eq!(
        find_symbol_property(&interner, result, sym1),
        Some(TypeId::NUMBER),
        "as clause `K extends symbol ? K : never` should preserve unique symbol key"
    );
}

#[test]
fn test_mapped_type_string_filter_as_clause_removes_symbol_keys() {
    // { [K in "str" | typeof sym1 as K extends string ? K : never]: number }
    // → { str: number }  (symbol filtered out)
    let interner = TypeInterner::new();
    let sym1 = crate::types::SymbolRef(506);
    let sym1_type = interner.unique_symbol(sym1);
    let str_key = interner.literal_string("str");
    let constraint = interner.union(vec![str_key, sym1_type]);

    let k_name = interner.intern_string("K");
    let k_info = TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let k_type = interner.type_param(crate::types::TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    });

    // as K extends string ? K : never
    let name_type = interner.conditional(ConditionalType {
        check_type: k_type,
        extends_type: TypeId::STRING,
        true_type: k_type,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    let mapped = MappedType {
        type_param: k_info,
        constraint,
        name_type: Some(name_type),
        template: TypeId::NUMBER,
        optional_modifier: None,
        readonly_modifier: None,
    };
    let result = evaluate_type(&interner, interner.mapped(mapped));

    // Symbol key should be filtered out
    assert!(
        find_symbol_property(&interner, result, sym1).is_none(),
        "as K extends string ? K : never should filter out symbol key"
    );
}

fn req_elem(type_id: TypeId) -> crate::types::TupleElement {
    crate::types::TupleElement {
        type_id,
        name: None,
        optional: false,
        rest: false,
    }
}

fn rest_elem(type_id: TypeId) -> crate::types::TupleElement {
    crate::types::TupleElement {
        type_id,
        name: None,
        optional: false,
        rest: true,
    }
}

/// Build and evaluate the post-instantiation form of the identity homomorphic
/// mapping `{ [<iter> in keyof X]: X[<iter>] }` with `X` = `source`.
fn eval_identity_mapped(interner: &TypeInterner, iter: &str, source: TypeId) -> TypeId {
    let keyof = interner.keyof(source);
    let tp_info = TypeParamInfo {
        name: interner.intern_string(iter),
        constraint: None,
        default: None,
        is_const: false,
    };
    let tp = interner.intern(TypeData::TypeParameter(tp_info));
    let template = interner.index_access(source, tp);
    let mapped = MappedType {
        type_param: tp_info,
        constraint: keyof,
        name_type: None,
        template,
        optional_modifier: None,
        readonly_modifier: None,
    };
    evaluate_type(interner, interner.mapped(mapped))
}

#[test]
fn identity_mapped_over_variadic_tuple_roundtrips() {
    // Identity<[string, ...number[]]> === [string, ...number[]].
    // Renaming the iteration variable must not change the result — the fix is
    // structural, not keyed on the spelling `I`.
    for iter in ["I", "Idx", "P"] {
        let interner = TypeInterner::new();
        let source = interner.tuple(vec![
            req_elem(TypeId::STRING),
            rest_elem(interner.array(TypeId::NUMBER)),
        ]);

        let result = eval_identity_mapped(&interner, iter, source);
        assert_eq!(
            result, source,
            "identity over [string, ...number[]] should roundtrip (iter `{iter}`)"
        );

        // Pin the rest element type: it must be the rest element's own type
        // (number[]), not `string | number`.
        let Some(TypeData::Tuple(list_id)) = interner.lookup(result) else {
            panic!("expected tuple result, got {:?}", interner.lookup(result));
        };
        let elements = interner.tuple_list(list_id);
        assert_eq!(elements.len(), 2);
        assert!(elements[1].rest, "second element must stay a rest element");
        assert_eq!(
            elements[1].type_id,
            interner.array(TypeId::NUMBER),
            "rest member must map to number[], not (string | number)[]"
        );
    }
}

#[test]
fn identity_mapped_over_labeled_variadic_tuple_roundtrips() {
    // [a: string, ...b: number[]] — labels must not change the per-position rule.
    let interner = TypeInterner::new();
    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let source = interner.tuple(vec![
        crate::types::TupleElement {
            type_id: TypeId::STRING,
            name: Some(a),
            optional: false,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: interner.array(TypeId::NUMBER),
            name: Some(b),
            optional: false,
            rest: true,
        },
    ]);

    let result = eval_identity_mapped(&interner, "I", source);
    let Some(TypeData::Tuple(list_id)) = interner.lookup(result) else {
        panic!("expected tuple result, got {:?}", interner.lookup(result));
    };
    let elements = interner.tuple_list(list_id);
    assert_eq!(elements.len(), 2);
    assert!(elements[1].rest);
    assert_eq!(
        elements[1].type_id,
        interner.array(TypeId::NUMBER),
        "labeled rest member must map to number[]"
    );
}

#[test]
fn identity_mapped_over_multi_prefix_variadic_tuple_roundtrips() {
    // [string, symbol, ...number[]] — the rest member must be number, not the
    // union of the whole tuple (string | symbol | number).
    let interner = TypeInterner::new();
    let source = interner.tuple(vec![
        req_elem(TypeId::STRING),
        req_elem(TypeId::SYMBOL),
        rest_elem(interner.array(TypeId::NUMBER)),
    ]);

    let result = eval_identity_mapped(&interner, "I", source);
    assert_eq!(
        result, source,
        "identity over [string, symbol, ...number[]] should roundtrip"
    );
}

#[test]
fn wrapping_mapped_over_variadic_tuple_wraps_per_position_element() {
    // { [I in keyof X]: [X[I]] } over [string, ...number[]] should produce
    // [[string], ...[number][]] — the rest member is [number], NOT [string | number].
    let interner = TypeInterner::new();
    let source = interner.tuple(vec![
        req_elem(TypeId::STRING),
        rest_elem(interner.array(TypeId::NUMBER)),
    ]);

    let keyof = interner.keyof(source);
    let tp_info = TypeParamInfo {
        name: interner.intern_string("I"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let tp = interner.intern(TypeData::TypeParameter(tp_info));
    // template: [X[I]]
    let template = interner.tuple(vec![req_elem(interner.index_access(source, tp))]);
    let mapped = MappedType {
        type_param: tp_info,
        constraint: keyof,
        name_type: None,
        template,
        optional_modifier: None,
        readonly_modifier: None,
    };
    let result = evaluate_type(&interner, interner.mapped(mapped));

    let expected_prefix = interner.tuple(vec![req_elem(TypeId::STRING)]); // [string]
    let expected_rest_inner = interner.tuple(vec![req_elem(TypeId::NUMBER)]); // [number]
    let expected = interner.tuple(vec![
        req_elem(expected_prefix),
        rest_elem(interner.array(expected_rest_inner)),
    ]);
    assert_eq!(
        result, expected,
        "wrapping mapper should give [[string], ...[number][]]"
    );
}

#[test]
fn identity_mapped_fixed_tuple_and_plain_array_controls_roundtrip() {
    // CONTROLS: a fixed tuple and a plain array were never affected and must
    // continue to roundtrip cleanly.
    let interner = TypeInterner::new();

    let fixed = interner.tuple(vec![req_elem(TypeId::STRING), req_elem(TypeId::NUMBER)]);
    assert_eq!(
        eval_identity_mapped(&interner, "I", fixed),
        fixed,
        "identity over fixed tuple [string, number] should roundtrip"
    );

    let plain = interner.array(TypeId::NUMBER);
    assert_eq!(
        eval_identity_mapped(&interner, "I", plain),
        plain,
        "identity over number[] should roundtrip"
    );
}

#[test]
fn identity_mapped_only_rewrites_first_rest_of_multi_rest_tuple() {
    // GUARD: the per-position rule is only safe for the FIRST rest element,
    // whose prefix is all fixed. A tuple with a second rest (e.g. from a
    // generic variadic spread `[string, ...U, ...boolean[]]`) must NOT use
    // positional indexing for the later rest, since `tuple_index_literal`
    // short-circuits on the first rest it meets. Here we synthesize two
    // array rests directly: the first must map precisely to `number[]`; the
    // later rest must fall back to the original behavior (no crash, stays a
    // rest element) rather than borrowing the first rest's index.
    let interner = TypeInterner::new();
    let source = interner.tuple(vec![
        req_elem(TypeId::STRING),
        rest_elem(interner.array(TypeId::NUMBER)),
        rest_elem(interner.array(TypeId::BOOLEAN)),
    ]);

    let result = eval_identity_mapped(&interner, "I", source);
    let Some(TypeData::Tuple(list_id)) = interner.lookup(result) else {
        panic!("expected tuple result, got {:?}", interner.lookup(result));
    };
    let elements = interner.tuple_list(list_id);
    assert_eq!(elements.len(), 3);
    assert!(elements[1].rest && elements[2].rest);
    assert_eq!(
        elements[1].type_id,
        interner.array(TypeId::NUMBER),
        "first rest must map precisely to number[] via the per-position rule"
    );
    // The later rest keeps the original (union-based) behavior; we only assert
    // it remains a rest element rather than pinning the legacy union shape.
}
