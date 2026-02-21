//! Contextual literal types, circular reference detection, and private property access.

use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;
use tsz_solver::type_queries_extended::{
    ContextualLiteralAllowKind, classify_for_contextual_literal,
};

impl<'a> CheckerState<'a> {
    pub(crate) fn contextual_literal_type(&mut self, literal_type: TypeId) -> Option<TypeId> {
        let ctx_type = self.ctx.contextual_type?;
        self.contextual_type_allows_literal(ctx_type, literal_type)
            .then_some(literal_type)
    }

    pub(crate) fn contextual_type_allows_literal(
        &mut self,
        ctx_type: TypeId,
        literal_type: TypeId,
    ) -> bool {
        let mut visited = FxHashSet::default();
        self.contextual_type_allows_literal_inner(ctx_type, literal_type, &mut visited)
    }

    pub(crate) fn contextual_type_allows_literal_inner(
        &mut self,
        ctx_type: TypeId,
        literal_type: TypeId,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        if ctx_type == literal_type {
            return true;
        }
        if !visited.insert(ctx_type) {
            return false;
        }

        // Resolve Lazy(DefId) types before classification. Type aliases like
        // `type Direction = "north" | "south"` are Lazy until resolved.
        if let Some(def_id) = tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, ctx_type) {
            // Try type_env first
            let resolved = {
                let env = self.ctx.type_env.borrow();
                env.get_def(def_id)
            };
            if let Some(resolved) = resolved
                && resolved != ctx_type
            {
                return self.contextual_type_allows_literal_inner(resolved, literal_type, visited);
            }
            // If not resolved, use centralized relation precondition setup to populate type_env.
            self.ensure_relation_input_ready(ctx_type);
            let resolved = {
                let env = self.ctx.type_env.borrow();
                env.get_def(def_id)
            };
            if let Some(resolved) = resolved
                && resolved != ctx_type
            {
                return self.contextual_type_allows_literal_inner(resolved, literal_type, visited);
            }
            return false;
        }

        // Evaluate KeyOf and IndexAccess types to their concrete form before
        // classification. E.g., keyof Person → "name" | "age".
        if tsz_solver::type_queries::is_keyof_type(self.ctx.types, ctx_type)
            || tsz_solver::type_queries::is_index_access_type(self.ctx.types, ctx_type)
        {
            let evaluated = self.evaluate_type_with_env(ctx_type);
            if evaluated != ctx_type && evaluated != TypeId::ERROR {
                return self.contextual_type_allows_literal_inner(evaluated, literal_type, visited);
            }
        }

