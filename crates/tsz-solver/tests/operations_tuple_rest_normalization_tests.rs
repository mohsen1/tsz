use super::*;
use crate::def::{DefId, DefKind};
use crate::intern::TypeInterner;
use crate::relations::compat::CompatChecker;
use crate::relations::subtype::{TypeEnvironment, TypeResolver};
use crate::types::{FunctionShape, ParamInfo, TupleElement, TypeData, TypeId, TypeParamInfo};

struct ResolverAwareCompat<'a> {
    interner: &'a TypeInterner,
    resolver: &'a TypeEnvironment,
}

impl AssignabilityChecker for ResolverAwareCompat<'_> {
    fn is_assignable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        CompatChecker::with_resolver(self.interner, self.resolver).is_assignable(source, target)
    }

    fn evaluate_type(&mut self, type_id: TypeId) -> TypeId {
        crate::evaluation::evaluate::evaluate_type_with_resolver(
            self.interner,
            self.resolver,
            type_id,
        )
    }

    fn type_resolver(&self) -> Option<&dyn TypeResolver> {
        Some(self.resolver)
    }
}

#[test]
fn infer_generic_tuple_rest_from_rest_argument_returns_array() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(interner.array(TypeId::ANY)),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let tuple_t = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: t_type,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: tuple_t,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let string_array = interner.array(TypeId::STRING);
    let tuple_arg = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let result = infer_generic_function(&interner, &mut subtype, &func, &[tuple_arg]);

    assert_eq!(result, string_array, "T should be inferred as string[]");
}

#[test]
fn noinfer_tuple_rest_keeps_aggregate_arity_and_argument_checking() {
    let interner = TypeInterner::new();
    let tuple = interner.tuple(vec![
        TupleElement::fixed(TypeId::STRING),
        TupleElement::fixed(TypeId::NUMBER),
    ]);
    let noinfer_tuple = interner.no_infer(tuple);
    let func = FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: noinfer_tuple,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    };

    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);
    assert_eq!(
        evaluator.arg_count_bounds(&func.params, &func.type_params),
        (0, None),
        "`NoInfer` blocks tuple-rest arity exposure"
    );

    let mismatch = evaluator.resolve_function_call(&func, &[TypeId::BOOLEAN, TypeId::NUMBER]);
    let CallResult::ArgumentTypeMismatch {
        index,
        expected,
        actual,
        ..
    } = mismatch
    else {
        panic!("expected aggregate tuple mismatch, got {mismatch:?}");
    };
    assert_eq!(index, 0);
    assert_eq!(
        expected, noinfer_tuple,
        "the relation receives the intact `NoInfer<[string, number]>` wrapper"
    );
    let Some(TypeData::Tuple(elements)) = interner.lookup(actual) else {
        panic!("remaining arguments must be packed into one tuple");
    };
    assert_eq!(
        interner
            .tuple_list(elements)
            .iter()
            .map(|element| element.type_id)
            .collect::<Vec<_>>(),
        vec![TypeId::BOOLEAN, TypeId::NUMBER]
    );

    assert!(matches!(
        evaluator.resolve_function_call(&func, &[TypeId::STRING, TypeId::NUMBER]),
        CallResult::Success(TypeId::BOOLEAN)
    ));
}

#[test]
fn aliased_noinfer_tuple_rest_keeps_aggregate_shape_with_resolver() {
    let interner = TypeInterner::new();
    let tuple = interner.tuple(vec![
        TupleElement::fixed(TypeId::STRING),
        TupleElement::fixed(TypeId::NUMBER),
    ]);
    let noinfer_tuple = interner.no_infer(tuple);
    let alias_def = DefId(200);
    let mut resolver = TypeEnvironment::new();
    resolver.insert_def(alias_def, noinfer_tuple);
    resolver.insert_def_kind(alias_def, DefKind::TypeAlias);
    let func = FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: interner.lazy(alias_def),
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    };

    let mut checker = ResolverAwareCompat {
        interner: &interner,
        resolver: &resolver,
    };
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);
    assert_eq!(
        evaluator.arg_count_bounds(&func.params, &func.type_params),
        (0, None)
    );
    let mismatch = evaluator.resolve_function_call(&func, &[TypeId::BOOLEAN, TypeId::NUMBER]);
    let CallResult::ArgumentTypeMismatch {
        index,
        expected,
        actual,
        ..
    } = mismatch
    else {
        panic!("expected aggregate tuple mismatch, got {mismatch:?}");
    };
    assert_eq!(index, 0);
    assert_eq!(
        expected, noinfer_tuple,
        "alias exposure must stop at the outer `NoInfer` wrapper"
    );
    assert!(matches!(interner.lookup(actual), Some(TypeData::Tuple(_))));
}
