//! Type construction boundary helpers.
//!
//! Provides mediated access to solver type construction facilities.
//! Production checker code should prefer purpose-specific helpers here
//! over direct `TypeInterner` access. Test code may use the re-exported
//! `TypeInterner` type for scaffolding.

use tsz_binder::SymbolId;
use tsz_common::Atom;
use tsz_solver::construction::TypeDatabase;
#[cfg(test)]
pub(crate) use tsz_solver::construction::TypeInterner;
use tsz_solver::{
    DefId, IndexSignature, ObjectFlags, ObjectShape, PropertyInfo, StringIntrinsicKind,
    TupleElement, TypeId, Visibility,
};

pub(crate) struct DeclaredSurfaceProperty {
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
}

pub(crate) const fn declared_surface_property(input: DeclaredSurfaceProperty) -> PropertyInfo {
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
        non_widening: false,
    }
}

pub(crate) const fn declared_index_signature(
    key_type: TypeId,
    value_type: TypeId,
    readonly: bool,
    param_name: Option<Atom>,
) -> IndexSignature {
    IndexSignature {
        key_type,
        value_type,
        readonly,
        param_name,
    }
}

pub(crate) fn declared_object_with_symbol(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    symbol: Option<SymbolId>,
) -> TypeId {
    db.object_with_flags_and_symbol(properties, ObjectFlags::empty(), symbol)
}

pub(crate) fn declared_object_with_indexes(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    string_index: Option<IndexSignature>,
    number_index: Option<IndexSignature>,
    symbol_index: Option<IndexSignature>,
    symbol: Option<SymbolId>,
) -> TypeId {
    db.object_with_index(ObjectShape {
        properties,
        string_index,
        number_index,
        symbol_index,
        symbol,
        ..ObjectShape::default()
    })
}

pub(crate) fn type_literal_object_with_late_bound(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    has_late_bound_members: bool,
) -> TypeId {
    let mut flags = ObjectFlags::empty();
    if has_late_bound_members {
        flags |= ObjectFlags::HAS_LATE_BOUND_MEMBERS;
    }
    let result = db.object_with_flags_and_symbol(properties, flags, None);
    db.mark_literal_object_annotation(result);
    result
}

pub(crate) fn type_literal_object_with_indexes_and_late_bound(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    string_index: Option<IndexSignature>,
    number_index: Option<IndexSignature>,
    symbol_index: Option<IndexSignature>,
    has_late_bound_members: bool,
) -> TypeId {
    let mut shape = ObjectShape {
        properties,
        string_index,
        number_index,
        symbol_index,
        ..ObjectShape::default()
    };
    if has_late_bound_members {
        shape.mark_has_late_bound_members();
    }
    let result = db.object_with_index(shape);
    db.mark_literal_object_annotation(result);
    result
}

pub(crate) fn type_literal_number_index_member(
    db: &dyn TypeDatabase,
    index: IndexSignature,
) -> TypeId {
    db.object_with_index(ObjectShape {
        number_index: Some(index),
        ..ObjectShape::default()
    })
}

pub(crate) fn raw_intersection_pair(db: &dyn TypeDatabase, left: TypeId, right: TypeId) -> TypeId {
    db.intersect_types_raw2(left, right)
}

pub(crate) fn type_node_union(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.union(members)
}

/// The globally-unique `SymbolRef` for a `unique symbol` declaration at
/// `(file_name, pos, end)`. Query-boundary wrapper over the solver-owned
/// `tsz_solver::unique_symbol_identity::unique_symbol_ref_from_source_span` so
/// checker code mints the same ref as the lowering pass through a sanctioned
/// boundary.
pub(crate) fn unique_symbol_ref_from_source_span(
    file_name: &str,
    pos: u32,
    end: u32,
) -> tsz_solver::SymbolRef {
    tsz_solver::unique_symbol_identity::unique_symbol_ref_from_source_span(file_name, pos, end)
}

pub(crate) fn type_node_annotation_union_with_origin(
    db: &dyn TypeDatabase,
    members: Vec<TypeId>,
) -> TypeId {
    let result = tsz_solver::utils::union_or_single_literal_reduce(db, members.clone());
    db.store_union_origin(result, members);
    result
}

pub(crate) fn type_node_intersection(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.intersection(members)
}

pub(crate) fn type_node_array(db: &dyn TypeDatabase, element_type: TypeId) -> TypeId {
    db.array(element_type)
}

