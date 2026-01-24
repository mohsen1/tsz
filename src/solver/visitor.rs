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
//! use crate::solver::visitor::{TypeVisitor, TypeKind, is_type_kind};
//! use crate::solver::types::{IntrinsicKind, LiteralValue};
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

use crate::solver::types::{IntrinsicKind, StringIntrinsicKind, TypeParamInfo};
use crate::solver::{LiteralValue, TypeId, TypeInterner, TypeKey};
use rustc_hash::FxHashSet;

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

    /// Visit a named type reference (interface, class, type alias).
    fn visit_ref(&mut self, _symbol_ref: u32) -> Self::Output {
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

    // =========================================================================
    // Helper Methods
    // =========================================================================

    /// Default output for unimplemented variants.
    fn default_output() -> Self::Output;

    /// Visit a type by dispatching to the appropriate method.
    ///
    /// This is the main entry point for using the visitor.
    fn visit_type(&mut self, types: &TypeInterner, type_id: TypeId) -> Self::Output {
        match types.lookup(type_id) {
            Some(ref type_key) => self.visit_type_key(types, type_key),
            None => Self::default_output(),
        }
    }

    /// Visit a TypeKey by dispatching to the appropriate method.
    fn visit_type_key(&mut self, _types: &TypeInterner, type_key: &TypeKey) -> Self::Output {
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
            TypeKey::Ref(sym_ref) => self.visit_ref(sym_ref.0),
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
            TypeKey::Error => self.visit_error(),
        }
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
    /// Generic types
    Generic,
    /// Type parameters
    TypeParameter,
    /// Conditional types
    Conditional,
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
    fn get_kind(type_key: &TypeKey) -> TypeKind {
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
            TypeKey::TypeParameter(_) | TypeKey::Infer(_) => TypeKind::TypeParameter,
            TypeKey::Conditional(_) => TypeKind::Conditional,
            TypeKey::Ref(_) => TypeKind::Other, // Named type references
            TypeKey::Mapped(_) => TypeKind::Other, // Mapped types
            TypeKey::IndexAccess(_, _) => TypeKind::Other, // Indexed access types
            TypeKey::TemplateLiteral(_) => TypeKind::Other, // Template literal types
            TypeKey::TypeQuery(_) => TypeKind::Other, // Typeof queries
            TypeKey::KeyOf(_) => TypeKind::Other, // Keyof types
            TypeKey::ReadonlyType(_) => TypeKind::Other, // Modifiers don't change kind
            TypeKey::UniqueSymbol(_) => TypeKind::Other,
            TypeKey::ThisType => TypeKind::Other,
            TypeKey::StringIntrinsic { .. } => TypeKind::Other,
            TypeKey::Error => TypeKind::Other,
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

    fn visit_type_key(&mut self, _types: &TypeInterner, type_key: &TypeKey) -> Self::Output {
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

    fn visit_type_key(&mut self, _types: &TypeInterner, type_key: &TypeKey) -> Self::Output {
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
/// use crate::solver::visitor::{is_type_kind, TypeKind};
///
/// let is_object = is_type_kind(&types, type_id, TypeKind::Object);
/// ```
pub fn is_type_kind(types: &TypeInterner, type_id: TypeId, kind: TypeKind) -> bool {
    let mut visitor = TypeKindVisitor::new(kind);
    visitor.visit_type(types, type_id)
}

/// Collect all types referenced by a type.
///
/// # Example
///
/// ```rust
/// use crate::solver::visitor::collect_referenced_types;
///
/// let types = collect_referenced_types(&type_interner, type_id);
/// ```
pub fn collect_referenced_types(types: &TypeInterner, type_id: TypeId) -> FxHashSet<TypeId> {
    let mut visitor = TypeCollectorVisitor::new();
    visitor.visit_type(types, type_id);
    visitor.types
}

/// Test a type against a predicate function.
///
/// # Example
///
/// ```rust
/// use crate::solver::{TypeKey, LiteralValue, visitor::test_type};
///
/// let is_string_literal = test_type(&types, type_id, |key| {
///     matches!(key, TypeKey::Literal(LiteralValue::String(_)))
/// });
/// ```
pub fn test_type<F>(types: &TypeInterner, type_id: TypeId, predicate: F) -> bool
where
    F: Fn(&TypeKey) -> bool,
{
    let mut visitor = TypePredicateVisitor::new(predicate);
    visitor.visit_type(types, type_id)
}
