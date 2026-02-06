//! Type Visitor Pattern
//!
//! This module implements the Visitor pattern for TypeKey operations,
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
//! ```rust
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
    CallableShapeId, ConditionalTypeId, FunctionShapeId, IntrinsicKind, MappedTypeId,
    ObjectShapeId, OrderedFloat, StringIntrinsicKind, TemplateLiteralId, TupleElement, TupleListId,
    TypeApplicationId, TypeListId, TypeParamInfo,
};
use crate::{LiteralValue, SymbolRef, TypeDatabase, TypeId, TypeKey};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::interner::Atom;

// =============================================================================
// Type Visitor Trait
// =============================================================================

/// Visitor pattern for TypeKey traversal and transformation.
///
/// Implement this trait to perform custom operations on types without
/// writing repetitive match statements. Each method corresponds to a
/// TypeKey variant and receives the relevant data for that type.
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

    /// Visit a lazy type reference using DefId.
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

    /// Visit a TypeKey by dispatching to the appropriate method.
    fn visit_type_key(&mut self, _types: &dyn TypeDatabase, type_key: &TypeKey) -> Self::Output {
        match type_key {
            TypeKey::Intrinsic(kind) => self.visit_intrinsic(*kind),
            TypeKey::Literal(value) => self.visit_literal(value),
            TypeKey::Object(id) => self.visit_object(id.0),
            TypeKey::ObjectWithIndex(id) => self.visit_object_with_index(id.0),
            TypeKey::Union(id) => self.visit_union(id.0),
            TypeKey::Intersection(id) => self.visit_intersection(id.0),
            TypeKey::Array(element_type) => self.visit_array(*element_type),
            TypeKey::Tuple(id) => self.visit_tuple(id.0),
            TypeKey::Function(id) => self.visit_function(id.0),
            TypeKey::Callable(id) => self.visit_callable(id.0),
            TypeKey::TypeParameter(info) => self.visit_type_parameter(info),
            TypeKey::BoundParameter(index) => self.visit_bound_parameter(*index),
            TypeKey::Lazy(def_id) => self.visit_lazy(def_id.0),
            TypeKey::Recursive(index) => self.visit_recursive(*index),
            TypeKey::Enum(def_id, member_type) => self.visit_enum(def_id.0, *member_type),
            TypeKey::Application(id) => self.visit_application(id.0),
            TypeKey::Conditional(id) => self.visit_conditional(id.0),
            TypeKey::Mapped(id) => self.visit_mapped(id.0),
            TypeKey::IndexAccess(obj, key) => self.visit_index_access(*obj, *key),
            TypeKey::TemplateLiteral(id) => self.visit_template_literal(id.0),
            TypeKey::TypeQuery(sym_ref) => self.visit_type_query(sym_ref.0),
            TypeKey::KeyOf(type_id) => self.visit_keyof(*type_id),
            TypeKey::ReadonlyType(inner) => self.visit_readonly_type(*inner),
            TypeKey::UniqueSymbol(sym_ref) => self.visit_unique_symbol(sym_ref.0),
            TypeKey::Infer(info) => self.visit_infer(info),
            TypeKey::ThisType => self.visit_this_type(),
            TypeKey::StringIntrinsic { kind, type_arg } => {
                self.visit_string_intrinsic(*kind, *type_arg)
            }
            TypeKey::ModuleNamespace(sym_ref) => self.visit_module_namespace(sym_ref.0),
            TypeKey::Error => self.visit_error(),
        }
    }
}

// =============================================================================
// Type Traversal Helpers
// =============================================================================