pub(crate) const fn type_node_tuple_element(
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

pub(crate) fn type_node_tuple(db: &dyn TypeDatabase, elements: Vec<TupleElement>) -> TypeId {
    db.tuple(elements)
}

pub(crate) fn type_node_readonly_array(db: &dyn TypeDatabase, element_type: TypeId) -> TypeId {
    db.readonly_type(type_node_array(db, element_type))
}

pub(crate) fn type_node_array_reference(
    db: &dyn TypeDatabase,
    element_type: TypeId,
    readonly: bool,
) -> TypeId {
    let array_type = type_node_array(db, element_type);
    if readonly {
        db.readonly_type(array_type)
    } else {
        array_type
    }
}

pub(crate) fn type_node_readonly_any_array(db: &dyn TypeDatabase) -> TypeId {
    type_node_readonly_array(db, TypeId::ANY)
}

pub(crate) fn type_node_application(
    db: &dyn TypeDatabase,
    base: TypeId,
    args: Vec<TypeId>,
) -> TypeId {
    db.application(base, args)
}

pub(crate) fn type_node_lazy_type(db: &dyn TypeDatabase, def_id: DefId) -> TypeId {
    db.lazy(def_id)
}

pub(crate) fn type_node_no_infer(db: &dyn TypeDatabase, inner: TypeId) -> TypeId {
    db.no_infer(inner)
}

pub(crate) fn type_node_lazy_application(
    db: &dyn TypeDatabase,
    def_id: DefId,
    args: Vec<TypeId>,
) -> TypeId {
    type_node_application(db, type_node_lazy_type(db, def_id), args)
}

pub(crate) fn type_node_unresolved_type_name(db: &dyn TypeDatabase, name: Atom) -> TypeId {
    db.unresolved_type_name(name)
}

pub(crate) fn type_node_unresolved_application(
    db: &dyn TypeDatabase,
    name: Atom,
    args: Vec<TypeId>,
) -> TypeId {
    let base = type_node_unresolved_type_name(db, name);
    if args.is_empty() {
        base
    } else {
        type_node_application(db, base, args)
    }
}

pub(crate) fn type_node_nullable_predicate_union(
    db: &dyn TypeDatabase,
    predicate_type: TypeId,
) -> TypeId {
    type_node_union(db, vec![predicate_type, TypeId::NULL, TypeId::UNDEFINED])
}

/// Intern an object type carrying only a string index signature
/// (`{ [key: string]: V }`).
pub(crate) fn object_with_string_index_value(
    db: &dyn TypeDatabase,
    value_type: TypeId,
    readonly: bool,
) -> TypeId {
    db.object_with_index(ObjectShape {
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type,
            readonly,
            param_name: None,
        }),
        ..ObjectShape::default()
    })
}

fn enum_namespace_flags(is_const_enum: bool) -> ObjectFlags {
    let mut flags = ObjectFlags::ENUM_NAMESPACE;
    if is_const_enum {
        flags |= ObjectFlags::CONST_ENUM;
    }
    flags
}

/// Intern a `typeof Enum` namespace object without exposing raw solver flags to
/// checker call sites.
pub(crate) fn enum_namespace_object(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    symbol: Option<SymbolId>,
    is_const_enum: bool,
) -> TypeId {
    db.object_with_flags_and_symbol(properties, enum_namespace_flags(is_const_enum), symbol)
}

/// Intern a numeric/mixed enum namespace object with the implicit reverse-map
/// number index used by value-side element access.
pub(crate) fn enum_namespace_object_with_number_reverse_map(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    symbol: Option<SymbolId>,
    index_param_name: Option<Atom>,
    index_readonly: bool,
    is_const_enum: bool,
) -> TypeId {
    db.object_with_index(ObjectShape {
        flags: enum_namespace_flags(is_const_enum),
        properties,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: index_readonly,
            param_name: index_param_name,
        }),
        symbol,
        ..ObjectShape::default()
    })
}

/// Intern the indexed object produced by an inline type literal.
pub(crate) fn type_literal_object_with_index(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    string_index: Option<IndexSignature>,
    number_index: Option<IndexSignature>,
    symbol_index: Option<IndexSignature>,
    has_late_bound_members: bool,
) -> TypeId {
    let mut shape = ObjectShape {
        properties,
        string_index,
        number_index,
        symbol_index,
        ..ObjectShape::default()
    };
    if has_late_bound_members {
        shape.mark_has_late_bound_members();
    }
    db.object_with_index(shape)
}

/// Intern the extra number-index object intersected into a type literal when
/// multiple number index signatures must be preserved.
pub(crate) fn type_literal_extra_number_index_object(
    db: &dyn TypeDatabase,
    number_index: IndexSignature,
) -> TypeId {
    db.object_with_index(ObjectShape {
        number_index: Some(number_index),
        ..ObjectShape::default()
    })
}

/// Intern the plain object produced by an inline type literal.
pub(crate) fn type_literal_object(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    has_late_bound_members: bool,
) -> TypeId {
    let flags = if has_late_bound_members {
        ObjectFlags::HAS_LATE_BOUND_MEMBERS
    } else {
        ObjectFlags::empty()
    };
    db.object_with_flags_and_symbol(properties, flags, None)
}

/// Intern an intersection type produced from a type-node member list.
pub(crate) fn type_node_intersection_or_single(
    db: &dyn TypeDatabase,
    members: Vec<TypeId>,
) -> TypeId {
    tsz_solver::utils::intersection_or_single(db, members)
}

/// Intern and mark the plain object surface for a hand-written type literal.
pub(crate) fn type_node_literal_object(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
) -> TypeId {
    let result = db.object(properties);
    db.mark_literal_object_annotation(result);
    result
}

/// Intern and mark the indexed object surface for a hand-written type literal.
pub(crate) fn type_literal_object_with_indexes(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    string_index: Option<IndexSignature>,
    number_index: Option<IndexSignature>,
    symbol_index: Option<IndexSignature>,
) -> TypeId {
    let result = db.object_with_index(ObjectShape {
        properties,
        string_index,
        number_index,
        symbol_index,
        ..ObjectShape::default()
    });
    db.mark_literal_object_annotation(result);
    result
}

/// Create a string intrinsic type from a validated lib intrinsic name.
pub(crate) fn string_intrinsic_by_name(
    db: &dyn TypeDatabase,
    name: &str,
    type_arg: TypeId,
) -> TypeId {
    match name {
        "Uppercase" => db.string_intrinsic(StringIntrinsicKind::Uppercase, type_arg),
        "Lowercase" => db.string_intrinsic(StringIntrinsicKind::Lowercase, type_arg),
        "Capitalize" => db.string_intrinsic(StringIntrinsicKind::Capitalize, type_arg),
        "Uncapitalize" => db.string_intrinsic(StringIntrinsicKind::Uncapitalize, type_arg),
        _ => TypeId::ERROR,
    }
}
