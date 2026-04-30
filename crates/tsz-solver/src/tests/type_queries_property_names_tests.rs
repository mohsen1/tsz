use crate::type_queries::{collect_property_name_atoms_for_diagnostics, keyof_object_properties};
use crate::{PropertyInfo, TypeId, TypeInterner, Visibility};

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
