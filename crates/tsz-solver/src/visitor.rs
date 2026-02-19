//! Type Visitor Pattern
//!
//! This module implements the Visitor pattern for `TypeData` operations,
//! providing a clean alternative to repetitive match statements.
//!
//! # Benefits
//!
//! - **Centralized type logic**: All type handling in one place
//! - **Easier to extend**: Add new visitors without modifying existing code
//! - **Type-safe**: Compiler ensures all variants are handled
//! - **Composable**: Visitors can be combined and chained
//!
//! # Usage
//!
//! ```rust,ignore
//! use crate::visitor::{TypeVisitor, TypeKind, is_type_kind};
//! use crate::types::{IntrinsicKind, LiteralValue};
//!
//! struct MyVisitor;
//!
//! impl TypeVisitor for MyVisitor {
//!     type Output = bool;
//!
//!     fn visit_intrinsic(&mut self, kind: IntrinsicKind) -> Self::Output {
//!         matches!(kind, IntrinsicKind::Any)
//!     }
//!
//!     fn visit_literal(&mut self, value: &LiteralValue) -> Self::Output {
//!         matches!(value, LiteralValue::Boolean(true))
//!     }
//!
//!     fn default_output() -> Self::Output {
//!         false
//!     }
//! }
//!
//! // Or use convenience functions:
//! let is_object = is_type_kind(&types, type_id, TypeKind::Object);
//! ```

use crate::def::DefId;
use crate::types::{
    IntrinsicKind, ObjectShapeId, StringIntrinsicKind, TupleElement, TypeParamInfo,
};
use crate::{LiteralValue, SymbolRef, TypeData, TypeDatabase, TypeId};
use rustc_hash::{FxHashMap, FxHashSet};

// Re-export type data extraction helpers (extracted to visitor_extract.rs)
pub use crate::visitor_extract::*;

// =============================================================================
// Type Visitor Trait
// =============================================================================

/// Visitor pattern for `TypeData` traversal and transformation.
///
/// Implement this trait to perform custom operations on types without
/// writing repetitive match statements. Each method corresponds to a
/// `TypeData` variant and receives the relevant data for that type.
pub trait TypeVisitor: Sized {
    /// The output type produced by visiting.
    type Output;

    // =========================================================================
    // Core Type Visitors
    // =========================================================================

    /// Visit an intrinsic type (any, unknown, never, void, etc.).
    fn visit_intrinsic(&mut self, kind: IntrinsicKind) -> Self::Output;

    /// Visit a literal type (string, number, boolean, bigint literals).
    fn visit_literal(&mut self, value: &LiteralValue) -> Self::Output;

    // =========================================================================
    // Composite Types - Default implementations provided
    // =========================================================================

