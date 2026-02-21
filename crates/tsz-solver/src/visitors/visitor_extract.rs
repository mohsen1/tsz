//! Type data extraction helpers.
//!
//! Contains convenience functions for extracting specific data from `TypeData` variants
//! using the visitor pattern. Each function takes a `TypeDatabase` and `TypeId` and returns
//! the relevant data if the type matches the expected variant.

use crate::def::DefId;
use crate::types::{
    CallableShapeId, ConditionalTypeId, FunctionShapeId, IntrinsicKind, LiteralValue, MappedTypeId,
    ObjectShapeId, OrderedFloat, StringIntrinsicKind, TemplateLiteralId, TupleListId,
    TypeApplicationId, TypeListId, TypeParamInfo,
};
use crate::visitor::TypeVisitor;
use crate::{SymbolRef, TypeData, TypeDatabase, TypeId};
use tsz_common::interner::Atom;

struct TypeDataDataVisitor<F, T>
where
    F: Fn(&TypeData) -> Option<T>,
{
    extractor: F,
}

impl<F, T> TypeDataDataVisitor<F, T>
where
    F: Fn(&TypeData) -> Option<T>,
{
    const fn new(extractor: F) -> Self {
        Self { extractor }
    }
}

impl<F, T> TypeVisitor for TypeDataDataVisitor<F, T>
where
    F: Fn(&TypeData) -> Option<T>,
{
    type Output = Option<T>;

    fn visit_type_key(&mut self, _types: &dyn TypeDatabase, type_key: &TypeData) -> Self::Output {
        (self.extractor)(type_key)
    }

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        Self::default_output()
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        Self::default_output()
    }

    fn default_output() -> Self::Output {
        None
    }
}

fn extract_type_data<T, F>(types: &dyn TypeDatabase, type_id: TypeId, extractor: F) -> Option<T>
where
    F: Fn(&TypeData) -> Option<T>,
{
    let mut visitor = TypeDataDataVisitor::new(extractor);
    visitor.visit_type(types, type_id)
}

/// Extract the union list id if this is a union type.
pub fn union_list_id(types: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeListId> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::Union(list_id) => Some(*list_id),
        _ => None,
    })
}

/// Extract the intersection list id if this is an intersection type.
pub fn intersection_list_id(types: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeListId> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::Intersection(list_id) => Some(*list_id),
        _ => None,
    })
}

/// Extract the object shape id if this is an object type.
pub fn object_shape_id(types: &dyn TypeDatabase, type_id: TypeId) -> Option<ObjectShapeId> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::Object(shape_id) => Some(*shape_id),
        _ => None,
    })
}

/// Extract the object-with-index shape id if this is an indexed object type.
pub fn object_with_index_shape_id(
    types: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<ObjectShapeId> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::ObjectWithIndex(shape_id) => Some(*shape_id),
        _ => None,
    })
}

/// Extract the array element type if this is an array type.
pub fn array_element_type(types: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::Array(element) => Some(*element),
        _ => None,
    })
}

/// Extract the tuple list id if this is a tuple type.
pub fn tuple_list_id(types: &dyn TypeDatabase, type_id: TypeId) -> Option<TupleListId> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::Tuple(list_id) => Some(*list_id),
        _ => None,
    })
}

/// Extract the intrinsic kind if this is an intrinsic type.
pub fn intrinsic_kind(types: &dyn TypeDatabase, type_id: TypeId) -> Option<IntrinsicKind> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::Intrinsic(kind) => Some(*kind),
        _ => None,
    })
}

/// Extract the literal value if this is a literal type.
pub fn literal_value(types: &dyn TypeDatabase, type_id: TypeId) -> Option<LiteralValue> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::Literal(value) => Some(value.clone()),
        _ => None,
    })
}

/// Extract the string literal atom if this is a string literal type.
pub fn literal_string(types: &dyn TypeDatabase, type_id: TypeId) -> Option<Atom> {
    match literal_value(types, type_id) {
        Some(LiteralValue::String(atom)) => Some(atom),
        _ => None,
    }
}

/// Extract the numeric literal if this is a number literal type.
pub fn literal_number(types: &dyn TypeDatabase, type_id: TypeId) -> Option<OrderedFloat> {
    match literal_value(types, type_id) {
        Some(LiteralValue::Number(value)) => Some(value),
        _ => None,
    }
}

/// Extract the template literal list id if this is a template literal type.
pub fn template_literal_id(types: &dyn TypeDatabase, type_id: TypeId) -> Option<TemplateLiteralId> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::TemplateLiteral(list_id) => Some(*list_id),
        _ => None,
    })
}

/// Extract the type parameter info if this is a type parameter or infer type.
pub fn type_param_info(types: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeParamInfo> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::TypeParameter(info) | TypeData::Infer(info) => Some(info.clone()),
        _ => None,
    })
}

/// Extract the type reference symbol if this is a Ref type.
pub fn ref_symbol(types: &dyn TypeDatabase, type_id: TypeId) -> Option<SymbolRef> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::Lazy(_def_id) => {
            // TypeData::Ref has been migrated to TypeData::Lazy(DefId)
            // We can no longer extract SymbolRef from it
            // Return None or handle as needed based on migration strategy
            None
        }
        _ => None,
    })
}

