//! Object-literal result construction boundary.
//!
//! Object-literal computation owns AST traversal, spread policy, contextual
//! typing, property ordering, and display normalization. This module owns the
//! solver surfaces those decisions produce for final object literal results and
//! mapped-spread fallbacks.

use rustc_hash::FxHashSet;
use tsz_common::interner::Atom;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::{
    IndexSignature, MappedType, ObjectFlags, ObjectShape, PropertyInfo, TypeId, Visibility,
};

pub(crate) struct ObjectLiteralMemberProperty {
    pub(crate) name: Atom,
    pub(crate) type_id: TypeId,
    pub(crate) write_type: TypeId,
    pub(crate) optional: bool,
    pub(crate) readonly: bool,
    pub(crate) is_method: bool,
    pub(crate) declaration_order: u32,
    pub(crate) is_string_named: bool,
    pub(crate) is_symbol_named: bool,
    pub(crate) single_quoted_name: bool,
    pub(crate) non_widening: bool,
}

pub(crate) const fn object_literal_member_property(
    input: ObjectLiteralMemberProperty,
) -> PropertyInfo {
    PropertyInfo {
        name: input.name,
        type_id: input.type_id,
        write_type: input.write_type,
        optional: input.optional,
        readonly: input.readonly,
        is_method: input.is_method,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: input.declaration_order,
        is_string_named: input.is_string_named,
        is_symbol_named: input.is_symbol_named,
        single_quoted_name: input.single_quoted_name,
        non_widening: input.non_widening,
    }
}

pub(crate) struct ObjectLiteralIndexedType {
    pub(crate) properties: Vec<PropertyInfo>,
    pub(crate) string_index_types: Vec<TypeId>,
    pub(crate) number_index_types: Vec<TypeId>,
    pub(crate) symbol_index_types: Vec<TypeId>,
    pub(crate) string_index_param_name: Option<Atom>,
    pub(crate) number_index_param_name: Option<Atom>,
    pub(crate) in_const_assertion: bool,
    pub(crate) has_spread: bool,
    pub(crate) all_properties_context_sensitive: bool,
    pub(crate) display_properties: Option<Vec<PropertyInfo>>,
}

pub(crate) fn spread_object_type(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    display_properties: Vec<PropertyInfo>,
) -> TypeId {
    let result = db.object_with_flags_and_symbol(
        properties,
        ObjectFlags::PRESERVE_DECLARATION_ORDER | ObjectFlags::SPREAD_LITERAL,
        None,
    );
    db.store_display_properties(result, display_properties);
    result
}

pub(crate) fn fresh_object_type(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    all_properties_context_sensitive: bool,
    display_properties: Option<Vec<PropertyInfo>>,
) -> TypeId {
    let flags = if all_properties_context_sensitive {
        ObjectFlags::FRESH_LITERAL | ObjectFlags::ALL_PROPERTIES_CONTEXT_SENSITIVE
    } else {
        ObjectFlags::FRESH_LITERAL
    };
    let result = db.object_with_flags_and_symbol(properties, flags, None);
    if let Some(display_properties) = display_properties {
        db.store_display_properties(result, display_properties);
    }
    result
}

pub(crate) fn indexed_object_type(
    db: &dyn TypeDatabase,
    input: ObjectLiteralIndexedType,
) -> TypeId {
    let ObjectLiteralIndexedType {
        properties,
        mut string_index_types,
        number_index_types,
        symbol_index_types,
        string_index_param_name,
        number_index_param_name,
        in_const_assertion,
        has_spread,
        all_properties_context_sensitive,
        display_properties,
    } = input;

    if !string_index_types.is_empty() {
        let prop_types = properties.iter().map(|prop| prop.type_id);
        if in_const_assertion {
            string_index_types = prop_types.chain(string_index_types).collect();
        } else {
            string_index_types.extend(prop_types);
        }
    }

    let string_index = index_signature(
        db,
        TypeId::STRING,
        string_index_types,
        string_index_param_name,
        in_const_assertion,
    );
    let number_index = index_signature(
        db,
        TypeId::NUMBER,
        number_index_types,
        number_index_param_name,
        in_const_assertion,
    );
    let symbol_index = index_signature(
        db,
        TypeId::SYMBOL,
        symbol_index_types,
        None,
        in_const_assertion,
    );

    let mut shape = ObjectShape {
        properties,
        string_index,
        number_index,
        symbol_index,
        ..ObjectShape::default()
    };
    if has_spread {
        shape.mark_preserve_declaration_order();
        shape.mark_spread_literal();
    } else {
        shape.mark_fresh_literal();
        if all_properties_context_sensitive {
            shape.mark_all_properties_context_sensitive();
        }
    }

    let result = db.object_with_index(shape);
    if let Some(display_properties) = display_properties {
        db.store_display_properties(result, display_properties);
    }
    result
}

pub(crate) const fn spread_fallback_index_signature(
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

fn index_signature(
    db: &dyn TypeDatabase,
    key_type: TypeId,
    value_types: Vec<TypeId>,
    param_name: Option<Atom>,
    preserve_order: bool,
) -> Option<IndexSignature> {
    if value_types.is_empty() {
        return None;
    }

    let value_type = if preserve_order {
        order_preserving_union(db, value_types)
    } else {
        db.union(value_types)
    };
    Some(IndexSignature {
        key_type,
        value_type,
        readonly: false,
        param_name,
    })
}

pub(crate) fn order_preserving_union(db: &dyn TypeDatabase, mut members: Vec<TypeId>) -> TypeId {
    let mut seen = FxHashSet::default();
    members.retain(|id| *id != TypeId::NEVER && seen.insert(*id));
    match members.as_slice() {
        [] => TypeId::NEVER,
        [only] => *only,
        _ => db.union_from_sorted_vec(members),
    }
}

pub(crate) fn union_type(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.union(members)
}

pub(crate) fn intersection_type(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.intersection(members)
}

pub(crate) fn mapped_type_with_constraint(
    db: &dyn TypeDatabase,
    mapped: &MappedType,
    constraint: TypeId,
) -> TypeId {
    db.mapped(MappedType {
        type_param: mapped.type_param,
        constraint,
        name_type: mapped.name_type,
        template: mapped.template,
        readonly_modifier: mapped.readonly_modifier,
        optional_modifier: mapped.optional_modifier,
    })
}

pub(crate) const fn mapped_spread_property(name: Atom, type_id: TypeId) -> PropertyInfo {
    PropertyInfo::new(name, type_id)
}

pub(crate) fn mapped_spread_object_type(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
) -> TypeId {
    db.object(properties)
}
