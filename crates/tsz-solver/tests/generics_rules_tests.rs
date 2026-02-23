use super::*;
use crate::TypeInterner;
use crate::def::{DefId, DefKind};
use crate::visitor::application_id;
use std::sync::Arc;

fn atom_names(interner: &TypeInterner, atoms: &[tsz_common::interner::Atom]) -> Vec<String> {
    let mut names: Vec<String> = atoms
        .iter()
        .map(|atom| interner.resolve_atom(*atom))
        .collect();
    names.sort();
    names
}

#[test]
fn test_try_evaluate_mapped_constraint_keyof_object() {
    let interner = TypeInterner::new();
    let checker = SubtypeChecker::new(&interner);

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
    ]);

    let constraint = interner.intern(TypeData::KeyOf(obj));
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
        properties: vec![PropertyInfo::new(
            interner.intern_string("x"),
            TypeId::NUMBER,
        )],
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
    let resolved = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::STRING,
    )]);

    let mut env = TypeEnvironment::new();
    env.insert_def(def_id, resolved);
    let checker = SubtypeChecker::with_resolver(&interner, &env);

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
        is_const: false,
    };
    let param_type = interner.intern(TypeData::TypeParameter(param_info.clone()));

    let def_id = DefId(20);
    let box_struct = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        param_type,
    )]);

    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(def_id, box_struct, vec![param_info]);
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    let base_ref = interner.lazy(def_id);
    let app_type = interner.application(base_ref, vec![TypeId::STRING]);
    let app_id = application_id(&interner, app_type).expect("expected app id");

    let expanded = checker
        .try_expand_application(app_id)
        .expect("expected expanded application");

    let Some(TypeData::Object(shape_id)) = interner.lookup(expanded) else {
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
            is_const: false,
        }],
    );

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    assert!(checker.try_expand_application(app_id).is_none());
}

#[derive(Debug)]
struct MockVarianceResolver<'a> {
    env: &'a TypeEnvironment,
    def_id: DefId,
    variances: Arc<[Variance]>,
}

impl TypeResolver for MockVarianceResolver<'_> {
    fn resolve_ref(&self, symbol: SymbolRef, interner: &dyn TypeDatabase) -> Option<TypeId> {
        self.env.resolve_ref(symbol, interner)
    }

    fn resolve_lazy(&self, def_id: DefId, interner: &dyn TypeDatabase) -> Option<TypeId> {
        self.env.resolve_lazy(def_id, interner)
    }

    fn get_lazy_type_params(&self, def_id: DefId) -> Option<Vec<TypeParamInfo>> {
        self.env.get_lazy_type_params(def_id)
    }

    fn get_def_kind(&self, def_id: DefId) -> Option<crate::def::DefKind> {
        self.env.get_def_kind(def_id)
    }

    fn get_type_param_variance(&self, def_id: DefId) -> Option<Arc<[Variance]>> {
        if def_id == self.def_id {
            Some(Arc::clone(&self.variances))
        } else {
            None
        }
    }
}

#[test]
fn test_non_interface_invariant_application_fastpath_rejects_without_structural_fallback() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let invariant_arg = DefId(500);
    let param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let body = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    env.insert_def_with_params(invariant_arg, body, vec![param]);
    env.insert_def_kind(invariant_arg, DefKind::Class);

    let source = interner.application(interner.lazy(invariant_arg), vec![TypeId::STRING]);
    let target = interner.application(interner.lazy(invariant_arg), vec![TypeId::NUMBER]);
    let source_app = application_id(&interner, source).expect("application id");
    let target_app = application_id(&interner, target).expect("application id");

    let resolver = MockVarianceResolver {
        env: &env,
        def_id: invariant_arg,
        variances: Arc::from(vec![Variance::COVARIANT | Variance::CONTRAVARIANT]),
    };
    let mut checker = SubtypeChecker::with_resolver(&interner, &resolver);

    assert!(
        checker
            .check_application_to_application_subtype(source_app, target_app)
            .is_false()
    );
}

