use tsz_binder::SymbolId;
use tsz_common::interner::Atom;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::def::DefId;
use tsz_solver::{
    CallSignature, CallableShape, IndexSignature, ParamInfo, PropertyInfo, TypeId, TypeParamInfo,
    TypePredicate, TypePredicateTarget, Visibility,
};

pub(crate) use super::common::{
    array_element_type, callable_shape_for_type, construct_signatures_for_type,
    contains_conditional_type, has_function_shape, intersection_members, is_generic_mapped_type,
    is_generic_type, object_shape_for_type,
};

pub(crate) fn function_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::FunctionShape>> {
    tsz_solver::type_queries::get_function_shape(db, type_id)
}

/// Boundary for [`tsz_solver::type_queries::callable_requires_explicit_receiver`].
/// See the solver query for the structural rule.
pub(crate) fn callable_requires_explicit_receiver(
    db: &dyn TypeDatabase,
    callee_type: TypeId,
) -> bool {
    tsz_solver::type_queries::callable_requires_explicit_receiver(db, callee_type)
}

pub(crate) fn type_includes_undefined(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::type_includes_undefined(db, type_id)
}

pub(crate) fn type_parameter_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_type_parameter_constraint(db, type_id)
}

/// Check if `undefined` is potentially assignable to the given type.
///
/// This mirrors tsc's `isTypeAssignableTo(undefinedType, type)` for the purposes
/// of TS2564 checking. In particular:
/// - `undefined` is assignable to `any`, `unknown`, `void`, `undefined`
/// - `undefined` is assignable to unions containing `undefined`
///
/// TypeScript does NOT suppress TS2564 for naked type parameters, even when their
/// constraint is `any`, `unknown`, or includes `undefined`. Only the declared
/// property type itself matters here, not what a future instantiation might allow.
pub(crate) fn undefined_is_assignable_to(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::ANY
        || type_id == TypeId::UNKNOWN
        || type_id == TypeId::UNDEFINED
        || type_id == TypeId::VOID
    {
        return true;
    }

    // Check if type directly includes undefined (e.g., string | undefined)
    if type_includes_undefined(db, type_id) {
        return true;
    }

    false
}

pub(crate) fn merged_static_late_bound_index_value_type(
    db: &dyn TypeDatabase,
    existing: TypeId,
    incoming: TypeId,
) -> TypeId {
    db.union2(existing, incoming)
}

pub(crate) const fn static_late_bound_index_signature(
    key_type: TypeId,
    value_type: TypeId,
) -> IndexSignature {
    IndexSignature {
        key_type,
        value_type,
        readonly: false,
        param_name: None,
    }
}

pub(crate) fn partial_static_method_type(
    db: &dyn TypeDatabase,
    signatures: &[CallSignature],
) -> TypeId {
    db.callable(CallableShape {
        call_signatures: signatures.to_vec(),
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    })
}

pub(crate) const fn partial_static_method_property(
    name: Atom,
    type_id: TypeId,
    optional: bool,
    visibility: Visibility,
    parent_id: Option<SymbolId>,
) -> PropertyInfo {
    PropertyInfo {
        name,
        type_id,
        write_type: type_id,
        optional,
        readonly: false,
        is_method: true,
        is_class_prototype: false,
        visibility,
        parent_id,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }
}

pub(crate) const fn partial_static_accessor_property(
    name: Atom,
    read_type: TypeId,
    write_type: TypeId,
    readonly: bool,
    visibility: Visibility,
    parent_id: Option<SymbolId>,
) -> PropertyInfo {
    PropertyInfo {
        name,
        type_id: read_type,
        write_type,
        optional: false,
        readonly,
        is_method: false,
        is_class_prototype: false,
        visibility,
        parent_id,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }
}

pub(crate) const fn partial_static_placeholder_property(
    name: Atom,
    parent_id: Option<SymbolId>,
) -> PropertyInfo {
    PropertyInfo {
        name,
        type_id: TypeId::ANY,
        write_type: TypeId::ANY,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }
}

pub(crate) fn partial_static_constructor_callable_type(
    db: &dyn TypeDatabase,
    symbol: Option<SymbolId>,
    properties: Vec<PropertyInfo>,
    construct_signatures: &[CallSignature],
    string_index: Option<IndexSignature>,
    number_index: Option<IndexSignature>,
) -> TypeId {
    db.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures: construct_signatures.to_vec(),
        properties,
        string_index,
        number_index,
        symbol,
        is_abstract: false,
    })
}

pub(crate) fn class_constructor_companion_lazy_type(
    db: &dyn TypeDatabase,
    def_id: DefId,
) -> TypeId {
    db.lazy(def_id)
}

pub(crate) fn rough_self_instance_lazy_type(db: &dyn TypeDatabase, def_id: DefId) -> TypeId {
    db.lazy(def_id)
}

pub(crate) fn rough_self_instance_application_type(
    db: &dyn TypeDatabase,
    lazy_ref: TypeId,
    args: Vec<TypeId>,
) -> TypeId {
    db.application(lazy_ref, args)
}

pub(crate) const fn class_construct_param(
    name: Option<Atom>,
    type_id: TypeId,
    optional: bool,
    rest: bool,
) -> ParamInfo {
    ParamInfo {
        name,
        type_id,
        optional,
        rest,
    }
}

pub(crate) const fn class_type_predicate(
    asserts: bool,
    target: TypePredicateTarget,
    type_id: Option<TypeId>,
    parameter_index: Option<usize>,
) -> TypePredicate {
    TypePredicate {
        asserts,
        target,
        type_id,
        parameter_index,
    }
}

pub(crate) const fn class_construct_signature(
    type_params: Vec<TypeParamInfo>,
    params: Vec<ParamInfo>,
    this_type: Option<TypeId>,
    return_type: TypeId,
    type_predicate: Option<TypePredicate>,
    is_method: bool,
) -> CallSignature {
    CallSignature {
        type_params,
        params,
        this_type,
        return_type,
        type_predicate,
        is_method,
    }
}

pub(crate) fn enclosing_function_type_param_type(db: &dyn TypeDatabase, name: Atom) -> TypeId {
    db.type_param(TypeParamInfo::simple(name))
}
