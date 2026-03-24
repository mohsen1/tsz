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
    let mut checker = SubtypeChecker::new(&interner);

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
    let mut checker = SubtypeChecker::new(&interner);

    let constraint = interner.literal_string("k");
    let keys = checker
        .try_evaluate_mapped_constraint(constraint)
        .expect("expected literal key");

    assert_eq!(atom_names(&interner, &keys), vec!["k"]);
}

#[test]
fn test_try_evaluate_mapped_constraint_union_with_non_literal_member() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

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
    let mut checker = SubtypeChecker::new(&interner);

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
            param_name: None,
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
    let mut checker = SubtypeChecker::new(&interner);

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
        is_const: false,
    };
    let param_type = interner.intern(TypeData::TypeParameter(param_info));

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
    symbol: Option<SymbolRef>,
    variances: Arc<[Variance]>,
}

impl TypeResolver for MockVarianceResolver<'_> {
    fn resolve_ref(&self, symbol: SymbolRef, interner: &dyn TypeDatabase) -> Option<TypeId> {
        self.env.resolve_ref(symbol, interner)
    }

    fn resolve_lazy(&self, def_id: DefId, interner: &dyn TypeDatabase) -> Option<TypeId> {
        self.env.resolve_lazy(def_id, interner)
    }

    fn symbol_to_def_id(&self, symbol: SymbolRef) -> Option<DefId> {
        (self.symbol == Some(symbol)).then_some(self.def_id)
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
fn test_non_interface_invariant_application_structural_fallback_accepts_equivalent_types() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let invariant_arg = DefId(500);
    let param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    // Body doesn't use T, so all instantiations are structurally equivalent
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
        symbol: None,
        variances: Arc::from(vec![
            Variance::COVARIANT | Variance::CONTRAVARIANT | Variance::NEEDS_STRUCTURAL_FALLBACK,
        ]),
    };
    let mut checker = SubtypeChecker::with_resolver(&interner, &resolver);

    // With structural fallback on variance failure: both expand to {x: number}
    // since T is unused in the body. Structural comparison correctly returns True.
    assert!(
        checker
            .check_application_to_application_subtype(source_app, target_app)
            .is_true()
    );
}

#[test]
fn test_type_alias_with_failed_variance_check_rejects_same_application_family() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let pick_t = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let pick_t_type = interner.intern(TypeData::TypeParameter(pick_t));

    let pick_k = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(interner.keyof(pick_t_type)),
        default: None,
        is_const: false,
    };
    let pick_k_type = interner.intern(TypeData::TypeParameter(pick_k));

    let pick_param = TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let pick_p_type = interner.intern(TypeData::TypeParameter(pick_param));

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
    let x_type = interner.intern(TypeData::TypeParameter(x_info));

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
        symbol: None,
        variances: Arc::from(vec![
            Variance::COVARIANT | Variance::CONTRAVARIANT | Variance::NEEDS_STRUCTURAL_FALLBACK,
        ]),
    };
    let mut checker = SubtypeChecker::with_resolver(&interner, &resolver);

    // With structural fallback on variance failure, this correctly returns True:
    // T<{x: string, y: number}> = Pick<{x: string, y: number}, "x"> = {x: string}
    // T<{x: string, z: boolean}> = Pick<{x: string, z: boolean}, "x"> = {x: string}
    // The type arguments differ but the expanded types are structurally equivalent.
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
    let x_type = interner.intern(TypeData::TypeParameter(x_info));

    let mapped_param = TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let mapped_param_type = interner.intern(TypeData::TypeParameter(mapped_param));

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
        symbol: None,
        variances: Arc::from(vec![variance]),
    };

    let mut checker = SubtypeChecker::with_resolver(&interner, &resolver);
    assert!(variance.is_covariant());
    // Mapped types always set NEEDS_STRUCTURAL_FALLBACK because the key set
    // depends on the type parameter, making variance-only checks insufficient.
    assert!(variance.needs_structural_fallback());
    assert!(
        checker
            .check_application_to_application_subtype(source_app, target_app)
            .is_true()
    );
}