    /// Visit an object type with properties.
    fn visit_object(&mut self, _shape_id: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit an object type with index signatures.
    fn visit_object_with_index(&mut self, _shape_id: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit a union type (A | B | C).
    fn visit_union(&mut self, _list_id: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit an intersection type (A & B & C).
    fn visit_intersection(&mut self, _list_id: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit an array type T[].
    fn visit_array(&mut self, _element_type: TypeId) -> Self::Output {
        Self::default_output()
    }

    /// Visit a tuple type [T, U, V].
    fn visit_tuple(&mut self, _list_id: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit a function type.
    fn visit_function(&mut self, _shape_id: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit a callable type with call/construct signatures.
    fn visit_callable(&mut self, _shape_id: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit a type parameter (generic type variable).
    fn visit_type_parameter(&mut self, _param_info: &TypeParamInfo) -> Self::Output {
        Self::default_output()
    }

    /// Visit a bound type parameter using De Bruijn index for alpha-equivalence.
    ///
    /// This is used for canonicalizing generic types to achieve structural identity,
    /// where `type F<T> = T` and `type G<U> = U` are considered identical.
    /// The index represents which parameter in the binding scope (0 = innermost).
    fn visit_bound_parameter(&mut self, _de_bruijn_index: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit a named type reference (interface, class, type alias).
    fn visit_ref(&mut self, _symbol_ref: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit an enum type with nominal identity and structural member types.
    fn visit_enum(&mut self, _def_id: u32, _member_type: TypeId) -> Self::Output {
        Self::default_output()
    }

    /// Visit a lazy type reference using `DefId`.
    fn visit_lazy(&mut self, _def_id: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit a recursive type reference using De Bruijn index.
    ///
    /// This is used for canonicalizing recursive types to achieve O(1) equality.
    /// The index represents how many levels up the nesting chain to refer to.
    fn visit_recursive(&mut self, _de_bruijn_index: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit a generic type application Base<Args>.
    fn visit_application(&mut self, _app_id: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit a conditional type T extends U ? X : Y.
    fn visit_conditional(&mut self, _cond_id: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit a mapped type { [K in Keys]: V }.
    fn visit_mapped(&mut self, _mapped_id: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit an indexed access type T[K].
    fn visit_index_access(&mut self, _object_type: TypeId, _key_type: TypeId) -> Self::Output {
        Self::default_output()
    }

    /// Visit a template literal type `hello${x}world`.
    fn visit_template_literal(&mut self, _template_id: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit a type query (typeof expr).
    fn visit_type_query(&mut self, _symbol_ref: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit a keyof type.
    fn visit_keyof(&mut self, _type_id: TypeId) -> Self::Output {
        Self::default_output()
    }

    /// Visit a readonly type modifier.
    fn visit_readonly_type(&mut self, _inner_type: TypeId) -> Self::Output {
        Self::default_output()
    }

    /// Visit a unique symbol type.
    fn visit_unique_symbol(&mut self, _symbol_ref: u32) -> Self::Output {
        Self::default_output()
    }

    /// Visit an infer type (for type inference in conditional types).
    fn visit_infer(&mut self, _param_info: &TypeParamInfo) -> Self::Output {
        Self::default_output()
    }

    /// Visit a this type (polymorphic this parameter).
    fn visit_this_type(&mut self) -> Self::Output {
        Self::default_output()
    }

    /// Visit a string manipulation intrinsic type.
    fn visit_string_intrinsic(
        &mut self,
        _kind: StringIntrinsicKind,
        _type_arg: TypeId,
    ) -> Self::Output {
        Self::default_output()
    }

    /// Visit an error type.
    fn visit_error(&mut self) -> Self::Output {
        Self::default_output()
    }

    /// Visit a `NoInfer`<T> type (TypeScript 5.4+).
    /// Traverses the inner type (`NoInfer` is transparent for traversal).
    fn visit_no_infer(&mut self, _inner: TypeId) -> Self::Output {
        Self::default_output()
    }

    /// Visit a module namespace type (import * as ns).
    fn visit_module_namespace(&mut self, _symbol_ref: u32) -> Self::Output {
        Self::default_output()
    }

    // =========================================================================
    // Helper Methods
    // =========================================================================

    /// Default output for unimplemented variants.
    fn default_output() -> Self::Output;

    /// Visit a type by dispatching to the appropriate method.
    ///
    /// This is the main entry point for using the visitor.
    fn visit_type(&mut self, types: &dyn TypeDatabase, type_id: TypeId) -> Self::Output {
        match types.lookup(type_id) {
            Some(ref type_key) => self.visit_type_key(types, type_key),
            None => Self::default_output(),
        }
    }

    /// Visit a `TypeData` by dispatching to the appropriate method.
    fn visit_type_key(&mut self, _types: &dyn TypeDatabase, type_key: &TypeData) -> Self::Output {
        match type_key {
            TypeData::Intrinsic(kind) => self.visit_intrinsic(*kind),
            TypeData::Literal(value) => self.visit_literal(value),
            TypeData::Object(id) => self.visit_object(id.0),
            TypeData::ObjectWithIndex(id) => self.visit_object_with_index(id.0),
            TypeData::Union(id) => self.visit_union(id.0),
            TypeData::Intersection(id) => self.visit_intersection(id.0),
            TypeData::Array(element_type) => self.visit_array(*element_type),
            TypeData::Tuple(id) => self.visit_tuple(id.0),
            TypeData::Function(id) => self.visit_function(id.0),
            TypeData::Callable(id) => self.visit_callable(id.0),
            TypeData::TypeParameter(info) => self.visit_type_parameter(info),
            TypeData::BoundParameter(index) => self.visit_bound_parameter(*index),
            TypeData::Lazy(def_id) => self.visit_lazy(def_id.0),
            TypeData::Recursive(index) => self.visit_recursive(*index),
            TypeData::Enum(def_id, member_type) => self.visit_enum(def_id.0, *member_type),
            TypeData::Application(id) => self.visit_application(id.0),
            TypeData::Conditional(id) => self.visit_conditional(id.0),
            TypeData::Mapped(id) => self.visit_mapped(id.0),
            TypeData::IndexAccess(obj, key) => self.visit_index_access(*obj, *key),
            TypeData::TemplateLiteral(id) => self.visit_template_literal(id.0),
            TypeData::TypeQuery(sym_ref) => self.visit_type_query(sym_ref.0),
            TypeData::KeyOf(type_id) => self.visit_keyof(*type_id),
            TypeData::ReadonlyType(inner) => self.visit_readonly_type(*inner),
            TypeData::UniqueSymbol(sym_ref) => self.visit_unique_symbol(sym_ref.0),
            TypeData::Infer(info) => self.visit_infer(info),
            TypeData::ThisType => self.visit_this_type(),
            TypeData::StringIntrinsic { kind, type_arg } => {
                self.visit_string_intrinsic(*kind, *type_arg)
            }
            TypeData::ModuleNamespace(sym_ref) => self.visit_module_namespace(sym_ref.0),
            TypeData::NoInfer(inner) => self.visit_no_infer(*inner),
            TypeData::Error => self.visit_error(),
        }
    }
}

// =============================================================================
// Type Traversal Helpers
// =============================================================================

/// Invoke a function on each immediate child `TypeId` of a `TypeData`.
///
/// This function provides a simple way to traverse the type graph without
/// requiring the full Visitor pattern. It's useful for operations like:
/// - Populating caches (ensuring all nested types are resolved)
/// - Collecting dependencies
/// - Type environment population
///
/// # Parameters
///
/// * `db` - The type database to look up type structures
/// * `key` - The `TypeData` whose children should be visited
/// * `f` - Function to call for each child `TypeId`
///
/// # Examples
///
/// ```rust,ignore
/// use crate::visitor::for_each_child;
///
/// for_each_child(types, &type_key, |child_id| {
///     // Process each nested type
/// });
/// ```
///
/// # `TypeData` Variants Handled
///
/// This function handles ALL `TypeData` variants to ensure complete traversal:
/// - **Single nested types**: Array, `ReadonlyType`, `KeyOf`, etc.
/// - **Multiple members**: Union, Intersection
/// - **Structured types**: Object, Tuple, Function, Callable
/// - **Complex types**: Application, Conditional, Mapped, `IndexAccess`
/// - **Template literals**: Iterates over template spans
/// - **String intrinsics**: Visits type argument
/// - **Leaf types**: Intrinsic, Literal, Lazy, `TypeQuery`, etc. (no children)
pub fn for_each_child<F>(db: &dyn TypeDatabase, key: &TypeData, mut f: F)
where
    F: FnMut(TypeId),
{
    match key {
        // Single nested type
        TypeData::Array(inner)
        | TypeData::ReadonlyType(inner)
        | TypeData::KeyOf(inner)
        | TypeData::NoInfer(inner) => {
            f(*inner);
        }

        // Composite types with multiple members
        TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
            for &member in db.type_list(*list_id).iter() {
                f(member);
            }
        }

        // Object types with properties and index signatures
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = db.object_shape(*shape_id);
            for prop in &shape.properties {
                f(prop.type_id);
                f(prop.write_type); // IMPORTANT: Must visit both read and write types
            }
            if let Some(ref sig) = shape.string_index {
                f(sig.key_type);
                f(sig.value_type);
            }
            if let Some(ref sig) = shape.number_index {
                f(sig.key_type);
                f(sig.value_type);
            }
        }

        // Tuple types
        TypeData::Tuple(tuple_id) => {
            for elem in db.tuple_list(*tuple_id).iter() {
                f(elem.type_id);
            }
        }

        // Function types
        TypeData::Function(func_id) => {
            let sig = db.function_shape(*func_id);
            f(sig.return_type);
            if let Some(this_type) = sig.this_type {
                f(this_type);
            }
            if let Some(ref type_predicate) = sig.type_predicate
                && let Some(type_id) = type_predicate.type_id
            {
                f(type_id);
            }
            for param in &sig.params {
                f(param.type_id);
            }
            for type_param in &sig.type_params {
                if let Some(constraint) = type_param.constraint {
                    f(constraint);
                }
                if let Some(default) = type_param.default {
                    f(default);
                }
            }
        }

        // Callable types
        TypeData::Callable(callable_id) => {
            let callable = db.callable_shape(*callable_id);
            for sig in &callable.call_signatures {
                f(sig.return_type);
                if let Some(this_type) = sig.this_type {
                    f(this_type);
                }
                if let Some(ref type_predicate) = sig.type_predicate
                    && let Some(type_id) = type_predicate.type_id
                {
                    f(type_id);
                }
                for param in &sig.params {
                    f(param.type_id);
                }
                for type_param in &sig.type_params {
                    if let Some(constraint) = type_param.constraint {
                        f(constraint);
                    }
                    if let Some(default) = type_param.default {
                        f(default);
                    }
                }
            }
            for sig in &callable.construct_signatures {
                f(sig.return_type);
                if let Some(this_type) = sig.this_type {
                    f(this_type);
                }
                if let Some(ref type_predicate) = sig.type_predicate
                    && let Some(type_id) = type_predicate.type_id
                {
                    f(type_id);
                }
                for param in &sig.params {
                    f(param.type_id);
                }
                for type_param in &sig.type_params {
                    if let Some(constraint) = type_param.constraint {
                        f(constraint);
                    }
                    if let Some(default) = type_param.default {
                        f(default);
                    }
                }
            }
            // Visit prototype properties
            for prop in &callable.properties {
                f(prop.type_id);
                f(prop.write_type);
            }
            if let Some(ref sig) = callable.string_index {
                f(sig.key_type);
                f(sig.value_type);
            }
            if let Some(ref sig) = callable.number_index {
                f(sig.key_type);
                f(sig.value_type);
            }
        }

        // Type applications
        TypeData::Application(app_id) => {
            let app = db.type_application(*app_id);
            f(app.base);
            for &arg in &app.args {
                f(arg);
            }
        }

        // Conditional types
        TypeData::Conditional(cond_id) => {
            let cond = db.conditional_type(*cond_id);
            f(cond.check_type);
            f(cond.extends_type);
            f(cond.true_type);
            f(cond.false_type);
        }

        // Mapped types
        TypeData::Mapped(mapped_id) => {
            let mapped = db.mapped_type(*mapped_id);
            if let Some(constraint) = mapped.type_param.constraint {
                f(constraint);
            }
            if let Some(default) = mapped.type_param.default {
                f(default);
            }
            f(mapped.constraint);
            f(mapped.template);
            if let Some(name_type) = mapped.name_type {
                f(name_type);
            }
        }

        // Index access types
        TypeData::IndexAccess(obj, idx) => {
            f(*obj);
            f(*idx);
        }

        // Template literal types
        TypeData::TemplateLiteral(template_id) => {
            for span in db.template_list(*template_id).iter() {
                match span {
                    crate::types::TemplateSpan::Text(_) => {}
                    crate::types::TemplateSpan::Type(type_id) => {
                        f(*type_id);
                    }
                }
            }
        }

        // String intrinsics
        TypeData::StringIntrinsic { type_arg, .. } => {
            f(*type_arg);
        }

        // Type parameters with constraints
        TypeData::TypeParameter(info) | TypeData::Infer(info) => {
            if let Some(constraint) = info.constraint {
                f(constraint);
            }
        }

        // Enum types
        TypeData::Enum(_def_id, member_type) => {
            f(*member_type);
        }

        // Leaf types - no children to visit
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

/// Walk all transitively referenced type IDs from `root`.
///
/// The callback is invoked once per unique reachable type (including `root`).
pub fn walk_referenced_types<F>(types: &dyn TypeDatabase, root: TypeId, mut f: F)
where
    F: FnMut(TypeId),
{
    let mut visited = FxHashSet::default();
    let mut stack = vec![root];

    while let Some(current) = stack.pop() {
        if !visited.insert(current) {
            continue;
        }
        f(current);

        let Some(key) = types.lookup(current) else {
            continue;
        };
        for_each_child(types, &key, |child| stack.push(child));
    }
}

/// Collect all unique lazy `DefIds` reachable from `root`.
pub fn collect_lazy_def_ids(types: &dyn TypeDatabase, root: TypeId) -> Vec<DefId> {
    let mut out = Vec::new();
    let mut seen = FxHashSet::default();

    walk_referenced_types(types, root, |type_id| {
        if let Some(TypeData::Lazy(def_id)) = types.lookup(type_id)
            && seen.insert(def_id)
        {
            out.push(def_id);
        }
    });

    out
}

/// Collect all unique enum `DefIds` reachable from `root`.
pub fn collect_enum_def_ids(types: &dyn TypeDatabase, root: TypeId) -> Vec<DefId> {
    let mut out = Vec::new();
    let mut seen = FxHashSet::default();

    walk_referenced_types(types, root, |type_id| {
        if let Some(TypeData::Enum(def_id, _)) = types.lookup(type_id)
            && seen.insert(def_id)
        {
            out.push(def_id);
        }
    });

    out
}

/// Collect all unique type-query symbol references reachable from `root`.
pub fn collect_type_queries(types: &dyn TypeDatabase, root: TypeId) -> Vec<SymbolRef> {
    let mut out = Vec::new();
    let mut seen = FxHashSet::default();

    walk_referenced_types(types, root, |type_id| {
        if let Some(TypeData::TypeQuery(symbol_ref)) = types.lookup(type_id)
            && seen.insert(symbol_ref)
        {
            out.push(symbol_ref);
        }
    });

    out
}

// =============================================================================
// Common Visitor Implementations
// =============================================================================

/// Classification of types into broad categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeKind {
    /// Primitive types (string, number, boolean, etc.)
    Primitive,
    /// Literal types ("hello", 42, true)
    Literal,
    /// Object types
    Object,
    /// Array types
    Array,
    /// Tuple types
    Tuple,
    /// Union types
    Union,
    /// Intersection types
    Intersection,
    /// Function/callable types
    Function,
    /// Generic types (type applications)
    Generic,
    /// Type parameters (T, K, etc.)
    TypeParameter,
    /// Conditional types (T extends U ? X : Y)
    Conditional,
    /// Mapped types ({ [K in Keys]: V })
    Mapped,
    /// Index access types (T[K])
    IndexAccess,
    /// Template literal types (`hello${T}`)
    TemplateLiteral,
    /// Type query (typeof expr)
    TypeQuery,
    /// `KeyOf` types (keyof T)
    KeyOf,
    /// Named type references (interfaces, type aliases)
    Reference,
    /// Error types
    Error,
    /// Other/unknown
    Other,
}

/// Visitor that checks if a type is a specific `TypeKind`.
pub struct TypeKindVisitor {
    /// The kind to check for.
    pub target_kind: TypeKind,
}

impl TypeKindVisitor {
    /// Create a new `TypeKindVisitor`.
    pub const fn new(target_kind: TypeKind) -> Self {
        Self { target_kind }
    }

    /// Get the kind of a type from its `TypeData`.
    pub const fn get_kind(type_key: &TypeData) -> TypeKind {
        match type_key {
            TypeData::Intrinsic(_)
            | TypeData::Enum(_, _)
            | TypeData::UniqueSymbol(_)
            | TypeData::StringIntrinsic { .. } => TypeKind::Primitive,
            TypeData::Literal(_) => TypeKind::Literal,
            TypeData::Object(_) | TypeData::ObjectWithIndex(_) | TypeData::ModuleNamespace(_) => {
                TypeKind::Object
            }
            TypeData::Array(_) => TypeKind::Array,
            TypeData::Tuple(_) => TypeKind::Tuple,
            TypeData::Union(_) => TypeKind::Union,
            TypeData::Intersection(_) => TypeKind::Intersection,
            TypeData::Function(_) | TypeData::Callable(_) => TypeKind::Function,
            TypeData::Application(_) => TypeKind::Generic,
            TypeData::TypeParameter(_) | TypeData::Infer(_) | TypeData::BoundParameter(_) => {
                TypeKind::TypeParameter
            }
            TypeData::Conditional(_) => TypeKind::Conditional,
            TypeData::Lazy(_) | TypeData::Recursive(_) => TypeKind::Reference,
            TypeData::Mapped(_) => TypeKind::Mapped,
            TypeData::IndexAccess(_, _) => TypeKind::IndexAccess,
            TypeData::TemplateLiteral(_) => TypeKind::TemplateLiteral,
            TypeData::TypeQuery(_) => TypeKind::TypeQuery,
            TypeData::KeyOf(_) => TypeKind::KeyOf,
            TypeData::ReadonlyType(_inner) => {
                // Readonly doesn't change the kind - look through it
                // Note: This requires lookup which we don't have here
                // For now, return Other and let callers handle it
                TypeKind::Other
            }
            TypeData::NoInfer(_inner) => {
                // NoInfer doesn't change the kind - look through it
                TypeKind::Other
            }
            TypeData::ThisType => TypeKind::TypeParameter, // this is type-parameter-like
            TypeData::Error => TypeKind::Error,
        }
    }

    /// Get the kind of a type by `TypeId`.
    pub fn get_kind_of(types: &dyn TypeDatabase, type_id: TypeId) -> TypeKind {
        match types.lookup(type_id) {
            Some(ref type_key) => Self::get_kind(type_key),
            None => TypeKind::Other,
        }
    }
}

impl TypeVisitor for TypeKindVisitor {
    type Output = bool;

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        self.target_kind == TypeKind::Primitive
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        self.target_kind == TypeKind::Literal
    }

    fn visit_type_key(&mut self, _types: &dyn TypeDatabase, type_key: &TypeData) -> Self::Output {
        Self::get_kind(type_key) == self.target_kind
    }

    fn default_output() -> Self::Output {
        false
    }
}

/// Visitor that collects all `TypeIds` referenced by a type.
///
/// Useful for finding dependencies or tracking type usage.
pub struct TypeCollectorVisitor {
    /// Set of collected type IDs.
    pub types: FxHashSet<TypeId>,
    /// Maximum depth to traverse.
    pub max_depth: usize,
}

impl TypeCollectorVisitor {
    /// Create a new `TypeCollectorVisitor`.
    pub fn new() -> Self {
        Self {
            types: FxHashSet::default(),
            max_depth: 10,
        }
    }

    /// Create a new `TypeCollectorVisitor` with custom max depth.
    pub fn with_max_depth(max_depth: usize) -> Self {
        Self {
            types: FxHashSet::default(),
            max_depth,
        }
    }
}

impl Default for TypeCollectorVisitor {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeVisitor for TypeCollectorVisitor {
    type Output = ();

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        // Intrinsic types have no child types to collect
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        // Literal types have no child types to collect
    }

    fn visit_array(&mut self, element_type: TypeId) -> Self::Output {
        if self.max_depth > 0 {
            self.types.insert(element_type);
        }
    }

    fn visit_index_access(&mut self, object_type: TypeId, key_type: TypeId) -> Self::Output {
        if self.max_depth > 0 {
            self.types.insert(object_type);
            self.types.insert(key_type);
        }
    }

    fn visit_keyof(&mut self, type_id: TypeId) -> Self::Output {
        if self.max_depth > 0 {
            self.types.insert(type_id);
        }
    }

    fn visit_readonly_type(&mut self, inner_type: TypeId) -> Self::Output {
        if self.max_depth > 0 {
            self.types.insert(inner_type);
        }
    }

    fn visit_string_intrinsic(
        &mut self,
        _kind: StringIntrinsicKind,
        type_arg: TypeId,
    ) -> Self::Output {
        if self.max_depth > 0 {
            self.types.insert(type_arg);
        }
    }

    fn visit_enum(&mut self, _def_id: u32, member_type: TypeId) -> Self::Output {
        if self.max_depth > 0 {
            self.types.insert(member_type);
        }
    }

    fn default_output() -> Self::Output {}
}

/// Visitor that checks if a type matches a specific predicate.
pub struct TypePredicateVisitor<F>
where
    F: Fn(&TypeData) -> bool,
{
    /// Predicate function to test against `TypeData`.
    pub predicate: F,
}

impl<F> TypePredicateVisitor<F>
where
    F: Fn(&TypeData) -> bool,
{
    /// Create a new `TypePredicateVisitor`.
    pub const fn new(predicate: F) -> Self {
        Self { predicate }
    }
}

impl<F> TypeVisitor for TypePredicateVisitor<F>
where
    F: Fn(&TypeData) -> bool,
{
    type Output = bool;

    fn visit_type_key(&mut self, _types: &dyn TypeDatabase, type_key: &TypeData) -> Self::Output {
        (self.predicate)(type_key)
    }

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        false
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        false
    }

    fn default_output() -> Self::Output {
        false
    }
}

// =============================================================================
// Convenience Functions
// =============================================================================

/// Check if a type is a specific kind using the `TypeKindVisitor`.
///
/// # Example
///
/// ```rust,ignore
/// use crate::visitor::{is_type_kind, TypeKind};
///
/// let is_object = is_type_kind(&types, type_id, TypeKind::Object);
/// ```
pub fn is_type_kind(types: &dyn TypeDatabase, type_id: TypeId, kind: TypeKind) -> bool {
    let mut visitor = TypeKindVisitor::new(kind);
    visitor.visit_type(types, type_id)
}

/// Collect all types referenced by a type.
///
/// # Example
///
/// ```rust,ignore
/// use crate::visitor::collect_referenced_types;
///
/// let types = collect_referenced_types(&type_interner, type_id);
/// ```
pub fn collect_referenced_types(types: &dyn TypeDatabase, type_id: TypeId) -> FxHashSet<TypeId> {
    collect_all_types(types, type_id)
}

/// Test a type against a predicate function.
///
/// # Example
///
/// ```rust,ignore
/// use crate::{TypeData, LiteralValue, visitor::test_type};
///
/// let is_string_literal = test_type(&types, type_id, |key| {
///     matches!(key, TypeData::Literal(LiteralValue::String(_)))
/// });
/// ```
pub fn test_type<F>(types: &dyn TypeDatabase, type_id: TypeId, predicate: F) -> bool
where
    F: Fn(&TypeData) -> bool,
{
    let mut visitor = TypePredicateVisitor::new(predicate);
    visitor.visit_type(types, type_id)
}

// =============================================================================
// Specialized Type Predicate Visitors
// =============================================================================

/// Check if a type is a literal type.
///
/// Matches: `TypeData::Literal`(_)
pub fn is_literal_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeData::Literal(_)))
}

/// Check if a type is a module namespace type (import * as ns).
///
/// Matches: `TypeData::ModuleNamespace`(_)
pub fn is_module_namespace_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeData::ModuleNamespace(_)))
}

/// Check if a type is a function type (Function or Callable).
///
/// This also handles intersections containing function types.
pub fn is_function_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_function_type_impl(types, type_id)
}

fn is_function_type_impl(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match types.lookup(type_id) {
        Some(TypeData::Function(_) | TypeData::Callable(_)) => true,
        Some(TypeData::Intersection(members)) => {
            let members = types.type_list(members);
            members
                .iter()
                .any(|&member| is_function_type_impl(types, member))
        }
        _ => false,
    }
}

/// Check if a type is an object-like type (suitable for typeof "object").
///
/// Returns true for: Object, `ObjectWithIndex`, Array, Tuple, Mapped, `ReadonlyType` (of object)
pub fn is_object_like_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_object_like_type_impl(types, type_id)
}

fn is_object_like_type_impl(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match types.lookup(type_id) {
        Some(
            TypeData::Object(_)
            | TypeData::ObjectWithIndex(_)
            | TypeData::Array(_)
            | TypeData::Tuple(_)
            | TypeData::Mapped(_)
            | TypeData::Function(_)
            | TypeData::Callable(_)
            | TypeData::Intrinsic(IntrinsicKind::Object | IntrinsicKind::Function),
        ) => true,
        Some(TypeData::ReadonlyType(inner)) => is_object_like_type_impl(types, inner),
        Some(TypeData::Intersection(members)) => {
            let members = types.type_list(members);
            members
                .iter()
                .all(|&member| is_object_like_type_impl(types, member))
        }
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info
            .constraint
            .is_some_and(|constraint| is_object_like_type_impl(types, constraint)),
        _ => false,
    }
}

/// Check if a type is an empty object type (no properties, no index signatures).
pub fn is_empty_object_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match types.lookup(type_id) {
        Some(TypeData::Object(shape_id)) => {
            let shape = types.object_shape(shape_id);
            shape.properties.is_empty()
        }
        Some(TypeData::ObjectWithIndex(shape_id)) => {
            let shape = types.object_shape(shape_id);
            shape.properties.is_empty()
                && shape.string_index.is_none()
                && shape.number_index.is_none()
        }
        _ => false,
    }
}

/// Check if a type is a primitive type (intrinsic or literal).
pub fn is_primitive_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // Check well-known intrinsic TypeIds first
    if type_id.is_intrinsic() {
        return true;
    }
    matches!(
        types.lookup(type_id),
        Some(TypeData::Intrinsic(_) | TypeData::Literal(_))
    )
}

/// Check if a type is a union type.
pub fn is_union_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeData::Union(_)))
}

