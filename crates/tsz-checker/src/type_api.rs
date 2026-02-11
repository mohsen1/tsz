//! Type API Module
//!
//! This module provides a public API for type queries and manipulations.
//! It contains convenience wrappers around the type interner and type queries
//! for use by external consumers of the checker.
//!
//! This module extends CheckerState with type API methods as part of
//! the Phase 2 architecture refactoring (task 2.3 - file splitting).

use crate::state::CheckerState;
use tsz_solver::TypeId;

// =============================================================================
// Type API Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Primitive Type Checks
    // =========================================================================

    /// Check if a type is the error type.
    pub fn is_error_type(&self, ty: TypeId) -> bool {
        ty == TypeId::ERROR
    }

    /// Check if a type is the any type.
    pub fn is_any_type(&self, ty: TypeId) -> bool {
        ty == TypeId::ANY
    }

    /// Check if a type is the unknown type.
    pub fn is_unknown_type(&self, ty: TypeId) -> bool {
        ty == TypeId::UNKNOWN
    }

    /// Check if a type is the void type.
    pub fn is_void_type(&self, ty: TypeId) -> bool {
        ty == TypeId::VOID
    }

    /// Check if a type is a never type.
    pub fn is_never_type(&self, ty: TypeId) -> bool {
        ty == TypeId::NEVER
    }

    // =========================================================================
    // Type Format Utilities
    // =========================================================================

    /// Format a type for display in error messages.
    pub fn format_type_for_display(&self, ty: TypeId) -> String {
        self.format_type(ty)
    }

    /// Format a type for display, with optional simplification.
    pub fn format_type_simplified(&self, ty: TypeId, simplify: bool) -> String {
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
    pub fn check_is_assignable(&mut self, source: TypeId, target: TypeId) -> bool {
        self.is_assignable_to(source, target)
    }

    /// Check if a type is identical to another type.
    pub fn is_same_type(&self, ty1: TypeId, ty2: TypeId) -> bool {
        ty1 == ty2
    }

    /// Check if a type is a function type.
    pub fn is_function_type(&self, ty: TypeId) -> bool {
        tsz_solver::type_queries::is_function_type(self.ctx.types, ty)
    }

    /// Check if a type is an object type.
    pub fn is_object_type(&self, ty: TypeId) -> bool {
        tsz_solver::type_queries::is_object_type(self.ctx.types, ty)
    }

    /// Check if a type is an array type.
    pub fn is_array_type(&self, ty: TypeId) -> bool {
        let type_str = self.format_type(ty);
        type_str.contains("[]") || type_str.starts_with("Array<")
    }

    /// Check if a type is a tuple type.
    pub fn is_tuple_type(&self, ty: TypeId) -> bool {
        tsz_solver::type_queries::is_tuple_type(self.ctx.types, ty)
    }

    /// Check if a type is a union type.
    pub fn is_union_type(&self, ty: TypeId) -> bool {
        tsz_solver::type_queries::is_union_type(self.ctx.types, ty)
    }

    /// Check if a type is an intersection type.
    pub fn is_intersection_type(&self, ty: TypeId) -> bool {
        tsz_solver::type_queries::is_intersection_type(self.ctx.types, ty)
    }

    /// Check if a type is a literal type.
    pub fn is_literal_type(&self, ty: TypeId) -> bool {
        tsz_solver::type_queries::is_literal_type(self.ctx.types, ty)
    }

    /// Check if a type is a generic type application.
    pub fn is_generic_type(&self, ty: TypeId) -> bool {
        tsz_solver::type_queries::is_generic_type(self.ctx.types, ty)
    }

    /// Check if a type is a reference to another type.
    pub fn is_type_reference(&self, ty: TypeId) -> bool {
        tsz_solver::type_queries::is_type_reference(self.ctx.types, ty)
    }

    /// Check if a type is a conditional type.
    pub fn is_conditional_type(&self, ty: TypeId) -> bool {
        tsz_solver::type_queries::is_conditional_type(self.ctx.types, ty)
    }

    /// Check if a type is a mapped type.
    pub fn is_mapped_type(&self, ty: TypeId) -> bool {
        tsz_solver::type_queries::is_mapped_type(self.ctx.types, ty)
    }

    /// Check if a type is a template literal type.
    pub fn is_template_literal_type(&self, ty: TypeId) -> bool {
        tsz_solver::type_queries::is_template_literal_type(self.ctx.types, ty)
    }

    /// Check if a type is a callable type.
    pub fn is_callable_type(&self, ty: TypeId) -> bool {
        tsz_solver::type_queries::is_callable_type(self.ctx.types, ty)
    }

    // =========================================================================
    // Special Type Utilities
    // =========================================================================

    /// Get the element type of an array type.
    pub fn get_array_element_type(&self, _array_ty: TypeId) -> TypeId {
        TypeId::ANY
    }

    /// Get the return type of a function type.
    pub fn get_function_return_type(&self, _func_ty: TypeId) -> TypeId {
        TypeId::ANY
    }

    // =========================================================================
    // Type Construction Utilities
    // =========================================================================

    /// Create an array type from an element type.
    pub fn make_array_type(&self, elem_type: TypeId) -> TypeId {
        self.ctx.types.array(elem_type)
    }

    /// Create a tuple type from element types.
    pub fn make_tuple_type(&self, elem_types: Vec<TypeId>) -> TypeId {
        use tsz_solver::TupleElement;
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
    pub fn make_function_type(
        &self,
        params: Vec<tsz_solver::ParamInfo>,
        return_type: TypeId,
    ) -> TypeId {
        use tsz_solver::FunctionShape;
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
    pub fn get_generic_base(&self, ty: TypeId) -> Option<TypeId> {
        tsz_solver::type_queries::get_application_info(self.ctx.types, ty).map(|(base, _)| base)
    }

    /// Get the type arguments from a generic application.
    pub fn get_generic_args(&self, ty: TypeId) -> Option<Vec<TypeId>> {
        tsz_solver::type_queries::get_application_info(self.ctx.types, ty).map(|(_, args)| args)
    }

    // =========================================================================
    // Type Analysis Utilities
    // =========================================================================

    /// Check if a type contains a type parameter.
    pub fn contains_type_parameter(&self, ty: TypeId) -> bool {
        let mut visited = rustc_hash::FxHashSet::default();
        self.contains_type_parameter_inner(ty, &mut visited)
    }

    fn contains_type_parameter_inner(
        &self,
        ty: TypeId,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) -> bool {
        use tsz_solver::type_queries::{
            TypeParameterContentKind, classify_for_type_parameter_content,
        };

        if !visited.insert(ty) {
            return false;
        }

        match classify_for_type_parameter_content(self.ctx.types, ty) {
            TypeParameterContentKind::IsTypeParameter => true,
            TypeParameterContentKind::Array(elem) => {
                self.contains_type_parameter_inner(elem, visited)
            }
            TypeParameterContentKind::Tuple(list_id) => {
                let elems = self.ctx.types.tuple_list(list_id);
                elems
                    .iter()
                    .any(|e| self.contains_type_parameter_inner(e.type_id, visited))
            }
            TypeParameterContentKind::Union(members)
            | TypeParameterContentKind::Intersection(members) => members
                .iter()
                .any(|m| self.contains_type_parameter_inner(*m, visited)),
            TypeParameterContentKind::Application { base, args } => {
                if self.contains_type_parameter_inner(base, visited) {
                    return true;
                }
                args.iter()
                    .any(|a| self.contains_type_parameter_inner(*a, visited))
            }
            TypeParameterContentKind::NotTypeParameter => false,
        }
    }

    /// Check if a type is concrete (no type parameters).
    pub fn is_concrete_type(&self, ty: TypeId) -> bool {
        !self.contains_type_parameter(ty)
    }

    /// Get the depth of type nesting.
    pub fn type_depth(&self, ty: TypeId) -> usize {
        let mut visited = rustc_hash::FxHashSet::default();
        self.type_depth_inner(ty, &mut visited)
    }

    fn type_depth_inner(&self, ty: TypeId, visited: &mut rustc_hash::FxHashSet<TypeId>) -> usize {
        use tsz_solver::type_queries::{TypeDepthKind, classify_for_type_depth};

        if !visited.insert(ty) {
            return 0;
        }

        match classify_for_type_depth(self.ctx.types, ty) {
            TypeDepthKind::Array(elem) => 1 + self.type_depth_inner(elem, visited),
            TypeDepthKind::Tuple(list_id) => {
                let elems = self.ctx.types.tuple_list(list_id);
                1 + elems
                    .iter()
                    .map(|e| self.type_depth_inner(e.type_id, visited))
                    .max()
                    .unwrap_or(0)
            }
            TypeDepthKind::Members(members) => {
                1 + members
                    .iter()
                    .map(|m| self.type_depth_inner(*m, visited))
                    .max()
                    .unwrap_or(0)
            }
            TypeDepthKind::Application { base, args } => {
                1 + std::cmp::max(
                    self.type_depth_inner(base, visited),
                    args.iter()
                        .map(|a| self.type_depth_inner(*a, visited))
                        .max()
                        .unwrap_or(0),
                )
            }
            TypeDepthKind::Terminal => 1,
        }
    }
}