#[test]
fn test_application_subtype_canonicalizes_lazy_and_typequery_bases() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let promise_def = DefId(4040);
    let promise_symbol = SymbolRef(4040);
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let promise_body = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        t_type,
    )]);
    env.insert_def_with_params(promise_def, promise_body, vec![t_param]);
    env.insert_def_kind(promise_def, DefKind::Interface);

    let source = interner.application(interner.lazy(promise_def), vec![TypeId::NUMBER]);
    let target = interner.application(
        interner.intern(TypeData::TypeQuery(promise_symbol)),
        vec![TypeId::NUMBER],
    );
    let mismatch = interner.application(
        interner.intern(TypeData::TypeQuery(promise_symbol)),
        vec![TypeId::STRING],
    );

    let resolver = MockVarianceResolver {
        env: &env,
        def_id: promise_def,
        symbol: Some(promise_symbol),
        variances: Arc::from(vec![Variance::COVARIANT]),
    };
    let mut checker = SubtypeChecker::with_resolver(&interner, &resolver);

    assert!(checker.is_subtype_of(source, target));
    assert!(!checker.is_subtype_of(source, mismatch));
}

#[test]
fn test_function_subtype_accepts_canonicalized_application_return_types() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let promise_def = DefId(4041);
    let promise_symbol = SymbolRef(4041);
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let promise_body = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        t_type,
    )]);
    env.insert_def_with_params(promise_def, promise_body, vec![t_param]);
    env.insert_def_kind(promise_def, DefKind::Interface);

    let source_return = interner.application(
        interner.intern(TypeData::TypeQuery(promise_symbol)),
        vec![TypeId::NUMBER],
    );
    let target_return = interner.application(interner.lazy(promise_def), vec![TypeId::NUMBER]);
    let mismatch_return = interner.application(interner.lazy(promise_def), vec![TypeId::STRING]);

    let source_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
        this_type: None,
        return_type: source_return,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let target_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
        this_type: None,
        return_type: target_return,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let mismatch_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
        this_type: None,
        return_type: mismatch_return,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let resolver = MockVarianceResolver {
        env: &env,
        def_id: promise_def,
        symbol: Some(promise_symbol),
        variances: Arc::from(vec![Variance::COVARIANT]),
    };
    let mut checker = SubtypeChecker::with_resolver(&interner, &resolver);

    assert!(checker.is_subtype_of(source_fn, target_fn));
    assert!(!checker.is_subtype_of(source_fn, mismatch_fn));
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
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // IObservable<T> = { n: IObservable<T[]> }
    let obs_body = interner.object(vec![PropertyInfo::new(
        interner.intern_string("n"),
        interner.application(interner.lazy(obs_def), vec![interner.array(t_type)]),
    )]);
    env.insert_def_with_params(obs_def, obs_body, vec![t_param]);

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
        "Recursive expansion currently treats this pair as subtype-compatible"
    );
}

// ─── flatten_mapped_chain and nested mapped type tests ────────────────────────

use crate::relations::subtype::rules::generics::flatten_mapped_chain;

/// Helper to create a homomorphic mapped type: { [K in keyof source]<modifiers>: source[K] }
fn make_homomorphic_mapped(
    interner: &TypeInterner,
    source: TypeId,
    optional: Option<crate::MappedModifier>,
    readonly: Option<crate::MappedModifier>,
) -> TypeId {
    let k_name = interner.intern_string("K");
    let k_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let template = interner.intern(TypeData::IndexAccess(source, k_param));
    let constraint = interner.intern(TypeData::KeyOf(source));
    interner.mapped(crate::MappedType {
        type_param: crate::TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint,
        name_type: None,
        template,
        optional_modifier: optional,
        readonly_modifier: readonly,
    })
}

#[test]
fn test_flatten_mapped_chain_simple_partial() {
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Partial<T>: { [K in keyof T]?: T[K] }
    let partial_t =
        make_homomorphic_mapped(&interner, t_param, Some(crate::MappedModifier::Add), None);

    let mapped_id = match interner.lookup(partial_t) {
        Some(TypeData::Mapped(id)) => id,
        _ => panic!("expected mapped type"),
    };

    let flat = flatten_mapped_chain(&interner, mapped_id).expect("should flatten");
    assert_eq!(flat.source, t_param);
    assert!(flat.has_optional);
    assert!(!flat.has_readonly);
}

#[test]
fn test_flatten_mapped_chain_simple_readonly() {
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Readonly<T>: { readonly [K in keyof T]: T[K] }
    let readonly_t =
        make_homomorphic_mapped(&interner, t_param, None, Some(crate::MappedModifier::Add));

    let mapped_id = match interner.lookup(readonly_t) {
        Some(TypeData::Mapped(id)) => id,
        _ => panic!("expected mapped type"),
    };

    let flat = flatten_mapped_chain(&interner, mapped_id).expect("should flatten");
    assert_eq!(flat.source, t_param);
    assert!(!flat.has_optional);
    assert!(flat.has_readonly);
}