/// Invoke a function on each immediate child TypeId of a TypeKey.
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
/// * `key` - The TypeKey whose children should be visited
/// * `f` - Function to call for each child TypeId
///
/// # Examples
///
/// ```rust
/// use crate::visitor::for_each_child;
///
/// for_each_child(types, &type_key, |child_id| {
///     // Process each nested type
/// });
/// ```
///
/// # TypeKey Variants Handled
///
/// This function handles ALL TypeKey variants to ensure complete traversal:
/// - **Single nested types**: Array, ReadonlyType, KeyOf, etc.
/// - **Multiple members**: Union, Intersection
/// - **Structured types**: Object, Tuple, Function, Callable
/// - **Complex types**: Application, Conditional, Mapped, IndexAccess
/// - **Template literals**: Iterates over template spans
/// - **String intrinsics**: Visits type argument
/// - **Leaf types**: Intrinsic, Literal, Lazy, TypeQuery, etc. (no children)
pub fn for_each_child<F>(db: &dyn TypeDatabase, key: &TypeKey, mut f: F)
where
    F: FnMut(TypeId),
{
    match key {
        // Single nested type
        TypeKey::Array(inner) | TypeKey::ReadonlyType(inner) | TypeKey::KeyOf(inner) => {
            f(*inner);
        }

        // Composite types with multiple members
        TypeKey::Union(list_id) | TypeKey::Intersection(list_id) => {
            for &member in db.type_list(*list_id).iter() {
                f(member);
            }
        }

        // Object types with properties and index signatures
        TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
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
        TypeKey::Tuple(tuple_id) => {
            for elem in db.tuple_list(*tuple_id).iter() {
                f(elem.type_id);
            }
        }

        // Function types
        TypeKey::Function(func_id) => {
            let sig = db.function_shape(*func_id);
            f(sig.return_type);
            if let Some(this_type) = sig.this_type {
                f(this_type);
            }
            if let Some(ref type_predicate) = sig.type_predicate {
                if let Some(type_id) = type_predicate.type_id {
                    f(type_id);
                }
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
        TypeKey::Callable(callable_id) => {
            let callable = db.callable_shape(*callable_id);
            for sig in &callable.call_signatures {
                f(sig.return_type);
                if let Some(this_type) = sig.this_type {
                    f(this_type);
                }
                if let Some(ref type_predicate) = sig.type_predicate {
                    if let Some(type_id) = type_predicate.type_id {
                        f(type_id);
                    }
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
                if let Some(ref type_predicate) = sig.type_predicate {
                    if let Some(type_id) = type_predicate.type_id {
                        f(type_id);
                    }
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
        TypeKey::Application(app_id) => {
            let app = db.type_application(*app_id);
            f(app.base);
            for &arg in &app.args {
                f(arg);
            }
        }

        // Conditional types
        TypeKey::Conditional(cond_id) => {
            let cond = db.conditional_type(*cond_id);
            f(cond.check_type);
            f(cond.extends_type);
            f(cond.true_type);
            f(cond.false_type);
        }

        // Mapped types
        TypeKey::Mapped(mapped_id) => {
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
        TypeKey::IndexAccess(obj, idx) => {
            f(*obj);
            f(*idx);
        }

        // Template literal types
        TypeKey::TemplateLiteral(template_id) => {
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
        TypeKey::StringIntrinsic { type_arg, .. } => {
            f(*type_arg);
        }

        // Type parameters with constraints
        TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
            if let Some(constraint) = info.constraint {
                f(constraint);
            }
        }

        // Enum types
        TypeKey::Enum(_def_id, member_type) => {
            f(*member_type);
        }

        // Leaf types - no children to visit
        TypeKey::Intrinsic(_)
        | TypeKey::Literal(_)
        | TypeKey::Lazy(_)
        | TypeKey::Recursive(_)
        | TypeKey::BoundParameter(_)
        | TypeKey::TypeQuery(_)
        | TypeKey::UniqueSymbol(_)
        | TypeKey::ThisType
        | TypeKey::ModuleNamespace(_)
        | TypeKey::Error => {}
    }
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
    /// KeyOf types (keyof T)
    KeyOf,
    /// Named type references (interfaces, type aliases)
    Reference,
    /// Error types
    Error,
    /// Other/unknown
    Other,
}

/// Visitor that checks if a type is a specific TypeKind.
pub struct TypeKindVisitor {
    /// The kind to check for.
    pub target_kind: TypeKind,
}

impl TypeKindVisitor {
    /// Create a new TypeKindVisitor.
    pub fn new(target_kind: TypeKind) -> Self {
        Self { target_kind }
    }

    /// Get the kind of a type from its TypeKey.
    pub fn get_kind(type_key: &TypeKey) -> TypeKind {
        match type_key {
            TypeKey::Intrinsic(_) => TypeKind::Primitive,
            TypeKey::Literal(_) => TypeKind::Literal,
            TypeKey::Object(_) | TypeKey::ObjectWithIndex(_) => TypeKind::Object,
            TypeKey::Array(_) => TypeKind::Array,
            TypeKey::Tuple(_) => TypeKind::Tuple,
            TypeKey::Union(_) => TypeKind::Union,
            TypeKey::Intersection(_) => TypeKind::Intersection,
            TypeKey::Function(_) | TypeKey::Callable(_) => TypeKind::Function,
            TypeKey::Application(_) => TypeKind::Generic,
            TypeKey::TypeParameter(_) | TypeKey::Infer(_) | TypeKey::BoundParameter(_) => {
                TypeKind::TypeParameter
            }
            TypeKey::Conditional(_) => TypeKind::Conditional,
            TypeKey::Lazy(_) | TypeKey::Recursive(_) => TypeKind::Reference,
            TypeKey::Enum(_, _) => TypeKind::Primitive, // enums behave like primitives
            TypeKey::Mapped(_) => TypeKind::Mapped,
            TypeKey::IndexAccess(_, _) => TypeKind::IndexAccess,
            TypeKey::TemplateLiteral(_) => TypeKind::TemplateLiteral,
            TypeKey::TypeQuery(_) => TypeKind::TypeQuery,
            TypeKey::KeyOf(_) => TypeKind::KeyOf,
            TypeKey::ReadonlyType(_inner) => {
                // Readonly doesn't change the kind - look through it
                // Note: This requires lookup which we don't have here
                // For now, return Other and let callers handle it
                TypeKind::Other
            }
            TypeKey::UniqueSymbol(_) => TypeKind::Primitive, // unique symbol is a primitive
            TypeKey::ThisType => TypeKind::TypeParameter,    // this is type-parameter-like
            TypeKey::StringIntrinsic { .. } => TypeKind::Primitive, // string intrinsics produce strings
            TypeKey::ModuleNamespace(_) => TypeKind::Object, // module namespace is object-like
            TypeKey::Error => TypeKind::Error,
        }
    }

    /// Get the kind of a type by TypeId.
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

    fn visit_type_key(&mut self, _types: &dyn TypeDatabase, type_key: &TypeKey) -> Self::Output {
        Self::get_kind(type_key) == self.target_kind
    }

    fn default_output() -> Self::Output {
        false
    }
}

/// Visitor that collects all TypeIds referenced by a type.
///
/// Useful for finding dependencies or tracking type usage.
pub struct TypeCollectorVisitor {
    /// Set of collected type IDs.
    pub types: FxHashSet<TypeId>,
    /// Maximum depth to traverse.
    pub max_depth: usize,
}

impl TypeCollectorVisitor {
    /// Create a new TypeCollectorVisitor.
    pub fn new() -> Self {
        Self {
            types: FxHashSet::default(),
            max_depth: 10,
        }
    }

    /// Create a new TypeCollectorVisitor with custom max depth.
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
    F: Fn(&TypeKey) -> bool,
{
    /// Predicate function to test against TypeKey.
    pub predicate: F,
}

impl<F> TypePredicateVisitor<F>
where
    F: Fn(&TypeKey) -> bool,
{
    /// Create a new TypePredicateVisitor.
    pub fn new(predicate: F) -> Self {
        Self { predicate }
    }
}

impl<F> TypeVisitor for TypePredicateVisitor<F>
where
    F: Fn(&TypeKey) -> bool,
{
    type Output = bool;

    fn visit_type_key(&mut self, _types: &dyn TypeDatabase, type_key: &TypeKey) -> Self::Output {
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

/// Check if a type is a specific kind using the TypeKindVisitor.
///
/// # Example
///
/// ```rust
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
/// ```rust
/// use crate::visitor::collect_referenced_types;
///
/// let types = collect_referenced_types(&type_interner, type_id);
/// ```
pub fn collect_referenced_types(types: &dyn TypeDatabase, type_id: TypeId) -> FxHashSet<TypeId> {
    let mut visitor = TypeCollectorVisitor::new();
    visitor.visit_type(types, type_id);
    visitor.types
}

/// Test a type against a predicate function.
///
/// # Example
///
/// ```rust
/// use crate::{TypeKey, LiteralValue, visitor::test_type};
///
/// let is_string_literal = test_type(&types, type_id, |key| {
///     matches!(key, TypeKey::Literal(LiteralValue::String(_)))
/// });
/// ```
pub fn test_type<F>(types: &dyn TypeDatabase, type_id: TypeId, predicate: F) -> bool
where
    F: Fn(&TypeKey) -> bool,
{
    let mut visitor = TypePredicateVisitor::new(predicate);
    visitor.visit_type(types, type_id)
}

// =============================================================================
// Type Data Extraction Helpers
// =============================================================================

struct TypeKeyDataVisitor<F, T>
where
    F: Fn(&TypeKey) -> Option<T>,
{
    extractor: F,
}

impl<F, T> TypeKeyDataVisitor<F, T>
where
    F: Fn(&TypeKey) -> Option<T>,
{
    fn new(extractor: F) -> Self {
        Self { extractor }
    }
}

impl<F, T> TypeVisitor for TypeKeyDataVisitor<F, T>
where
    F: Fn(&TypeKey) -> Option<T>,
{
    type Output = Option<T>;

    fn visit_type_key(&mut self, _types: &dyn TypeDatabase, type_key: &TypeKey) -> Self::Output {
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
    F: Fn(&TypeKey) -> Option<T>,
{
    let mut visitor = TypeKeyDataVisitor::new(extractor);
    visitor.visit_type(types, type_id)
}

/// Extract the union list id if this is a union type.
pub fn union_list_id(types: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeListId> {
    extract_type_data(types, type_id, |key| match key {
        TypeKey::Union(list_id) => Some(*list_id),
        _ => None,
    })
}

/// Extract the intersection list id if this is an intersection type.
pub fn intersection_list_id(types: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeListId> {
    extract_type_data(types, type_id, |key| match key {
        TypeKey::Intersection(list_id) => Some(*list_id),
        _ => None,
    })
}

/// Extract the object shape id if this is an object type.
pub fn object_shape_id(types: &dyn TypeDatabase, type_id: TypeId) -> Option<ObjectShapeId> {
    extract_type_data(types, type_id, |key| match key {
        TypeKey::Object(shape_id) => Some(*shape_id),
        _ => None,
    })
}

/// Extract the object-with-index shape id if this is an indexed object type.
pub fn object_with_index_shape_id(
    types: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<ObjectShapeId> {
    extract_type_data(types, type_id, |key| match key {
        TypeKey::ObjectWithIndex(shape_id) => Some(*shape_id),
        _ => None,
    })
}

/// Extract the array element type if this is an array type.
pub fn array_element_type(types: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    extract_type_data(types, type_id, |key| match key {
        TypeKey::Array(element) => Some(*element),
        _ => None,
    })
}

/// Extract the tuple list id if this is a tuple type.
pub fn tuple_list_id(types: &dyn TypeDatabase, type_id: TypeId) -> Option<TupleListId> {
    extract_type_data(types, type_id, |key| match key {
        TypeKey::Tuple(list_id) => Some(*list_id),
        _ => None,
    })
}

/// Extract the intrinsic kind if this is an intrinsic type.
pub fn intrinsic_kind(types: &dyn TypeDatabase, type_id: TypeId) -> Option<IntrinsicKind> {
    extract_type_data(types, type_id, |key| match key {
        TypeKey::Intrinsic(kind) => Some(*kind),
        _ => None,
    })
}

/// Extract the literal value if this is a literal type.
pub fn literal_value(types: &dyn TypeDatabase, type_id: TypeId) -> Option<LiteralValue> {
    extract_type_data(types, type_id, |key| match key {
        TypeKey::Literal(value) => Some(value.clone()),
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
        TypeKey::TemplateLiteral(list_id) => Some(*list_id),
        _ => None,
    })
}

/// Extract the type parameter info if this is a type parameter or infer type.
pub fn type_param_info(types: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeParamInfo> {
    extract_type_data(types, type_id, |key| match key {
        TypeKey::TypeParameter(info) | TypeKey::Infer(info) => Some(info.clone()),
        _ => None,
    })
}

/// Extract the type reference symbol if this is a Ref type.
pub fn ref_symbol(types: &dyn TypeDatabase, type_id: TypeId) -> Option<SymbolRef> {
    extract_type_data(types, type_id, |key| match key {
        TypeKey::Lazy(_def_id) => {
            // TypeKey::Ref has been migrated to TypeKey::Lazy(DefId)
            // We can no longer extract SymbolRef from it
            // Return None or handle as needed based on migration strategy
            None
        }
        _ => None,
    })
}

/// Extract the lazy DefId if this is a Lazy type.
pub fn lazy_def_id(types: &dyn TypeDatabase, type_id: TypeId) -> Option<DefId> {
    extract_type_data(types, type_id, |key| match key {
        TypeKey::Lazy(def_id) => Some(*def_id),
        _ => None,
    })
}

/// Check if this is an Enum type.
pub fn is_enum_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeKey::Enum(_, _)))
}

/// Extract the enum components (DefId and member type) if this is an Enum type.
///
/// Returns `Some((def_id, member_type))` where:
/// - `def_id` is the unique identity of the enum for nominal checking
/// - `member_type` is the structural union of member types (e.g., 0 | 1)
pub fn enum_components(types: &dyn TypeDatabase, type_id: TypeId) -> Option<(DefId, TypeId)> {
    extract_type_data(types, type_id, |key| match key {
        TypeKey::Enum(def_id, member_type) => Some((*def_id, *member_type)),
        _ => None,
    })
}

/// Extract the application id if this is a generic application type.
pub fn application_id(types: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeApplicationId> {
    extract_type_data(types, type_id, |key| match key {
        TypeKey::Application(app_id) => Some(*app_id),
        _ => None,
    })
}

/// Extract the mapped type id if this is a mapped type.
pub fn mapped_type_id(types: &dyn TypeDatabase, type_id: TypeId) -> Option<MappedTypeId> {
    extract_type_data(types, type_id, |key| match key {
        TypeKey::Mapped(mapped_id) => Some(*mapped_id),
        _ => None,
    })
}

/// Extract the conditional type id if this is a conditional type.
pub fn conditional_type_id(types: &dyn TypeDatabase, type_id: TypeId) -> Option<ConditionalTypeId> {
    extract_type_data(types, type_id, |key| match key {
        TypeKey::Conditional(cond_id) => Some(*cond_id),
        _ => None,
    })
}

/// Extract index access components if this is an index access type.
pub fn index_access_parts(types: &dyn TypeDatabase, type_id: TypeId) -> Option<(TypeId, TypeId)> {
    extract_type_data(types, type_id, |key| match key {
        TypeKey::IndexAccess(object_type, index_type) => Some((*object_type, *index_type)),
        _ => None,
    })
}

/// Extract the type query symbol if this is a TypeQuery.
pub fn type_query_symbol(types: &dyn TypeDatabase, type_id: TypeId) -> Option<SymbolRef> {
    extract_type_data(types, type_id, |key| match key {
        TypeKey::TypeQuery(sym_ref) => Some(*sym_ref),
        _ => None,
    })
}

/// Extract the inner type if this is a keyof type.
pub fn keyof_inner_type(types: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    extract_type_data(types, type_id, |key| match key {
        TypeKey::KeyOf(inner) => Some(*inner),
        _ => None,
    })
}

/// Extract the inner type if this is a readonly type.
pub fn readonly_inner_type(types: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    extract_type_data(types, type_id, |key| match key {
        TypeKey::ReadonlyType(inner) => Some(*inner),
        _ => None,
    })
}

/// Extract the unique symbol ref if this is a unique symbol type.
pub fn unique_symbol_ref(types: &dyn TypeDatabase, type_id: TypeId) -> Option<SymbolRef> {
    extract_type_data(types, type_id, |key| match key {
        TypeKey::UniqueSymbol(sym_ref) => Some(*sym_ref),
        _ => None,
    })
}

/// Check if a type is the special `this` type.
pub fn is_this_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    extract_type_data(types, type_id, |key| match key {
        TypeKey::ThisType => Some(true),
        _ => None,
    })
    .unwrap_or(false)
}

/// Extract the function shape id if this is a function type.
pub fn function_shape_id(types: &dyn TypeDatabase, type_id: TypeId) -> Option<FunctionShapeId> {
    extract_type_data(types, type_id, |key| match key {
        TypeKey::Function(shape_id) => Some(*shape_id),
        _ => None,
    })
}

/// Extract the callable shape id if this is a callable type.
pub fn callable_shape_id(types: &dyn TypeDatabase, type_id: TypeId) -> Option<CallableShapeId> {
    extract_type_data(types, type_id, |key| match key {
        TypeKey::Callable(shape_id) => Some(*shape_id),
        _ => None,
    })
}

// =============================================================================
// Specialized Type Predicate Visitors
// =============================================================================

/// Check if a type is a literal type.
///
/// Matches: TypeKey::Literal(_)
pub fn is_literal_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeKey::Literal(_)))
}

/// Check if a type is a module namespace type (import * as ns).
///
/// Matches: TypeKey::ModuleNamespace(_)
pub fn is_module_namespace_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeKey::ModuleNamespace(_)))
}

/// Check if a type is a function type (Function or Callable).
///
/// This also handles intersections containing function types.
pub fn is_function_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_function_type_impl(types, type_id)
}

fn is_function_type_impl(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match types.lookup(type_id) {
        Some(TypeKey::Function(_) | TypeKey::Callable(_)) => true,
        Some(TypeKey::Intersection(members)) => {
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
/// Returns true for: Object, ObjectWithIndex, Array, Tuple, Mapped, ReadonlyType (of object)
pub fn is_object_like_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_object_like_type_impl(types, type_id)
}

fn is_object_like_type_impl(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match types.lookup(type_id) {
        Some(TypeKey::Object(_))
        | Some(TypeKey::ObjectWithIndex(_))
        | Some(TypeKey::Array(_))
        | Some(TypeKey::Tuple(_))
        | Some(TypeKey::Mapped(_)) => true,
        Some(TypeKey::ReadonlyType(inner)) => is_object_like_type_impl(types, inner),
        Some(TypeKey::Intersection(members)) => {
            let members = types.type_list(members);
            members
                .iter()
                .all(|&member| is_object_like_type_impl(types, member))
        }
        Some(TypeKey::TypeParameter(info) | TypeKey::Infer(info)) => info
            .constraint
            .map(|constraint| is_object_like_type_impl(types, constraint))
            .unwrap_or(false),
        _ => false,
    }
}

/// Check if a type is an empty object type (no properties, no index signatures).
pub fn is_empty_object_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match types.lookup(type_id) {
        Some(TypeKey::Object(shape_id)) => {
            let shape = types.object_shape(shape_id);
            shape.properties.is_empty()
        }
        Some(TypeKey::ObjectWithIndex(shape_id)) => {
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
        Some(TypeKey::Intrinsic(_)) | Some(TypeKey::Literal(_))
    )
}

/// Check if a type is a union type.
pub fn is_union_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeKey::Union(_)))
}

/// Check if a type is an intersection type.
pub fn is_intersection_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeKey::Intersection(_)))
}

/// Check if a type is an array type.
pub fn is_array_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeKey::Array(_)))
}

