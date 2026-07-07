//! JSDoc construction boundary.
//!
//! JSDoc resolution code parses source text and gathers structural facts. This
//! module owns the solver shape literals and interning for the object/function
//! types those facts describe.

use tsz_common::Atom;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::{
    FunctionShape, IndexSignature, ObjectShape, ParamInfo, PropertyInfo, TypeId, TypeParamInfo,
    TypePredicate,
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
