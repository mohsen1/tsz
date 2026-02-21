//! Type data extraction helpers.
//!
//! Contains convenience functions for extracting specific data from `TypeData` variants
//! using the visitor pattern. Each function takes a `TypeDatabase` and `TypeId` and returns
//! the relevant data if the type matches the expected variant.

use crate::def::DefId;
use crate::types::{
    CallableShapeId, ConditionalTypeId, FunctionShapeId, IntrinsicKind, LiteralValue, MappedTypeId,
    ObjectShapeId, OrderedFloat, StringIntrinsicKind, TemplateLiteralId, TemplateSpan, TupleListId,
    TypeApplicationId, TypeListId, TypeParamInfo,
};
use crate::visitor::TypeVisitor;
use crate::{SymbolRef, TypeData, TypeDatabase, TypeId};
use rustc_hash::FxHashSet;
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

/// Recursively walk the type graph and collect all `Infer` type bindings.
///
/// Returns a list of `(name, type_id)` pairs — one for each `TypeData::Infer`
/// encountered during deep traversal. This handles cycle detection via a visited
/// set and walks into all composite type structures (unions, objects, functions,
/// conditionals, mapped types, etc.).
///
/// This is the solver-owned utility for type-graph traversal that was previously
/// duplicated in the lowering crate. Per architecture rules, type-graph walking
/// belongs in the solver.
pub fn collect_infer_bindings(types: &dyn TypeDatabase, type_id: TypeId) -> Vec<(Atom, TypeId)> {
    let mut result = Vec::new();
    let mut visited = FxHashSet::default();
    collect_infer_bindings_inner(types, type_id, &mut result, &mut visited);
    result
}