/// Check if a type is a tuple type.
pub fn is_tuple_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeKey::Tuple(_)))
}

/// Check if a type is a type parameter.
pub fn is_type_parameter(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        types.lookup(type_id),
        Some(TypeKey::TypeParameter(_)) | Some(TypeKey::Infer(_))
    )
}

/// Check if a type is a conditional type.
pub fn is_conditional_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeKey::Conditional(_)))
}

/// Check if a type is a mapped type.
pub fn is_mapped_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeKey::Mapped(_)))
}

/// Check if a type is an index access type.
pub fn is_index_access_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeKey::IndexAccess(_, _)))
}

/// Check if a type is a template literal type.
pub fn is_template_literal_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeKey::TemplateLiteral(_)))
}

/// Check if a type is a type reference (Lazy/DefId).
pub fn is_type_reference(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        types.lookup(type_id),
        Some(TypeKey::Lazy(_) | TypeKey::Recursive(_))
    )
}

/// Check if a type is a generic type application.
pub fn is_generic_application(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeKey::Application(_)))
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
/// - Enum members (TypeKey::Enum)
/// - Unique symbols
/// - null, undefined, void
/// - Tuples where ALL elements are unit types (and no rest elements)
///
/// NOTE: This does NOT handle ReadonlyType - readonly tuples must be checked separately
/// because `["a"]` is a subtype of `readonly ["a"]` even though they have different TypeIds.
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
        // Literal types are unit types
        Some(TypeKey::Literal(_)) => true,

        // Enum members are unit types (nominal)
        Some(TypeKey::Enum(_, _)) => true,

        // Unique symbols are unit types
        Some(TypeKey::UniqueSymbol(_)) => true,

        // Tuples are unit types if ALL elements are unit types (no rest elements)
        Some(TypeKey::Tuple(list_id)) => {
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

        // ReadonlyType of a unit tuple is NOT considered a unit type for optimization purposes
        // because ["a"] <: readonly ["a"] but they have different TypeIds
        Some(TypeKey::ReadonlyType(_)) => false,

        // Everything else is not a unit type
        _ => false,
    }
}