        match classify_for_contextual_literal(self.ctx.types, ctx_type) {
            ContextualLiteralAllowKind::Members(members) => members.iter().any(|&member| {
                self.contextual_type_allows_literal_inner(member, literal_type, visited)
            }),
            // Type parameters always allow literal types. In TypeScript, when the
            // expected type is a type parameter (e.g., K extends keyof T), the literal
            // is preserved and the constraint is checked later during generic inference.
            ContextualLiteralAllowKind::TypeParameter { .. }
            | ContextualLiteralAllowKind::TemplateLiteral => true,
            ContextualLiteralAllowKind::Application => {
                let expanded = self.evaluate_application_type(ctx_type);
                if expanded != ctx_type {
                    return self.contextual_type_allows_literal_inner(
                        expanded,
                        literal_type,
                        visited,
                    );
                }
                false
            }
            ContextualLiteralAllowKind::Mapped => {
                let expanded = self.evaluate_mapped_type_with_resolution(ctx_type);
                if expanded != ctx_type {
                    return self.contextual_type_allows_literal_inner(
                        expanded,
                        literal_type,
                        visited,
                    );
                }
                false
            }
            ContextualLiteralAllowKind::NotAllowed => false,
        }
    }

    /// Check if a type node is a simple type reference without structural wrapping.
    ///
    /// Returns true for bare type references like `type A = B`, false for wrapped
    /// references like `type A = { x: B }` or `type A = B | null`.
    pub(crate) fn is_simple_type_reference(&self, type_node: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(type_node) else {
            return false;
        };

        // Type reference or identifier without structural wrapping
        matches!(
            node.kind,
            k if k == syntax_kind_ext::TYPE_REFERENCE || k == SyntaxKind::Identifier as u16
        )
    }

    /// Check if a type alias directly circularly references itself.
    ///
    /// Returns true when a type alias resolves to itself without structural wrapping,
    /// which is invalid: `type A = B; type B = A;`
    ///
    /// Returns false for valid recursive types that use structural wrapping:
    /// `type List = { value: number; next: List | null };`
    pub(crate) fn is_direct_circular_reference(
        &self,
        sym_id: SymbolId,
        resolved_type: TypeId,
        type_node: NodeIndex,
    ) -> bool {
        // Check if resolved_type is Lazy(DefId) pointing back to sym_id
        if let Some(def_id) =
            tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, resolved_type)
        {
            // Map DefId back to SymbolId
            if let Some(&target_sym_id) = self.ctx.def_to_symbol.borrow().get(&def_id)
                && target_sym_id == sym_id
            {
                // It's a self-reference - check if it's direct (no structural wrapping)
                return self.is_simple_type_reference(type_node);
            }
        }

        // Also check union/intersection members for circular references.
        if let Some(members) =
            tsz_solver::type_queries::get_union_members(self.ctx.types, resolved_type)
        {
            for &member in &members {
                if self.is_direct_circular_reference(sym_id, member, type_node) {
                    return true;
                }
            }
        }
        if let Some(members) =
            tsz_solver::type_queries::get_intersection_members(self.ctx.types, resolved_type)
        {
            for &member in &members {
                if self.is_direct_circular_reference(sym_id, member, type_node) {
                    return true;
                }
            }
        }

        false
    }

    pub(crate) fn report_private_identifier_outside_class(
        &mut self,
        name_idx: NodeIndex,
        property_name: &str,
        object_type: TypeId,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        let class_name = self
            .get_class_name_from_type(object_type)
            .unwrap_or_else(|| "the class".to_string());
        let message = format_message(
            diagnostic_messages::PROPERTY_IS_NOT_ACCESSIBLE_OUTSIDE_CLASS_BECAUSE_IT_HAS_A_PRIVATE_IDENTIFIER,
            &[property_name, &class_name],
        );
        self.error_at_node(
            name_idx,
            &message,
            diagnostic_codes::PROPERTY_IS_NOT_ACCESSIBLE_OUTSIDE_CLASS_BECAUSE_IT_HAS_A_PRIVATE_IDENTIFIER,
        );
    }

    pub(crate) fn report_private_identifier_shadowed(
        &mut self,
        name_idx: NodeIndex,
        property_name: &str,
        object_type: TypeId,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        let type_string = self
            .get_class_name_from_type(object_type)
            .unwrap_or_else(|| "the type".to_string());
        let message = format_message(
            diagnostic_messages::THE_PROPERTY_CANNOT_BE_ACCESSED_ON_TYPE_WITHIN_THIS_CLASS_BECAUSE_IT_IS_SHADOWED,
            &[property_name, &type_string],
        );
        self.error_at_node(
            name_idx,
            &message,
            diagnostic_codes::THE_PROPERTY_CANNOT_BE_ACCESSED_ON_TYPE_WITHIN_THIS_CLASS_BECAUSE_IT_IS_SHADOWED,
        );
    }

    // Resolve a typeof type reference to its structural type.
    //
    // This function resolves `typeof X` type queries to the actual type of `X`.
    // This is useful for type operations where we need the structural type rather
    // than the type query itself.
    // **TypeQuery Resolution:**
    // - **TypeQuery**: `typeof X` → get the type of symbol X
    // - **Other types**: Return unchanged (not a typeof query)
    //
    // **Use Cases:**
    // - Assignability checking (need actual type, not typeof reference)
    // - Type comparison (typeof X should be compared to X's type)
    // - Generic constraint evaluation
    pub(crate) fn get_type_of_private_property_access(
        &mut self,
        idx: NodeIndex,
        access: &tsz_parser::parser::node::AccessExprData,
        name_idx: NodeIndex,
        object_type: TypeId,
    ) -> TypeId {
        let factory = self.ctx.types.factory();
        use tsz_solver::operations_property::PropertyAccessResult;

        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return TypeId::ERROR; // Missing identifier data - propagate error
        };
        let property_name = ident.escaped_text.clone();

        let (symbols, saw_class_scope) = self.resolve_private_identifier_symbols(name_idx);

        // NOTE: Do NOT emit TS18016 here for property access expressions.
        // `obj.#prop` is always valid syntax — the private identifier in a property
        // access position is grammatically correct. TSC only emits TS18016 for truly
        // invalid positions (object literals, standalone expressions). For property
        // access, the error is always semantic (TS18013: can't access private member),
        // which is handled below based on the object's type.

        // Evaluate for type checking but preserve original for error messages
        // This preserves nominal identity (e.g., D<string>) in error messages
        let original_object_type = object_type;
        let object_type = self.evaluate_application_type(object_type);

        // Property access on `never` returns `never` (bottom type propagation).
        // TSC does not emit TS18050 for property access on `never` — the result is
        // simply `never`, which allows exhaustive narrowing patterns to work correctly.
        if object_type == TypeId::NEVER {
            return TypeId::NEVER;
        }

        let (object_type_for_check, nullish_cause) = self.split_nullish_type(object_type);
        let Some(object_type_for_check) = object_type_for_check else {
            if access.question_dot_token {
                return TypeId::UNDEFINED;
            }
            if let Some(cause) = nullish_cause {
                // Type is entirely nullish - emit TS18050 "The value X cannot be used here"
                self.report_nullish_object(access.expression, cause, true);
            }
            return TypeId::ERROR;
        };

        // If `symbols.is_empty()`, the private identifier was not declared in any enclosing lexical class scope.
        // Therefore, this access is invalid, regardless of whether the object type actually has the property.
        if symbols.is_empty() {
            let resolved_type = self.resolve_type_for_property_access(object_type_for_check);
            let mut found = false;

            use tsz_solver::operations_property::PropertyAccessResult;
            match self
                .ctx
                .types
                .property_access_type(resolved_type, &property_name)
            {
                PropertyAccessResult::Success { .. } => {
                    found = true;
                }
                _ => {
                    if let Some(shape) =
                        crate::query_boundaries::state_type_analysis::callable_shape_for_type(
                            self.ctx.types,
                            resolved_type,
                        )
                    {
                        let prop_atom = self.ctx.types.intern_string(&property_name);
                        for prop in &shape.properties {
                            if prop.name == prop_atom {
                                found = true;
                                break;
                            }
                        }
                    }
                }
            }

            if found {
                // Property exists, but we are not in the declaring scope (TS18013)
                self.report_private_identifier_outside_class(
                    name_idx,
                    &property_name,
                    original_object_type,
                );
            } else if !saw_class_scope
                && (resolved_type == TypeId::ANY || resolved_type == TypeId::UNKNOWN)
            {
                // TS18016: Private identifiers are not allowed outside class bodies.
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node(
                    name_idx,
                    "Private identifiers are not allowed outside class bodies.",
                    diagnostic_codes::PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_OUTSIDE_CLASS_BODIES,
                );
            } else {
                // TS2339: Property does not exist
                self.error_property_not_exist_at(&property_name, original_object_type, name_idx);
            }
            return TypeId::ERROR;
        }

        let declaring_type = match self.private_member_declaring_type(symbols[0]) {
            Some(ty) => ty,
            None => {
                if saw_class_scope {
                    // Use original_object_type to preserve nominal identity (e.g., D<string>)
                    self.error_property_not_exist_at(
                        &property_name,
                        original_object_type,
                        name_idx,
                    );
                } else {
                    self.report_private_identifier_outside_class(
                        name_idx,
                        &property_name,
                        original_object_type,
                    );
                }
                return TypeId::ERROR;
            }
        };

        if object_type_for_check == TypeId::ANY {
            return TypeId::ANY;
        }
        if object_type_for_check == TypeId::ERROR {
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        }
        if object_type_for_check == TypeId::UNKNOWN {
            return TypeId::ANY; // UNKNOWN remains ANY for now (could be stricter)
        }

        // For private member access, use nominal typing based on private brand.
        // If both types have the same private brand, they're from the same class
        // declaration and the access should be allowed.
        let types_compatible =
            if self.types_have_same_private_brand(object_type_for_check, declaring_type) {
                true
            } else {
                self.is_assignable_to(object_type_for_check, declaring_type)
            };

        if !types_compatible {
            let shadowed = symbols.iter().skip(1).any(|sym_id| {
                self.private_member_declaring_type(*sym_id)
                    .is_some_and(|ty| {
                        if self.types_have_same_private_brand(object_type_for_check, ty) {
                            true
                        } else {
                            self.is_assignable_to(object_type_for_check, ty)
                        }
                    })
            });
            if shadowed {
                self.report_private_identifier_shadowed(
                    name_idx,
                    &property_name,
                    original_object_type,
                );
                return TypeId::ERROR;
            }

            // Use original_object_type to preserve nominal identity (e.g., D<string>)
            self.error_property_not_exist_at(&property_name, original_object_type, name_idx);
            return TypeId::ERROR;
        }

        let declaring_type = self.resolve_type_for_property_access(declaring_type);
        let mut result_type = match self
            .ctx
            .types
            .property_access_type(declaring_type, &property_name)
        {
            PropertyAccessResult::Success {
                type_id,
                from_index_signature,
                ..
            } => {
                if from_index_signature {
                    // Private fields can't come from index signatures
                    // Use original_object_type to preserve nominal identity (e.g., D<string>)
                    self.error_property_not_exist_at(
                        &property_name,
                        original_object_type,
                        name_idx,
                    );
                    return TypeId::ERROR;
                }
                type_id
            }
            PropertyAccessResult::PropertyNotFound { .. } => {
                // If we got here, we already resolved the symbol, so the private field exists.
                // The solver might not find it due to type encoding issues.
                // FALLBACK: Try to manually find the property in the callable type
                if let Some(shape) =
                    crate::query_boundaries::state_type_analysis::callable_shape_for_type(
                        self.ctx.types,
                        declaring_type,
                    )
                {
                    let prop_atom = self.ctx.types.intern_string(&property_name);
                    for prop in &shape.properties {
                        if prop.name == prop_atom {
                            // Property found! Return its type
                            return if prop.optional {
                                factory.union(vec![prop.type_id, TypeId::UNDEFINED])
                            } else {
                                prop.type_id
                            };
                        }
                    }
                }
                // Property not found even in fallback, return ANY for type recovery
                TypeId::ANY
            }
            PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                property_type.unwrap_or(TypeId::UNKNOWN)
            }
            PropertyAccessResult::IsUnknown => {
                // TS2339: Property does not exist on type 'unknown'
                // Use the same error as TypeScript for property access on unknown
                // Use original_object_type to preserve nominal identity (e.g., D<string>)
                self.error_property_not_exist_at(&property_name, original_object_type, name_idx);
                TypeId::ERROR
            }
        };

        if let Some(cause) = nullish_cause {
            if access.question_dot_token {
                result_type = factory.union(vec![result_type, TypeId::UNDEFINED]);
            } else {
                self.report_possibly_nullish_object(access.expression, cause);
            }
        }

        self.apply_flow_narrowing(idx, result_type)
    }
}
