//! Class-property construction boundary.
//!
//! Class-property scanning and checking gather AST, JSDoc, modifier,
//! source-order, and declaration-owner facts. This module owns the solver
//! shape literals those facts become.

use tsz_binder::SymbolId;
use tsz_common::{Atom, Visibility};
use tsz_solver::construction::TypeDatabase;
use tsz_solver::{
    CallSignature, CallableShape, ObjectShape, ParamInfo, PropertyInfo, TypeId, TypeParamInfo,
    TypeParamOrigin,
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct JsClassPropertyFact {
    pub(crate) name: Atom,
    pub(crate) type_id: TypeId,
    pub(crate) write_type: TypeId,
    pub(crate) optional: bool,
    pub(crate) readonly: bool,
    pub(crate) is_method: bool,
    pub(crate) is_class_prototype: bool,
    pub(crate) visibility: Visibility,
    pub(crate) parent_id: Option<SymbolId>,
}

impl JsClassPropertyFact {
    pub(crate) const fn new(name: Atom, type_id: TypeId) -> Self {
        Self {
            name,
            type_id,
            write_type: type_id,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
        }
    }
}

pub(crate) const fn js_class_type_param_info(
    name: Atom,
    constraint: Option<TypeId>,
    default: Option<TypeId>,
    is_const: bool,
) -> TypeParamInfo {
    TypeParamInfo {
        name,
        constraint,
        default,
        is_const,
        origin: TypeParamOrigin::User,
    }
}

pub(crate) fn js_class_type_param_type(
    db: &dyn TypeDatabase,
    name: Atom,
    constraint: Option<TypeId>,
    default: Option<TypeId>,
    is_const: bool,
) -> TypeId {
    db.type_param(js_class_type_param_info(
        name, constraint, default, is_const,
    ))
}

pub(crate) fn js_class_array_type(db: &dyn TypeDatabase, element: TypeId) -> TypeId {
    db.array(element)
}

pub(crate) fn js_class_union_type(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.union(members)
}

pub(crate) fn js_class_union_pair_type(
    db: &dyn TypeDatabase,
    left: TypeId,
    right: TypeId,
) -> TypeId {
    db.union2(left, right)
}

pub(crate) fn class_property_optional_type_with_undefined(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    db.union2(type_id, TypeId::UNDEFINED)
}

pub(crate) fn static_readonly_unique_symbol_type(
    db: &dyn TypeDatabase,
    symbol: SymbolId,
) -> TypeId {
    db.unique_symbol(tsz_solver::SymbolRef(symbol.0))
}

pub(crate) const fn js_class_property_info(fact: JsClassPropertyFact) -> PropertyInfo {
    PropertyInfo {
        name: fact.name,
        type_id: fact.type_id,
        write_type: fact.write_type,
        optional: fact.optional,
        readonly: fact.readonly,
        is_method: fact.is_method,
        is_class_prototype: fact.is_class_prototype,
        visibility: fact.visibility,
        parent_id: fact.parent_id,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }
}

pub(crate) fn js_class_method_callable_type(db: &dyn TypeDatabase) -> TypeId {
    db.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![ParamInfo {
                name: None,
                type_id: TypeId::ANY,
                optional: false,
                rest: true,
                arity_only_optional: false,
            }],
            this_type: None,
            return_type: TypeId::ANY,
            type_predicate: None,
            is_method: true,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    })
}

pub(crate) fn js_class_instance_object_type(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    symbol: Option<SymbolId>,
) -> TypeId {
    db.object_with_index(ObjectShape {
        properties,
        symbol,
        ..ObjectShape::default()
    })
}