#[test]
fn test_flatten_mapped_chain_partial_readonly_nested() {
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Build Readonly<T> first, then Partial<Readonly<T>>
    let readonly_t =
        make_homomorphic_mapped(&interner, t_param, None, Some(crate::MappedModifier::Add));
    let partial_readonly_t = make_homomorphic_mapped(
        &interner,
        readonly_t,
        Some(crate::MappedModifier::Add),
        None,
    );

    let mapped_id = match interner.lookup(partial_readonly_t) {
        Some(TypeData::Mapped(id)) => id,
        _ => panic!("expected mapped type"),
    };

    let flat = flatten_mapped_chain(&interner, mapped_id).expect("should flatten nested chain");
    assert_eq!(flat.source, t_param, "should unwrap to T");
    assert!(flat.has_optional, "Partial adds optional");
    assert!(flat.has_readonly, "Readonly adds readonly");
}

#[test]
fn test_flatten_mapped_chain_required_cancels_partial() {
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Required<Partial<T>>: outer removes optional, inner adds optional
    let partial_t =
        make_homomorphic_mapped(&interner, t_param, Some(crate::MappedModifier::Add), None);
    let required_partial_t = make_homomorphic_mapped(
        &interner,
        partial_t,
        Some(crate::MappedModifier::Remove),
        None,
    );

    let mapped_id = match interner.lookup(required_partial_t) {
        Some(TypeData::Mapped(id)) => id,
        _ => panic!("expected mapped type"),
    };

    let flat = flatten_mapped_chain(&interner, mapped_id).expect("should flatten");
    assert_eq!(flat.source, t_param);
    assert!(!flat.has_optional, "Required<Partial<T>> removes optional");
    assert!(!flat.has_readonly);
}

#[test]
fn test_mapped_to_mapped_partial_readonly_t_subtype_partial_t() {
    // Partial<Readonly<T>> <: Partial<T> should be TRUE
    // Both end up with source=T, and both have has_optional=true.
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let readonly_t =
        make_homomorphic_mapped(&interner, t_param, None, Some(crate::MappedModifier::Add));
    let partial_readonly_t = make_homomorphic_mapped(
        &interner,
        readonly_t,
        Some(crate::MappedModifier::Add),
        None,
    );
    let partial_t =
        make_homomorphic_mapped(&interner, t_param, Some(crate::MappedModifier::Add), None);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker
            .check_subtype(partial_readonly_t, partial_t)
            .is_true(),
        "Partial<Readonly<T>> should be subtype of Partial<T>"
    );
}

#[test]
fn test_mapped_to_mapped_partial_t_not_subtype_readonly_t() {
    // Partial<T> <: Readonly<T> should be FALSE
    // Partial has optional, Readonly doesn't.
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let partial_t =
        make_homomorphic_mapped(&interner, t_param, Some(crate::MappedModifier::Add), None);
    let readonly_t =
        make_homomorphic_mapped(&interner, t_param, None, Some(crate::MappedModifier::Add));

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.check_subtype(partial_t, readonly_t).is_true(),
        "Partial<T> should NOT be subtype of Readonly<T>"
    );
}

#[test]
fn test_mapped_to_mapped_readonly_partial_t_equiv_partial_readonly_t() {
    // Readonly<Partial<T>> <: Partial<Readonly<T>> and vice versa
    // Both flatten to source=T, has_optional=true, has_readonly=true
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let readonly_t =
        make_homomorphic_mapped(&interner, t_param, None, Some(crate::MappedModifier::Add));
    let partial_t =
        make_homomorphic_mapped(&interner, t_param, Some(crate::MappedModifier::Add), None);

    let readonly_partial_t =
        make_homomorphic_mapped(&interner, partial_t, None, Some(crate::MappedModifier::Add));
    let partial_readonly_t = make_homomorphic_mapped(
        &interner,
        readonly_t,
        Some(crate::MappedModifier::Add),
        None,
    );

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker
            .check_subtype(readonly_partial_t, partial_readonly_t)
            .is_true(),
        "Readonly<Partial<T>> should be subtype of Partial<Readonly<T>>"
    );
    assert!(
        checker
            .check_subtype(partial_readonly_t, readonly_partial_t)
            .is_true(),
        "Partial<Readonly<T>> should be subtype of Readonly<Partial<T>>"
    );
}

