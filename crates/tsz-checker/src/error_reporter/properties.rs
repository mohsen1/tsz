//! Property-related error reporting (TS2339, TS2741, TS2540, TS7053, TS18046).

use crate::diagnostics::diagnostic_codes;
use crate::error_reporter::fingerprint_policy::{DiagnosticAnchorKind, DiagnosticRenderRequest};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn fresh_empty_object_member_for_missing_union(
        &mut self,
        object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        let members = tsz_solver::type_queries::get_union_members(self.ctx.types, object_type)?;
        let mut saw_present_member = false;
        let mut fresh_empty_member = None;

        for &member in members.iter() {
            if member.is_nullable() {
                continue;
            }

            let evaluated_member = self.evaluate_application_type(member);
            let resolved_member = self.resolve_type_for_property_access(evaluated_member);
            match self.resolve_property_access_with_env(resolved_member, property_name) {
                crate::query_boundaries::common::PropertyAccessResult::Success { .. }
                | crate::query_boundaries::common::PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type: Some(_),
                    ..
                } => {
                    saw_present_member = true;
                }
                crate::query_boundaries::common::PropertyAccessResult::PropertyNotFound { .. } => {
                    if crate::query_boundaries::common::is_empty_object_type(
                        self.ctx.types,
                        resolved_member,
                    ) && crate::query_boundaries::common::is_fresh_object_type(
                        self.ctx.types,
                        resolved_member,
                    ) {
                        fresh_empty_member = Some(resolved_member);
                    }
                }
                crate::query_boundaries::common::PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type: None,
                    ..
                }
                | crate::query_boundaries::common::PropertyAccessResult::IsUnknown => {}
            }
        }

        if saw_present_member {
            fresh_empty_member
        } else {
            None
        }
    }

    fn should_suppress_excess_property_for_target(&mut self, target: TypeId) -> bool {
        [target, self.evaluate_type_for_assignability(target)]
            .into_iter()
            .filter_map(|candidate| {
                crate::query_boundaries::common::intersection_members(self.ctx.types, candidate)
            })
            .any(|members| {
                members.iter().any(|member| {
                    let evaluated_member = self.evaluate_type_for_assignability(*member);
                    tsz_solver::is_primitive_type(self.ctx.types, evaluated_member)
                        || tsz_solver::type_queries::is_type_parameter_like(
                            self.ctx.types,
                            evaluated_member,
                        )
                })
            })
    }

    fn excess_property_target_display_for_site(
        &mut self,
        target: TypeId,
        idx: NodeIndex,
    ) -> String {
        let inferred_display = self.format_excess_property_target_type(target);
        if let Some(annotation_text) = self.excess_property_target_annotation_text_for_site(idx) {
            let annotation_display = self.format_annotation_like_type(&annotation_text);
            if inferred_display.starts_with('{') && annotation_display.contains("object &") {
                return annotation_display;
            }
            if inferred_display.starts_with('{')
                && !annotation_display.contains('|')
                && !annotation_display.contains("object")
                && annotation_display.contains('&')
            {
                return annotation_display;
            }
        }
        inferred_display
    }

    pub(crate) fn excess_property_diagnostic_message(
        &mut self,
        prop_name: &str,
        target: TypeId,
        idx: NodeIndex,
    ) -> (u32, String) {
        let type_str = self.excess_property_target_display_for_site(target, idx);
        let suggestion_target = self.strip_non_object_union_members_for_excess_display(target);
        if !self.has_syntax_parse_errors()
            && let Some(suggestion) = self
                .find_similar_property(prop_name, suggestion_target)
                .or_else(|| self.find_similar_property(prop_name, target))
        {
            return (
                diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_BUT_DOES_NOT_EXIST_IN_TYPE_DID,
                format!(
                    "Object literal may only specify known properties, but '{prop_name}' does not exist in type '{type_str}'. Did you mean to write '{suggestion}'?"
                ),
            );
        }

        (
            diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE,
            format!(
                "Object literal may only specify known properties, and '{prop_name}' does not exist in type '{type_str}'."
            ),
        )
    }

    fn access_receiver_for_diagnostic_node(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(idx)?;
        if (node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            && let Some(access) = self.ctx.arena.get_access_expr(node)
        {
            return Some(
                self.ctx
                    .arena
                    .skip_parenthesized_and_assertions(access.expression),
            );
        }

        self.ctx
            .arena
            .node_info(idx)
            .and_then(|info| self.ctx.arena.get(info.parent))
            .and_then(|parent| self.ctx.arena.get_access_expr(parent))
            .map(|access| {
                self.ctx
                    .arena
                    .skip_parenthesized_and_assertions(access.expression)
            })
    }

    fn js_constructor_receiver_display_for_node(&self, idx: NodeIndex) -> Option<String> {
        if !self.is_js_file() {
            return None;
        }

        let receiver = self.access_receiver_for_diagnostic_node(idx)?;
        let receiver_node = self.ctx.arena.get(receiver)?;
        if receiver_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let sym_id = self.resolve_identifier_symbol(receiver)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl_idx = symbol.value_declaration;
        let decl_node = self.ctx.arena.get(decl_idx)?;
        let decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        let init = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(decl.initializer);
        let init_node = self.ctx.arena.get(init)?;
        if init_node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return None;
        }

        let new_expr = self.ctx.arena.get_call_expr(init_node)?;
        let ctor_expr = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(new_expr.expression);
        let ctor_node = self.ctx.arena.get(ctor_expr)?;

        if ctor_node.kind == SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(ctor_node)
                .map(|ident| ident.escaped_text.clone());
        }

        if ctor_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.ctx.arena.get_access_expr(ctor_node)?;
            let name = self.ctx.arena.get(access.name_or_argument)?;
            return self
                .ctx
                .arena
                .get_identifier(name)
                .map(|ident| ident.escaped_text.clone());
        }

        None
    }

    fn property_receiver_display_for_node(&mut self, type_id: TypeId, idx: NodeIndex) -> String {
        let idx = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        if let Some(name) = self.js_constructor_receiver_display_for_node(idx) {
            return name;
        }
        if self.is_js_file()
            && let Some(receiver) = self.access_receiver_for_diagnostic_node(idx)
            && let Some(receiver_node) = self.ctx.arena.get(receiver)
            && receiver_node.kind == SyntaxKind::Identifier as u16
            && let Some(ident) = self.ctx.arena.get_identifier(receiver_node)
            && let Some(shape) =
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, type_id)
            && shape.symbol.is_none()
            && self
                .resolve_identifier_symbol(receiver)
                .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                .and_then(|symbol| self.ctx.arena.get(symbol.value_declaration))
                .and_then(|decl_node| self.ctx.arena.get_variable_declaration(decl_node))
                .is_some_and(|decl| {
                    self.ctx.arena.get(decl.initializer).is_some_and(|init| {
                        init.kind == tsz_parser::parser::syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    })
                })
        {
            return format!("typeof {}", ident.escaped_text);
        }
        let is_element_access_receiver =
            self.access_receiver_for_diagnostic_node(idx)
                .is_some_and(|expr| {
                    self.ctx
                        .arena
                        .get(expr)
                        .is_some_and(|node| node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                });

        if is_element_access_receiver {
            return self.format_type_diagnostic_structural(type_id);
        }

        self.format_property_receiver_type_for_diagnostic(type_id)
    }

    // =========================================================================
    // Property Errors
    // =========================================================================

    /// Report a property not exist error using solver diagnostics with source tracking.
    /// If a similar property name is found on the type, emits TS2551 ("Did you mean?")
    /// instead of TS2339.
    pub fn error_property_not_exist_at(
        &mut self,
        prop_name: &str,
        type_id: TypeId,
        idx: NodeIndex,
    ) {
        // Suppress error if type is ERROR/ANY or an Error type wrapper.
        // This prevents cascading errors when accessing properties on error types.
        // NOTE: We do NOT suppress for UNKNOWN — accessing properties on unknown should error (TS2339).
        // NOTE: We do NOT suppress for NEVER — tsc emits TS2339 for property access on `never`
        // (e.g., after typeof narrowing exhausts all possibilities).
        if type_id == TypeId::ERROR
            || type_id == TypeId::ANY
            || tsz_solver::is_error_type(self.ctx.types, type_id)
        {
            return;
        }

        // Suppress cascaded TS2339 from failed generic inference when the receiver
        // remains a union that still contains unresolved type parameters.
        // This keeps follow-on property errors from obscuring the primary root cause
        // (typically assignability/inference diagnostics).
        //
        // Only suppress when a DIRECT union member is a type parameter (e.g., T | Foo).
        // Do NOT suppress when type parameters are deeply nested inside object types
        // (e.g., string | MyInterface where MyInterface has generic base types).
        // The deep nesting case occurs with concrete unions like `string | MyArr`
        // where MyArr extends Array<string> -- the resolved object shape may contain
        // type parameters from the generic base, but the union itself is concrete.
        // NOTE: In tsc 6.0, unconstrained type parameters in unions DO trigger
        // TS2339 when the property doesn't exist on the type parameter member.
        // We no longer suppress TS2339 for unions with type parameters.

        // When a class extends `any`, tsc treats unknown member accesses as `any`
        // and does not emit TS2339. Check this before computing source location
        // to avoid unnecessary work.
        if self.class_extends_any_base(type_id) {
            return;
        }

        if self
            .resolve_diagnostic_anchor(idx, DiagnosticAnchorKind::PropertyToken)
            .is_some()
        {
            // TS2550: Check if property exists in a newer lib version before
            // trying spelling suggestions. This matches tsc's priority order.
            if !self.has_syntax_parse_errors()
                && let Some((lib_name, override_type_name)) =
                    self.get_lib_suggestion_for_property_with_node(prop_name, type_id, idx)
            {
                let type_str = if let Some(name) = override_type_name {
                    name.to_string()
                } else {
                    self.property_receiver_display_for_node(type_id, idx)
                };
                let message = format!(
                    "Property '{prop_name}' does not exist on type '{type_str}'. Do you need to change your target library? Try changing the 'lib' compiler option to '{lib_name}' or later."
                );
                self.error_at_anchor(
                    idx,
                    DiagnosticAnchorKind::PropertyToken,
                    &message,
                    diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CH,
                );
                return;
            }

            // On files with syntax parse errors, TypeScript generally avoids TS2551
            // suggestion diagnostics and sticks with TS2339 to reduce cascades.
            let suggestion = if self.has_syntax_parse_errors() {
                None
            } else {
                self.find_similar_property(prop_name, type_id)
            };

            // For namespace types, override the type display to match TSC's
            // `typeof import("module")` format instead of the literal object shape.
            if let Some(module_name) = self.ctx.namespace_module_names.get(&type_id).cloned() {
                if let Some(members) =
                    crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)
                    && let Some(display_member) = members.into_iter().find(|&member| {
                        !self.ctx.namespace_module_names.contains_key(&member)
                            && !self.commonjs_direct_export_supports_named_exports(member)
                    })
                {
                    let type_str = self.format_type(display_member);
                    let (code, message) = if let Some(ref suggestion) = suggestion {
                        (
                            diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN,
                            format!(
                                "Property '{prop_name}' does not exist on type '{type_str}'. Did you mean '{suggestion}'?"
                            ),
                        )
                    } else {
                        (
                            diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                            format!("Property '{prop_name}' does not exist on type '{type_str}'."),
                        )
                    };
                    self.error_at_anchor(idx, DiagnosticAnchorKind::PropertyToken, &message, code);
                    return;
                }

                // Normalize module specifier: TSC displays resolved module names
                // without the relative path prefix (e.g., "./b" → "b").
                let display_name = module_name.strip_prefix("./").unwrap_or(&module_name);
                let type_str = format!("typeof import(\"{display_name}\")");
                let (code, message) = if let Some(ref suggestion) = suggestion {
                    (
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN,
                        format!(
                            "Property '{prop_name}' does not exist on type '{type_str}'. Did you mean '{suggestion}'?"
                        ),
                    )
                } else {
                    (
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                        format!("Property '{prop_name}' does not exist on type '{type_str}'."),
                    )
                };
                self.error_at_anchor(idx, DiagnosticAnchorKind::PropertyToken, &message, code);
                return;
            }

            // For enum container types (e.g., `U8.nonExistent`), tsc displays
            // "typeof EnumName" for the type in the error message.
            if let Some(def_id) = tsz_solver::type_queries::get_enum_def_id(self.ctx.types, type_id)
                && let Some(sym_id) = self.ctx.def_to_symbol_id(def_id)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            {
                let enum_name = &symbol.escaped_name;
                let type_str = format!("typeof {enum_name}");
                let (code, message) = if let Some(ref suggestion) = suggestion {
                    (
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN,
                        format!(
                            "Property '{prop_name}' does not exist on type '{type_str}'. Did you mean '{suggestion}'?"
                        ),
                    )
                } else {
                    (
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                        format!("Property '{prop_name}' does not exist on type '{type_str}'."),
                    )
                };
                self.error_at_anchor(idx, DiagnosticAnchorKind::PropertyToken, &message, code);
                return;
            }

            // For namespace/module value types (e.g., `namespace M { ... }`), tsc displays
            // "typeof NamespaceName" for the type in the error message.
            if let Some(name) = self.get_namespace_typeof_name(type_id) {
                let type_str = format!("typeof {name}");
                let (code, message) = if let Some(ref suggestion) = suggestion {
                    (
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN,
                        format!(
                            "Property '{prop_name}' does not exist on type '{type_str}'. Did you mean '{suggestion}'?"
                        ),
                    )
                } else {
                    (
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                        format!("Property '{prop_name}' does not exist on type '{type_str}'."),
                    )
                };
                self.error_at_anchor(idx, DiagnosticAnchorKind::PropertyToken, &message, code);
                return;
            }

            // TS2812: If the type name matches a known DOM global and the type is
            // structurally empty, suggest including the 'dom' lib option.
            if suggestion.is_none() && self.should_suggest_dom_lib_for_type(type_id) {
                let type_display = self.property_receiver_display_for_node(type_id, idx);
                let message = format!(
                    "Property '{prop_name}' does not exist on type '{type_display}'. Try changing the 'lib' compiler option to include 'dom'."
                );
                self.error_at_anchor(
                    idx,
                    DiagnosticAnchorKind::PropertyToken,
                    &message,
                    diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_TRY_CHANGING_THE_LIB_COMPILER_OPTION_TO_INCLUDE,
                );
                return;
            }

            let type_display = self.property_receiver_display_for_node(type_id, idx);
            let (code, message) = if let Some(ref suggestion) = suggestion {
                (
                    diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN,
                    format!(
                        "Property '{prop_name}' does not exist on type '{type_display}'. Did you mean '{suggestion}'?"
                    ),
                )
            } else {
                (
                    diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                    format!("Property '{prop_name}' does not exist on type '{type_display}'."),
                )
            };
            self.error_at_anchor(idx, DiagnosticAnchorKind::PropertyToken, &message, code);
        }
    }

    /// Report TS2339 with an explicit type display string instead of formatting from TypeId.
    /// Used when the apparent type should be displayed (e.g., `object` → `{}` in destructuring).
    pub fn error_property_not_exist_with_apparent_type(
        &mut self,
        prop_name: &str,
        type_display: &str,
        idx: NodeIndex,
    ) {
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
        if (symbol.flags & symbol_flags::ALIAS) != 0 {
            let mut visited = Vec::new();
            if let Some(resolved_sym_id) = self.resolve_alias_symbol(sym_id, &mut visited)
                && let Some(resolved_symbol) = self.ctx.binder.get_symbol(resolved_sym_id)
            {
                let resolved_is_namespace_import = resolved_symbol.import_module.is_some()
                    && (resolved_symbol.import_name.is_none()
                        || resolved_symbol.import_name.as_deref() == Some("*"));
                if resolved_is_namespace_import {
                    return true;
                }
                if (resolved_symbol.flags & symbol_flags::MODULE) != 0 {
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
        self.emit_render_request(
            idx,
            DiagnosticRenderRequest::simple(DiagnosticAnchorKind::PropertyToken, code, message),
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
    ) {
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
            || object_type == TypeId::UNKNOWN
            || object_type == TypeId::NEVER
        {
            return;
        }
        if self.is_element_access_on_this_or_super_with_any_base(expr_idx) {
            return;
        }

        // For literal indices on simple (non-union/non-intersection) types, emit
        // TS2339 ("Property X does not exist") instead of TS7053. tsc uses TS2339
        // for literal element access keys on simple types like `{}`, but uses
        // TS7053 for unions with partial index signature presence.
        let is_union_or_intersection =
            tsz_solver::type_queries::get_union_members(self.ctx.types, object_type).is_some()
                || tsz_solver::type_queries::get_intersection_members(self.ctx.types, object_type)
                    .is_some();
        // Check if the object has any index signature. If so, the more specific
        // TS7015/TS7053 diagnostics below should handle the error, not TS2339.
        let idx_resolver =
            tsz_solver::objects::index_signatures::IndexSignatureResolver::new(self.ctx.types);
        let has_any_index_signature = idx_resolver.resolve_string_index(object_type).is_some()
            || idx_resolver.resolve_number_index(object_type).is_some();

        if let Some(atom) =
            tsz_solver::type_queries::get_string_literal_value(self.ctx.types, index_type)
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
                && self.is_object_literal_element_access_receiver(expr_idx)
            {
                let object_str = self.property_receiver_display_for_node(object_type, expr_idx);
                let message =
                    format!("Property '{prop_name_str}' does not exist on type '{object_str}'.");
                self.error_at_anchor(
                    expr_idx,
                    DiagnosticAnchorKind::ElementAccessExpr,
                    &message,
                    diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                );
                return;
            }
        }

        // For non-union types with number literal indices and no index sigs,
        // generally fall through to TS7053. Exception: object literal expression
        // receivers (e.g., `{}[10]`) get TS2339.
        if !is_union_or_intersection
            && !has_any_index_signature
            && let Some(num) =
                tsz_solver::type_queries::get_number_literal_value(self.ctx.types, index_type)
            && self.is_object_literal_element_access_receiver(expr_idx)
        {
            let prop_name = if num.fract() == 0.0 && num.is_finite() {
                format!("{}", num as i64)
            } else {
                num.to_string()
            };
            let object_str = self.property_receiver_display_for_node(object_type, expr_idx);
            let message = format!("Property '{prop_name}' does not exist on type '{object_str}'.");
            self.error_at_anchor(
                expr_idx,
                DiagnosticAnchorKind::ElementAccessExpr,
                &message,
                diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
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
            tsz_solver::type_queries::get_union_members(self.ctx.types, object_type)
        {
            members
                .iter()
                .all(|&m| resolver.resolve_number_index(m).is_some())
        } else {
            resolver.resolve_number_index(object_type).is_some()
        };
        if has_number_index
            && !is_for_in_index
            && !self.ctx.types.is_assignable_to(index_type, TypeId::NUMBER)
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
                tsz_solver::type_queries::get_union_members(self.ctx.types, object_type)
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

        let mut formatter = self.ctx.create_type_formatter();
        let index_str = formatter.format(index_type);
        // For type parameters, tsc displays the constraint type name in the
        // diagnostic (e.g., "can't be used to index type 'Item'" not "'T'").
        let display_object_type =
            tsz_solver::type_queries::get_string_literal_value(self.ctx.types, index_type)
                .and_then(|atom| {
                    let prop_name = self.ctx.types.resolve_atom_ref(atom);
                    self.fresh_empty_object_member_for_missing_union(object_type, &prop_name)
                })
                .or_else(|| {
                    tsz_solver::type_queries::get_type_parameter_constraint(
                        self.ctx.types,
                        object_type,
                    )
                })
                .unwrap_or(object_type);
        let object_str = self.property_receiver_display_for_node(display_object_type, expr_idx);
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
        use tsz_binder::{SymbolId, symbol_flags};
        use tsz_solver::type_queries::{NamespaceMemberKind, classify_namespace_member};

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
        // Only namespace/module types (not enums, which are handled separately)
        if (symbol.flags & symbol_flags::MODULE) != 0 && (symbol.flags & symbol_flags::ENUM) == 0 {
            Some(symbol.escaped_name.clone())
        } else {
            None
        }
    }

    /// Check if a type should get TS2812 (suggest 'dom' lib) instead of TS2339.
    /// Returns true if ALL named components of the type match known DOM global names
    /// AND each component is structurally empty (no user-defined members).
    fn should_suggest_dom_lib_for_type(&self, type_id: TypeId) -> bool {
        // Check intersection members individually
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)
        {
            if members.is_empty() {
                return false;
            }
            return members.iter().all(|&m| self.is_empty_dom_named_type(m));
        }

        self.is_empty_dom_named_type(type_id)
    }

    /// Check if a single type has a known DOM type name and is structurally empty.
    fn is_empty_dom_named_type(&self, type_id: TypeId) -> bool {
        use crate::error_reporter::is_known_dom_global;

        // Get the type's display name to check against known DOM types.
        let name = self.dom_type_name(type_id);
        let name = match name {
            Some(ref n) if is_known_dom_global(n) => n.clone(),
            _ => return false,
        };

        // Check if the type is structurally empty (no user-defined properties).
        // Interfaces may be lazy or materialized - check both paths.
        if tsz_solver::is_empty_object_type(self.ctx.types, type_id) {
            return true;
        }

        // For lazy types (DefId-backed interfaces), check if the interface
        // declaration has zero members in the AST.
        if let Some(def_id) = tsz_solver::lazy_def_id(self.ctx.types, type_id)
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
                    && tsz_solver::is_empty_object_type(self.ctx.types, body)
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
        if let Some(def_id) = tsz_solver::lazy_def_id(self.ctx.types, type_id)
            && let Some(def) = self.ctx.definition_store.get(def_id)
        {
            let name = self.ctx.types.resolve_atom(def.name);
            if !name.is_empty() {
                return Some(name);
            }
        }
        // Try object shape symbol
        if let Some(shape_id) =
            tsz_solver::type_queries::get_object_shape_id(self.ctx.types, type_id)
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