#[test]
fn test_type_alias_with_failed_variance_check_still_uses_structural_expansion() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let pick_t = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let pick_t_type = interner.intern(TypeData::TypeParameter(pick_t.clone()));

    let pick_k = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(interner.keyof(pick_t_type)),
        default: None,
        is_const: false,
    };
    let pick_k_type = interner.intern(TypeData::TypeParameter(pick_k.clone()));

    let pick_param = TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let pick_p_type = interner.intern(TypeData::TypeParameter(pick_param.clone()));

    let pick_body = interner.mapped(MappedType {
        type_param: pick_param,
        constraint: pick_k_type,
        name_type: None,
        template: interner.index_access(pick_t_type, pick_p_type),
        readonly_modifier: None,
        optional_modifier: None,
    });

    let pick_def = DefId(1000);
    env.insert_def_with_params(pick_def, pick_body, vec![pick_t, pick_k]);
    env.insert_def_kind(pick_def, DefKind::TypeAlias);

    let x_name = interner.intern_string("X");
    let x_constraint = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::ANY,
    )]);
    let x_info = TypeParamInfo {
        name: x_name,
        constraint: Some(x_constraint),
        default: None,
        is_const: false,
    };
    let x_type = interner.intern(TypeData::TypeParameter(x_info.clone()));

    let pick_x_alias = interner.application(
        interner.lazy(pick_def),
        vec![x_type, interner.literal_string("x")],
    );

    let t_def = DefId(1001);
    env.insert_def_with_params(t_def, pick_x_alias, vec![x_info]);
    env.insert_def_kind(t_def, DefKind::TypeAlias);

    let source_arg = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("y"), TypeId::NUMBER),
    ]);
    let target_arg = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("z"), TypeId::BOOLEAN),
    ]);

    let source = interner.application(interner.lazy(t_def), vec![source_arg]);
    let target = interner.application(interner.lazy(t_def), vec![target_arg]);
    let source_app = application_id(&interner, source).expect("source app id");
    let target_app = application_id(&interner, target).expect("target app id");

    let resolver = MockVarianceResolver {
        env: &env,
        def_id: t_def,
        variances: Arc::from(vec![Variance::COVARIANT | Variance::CONTRAVARIANT]),
    };
    let mut checker = SubtypeChecker::with_resolver(&interner, &resolver);

    assert!(
        checker
            .check_application_to_application_subtype(source_app, target_app)
            .is_true()
    );
}

#[test]
fn test_mapped_generic_parameter_with_indexed_access_is_covariant() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let x_name = interner.intern_string("X");
    let x_constraint = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::ANY,
    )]);
    let x_info = TypeParamInfo {
        name: x_name,
        constraint: Some(x_constraint),
        default: None,
        is_const: false,
    };
    let x_type = interner.intern(TypeData::TypeParameter(x_info.clone()));

    let mapped_param = TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let mapped_param_type = interner.intern(TypeData::TypeParameter(mapped_param.clone()));

    let mapped_body = interner.mapped(MappedType {
        type_param: mapped_param,
        constraint: interner.keyof(x_type),
        name_type: None,
        template: interner.index_access(x_type, mapped_param_type),
        readonly_modifier: None,
        optional_modifier: None,
    });

    let mapped_def = DefId(900);
    env.insert_def_with_params(mapped_def, mapped_body, vec![x_info]);
    env.insert_def_kind(mapped_def, DefKind::TypeAlias);

    let source_arg = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::STRING,
    )]);
    let target_arg = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
    )]);
    let mapped_source = interner.application(interner.lazy(mapped_def), vec![source_arg]);
    let mapped_target = interner.application(interner.lazy(mapped_def), vec![target_arg]);
    let source_app = application_id(&interner, mapped_source).expect("source app id");
    let target_app = application_id(&interner, mapped_target).expect("target app id");

    let variance = crate::relations::variance::compute_variance(&interner, mapped_body, x_name);
    let resolver = MockVarianceResolver {
        env: &env,
        def_id: mapped_def,
        variances: Arc::from(vec![variance]),
    };

    let mut checker = SubtypeChecker::with_resolver(&interner, &resolver);
    assert!(variance.is_covariant());
    assert_eq!(variance, Variance::COVARIANT);
    assert!(
        checker
            .check_application_to_application_subtype(source_app, target_app)
            .is_true()
    );
}

#[test]
fn test_recursive_generic_extension_uses_structural_expansion_not_variance_arg_check() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let obs_def = DefId(200);
    let subject_def = DefId(201);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param.clone()));

    // IObservable<T> = { n: IObservable<T[]> }
    let obs_body = interner.object(vec![PropertyInfo::new(
        interner.intern_string("n"),
        interner.application(interner.lazy(obs_def), vec![interner.array(t_type)]),
    )]);
    env.insert_def_with_params(obs_def, obs_body, vec![t_param.clone()]);

    // ISubject<T> = IObservable<T>
    let subject_body = interner.application(interner.lazy(obs_def), vec![t_type]);
    env.insert_def_with_params(subject_def, subject_body, vec![t_param]);

    let foo = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::STRING,
    )]);
    let bar = interner.object(vec![PropertyInfo::new(
        interner.intern_string("y"),
        TypeId::STRING,
    )]);

    let source = interner.application(interner.lazy(subject_def), vec![bar]);
    let target = interner.application(interner.lazy(obs_def), vec![foo]);

    env.insert_def_kind(obs_def, DefKind::Interface);
    env.insert_def_kind(subject_def, DefKind::Interface);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    // ISubject<{y:string}> should be assignable to IObservable<{x:string}> via
    // coinductive cycle detection: recursive expansion of the shared base
    // IObservable hits the same (DefId, DefId) pair and terminates as CycleDetected (true).
    assert!(
        checker.check_subtype(source, target).is_true(),
        "ISubject<bar> should be subtype of IObservable<foo> via structural expansion with coinductive recursion"
    );
}