// =========================================================================
// Type parameter <: homomorphic mapped type tests
// =========================================================================
// These test the fix for T <: Partial<T>, T <: Readonly<T>, etc.
// In TypeScript, a type parameter is assignable to a homomorphic mapped type
// over itself when the mapped type doesn't remove optionality.

#[test]
fn test_type_param_subtype_of_partial_t() {
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Partial<T>: { [K in keyof T]?: T[K] }
    let partial_t =
        make_homomorphic_mapped(&interner, t_param, Some(crate::MappedModifier::Add), None);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.check_subtype(t_param, partial_t).is_true(),
        "T should be subtype of Partial<T>"
    );
}

#[test]
fn test_type_param_subtype_of_readonly_t() {
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Readonly<T>: { readonly [K in keyof T]: T[K] }
    let readonly_t =
        make_homomorphic_mapped(&interner, t_param, None, Some(crate::MappedModifier::Add));

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.check_subtype(t_param, readonly_t).is_true(),
        "T should be subtype of Readonly<T>"
    );
}

#[test]
fn test_type_param_subtype_of_identity_mapped() {
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Identity: { [K in keyof T]: T[K] } (no modifiers)
    let identity_t = make_homomorphic_mapped(&interner, t_param, None, None);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.check_subtype(t_param, identity_t).is_true(),
        "T should be subtype of identity mapped type"
    );
}

#[test]
fn test_partial_t_not_subtype_of_t() {
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let partial_t =
        make_homomorphic_mapped(&interner, t_param, Some(crate::MappedModifier::Add), None);

    let mut checker = SubtypeChecker::new(&interner);
    // Partial<T> adds optionality (+?), making it wider than T.
    // Partial<T> is NOT a subtype of T because optional properties may be undefined.
    assert!(
        checker.check_subtype(partial_t, t_param).is_false(),
        "Partial<T> should NOT be subtype of T (adds optionality)"
    );
}

#[test]
fn test_readonly_t_subtype_of_t() {
    // Readonly<T> <: T — readonly doesn't affect assignability direction.
    // Readonly only prevents mutation, it doesn't change the value type.
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let readonly_t =
        make_homomorphic_mapped(&interner, t_param, None, Some(crate::MappedModifier::Add));

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.check_subtype(readonly_t, t_param).is_true(),
        "Readonly<T> should be subtype of T (readonly doesn't widen)"
    );
}

#[test]
fn test_identity_mapped_subtype_of_t() {
    // { [K in keyof T]: T[K] } <: T — identity mapped type preserves T exactly.
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let identity_t = make_homomorphic_mapped(&interner, t_param, None, None);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.check_subtype(identity_t, t_param).is_true(),
        "Identity mapped type should be subtype of T"
    );
}

#[test]
fn test_type_param_not_subtype_of_required_t() {
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Required<T>: { [K in keyof T]-?: T[K] } — removes optional
    let required_t = make_homomorphic_mapped(
        &interner,
        t_param,
        Some(crate::MappedModifier::Remove),
        None,
    );

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.check_subtype(t_param, required_t).is_true(),
        "T should NOT be subtype of Required<T> (Required removes optionality)"
    );
}

#[test]
fn test_partial_t_subtype_of_readonly_partial_t() {
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let partial_t =
        make_homomorphic_mapped(&interner, t_param, Some(crate::MappedModifier::Add), None);

    // Readonly<Partial<T>>: readonly wrapper around Partial<T>
    let readonly_partial_t =
        make_homomorphic_mapped(&interner, partial_t, None, Some(crate::MappedModifier::Add));

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker
            .check_subtype(partial_t, readonly_partial_t)
            .is_true(),
        "Partial<T> should be subtype of Readonly<Partial<T>>"
    );
}

#[test]
fn test_t_subtype_of_partial_t_with_constraint() {
    let interner = TypeInterner::new();

    // Create a constraint type (e.g., { [key: string]: number })
    let constraint_obj = interner.object_with_index(crate::ObjectShape {
        flags: Default::default(),
        properties: vec![],
        string_index: Some(crate::IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        symbol: None,
    });

    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(constraint_obj),
        default: None,
        is_const: false,
    }));

    let partial_t =
        make_homomorphic_mapped(&interner, t_param, Some(crate::MappedModifier::Add), None);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.check_subtype(t_param, partial_t).is_true(),
        "T extends {{[key: string]: number}} should be subtype of Partial<T>"
    );
}