/// Check if a type is an intersection type.
pub fn is_intersection_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeData::Intersection(_)))
}

/// Check if a type is an array type.
pub fn is_array_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeData::Array(_)))
}

/// Check if a type is a tuple type.
pub fn is_tuple_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeData::Tuple(_)))
}

/// Check if a type is a type parameter.
pub fn is_type_parameter(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        types.lookup(type_id),
        Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
    )
}

/// Check if a type is a conditional type.
pub fn is_conditional_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeData::Conditional(_)))
}

/// Check if a type is a mapped type.
pub fn is_mapped_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeData::Mapped(_)))
}

/// Check if a type is an index access type.
pub fn is_index_access_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeData::IndexAccess(_, _)))
}

/// Check if a type is a template literal type.
pub fn is_template_literal_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeData::TemplateLiteral(_)))
}

/// Check if a type is a type reference (Lazy/DefId).
pub fn is_type_reference(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        types.lookup(type_id),
        Some(TypeData::Lazy(_) | TypeData::Recursive(_))
    )
}

/// Check if a type is a generic type application.
pub fn is_generic_application(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeData::Application(_)))
}

/// Check if a type is a "unit type" - a type that represents exactly one value.
///
/// Unit types are types where subtyping reduces to identity: two different unit types
/// are always disjoint (neither is a subtype of the other, except for identity).
///
/// This is used as an optimization to skip structural recursion in subtype checking.
/// For example, comparing `[E.A, E.B]` vs `[E.C, E.D]` can return `source == target`
/// in O(1) instead of walking into each tuple element.
///
/// Unit types include:
/// - Literal types (string, number, boolean, bigint literals)
/// - Enum members (`TypeData::Enum`)
/// - Unique symbols
/// - null, undefined, void
/// - Tuples where ALL elements are unit types (and no rest elements)
///
/// NOTE: This does NOT handle `ReadonlyType` - readonly tuples must be checked separately
/// because `["a"]` is a subtype of `readonly ["a"]` even though they have different `TypeIds`.
pub fn is_unit_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_unit_type_impl(types, type_id, 0)
}

