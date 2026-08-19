//! Regression fences for the provisional (mid-build) class-instance registry
//! (#16055).
//!
//! While a class's partial instance snapshot is registered, evaluating an
//! application of that class stays OPAQUE — the evaluator must not
//! materialize members from the partial body, because the materialization
//! would be interned into durable composites beside the completed
//! representation (the zod `types.ts` false `TS2349`). Once the registration
//! clears — the checker publishes the completed instance or the build window
//! closes — the same application materializes normally.
//!
//! Binder names are varied across cases so the assertions pin structure, not
//! a hard-coded spelling.

use super::*;
use crate::construction::TypeDatabase;
use crate::intern::TypeInterner;
use crate::relations::subtype::TypeResolver;
use crate::type_queries::get_object_shape;
use crate::types::SymbolRef;
use crate::{DefId, TypeEvaluator};

struct ClassInstanceResolver {
    def_id: DefId,
    body: TypeId,
    params: Vec<TypeParamInfo>,
}

impl TypeResolver for ClassInstanceResolver {
    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn resolve_lazy(&self, def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        (def_id == self.def_id).then_some(self.body)
    }

    fn get_lazy_type_params(&self, def_id: DefId) -> Option<Vec<TypeParamInfo>> {
        (def_id == self.def_id).then(|| self.params.clone())
    }
}

/// Build a generic-class-like fixture: a body `{ member: P }` whose member
/// references the class's own type parameter, and the application
/// `Lazy(def)<string>` over it.
fn class_like_fixture(
    interner: &TypeInterner,
    param_name: &str,
    member_name: &str,
    def_id: DefId,
) -> (TypeId, TypeParamInfo, TypeId) {
    let param = TypeParamInfo {
        name: interner.intern_string(param_name),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let param_type = interner.type_param(param);
    let body = interner.object(vec![PropertyInfo::new(
        interner.intern_string(member_name),
        param_type,
    )]);
    let application = interner.application(interner.lazy(def_id), vec![TypeId::STRING]);
    (body, param, application)
}

#[test]
fn registered_provisional_instance_keeps_application_opaque() {
    for (param_name, member_name) in [("T", "value"), ("Elem", "payload")] {
        let interner = TypeInterner::new();
        let def_id = DefId(555_001);
        let (body, param, application) =
            class_like_fixture(&interner, param_name, member_name, def_id);
        let resolver = ClassInstanceResolver {
            def_id,
            body,
            params: vec![param],
        };

        interner.register_provisional_class_instance(body, def_id, vec![param].into());
        let mut evaluator = TypeEvaluator::with_resolver(&interner, &resolver);
        assert_eq!(
            evaluator.evaluate(application),
            application,
            "{param_name}/{member_name}: an application whose def resolves to a REGISTERED \
             provisional snapshot must stay opaque, not materialize the partial body",
        );
    }
}

#[test]
fn cleared_registration_materializes_the_application() {
    for (param_name, member_name) in [("T", "value"), ("Widened", "slot")] {
        let interner = TypeInterner::new();
        let def_id = DefId(555_002);
        let (body, param, application) =
            class_like_fixture(&interner, param_name, member_name, def_id);
        let resolver = ClassInstanceResolver {
            def_id,
            body,
            params: vec![param],
        };

        // Register, then clear — the publication / window-close sequence.
        interner.register_provisional_class_instance(body, def_id, vec![param].into());
        interner.unregister_provisional_class_instances_for_def(def_id);

        let mut evaluator = TypeEvaluator::with_resolver(&interner, &resolver);
        let result = evaluator.evaluate(application);
        assert_ne!(
            result, application,
            "{param_name}/{member_name}: once the registration clears, the application \
             must materialize (the negative case: a CLOSED window behaves as before)",
        );
        let shape = get_object_shape(&interner, result)
            .expect("materialized class application should be an object shape");
        let member_atom = interner.intern_string(member_name);
        let member = shape
            .properties
            .iter()
            .find(|property| property.name == member_atom)
            .expect("materialized body keeps the declared member");
        assert_eq!(
            member.type_id,
            TypeId::STRING,
            "materialization substitutes the class type parameter with the argument",
        );
    }
}

#[test]
fn never_registered_body_is_untouched_by_the_registry_gate() {
    let interner = TypeInterner::new();
    let def_id = DefId(555_003);
    let (body, param, application) = class_like_fixture(&interner, "Row", "cell", def_id);
    let resolver = ClassInstanceResolver {
        def_id,
        body,
        params: vec![param],
    };

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &resolver);
    let result = evaluator.evaluate(application);
    assert_ne!(
        result, application,
        "a def whose body was never registered as provisional evaluates as before",
    );
}