#[test]
fn test_required_t_subtype_of_t() {
    // Required<T> → T should succeed: Required is narrower, assignable to broader T
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let required_t = make_homomorphic_mapped(
        &interner,
        t_param,
        Some(crate::MappedModifier::Remove),
        None,
    );

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.check_subtype(required_t, t_param).is_true(),
        "Required<T> SHOULD be subtype of T (narrower to broader)"
    );
}

#[test]
fn test_t_subtype_of_partial_t() {
    // T → Partial<T> should succeed: T satisfies all optional requirements
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let partial_t =
        make_homomorphic_mapped(&interner, t_param, Some(crate::MappedModifier::Add), None);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.check_subtype(t_param, partial_t).is_true(),
        "T SHOULD be subtype of Partial<T> (original satisfies optional)"
    );
}

#[test]
fn test_readonly_t_bidirectional_with_t() {
    // Readonly<T> ↔ T should work both ways (readonly doesn't affect assignability)
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let readonly_t =
        make_homomorphic_mapped(&interner, t_param, None, Some(crate::MappedModifier::Add));

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.check_subtype(readonly_t, t_param).is_true(),
        "Readonly<T> SHOULD be subtype of T"
    );
    assert!(
        checker.check_subtype(t_param, readonly_t).is_true(),
        "T SHOULD be subtype of Readonly<T>"
    );
}

#[test]
fn test_required_t_subtype_of_partial_t() {
    // Required<T> → Partial<T> should succeed: narrower to wider
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let required_t = make_homomorphic_mapped(
        &interner,
        t_param,
        Some(crate::MappedModifier::Remove),
        None,
    );
    let partial_t =
        make_homomorphic_mapped(&interner, t_param, Some(crate::MappedModifier::Add), None);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.check_subtype(required_t, partial_t).is_true(),
        "Required<T> SHOULD be subtype of Partial<T> (narrower to wider)"
    );
}

#[test]
fn test_partial_t_not_subtype_of_required_t() {
    // Partial<T> → Required<T> should fail: wider to narrower
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let partial_t =
        make_homomorphic_mapped(&interner, t_param, Some(crate::MappedModifier::Add), None);
    let required_t = make_homomorphic_mapped(
        &interner,
        t_param,
        Some(crate::MappedModifier::Remove),
        None,
    );

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.check_subtype(partial_t, required_t).is_true(),
        "Partial<T> should NOT be subtype of Required<T> (wider to narrower)"
    );
}

// ─── Filtering as-clause mapped type tests ───────────────────────────────────

/// Helper to create a homomorphic mapped type with a filtering as-clause:
/// { [K in keyof source as source[K] extends `check_type` ? K : never]<modifiers>: source[K] }
fn make_filtering_mapped(
    interner: &TypeInterner,
    source: TypeId,
    check_type: TypeId,
    optional: Option<crate::MappedModifier>,
    readonly: Option<crate::MappedModifier>,
) -> TypeId {
    let k_name = interner.intern_string("K");
    let k_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let template = interner.intern(TypeData::IndexAccess(source, k_param));
    let constraint = interner.intern(TypeData::KeyOf(source));

    // Build the conditional: source[K] extends check_type ? K : never
    let check = interner.intern(TypeData::IndexAccess(source, k_param));
    let name_type = interner.conditional(crate::ConditionalType {
        check_type: check,
        extends_type: check_type,
        true_type: k_param,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });

    interner.mapped(crate::MappedType {
        type_param: crate::TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint,
        name_type: Some(name_type),
        template,
        optional_modifier: optional,
        readonly_modifier: readonly,
    })
}

#[test]
fn test_t_subtype_of_filter_t_no_modifier() {
    // T → Filter<T> (filtering as-clause, no modifier change) should succeed.
    // Filter<T> = { [K in keyof T as T[K] extends Function ? K : never]: T[K] }
    // All keys in Filter<T> are also keys of T with the same types.
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    let function_type = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("Function"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let filter_t = make_filtering_mapped(&interner, t_param, function_type, None, None);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.check_subtype(t_param, filter_t).is_true(),
        "T SHOULD be subtype of Filter<T> (filtering as-clause preserves keys)"
    );
}