const MAX_UNIT_TYPE_DEPTH: u32 = 10;

fn is_unit_type_impl(types: &dyn TypeDatabase, type_id: TypeId, depth: u32) -> bool {
    // Prevent stack overflow on pathological types
    if depth > MAX_UNIT_TYPE_DEPTH {
        return false;
    }

    // Check well-known singleton types first
    if type_id == TypeId::NULL
        || type_id == TypeId::UNDEFINED
        || type_id == TypeId::VOID
        || type_id == TypeId::NEVER
    {
        return true;
    }

    match types.lookup(type_id) {
        // Unit-like scalar types are handled together.
        Some(TypeData::Literal(_))
        | Some(TypeData::Enum(_, _))
        | Some(TypeData::UniqueSymbol(_)) => true,

        // Tuples are unit types if ALL elements are unit types (no rest elements)
        Some(TypeData::Tuple(list_id)) => {
            let elements = types.tuple_list(list_id);
            // Check for rest elements - if any, not a unit type
            if elements.iter().any(|e| e.rest) {
                return false;
            }
            // All elements must be unit types
            elements
                .iter()
                .all(|e| is_unit_type_impl(types, e.type_id, depth + 1))
        }

        // Everything else is not a unit type
        // ReadonlyType of a unit tuple is NOT considered a unit type for optimization purposes
        // because ["a"] <: readonly ["a"] but they have different TypeIds.
        _ => false,
    }
}

