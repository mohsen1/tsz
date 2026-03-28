//! Readonly property assignment checking (TS2540, TS2542).
//!
//! Extracted from the `property` module to keep files focused and under
//! the 2000-line checker file limit.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn check_readonly_assignment_pattern(&mut self, pattern_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let pattern_idx = self.ctx.arena.skip_parenthesized(pattern_idx);
        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return false;
        };

        match pattern_node.kind {
            syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                let Some(obj) = self.ctx.arena.get_literal_expr(pattern_node) else {
                    return false;
                };

                let mut emitted = false;
                for &elem_idx in &obj.elements.nodes {
                    let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                        continue;
                    };

                    if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                        emitted |= self.check_readonly_assignment_pattern_target(prop.initializer);
                    } else if elem_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                        if let Some(shorthand) = self.ctx.arena.get_shorthand_property(elem_node) {
                            emitted |=
                                self.check_readonly_assignment_pattern_target(shorthand.name);
                        }
                    } else if elem_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT
                        && let Some(spread) = self.ctx.arena.get_spread(elem_node)
                    {
                        emitted |= self.check_readonly_assignment_pattern_target(spread.expression);
                    }
                }

                emitted
            }
            syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                let Some(array_lit) = self.ctx.arena.get_literal_expr(pattern_node) else {
                    return false;
                };

                let mut emitted = false;
                for &elem_idx in &array_lit.elements.nodes {
                    let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                        continue;
                    };
                    if elem_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                        continue;
                    }

                    let target_idx = if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                        self.ctx
                            .arena
                            .get_spread(elem_node)
                            .map(|spread| spread.expression)
                    } else {
                        Some(elem_idx)
                    };

                    if let Some(target_idx) = target_idx {
                        emitted |= self.check_readonly_assignment_pattern_target(target_idx);
                    }
                }

                emitted
            }
            _ => self.check_readonly_assignment(pattern_idx, NodeIndex::NONE),
        }
    }

    fn check_readonly_assignment_pattern_target(&mut self, target_idx: NodeIndex) -> bool {
        use tsz_scanner::SyntaxKind;

        let target_idx = self.ctx.arena.skip_parenthesized(target_idx);
        let Some(target_node) = self.ctx.arena.get(target_idx) else {
            return false;
        };

        if let Some(bin) = self.ctx.arena.get_binary_expr(target_node)
            && bin.operator_token == SyntaxKind::EqualsToken as u16
        {
            return self.check_readonly_assignment_pattern_target(bin.left);
        }

        self.check_readonly_assignment(target_idx, NodeIndex::NONE)
    }

    /// Check if a delete target is a readonly property.
    /// Reports TS2704 for readonly named properties and TS2542 for readonly index signatures.
    /// Returns `true` if a readonly delete diagnostic was emitted.
    pub(crate) fn check_readonly_delete_operand(&mut self, target_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let target_idx = self.ctx.arena.skip_parenthesized(target_idx);
        let Some(target_node) = self.ctx.arena.get(target_idx) else {
            return false;
        };

        match target_node.kind {
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {}
            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.ctx.arena.get_access_expr(target_node) {
                    let object_type = self.get_type_of_node(access.expression);
                    if object_type == TypeId::ANY
                        || object_type == TypeId::UNKNOWN
                        || object_type == TypeId::ERROR
                    {
                        return false;
                    }

                    let index_type = self.get_type_of_node(access.name_or_argument);
                    let enum_member_name = self
                        .get_literal_string_from_node(access.name_or_argument)
                        .or_else(|| {
                            self.get_literal_index_from_node(access.name_or_argument)
                                .map(|idx| idx.to_string())
                        });
                    if let Some(name) = enum_member_name
                        && self.is_enum_member_property(access.expression, &name)
                    {
                        self.error_delete_readonly_property_at(target_idx);
                        return true;
                    }
                    if let Some(name) = self.get_readonly_element_access_name(
                        object_type,
                        access.name_or_argument,
                        index_type,
                    ) {
                        use crate::query_boundaries::common::PropertyAccessResult;
                        use tsz_solver::operations::property::is_readonly_tuple_fixed_element;
                        let from_idx_sig = if name == "index signature" {
                            true
                        } else if is_readonly_tuple_fixed_element(
                            self.ctx.types,
                            object_type,
                            &name,
                        ) {
                            false
                        } else {
                            matches!(
                                self.resolve_property_access_with_env(object_type, &name),
                                PropertyAccessResult::Success {
                                    from_index_signature: true,
                                    ..
                                }
                            )
                        };
                        if from_idx_sig || self.is_readonly_mapped_type(object_type) {
                            self.error_readonly_index_signature_at(object_type, target_idx);
                        } else {
                            self.error_delete_readonly_property_at(target_idx);
                        }
                        return true;
                    }

                    if self.is_readonly_mapped_type(object_type) {
                        self.error_readonly_index_signature_at(object_type, target_idx);
                        return true;
                    }

                    if let Some(name) = self.get_literal_string_from_node(access.name_or_argument) {
                        if self.is_function_or_class_name_property(access.expression, &name) {
                            self.error_delete_readonly_property_at(target_idx);
                            return true;
                        }
                        if let Some(type_name) =
                            self.get_declared_type_name_from_expression(access.expression)
                            && self.is_interface_property_readonly(&type_name, &name)
                        {
                            self.error_delete_readonly_property_at(target_idx);
                            return true;
                        }
                        if self.is_namespace_const_property(access.expression, &name) {
                            self.error_delete_readonly_property_at(target_idx);
                            return true;
                        }
                    }
                }
                return false;
            }
            _ => return false,
        }

        let Some(access) = self.ctx.arena.get_access_expr(target_node) else {
            return false;
        };

        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            return false;
        };

        if self.is_private_identifier_name(access.name_or_argument) {
            return false;
        }

        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };

        let prop_name = ident.escaped_text.clone();

        if self.is_function_or_class_name_property(access.expression, &prop_name) {
            self.error_delete_readonly_property_at(target_idx);
            return true;
        }

        if prop_name == "globalThis" && self.is_global_this_expression(access.expression) {
            self.error_delete_readonly_property_at(target_idx);
            return true;
        }

        if self.is_enum_member_property(access.expression, &prop_name) {
            self.error_delete_readonly_property_at(target_idx);
            return true;
        }

        let obj_type = self.get_type_of_node(access.expression);
        let readonly_check_type = self.evaluate_type_for_assignability(obj_type);

        if self.is_namespace_const_property(access.expression, &prop_name) {
            self.error_delete_readonly_property_at(target_idx);
            return true;
        }

        use crate::query_boundaries::common::PropertyAccessResult;
        let property_result =
            self.resolve_property_access_with_env(readonly_check_type, &prop_name);
        let (property_exists, prop_from_index_sig) = match &property_result {
            PropertyAccessResult::Success {
                from_index_signature,
                ..
            } => (true, *from_index_signature),
            _ => (false, false),
        };

        if !property_exists {
            return false;
        }

        if self.is_namespace_import_binding(access.expression) {
            self.error_delete_readonly_property_at(target_idx);
            return true;
        }

        if self.is_property_readonly(readonly_check_type, &prop_name) {
            if prop_from_index_sig {
                self.error_readonly_index_signature_at(readonly_check_type, target_idx);
            } else {
                self.error_delete_readonly_property_at(target_idx);
            }
            return true;
        }

        if let Some(class_name) = self.get_class_name_from_expression(access.expression)
            && self.is_class_property_readonly(&class_name, &prop_name)
        {
            self.error_delete_readonly_property_at(target_idx);
            return true;
        }

        if let Some(type_name) = self.get_declared_type_name_from_expression(access.expression)
            && self.is_interface_property_readonly(&type_name, &prop_name)
        {
            self.error_delete_readonly_property_at(target_idx);
            return true;
        }

        false
    }

    /// Check if an assignment target is a readonly property.
    /// Reports error TS2540 if trying to assign to a readonly property.
    /// Returns `true` if a readonly error was emitted (caller should skip further type checks).
    #[tracing::instrument(skip(self), fields(target_idx = target_idx.0))]
    pub(crate) fn check_readonly_assignment(
        &mut self,
        target_idx: NodeIndex,
        _expr_idx: NodeIndex,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        // Skip parenthesized expressions to find the underlying property access.
        // E.g., `++((M.x))` should detect that `M.x` is readonly.
        let target_idx = self.ctx.arena.skip_parenthesized(target_idx);
        let Some(target_node) = self.ctx.arena.get(target_idx) else {
            return false;
        };

        match target_node.kind {
            syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            | syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                return self.check_readonly_assignment_pattern(target_idx);
            }
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {}
            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.ctx.arena.get_access_expr(target_node) {
                    let object_type = self.get_type_of_node(access.expression);
                    if object_type == TypeId::ANY
                        || object_type == TypeId::UNKNOWN
                        || object_type == TypeId::ERROR
                    {
                        return false;
                    }

                    let index_type = self.get_type_of_node(access.name_or_argument);
                    if let Some(name) = self.get_readonly_element_access_name(
                        object_type,
                        access.name_or_argument,
                        index_type,
                    ) {
                        // TS2542: use specific diagnostic for readonly index signatures.
                        // Check if the property resolved through an index signature
                        // (either the explicit "index signature" sentinel or via
                        // from_index_signature on a named property).
                        //
                        // Exception: readonly tuple fixed elements (e.g., v[0] on
                        // `readonly [number, number, ...number[]]`) are named properties
                        // even though resolve_array_property reports from_index_signature.
                        use crate::query_boundaries::common::PropertyAccessResult;
                        use tsz_solver::operations::property::is_readonly_tuple_fixed_element;
                        let from_idx_sig = if name == "index signature" {
                            true
                        } else if is_readonly_tuple_fixed_element(
                            self.ctx.types,
                            object_type,
                            &name,
                        ) {
                            false
                        } else {
                            matches!(
                                self.resolve_property_access_with_env(object_type, &name),
                                PropertyAccessResult::Success {
                                    from_index_signature: true,
                                    ..
                                }
                            )
                        };
                        if from_idx_sig {
                            // tsc anchors TS2542 at the full element access expression
                            self.error_readonly_index_signature_at(object_type, target_idx);
                        } else {
                            // tsc anchors TS2540 at the argument expression inside
                            // the brackets (e.g., the `0` in `v[0]`), not the full
                            // element access expression.
                            self.error_readonly_property_at(&name, access.name_or_argument);
                        }
                        return true;
                    }
                    // Check for mapped types with explicit readonly modifier (e.g., Readonly<T>).
                    // This handles Application types like Readonly<T> where T is generic,
                    // which require TypeEnvironment evaluation to resolve the base type alias.
                    if self.is_readonly_mapped_type(object_type) {
                        self.error_readonly_index_signature_at(object_type, target_idx);
                        return true;
                    }
                    // Check AST-level interface readonly for element access (obj["x"])
                    if let Some(name) = self.get_literal_string_from_node(access.name_or_argument) {
                        if let Some(type_name) =
                            self.get_declared_type_name_from_expression(access.expression)
                            && self.is_interface_property_readonly(&type_name, &name)
                        {
                            self.error_readonly_property_at(&name, access.name_or_argument);
                            return true;
                        }
                        // Also check namespace const exports via element access (M["x"])
                        if self.is_namespace_const_property(access.expression, &name) {
                            self.error_readonly_property_at(&name, access.name_or_argument);
                            return true;
                        }
                    }

                    // TS2862: Generic type parameters can only be indexed for reading.
                    // When the object type is a type parameter (e.g., T extends Record<string, any>),
                    // writing through an index signature is unsafe because T could have more specific
                    // property types than the constraint. Only emit when the index type is broad
                    // (not a specific literal that would resolve to a named property).
                    if self.is_generic_indexed_write(object_type, index_type) {
                        self.error_generic_only_indexed_for_reading(object_type, target_idx);
                        return true;
                    }
                }
                return false;
            }
            _ => return false,
        }

        let Some(access) = self.ctx.arena.get_access_expr(target_node) else {
            return false;
        };

        // Get the property name
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            return false;
        };

        // Check if this is a private identifier (method or field)
        // Private methods are always readonly
        if self.is_private_identifier_name(access.name_or_argument) {
            let prop_name = if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                ident.escaped_text.clone()
            } else {
                return false;
            };

            // Check if this private identifier is a method (not a field)
            // by resolving the symbol and checking if any declaration is a method
            let (symbols, _) = self.resolve_private_identifier_symbols(access.name_or_argument);
            if !symbols.is_empty() {
                let is_method = symbols.iter().any(|&sym_id| {
                    if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                        symbol.declarations.iter().any(|&decl_idx| {
                            if let Some(node) = self.ctx.arena.get(decl_idx) {
                                return node.kind == syntax_kind_ext::METHOD_DECLARATION;
                            }
                            false
                        })
                    } else {
                        false
                    }
                });

                if is_method {
                    self.error_private_method_not_writable(&prop_name, access.name_or_argument);
                    // Return false (not true) so that the caller does NOT suppress the
                    // assignability check. TSC emits both TS2803 (private method not
                    // writable) AND TS2322 (type mismatch) for private method assignments.
                    // Returning true would cause suppress_for_readonly to skip TS2322.
                    return false;
                }
            }
        }

        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };

        let prop_name = ident.escaped_text.clone();

        // `globalThis.globalThis` is a readonly self-reference (TS2540).
        // Since `typeof globalThis` is modeled as ANY, the general readonly detection
        // can't discover this. TSC treats it as `readonly globalThis: typeof globalThis`.
        if prop_name == "globalThis" && self.is_global_this_expression(access.expression) {
            self.error_readonly_property_at(&prop_name, access.name_or_argument);
            return true;
        }

        // Check if the property is an enum member (TS2540) BEFORE property existence check.
        // Enum members may not be found by resolve_property_access_with_env because
        // they are resolved through the binder's enum symbol, not the type system.
        if self.is_enum_member_property(access.expression, &prop_name) {
            self.error_readonly_property_at(&prop_name, access.name_or_argument);
            return true;
        }

        // Get the type of the object being accessed and normalize it through
        // solver-backed evaluation before property/read-only checks.
        let obj_type = self.get_type_of_node(access.expression);
        let mut readonly_check_type = self.evaluate_type_for_assignability(obj_type);
        // If evaluation produced a deferred Mapped type (e.g., from Omit/Pick),
        // resolve it through the checker's TypeEnvironment to get concrete
        // property readonly flags.
        readonly_check_type = self.resolve_deferred_mapped_type(readonly_check_type);

        // When the object type is `any` or `error` (e.g., unresolved module import
        // with TS2307), skip readonly checks entirely. TSC doesn't emit TS2540 for
        // properties on `any`-typed values.
        if readonly_check_type == TypeId::ANY || readonly_check_type == TypeId::ERROR {
            return false;
        }

        // Check if the property is a const export from a namespace/module (TS2540).
        // For `M.x = 1` where `export const x = 0` in namespace M.
        // Check before property existence, similar to enum members.
        if self.is_namespace_const_property(access.expression, &prop_name) {
            self.error_readonly_property_at(&prop_name, access.name_or_argument);
            return true;
        }

        // P1 fix: First check if the property exists on the type.
        // If the property doesn't exist, skip the readonly check - TS2339 will be
        // reported elsewhere. This matches tsc behavior which checks existence before
        // readonly status.
        use crate::query_boundaries::common::PropertyAccessResult;
        let property_result =
            self.resolve_property_access_with_env(readonly_check_type, &prop_name);
        let (property_exists, prop_from_index_sig) = match &property_result {
            PropertyAccessResult::Success {
                from_index_signature,
                ..
            } => (true, *from_index_signature),
            _ => (false, false),
        };

        if !property_exists {
            // Property doesn't exist on this type - skip readonly check
            // The property existence error (TS2339) is reported elsewhere
            return false;
        }

        // Namespace imports (`import * as ns`) are immutable views of module exports.
        // Any assignment to an existing property should report TS2540.
        if self.is_namespace_import_binding(access.expression) {
            self.error_readonly_property_at(&prop_name, access.name_or_argument);
            return true;
        }

        // For mapped types (e.g., Pick, Omit), check readonly through the
        // mapped type's homomorphic source. The solver's property_is_readonly
        // can only detect explicit +readonly modifiers, but homomorphic mapped
        // types inherit readonly from source properties.
        if crate::query_boundaries::common::is_mapped_type(self.ctx.types, readonly_check_type)
            && self.is_mapped_type_property_readonly(readonly_check_type, &prop_name)
        {
            if self.is_readonly_assignment_allowed_in_constructor(&prop_name, access.expression) {
                return false;
            }
            if prop_from_index_sig {
                // tsc anchors TS2542 at the full property access expression, not the name
                self.error_readonly_index_signature_at(readonly_check_type, target_idx);
            } else {
                self.error_readonly_property_at(&prop_name, access.name_or_argument);
            }
            return true;
        }

        // Check if the property is readonly in the object type (solver types)
        if self.is_property_readonly(readonly_check_type, &prop_name) {
            // Special case: readonly properties can be assigned in constructors
            // if the property is declared in the current class (not inherited)
            if self.is_readonly_assignment_allowed_in_constructor(&prop_name, access.expression) {
                return false;
            }

            // TS2542: use specific diagnostic for readonly index signatures
            if prop_from_index_sig {
                // tsc anchors TS2542 at the full property access expression, not the name
                self.error_readonly_index_signature_at(readonly_check_type, target_idx);
            } else {
                self.error_readonly_property_at(&prop_name, access.name_or_argument);
            }
            return true;
        }

        // Also check AST-level readonly on class properties
        // Get the class name from the object expression (for `c.ro`, get the type of `c`)
        if let Some(class_name) = self.get_class_name_from_expression(access.expression)
            && self.is_class_property_readonly(&class_name, &prop_name)
        {
            // Special case: readonly properties can be assigned in constructors
            // if the property is declared in the current class (not inherited)
            if self.is_readonly_assignment_allowed_in_constructor(&prop_name, access.expression) {
                return false;
            }

            self.error_readonly_property_at(&prop_name, access.name_or_argument);
            return true;
        }

        // Check AST-level readonly on interface properties
        // For `obj.x = 10` where `obj: I` and `interface I { readonly x: number }`
        if let Some(type_name) = self.get_declared_type_name_from_expression(access.expression)
            && self.is_interface_property_readonly(&type_name, &prop_name)
        {
            self.error_readonly_property_at(&prop_name, access.name_or_argument);
            return true;
        }

        false
    }

    /// Check if an element access on a generic type parameter would be an unsafe write.
    ///
    /// Returns `true` when the object type is a type parameter (generic) and the
    /// index type is a broad primitive key space (`string`, `number`, `symbol`, or
    /// a union of them), AND the type parameter's constraint has an applicable index
    /// signature. In this case, TS2862 should be emitted.
    ///
    /// Does NOT fire when:
    /// - The index is a specific literal (`"x"`, `1`) — resolves to a named property
    /// - The index is `keyof T` — constrains to the receiver's own key space
    /// - The index is `K extends keyof T` — a type parameter constrained to keyof
    /// - The constraint has no index signature (e.g., `{ a: string, b: number }`)
    fn is_generic_indexed_write(&mut self, object_type: TypeId, index_type: TypeId) -> bool {
        // Object must be a type parameter (e.g., T in `function f<T extends ...>(target: T)`)
        if !crate::query_boundaries::state::checking::is_type_parameter(self.ctx.types, object_type)
        {
            return false;
        }

        // Broad primitive keys definitely go through an index signature.
        if self.is_broad_index_type(index_type) {
            return self.constraint_has_index_signature(object_type, index_type);
        }

        if let Some(key_source) =
            crate::query_boundaries::state::checking::keyof_target(self.ctx.types, index_type)
            && key_source != object_type
        {
            let evaluated_index = self.evaluate_type_with_env(index_type);
            if self.is_broad_index_type(evaluated_index) {
                return self.constraint_has_index_signature(object_type, evaluated_index);
            }
        }

        false
    }

    /// Check if the constraint of a type parameter has an index signature
    /// applicable to the given broad index type.
    ///
    /// Evaluates the constraint through `TypeEnvironment` first to resolve
    /// Application/Lazy wrappers (e.g., `Record<string, any>` → `{ [key: string]: any }`).
    fn constraint_has_index_signature(&mut self, type_param: TypeId, index_type: TypeId) -> bool {
        use tsz_solver::objects::index_signatures::{IndexKind, IndexSignatureResolver};

        let Some(info) =
            crate::query_boundaries::common::type_param_info(self.ctx.types, type_param)
        else {
            return false;
        };
        let Some(constraint) = info.constraint else {
            // No constraint means unconstrained T — no index signature
            return false;
        };

        // Evaluate the constraint to resolve mapped types, type aliases, etc.
        // E.g., Record<string, any> is stored as Application(Mapped) and needs
        // evaluation to produce { [key: string]: any }.
        let resolved = self.evaluate_type_with_env(constraint);
        let resolver = IndexSignatureResolver::new(self.ctx.types);

        // Check if the constraint has an index signature matching the broad index type
        if index_type == TypeId::STRING {
            return resolver.has_index_signature(resolved, IndexKind::String);
        }
        if index_type == TypeId::NUMBER {
            return resolver.has_index_signature(resolved, IndexKind::Number);
        }
        // For symbol or unions, check for string index signature (most permissive)
        resolver.has_index_signature(resolved, IndexKind::String)
            || resolver.has_index_signature(resolved, IndexKind::Number)
    }

    /// Check if a type is a "broad" index type that would access through an index
    /// signature rather than a specific property.
    ///
    /// Returns `true` for: `string`, `number`, `symbol`, or unions of these.
    /// Returns `false` for: literals, `keyof T`, type parameters, etc.
    fn is_broad_index_type(&self, type_id: TypeId) -> bool {
        // Direct primitive types — these go through index signatures
        if type_id == TypeId::STRING || type_id == TypeId::NUMBER || type_id == TypeId::SYMBOL {
            return true;
        }

        // Check if it's a union where ALL members are broad index types
        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, type_id)
        {
            return !members.is_empty() && members.iter().all(|&m| self.is_broad_index_type(m));
        }

        false
    }

    /// Check if a property access refers to a `const` export from a namespace or module.
    ///
    /// For expressions like `M.x` where `namespace M { export const x = 0; }`,
    /// the property `x` should be treated as readonly (TS2540).
    fn is_namespace_const_property(&self, object_expr: NodeIndex, prop_name: &str) -> bool {
        self.is_namespace_const_property_inner(object_expr, prop_name)
            .unwrap_or(false)
    }

    fn is_namespace_const_property_inner(
        &self,
        object_expr: NodeIndex,
        prop_name: &str,
    ) -> Option<bool> {
        use tsz_binder::symbol_flags;

        // Resolve the object expression to a symbol (e.g., M -> namespace symbol)
        let sym_id = self.resolve_identifier_symbol(object_expr)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;

        // Must be a namespace/module symbol
        if symbol.flags & symbol_flags::MODULE == 0 {
            return Some(false);
        }

        // Look up the property in the namespace's exports
        let member_sym_id = symbol.exports.as_ref()?.get(prop_name)?;
        let member_symbol = self.ctx.binder.get_symbol(member_sym_id)?;

        // Check if the member is a block-scoped variable (const/let)
        if member_symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE == 0 {
            return Some(false);
        }

        // Check if its value declaration has the CONST flag
        let value_decl = member_symbol.value_declaration;
        if value_decl.is_none() {
            return Some(false);
        }

        self.ctx.arena.get(value_decl)?;
        Some(self.ctx.arena.is_const_variable_declaration(value_decl))
    }

    /// Check if a property access refers to an enum member.
    /// All enum members are readonly — `A.foo = 1` is invalid for `enum A { foo }`.
    pub(crate) fn is_enum_member_property(&self, object_expr: NodeIndex, _prop_name: &str) -> bool {
        // Unwrap parenthesized expressions: (Foo).X as const
        let object_expr = self.ctx.arena.skip_parenthesized(object_expr);

        self.resolve_expression_to_enum_symbol(object_expr)
    }

    /// Resolve an expression to check if it refers to an enum symbol.
    /// Handles simple identifiers (e.g. `Foo`), imported enums (e.g. `import {Foo}`),
    /// and property access chains through namespaces (e.g. `ns.Foo`).
    fn resolve_expression_to_enum_symbol(&self, expr: NodeIndex) -> bool {
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.ctx.arena.get(expr) else {
            return false;
        };

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            // Handle ns.Foo — resolve LHS, then look up property in exports
            if let Some(access) = self.ctx.arena.get_access_expr(node)
                && let lhs = self.ctx.arena.skip_parenthesized(access.expression)
                && let Some(lhs_sym_id) = self.resolve_identifier_symbol(lhs)
                && let Some(prop_ident) = self.ctx.arena.get_identifier_at(access.name_or_argument)
            {
                // Follow aliases (imported namespaces)
                let resolved_sym_id = self
                    .resolve_alias_symbol(lhs_sym_id, &mut Vec::new())
                    .unwrap_or(lhs_sym_id);
                let Some(resolved_symbol) = self.ctx.binder.get_symbol(resolved_sym_id) else {
                    return false;
                };

                if (resolved_symbol.flags & symbol_flags::NAMESPACE) == 0 {
                    return false;
                }

                let name = prop_ident.escaped_text.as_str();
                if let Some(ref exports) = resolved_symbol.exports
                    && let Some(member_sym_id) = exports.get(name)
                    && let Some(member_symbol) = self.ctx.binder.get_symbol(member_sym_id)
                {
                    return member_symbol.flags & symbol_flags::ENUM != 0;
                }
            }
            return false;
        }

        // Simple identifier case — follow aliases for imported enums
        let Some(sym_id) = self.resolve_identifier_symbol(expr) else {
            return false;
        };
        let resolved_sym_id = self
            .resolve_alias_symbol(sym_id, &mut Vec::new())
            .unwrap_or(sym_id);
        let Some(symbol) = self.ctx.binder.get_symbol(resolved_sym_id) else {
            return false;
        };
        symbol.flags & symbol_flags::ENUM != 0
    }

    /// Check whether an expression resolves to an immutable module import binding.
    ///
    /// Includes:
    /// - `import * as ns from "mod"`
    ///
    /// Note: `import ns = require("mod")` is intentionally excluded here.
    /// Unlike ES namespace imports, import-equals aliases can observe mutable
    /// augmented exports (e.g. `declare module "m" { let x: number }`), so
    /// property writes should be validated against property readonly metadata
    /// instead of being blanket-rejected as TS2540.
    fn is_namespace_import_binding(&self, object_expr: NodeIndex) -> bool {
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;

        let object_expr = self.ctx.arena.skip_parenthesized(object_expr);
        let Some(sym_id) = self.resolve_identifier_symbol(object_expr) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        if (symbol.flags & symbol_flags::ALIAS) == 0 {
            return false;
        }

        symbol.declarations.iter().any(|&decl_idx| {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                return false;
            };
            if decl_node.kind == syntax_kind_ext::NAMESPACE_IMPORT {
                return true;
            }

            let Some(ext) = self.ctx.arena.get_extended(decl_idx) else {
                return false;
            };
            self.ctx
                .arena
                .get(ext.parent)
                .is_some_and(|parent| parent.kind == syntax_kind_ext::NAMESPACE_IMPORT)
        })
    }

    fn is_function_or_class_name_property(&self, object_expr: NodeIndex, prop_name: &str) -> bool {
        if prop_name != "name" {
            return false;
        }

        let object_expr = self.ctx.arena.skip_parenthesized(object_expr);
        let Some(sym_id) = self.resolve_identifier_symbol(object_expr) else {
            return false;
        };
        let resolved_sym_id = self
            .resolve_alias_symbol(sym_id, &mut Vec::new())
            .unwrap_or(sym_id);
        let Some(symbol) = self.ctx.binder.get_symbol(resolved_sym_id) else {
            return false;
        };

        (symbol.flags & (tsz_binder::symbol_flags::CLASS | tsz_binder::symbol_flags::FUNCTION)) != 0
    }

    /// Check if a readonly property assignment is allowed in the current constructor context.
    ///
    /// Returns true if ALL of the following conditions are met:
    /// 1. We're in a constructor body
    /// 2. The assignment is to `this.property` (not some other object)
    /// 3. The property is declared in the current class (not inherited)
    pub(crate) fn is_readonly_assignment_allowed_in_constructor(
        &self,
        prop_name: &str,
        object_expr: NodeIndex,
    ) -> bool {
        // Must be in a constructor
        let class_idx = match &self.ctx.enclosing_class {
            Some(info) if info.in_constructor => info.class_idx,
            _ => return false,
        };

        // Must be assigning to `this.property` (not some other object)
        if !self.is_this_expression_in_constructor(object_expr) {
            return false;
        }

        // The property must be declared in the current class (not inherited).
        // In JS files, constructor `this.prop = value` assignments serve as property
        // declarations, so they are always allowed for readonly properties.
        self.is_js_file() || self.is_property_declared_in_class(prop_name, class_idx)
    }

    /// Check if an expression is `this` (helper to avoid conflict with existing method).
    pub(crate) fn is_this_expression_in_constructor(&self, expr_idx: NodeIndex) -> bool {
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };

        // Check if it's ThisKeyword (node.kind == 110)
        if node.kind == SyntaxKind::ThisKeyword as u16 {
            return true;
        }

        // Check if it's an identifier with text "this"
        if let Some(ident) = self.ctx.arena.get_identifier(node) {
            return ident.escaped_text == "this";
        }

        false
    }

    /// Check if a property is declared in a specific class (not inherited).
    pub(crate) fn is_property_declared_in_class(
        &self,
        prop_name: &str,
        class_idx: NodeIndex,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(class_node) = self.ctx.arena.get(class_idx) else {
            return false;
        };

        let Some(class) = self.ctx.arena.get_class(class_node) else {
            return false;
        };

        // Check all class members for a property declaration
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            // Check property declarations
            if let Some(prop_decl) = self.ctx.arena.get_property_decl(member_node)
                && let Some(name_node) = self.ctx.arena.get(prop_decl.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                && ident.escaped_text == prop_name
            {
                return true;
            }

            // Check parameter properties (constructor parameters with readonly/private/etc)
            // Find the constructor kind
            if member_node.kind == syntax_kind_ext::CONSTRUCTOR
                && let Some(ctor) = self.ctx.arena.get_constructor(member_node)
            {
                for &param_idx in &ctor.parameters.nodes {
                    let Some(param_node) = self.ctx.arena.get(param_idx) else {
                        continue;
                    };

                    // Check if it's a parameter property
                    if let Some(param_decl) = self.ctx.arena.get_parameter(param_node) {
                        // Parameter properties have modifiers and a name but no type annotation is required
                        // They're identified by having modifiers (readonly, private, public, protected)
                        if param_decl.modifiers.is_some()
                            && let Some(name_node) = self.ctx.arena.get(param_decl.name)
                            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                            && ident.escaped_text == prop_name
                        {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    /// Get the class name from an expression, if it's a class instance.
    pub(crate) fn get_class_name_from_expression(&mut self, expr_idx: NodeIndex) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;

        let node = self.ctx.arena.get(expr_idx)?;

        if node.kind == syntax_kind_ext::CALL_EXPRESSION {
            let call = self.ctx.arena.get_call_expr(node)?;
            let decl_idx = self.function_like_decl_from_callee(call.expression)?;
            let decl_node = self.ctx.arena.get(decl_idx)?;

            if let Some(func) = self.ctx.arena.get_function(decl_node) {
                return self.returned_class_name_from_body(func.body);
            }

            if let Some(method) = self.ctx.arena.get_method_decl(decl_node) {
                return self.returned_class_name_from_body(method.body);
            }
        }

        // If it's a simple identifier, look up its type from the binder
        if self.ctx.arena.get_identifier(node).is_some()
            && let Some(sym_id) = self.resolve_identifier_symbol(expr_idx)
        {
            let type_id = self.get_type_of_symbol(sym_id);
            if let Some(class_name) = self.get_class_name_from_type(type_id) {
                return Some(class_name);
            }
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                // Get the value declaration and check if it's a variable with new Class()
                if symbol.value_declaration.is_some() {
                    return self.get_class_name_from_var_decl(symbol.value_declaration);
                }
            }
        }

        None
    }

    pub(crate) fn is_readonly_index_signature(
        &self,
        type_id: TypeId,
        wants_string: bool,
        wants_number: bool,
    ) -> bool {
        self.ctx
            .types
            .is_readonly_index_signature(type_id, wants_string, wants_number)
    }

    pub(crate) fn get_readonly_element_access_name(
        &self,
        object_type: TypeId,
        index_expr: NodeIndex,
        index_type: TypeId,
    ) -> Option<String> {
        // First check for literal string/number properties that are readonly
        if let Some(name) = self.get_literal_string_from_node(index_expr)
            && self.is_property_readonly(object_type, &name)
        {
            return Some(name);
        }
        // Don't return yet - the literal might access a readonly index signature

        if let Some(index) = self.get_literal_index_from_node(index_expr) {
            let name = index.to_string();
            if self.is_property_readonly(object_type, &name) {
                return Some(name);
            }
            // Don't return yet - the literal might access a readonly index signature
        }

        if let Some((string_keys, number_keys)) = self.get_literal_key_union_from_type(index_type) {
            for key in string_keys {
                let name = self.ctx.types.resolve_atom(key);
                if self.is_property_readonly(object_type, &name) {
                    return Some(name);
                }
            }

            for key in number_keys {
                let name = format!("{key}");
                if self.is_property_readonly(object_type, &name) {
                    return Some(name);
                }
            }
            // Don't return yet - check for readonly index signatures
        }

        // Finally check for readonly index signatures
        if let Some((wants_string, wants_number)) = self.get_index_key_kind(index_type)
            && self.is_readonly_index_signature(object_type, wants_string, wants_number)
        {
            return Some("index signature".to_string());
        }

        // Note: Mapped types with explicit readonly modifier (e.g., Readonly<T>)
        // are checked separately in check_readonly_assignment because they require
        // mutable access to evaluate through the TypeEnvironment.

        None
    }

    /// Check if a type is a mapped type with an explicit `+readonly` modifier.
    ///
    /// Evaluates through the `TypeEnvironment` to resolve Application/Lazy wrappers
    /// (e.g., `Readonly<T>` where T is generic), then delegates to the solver's
    /// `is_mapped_type_with_readonly_modifier` query.
    /// Check if a specific property is readonly in a mapped type by examining
    /// the homomorphic source type's property modifiers.
    ///
    /// For mapped types like `Pick<A, K>` = `{ [P in K]: A[P] }`, the readonly
    /// flag is inherited from the source type `A`. The solver's standalone
    /// `property_is_readonly` only checks for explicit `+readonly` modifiers,
    /// but homomorphic mapped types need to look at the source property.
    fn is_mapped_type_property_readonly(&mut self, type_id: TypeId, prop_name: &str) -> bool {
        let db = self.ctx.types.as_type_database();
        let Some(mapped_id) = tsz_solver::mapped_type_id(self.ctx.types, type_id) else {
            return false;
        };
        let mapped = db.get_mapped(mapped_id);

        // Explicit +readonly modifier means all properties are readonly.
        if mapped.readonly_modifier == Some(tsz_solver::MappedModifier::Add) {
            return true;
        }
        // Explicit -readonly modifier means all properties are mutable.
        if mapped.readonly_modifier == Some(tsz_solver::MappedModifier::Remove) {
            return false;
        }

        // No explicit modifier: check if homomorphic and inherit from source.
        // Homomorphic pattern: template is IndexAccess(source, param) where
        // param matches the mapped type's iteration parameter.
        if let Some(source) = tsz_solver::type_queries::homomorphic_mapped_source(db, type_id) {
            // This is a homomorphic mapped type. Resolve the source type
            // through the checker environment and check the property.
            let resolved_source = self.evaluate_type_with_resolution(source);
            return self.is_property_readonly(resolved_source, prop_name);
        }

        false
    }

    fn is_readonly_mapped_type(&mut self, type_id: TypeId) -> bool {
        use tsz_solver::operations::property::is_mapped_type_with_readonly_modifier;

        // First try the direct solver query (handles Mapped, Application, Lazy)
        if is_mapped_type_with_readonly_modifier(self.ctx.types, type_id) {
            return true;
        }
        // For Application types wrapping Lazy(DefId), the standalone solver evaluator
        // can't resolve DefIds. Evaluate through the checker's TypeEnvironment first.
        let resolved = self.evaluate_type_with_env(type_id);
        if resolved != type_id {
            return is_mapped_type_with_readonly_modifier(self.ctx.types, resolved);
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{CheckerOptions, ScriptTarget};
    use crate::query_boundaries::type_construction::TypeInterner;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;
    use tsz_parser::parser::node::NodeArena;

    fn find_node_by_text_and_kind(
        arena: &NodeArena,
        source: &str,
        kind: u16,
        text: &str,
    ) -> Option<NodeIndex> {
        (0..arena.len()).find_map(|i| {
            let idx = NodeIndex(i as u32);
            let node = arena.get(idx)?;
            (node.kind == kind && &source[node.pos as usize..node.end as usize] == text)
                .then_some(idx)
        })
    }

    #[test]
    fn get_class_name_from_expression_resolves_named_class_expression_return() {
        use tsz_parser::parser::syntax_kind_ext;

        let source = r#"
const C = class D {
    static #field = D.#method();
    static #method() { return 42; }
    static getClass() { return D; }
};

C.getClass().#method;
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            CheckerOptions {
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );

        checker.check_source_file(root);

        let call_idx = find_node_by_text_and_kind(
            parser.get_arena(),
            source,
            syntax_kind_ext::CALL_EXPRESSION,
            "C.getClass()",
        )
        .expect("expected to find `C.getClass()` call expression");

        assert_eq!(
            checker.get_class_name_from_expression(call_idx),
            Some("D".to_string())
        );
    }
}
