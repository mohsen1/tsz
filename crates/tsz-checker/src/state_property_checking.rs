//! Object literal, readonly, and property access checking.

use crate::query_boundaries::state_checking as query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn check_object_literal_excess_properties(
        &mut self,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
    ) {
        use tsz_solver::freshness;

        // Excess property checks do not apply to type parameters (even with constraints).
        if query::is_type_parameter(self.ctx.types, target) {
            return;
        }

        // Only check excess properties for FRESH object literals
        // This is the key TypeScript behavior:
        // - const p: Point = {x: 1, y: 2, z: 3}  // ERROR: 'z' is excess (fresh)
        // - const obj = {x: 1, y: 2, z: 3}; p = obj;  // OK: obj loses freshness
        //
        // IMPORTANT: Freshness is tracked on the TypeId itself.
        // This fixes the "Zombie Freshness" bug by keeping fresh vs non-fresh
        // object types distinct at the interner level.
        if !freshness::is_fresh_object_type(self.ctx.types, source) {
            return;
        }

        // Get the properties of source type using type_queries
        let Some(source_shape) = query::object_shape(self.ctx.types, source) else {
            return;
        };

        let source_props = source_shape.properties.as_slice();
        let resolved_target = self.resolve_type_for_property_access(target);

        // Handle union targets first using type_queries
        if let Some(members) = query::union_members(self.ctx.types, resolved_target) {
            let mut target_shapes = Vec::new();

            for &member in &members {
                let resolved_member = self.resolve_type_for_property_access(member);
                let Some(shape) = query::object_shape(self.ctx.types, resolved_member) else {
                    // If a union member has no object shape and is a type parameter
                    // or the `object` intrinsic, it conceptually accepts any properties,
                    // so excess property checking should not apply at all.
                    if tsz_solver::type_queries::is_type_parameter(self.ctx.types, resolved_member)
                        || resolved_member == TypeId::OBJECT
                    {
                        return;
                    }
                    continue;
                };

                if shape.properties.is_empty()
                    || shape.string_index.is_some()
                    || shape.number_index.is_some()
                {
                    return;
                }

                target_shapes.push(shape);
            }

            if target_shapes.is_empty() {
                return;
            }

            for source_prop in source_props {
                // For unions, check if property exists in ANY member
                let target_prop_types: Vec<TypeId> = target_shapes
                    .iter()
                    .filter_map(|shape| {
                        shape
                            .properties
                            .iter()
                            .find(|prop| prop.name == source_prop.name)
                            .map(|prop| prop.type_id)
                    })
                    .collect();

                if target_prop_types.is_empty() {
                    let prop_name = self.ctx.types.resolve_atom(source_prop.name);
                    self.error_excess_property_at(&prop_name, target, idx);
                } else {
                    // =============================================================
                    // NESTED OBJECT LITERAL EXCESS PROPERTY CHECKING
                    // =============================================================
                    // For nested object literals, recursively check for excess properties
                    // Example: { x: { y: 1, z: 2 } } where target is { x: { y: number } }
                    // should error on 'z' in the nested object literal
                    //
                    // CRITICAL FIX: For union targets, we must union all property types
                    // from all members. Using only the first member causes false positives.
                    // Example: type T = { x: { a: number } } | { x: { b: number } }
                    // Assigning { x: { b: 1 } } should NOT error on 'b'.
                    // =============================================================
                    let nested_target = tsz_solver::utils::union_or_single(
                        self.ctx.types,
                        target_prop_types.clone(),
                    );

                    self.check_nested_object_literal_excess_properties(
                        source_prop.name,
                        Some(nested_target),
                        idx,
                    );
                }
            }
            return;
        }

        // Handle object targets using type_queries
        if let Some(target_shape) = query::object_shape(self.ctx.types, resolved_target) {
            let target_props = target_shape.properties.as_slice();

            // Empty object {} accepts any properties - no excess property check needed.
            // This is a key TypeScript behavior: {} means "any non-nullish value".
            // See https://github.com/microsoft/TypeScript/issues/60582
            if target_props.is_empty() {
                return;
            }

            if target_shape.string_index.is_some() || target_shape.number_index.is_some() {
                return;
            }

            // If target has an index signature, it accepts any properties
            if target_shape.string_index.is_some() || target_shape.number_index.is_some() {
                return;
            }
            // This is the "freshness" or "strict object literal" check
            for source_prop in source_props {
                let target_prop = target_props.iter().find(|p| p.name == source_prop.name);
                if target_prop.is_none() {
                    let prop_name = self.ctx.types.resolve_atom(source_prop.name);
                    self.error_excess_property_at(&prop_name, target, idx);
                } else if let Some(target_prop) = target_prop {
                    // =============================================================
                    // NESTED OBJECT LITERAL EXCESS PROPERTY CHECKING
                    // =============================================================
                    // For nested object literals, recursively check for excess properties
                    self.check_nested_object_literal_excess_properties(
                        source_prop.name,
                        Some(target_prop.type_id),
                        idx,
                    );
                }
            }
        }
        // Note: Missing property checks are handled by solver's explain_failure
    }

    /// Check nested object literal properties for excess properties.
    ///
    /// This implements recursive excess property checking for nested object literals.
    /// For example, in `const p: { x: { y: number } } = { x: { y: 1, z: 2 } }`,
    /// the nested object literal `{ y: 1, z: 2 }` should be checked for excess property `z`.
    fn check_nested_object_literal_excess_properties(
        &mut self,
        prop_name: tsz_common::interner::Atom,
        target_prop_type: Option<TypeId>,
        obj_literal_idx: NodeIndex,
    ) {
        // Get the AST node for the object literal
        let Some(obj_node) = self.ctx.arena.get(obj_literal_idx) else {
            return;
        };

        let Some(obj_lit) = self.ctx.arena.get_literal_expr(obj_node) else {
            return;
        };

        // =============================================================
        // CRITICAL FIX: Iterate in reverse to handle duplicate properties
        // =============================================================
        // JavaScript/TypeScript behavior is "last property wins".
        // Example: const o = { x: { a: 1 }, x: { b: 1 } }
        // The runtime value of o.x is { b: 1 }, so we must check the last assignment.
        // =============================================================
        for &elem_idx in obj_lit.elements.nodes.iter().rev() {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Get the property name from this element
            let elem_prop_name = match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_property_assignment(elem_node)
                    .and_then(|prop| self.get_property_name(prop.name))
                    .map(|name| self.ctx.types.intern_string(&name)),
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_shorthand_property(elem_node)
                    .and_then(|prop| {
                        self.get_property_name(prop.name)
                            .map(|name| self.ctx.types.intern_string(&name))
                    }),
                _ => None,
            };

            // Skip if this property doesn't match the one we're looking for
            if elem_prop_name != Some(prop_name) {
                continue;
            }

            // Get the value expression for this property
            let value_idx = match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_property_assignment(elem_node)
                    .map(|prop| prop.initializer),
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    // For shorthand properties, the value expression is the same as the property name expression
                    self.ctx
                        .arena
                        .get_shorthand_property(elem_node)
                        .map(|prop| prop.name)
                }
                _ => None,
            };

            let Some(value_idx) = value_idx else {
                continue;
            };

            // =============================================================
            // CRITICAL FIX: Handle parenthesized expressions
            // =============================================================
            // TypeScript treats parenthesized object literals as fresh.
            // Example: x: ({ a: 1 }) should be checked for excess properties.
            // We need to unwrap parentheses before checking the kind.
            // =============================================================
            let effective_value_idx = self.skip_parentheses(value_idx);
            let Some(value_node) = self.ctx.arena.get(effective_value_idx) else {
                continue;
            };

            if value_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                // Get the type of the nested object literal
                let nested_source_type = self.get_type_of_node(effective_value_idx);

                // Check if we have a target type for this property
                if let Some(nested_target_type) = target_prop_type {
                    // Recursively check the nested object literal for excess properties
                    self.check_object_literal_excess_properties(
                        nested_source_type,
                        nested_target_type,
                        effective_value_idx,
                    );
                }

                return; // Found the property, stop searching
            }
        }
    }

    /// Skip parentheses to get the effective expression node.
    ///
    /// This unwraps parenthesized expressions to get the underlying expression.
    /// Example: `({ a: 1 })` -> `{ a: 1 }` (`OBJECT_LITERAL_EXPRESSION`)
    fn skip_parentheses(&self, mut node_idx: NodeIndex) -> NodeIndex {
        while let Some(node) = self.ctx.arena.get(node_idx) {
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.ctx.arena.get_parenthesized(node)
            {
                node_idx = paren.expression;
                continue;
            }
            break;
        }
        node_idx
    }

    /// TS2353 guard for object destructuring from object literals with computed keys.
    ///
    /// TypeScript reports excess-property errors for computed properties in object
    /// literal initializers when the binding pattern contains only explicit keys.
    pub(crate) fn check_destructuring_object_literal_computed_excess_properties(
        &mut self,
        pattern_idx: NodeIndex,
        initializer_idx: NodeIndex,
        target_type: TypeId,
    ) {
        if initializer_idx.is_none() || target_type == TypeId::ANY || target_type == TypeId::ERROR {
            return;
        }

        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };
        if pattern_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN {
            return;
        }
        let Some(pattern) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        // Keep this narrow: if the pattern has rest or computed names, leave behavior to
        // the general relation path.
        for &element_idx in &pattern.elements.nodes {
            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            let Some(element) = self.ctx.arena.get_binding_element(element_node) else {
                continue;
            };
            if element.dot_dot_dot_token {
                return;
            }
            if !element.property_name.is_none()
                && let Some(prop_name_node) = self.ctx.arena.get(element.property_name)
                && prop_name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            {
                return;
            }
        }

        let effective_init = self.skip_parentheses(initializer_idx);
        let Some(init_node) = self.ctx.arena.get(effective_init) else {
            return;
        };
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return;
        };
        let Some(init_lit) = self.ctx.arena.get_literal_expr(init_node) else {
            return;
        };

        // Get the properties of the target type
        let Some(target_shape) = query::object_shape(self.ctx.types, target_type) else {
            return;
        };
        let target_props = target_shape.properties.as_slice();

        for &elem_idx in &init_lit.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Get the property name from this element
            let prop_name = match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_property_assignment(elem_node)
                    .and_then(|prop| self.get_property_name(prop.name)),
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_shorthand_property(elem_node)
                    .and_then(|prop| self.get_property_name(prop.name)),
                _ => None,
            };

            let Some(prop_name) = prop_name else {
                continue;
            };

            let prop_atom = self.ctx.types.intern_string(&prop_name);

            // Check if the property exists in the target type
            let target_prop = target_props.iter().find(|p| p.name == prop_atom);
            if target_prop.is_none()
                && let Some(ext) = self.ctx.arena.get_extended(elem_idx) {
                    self.error_excess_property_at(&prop_name, target_type, ext.parent);
                }
        }
    }

    /// Resolve property access using `TypeEnvironment` (includes lib.d.ts types).
    ///
    /// This method creates a `PropertyAccessEvaluator` with the `TypeEnvironment` as the resolver,
    /// allowing primitive property access to use lib.d.ts definitions instead of just hardcoded lists.
    ///
    /// For example, "foo".length will look up the String interface from lib.d.ts.
    pub(crate) fn resolve_property_access_with_env(
        &mut self,
        object_type: TypeId,
        prop_name: &str,
    ) -> tsz_solver::operations_property::PropertyAccessResult {
        // Resolve TypeQuery types (typeof X) before property access.
        // The solver-internal evaluator has no TypeResolver, so TypeQuery types
        // can't be resolved there. Resolve them here using the checker's environment.
        let object_type = self.resolve_type_query_type(object_type);

        // Ensure preconditions are ready in the environment for non-trivial
        // property-access inputs. Already-resolved/function-like inputs don't
        // need relation preconditioning here.
        let resolution_kind =
            crate::query_boundaries::state_type_environment::classify_for_property_access_resolution(
                self.ctx.types,
                object_type,
            );
        if !matches!(
            resolution_kind,
            crate::query_boundaries::state_type_environment::PropertyAccessResolutionKind::Resolved
                | crate::query_boundaries::state_type_environment::PropertyAccessResolutionKind::FunctionLike
        ) {
            self.ensure_relation_input_ready(object_type);
        }

        // Route through QueryDatabase so repeated property lookups hit QueryCache.
        // This is especially important for hot paths like repeated `string[].push`
        // checks in class-heavy files.
        let result = self.ctx.types.resolve_property_access_with_options(
            object_type,
            prop_name,
            self.ctx.compiler_options.no_unchecked_indexed_access,
        );

        self.resolve_property_access_with_env_post_query(object_type, prop_name, result)
    }

    /// Continue environment-aware property access resolution from an already
    /// computed initial solver result.
    ///
    /// This avoids duplicate first-pass lookups in hot paths that already
    /// queried `resolve_property_access_with_options` and only need mapped/
    /// application fallback behavior.
    pub(crate) fn resolve_property_access_with_env_post_query(
        &mut self,
        object_type: TypeId,
        prop_name: &str,
        result: tsz_solver::operations_property::PropertyAccessResult,
    ) -> tsz_solver::operations_property::PropertyAccessResult {
        let mut result = result;

        // If property not found and the type is an Application (e.g. Promise<number>),
        // the QueryCache's noop TypeResolver can't expand it. Evaluate the Application
        // to its structural form and retry property access on the expanded type.
        if matches!(
            result,
            tsz_solver::operations_property::PropertyAccessResult::PropertyNotFound { .. }
        ) && tsz_solver::is_generic_application(self.ctx.types, object_type)
        {
            let expanded = self.evaluate_application_type(object_type);
            if expanded != object_type && expanded != TypeId::ANY && expanded != TypeId::ERROR {
                result = self.ctx.types.resolve_property_access_with_options(
                    expanded,
                    prop_name,
                    self.ctx.compiler_options.no_unchecked_indexed_access,
                );
            }
        }

        // If property not found and the type is a Mapped type (e.g. { [P in Keys]: T }),
        // the solver's NoopResolver can't resolve Lazy(DefId) constraints (type alias refs).
        // Expand the mapped type using the checker's type environment and retry.
        if matches!(
            result,
            tsz_solver::operations_property::PropertyAccessResult::PropertyNotFound { .. }
        ) && tsz_solver::type_queries::is_mapped_type(self.ctx.types, object_type)
        {
            if let Some(mapped_property) =
                self.resolve_mapped_property_with_env(object_type, prop_name)
            {
                return mapped_property;
            }

            let expanded = self.evaluate_mapped_type_with_resolution(object_type);
            if expanded != object_type && expanded != TypeId::ANY && expanded != TypeId::ERROR {
                return self.ctx.types.resolve_property_access_with_options(
                    expanded,
                    prop_name,
                    self.ctx.compiler_options.no_unchecked_indexed_access,
                );
            }
        }

        result
    }

    /// Resolve a single mapped-type property with environment-aware key/template
    /// evaluation, without expanding the whole mapped object.
    ///
    /// Returns `None` when we cannot safely decide (e.g. complex key space),
    /// allowing the caller to fall back to full mapped expansion.
    fn resolve_mapped_property_with_env(
        &mut self,
        mapped_type: TypeId,
        prop_name: &str,
    ) -> Option<tsz_solver::operations_property::PropertyAccessResult> {
        let mapped_id = tsz_solver::mapped_type_id(self.ctx.types, mapped_type)?;
        let mapped = self.ctx.types.mapped_type(mapped_id);

        // Keep `as`-remapped keys on the conservative path for now.
        if mapped.name_type.is_some() {
            return None;
        }

        let constraint = self.evaluate_mapped_constraint_with_resolution(mapped.constraint);
        let prop_atom = self.ctx.types.intern_string(prop_name);
        let can_cache =
            !tsz_solver::type_queries::contains_type_parameters_db(self.ctx.types, mapped_type);
        let cache_key = (mapped_type, prop_atom);

        if can_cache
            && let Some(cached) = self
                .ctx
                .narrowing_cache
                .property_cache
                .borrow()
                .get(&cache_key)
                .copied()
        {
            return Some(match cached {
                Some(type_id) => tsz_solver::operations_property::PropertyAccessResult::Success {
                    type_id,
                    write_type: None,
                    from_index_signature: false,
                },
                None => tsz_solver::operations_property::PropertyAccessResult::PropertyNotFound {
                    type_id: mapped_type,
                    property_name: prop_atom,
                },
            });
        }

        // If the constraint is an explicit literal key set, reject unknown keys early.
        // For non-literal/complex constraints, fall back to full expansion.
        if !tsz_solver::type_queries::is_string_type(self.ctx.types, constraint) {
            let keys =
                tsz_solver::type_queries::extract_string_literal_keys(self.ctx.types, constraint);
            if !keys.is_empty() && !keys.contains(&prop_atom) {
                if can_cache {
                    self.ctx
                        .narrowing_cache
                        .property_cache
                        .borrow_mut()
                        .insert(cache_key, None);
                }
                return Some(
                    tsz_solver::operations_property::PropertyAccessResult::PropertyNotFound {
                        type_id: mapped_type,
                        property_name: prop_atom,
                    },
                );
            }
            if keys.is_empty() {
                return None;
            }
        }

        let key_literal = self.ctx.types.literal_string_atom(prop_atom);
        let mut subst = tsz_solver::TypeSubstitution::new();
        subst.insert(mapped.type_param.name, key_literal);

        let property_type = tsz_solver::instantiate_type(self.ctx.types, mapped.template, &subst);
        let property_type = self.evaluate_type_with_env(property_type);
        let property_type = match mapped.optional_modifier {
            Some(tsz_solver::MappedModifier::Add) => self
                .ctx
                .types
                .factory()
                .union(vec![property_type, TypeId::UNDEFINED]),
            Some(tsz_solver::MappedModifier::Remove) | None => property_type,
        };

        if can_cache {
            self.ctx
                .narrowing_cache
                .property_cache
                .borrow_mut()
                .insert(cache_key, Some(property_type));
        }

        Some(
            tsz_solver::operations_property::PropertyAccessResult::Success {
                type_id: property_type,
                write_type: None,
                from_index_signature: false,
            },
        )
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
                    if let Some(name) = self.get_readonly_element_access_name(
                        object_type,
                        access.name_or_argument,
                        index_type,
                    ) {
                        // TS2542: use specific diagnostic for readonly index signatures.
                        // Check if the property resolved through an index signature
                        // (either the explicit "index signature" sentinel or via
                        // from_index_signature on a named property).
                        use tsz_solver::operations_property::PropertyAccessResult;
                        let from_idx_sig = if name == "index signature" {
                            true
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
                            self.error_readonly_index_signature_at(object_type, target_idx);
                        } else {
                            self.error_readonly_property_at(&name, target_idx);
                        }
                        return true;
                    }
                    // Check AST-level interface readonly for element access (obj["x"])
                    if let Some(name) = self.get_literal_string_from_node(access.name_or_argument) {
                        if let Some(type_name) =
                            self.get_declared_type_name_from_expression(access.expression)
                            && self.is_interface_property_readonly(&type_name, &name)
                        {
                            self.error_readonly_property_at(&name, target_idx);
                            return true;
                        }
                        // Also check namespace const exports via element access (M["x"])
                        if self.is_namespace_const_property(access.expression, &name) {
                            self.error_readonly_property_at(&name, target_idx);
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
                    self.error_private_method_not_writable(&prop_name, target_idx);
                    return true;
                }
            }
        }

        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };

        let prop_name = ident.escaped_text.clone();

        // Check if the property is an enum member (TS2540) BEFORE property existence check.
        // Enum members may not be found by resolve_property_access_with_env because
        // they are resolved through the binder's enum symbol, not the type system.
        if self.is_enum_member_property(access.expression, &prop_name) {
            self.error_readonly_property_at(&prop_name, target_idx);
            return true;
        }

        // Get the type of the object being accessed and normalize it through
        // solver-backed evaluation before property/read-only checks.
        let obj_type = self.get_type_of_node(access.expression);
        let readonly_check_type = self.evaluate_type_for_assignability(obj_type);

        // Check if the property is a const export from a namespace/module (TS2540).
        // For `M.x = 1` where `export const x = 0` in namespace M.
        // Check before property existence, similar to enum members.
        if self.is_namespace_const_property(access.expression, &prop_name) {
            self.error_readonly_property_at(&prop_name, target_idx);
            return true;
        }

        // P1 fix: First check if the property exists on the type.
        // If the property doesn't exist, skip the readonly check - TS2339 will be
        // reported elsewhere. This matches tsc behavior which checks existence before
        // readonly status.
        use tsz_solver::operations_property::PropertyAccessResult;
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
            self.error_readonly_property_at(&prop_name, target_idx);
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
                self.error_readonly_index_signature_at(readonly_check_type, target_idx);
            } else {
                self.error_readonly_property_at(&prop_name, target_idx);
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

            self.error_readonly_property_at(&prop_name, target_idx);
            return true;
        }

        // Check AST-level readonly on interface properties
        // For `obj.x = 10` where `obj: I` and `interface I { readonly x: number }`
        if let Some(type_name) = self.get_declared_type_name_from_expression(access.expression)
            && self.is_interface_property_readonly(&type_name, &prop_name)
        {
            self.error_readonly_property_at(&prop_name, target_idx);
            return true;
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

        let decl_node = self.ctx.arena.get(value_decl)?;
        let mut decl_flags = decl_node.flags as u32;

        // If CONST flag not directly on node, check parent (VariableDeclarationList)
        use tsz_parser::parser::flags::node_flags;
        if (decl_flags & node_flags::CONST) == 0
            && let Some(ext) = self.ctx.arena.get_extended(value_decl)
            && let Some(parent_node) = self.ctx.arena.get(ext.parent)
            && parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
        {
            decl_flags |= parent_node.flags as u32;
        }

        Some(decl_flags & node_flags::CONST != 0)
    }

    /// Check if a property access refers to an enum member.
    /// All enum members are readonly — `A.foo = 1` is invalid for `enum A { foo }`.
    fn is_enum_member_property(&self, object_expr: NodeIndex, _prop_name: &str) -> bool {
        let sym_id = self.resolve_identifier_symbol(object_expr);
        let Some(sym_id) = sym_id else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        use tsz_binder::symbol_flags;
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

        let object_expr = self.skip_parentheses(object_expr);
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

        // The property must be declared in the current class (not inherited)
        self.is_property_declared_in_class(prop_name, class_idx)
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
        let node = self.ctx.arena.get(expr_idx)?;

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
                if !symbol.value_declaration.is_none() {
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

        None
    }
}