/// Extract the lazy `DefId` if this is a Lazy type.
pub fn lazy_def_id(types: &dyn TypeDatabase, type_id: TypeId) -> Option<DefId> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::Lazy(def_id) => Some(*def_id),
        _ => None,
    })
}

/// Extract the De Bruijn index if this is a bound type parameter.
pub fn bound_parameter_index(types: &dyn TypeDatabase, type_id: TypeId) -> Option<u32> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::BoundParameter(index) => Some(*index),
        _ => None,
    })
}

/// Extract the De Bruijn index if this is a recursive type reference.
pub fn recursive_index(types: &dyn TypeDatabase, type_id: TypeId) -> Option<u32> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::Recursive(index) => Some(*index),
        _ => None,
    })
}

/// Check if this is an Enum type.
pub fn is_enum_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeData::Enum(_, _)))
}

/// Extract the enum components (`DefId` and member type) if this is an Enum type.
///
/// Returns `Some((def_id, member_type))` where:
/// - `def_id` is the unique identity of the enum for nominal checking
/// - `member_type` is the structural union of member types (e.g., 0 | 1)
pub fn enum_components(types: &dyn TypeDatabase, type_id: TypeId) -> Option<(DefId, TypeId)> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::Enum(def_id, member_type) => Some((*def_id, *member_type)),
        _ => None,
    })
}

/// Extract the application id if this is a generic application type.
pub fn application_id(types: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeApplicationId> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::Application(app_id) => Some(*app_id),
        _ => None,
    })
}

/// Extract the mapped type id if this is a mapped type.
pub fn mapped_type_id(types: &dyn TypeDatabase, type_id: TypeId) -> Option<MappedTypeId> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::Mapped(mapped_id) => Some(*mapped_id),
        _ => None,
    })
}

/// Extract the conditional type id if this is a conditional type.
pub fn conditional_type_id(types: &dyn TypeDatabase, type_id: TypeId) -> Option<ConditionalTypeId> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::Conditional(cond_id) => Some(*cond_id),
        _ => None,
    })
}

/// Extract index access components if this is an index access type.
pub fn index_access_parts(types: &dyn TypeDatabase, type_id: TypeId) -> Option<(TypeId, TypeId)> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::IndexAccess(object_type, index_type) => Some((*object_type, *index_type)),
        _ => None,
    })
}

/// Extract the type query symbol if this is a `TypeQuery`.
pub fn type_query_symbol(types: &dyn TypeDatabase, type_id: TypeId) -> Option<SymbolRef> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::TypeQuery(sym_ref) => Some(*sym_ref),
        _ => None,
    })
}

/// Extract the inner type if this is a keyof type.
pub fn keyof_inner_type(types: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::KeyOf(inner) => Some(*inner),
        _ => None,
    })
}

/// Extract the inner type if this is a readonly type.
pub fn readonly_inner_type(types: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::ReadonlyType(inner) => Some(*inner),
        _ => None,
    })
}

/// Extract the inner type if this is a `NoInfer` type.
pub fn no_infer_inner_type(types: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::NoInfer(inner) => Some(*inner),
        _ => None,
    })
}

/// Extract string intrinsic components if this is a string intrinsic type.
pub fn string_intrinsic_components(
    types: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(StringIntrinsicKind, TypeId)> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::StringIntrinsic { kind, type_arg } => Some((*kind, *type_arg)),
        _ => None,
    })
}

/// Extract the unique symbol ref if this is a unique symbol type.
pub fn unique_symbol_ref(types: &dyn TypeDatabase, type_id: TypeId) -> Option<SymbolRef> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::UniqueSymbol(sym_ref) => Some(*sym_ref),
        _ => None,
    })
}

/// Extract the module namespace symbol ref if this is a module namespace type.
pub fn module_namespace_symbol_ref(types: &dyn TypeDatabase, type_id: TypeId) -> Option<SymbolRef> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::ModuleNamespace(sym_ref) => Some(*sym_ref),
        _ => None,
    })
}

/// Check if a type is the special `this` type.
pub fn is_this_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    extract_type_data(types, type_id, |key| match key {
        TypeData::ThisType => Some(true),
        _ => None,
    })
    .unwrap_or(false)
}

/// Check whether this is an explicit error type.
pub fn is_error_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    extract_type_data(types, type_id, |key| match key {
        TypeData::Error => Some(true),
        _ => None,
    })
    .unwrap_or(false)
}

/// Extract the function shape id if this is a function type.
pub fn function_shape_id(types: &dyn TypeDatabase, type_id: TypeId) -> Option<FunctionShapeId> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::Function(shape_id) => Some(*shape_id),
        _ => None,
    })
}

/// Extract the callable shape id if this is a callable type.
pub fn callable_shape_id(types: &dyn TypeDatabase, type_id: TypeId) -> Option<CallableShapeId> {
    extract_type_data(types, type_id, |key| match key {
        TypeData::Callable(shape_id) => Some(*shape_id),
        _ => None,
    })
}
