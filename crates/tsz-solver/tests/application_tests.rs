use super::*;
use crate::TypeInterner;
use crate::relations::subtype::NoopResolver;
use crate::type_queries::get_object_shape;
use crate::types::SymbolRef;
use crate::{DefId, TypeDatabase, TypeEvaluator, TypeResolver};
use tsz_binder::SymbolId;

#[test]
fn test_non_application_passthrough() {
    let interner = TypeInterner::new();
    let string_type = interner.intern(TypeData::Intrinsic(IntrinsicKind::String));

    let evaluator = ApplicationEvaluator::new(&interner, &NoopResolver);
    let result = evaluator.evaluate(string_type);

    assert!(matches!(result, ApplicationResult::NotApplication(_)));
}

#[test]
fn test_primitives_are_not_applications() {
    let interner = TypeInterner::new();
    let evaluator = ApplicationEvaluator::new(&interner, &NoopResolver);

    // Primitives should pass through as NotApplication
    assert!(matches!(
        evaluator.evaluate(TypeId::ANY),
        ApplicationResult::NotApplication(_)
    ));
    assert!(matches!(
        evaluator.evaluate(TypeId::NEVER),
        ApplicationResult::NotApplication(_)
    ));
    assert!(matches!(
        evaluator.evaluate(TypeId::STRING),
        ApplicationResult::NotApplication(_)
    ));
}

#[test]
fn evaluator_recovers_def_id_from_symbol_stamped_application_base() {
    struct SymbolBackedResolver {
        symbol: SymbolId,
        def_id: DefId,
        body: TypeId,
        params: Vec<TypeParamInfo>,
    }

    impl TypeResolver for SymbolBackedResolver {
        fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
            None
        }

        fn resolve_lazy(&self, def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
            (def_id == self.def_id).then_some(self.body)
        }

        fn get_lazy_type_params(&self, def_id: DefId) -> Option<Vec<TypeParamInfo>> {
            (def_id == self.def_id).then(|| self.params.clone())
        }

        fn symbol_to_def_id(&self, symbol: SymbolRef) -> Option<DefId> {
            (symbol.0 == self.symbol.0).then_some(self.def_id)
        }
    }

    let interner = TypeInterner::new();
    let value_atom = interner.intern_string("value");
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.type_param(t_param);
    let symbol = SymbolId(42);
    let body = interner.object_with_flags_and_symbol(
        vec![PropertyInfo::new(value_atom, t_type)],
        ObjectFlags::empty(),
        Some(symbol),
    );
    let app_with_structural_base = interner.application(body, vec![TypeId::STRING]);
    let resolver = SymbolBackedResolver {
        symbol,
        def_id: DefId(7),
        body,
        params: vec![t_param],
    };

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &resolver);
    let evaluated = evaluator.evaluate(app_with_structural_base);
    let shape = get_object_shape(&interner, evaluated).expect("expected object result");
    let value = shape
        .properties
        .iter()
        .find(|prop| prop.name == value_atom)
        .expect("expected instantiated value property");

    assert_eq!(value.type_id, TypeId::STRING);
}
