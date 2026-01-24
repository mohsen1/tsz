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
use crate::parser::syntax_kind_ext;
use crate::parser::NodeIndex;
use crate::solver::{ContextualTypeContext, TupleElement, TypeId, TypeKey};

// =============================================================================
// Type Computation Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Core Type Computation
    // =========================================================================

    /// Get the type of a conditional expression (ternary operator).
    ///
    /// Computes the type of `condition ? whenTrue : whenFalse`.
    /// Returns the union of the two branch types if they differ.
    pub(crate) fn get_type_of_conditional_expression(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(cond) = self.ctx.arena.get_conditional_expr(node) else {
            return TypeId::ERROR;
        };

        let when_true = self.get_type_of_node(cond.when_true);
        let when_false = self.get_type_of_node(cond.when_false);

        if when_true == when_false {
            when_true
        } else {
            // Use TypeInterner's union method for automatic normalization
            self.ctx.types.union(vec![when_true, when_false])
        }
    }

    /// Get type of array literal.
    ///
    /// Computes the type of array literals like `[1, 2, 3]` or `["a", "b"]`.
    /// Handles:
    /// - Empty arrays (infer from context or use never[])
    /// - Tuple contexts (e.g., `[string, number]`)
    /// - Spread elements (`[...arr]`)
    /// - Common type inference for mixed elements
    pub(crate) fn get_type_of_array_literal(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(array) = self.ctx.arena.get_literal_expr(node) else {
            return TypeId::ERROR;
        };

        if array.elements.nodes.is_empty() {
            // Empty array literal: infer from context or use never[]
            if let Some(contextual) = self.ctx.contextual_type {
                return contextual;
            }
            return self.ctx.types.array(TypeId::NEVER);
        }

        let tuple_context = match self.ctx.contextual_type {
            Some(ctx_type) => match self.ctx.types.lookup(ctx_type) {
                Some(TypeKey::Tuple(elements)) => {
                    let elements = self.ctx.types.tuple_list(elements);
                    Some(elements.as_ref().to_vec())
                }
                _ => None,
            },
            None => None,
        };

        let ctx_helper = if let Some(ctx_type) = self.ctx.contextual_type {
            Some(ContextualTypeContext::with_expected(
                self.ctx.types,
                ctx_type,
            ))
        } else {
            None
        };

        // Get types of all elements, applying contextual typing when available.
        let mut element_types = Vec::new();
        let mut tuple_elements = Vec::new();
        for (index, &elem_idx) in array.elements.nodes.iter().enumerate() {
            if elem_idx.is_none() {
                continue;
            }

            let prev_context = self.ctx.contextual_type;
            if let Some(ref helper) = ctx_helper {
                if tuple_context.is_some() {
                    self.ctx.contextual_type = helper.get_tuple_element_type(index);
                } else {
                    self.ctx.contextual_type = helper.get_array_element_type();
                }
            }

            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            let elem_is_spread = elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT;
            let elem_type = if elem_is_spread {
                if let Some(spread_data) = self.ctx.arena.get_spread(elem_node) {
                    let spread_expr_type = self.get_type_of_node(spread_data.expression);
                    // Check if spread argument is iterable, emit TS2488 if not
                    self.check_spread_iterability(spread_expr_type, spread_data.expression);
                    // In array context (`[...a]`), a spread contributes its *element* type, not the
                    // array/tuple type itself. Otherwise we'd infer `number[][]` for `[...number[]]`.
                    if tuple_context.is_some() {
                        spread_expr_type
                    } else {
                        self.for_of_element_type(spread_expr_type)
                    }
                } else {
                    TypeId::ANY
                }
            } else {
                self.get_type_of_node(elem_idx)
            };

            self.ctx.contextual_type = prev_context;

            if let Some(ref expected) = tuple_context {
                let (name, optional) = match expected.get(index) {
                    Some(el) => (el.name, el.optional),
                    None => (None, false),
                };
                tuple_elements.push(TupleElement {
                    type_id: elem_type,
                    name,
                    optional,
                    rest: elem_is_spread,
                });
            } else {
                element_types.push(elem_type);
            }
        }

        if tuple_context.is_some() {
            return self.ctx.types.tuple(tuple_elements);
        }

        if let Some(ref helper) = ctx_helper
            && let Some(context_element_type) = helper.get_array_element_type()
            && element_types
                .iter()
                .all(|&elem_type| self.is_assignable_to(elem_type, context_element_type))
        {
            return self.ctx.types.array(context_element_type);
        }

        // Choose a best common type if any element is a supertype of all others.
        let element_type = if element_types.len() == 1 {
            element_types[0]
        } else if element_types.is_empty() {
            TypeId::NEVER
        } else {
            let mut best = None;
            'candidates: for &candidate in &element_types {
                for &elem in &element_types {
                    if !self.is_assignable_to(elem, candidate) {
                        continue 'candidates;
                    }
                }
                best = Some(candidate);
                break;
            }
            best.unwrap_or_else(|| self.ctx.types.union(element_types))
        };

        self.ctx.types.array(element_type)
    }

    /// Get type of prefix unary expression.
    ///
    /// Computes the type of unary expressions like `!x`, `+x`, `-x`, `~x`, `++x`, `--x`, `typeof x`.
    /// Returns boolean for `!`, number for arithmetic operators, string for `typeof`.
    pub(crate) fn get_type_of_prefix_unary(&mut self, idx: NodeIndex) -> TypeId {
        use crate::scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(unary) = self.ctx.arena.get_unary_expr(node) else {
            return TypeId::ERROR;
        };

        match unary.operator {
            // ! returns boolean
            k if k == SyntaxKind::ExclamationToken as u16 => TypeId::BOOLEAN,
            // typeof returns string but still type-check operand for flow/node types.
            k if k == SyntaxKind::TypeOfKeyword as u16 => {
                self.get_type_of_node(unary.operand);
                TypeId::STRING
            }
            // Unary + and - return number unless contextual typing expects a numeric literal.
            k if k == SyntaxKind::PlusToken as u16 || k == SyntaxKind::MinusToken as u16 => {
                if let Some(literal_type) = self.literal_type_from_initializer(idx)
                    && self.contextual_literal_type(literal_type).is_some()
                {
                    return literal_type;
                }
                TypeId::NUMBER
            }
            // ~ returns number
            k if k == SyntaxKind::TildeToken as u16 => TypeId::NUMBER,
            // ++ and -- return number
            k if k == SyntaxKind::PlusPlusToken as u16
                || k == SyntaxKind::MinusMinusToken as u16 =>
            {
                TypeId::NUMBER
            }
            _ => TypeId::ANY,
        }
    }

    /// Get type of template expression (template literal with substitutions).
    ///
    /// Type-checks all expressions within template spans to emit errors like TS2304.
    /// Template expressions always produce string type.
    pub(crate) fn get_type_of_template_expression(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::STRING;
        };

        let Some(template) = self.ctx.arena.get_template_expr(node) else {
            return TypeId::STRING;
        };

        // Type-check each template span's expression
        for &span_idx in &template.template_spans.nodes {
            let Some(span_node) = self.ctx.arena.get(span_idx) else {
                continue;
            };

            let Some(span) = self.ctx.arena.get_template_span(span_node) else {
                continue;
            };

            // Type-check the expression - this will emit TS2304 if name is unresolved
            self.get_type_of_node(span.expression);
        }

        // Template expressions always produce string type
        TypeId::STRING
    }

    /// Get type of variable declaration.
    ///
    /// Computes the type of variable declarations like `let x: number = 5` or `const y = "hello"`.
    /// Returns the type annotation if present, otherwise infers from the initializer.
    pub(crate) fn get_type_of_variable_declaration(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) else {
            return TypeId::ERROR;
        };

        // First check type annotation - this takes precedence
        if !var_decl.type_annotation.is_none() {
            return self.get_type_from_type_node(var_decl.type_annotation);
        }

        if self.is_catch_clause_variable_declaration(idx)
            && self.ctx.use_unknown_in_catch_variables()
        {
            return TypeId::UNKNOWN;
        }

        // Infer from initializer
        if !var_decl.initializer.is_none() {
            return self.get_type_of_node(var_decl.initializer);
        }

        // No initializer - use UNKNOWN to enforce strict checking
        // This requires explicit type annotation or prevents unsafe usage
        TypeId::UNKNOWN
    }

    /// Get the type of an assignment target without definite assignment checks.
    ///
    /// Computes the type of the left-hand side of an assignment expression.
    /// Handles identifier resolution and type-only alias checking.
    pub(crate) fn get_type_of_assignment_target(&mut self, idx: NodeIndex) -> TypeId {
        use crate::scanner::SyntaxKind;

        if let Some(node) = self.ctx.arena.get(idx)
            && node.kind == SyntaxKind::Identifier as u16
            && let Some(sym_id) = self.resolve_identifier_symbol(idx)
        {
            if self.alias_resolves_to_type_only(sym_id) {
                if let Some(ident) = self.ctx.arena.get_identifier(node) {
                    self.error_type_only_value_at(&ident.escaped_text, idx);
                }
                return TypeId::ERROR;
            }
            let declared_type = self.get_type_of_symbol(sym_id);
            return declared_type;
        }

        self.get_type_of_node(idx)
    }

    /// Get the type of a property access when we know the property name.
    ///
    /// This is used for private member access when symbols resolution fails
    /// but the property exists in the object type.
    pub(crate) fn get_type_of_property_access_by_name(
        &mut self,
        idx: NodeIndex,
        access: &crate::parser::node::AccessExprData,
        object_type: TypeId,
        property_name: &str,
    ) -> TypeId {
        use crate::solver::{PropertyAccessResult, QueryDatabase};

        let object_type = self.resolve_type_for_property_access(object_type);
        let result_type = match self
            .ctx
            .types
            .property_access_type(object_type, property_name)
        {
            PropertyAccessResult::Success {
                type_id,
                from_index_signature,
            } => {
                if from_index_signature {
                    self.error_property_not_exist_at(property_name, object_type, idx);
                    return TypeId::ERROR;
                }
                type_id
            }
            PropertyAccessResult::PropertyNotFound { .. } => {
                // Don't emit TS2339 for private fields (starting with #) - they're handled elsewhere
                if !property_name.starts_with('#') {
                    self.error_property_not_exist_at(property_name, object_type, idx);
                }
                TypeId::ERROR
            }
            PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                property_type.unwrap_or(TypeId::UNKNOWN)
            }
            PropertyAccessResult::IsUnknown => {
                // TS2571: Object is of type 'unknown'
                use crate::checker::types::diagnostics::diagnostic_codes;
                self.error_at_node(
                    access.expression,
                    "Object is of type 'unknown'.",
                    diagnostic_codes::OBJECT_IS_OF_TYPE_UNKNOWN,
                );
                TypeId::ERROR
            }
        };

        // Handle nullish coercion
        if access.question_dot_token {
            self.ctx.types.union(vec![result_type, TypeId::UNDEFINED])
        } else {
            result_type
        }
    }

    /// Get the type of a class member.
    ///
    /// Computes the type for class property declarations, method declarations, and getters.
    pub(crate) fn get_type_of_class_member(&mut self, member_idx: NodeIndex) -> TypeId {
        use crate::parser::syntax_kind_ext;

        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return TypeId::ANY;
        };

        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                    return TypeId::ANY;
                };

                // Get the type: either from annotation or inferred from initializer
                if !prop.type_annotation.is_none() {
                    self.get_type_from_type_node(prop.type_annotation)
                } else if !prop.initializer.is_none() {
                    self.get_type_of_node(prop.initializer)
                } else {
                    TypeId::ANY
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                // Get the method type
                self.get_type_of_node(member_idx)
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                    return TypeId::ANY;
                };

                if !accessor.type_annotation.is_none() {
                    self.get_type_from_type_node(accessor.type_annotation)
                } else {
                    self.infer_getter_return_type(accessor.body)
                }
            }
            _ => TypeId::ANY,
        }
    }

    /// Get the simple type of an interface member (without wrapping in object type).
    ///
    /// For property signatures: returns the property type
    /// For method signatures: returns the function type
    pub(crate) fn get_type_of_interface_member_simple(&mut self, member_idx: NodeIndex) -> TypeId {
        use crate::parser::syntax_kind_ext::{METHOD_SIGNATURE, PROPERTY_SIGNATURE};
        use crate::solver::FunctionShape;

        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return TypeId::ANY;
        };

        if member_node.kind == METHOD_SIGNATURE {
            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                return TypeId::ANY;
            };

            let (type_params, type_param_updates) = self.push_type_parameters(&sig.type_parameters);
            let (params, this_type) = self.extract_params_from_signature(sig);
            let (return_type, type_predicate) = self.return_type_and_predicate(sig.type_annotation);

            let shape = FunctionShape {
                type_params,
                params,
                this_type,
                return_type,
                type_predicate,
                is_constructor: false,
                is_method: true,
            };
            self.pop_type_parameters(type_param_updates);
            return self.ctx.types.function(shape);
        }

        if member_node.kind == PROPERTY_SIGNATURE {
            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                return TypeId::ANY;
            };

            if !sig.type_annotation.is_none() {
                return self.get_type_from_type_node(sig.type_annotation);
            }
            return TypeId::ANY;
        }

        TypeId::ANY
    }

    /// Get the type of an interface member.
    ///
    /// Returns an object type containing the member. For method signatures,
    /// creates a callable type. For property signatures, creates a property type.
    pub(crate) fn get_type_of_interface_member(&mut self, member_idx: NodeIndex) -> TypeId {
        use crate::interner::Atom;
        use crate::parser::syntax_kind_ext::{METHOD_SIGNATURE, PROPERTY_SIGNATURE};
        use crate::solver::{FunctionShape, PropertyInfo};

        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return TypeId::ERROR;
        };

        if member_node.kind == METHOD_SIGNATURE || member_node.kind == PROPERTY_SIGNATURE {
            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                return TypeId::ERROR;
            };
            let name = self.get_property_name(sig.name);
            let Some(name) = name else {
                return TypeId::ERROR;
            };
            let name_atom = self.ctx.types.intern_string(&name);

            if member_node.kind == METHOD_SIGNATURE {
                let (type_params, type_param_updates) =
                    self.push_type_parameters(&sig.type_parameters);
                let (params, this_type) = self.extract_params_from_signature(sig);
                let (return_type, type_predicate) =
                    self.return_type_and_predicate(sig.type_annotation);

                let shape = FunctionShape {
                    type_params,
                    params,
                    this_type,
                    return_type,
                    type_predicate,
                    is_constructor: false,
                    is_method: true,
                };
                self.pop_type_parameters(type_param_updates);
                let method_type = self.ctx.types.function(shape);

                let prop = PropertyInfo {
                    name: name_atom,
                    type_id: method_type,
                    write_type: method_type,
                    optional: sig.question_token,
                    readonly: self.has_readonly_modifier(&sig.modifiers),
                    is_method: true,
                };
                return self.ctx.types.object(vec![prop]);
            }

            let type_id = if !sig.type_annotation.is_none() {
                self.get_type_from_type_node(sig.type_annotation)
            } else {
                TypeId::ANY
            };
            let prop = PropertyInfo {
                name: name_atom,
                type_id,
                write_type: type_id,
                optional: sig.question_token,
                readonly: self.has_readonly_modifier(&sig.modifiers),
                is_method: false,
            };
            return self.ctx.types.object(vec![prop]);
        }

        TypeId::ANY
    }

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
