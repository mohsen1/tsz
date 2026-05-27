//! Regression tests for #9709: a failed indexed access resolves to the error type.
//!
//! When an object/callable type is indexed by the bare `string`, `number`, or
//! `symbol` primitive and no property key or index signature applies, tsc reports
//! TS2536/TS2537 and resolves the access to the error type (bidirectionally
//! assignable, like `any`). These cases previously resolved to a concrete type
//! (`undefined`, or the union of all member value types for a `string` index),
//! which produced cascading false-positive diagnostics.
//!
//! Housed in a focused shard rather than the oversized `evaluate_tests.rs` /
//! `operations_tests.rs` files (AGENTS §19 2000-line limit).

use crate::evaluation::evaluate::evaluate_index_access;
use crate::intern::TypeInterner;
use crate::operations::infer_generic_function;
use crate::relations::compat::CompatChecker;
use crate::types::{FunctionShape, ParamInfo, PropertyInfo, TypeData, TypeId, TypeParamInfo};

#[test]
fn plain_object_string_index_resolves_to_error_type() {
    let interner = TypeInterner::new();

    // `{ x?: number; y: string }` has no string index signature, so indexing it by
    // the bare `string` type is a TS2536/TS2537 failure: tsc resolves the access to
    // the error type rather than the union of property types.
    let obj = interner.object(vec![
        PropertyInfo::opt(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    let result = evaluate_index_access(&interner, obj, TypeId::STRING);
    assert_eq!(result, TypeId::ERROR);
}

#[test]
fn infer_generic_index_access_param_from_index_access_arg_resolves_to_error_type() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let k_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let k_type = interner.intern(TypeData::TypeParameter(k_param));
    let index_access_param = interner.intern(TypeData::IndexAccess(t_type, k_type));

    let func = FunctionShape {
        type_params: vec![t_param, k_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: index_access_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: index_access_param,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let key_literal = interner.literal_string("value");
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::NUMBER,
    )]);
    let index_access_arg = interner.intern(TypeData::IndexAccess(obj, key_literal));

    let result = infer_generic_function(&interner, &mut subtype, &func, &[index_access_arg]);
    // `K` is unconstrained here (in tsc, `T[K]` with `K` not bounded by `keyof T` is
    // itself a TS2536 error). Inference cannot pin `K` to the `"value"` literal, so the
    // instantiated `T[K]` indexes the object by the bare key type, which matches no
    // index signature and resolves to the error type. The old result was the property
    // type only because the single-property object made `obj[string]` collapse to it.
    assert_eq!(result, TypeId::ERROR);
}
