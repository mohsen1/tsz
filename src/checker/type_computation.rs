//! Type Computation Module
//!
//! This module contains type computation methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Type computation helpers
//! - Type relationship queries
//! - Type format utilities
//!
//! This module extends CheckerState with additional methods for type-related
//! operations, providing cleaner APIs for common patterns.

use crate::checker::state::CheckerState;
use crate::parser::NodeIndex;
use crate::solver::{TypeId, TypeKey};

// =============================================================================
// Type Computation Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Core Type Computation
    // =========================================================================

    /// Get the type of a node with a fallback.
    ///
    /// Returns the computed type, or the fallback if the computed type is ERROR.
    pub fn get_type_of_node_or(&mut self, idx: NodeIndex, fallback: TypeId) -> TypeId {
        let ty = self.get_type_of_node(idx);
        if ty == TypeId::ERROR { fallback } else { ty }
    }

    // =========================================================================
    // Type Relationship Queries
    // =========================================================================

    /// Check if a type is the error type.
    ///
    /// Returns true if the type is TypeId::ERROR.
    pub fn is_error_type(&self, ty: TypeId) -> bool {
        ty == TypeId::ERROR
    }

    /// Check if a type is the any type.
    ///
    /// Returns true if the type is TypeId::ANY.
    pub fn is_any_type(&self, ty: TypeId) -> bool {
        ty == TypeId::ANY
    }

    /// Check if a type is the unknown type.
    ///
    /// Returns true if the type is TypeId::UNKNOWN.
    pub fn is_unknown_type(&self, ty: TypeId) -> bool {
        ty == TypeId::UNKNOWN
    }

    /// Check if a type is the undefined type.
    ///
    /// Returns true if the type is TypeId::UNDEFINED.
    pub fn is_undefined_type(&self, ty: TypeId) -> bool {
        ty == TypeId::UNDEFINED
    }

    /// Check if a type is the void type.
    ///
    /// Returns true if the type is TypeId::VOID.
    pub fn is_void_type(&self, ty: TypeId) -> bool {
        ty == TypeId::VOID
    }

    /// Check if a type is the null type.
    ///
    /// Returns true if the type is TypeId::NULL.
    pub fn is_null_type(&self, ty: TypeId) -> bool {
        ty == TypeId::NULL
    }

    /// Check if a type is a nullable type (null or undefined).
    ///
    /// Returns true if the type is null or undefined.
    pub fn is_nullable_type(&self, ty: TypeId) -> bool {
        ty == TypeId::NULL || ty == TypeId::UNDEFINED
    }

    /// Check if a type is a never type.
    ///
    /// Returns true if the type is TypeId::NEVER.
    pub fn is_never_type(&self, ty: TypeId) -> bool {
        ty == TypeId::NEVER
    }

    // =========================================================================
    // Type Format Utilities
    // =========================================================================

    /// Format a type for display in error messages.
    ///
    /// This is a convenience wrapper that calls the internal format_type method.
    pub fn format_type_for_display(&self, ty: TypeId) -> String {
        self.format_type(ty)
    }

    /// Format a type for display, with optional simplification.
    ///
    /// If `simplify` is true, complex types are simplified for readability.
    pub fn format_type_simplified(&self, ty: TypeId, simplify: bool) -> String {
        // For now, just use the regular formatting
        // A future enhancement could add simplification logic
        if simplify {
            self.format_type(ty)
        } else {
            self.format_type(ty)
        }
    }

    // =========================================================================
    // Type Checking Helpers
    // =========================================================================

    /// Check if a type is assignable to another type.
    ///
    /// This is a convenience wrapper around `is_assignable_to`.
    pub fn check_is_assignable(&mut self, source: TypeId, target: TypeId) -> bool {
        self.is_assignable_to(source, target)
    }

    /// Check if a type is identical to another type.
    ///
    /// This performs a strict equality check on TypeIds.
    pub fn is_same_type(&self, ty1: TypeId, ty2: TypeId) -> bool {
        ty1 == ty2
    }

    /// Check if a type is a function type.
    ///
    /// Returns true if the type represents a callable function.
    pub fn is_function_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(key, Some(crate::solver::TypeKey::Callable(_)))
    }

    /// Check if a type is an object type.
    ///
    /// Returns true if the type represents an object or class.
    pub fn is_object_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(
            key,
            Some(crate::solver::TypeKey::Object(_) | crate::solver::TypeKey::ObjectWithIndex(_))
        )
    }

    /// Check if a type is an array type.
    ///
    /// Returns true if the type represents an array.
    pub fn is_array_type(&self, ty: TypeId) -> bool {
        // Check if it's a reference to the Array interface or an array literal
        // For now, this is a simplified check
        let type_str = self.format_type(ty);
        type_str.contains("[]") || type_str.starts_with("Array<")
    }

    /// Check if a type is a tuple type.
    ///
    /// Returns true if the type represents a tuple.
    pub fn is_tuple_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(key, Some(crate::solver::TypeKey::Tuple(_)))
    }

    /// Check if a type is a union type.
    ///
    /// Returns true if the type is a union of multiple types.
    pub fn is_union_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(key, Some(crate::solver::TypeKey::Union(_)))
    }

    /// Check if a type is an intersection type.
    ///
    /// Returns true if the type is an intersection of multiple types.
    pub fn is_intersection_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(key, Some(crate::solver::TypeKey::Intersection(_)))
    }

    /// Check if a type is a literal type.
    ///
    /// Returns true if the type is a specific literal value (string, number, boolean).
    pub fn is_literal_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(key, Some(crate::solver::TypeKey::Literal(_)))
    }

    /// Check if a type is a generic type application.
    ///
    /// Returns true if the type is a parameterized generic like Map<K, V>.
    pub fn is_generic_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(key, Some(crate::solver::TypeKey::Application(_)))
    }

    /// Check if a type is a reference to another type.
    ///
    /// Returns true if the type is a type reference (interface, class, type alias).
    pub fn is_type_reference(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(key, Some(crate::solver::TypeKey::Ref(_)))
    }

    /// Check if a type is a conditional type.
    ///
    /// Returns true if the type is a conditional type like T extends U ? X : Y.
    pub fn is_conditional_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(key, Some(crate::solver::TypeKey::Conditional(_)))
    }

    /// Check if a type is a mapped type.
    ///
    /// Returns true if the type is a mapped type like { [K in T]: U }.
    pub fn is_mapped_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(key, Some(crate::solver::TypeKey::Mapped(_)))
    }

    /// Check if a type is a template literal type.
    ///
    /// Returns true if the type is a template literal type like `foo${string}bar`.
    pub fn is_template_literal_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(key, Some(crate::solver::TypeKey::TemplateLiteral(_)))
    }

    /// Check if a type is a callable type.
    ///
    /// Returns true if the type represents a function or callable.
    pub fn is_callable_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(
            key,
            Some(
                crate::solver::TypeKey::Function(_)
                    | crate::solver::TypeKey::Callable(_)
                    | crate::solver::TypeKey::ObjectWithIndex(_)
            )
        )
    }

    // =========================================================================
    // Special Type Utilities
    // =========================================================================

    /// Get the element type of an array type.
    ///
    /// Returns the type of elements in the array, or ANY if not an array.
    pub fn get_array_element_type(&self, _array_ty: TypeId) -> TypeId {
        // This is a simplified implementation
        // The full version would extract the element type from array types
        TypeId::ANY
    }

    /// Get the return type of a function type.
    ///
    /// Returns the return type of a function, or ANY if not a function.
    pub fn get_function_return_type(&self, _func_ty: TypeId) -> TypeId {
        // This is a simplified implementation
        // The full version would extract the return type from callable types
        TypeId::ANY
    }

    // =========================================================================
    // Type Construction Utilities
    // =========================================================================

    // =========================================================================
    // Type Manipulation Utilities
    // =========================================================================

    /// Create an array type from an element type.
    ///
    /// Creates a type representing T[] for the given element type T.
    pub fn make_array_type(&self, elem_type: TypeId) -> TypeId {
        self.ctx.types.array(elem_type)
    }

    /// Create a tuple type from element types.
    ///
    /// Creates a type representing a tuple with the given elements.
    pub fn make_tuple_type(&self, elem_types: Vec<TypeId>) -> TypeId {
        use crate::solver::TupleElement;
        let elements: Vec<TupleElement> = elem_types
            .into_iter()
            .map(|type_id| TupleElement {
                type_id,
                name: None,
                optional: false,
                rest: false,
            })
            .collect();
        self.ctx.types.tuple(elements)
    }

    /// Create a function type with parameters and return type.
    ///
    /// Creates a callable type representing a function signature.
    pub fn make_function_type(
        &self,
        params: Vec<crate::solver::ParamInfo>,
        return_type: TypeId,
    ) -> TypeId {
        use crate::solver::FunctionShape;
        let func_shape = FunctionShape {
            type_params: vec![],
            params,
            this_type: None,
            return_type,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        };
        self.ctx.types.function(func_shape)
    }

    /// Get the base type of a generic application.
    ///
    /// For a type like Map<string, number>, returns the Map base type.
    pub fn get_generic_base(&self, ty: TypeId) -> Option<TypeId> {
        match self.ctx.types.lookup(ty) {
            Some(crate::solver::TypeKey::Application(app)) => {
                let app_info = self.ctx.types.type_application(app);
                Some(app_info.base)
            }
            _ => None,
        }
    }

    /// Get the type arguments from a generic application.
    ///
    /// For a type like Map<string, number>, returns [string, number].
    pub fn get_generic_args(&self, ty: TypeId) -> Option<Vec<TypeId>> {
        match self.ctx.types.lookup(ty) {
            Some(crate::solver::TypeKey::Application(app)) => {
                let app_info = self.ctx.types.type_application(app);
                Some(app_info.args.clone())
            }
            _ => None,
        }
    }

    // =========================================================================
    // Type Analysis Utilities
    // =========================================================================

    /// Check if a type contains a type parameter.
    ///
    /// Recursively checks if the type (or any nested type) is a type parameter.
    pub fn contains_type_parameter(&self, ty: TypeId) -> bool {
        let mut visited = std::collections::HashSet::new();
        self.contains_type_parameter_inner(ty, &mut visited)
    }

    fn contains_type_parameter_inner(
        &self,
        ty: TypeId,
        visited: &mut std::collections::HashSet<TypeId>,
    ) -> bool {
        if !visited.insert(ty) {
            return false;
        }

        match self.ctx.types.lookup(ty) {
            Some(TypeKey::TypeParameter(_)) | Some(TypeKey::Infer(_)) => true,
            Some(TypeKey::Array(elem)) => self.contains_type_parameter_inner(elem, visited),
            Some(TypeKey::Tuple(list_id)) => {
                let elems = self.ctx.types.tuple_list(list_id);
                elems
                    .iter()
                    .any(|e| self.contains_type_parameter_inner(e.type_id, visited))
            }
            Some(TypeKey::Union(list_id)) => {
                let members = self.ctx.types.type_list(list_id);
                members
                    .iter()
                    .any(|m| self.contains_type_parameter_inner(*m, visited))
            }
            Some(TypeKey::Intersection(list_id)) => {
                let members = self.ctx.types.type_list(list_id);
                members
                    .iter()
                    .any(|m| self.contains_type_parameter_inner(*m, visited))
            }
            Some(TypeKey::Application(app)) => {
                let app = self.ctx.types.type_application(app);
                if self.contains_type_parameter_inner(app.base, visited) {
                    return true;
                }
                app.args
                    .iter()
                    .any(|a| self.contains_type_parameter_inner(*a, visited))
            }
            _ => false,
        }
    }

    /// Check if a type is concrete (no type parameters).
    ///
    /// A concrete type has no type parameters and can be instantiated directly.
    pub fn is_concrete_type(&self, ty: TypeId) -> bool {
        !self.contains_type_parameter(ty)
    }

    /// Get the depth of type nesting.
    ///
    /// Returns how deeply nested the type structure is (useful for complexity analysis).
    pub fn type_depth(&self, ty: TypeId) -> usize {
        let mut visited = std::collections::HashSet::new();
        self.type_depth_inner(ty, &mut visited)
    }

    fn type_depth_inner(
        &self,
        ty: TypeId,
        visited: &mut std::collections::HashSet<TypeId>,
    ) -> usize {
        if !visited.insert(ty) {
            return 0;
        }

        match self.ctx.types.lookup(ty) {
            Some(TypeKey::Array(elem)) => 1 + self.type_depth_inner(elem, visited),
            Some(TypeKey::Tuple(list_id)) => {
                let elems = self.ctx.types.tuple_list(list_id);
                1 + elems
                    .iter()
                    .map(|e| self.type_depth_inner(e.type_id, visited))
                    .max()
                    .unwrap_or(0)
            }
            Some(TypeKey::Union(list_id) | TypeKey::Intersection(list_id)) => {
                let members = self.ctx.types.type_list(list_id);
                1 + members
                    .iter()
                    .map(|m| self.type_depth_inner(*m, visited))
                    .max()
                    .unwrap_or(0)
            }
            Some(TypeKey::Application(app)) => {
                let app = self.ctx.types.type_application(app);
                1 + std::cmp::max(
                    self.type_depth_inner(app.base, visited),
                    app.args
                        .iter()
                        .map(|a| self.type_depth_inner(*a, visited))
                        .max()
                        .unwrap_or(0),
                )
            }
            _ => 1,
        }
    }
}
