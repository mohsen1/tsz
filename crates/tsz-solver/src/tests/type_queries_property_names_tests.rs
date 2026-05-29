use crate::construction::TypeInterner;
use crate::def::DefId;
use crate::evaluation::evaluate::TypeEvaluator;
use crate::relations::subtype::TypeEnvironment;
use crate::type_queries::{
    collect_homomorphic_source_property_infos,
    collect_homomorphic_source_property_infos_with_evaluator,
    collect_property_name_atoms_for_diagnostics, keyof_object_properties,
};
use crate::{PropertyInfo, TypeData, TypeId, TypeParamInfo, Visibility};

fn object_with_property(interner: &TypeInterner, name: &str) -> TypeId {
    interner.object(vec![PropertyInfo {
        name: interner.intern_string(name),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }])
}

#[test]
fn collect_property_name_atoms_for_diagnostics_is_unique_and_transitive() {
    let interner = TypeInterner::new();
    let a = object_with_property(&interner, "a");
    let b = object_with_property(&interner, "b");
    let b_dupe = object_with_property(&interner, "b");
    let union = interner.union(vec![a, b, b_dupe]);

    let atoms = collect_property_name_atoms_for_diagnostics(&interner, union, 5);
    let mut names: Vec<String> = atoms
        .into_iter()
        .map(|atom| interner.resolve_atom_ref(atom).to_string())
        .collect();
    names.sort();

    assert_eq!(names, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn collect_property_name_atoms_for_diagnostics_honors_depth_limit() {
    let interner = TypeInterner::new();
    let a = object_with_property(&interner, "a");
    let b = object_with_property(&interner, "b");
    let nested = interner.union(vec![a, b]);
    let root = interner.union(vec![nested]);

    assert!(
        collect_property_name_atoms_for_diagnostics(&interner, root, 0).is_empty(),
        "depth limit should stop before reaching object members"
    );

    let atoms = collect_property_name_atoms_for_diagnostics(&interner, root, 1);
    let mut names: Vec<String> = atoms
        .into_iter()
        .map(|atom| interner.resolve_atom_ref(atom).to_string())
        .collect();
    names.sort();
    assert_eq!(names, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn keyof_object_properties_excludes_non_public_members() {
    let interner = TypeInterner::new();
    let obj = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("visible"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
        PropertyInfo {
            name: interner.intern_string("#hidden"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Private,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
        PropertyInfo {
            name: interner.intern_string("secret"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Protected,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
    ]);

    let keyof = keyof_object_properties(&interner, obj).expect("expected object keyof");
    let members = match interner.lookup(keyof) {
        Some(crate::TypeData::Union(list)) => interner.type_list(list).to_vec(),
        Some(crate::TypeData::Literal(crate::LiteralValue::String(_))) => vec![keyof],
        other => panic!("expected string literal or union for keyof object, got {other:?}"),
    };

    let names: Vec<_> = members
        .into_iter()
        .map(|member| match interner.lookup(member) {
            Some(crate::TypeData::Literal(crate::LiteralValue::String(atom))) => {
                interner.resolve_atom_ref(atom).to_string()
            }
            other => panic!("expected string literal member, got {other:?}"),
        })
        .collect();
    assert_eq!(names, vec!["visible"]);
}

#[test]
fn keyof_object_properties_preserves_declaration_order() {
    // Object shapes are stored sorted by atom for hash consistency, but
    // `keyof T`'s union must reflect source declaration order so that the
    // type printer matches tsc (e.g., `{ foo, bar }` -> `"foo" | "bar"`,
    // not the alphabetical `"bar" | "foo"`). This test uses property names
    // that sort opposite to declaration order ("xyz" before "abc" in source,
    // but "abc" sorts before "xyz" alphabetically).
    let interner = TypeInterner::new();
    // Intern atoms in declaration order so `Atom` IDs alone are not enough
    // to recover declaration order via the storage sort.
    let xyz_atom = interner.intern_string("xyzunique1");
    let abc_atom = interner.intern_string("abcunique2");
    let make_prop = |name, order| PropertyInfo {
        name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: order,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    };
    // Pass declaration_order explicitly to defeat the per-shape "auto-assign
    // 1..n in input order" fallback in the object constructor.
    let obj = interner.object(vec![make_prop(xyz_atom, 1), make_prop(abc_atom, 2)]);

    let keyof = keyof_object_properties(&interner, obj).expect("expected object keyof");
    let members = match interner.lookup(keyof) {
        Some(crate::TypeData::Union(list)) => interner.type_list(list).to_vec(),
        other => panic!("expected union for keyof of multi-property object, got {other:?}"),
    };

    let names: Vec<_> = members
        .into_iter()
        .map(|member| match interner.lookup(member) {
            Some(crate::TypeData::Literal(crate::LiteralValue::String(atom))) => {
                interner.resolve_atom_ref(atom).to_string()
            }
            other => panic!("expected string literal member, got {other:?}"),
        })
        .collect();
    assert_eq!(names, vec!["xyzunique1", "abcunique2"]);
}

#[test]
fn homomorphic_source_display_properties_preserve_declaration_order() {
    let interner = TypeInterner::new();
    let storage_first_atom = interner.intern_string("storageFirstUnique");
    let source_first_atom = interner.intern_string("sourceFirstUnique");
    let make_prop = |name, order| PropertyInfo {
        name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: order,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    };
    let source_first = make_prop(source_first_atom, 1);
    let storage_first = make_prop(storage_first_atom, 2);
    let obj = interner.object(vec![source_first.clone(), storage_first.clone()]);
    interner.store_display_properties(obj, vec![storage_first, source_first]);

    let ordered = collect_homomorphic_source_property_infos(&interner, obj);
    let names: Vec<_> = ordered
        .into_iter()
        .map(|prop| interner.resolve_atom_ref(prop.name).to_string())
        .collect();
    assert_eq!(names, vec!["sourceFirstUnique", "storageFirstUnique"]);
}

#[test]
fn homomorphic_array_source_prefers_es5_display_head() {
    let interner = TypeInterner::new();
    let make_prop = |name: &str, order| PropertyInfo {
        name: interner.intern_string(name),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: order,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    };
    let array_base = interner.object(vec![
        make_prop("includes", 1),
        make_prop("length", 2),
        make_prop("toLocaleString", 3),
        make_prop("toString", 4),
    ]);
    interner.set_array_display_base_type(array_base);
    interner.store_display_properties(
        array_base,
        vec![
            make_prop("includes", 1),
            make_prop("length", 2),
            make_prop("toLocaleString", 3),
            make_prop("toString", 4),
            make_prop("flatMap", 5),
            make_prop("flat", 6),
        ],
    );

    let source = interner.array(TypeId::NUMBER);
    let ordered = collect_homomorphic_source_property_infos(&interner, source);
    let names: Vec<_> = ordered
        .into_iter()
        .map(|prop| interner.resolve_atom_ref(prop.name).to_string())
        .collect();
    assert_eq!(
        &names[..4],
        ["length", "toString", "toLocaleString", "includes"]
    );
    assert!(names.iter().any(|name| name == "flatMap"));
    assert!(names.iter().any(|name| name == "flat"));
}

#[test]
fn homomorphic_array_source_uses_resolver_for_member_applications() {
    let interner = TypeInterner::new();
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let box_def = DefId(99);
    let array_base = interner.object(vec![PropertyInfo::new(
        interner.intern_string("wrapped"),
        interner.application(interner.lazy(box_def), vec![t_type]),
    )]);
    interner.set_array_base_type(array_base, vec![t_param]);

    let value_atom = interner.intern_string("value");
    let box_body = interner.object(vec![PropertyInfo::new(value_atom, t_type)]);
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(box_def, box_body, vec![t_param]);
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);

    let props = collect_homomorphic_source_property_infos_with_evaluator(
        &interner,
        interner.array(TypeId::NUMBER),
        &mut |type_id| evaluator.evaluate(type_id),
    );
    let wrapped = props
        .iter()
        .find(|prop| interner.resolve_atom_ref(prop.name).as_ref() == "wrapped")
        .expect("expected wrapped property");
    let expected = interner.object(vec![PropertyInfo::new(value_atom, TypeId::NUMBER)]);
    assert_eq!(wrapped.type_id, expected);
}
