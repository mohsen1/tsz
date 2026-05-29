impl<'a> CheckerState<'a> {
    fn actual_lib_namespace_merged_type_has_property(
        &mut self,
        type_id: TypeId,
        prop_name: &str,
    ) -> bool {
        let lazy_def_id = query::lazy_def_id(self.ctx.types, type_id);
        let sym_id = lazy_def_id
            .and_then(|def_id| self.ctx.def_to_symbol_id(def_id))
            .or_else(|| query::type_shape_symbol(self.ctx.types, type_id));

        let (export_name, require_symbol_match) = if let Some(sym_id) = sym_id {
            let lib_binders = self.get_lib_binders();
            let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
                return false;
            };
            if self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id) {
                (symbol.escaped_name.clone(), Some(sym_id))
            } else if symbol.parent.is_some()
                && self.ctx.symbol_is_from_actual_or_cloned_lib(symbol.parent)
            {
                (symbol.escaped_name.clone(), None)
            } else {
                return false;
            }
        } else {
            let Some(def_id) = lazy_def_id else {
                return false;
            };
            let Some(def_info) = self.ctx.definition_store.get(def_id) else {
                return false;
            };
            let name = self.ctx.types.resolve_atom_ref(def_info.name).to_string();
            if name.is_empty() {
                return false;
            }
            (name, None)
        };

        let namespace = "Intl";
        let Some(export_sym_id) = self.resolve_lib_namespace_export_symbol(namespace, &export_name)
        else {
            return false;
        };
        if require_symbol_match.is_some_and(|sym_id| export_sym_id != sym_id) {
            return false;
        }

        let cache_name = format!("{namespace}.{export_name}");
        self.ctx.lib_type_resolution_cache.remove(&cache_name);
        let Some(merged_type) =
            self.resolve_lib_interface_type_by_symbol(&cache_name, export_sym_id)
        else {
            return false;
        };
        let prop_atom = self.ctx.types.intern_string(prop_name);
        query::raw_property_type(self.ctx.types.as_type_database(), merged_type, prop_atom)
            .is_some()
    }

    /// Report TS2339 with an explicit type display string instead of formatting from TypeId.
    /// Used when the apparent type should be displayed (e.g., `object` → `{}` in destructuring).
    pub fn error_property_not_exist_with_apparent_type(
        &mut self,
        prop_name: &str,
        type_display: &str,
        idx: NodeIndex,
    ) {
        // Suppress TS2339 when the property access is on an expression rooted in an
        // unresolved import (TS2307 was already emitted for the missing module).
        if let Some(parent) = self.ctx.arena.get_extended(idx)
            && let Some(parent_node) = self.ctx.arena.get(parent.parent)
            && parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(parent_node)
        {
            if self.is_unresolved_import_symbol(access.expression) {
                return;
            }
            if self.is_property_access_on_unresolved_import(parent.parent) {
                return;
            }
        }

        let message = format!("Property '{prop_name}' does not exist on type '{type_display}'.");
        self.error_at_anchor(
            idx,
            DiagnosticAnchorKind::PropertyToken,
            &message,
            diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
        );
    }

    /// Report TS2339/TS2551 for an enum object property access failure.
    /// Checks for spelling suggestions and emits TS2551 if a match is found.
    pub fn error_property_not_exist_on_enum(
        &mut self,
        prop_name: &str,
        enum_name: &str,
        object_type: TypeId,
        idx: NodeIndex,
    ) {
        let type_str = format!("typeof {enum_name}");
        let suggestion = self.find_similar_property(prop_name, object_type);
        if let Some(ref suggestion) = suggestion {
            let message = format!(
                "Property '{prop_name}' does not exist on type '{type_str}'. Did you mean '{suggestion}'?"
            );
            self.error_at_anchor(
                idx,
                DiagnosticAnchorKind::PropertyToken,
                &message,
                diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN,
            );
        } else {
            self.error_property_not_exist_with_apparent_type(prop_name, &type_str, idx);
        }
    }

    /// Report TS18046: "'x' is of type 'unknown'."
    /// Emitted when an expression of type `unknown` is used in a position that requires
    /// a more specific type (property access, function call, arithmetic, etc.).
    /// Falls back to TS2571 ("Object is of type 'unknown'.") when the expression name
    /// cannot be determined.
    ///
    /// Returns `true` if the error was emitted, `false` if suppressed.
    /// Callers should treat `unknown` as `any` when `false`.
    pub fn error_is_of_type_unknown(&mut self, expr_idx: NodeIndex) -> bool {
        // In tsc, TS18046 is emitted regardless of --strictNullChecks.
        // The `unknown` type is always restricted: you cannot access properties,
        // call, or operate on it without narrowing. The --strictNullChecks flag
        // only controls `null`/`undefined` checking (TS2531/TS2532), not `unknown`.
        let expr_text = self.expression_text(expr_idx);
        let loc = self.get_source_location(expr_idx);

        // Namespace imports are value bindings (`import * as ns`) and should not
        // produce TS18046 when internal module namespace resolution falls back
        // to unknown during cross-file/type-only export scenarios.
        if self.is_namespace_import_rooted_expression(expr_idx) {
            return false;
        }
        if self.ctx.is_js_file() && self.commonjs_destructured_named_export_exists(expr_idx) {
            return false;
        }
        let name = expr_text;
        if loc.is_some() {
            let (code, message) = if let Some(ref name) = name {
                (
                    diagnostic_codes::IS_OF_TYPE_UNKNOWN,
                    format!("'{name}' is of type 'unknown'."),
                )
            } else {
                (
                    diagnostic_codes::OBJECT_IS_OF_TYPE_UNKNOWN,
                    "Object is of type 'unknown'.".to_string(),
                )
            };
            self.error_at_node(expr_idx, &message, code);
            return true;
        }
        false
    }

    fn is_namespace_import_rooted_expression(&self, expr_idx: NodeIndex) -> bool {
        use tsz_binder::symbol_flags;

        let Some(root_ident) = self.root_identifier_for_expression(expr_idx) else {
            return false;
        };
        let Some(sym_id) = self.resolve_identifier_symbol(root_ident) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let symbol_is_namespace_import = symbol.import_module.is_some()
            && (symbol.import_name.is_none() || symbol.import_name.as_deref() == Some("*"));
        if symbol_is_namespace_import {
            return true;
        }
        if symbol.has_any_flags(symbol_flags::ALIAS) {
            let mut visited = AliasCycleTracker::new();
            if let Some(resolved_sym_id) = self.resolve_alias_symbol(sym_id, &mut visited)
                && let Some(resolved_symbol) = self.ctx.binder.get_symbol(resolved_sym_id)
            {
                let resolved_is_namespace_import = resolved_symbol.import_module.is_some()
                    && (resolved_symbol.import_name.is_none()
                        || resolved_symbol.import_name.as_deref() == Some("*"));
                if resolved_is_namespace_import {
                    return true;
                }
                if resolved_symbol.has_any_flags(symbol_flags::MODULE) {
                    return true;
                }
            }
        } else {
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

    fn root_identifier_for_expression(&self, expr_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = self.ctx.arena.skip_parenthesized(expr_idx);
        loop {
            let node = self.ctx.arena.get(current)?;
            if node.kind == SyntaxKind::Identifier as u16 {
                return Some(current);
            }

            if (node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                && let Some(access) = self.ctx.arena.get_access_expr(node)
            {
                current = self.ctx.arena.skip_parenthesized(access.expression);
                continue;
            }

            return None;
        }
    }

    /// Report an excess property error using solver diagnostics with source tracking.
    pub fn error_excess_property_at(&mut self, prop_name: &str, target: TypeId, idx: NodeIndex) {
        // Honor removed-but-still-effective suppressExcessPropertyErrors flag
        if self.ctx.compiler_options.suppress_excess_property_errors {
            return;
        }
        // Suppress cascade errors from unresolved types
        if target == TypeId::ERROR || target == TypeId::ANY || target == TypeId::UNKNOWN {
            return;
        }
        if self.should_suppress_excess_property_for_target(target) {
            return;
        }

        let (code, message) = self.excess_property_diagnostic_message(prop_name, target, idx);
        // Drill into the source expression to anchor the diagnostic at the
        // offending property name token (tsc underlines `b` in
        // `{ a: '', b: 123 }`, not `{` of the containing literal or the
        // enclosing `||`/`? :` expression).
        let prop_atom = self.ctx.types.intern_string(prop_name);
        if let Some((start, length)) = self.find_excess_property_anchor(idx, prop_atom) {
            self.error(start, length, message, code);
            return;
        }
        self.emit_render_request(
            idx,
            DiagnosticRenderRequest::simple(DiagnosticAnchorKind::PropertyToken, code, message),
        );
    }

    pub fn error_excess_property_at_no_suggestion(
        &mut self,
        prop_name: &str,
        target: TypeId,
        idx: NodeIndex,
    ) {
        if self.ctx.compiler_options.suppress_excess_property_errors {
            return;
        }
        if target == TypeId::ERROR || target == TypeId::ANY || target == TypeId::UNKNOWN {
            return;
        }
        if self.should_suppress_excess_property_for_target(target) {
            return;
        }

        let prop_display = tsz_solver::format_excess_property_name(prop_name);
        let type_str = self.excess_property_target_display_for_site(target, idx);
        let message = format!(
            "Object literal may only specify known properties, and '{prop_display}' does not exist in type '{type_str}'."
        );
        self.emit_render_request(
            idx,
            DiagnosticRenderRequest::simple(
                DiagnosticAnchorKind::PropertyToken,
                diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE,
                message,
            ),
        );
    }

    /// Report a "Cannot assign to readonly property" error using solver diagnostics with source tracking.
    pub fn error_readonly_property_at(&mut self, prop_name: &str, idx: NodeIndex) {
        if let Some(anchor) =
            self.resolve_diagnostic_anchor(idx, DiagnosticAnchorKind::PropertyToken)
        {
            let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
                self.ctx.types,
                &self.ctx.binder.symbols,
                self.ctx.file_name.as_str(),
            )
            .with_def_store(&self.ctx.definition_store);
            let diag = builder.readonly_property(prop_name, anchor.start, anchor.length);
            self.ctx
                .diagnostics
                .push(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Report TS2542: Index signature in type '{0}' only permits reading.
    pub fn error_readonly_index_signature_at(
        &mut self,
        object_type: tsz_solver::TypeId,
        idx: NodeIndex,
    ) {
        let type_name = self.format_type_diagnostic(object_type);
        self.error_at_node_msg(
            idx,
            diagnostic_codes::INDEX_SIGNATURE_IN_TYPE_ONLY_PERMITS_READING,
            &[&type_name],
        );
    }

    /// Report TS2704: The operand of a 'delete' operator cannot be a read-only property.
    pub fn error_delete_readonly_property_at(&mut self, idx: NodeIndex) {
        self.error_at_node_msg(
            idx,
            diagnostic_codes::THE_OPERAND_OF_A_DELETE_OPERATOR_CANNOT_BE_A_READ_ONLY_PROPERTY,
            &[],
        );
    }

    /// Report TS2862: Type '{0}' is generic and can only be indexed for reading.
    pub fn error_generic_only_indexed_for_reading(
        &mut self,
        object_type: tsz_solver::TypeId,
        idx: NodeIndex,
    ) {
        let type_name = self.format_type_diagnostic(object_type);
        self.error_at_node_msg(
            idx,
            diagnostic_codes::TYPE_IS_GENERIC_AND_CAN_ONLY_BE_INDEXED_FOR_READING,
            &[&type_name],
        );
    }

    /// Report TS2803: Cannot assign to private method. Private methods are not writable.
    pub fn error_private_method_not_writable(&mut self, prop_name: &str, idx: NodeIndex) {
        self.error_at_node_msg(
            idx,
            diagnostic_codes::CANNOT_ASSIGN_TO_PRIVATE_METHOD_PRIVATE_METHODS_ARE_NOT_WRITABLE,
            &[prop_name],
        );
    }

    /// Report no index signature error.
    ///
    /// `expr_idx` is the element access expression node (for TS7053 error span).
    /// `arg_idx` is the argument/index node inside brackets (for TS2551 "did you mean" span).
    /// tsc reports TS7053 at the full expression, but TS2551 at the argument.
    pub(crate) fn error_no_index_signature_at(
        &mut self,
        index_type: TypeId,
        object_type: TypeId,
        expr_idx: NodeIndex,
        arg_idx: NodeIndex,
        prefer_write_method: bool,
    ) {
        let prefer_write_method =
            prefer_write_method || self.is_element_access_write_like(expr_idx);
        // Note: suppressImplicitAnyIndexErrors was removed in TypeScript 6.0.
        // tsc now emits TS5102 warning and still reports the errors.
        // TS7053 is a noImplicitAny error - suppress without it
        if !self.ctx.no_implicit_any() {
            return;
        }
        // Suppress when types are unresolved (but NOT for `any` — tsc reports
        // TS7053 when `any` is used to index a type without an index signature
        // under noImplicitAny, e.g., `emptyObj[hi]` where `hi: any`).
        if index_type == TypeId::ERROR || index_type == TypeId::UNKNOWN {
            return;
        }
        if object_type == TypeId::ANY
            || object_type == TypeId::ERROR
            || object_type == TypeId::NEVER
        {
            return;
        }
        if self.is_element_access_on_this_or_super_with_any_base(expr_idx) {
            return;
        }

        if self
            .ctx
            .arena
            .get(expr_idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION)
            && let Some(atom) =
                crate::query_boundaries::common::string_literal_value(self.ctx.types, index_type)
        {
            let prop_name = self.ctx.types.resolve_atom_ref(atom).to_string();
            self.error_property_not_exist_at(&prop_name, object_type, arg_idx);
            return;
        }

        // For literal indices on simple (non-union/non-intersection) types, emit
        // TS2339 ("Property X does not exist") instead of TS7053. tsc uses TS2339
        // for literal element access keys on simple types like `{}`, but uses
        // TS7053 for unions with partial index signature presence.
        //
        // Also handle union index types where at least one member is a string
        // literal that doesn't exist on the object type. In this case, emit
        // TS2339 for the first missing property (matching tsc behavior).
        let is_union_or_intersection =
            crate::query_boundaries::common::union_members(self.ctx.types, object_type).is_some()
                || crate::query_boundaries::common::intersection_members(
                    self.ctx.types,
                    object_type,
                )
                .is_some();
        // Check if the object has any index signature. If so, the more specific
        // TS7015/TS7053 diagnostics below should handle the error, not TS2339.
        let idx_resolver =
            tsz_solver::objects::index_signatures::IndexSignatureResolver::new(self.ctx.types);
        let has_any_index_signature = idx_resolver.resolve_string_index(object_type).is_some()
            || idx_resolver.resolve_number_index(object_type).is_some();

        // Helper closure to emit TS2339 for a missing property
        let emit_ts2339_for_missing_prop =
            |prop_name_str: &str, object_type: TypeId, expr_idx: NodeIndex, checker: &mut Self| {
                let object_str =
                    if checker.is_object_literal_backed_element_access_receiver(expr_idx) {
                        checker.format_type_for_assignability_message(object_type)
                    } else {
                        checker.property_receiver_display_for_node(object_type, expr_idx)
                    };
                let message =
                    format!("Property '{prop_name_str}' does not exist on type '{object_str}'.");
                checker.error_at_anchor(
                    expr_idx,
                    DiagnosticAnchorKind::ElementAccessExpr,
                    &message,
                    diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                );
            };

        // Check if index is a string literal
        if let Some(atom) =
            crate::query_boundaries::common::string_literal_value(self.ctx.types, index_type)
        {
            let prop_name = self.ctx.types.resolve_atom_ref(atom);
            let prop_name_str: &str = &prop_name;
            let suppress_did_you_mean =
                self.has_syntax_parse_errors() || self.class_extends_any_base(object_type);

            let suggestion = if suppress_did_you_mean {
                None
            } else {
                self.find_similar_property(prop_name_str, object_type)
            };

            if suggestion.is_some() {
                // If there's a suggestion, TypeScript emits TS2551 instead of TS7053.
                // TS2551 is reported at the argument node (e.g., "foo" in i["foo"]).
                self.error_property_not_exist_at(prop_name_str, object_type, arg_idx);
                return;
            }

            // For non-union types without index signatures, generally fall
            // through to TS7053. tsc emits TS7053 for element access with
            // literal keys on types without matching properties.
            //
            // Exception: when the receiver is an object literal expression
            // (e.g., `{}["hi"]`), tsc emits TS2339 instead of TS7053.
            // Named types like `interface Empty {}` get TS7053.
            if !is_union_or_intersection
                && !has_any_index_signature
                && !prefer_write_method
                && self.is_object_literal_backed_element_access_receiver(expr_idx)
            {
                emit_ts2339_for_missing_prop(prop_name_str, object_type, expr_idx, self);
                return;
            }
        }
        // Check if index is a union of string literals (e.g., 'a' | 'b' | 'z')
        // If so and the receiver is an object literal, emit TS2339 for the first
        // missing property instead of TS7053.
        else if !is_union_or_intersection
            && !has_any_index_signature
            && !prefer_write_method
            && self.is_object_literal_backed_element_access_receiver(expr_idx)
            && let Some(union_members) =
                crate::query_boundaries::common::union_members(self.ctx.types, index_type)
        {
            // Find the first string literal member that doesn't exist as a property
            for member in union_members {
                if let Some(atom) =
                    crate::query_boundaries::common::string_literal_value(self.ctx.types, member)
                {
                    let prop_name = self.ctx.types.resolve_atom_ref(atom);
                    let prop_name_str: &str = &prop_name;

                    // Check if this property exists on the object type
                    let prop_exists = self
                        .resolve_property_access_with_env(object_type, prop_name_str)
                        .is_success();

                    if !prop_exists {
                        // Property doesn't exist - emit TS2339
                        emit_ts2339_for_missing_prop(prop_name_str, object_type, expr_idx, self);
                        return;
                    }
                }
            }
        }

        // For non-union types with number literal indices and no index sigs,
        // generally fall through to TS7053. Exception: object literal expression
        // receivers (e.g., `{}[10]`) get TS2339.
        if !is_union_or_intersection
            && !has_any_index_signature
            && let Some(num) =
                crate::query_boundaries::common::number_literal_value(self.ctx.types, index_type)
            && !prefer_write_method
            && self.is_object_literal_backed_element_access_receiver(expr_idx)
        {
            let prop_name = if num.fract() == 0.0 && num.is_finite() {
                format!("{}", num as i64)
            } else {
                num.to_string()
            };
            let object_str = self
                .object_literal_initializer_display_type_for_receiver(expr_idx)
                .map(|init_type| {
                    self.format_type_diagnostic(self.widen_type_for_display(init_type))
                })
                .unwrap_or_else(|| self.property_receiver_display_for_node(object_type, expr_idx));
            let message = format!("Property '{prop_name}' does not exist on type '{object_str}'.");
            self.error_at_anchor(
                expr_idx,
                DiagnosticAnchorKind::ElementAccessExpr,
                &message,
                diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
            );
            return;
        }

        if let Some(method_suggestion) = self.no_index_signature_method_suggestion(
            object_type,
            index_type,
            expr_idx,
            prefer_write_method,
        ) {
            let display_object_type =
                crate::query_boundaries::common::string_literal_value(self.ctx.types, index_type)
                    .and_then(|atom| {
                        let prop_name = self.ctx.types.resolve_atom_ref(atom);
                        self.fresh_empty_object_member_for_missing_union(object_type, &prop_name)
                    })
                    .or_else(|| {
                        crate::query_boundaries::common::type_parameter_constraint(
                            self.ctx.types,
                            object_type,
                        )
                    })
                    .unwrap_or(object_type);
            let object_str = self
                .object_literal_initializer_display_type_for_receiver(expr_idx)
                .map(|init_type| self.format_type_for_assignability_message(init_type))
                .unwrap_or_else(|| self.format_type_for_assignability_message(display_object_type));
            self.error_at_anchor(
                expr_idx,
                DiagnosticAnchorKind::ElementAccessExpr,
                &format!(
                    "Element implicitly has an 'any' type because type '{object_str}' has no index signature. Did you mean to call '{method_suggestion}'?"
                ),
                diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_TYPE_HAS_NO_INDEX_SIGNATURE_DID_YOU_M,
            );
            return;
        }

        // TS7015: indexed with a non-numeric type when the object has a number index signature.
        // tsc emits the more specific TS7015 ("index expression is not of type 'number'")
        // for arrays, tuples, enums, or any type with a numeric indexer when the index
        // type is not assignable to number.
        //
        // Suppress for for-in variables: `for (var i in arr) { arr[i] }` is a valid
        // pattern — for-in produces string indices that are numeric at runtime.
        // tsc does not emit TS7015 (or TS7053) for for-in variables indexing their
        // iteration target or other arrays.
        let is_for_in_index = self.is_for_in_variable_identifier(arg_idx);
        // For union types, ALL members must have a number index (resolve_number_index uses
        // find_map which is too permissive — it returns Some if any member matches).
        let resolver =
            tsz_solver::objects::index_signatures::IndexSignatureResolver::new(self.ctx.types);
        let has_number_index = if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, object_type)
        {
            members
                .iter()
                .all(|&m| resolver.resolve_number_index(m).is_some())
        } else {
            resolver.resolve_number_index(object_type).is_some()
        };
        if has_number_index
            && !is_for_in_index
            && !self
                .assign_relation_outcome(index_type, TypeId::NUMBER)
                .related
        {
            // tsc reports TS7015 at the index expression (arg_idx), not the full element access.
            self.error_at_anchor(
                arg_idx,
                DiagnosticAnchorKind::ElementIndexArg,
                "Element implicitly has an 'any' type because index expression is not of type 'number'.",
                diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_INDEX_EXPRESSION_IS_NOT_OF_TYPE_NUMBE,
            );
            return;
        }

        // Suppress TS7053 for for-in variables ONLY when the target type has an
        // index signature. For union types, ALL members must have a string index
        // signature — a number index alone is not sufficient because for-in produces
        // string keys and arrays (which only have number index) cannot be string-indexed.
        // For non-union types, either string or number index is acceptable (arrays
        // with for-in string keys are a valid pattern in tsc).
        if is_for_in_index {
            let has_string_index = if let Some(members) =
                crate::query_boundaries::common::union_members(self.ctx.types, object_type)
            {
                // For union types: ALL members must have an explicit string index signature.
                // `resolve_string_index` returns Some for arrays (treating them as string-indexable),
                // but arrays are only numeric-indexed; string keys produce implicit `any` (TS7053).
                // Use `is_element_indexable(m, wants_string=true, wants_number=false)` which
                // correctly returns false for arrays (Array kind only supports wants_number).
                // e.g. `any[] | Record<string, any>`: `any[]` returns false → don't suppress.
                members
                    .iter()
                    .all(|&m| self.is_element_indexable(m, true, false))
            } else {
                resolver.resolve_string_index(object_type).is_some() || has_number_index
            };
            if has_string_index {
                return;
            }
        }

        let index_str = self.format_type_for_assignability_message(index_type);
        // For type parameters, tsc displays the constraint type name in the
        // diagnostic (e.g., "can't be used to index type 'Item'" not "'T'").
        let display_object_type =
            crate::query_boundaries::common::string_literal_value(self.ctx.types, index_type)
                .and_then(|atom| {
                    let prop_name = self.ctx.types.resolve_atom_ref(atom);
                    self.fresh_empty_object_member_for_missing_union(object_type, &prop_name)
                })
                .or_else(|| {
                    crate::query_boundaries::common::type_parameter_constraint(
                        self.ctx.types,
                        object_type,
                    )
                })
                // An unconstrained type parameter has the base constraint
                // `unknown`; tsc displays "type 'unknown'", not the parameter name.
                .unwrap_or_else(|| {
                    if crate::query_boundaries::common::is_type_parameter(
                        self.ctx.types,
                        object_type,
                    ) {
                        TypeId::UNKNOWN
                    } else {
                        object_type
                    }
                });
        let object_str = self
            .object_literal_initializer_display_type_for_receiver(expr_idx)
            .map(|init_type| self.format_type_for_assignability_message(init_type))
            .unwrap_or_else(|| {
                self.property_receiver_display_for_node(display_object_type, expr_idx)
            });
        let message = format!(
            "Element implicitly has an 'any' type because expression of type '{index_str}' can't be used to index type '{object_str}'."
        );

        // TS7053 is reported at the full element access expression.
        self.error_at_anchor(
            expr_idx,
            DiagnosticAnchorKind::ElementAccessExpr,
            &message,
            diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN,
        );
    }

    fn no_index_signature_method_suggestion(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
        expr_idx: NodeIndex,
        prefer_write_method: bool,
    ) -> Option<String> {
        let method_name = if prefer_write_method { "set" } else { "get" };
        if !self.no_index_signature_method_accepts_index(object_type, method_name, index_type) {
            return None;
        }

        let receiver = self.access_receiver_for_diagnostic_node(expr_idx)?;
        if self.is_named_method_suggestion_receiver(receiver)
            && let Some(receiver_text) = self.named_method_suggestion_receiver_text(receiver)
            && !receiver_text.is_empty()
        {
            return Some(format!("{receiver_text}.{method_name}"));
        }

        Some(method_name.to_string())
    }

    fn no_index_signature_method_accepts_index(
        &mut self,
        object_type: TypeId,
        method_name: &str,
        index_type: TypeId,
    ) -> bool {
        let Some(method_type) = (match self
            .resolve_property_access_with_env(object_type, method_name)
        {
            crate::query_boundaries::common::PropertyAccessResult::Success { type_id, .. } => {
                Some(type_id)
            }
            crate::query_boundaries::common::PropertyAccessResult::PossiblyNullOrUndefined {
                property_type: Some(type_id),
                ..
            } => Some(type_id),
            _ => None,
        }) else {
            return false;
        };

        self.callable_accepts_index_argument(method_type, index_type)
    }

    fn callable_accepts_index_argument(
        &mut self,
        callable_type: TypeId,
        index_type: TypeId,
    ) -> bool {
        if let Some(shape) =
            crate::query_boundaries::property_access::function_shape(self.ctx.types, callable_type)
        {
            return self.signature_accepts_index_argument(&shape.params, index_type);
        }

        crate::query_boundaries::property_access::callable_shape(self.ctx.types, callable_type)
            .is_some_and(|shape| {
                shape
                    .call_signatures
                    .iter()
                    .any(|sig| self.signature_accepts_index_argument(&sig.params, index_type))
            })
    }

    fn signature_accepts_index_argument(
        &mut self,
        params: &[tsz_solver::ParamInfo],
        index_type: TypeId,
    ) -> bool {
        let Some(first) = params.first() else {
            return false;
        };

        self.assign_relation_outcome(index_type, first.type_id)
            .related
    }

    fn is_named_method_suggestion_receiver(&self, idx: NodeIndex) -> bool {
        let idx = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        if node.kind == SyntaxKind::Identifier as u16
            || node.kind == SyntaxKind::ThisKeyword as u16
            || node.kind == SyntaxKind::SuperKeyword as u16
        {
            return true;
        }

        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        self.ctx
            .arena
            .get_access_expr(node)
            .is_some_and(|access| self.is_named_method_suggestion_receiver(access.expression))
    }

    fn is_element_access_write_like(&self, expr_idx: NodeIndex) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let Some(ext) = self.ctx.arena.get_extended(expr_idx) else {
            return false;
        };
        let Some(parent_node) = self.ctx.arena.get(ext.parent) else {
            return false;
        };

        if parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(parent_node)
        {
            return binary.left == expr_idx && self.is_assignment_operator(binary.operator_token);
        }

        if (parent_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || parent_node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION)
            && let Some(unary) = self.ctx.arena.get_unary_expr(parent_node)
        {
            return unary.operand == expr_idx
                && (unary.operator == SyntaxKind::PlusPlusToken as u16
                    || unary.operator == SyntaxKind::MinusMinusToken as u16);
        }

        false
    }

    fn named_method_suggestion_receiver_text(&self, idx: NodeIndex) -> Option<String> {
        let idx = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        let node = self.ctx.arena.get(idx)?;

        if node.kind == SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.clone());
        }

        if node.kind == SyntaxKind::ThisKeyword as u16 {
            return Some("this".to_string());
        }

        if node.kind == SyntaxKind::SuperKeyword as u16 {
            return Some("super".to_string());
        }

        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }

        let access = self.ctx.arena.get_access_expr(node)?;
        let left = self.named_method_suggestion_receiver_text(access.expression)?;
        let right_node = self.ctx.arena.get(access.name_or_argument)?;
        let right = self
            .ctx
            .arena
            .get_identifier(right_node)?
            .escaped_text
            .clone();
        Some(format!("{left}.{right}"))
    }

    fn is_object_literal_backed_element_access_receiver(&self, expr_idx: NodeIndex) -> bool {
        let Some(receiver) = self.access_receiver_for_diagnostic_node(expr_idx) else {
            return false;
        };
        self.is_object_literal_backed_receiver(receiver)
    }

    fn is_object_literal_backed_receiver(&self, idx: NodeIndex) -> bool {
        let idx = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return true;
        }

        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        self.ctx
            .arena
            .get_access_expr(node)
            .is_some_and(|access| self.is_object_literal_backed_receiver(access.expression))
    }

    /// Check if the receiver of an element access expression is an object literal
    /// expression (e.g., `{}["hi"]`). Used to distinguish TS2339 vs TS7053 for
    /// literal-keyed element access on types without index signatures.
    fn is_object_literal_element_access_receiver(&self, expr_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if node.kind != tsz_parser::parser::syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return false;
        };
        self.ctx
            .arena
            .get(access.expression)
            .is_some_and(|receiver| {
                receiver.kind == tsz_parser::parser::syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            })
    }

    /// Check if an identifier node refers to a variable declared in a for-in statement.
    fn is_for_in_variable_identifier(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return false;
        }

        // Resolve to symbol, then find the value declaration
        let Some(sym_id) = self
            .ctx
            .binder
            .get_node_symbol(idx)
            .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, idx))
        else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        let decl = symbol.value_declaration;
        if decl.is_none() {
            return false;
        }

        // Check: declaration → parent (VarDeclList) → parent (ForInStatement?)
        let Some(decl_node) = self.ctx.arena.get(decl) else {
            return false;
        };
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return false;
        }
        let Some(vdl_ext) = self.ctx.arena.get_extended(decl) else {
            return false;
        };
        let vdl_idx = vdl_ext.parent;
        if vdl_idx.is_none() {
            return false;
        }
        let Some(for_ext) = self.ctx.arena.get_extended(vdl_idx) else {
            return false;
        };
        let for_idx = for_ext.parent;
        if for_idx.is_none() {
            return false;
        }
        let Some(for_node) = self.ctx.arena.get(for_idx) else {
            return false;
        };
        for_node.kind == syntax_kind_ext::FOR_IN_STATEMENT
    }

    /// TypeScript suppresses TS7053 for `this[...]`/`super[...]` when the class extends an `any` base.
    fn is_element_access_on_this_or_super_with_any_base(&mut self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        // idx may be the element access expression itself or its argument node.
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        let access = if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            // idx IS the element access expression
            self.ctx.arena.get_access_expr(node)
        } else {
            // idx is the argument — find parent element access
            let Some(ext) = self.ctx.arena.get_extended(idx) else {
                return false;
            };
            let Some(parent) = self.ctx.arena.get(ext.parent) else {
                return false;
            };
            if parent.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
                return false;
            }
            let access = self.ctx.arena.get_access_expr(parent);
            if access.as_ref().is_some_and(|a| a.name_or_argument != idx) {
                return false;
            }
            access
        };

        let Some(access) = access else {
            return false;
        };
        let Some(expr_node) = self.ctx.arena.get(access.expression) else {
            return false;
        };
        let is_this_or_super = expr_node.kind == SyntaxKind::SuperKeyword as u16
            || expr_node.kind == SyntaxKind::ThisKeyword as u16;
        if !is_this_or_super {
            return false;
        }

        let Some(class_info) = self.ctx.enclosing_class.clone() else {
            return false;
        };
        let Some(class_decl) = self.ctx.arena.get_class_at(class_info.class_idx) else {
            return false;
        };
        let Some(heritage_clauses) = &class_decl.heritage_clauses else {
            return false;
        };

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause) = self.ctx.arena.get_heritage_clause_at(clause_idx) else {
                continue;
            };
            if clause.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            let Some(&type_idx) = clause.types.nodes.first() else {
                continue;
            };
            let expr_idx =
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args_at(type_idx) {
                    expr_type_args.expression
                } else {
                    type_idx
                };
            if self.get_type_of_node(expr_idx) == TypeId::ANY {
                return true;
            }
        }

        false
    }

    /// Get the display name for a namespace/module value type, if applicable.
    /// Returns `Some("M")` for `namespace M {}` types, enabling `typeof M` display.
    fn get_namespace_typeof_name(&self, type_id: TypeId) -> Option<String> {
        use crate::query_boundaries::common::{NamespaceMemberKind, classify_namespace_member};
        use tsz_binder::{SymbolId, symbol_flags};

        const fn is_pure_namespace(symbol: &tsz_binder::Symbol) -> bool {
            symbol.has_any_flags(symbol_flags::MODULE)
                && !symbol.has_any_flags(symbol_flags::ENUM)
                && !symbol.has_any_flags(symbol_flags::CLASS)
        }

        let kind = classify_namespace_member(self.ctx.types, type_id);
        let sym_id = match kind {
            NamespaceMemberKind::Lazy(def_id) => self.ctx.def_to_symbol_id(def_id)?,
            NamespaceMemberKind::TypeQuery(sym_ref) => SymbolId(sym_ref.0),
            NamespaceMemberKind::Callable(shape_id) => {
                // Callable with namespace flags (class+namespace merges etc.)
                let shape = self.ctx.types.callable_shape(shape_id);
                shape.symbol?
            }
            _ => return None,
        };

        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        // Keep class+namespace merges on their class-instance display path.
        if is_pure_namespace(symbol) {
            Some(symbol.escaped_name.clone())
        } else {
            None
        }
    }

    /// Check if a type should get TS2812 (suggest 'dom' lib) instead of TS2339.
    /// Returns true if ALL named components of the type match known DOM global names
    /// AND each component is structurally empty (no user-defined members).
    fn should_suggest_dom_lib_for_type(&mut self, type_id: TypeId) -> bool {
        // Check intersection members individually
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)
        {
            if members.is_empty() {
                return false;
            }
            return members
                .iter()
                .all(|&m| self.should_suggest_dom_lib_for_empty_named_type(m));
        }

        self.should_suggest_dom_lib_for_empty_named_type(type_id)
    }

    fn should_suggest_dom_lib_for_empty_named_type(&mut self, type_id: TypeId) -> bool {
        self.is_empty_dom_named_type(type_id) && !self.has_dom_lib_loaded()
    }

    fn has_dom_lib_loaded(&self) -> bool {
        if self.ctx.typescript_dom_replacement_loaded {
            return true;
        }

        self.ctx.lib_contexts.iter().any(|lib_ctx| {
            lib_ctx.arena.source_files.iter().any(|sf| {
                let file_name = sf.file_name.as_str();
                file_name.ends_with("lib.dom.d.ts") || file_name.ends_with("lib.dom.iterable.d.ts")
            })
        })
    }

    /// Check if a single type has a known DOM type name and is structurally empty.
    fn is_empty_dom_named_type(&self, type_id: TypeId) -> bool {
        // Get the type's display name to check against the DOM-element name
        // pattern. We mirror tsc's `containerSeemsToBeEmptyDomElement`, which
        // tests the name against the regex `^(?:EventTarget|Node|(?:HTML[a-zA-Z]*)?Element)$`
        // regardless of whether the declaration originates from a lib file.
        let name = match self.dom_type_name(type_id) {
            Some(n) if is_dom_element_like_name(&n) => n,
            _ => return false,
        };

        // Check if the type is structurally empty (no user-defined properties).
        // Interfaces may be lazy or materialized - check both paths.
        if crate::query_boundaries::common::is_empty_object_type(self.ctx.types, type_id) {
            return true;
        }

        // For lazy types (DefId-backed interfaces), check if the interface
        // declaration has zero members in the AST.
        if let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, type_id)
            .or_else(|| self.ctx.definition_store.find_def_for_type(type_id))
            .or_else(|| {
                self.ctx
                    .resolve_type_to_symbol_id(type_id)
                    .and_then(|sym_id| self.ctx.get_existing_def_id(sym_id))
            })
            && let Some(def) = self.ctx.definition_store.get(def_id)
        {
            let def_name = self.ctx.types.resolve_atom(def.name);
            if def_name == name {
                // Check if the body type is an empty object
                if let Some(body) = def.body
                    && crate::query_boundaries::common::is_empty_object_type(self.ctx.types, body)
                {
                    return true;
                }
                // Check via symbol: if interface has no AST members
                if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) {
                    return self.interface_has_no_members(sym_id);
                }
            }
        }
        false
    }

    /// Try to get the display name for a type, checking symbol and def store.
    fn dom_type_name(&self, type_id: TypeId) -> Option<String> {
        // Try Lazy(DefId) types directly
        if let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, type_id)
            && let Some(def) = self.ctx.definition_store.get(def_id)
        {
            let name = self.ctx.types.resolve_atom(def.name);
            if !name.is_empty() {
                return Some(name);
            }
        }
        // Try object shape symbol
        if let Some(shape_id) =
            crate::query_boundaries::common::object_shape_id(self.ctx.types, type_id)
        {
            let shape = self.ctx.types.object_shape(shape_id);
            if let Some(sym_id) = shape.symbol
                && let Some(symbol) = self.get_cross_file_symbol(sym_id)
            {
                return Some(symbol.escaped_name.clone());
            }
        }
        // Try definition store by type body
        if let Some(def_id) = self
            .ctx
            .definition_store
            .find_def_for_type(type_id)
            .or_else(|| self.ctx.definition_store.find_type_alias_by_body(type_id))
            && let Some(def) = self.ctx.definition_store.get(def_id)
        {
            let name = self.ctx.types.resolve_atom(def.name);
            if !name.is_empty() {
                return Some(name);
            }
        }
        None
    }

    /// Check if an interface symbol's declarations have zero members.
    fn interface_has_no_members(&self, sym_id: tsz_binder::SymbolId) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        for &decl_idx in &symbol.declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                && let Some(iface) = self.ctx.arena.get_interface(node)
                && !iface.members.nodes.is_empty()
            {
                return false;
            }
        }
        true
    }
}