// =============================================================================
// Recursive Type Visitor - Traverses into nested types
// =============================================================================

/// A visitor that recursively collects all types referenced by a root type.
/// Unlike TypeCollectorVisitor, this properly traverses into nested structures.
pub struct RecursiveTypeCollector<'a> {
    types: &'a dyn TypeDatabase,
    collected: FxHashSet<TypeId>,
    visiting: FxHashSet<TypeId>,
    max_depth: usize,
    current_depth: usize,
}

impl<'a> RecursiveTypeCollector<'a> {
    pub fn new(types: &'a dyn TypeDatabase) -> Self {
        Self {
            types,
            collected: FxHashSet::default(),
            visiting: FxHashSet::default(),
            max_depth: 20,
            current_depth: 0,
        }
    }

    pub fn with_max_depth(types: &'a dyn TypeDatabase, max_depth: usize) -> Self {
        Self {
            types,
            collected: FxHashSet::default(),
            visiting: FxHashSet::default(),
            max_depth,
            current_depth: 0,
        }
    }

    /// Collect all types reachable from the given type.
    pub fn collect(&mut self, type_id: TypeId) -> FxHashSet<TypeId> {
        self.visit(type_id);
        std::mem::take(&mut self.collected)
    }

    fn visit(&mut self, type_id: TypeId) {
        // Depth check
        if self.current_depth >= self.max_depth {
            return;
        }

        // Cycle check
        if self.visiting.contains(&type_id) {
            return;
        }

        // Already collected
        if self.collected.contains(&type_id) {
            return;
        }

        self.collected.insert(type_id);
        self.visiting.insert(type_id);
        self.current_depth += 1;

        if let Some(key) = self.types.lookup(type_id) {
            self.visit_key(&key);
        }

        self.current_depth -= 1;
        self.visiting.remove(&type_id);
    }