fn collect_infer_bindings_inner(
    types: &dyn TypeDatabase,
    type_id: TypeId,
    result: &mut Vec<(Atom, TypeId)>,
    visited: &mut FxHashSet<TypeId>,
) {
    if !visited.insert(type_id) {
        return;
    }

    let key = match types.lookup(type_id) {
        Some(key) => key,
        None => return,
    };

    match key {
        TypeData::Infer(info) => {
            result.push((info.name, type_id));
            if let Some(constraint) = info.constraint {
                collect_infer_bindings_inner(types, constraint, result, visited);
            }
            if let Some(default) = info.default {
                collect_infer_bindings_inner(types, default, result, visited);
            }
        }
        TypeData::Array(elem) => {
            collect_infer_bindings_inner(types, elem, result, visited);
        }
        TypeData::Tuple(elements) => {
            let elements = types.tuple_list(elements);
            for element in elements.iter() {
                collect_infer_bindings_inner(types, element.type_id, result, visited);
            }
        }
        TypeData::Union(members) | TypeData::Intersection(members) => {
            let members = types.type_list(members);
            for member in members.iter() {
                collect_infer_bindings_inner(types, *member, result, visited);
            }
        }
        TypeData::Object(shape_id) => {
            let shape = types.object_shape(shape_id);
            for prop in &shape.properties {
                collect_infer_bindings_inner(types, prop.type_id, result, visited);
            }
        }
        TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            for prop in &shape.properties {
                collect_infer_bindings_inner(types, prop.type_id, result, visited);
            }
            if let Some(index) = &shape.string_index {
                collect_infer_bindings_inner(types, index.key_type, result, visited);
                collect_infer_bindings_inner(types, index.value_type, result, visited);
            }
            if let Some(index) = &shape.number_index {
                collect_infer_bindings_inner(types, index.key_type, result, visited);
                collect_infer_bindings_inner(types, index.value_type, result, visited);
            }
        }
        TypeData::Function(shape_id) => {
            let shape = types.function_shape(shape_id);
            for param in &shape.params {
                collect_infer_bindings_inner(types, param.type_id, result, visited);
            }
            collect_infer_bindings_inner(types, shape.return_type, result, visited);
            for param in &shape.type_params {
                if let Some(constraint) = param.constraint {
                    collect_infer_bindings_inner(types, constraint, result, visited);
                }
                if let Some(default) = param.default {
                    collect_infer_bindings_inner(types, default, result, visited);
                }
            }
        }
        TypeData::Callable(shape_id) => {
            let shape = types.callable_shape(shape_id);
            for sig in &shape.call_signatures {
                collect_infer_sig(types, sig, result, visited);
            }
            for sig in &shape.construct_signatures {
                collect_infer_sig(types, sig, result, visited);
            }
            for prop in &shape.properties {
                collect_infer_bindings_inner(types, prop.type_id, result, visited);
            }
        }
        TypeData::TypeParameter(info) => {
            if let Some(constraint) = info.constraint {
                collect_infer_bindings_inner(types, constraint, result, visited);
            }
            if let Some(default) = info.default {
                collect_infer_bindings_inner(types, default, result, visited);
            }
        }
        TypeData::Application(app_id) => {
            let app = types.type_application(app_id);
            collect_infer_bindings_inner(types, app.base, result, visited);
            for &arg in &app.args {
                collect_infer_bindings_inner(types, arg, result, visited);
            }
        }
        TypeData::Conditional(cond_id) => {
            let cond = types.conditional_type(cond_id);
            collect_infer_bindings_inner(types, cond.check_type, result, visited);
            collect_infer_bindings_inner(types, cond.extends_type, result, visited);
            collect_infer_bindings_inner(types, cond.true_type, result, visited);
            collect_infer_bindings_inner(types, cond.false_type, result, visited);
        }
        TypeData::Mapped(mapped_id) => {
            let mapped = types.mapped_type(mapped_id);
            if let Some(constraint) = mapped.type_param.constraint {
                collect_infer_bindings_inner(types, constraint, result, visited);
            }
            if let Some(default) = mapped.type_param.default {
                collect_infer_bindings_inner(types, default, result, visited);
            }
            collect_infer_bindings_inner(types, mapped.constraint, result, visited);
            if let Some(name_type) = mapped.name_type {
                collect_infer_bindings_inner(types, name_type, result, visited);
            }
            collect_infer_bindings_inner(types, mapped.template, result, visited);
        }
        TypeData::IndexAccess(obj, idx) => {
            collect_infer_bindings_inner(types, obj, result, visited);
            collect_infer_bindings_inner(types, idx, result, visited);
        }
        TypeData::KeyOf(inner) | TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
            collect_infer_bindings_inner(types, inner, result, visited);
        }
        TypeData::TemplateLiteral(spans) => {
            let spans = types.template_list(spans);
            for span in spans.iter() {
                if let TemplateSpan::Type(inner) = span {
                    collect_infer_bindings_inner(types, *inner, result, visited);
                }
            }
        }
        TypeData::StringIntrinsic { type_arg, .. } => {
            collect_infer_bindings_inner(types, type_arg, result, visited);
        }
        TypeData::Enum(_def_id, member_type) => {
            collect_infer_bindings_inner(types, member_type, result, visited);
        }
        TypeData::Intrinsic(_)
        | TypeData::Literal(_)
        | TypeData::Lazy(_)
        | TypeData::Recursive(_)
        | TypeData::BoundParameter(_)
        | TypeData::TypeQuery(_)
        | TypeData::UniqueSymbol(_)
        | TypeData::ThisType
        | TypeData::ModuleNamespace(_)
        | TypeData::Error => {}
    }
}

/// Helper to collect infer bindings from a call signature's params, return type,
/// and type params.
fn collect_infer_sig(
    types: &dyn TypeDatabase,
    sig: &crate::types::CallSignature,
    result: &mut Vec<(Atom, TypeId)>,
    visited: &mut FxHashSet<TypeId>,
) {
    for param in &sig.params {
        collect_infer_bindings_inner(types, param.type_id, result, visited);
    }
    collect_infer_bindings_inner(types, sig.return_type, result, visited);
    for param in &sig.type_params {
        if let Some(constraint) = param.constraint {
            collect_infer_bindings_inner(types, constraint, result, visited);
        }
        if let Some(default) = param.default {
            collect_infer_bindings_inner(types, default, result, visited);
        }
    }
}
