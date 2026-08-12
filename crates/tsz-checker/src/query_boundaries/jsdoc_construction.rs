//! JSDoc construction boundary.
//!
//! JSDoc resolution code parses source text and gathers structural facts. This
//! module owns the solver shape literals and interning for the object/function
//! types those facts describe.

use tsz_common::Atom;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::def::DefId;
use tsz_solver::{
    ConditionalType, FunctionShape, IndexSignature, MappedModifier, MappedType, ObjectShape,
    ParamInfo, PropertyInfo, TupleElement, TypeId, TypeParamInfo, TypeParamOrigin, TypePredicate,
    TypePredicateTarget,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum JsdocObjectIndexKind {
    String,
    Number,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct JsdocObjectIndexFact {
    pub(crate) key_type: TypeId,
    pub(crate) value_type: TypeId,
    pub(crate) readonly: bool,
    pub(crate) param_name: Option<Atom>,
}

impl JsdocObjectIndexFact {
    const fn into_index_signature(self) -> IndexSignature {
        IndexSignature {
            key_type: self.key_type,
            value_type: self.value_type,
            readonly: self.readonly,
            param_name: self.param_name,
        }
    }
}

pub(crate) const fn jsdoc_object_index_fact(
    key_type: TypeId,
    value_type: TypeId,
    readonly: bool,
    param_name: Option<Atom>,
) -> Option<(JsdocObjectIndexKind, JsdocObjectIndexFact)> {
    let kind = match key_type {
        TypeId::STRING | TypeId::SYMBOL => JsdocObjectIndexKind::String,
        TypeId::NUMBER => JsdocObjectIndexKind::Number,
        _ => return None,
    };
    Some((
        kind,
        JsdocObjectIndexFact {
            key_type,
            value_type,
            readonly,
            param_name,
        },
    ))
}

pub(crate) fn jsdoc_empty_object_type(db: &dyn TypeDatabase) -> TypeId {
    db.object(Vec::new())
}

pub(crate) fn jsdoc_array_type(db: &dyn TypeDatabase, element: TypeId) -> TypeId {
    db.array(element)
}

pub(crate) fn jsdoc_union_type(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.union(members)
}

pub(crate) fn jsdoc_union_pair_type(db: &dyn TypeDatabase, left: TypeId, right: TypeId) -> TypeId {
    db.union2(left, right)
}

pub(crate) fn jsdoc_intersection_type(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.intersection(members)
}

pub(crate) fn jsdoc_intersection_pair_type(
    db: &dyn TypeDatabase,
    left: TypeId,
    right: TypeId,
) -> TypeId {
    db.intersection2(left, right)
}

pub(crate) fn jsdoc_application_type(
    db: &dyn TypeDatabase,
    base: TypeId,
    args: Vec<TypeId>,
) -> TypeId {
    db.application(base, args)
}

pub(crate) fn jsdoc_index_access_type(
    db: &dyn TypeDatabase,
    object_type: TypeId,
    index_type: TypeId,
) -> TypeId {
    db.index_access(object_type, index_type)
}

pub(crate) fn jsdoc_keyof_type(db: &dyn TypeDatabase, operand: TypeId) -> TypeId {
    db.keyof(operand)
}

pub(crate) fn jsdoc_lazy_type(db: &dyn TypeDatabase, def_id: DefId) -> TypeId {
    db.lazy(def_id)
}

pub(crate) fn jsdoc_readonly_type(db: &dyn TypeDatabase, inner: TypeId) -> TypeId {
    db.readonly_type(inner)
}

pub(crate) fn jsdoc_literal_string_type(db: &dyn TypeDatabase, value: &str) -> TypeId {
    db.literal_string(value)
}

pub(crate) fn jsdoc_literal_boolean_type(db: &dyn TypeDatabase, value: bool) -> TypeId {
    db.literal_boolean(value)
}

pub(crate) fn jsdoc_literal_number_type(db: &dyn TypeDatabase, value: f64) -> TypeId {
    db.literal_number(value)
}

pub(crate) const fn jsdoc_type_param_info(
    name: Atom,
    constraint: Option<TypeId>,
    default: Option<TypeId>,
) -> TypeParamInfo {
    TypeParamInfo {
        name,
        constraint,
        default,
        is_const: false,
        origin: TypeParamOrigin::User,
    }
}

pub(crate) fn jsdoc_type_param_type(db: &dyn TypeDatabase, type_param: TypeParamInfo) -> TypeId {
    db.type_param(type_param)
}

pub(crate) fn jsdoc_tuple_type(db: &dyn TypeDatabase, elements: Vec<TupleElement>) -> TypeId {
    db.tuple(elements)
}

pub(crate) const fn jsdoc_tuple_element(
    type_id: TypeId,
    name: Option<Atom>,
    optional: bool,
    rest: bool,
) -> TupleElement {
    TupleElement {
        type_id,
        name,
        optional,
        rest,
    }
}

pub(crate) const fn jsdoc_param_info(
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
        arity_only_optional: false,
    }
}

pub(crate) const fn jsdoc_property_info(
    name: Atom,
    type_id: TypeId,
    optional: bool,
    readonly: bool,
    is_method: bool,
    declaration_order: u32,
) -> PropertyInfo {
    PropertyInfo {
        name,
        type_id,
        write_type: type_id,
        optional,
        readonly,
        is_method,
        is_class_prototype: false,
        visibility: tsz_solver::Visibility::Public,
        parent_id: None,
        declaration_order,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }
}

pub(crate) const fn jsdoc_type_predicate(
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

pub(crate) fn jsdoc_mapped_type(
    db: &dyn TypeDatabase,
    type_param: TypeParamInfo,
    constraint: TypeId,
    template: TypeId,
    optional_modifier: Option<MappedModifier>,
) -> TypeId {
    db.mapped(MappedType {
        type_param,
        constraint,
        name_type: None,
        template,
        readonly_modifier: None,
        optional_modifier,
    })
}

pub(crate) fn jsdoc_conditional_type(
    db: &dyn TypeDatabase,
    check_type: TypeId,
    extends_type: TypeId,
    true_type: TypeId,
    false_type: TypeId,
) -> TypeId {
    db.conditional(ConditionalType {
        check_type,
        extends_type,
        true_type,
        false_type,
        is_distributive: true,
    })
}

pub(crate) fn jsdoc_object_type(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    string_index: Option<JsdocObjectIndexFact>,
    number_index: Option<JsdocObjectIndexFact>,
) -> Option<TypeId> {
    if properties.is_empty() && string_index.is_none() && number_index.is_none() {
        return None;
    }
    if string_index.is_some() || number_index.is_some() {
        Some(db.object_with_index(ObjectShape {
            properties,
            string_index: string_index.map(JsdocObjectIndexFact::into_index_signature),
            number_index: number_index.map(JsdocObjectIndexFact::into_index_signature),
            ..ObjectShape::default()
        }))
    } else {
        Some(db.object(properties))
    }
}

pub(crate) fn jsdoc_object_index_type(
    db: &dyn TypeDatabase,
    key_type: TypeId,
    value_type: TypeId,
    readonly: bool,
    param_name: Option<Atom>,
) -> Option<TypeId> {
    let (kind, fact) = jsdoc_object_index_fact(key_type, value_type, readonly, param_name)?;
    let (string_index, number_index) = match kind {
        JsdocObjectIndexKind::String => (Some(fact), None),
        JsdocObjectIndexKind::Number => (None, Some(fact)),
    };
    jsdoc_object_type(db, Vec::new(), string_index, number_index)
}

pub(crate) fn jsdoc_function_type(
    db: &dyn TypeDatabase,
    type_params: Vec<TypeParamInfo>,
    params: Vec<ParamInfo>,
    this_type: Option<TypeId>,
    return_type: TypeId,
    type_predicate: Option<TypePredicate>,
    is_constructor: bool,
    is_method: bool,
) -> TypeId {
    db.function(FunctionShape {
        type_params,
        params,
        this_type,
        return_type,
        type_predicate,
        is_constructor,
        is_method,
    })
}