    fn visit_key(&mut self, key: &TypeKey) {
        match key {
            TypeKey::Intrinsic(_)
            | TypeKey::Literal(_)
            | TypeKey::Error
            | TypeKey::ThisType
            | TypeKey::BoundParameter(_) => {
                // Leaf types - nothing to traverse
            }
            TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.types.object_shape(*shape_id);
                for prop in shape.properties.iter() {
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
            TypeKey::Union(list_id) | TypeKey::Intersection(list_id) => {
                let members = self.types.type_list(*list_id);
                for &member in members.iter() {
                    self.visit(member);
                }
            }
            TypeKey::Array(elem) => {
                self.visit(*elem);
            }
            TypeKey::Tuple(list_id) => {
                let elements = self.types.tuple_list(*list_id);
                for elem in elements.iter() {
                    self.visit(elem.type_id);
                }
            }
            TypeKey::Function(shape_id) => {
                let shape = self.types.function_shape(*shape_id);
                for param in shape.params.iter() {
                    self.visit(param.type_id);
                }
                self.visit(shape.return_type);
                if let Some(this_type) = shape.this_type {
                    self.visit(this_type);
                }
                if let Some(ref type_predicate) = shape.type_predicate {
                    if let Some(type_id) = type_predicate.type_id {
                        self.visit(type_id);
                    }
                }
                for type_param in shape.type_params.iter() {
                    if let Some(constraint) = type_param.constraint {
                        self.visit(constraint);
                    }
                    if let Some(default) = type_param.default {
                        self.visit(default);
                    }
                }
            }
            TypeKey::Callable(shape_id) => {
                let shape = self.types.callable_shape(*shape_id);
                for sig in shape.call_signatures.iter() {
                    for param in sig.params.iter() {
                        self.visit(param.type_id);
                    }
                    self.visit(sig.return_type);
                    if let Some(this_type) = sig.this_type {
                        self.visit(this_type);
                    }
                    if let Some(ref type_predicate) = sig.type_predicate {
                        if let Some(type_id) = type_predicate.type_id {
                            self.visit(type_id);
                        }
                    }
                    for type_param in sig.type_params.iter() {
                        if let Some(constraint) = type_param.constraint {
                            self.visit(constraint);
                        }
                        if let Some(default) = type_param.default {
                            self.visit(default);
                        }
                    }
                }
                for sig in shape.construct_signatures.iter() {
                    for param in sig.params.iter() {
                        self.visit(param.type_id);
                    }
                    self.visit(sig.return_type);
                    if let Some(this_type) = sig.this_type {
                        self.visit(this_type);
                    }
                    if let Some(ref type_predicate) = sig.type_predicate {
                        if let Some(type_id) = type_predicate.type_id {
                            self.visit(type_id);
                        }
                    }
                    for type_param in sig.type_params.iter() {
                        if let Some(constraint) = type_param.constraint {
                            self.visit(constraint);
                        }
                        if let Some(default) = type_param.default {
                            self.visit(default);
                        }
                    }
                }
                for prop in shape.properties.iter() {
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
            TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
                if let Some(constraint) = info.constraint {
                    self.visit(constraint);
                }
                if let Some(default) = info.default {
                    self.visit(default);
                }
            }
            TypeKey::Lazy(_)
            | TypeKey::Recursive(_)
            | TypeKey::TypeQuery(_)
            | TypeKey::UniqueSymbol(_)
            | TypeKey::ModuleNamespace(_) => {
                // Symbol/DefId references - don't traverse (would need resolver)
            }
            TypeKey::Application(app_id) => {
                let app = self.types.type_application(*app_id);
                self.visit(app.base);
                for &arg in app.args.iter() {
                    self.visit(arg);
                }
            }
            TypeKey::Conditional(cond_id) => {
                let cond = self.types.conditional_type(*cond_id);
                self.visit(cond.check_type);
                self.visit(cond.extends_type);
                self.visit(cond.true_type);
                self.visit(cond.false_type);
            }
            TypeKey::Mapped(mapped_id) => {
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
            TypeKey::IndexAccess(obj, idx) => {
                self.visit(*obj);
                self.visit(*idx);
            }
            TypeKey::TemplateLiteral(list_id) => {
                let spans = self.types.template_list(*list_id);
                for span in spans.iter() {
                    if let crate::types::TemplateSpan::Type(type_id) = span {
                        self.visit(*type_id);
                    }
                }
            }
            TypeKey::KeyOf(inner) | TypeKey::ReadonlyType(inner) => {
                self.visit(*inner);
            }
            TypeKey::StringIntrinsic { type_arg, .. } => {
                self.visit(*type_arg);
            }
            TypeKey::Enum(_def_id, member_type) => {
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
        matches!(key, TypeKey::TypeParameter(_) | TypeKey::Infer(_))
    })
}

/// Check if a type contains any `infer` types.
pub fn contains_infer_types(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_type_matching(types, type_id, |key| matches!(key, TypeKey::Infer(_)))
}

/// Check if a type contains the error type.
pub fn contains_error_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::ERROR {
        return true;
    }
    contains_type_matching(types, type_id, |key| matches!(key, TypeKey::Error))
}

/// Check if a type contains the `this` type anywhere.
pub fn contains_this_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_type_matching(types, type_id, |key| matches!(key, TypeKey::ThisType))
}

/// Check if a type contains any type matching a predicate.
pub fn contains_type_matching<F>(types: &dyn TypeDatabase, type_id: TypeId, predicate: F) -> bool
where
    F: Fn(&TypeKey) -> bool,
{
    let mut checker = ContainsTypeChecker {
        types,
        predicate,
        memo: FxHashMap::default(),
        visiting: FxHashSet::default(),
        max_depth: 20,
        current_depth: 0,
    };
    checker.check(type_id)
}

struct ContainsTypeChecker<'a, F>
where
    F: Fn(&TypeKey) -> bool,
{
    types: &'a dyn TypeDatabase,
    predicate: F,
    memo: FxHashMap<TypeId, bool>,
    visiting: FxHashSet<TypeId>,
    max_depth: usize,
    current_depth: usize,
}

impl<'a, F> ContainsTypeChecker<'a, F>
where
    F: Fn(&TypeKey) -> bool,
{
    fn check(&mut self, type_id: TypeId) -> bool {
        if let Some(&cached) = self.memo.get(&type_id) {
            return cached;
        }
        if self.current_depth >= self.max_depth {
            return false;
        }
        if !self.visiting.insert(type_id) {
            return false;
        }

        let Some(key) = self.types.lookup(type_id) else {
            self.visiting.remove(&type_id);
            return false;
        };

        if (self.predicate)(&key) {
            self.visiting.remove(&type_id);
            self.memo.insert(type_id, true);
            return true;
        }

        self.current_depth += 1;

        let result = self.check_key(&key);

        self.current_depth -= 1;
        self.visiting.remove(&type_id);
        self.memo.insert(type_id, result);

        result
    }

    fn check_key(&mut self, key: &TypeKey) -> bool {
        match key {
            TypeKey::Intrinsic(_)
            | TypeKey::Literal(_)
            | TypeKey::Error
            | TypeKey::ThisType
            | TypeKey::BoundParameter(_) => false,
            TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.types.object_shape(*shape_id);
                shape.properties.iter().any(|p| self.check(p.type_id))
                    || shape
                        .string_index
                        .as_ref()
                        .map(|i| self.check(i.value_type))
                        .unwrap_or(false)
                    || shape
                        .number_index
                        .as_ref()
                        .map(|i| self.check(i.value_type))
                        .unwrap_or(false)
            }
            TypeKey::Union(list_id) | TypeKey::Intersection(list_id) => {
                let members = self.types.type_list(*list_id);
                members.iter().any(|&m| self.check(m))
            }
            TypeKey::Array(elem) => self.check(*elem),
            TypeKey::Tuple(list_id) => {
                let elements = self.types.tuple_list(*list_id);
                elements.iter().any(|e| self.check(e.type_id))
            }
            TypeKey::Function(shape_id) => {
                let shape = self.types.function_shape(*shape_id);
                shape.params.iter().any(|p| self.check(p.type_id))
                    || self.check(shape.return_type)
                    || shape.this_type.map(|t| self.check(t)).unwrap_or(false)
            }
            TypeKey::Callable(shape_id) => {
                let shape = self.types.callable_shape(*shape_id);
                shape.call_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id)) || self.check(s.return_type)
                }) || shape.construct_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id)) || self.check(s.return_type)
                }) || shape.properties.iter().any(|p| self.check(p.type_id))
            }
            TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
                info.constraint.map(|c| self.check(c)).unwrap_or(false)
                    || info.default.map(|d| self.check(d)).unwrap_or(false)
            }
            TypeKey::Lazy(_)
            | TypeKey::Recursive(_)
            | TypeKey::TypeQuery(_)
            | TypeKey::UniqueSymbol(_)
            | TypeKey::ModuleNamespace(_) => false,
            TypeKey::Application(app_id) => {
                let app = self.types.type_application(*app_id);
                self.check(app.base) || app.args.iter().any(|&a| self.check(a))
            }
            TypeKey::Conditional(cond_id) => {
                let cond = self.types.conditional_type(*cond_id);
                self.check(cond.check_type)
                    || self.check(cond.extends_type)
                    || self.check(cond.true_type)
                    || self.check(cond.false_type)
            }
            TypeKey::Mapped(mapped_id) => {
                let mapped = self.types.mapped_type(*mapped_id);
                mapped
                    .type_param
                    .constraint
                    .map(|c| self.check(c))
                    .unwrap_or(false)
                    || mapped
                        .type_param
                        .default
                        .map(|d| self.check(d))
                        .unwrap_or(false)
                    || self.check(mapped.constraint)
                    || self.check(mapped.template)
                    || mapped.name_type.map(|n| self.check(n)).unwrap_or(false)
            }
            TypeKey::IndexAccess(obj, idx) => self.check(*obj) || self.check(*idx),
            TypeKey::TemplateLiteral(list_id) => {
                let spans = self.types.template_list(*list_id);
                spans.iter().any(|span| {
                    if let crate::types::TemplateSpan::Type(type_id) = span {
                        self.check(*type_id)
                    } else {
                        false
                    }
                })
            }
            TypeKey::KeyOf(inner) | TypeKey::ReadonlyType(inner) => self.check(*inner),
            TypeKey::StringIntrinsic { type_arg, .. } => self.check(*type_arg),
            TypeKey::Enum(_def_id, member_type) => self.check(*member_type),
        }
    }
}