// =============================================================================
// Recursive Type Visitor - Traverses into nested types
// =============================================================================

/// A visitor that recursively collects all types referenced by a root type.
/// Unlike `TypeCollectorVisitor`, this properly traverses into nested structures.
pub struct RecursiveTypeCollector<'a> {
    types: &'a dyn TypeDatabase,
    collected: FxHashSet<TypeId>,
    guard: crate::recursion::RecursionGuard<TypeId>,
}

impl<'a> RecursiveTypeCollector<'a> {
    pub fn new(types: &'a dyn TypeDatabase) -> Self {
        Self {
            types,
            collected: FxHashSet::default(),
            guard: crate::recursion::RecursionGuard::with_profile(
                crate::recursion::RecursionProfile::ShallowTraversal,
            ),
        }
    }

    pub fn with_max_depth(types: &'a dyn TypeDatabase, max_depth: usize) -> Self {
        Self {
            types,
            collected: FxHashSet::default(),
            guard: crate::recursion::RecursionGuard::new(max_depth as u32, 100_000),
        }
    }

    /// Collect all types reachable from the given type.
    pub fn collect(&mut self, type_id: TypeId) -> FxHashSet<TypeId> {
        self.visit(type_id);
        std::mem::take(&mut self.collected)
    }

    fn visit(&mut self, type_id: TypeId) {
        // Already collected
        if self.collected.contains(&type_id) {
            return;
        }

        // Unified enter: checks depth, cycle, iterations
        match self.guard.enter(type_id) {
            crate::recursion::RecursionResult::Entered => {}
            _ => return,
        }

        self.collected.insert(type_id);

        if let Some(key) = self.types.lookup(type_id) {
            self.visit_key(&key);
        }

        self.guard.leave(type_id);
    }

