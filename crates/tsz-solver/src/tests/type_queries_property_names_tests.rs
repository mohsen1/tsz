use crate::type_queries::collect_property_name_atoms_for_diagnostics;
use crate::{PropertyInfo, TypeId, TypeInterner, Visibility};

fn object_with_property(interner: &TypeInterner, name: &str) -> TypeId {
    interner.object(vec![PropertyInfo {
        name: interner.intern_string(name),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
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