// =============================================================================
// TypeDatabase-based convenience functions
// =============================================================================

/// Check if a type is a literal type (TypeDatabase version).
pub fn is_literal_type_db(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    LiteralTypeChecker::check(types, type_id)
}

/// Check if a type is a module namespace type (TypeDatabase version).
pub fn is_module_namespace_type_db(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeKey::ModuleNamespace(_)))
}

/// Check if a type is a function type (TypeDatabase version).
pub fn is_function_type_db(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    FunctionTypeChecker::check(types, type_id)
}

/// Check if a type is object-like (TypeDatabase version).
pub fn is_object_like_type_db(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    ObjectTypeChecker::check(types, type_id)
}

/// Check if a type is an empty object type (TypeDatabase version).
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
        Some(TypeKey::Object(shape_id)) => ObjectTypeKind::Object(shape_id),
        Some(TypeKey::ObjectWithIndex(shape_id)) => ObjectTypeKind::ObjectWithIndex(shape_id),
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
            Some(TypeKey::Literal(_)) => true,
            Some(TypeKey::ReadonlyType(inner)) => Self::check(types, inner),
            Some(TypeKey::TypeParameter(info) | TypeKey::Infer(info)) => info
                .constraint
                .map(|c| Self::check(types, c))
                .unwrap_or(false),
            _ => false,
        }
    }
}