    fn visit_key(&mut self, key: &TypeData) {
        match key {
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Error
            | TypeData::ThisType
            | TypeData::BoundParameter(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_) => {
                // Leaf types - nothing to traverse
            }
            TypeData::NoInfer(inner) => {
                // Traverse inner type
                self.visit(*inner);
            }
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.types.object_shape(*shape_id);
                for prop in &shape.properties {
                    self.visit(prop.type_id);
                    self.visit(prop.write_type);
                }
                if let Some(ref idx) = shape.string_index {
                    self.visit(idx.key_type);
                    self.visit(idx.value_type);
                }
                if let Some(ref idx) = shape.number_index {
                    self.visit(idx.key_type);
                    self.visit(idx.value_type);
                }
            }
            TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
                let members = self.types.type_list(*list_id);
                for &member in members.iter() {
                    self.visit(member);
                }
            }
            TypeData::Array(elem) => {
                self.visit(*elem);
            }
            TypeData::Tuple(list_id) => {
                let elements = self.types.tuple_list(*list_id);
                for elem in elements.iter() {
                    self.visit(elem.type_id);
                }
            }
            TypeData::Function(shape_id) => {
                let shape = self.types.function_shape(*shape_id);
                for param in &shape.params {
                    self.visit(param.type_id);
                }
                self.visit(shape.return_type);
                if let Some(this_type) = shape.this_type {
                    self.visit(this_type);
                }
                if let Some(ref type_predicate) = shape.type_predicate
                    && let Some(type_id) = type_predicate.type_id
                {
                    self.visit(type_id);
                }
                for type_param in &shape.type_params {
                    if let Some(constraint) = type_param.constraint {
                        self.visit(constraint);
                    }
                    if let Some(default) = type_param.default {
                        self.visit(default);
                    }
                }
            }
            TypeData::Callable(shape_id) => {
                let shape = self.types.callable_shape(*shape_id);
                for sig in &shape.call_signatures {
                    for param in &sig.params {
                        self.visit(param.type_id);
                    }
                    self.visit(sig.return_type);
                    if let Some(this_type) = sig.this_type {
                        self.visit(this_type);
                    }
                    if let Some(ref type_predicate) = sig.type_predicate
                        && let Some(type_id) = type_predicate.type_id
                    {
                        self.visit(type_id);
                    }
                    for type_param in &sig.type_params {
                        if let Some(constraint) = type_param.constraint {
                            self.visit(constraint);
                        }
                        if let Some(default) = type_param.default {
                            self.visit(default);
                        }
                    }
                }
                for sig in &shape.construct_signatures {
                    for param in &sig.params {
                        self.visit(param.type_id);
                    }
                    self.visit(sig.return_type);
                    if let Some(this_type) = sig.this_type {
                        self.visit(this_type);
                    }
                    if let Some(ref type_predicate) = sig.type_predicate
                        && let Some(type_id) = type_predicate.type_id
                    {
                        self.visit(type_id);
                    }
                    for type_param in &sig.type_params {
                        if let Some(constraint) = type_param.constraint {
                            self.visit(constraint);
                        }
                        if let Some(default) = type_param.default {
                            self.visit(default);
                        }
                    }
                }
                for prop in &shape.properties {
                    self.visit(prop.type_id);
                    self.visit(prop.write_type);
                }
                if let Some(ref sig) = shape.string_index {
                    self.visit(sig.key_type);
                    self.visit(sig.value_type);
                }
                if let Some(ref sig) = shape.number_index {
                    self.visit(sig.key_type);
                    self.visit(sig.value_type);
                }
            }
            TypeData::TypeParameter(info) | TypeData::Infer(info) => {
                if let Some(constraint) = info.constraint {
                    self.visit(constraint);
                }
                if let Some(default) = info.default {
                    self.visit(default);
                }
            }
            TypeData::Application(app_id) => {
                let app = self.types.type_application(*app_id);
                self.visit(app.base);
                for &arg in &app.args {
                    self.visit(arg);
                }
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.types.conditional_type(*cond_id);
                self.visit(cond.check_type);
                self.visit(cond.extends_type);
                self.visit(cond.true_type);
                self.visit(cond.false_type);
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.types.mapped_type(*mapped_id);
                if let Some(constraint) = mapped.type_param.constraint {
                    self.visit(constraint);
                }
                if let Some(default) = mapped.type_param.default {
                    self.visit(default);
                }
                self.visit(mapped.constraint);
                self.visit(mapped.template);
                if let Some(name_type) = mapped.name_type {
                    self.visit(name_type);
                }
            }
            TypeData::IndexAccess(obj, idx) => {
                self.visit(*obj);
                self.visit(*idx);
            }
            TypeData::TemplateLiteral(list_id) => {
                let spans = self.types.template_list(*list_id);
                for span in spans.iter() {
                    if let crate::types::TemplateSpan::Type(type_id) = span {
                        self.visit(*type_id);
                    }
                }
            }
            TypeData::KeyOf(inner) | TypeData::ReadonlyType(inner) => {
                self.visit(*inner);
            }
            TypeData::StringIntrinsic { type_arg, .. } => {
                self.visit(*type_arg);
            }
            TypeData::Enum(_def_id, member_type) => {
                // Traverse into the structural member type
                self.visit(*member_type);
            }
        }
    }
}

/// Collect all types recursively reachable from a root type.
pub fn collect_all_types(types: &dyn TypeDatabase, type_id: TypeId) -> FxHashSet<TypeId> {
    let mut collector = RecursiveTypeCollector::new(types);
    collector.collect(type_id)
}

// =============================================================================
// Type Contains Visitor - Check if a type contains specific types
// =============================================================================

/// Check if a type contains any type parameters.
pub fn contains_type_parameters(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_type_matching(types, type_id, |key| {
        matches!(key, TypeData::TypeParameter(_) | TypeData::Infer(_))
    })
}

/// Check if a type contains any `infer` types.
pub fn contains_infer_types(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_type_matching(types, type_id, |key| matches!(key, TypeData::Infer(_)))
}

/// Check if a type contains the error type.
pub fn contains_error_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::ERROR {
        return true;
    }
    contains_type_matching(types, type_id, |key| matches!(key, TypeData::Error))
}

/// Check if a type contains the `this` type anywhere.
pub fn contains_this_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_type_matching(types, type_id, |key| matches!(key, TypeData::ThisType))
}

/// Check if a type contains any type matching a predicate.
pub fn contains_type_matching<F>(types: &dyn TypeDatabase, type_id: TypeId, predicate: F) -> bool
where
    F: Fn(&TypeData) -> bool,
{
    let mut checker = ContainsTypeChecker {
        types,
        predicate,
        memo: FxHashMap::default(),
        guard: crate::recursion::RecursionGuard::with_profile(
            crate::recursion::RecursionProfile::ShallowTraversal,
        ),
    };
    checker.check(type_id)
}

