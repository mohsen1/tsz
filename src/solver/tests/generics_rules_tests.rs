use super::*;
use crate::solver::def::DefId;
use crate::solver::visitor::application_id;

fn atom_names(interner: &TypeInterner, atoms: &[crate::interner::Atom]) -> Vec<String> {
    let mut names: Vec<String> = atoms
        .iter()
        .map(|atom| interner.resolve_atom(*atom).to_string())
        .collect();
    names.sort();
    names
}

#[test]
fn test_try_evaluate_mapped_constraint_keyof_object() {
    let interner = TypeInterner::new();
    let checker = SubtypeChecker::new(&interner);

    let obj = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("b"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let constraint = interner.intern(TypeKey::KeyOf(obj));
    let keys = checker
        .try_evaluate_mapped_constraint(constraint)
        .expect("expected concrete keys");

    assert_eq!(atom_names(&interner, &keys), vec!["a", "b"]);
}

#[test]
fn test_try_evaluate_mapped_constraint_string_literal() {
    let interner = TypeInterner::new();
    let checker = SubtypeChecker::new(&interner);

    let constraint = interner.literal_string("k");
    let keys = checker
        .try_evaluate_mapped_constraint(constraint)
        .expect("expected literal key");

    assert_eq!(atom_names(&interner, &keys), vec!["k"]);
}

#[test]
fn test_try_evaluate_mapped_constraint_union_with_non_literal_member() {
    let interner = TypeInterner::new();
    let checker = SubtypeChecker::new(&interner);

    let constraint = interner.union(vec![
        interner.literal_string("only"),
        interner.literal_number(1.0),
    ]);
    let keys = checker
        .try_evaluate_mapped_constraint(constraint)
        .expect("expected partial literal keys");

    assert_eq!(atom_names(&interner, &keys), vec!["only"]);
}

#[test]
fn test_try_get_keyof_keys_object_with_index_returns_properties() {
    let interner = TypeInterner::new();
    let checker = SubtypeChecker::new(&interner);

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name: interner.intern_string("x"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
        }),
        number_index: None,
    });

    let keys = checker
        .try_get_keyof_keys(obj)
        .expect("expected property keys");
    assert_eq!(atom_names(&interner, &keys), vec!["x"]);
}

#[test]
fn test_try_get_keyof_keys_empty_object_returns_none() {
    let interner = TypeInterner::new();
    let checker = SubtypeChecker::new(&interner);

    let obj = interner.object(vec![]);
    assert!(checker.try_get_keyof_keys(obj).is_none());
}

#[test]
fn test_try_get_keyof_keys_resolves_reference() {
    let interner = TypeInterner::new();

    let def_id = DefId(10);
    let resolved = interner.object(vec![PropertyInfo {
        name: interner.intern_string("value"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let mut env = TypeEnvironment::new();
    env.insert_def(def_id, resolved);
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    let ref_type = interner.lazy(def_id);
    let keys = checker
        .try_get_keyof_keys(ref_type)
        .expect("expected resolved keys");

    assert_eq!(atom_names(&interner, &keys), vec!["value"]);
}

#[test]
fn test_try_expand_application_instantiates_type_params() {
    let interner = TypeInterner::new();

    let param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
    };
    let param_type = interner.intern(TypeKey::TypeParameter(param_info.clone()));

    let def_id = DefId(20);
    let box_struct = interner.object(vec![PropertyInfo {
        name: interner.intern_string("value"),
        type_id: param_type,
        write_type: param_type,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(def_id, box_struct, vec![param_info]);
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    let base_ref = interner.lazy(def_id);
    let app_type = interner.application(base_ref, vec![TypeId::STRING]);
    let app_id = application_id(&interner, app_type).expect("expected app id");

    let expanded = checker
        .try_expand_application(app_id)
        .expect("expected expanded application");

    let Some(TypeKey::Object(shape_id)) = interner.lookup(expanded) else {
        panic!("expected expanded object type");
    };
    let shape = interner.object_shape(shape_id);
    let value_name = interner.intern_string("value");
    let value_prop = shape
        .properties
        .iter()
        .find(|prop| prop.name == value_name)
        .expect("expected value property");

    assert_eq!(value_prop.type_id, TypeId::STRING);
}

#[test]
fn test_try_expand_application_non_ref_base_returns_none() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let base = interner.object(vec![]);
    let app_type = interner.application(base, vec![TypeId::STRING]);
    let app_id = application_id(&interner, app_type).expect("expected app id");

    assert!(checker.try_expand_application(app_id).is_none());
}

#[test]
fn test_try_expand_application_self_reference_returns_none() {
    let interner = TypeInterner::new();

    let symbol = SymbolRef(30);
    let base_ref = interner.reference(symbol);
    let app_type = interner.application(base_ref, vec![TypeId::STRING]);
    let app_id = application_id(&interner, app_type).expect("expected app id");

    let mut env = TypeEnvironment::new();
    env.insert_with_params(
        symbol,
        app_type,
        vec![TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None,
            default: None,
        }],
    );

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    assert!(checker.try_expand_application(app_id).is_none());
}