/// Visitor to check if a type is a function type.
struct FunctionTypeChecker;

impl FunctionTypeChecker {
    fn check(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
        match types.lookup(type_id) {
            Some(TypeKey::Function(_) | TypeKey::Callable(_)) => true,
            Some(TypeKey::Intersection(members)) => {
                let members = types.type_list(members);
                members.iter().any(|&member| Self::check(types, member))
            }
            Some(TypeKey::TypeParameter(info) | TypeKey::Infer(info)) => info
                .constraint
                .map(|c| Self::check(types, c))
                .unwrap_or(false),
            _ => false,
        }
    }
}

/// Visitor to check if a type is object-like.
struct ObjectTypeChecker;

impl ObjectTypeChecker {
    fn check(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
        match types.lookup(type_id) {
            Some(TypeKey::Object(_))
            | Some(TypeKey::ObjectWithIndex(_))
            | Some(TypeKey::Array(_))
            | Some(TypeKey::Tuple(_))
            | Some(TypeKey::Mapped(_)) => true,
            Some(TypeKey::ReadonlyType(inner)) => Self::check(types, inner),
            Some(TypeKey::Intersection(members)) => {
                let members = types.type_list(members);
                members.iter().all(|&member| Self::check(types, member))
            }
            Some(TypeKey::TypeParameter(info) | TypeKey::Infer(info)) => info
                .constraint
                .map(|constraint| Self::check(types, constraint))
                .unwrap_or(false),
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
            Some(TypeKey::Object(shape_id)) => {
                let shape = self.db.object_shape(shape_id);
                shape.properties.is_empty()
            }
            Some(TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = self.db.object_shape(shape_id);
                shape.properties.is_empty()
                    && shape.string_index.is_none()
                    && shape.number_index.is_none()
            }
            Some(TypeKey::ReadonlyType(inner)) => self.check(inner),
            Some(TypeKey::TypeParameter(info) | TypeKey::Infer(info)) => {
                info.constraint.map(|c| self.check(c)).unwrap_or(false)
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
    /// Types currently being visited to prevent infinite recursion.
    pub visiting: FxHashSet<TypeId>,
}

impl<'a> ConstAssertionVisitor<'a> {
    /// Create a new ConstAssertionVisitor.
    pub fn new(db: &'a dyn TypeDatabase) -> Self {
        Self {
            db,
            visiting: FxHashSet::default(),
        }
    }

    /// Apply const assertion to a type, returning the transformed type ID.
    pub fn apply_const_assertion(&mut self, type_id: TypeId) -> TypeId {
        // Prevent infinite recursion
        if !self.visiting.insert(type_id) {
            return type_id;
        }

        let result = match self.db.lookup(type_id) {
            // Literals: preserved as-is
            Some(TypeKey::Literal(_)) => type_id,

            // Arrays: Convert to readonly tuple
            Some(TypeKey::Array(element_type)) => {
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
            Some(TypeKey::Tuple(list_id)) => {
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
            Some(TypeKey::Object(shape_id)) => {
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
            Some(TypeKey::ObjectWithIndex(shape_id)) => {
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
            Some(TypeKey::ReadonlyType(inner)) => {
                let const_inner = self.apply_const_assertion(inner);
                self.db.readonly_type(const_inner)
            }

            // Unions: Recursively apply to all members
            Some(TypeKey::Union(list_id)) => {
                let members = self.db.type_list(list_id);
                let const_members: Vec<TypeId> = members
                    .iter()
                    .map(|&m| self.apply_const_assertion(m))
                    .collect();
                self.db.union(const_members)
            }

            // Intersections: Recursively apply to all members
            Some(TypeKey::Intersection(list_id)) => {
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

        self.visiting.remove(&type_id);
        result
    }
}