struct ContainsTypeChecker<'a, F>
where
    F: Fn(&TypeData) -> bool,
{
    types: &'a dyn TypeDatabase,
    predicate: F,
    memo: FxHashMap<TypeId, bool>,
    guard: crate::recursion::RecursionGuard<TypeId>,
}

impl<'a, F> ContainsTypeChecker<'a, F>
where
    F: Fn(&TypeData) -> bool,
{
    fn check(&mut self, type_id: TypeId) -> bool {
        if let Some(&cached) = self.memo.get(&type_id) {
            return cached;
        }

        match self.guard.enter(type_id) {
            crate::recursion::RecursionResult::Entered => {}
            _ => return false,
        }

        let Some(key) = self.types.lookup(type_id) else {
            self.guard.leave(type_id);
            return false;
        };

        if (self.predicate)(&key) {
            self.guard.leave(type_id);
            self.memo.insert(type_id, true);
            return true;
        }

        let result = self.check_key(&key);

        self.guard.leave(type_id);
        self.memo.insert(type_id, result);

        result
    }

    fn check_key(&mut self, key: &TypeData) -> bool {
        match key {
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Error
            | TypeData::ThisType
            | TypeData::BoundParameter(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_) => false,
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.types.object_shape(*shape_id);
                shape.properties.iter().any(|p| self.check(p.type_id))
                    || shape
                        .string_index
                        .as_ref()
                        .is_some_and(|i| self.check(i.value_type))
                    || shape
                        .number_index
                        .as_ref()
                        .is_some_and(|i| self.check(i.value_type))
            }
            TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
                let members = self.types.type_list(*list_id);
                members.iter().any(|&m| self.check(m))
            }
            TypeData::Array(elem) => self.check(*elem),
            TypeData::Tuple(list_id) => {
                let elements = self.types.tuple_list(*list_id);
                elements.iter().any(|e| self.check(e.type_id))
            }
            TypeData::Function(shape_id) => {
                let shape = self.types.function_shape(*shape_id);
                shape.params.iter().any(|p| self.check(p.type_id))
                    || self.check(shape.return_type)
                    || shape.this_type.is_some_and(|t| self.check(t))
            }
            TypeData::Callable(shape_id) => {
                let shape = self.types.callable_shape(*shape_id);
                shape.call_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id)) || self.check(s.return_type)
                }) || shape.construct_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id)) || self.check(s.return_type)
                }) || shape.properties.iter().any(|p| self.check(p.type_id))
            }
            TypeData::TypeParameter(info) | TypeData::Infer(info) => {
                info.constraint.is_some_and(|c| self.check(c))
                    || info.default.is_some_and(|d| self.check(d))
            }
            TypeData::Application(app_id) => {
                let app = self.types.type_application(*app_id);
                self.check(app.base) || app.args.iter().any(|&a| self.check(a))
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.types.conditional_type(*cond_id);
                self.check(cond.check_type)
                    || self.check(cond.extends_type)
                    || self.check(cond.true_type)
                    || self.check(cond.false_type)
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.types.mapped_type(*mapped_id);
                mapped.type_param.constraint.is_some_and(|c| self.check(c))
                    || mapped.type_param.default.is_some_and(|d| self.check(d))
                    || self.check(mapped.constraint)
                    || self.check(mapped.template)
                    || mapped.name_type.is_some_and(|n| self.check(n))
            }
            TypeData::IndexAccess(obj, idx) => self.check(*obj) || self.check(*idx),
            TypeData::TemplateLiteral(list_id) => {
                let spans = self.types.template_list(*list_id);
                spans.iter().any(|span| {
                    if let crate::types::TemplateSpan::Type(type_id) = span {
                        self.check(*type_id)
                    } else {
                        false
                    }
                })
            }
            TypeData::KeyOf(inner) | TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
                self.check(*inner)
            }
            TypeData::StringIntrinsic { type_arg, .. } => self.check(*type_arg),
            TypeData::Enum(_def_id, member_type) => self.check(*member_type),
        }
    }
}

// =============================================================================
// TypeDatabase-based convenience functions
// =============================================================================

/// Check if a type is a literal type (`TypeDatabase` version).
pub fn is_literal_type_db(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    LiteralTypeChecker::check(types, type_id)
}

/// Check if a type is a module namespace type (`TypeDatabase` version).
pub fn is_module_namespace_type_db(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeData::ModuleNamespace(_)))
}

/// Check if a type is a function type (`TypeDatabase` version).
pub fn is_function_type_db(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    FunctionTypeChecker::check(types, type_id)
}

/// Check if a type is object-like (`TypeDatabase` version).
pub fn is_object_like_type_db(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    ObjectTypeChecker::check(types, type_id)
}

/// Check if a type is an empty object type (`TypeDatabase` version).
pub fn is_empty_object_type_db(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let checker = EmptyObjectChecker::new(types);
    checker.check(type_id)
}

// =============================================================================
// Object Type Classification
// =============================================================================

/// Classification of object types for freshness tracking.
pub enum ObjectTypeKind {
    /// A regular object type (no index signatures).
    Object(ObjectShapeId),
    /// An object type with index signatures.
    ObjectWithIndex(ObjectShapeId),
    /// Not an object type.
    NotObject,
}

/// Classify a type as an object type kind.
///
/// This is used by the freshness tracking system to determine if a type
/// is a fresh object literal that needs special handling.
pub fn classify_object_type(types: &dyn TypeDatabase, type_id: TypeId) -> ObjectTypeKind {
    match types.lookup(type_id) {
        Some(TypeData::Object(shape_id)) => ObjectTypeKind::Object(shape_id),
        Some(TypeData::ObjectWithIndex(shape_id)) => ObjectTypeKind::ObjectWithIndex(shape_id),
        _ => ObjectTypeKind::NotObject,
    }
}

// =============================================================================
// Visitor Pattern Implementations for Helper Functions
// =============================================================================

/// Visitor to check if a type is a literal type.
struct LiteralTypeChecker;

impl LiteralTypeChecker {
    fn check(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
        match types.lookup(type_id) {
            Some(TypeData::Literal(_)) => true,
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
                Self::check(types, inner)
            }
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
                info.constraint.is_some_and(|c| Self::check(types, c))
            }
            _ => false,
        }
    }
}

/// Visitor to check if a type is a function type.
struct FunctionTypeChecker;

impl FunctionTypeChecker {
    fn check(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
        match types.lookup(type_id) {
            Some(TypeData::Function(_) | TypeData::Callable(_)) => true,
            Some(TypeData::Intersection(members)) => {
                let members = types.type_list(members);
                members.iter().any(|&member| Self::check(types, member))
            }
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
                info.constraint.is_some_and(|c| Self::check(types, c))
            }
            _ => false,
        }
    }
}

/// Visitor to check if a type is object-like.
struct ObjectTypeChecker;