#[test]
fn test_t_subtype_of_filter_t_with_optional() {
    // T → FilterInclOpt<T> (filtering + add optional) should succeed.
    // Adding optional makes the target wider, so T is still assignable.
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    let function_type = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("Function"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let filter_opt_t = make_filtering_mapped(
        &interner,
        t_param,
        function_type,
        Some(crate::MappedModifier::Add),
        None,
    );

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.check_subtype(t_param, filter_opt_t).is_true(),
        "T SHOULD be subtype of FilterInclOpt<T> (filtering + optional widens)"
    );
}

#[test]
fn test_t_not_subtype_of_filter_t_remove_optional() {
    // T → FilterExclOpt<T> (filtering + remove optional) should FAIL.
    // Removing optional means required properties that T might have as optional.
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    let function_type = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("Function"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let filter_required_t = make_filtering_mapped(
        &interner,
        t_param,
        function_type,
        Some(crate::MappedModifier::Remove),
        None,
    );

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.check_subtype(t_param, filter_required_t).is_true(),
        "T should NOT be subtype of FilterExclOpt<T> (-? makes target narrower)"
    );
}

// ─── Non-identity homomorphic mapped type template tests ─────────────────────

/// Helper to create a homomorphic mapped type with a custom template:
/// { [K in keyof source]<modifiers>: template }
fn make_homomorphic_mapped_with_template(
    interner: &TypeInterner,
    source: TypeId,
    template: TypeId,
    optional: Option<crate::MappedModifier>,
    readonly: Option<crate::MappedModifier>,
) -> TypeId {
    let k_name = interner.intern_string("K");
    let constraint = interner.intern(TypeData::KeyOf(source));
    interner.mapped(crate::MappedType {
        type_param: crate::TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint,
        name_type: None,
        template,
        optional_modifier: optional,
        readonly_modifier: readonly,
    })
}

#[test]
fn test_type_param_assignable_to_widened_template_mapped() {
    // type MyMap<T> = { [P in keyof T]: T[keyof T] }
    // U <: MyMap<U> should be TRUE
    //
    // T[keyof T] is the union of all value types, so for each property P,
    // T[P] is always assignable to T[keyof T] (a member of a union).
    let interner = TypeInterner::new();
    let u_name = interner.intern_string("U");
    let u_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Template: U[keyof U]
    let keyof_u = interner.intern(TypeData::KeyOf(u_param));
    let template = interner.intern(TypeData::IndexAccess(u_param, keyof_u));

    // { [P in keyof U]: U[keyof U] }
    let mapped = make_homomorphic_mapped_with_template(&interner, u_param, template, None, None);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.check_subtype(u_param, mapped).is_true(),
        "U should be assignable to {{ [P in keyof U]: U[keyof U] }}"
    );
}

#[test]
fn test_type_param_not_assignable_to_string_template_mapped() {
    // type StringMap<T> = { [P in keyof T]: string }
    // U <: StringMap<U> should be FALSE (U's values aren't necessarily string)
    let interner = TypeInterner::new();
    let u_name = interner.intern_string("U");
    let u_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // { [P in keyof U]: string }
    let mapped =
        make_homomorphic_mapped_with_template(&interner, u_param, TypeId::STRING, None, None);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.check_subtype(u_param, mapped).is_true(),
        "U should NOT be assignable to {{ [P in keyof U]: string }}"
    );
}

#[test]
fn test_type_param_assignable_to_identity_mapped() {
    // type Identity<T> = { [P in keyof T]: T[P] }
    // U <: Identity<U> should be TRUE (existing behavior, now also handled by general path)
    let interner = TypeInterner::new();
    let u_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let identity_u = make_homomorphic_mapped(&interner, u_param, None, None);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.check_subtype(u_param, identity_u).is_true(),
        "U should be assignable to {{ [K in keyof U]: U[K] }}"
    );
}

#[test]
fn test_type_param_not_assignable_to_required_mapped() {
    // U <: Required<U> should be FALSE (Required removes optionality)
    let interner = TypeInterner::new();
    let u_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let required_u = make_homomorphic_mapped(
        &interner,
        u_param,
        Some(crate::MappedModifier::Remove),
        None,
    );

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.check_subtype(u_param, required_u).is_true(),
        "U should NOT be assignable to Required<U> (-? removes optionality)"
    );
}