impl ObjectTypeChecker {
    fn check(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
        match types.lookup(type_id) {
            Some(
                TypeData::Object(_)
                | TypeData::ObjectWithIndex(_)
                | TypeData::Array(_)
                | TypeData::Tuple(_)
                | TypeData::Mapped(_),
            ) => true,
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
                Self::check(types, inner)
            }
            Some(TypeData::Intersection(members)) => {
                let members = types.type_list(members);
                members.iter().all(|&member| Self::check(types, member))
            }
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info
                .constraint
                .is_some_and(|constraint| Self::check(types, constraint)),
            _ => false,
        }
    }
}

/// Visitor to check if a type is an empty object type.
struct EmptyObjectChecker<'a> {
    db: &'a dyn TypeDatabase,
}

impl<'a> EmptyObjectChecker<'a> {
    fn new(db: &'a dyn TypeDatabase) -> Self {
        Self { db }
    }

    fn check(&self, type_id: TypeId) -> bool {
        match self.db.lookup(type_id) {
            Some(TypeData::Object(shape_id)) => {
                let shape = self.db.object_shape(shape_id);
                shape.properties.is_empty()
            }
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.db.object_shape(shape_id);
                shape.properties.is_empty()
                    && shape.string_index.is_none()
                    && shape.number_index.is_none()
            }
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => self.check(inner),
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
                info.constraint.is_some_and(|c| self.check(c))
            }
            _ => false,
        }
    }
}

// =============================================================================
// Const Assertion Visitor
// =============================================================================

/// Visitor that applies `as const` transformation to a type.
///
/// This visitor implements the const assertion logic from TypeScript:
/// - Literals: Preserved as-is
/// - Arrays: Converted to readonly tuples
/// - Tuples: Marked readonly, elements recursively const-asserted
/// - Objects: All properties marked readonly, recursively const-asserted
/// - Other types: Preserved as-is (any, unknown, primitives, etc.)
pub struct ConstAssertionVisitor<'a> {
    /// The type database/interner.
    pub db: &'a dyn TypeDatabase,
    /// Unified recursion guard for cycle detection.
    pub guard: crate::recursion::RecursionGuard<TypeId>,
}

impl<'a> ConstAssertionVisitor<'a> {
    /// Create a new `ConstAssertionVisitor`.
    pub fn new(db: &'a dyn TypeDatabase) -> Self {
        Self {
            db,
            guard: crate::recursion::RecursionGuard::with_profile(
                crate::recursion::RecursionProfile::ConstAssertion,
            ),
        }
    }

    /// Apply const assertion to a type, returning the transformed type ID.
    pub fn apply_const_assertion(&mut self, type_id: TypeId) -> TypeId {
        // Prevent infinite recursion
        match self.guard.enter(type_id) {
            crate::recursion::RecursionResult::Entered => {}
            _ => return type_id,
        }

        let result = match self.db.lookup(type_id) {
            // Arrays: Convert to readonly tuple
            Some(TypeData::Array(element_type)) => {
                let const_element = self.apply_const_assertion(element_type);
                // Arrays become readonly tuples when const-asserted
                let tuple_elem = TupleElement {
                    type_id: const_element,
                    name: None,
                    optional: false,
                    rest: false,
                };
                let tuple_type = self.db.tuple(vec![tuple_elem]);
                self.db.readonly_type(tuple_type)
            }

            // Tuples: Mark readonly and recurse on elements
            Some(TypeData::Tuple(list_id)) => {
                let elements = self.db.tuple_list(list_id);
                let const_elements: Vec<TupleElement> = elements
                    .iter()
                    .map(|elem| {
                        let const_type = self.apply_const_assertion(elem.type_id);
                        TupleElement {
                            type_id: const_type,
                            name: elem.name,
                            optional: elem.optional,
                            rest: elem.rest,
                        }
                    })
                    .collect();
                let tuple_type = self.db.tuple(const_elements);
                self.db.readonly_type(tuple_type)
            }

            // Objects: Mark all properties readonly and recurse
            Some(TypeData::Object(shape_id)) => {
                let shape = self.db.object_shape(shape_id);
                let mut new_props = Vec::with_capacity(shape.properties.len());

                for prop in &shape.properties {
                    let const_prop_type = self.apply_const_assertion(prop.type_id);
                    let const_write_type = self.apply_const_assertion(prop.write_type);
                    new_props.push(crate::types::PropertyInfo {
                        name: prop.name,
                        type_id: const_prop_type,
                        write_type: const_write_type,
                        optional: prop.optional,
                        readonly: true, // Mark as readonly
                        is_method: prop.is_method,
                        visibility: prop.visibility,
                        parent_id: prop.parent_id,
                    });
                }

                self.db.object(new_props)
            }

            // Objects with index signatures
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.db.object_shape(shape_id);
                let mut new_props = Vec::with_capacity(shape.properties.len());

                for prop in &shape.properties {
                    let const_prop_type = self.apply_const_assertion(prop.type_id);
                    let const_write_type = self.apply_const_assertion(prop.write_type);
                    new_props.push(crate::types::PropertyInfo {
                        name: prop.name,
                        type_id: const_prop_type,
                        write_type: const_write_type,
                        optional: prop.optional,
                        readonly: true, // Mark as readonly
                        is_method: prop.is_method,
                        visibility: prop.visibility,
                        parent_id: prop.parent_id,
                    });
                }

                // Mark index signatures as readonly
                let string_index =
                    shape
                        .string_index
                        .as_ref()
                        .map(|idx| crate::types::IndexSignature {
                            key_type: idx.key_type,
                            value_type: self.apply_const_assertion(idx.value_type),
                            readonly: true,
                        });

                let number_index =
                    shape
                        .number_index
                        .as_ref()
                        .map(|idx| crate::types::IndexSignature {
                            key_type: idx.key_type,
                            value_type: self.apply_const_assertion(idx.value_type),
                            readonly: true,
                        });

                let mut new_shape = (*shape).clone();
                new_shape.properties = new_props;
                new_shape.string_index = string_index;
                new_shape.number_index = number_index;

                self.db.object_with_index(new_shape)
            }

            // Readonly types: Unwrap, process, re-wrap
            Some(TypeData::ReadonlyType(inner)) => {
                let const_inner = self.apply_const_assertion(inner);
                self.db.readonly_type(const_inner)
            }

            // Unions: Recursively apply to all members
            Some(TypeData::Union(list_id)) => {
                let members = self.db.type_list(list_id);
                let const_members: Vec<TypeId> = members
                    .iter()
                    .map(|&m| self.apply_const_assertion(m))
                    .collect();
                self.db.union(const_members)
            }

            // Intersections: Recursively apply to all members
            Some(TypeData::Intersection(list_id)) => {
                let members = self.db.type_list(list_id);
                let const_members: Vec<TypeId> = members
                    .iter()
                    .map(|&m| self.apply_const_assertion(m))
                    .collect();
                self.db.intersection(const_members)
            }

            // All other types: preserved as-is
            _ => type_id,
        };

        self.guard.leave(type_id);
        result
    }
}
