//! Property access type resolution, global augmentation property lookup,
//! and expando function pattern detection.

use crate::classes_domain::class_summary::ClassMemberKind;
use crate::context::TypingRequest;
use crate::query_boundaries::common::PropertyAccessResult;
use crate::query_boundaries::property_access as access_query;
use crate::state::{CheckerState, MAX_INSTANTIATION_DEPTH};
use tsz_binder::symbol_flags;
use tsz_common::common::Visibility;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn current_file_commonjs_module_identifier_is_unshadowed(&self, idx: NodeIndex) -> bool {
        !self
            .resolve_identifier_symbol_without_tracking(idx)
            .is_some_and(|sym_id| {
                self.ctx
                    .binder
                    .get_symbol(sym_id)
                    .is_some_and(|symbol| symbol.decl_file_idx == self.ctx.current_file_idx as u32)
            })
    }

    fn current_file_commonjs_exports_target_is_unshadowed(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(node)
                .is_some_and(|ident| ident.escaped_text == "exports")
                && !self
                    .resolve_identifier_symbol_without_tracking(idx)
                    .is_some_and(|sym_id| {
                        self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                            symbol.decl_file_idx == self.ctx.current_file_idx as u32
                        })
                    });
        }

        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return false;
        };
        self.ctx
            .arena
            .get_identifier_at(access.expression)
            .is_some_and(|ident| ident.escaped_text == "module")
            && self.current_file_commonjs_module_identifier_is_unshadowed(access.expression)
            && self
                .ctx
                .arena
                .get_identifier_at(access.name_or_argument)
                .is_some_and(|ident| ident.escaped_text == "exports")
    }

    pub(crate) fn is_jsdoc_annotated_this_member_declaration(&mut self, idx: NodeIndex) -> bool {
        if !self.is_js_file() {
            return false;
        }

        let mut current = idx;
        for _ in 0..4 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let Some(parent_node) = self.ctx.arena.get(ext.parent) else {
                return false;
            };
            if parent_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT {
                if self.jsdoc_type_annotation_for_node(ext.parent).is_none() {
                    return false;
                }
                let Some(stmt) = self.ctx.arena.get_expression_statement(parent_node) else {
                    return false;
                };
                let Some(expr_node) = self.ctx.arena.get(stmt.expression) else {
                    return false;
                };
                if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    && expr_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                {
                    return false;
                }
                let Some(access) = self.ctx.arena.get_access_expr(expr_node) else {
                    return false;
                };
                let Some(base_node) = self.ctx.arena.get(access.expression) else {
                    return false;
                };
                return base_node.kind == SyntaxKind::ThisKeyword as u16
                    && self.this_has_contextual_owner(access.expression).is_some();
            }
            current = ext.parent;
        }

        false
    }

    fn finalize_property_access_result(
        &self,
        idx: NodeIndex,
        result_type: TypeId,
        skip_flow_narrowing: bool,
        skip_result_flow_for_result: bool,
    ) -> TypeId {
        if skip_flow_narrowing || skip_result_flow_for_result {
            result_type
        } else {
            self.apply_flow_narrowing(idx, result_type)
        }
    }

    pub(crate) fn union_write_requires_existing_named_member(
        &mut self,
        object_type: TypeId,
        property_name: &str,
    ) -> bool {
        let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, object_type)
        else {
            return false;
        };

        let mut saw_present_member = false;
        let mut saw_fresh_empty_missing_member = false;

        for member in members {
            if member.is_nullable() {
                continue;
            }

            let evaluated_member = self.evaluate_application_type(member);
            let resolved_member = self.resolve_type_for_property_access(evaluated_member);
            match self.resolve_property_access_with_env(resolved_member, property_name) {
                PropertyAccessResult::Success { .. }
                | PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type: Some(_),
                    ..
                } => {
                    saw_present_member = true;
                }
                PropertyAccessResult::PropertyNotFound { .. } => {
                    if crate::query_boundaries::common::is_empty_object_type(
                        self.ctx.types,
                        resolved_member,
                    ) && crate::query_boundaries::common::is_fresh_object_type(
                        self.ctx.types,
                        resolved_member,
                    ) {
                        saw_fresh_empty_missing_member = true;
                    } else {
                        return false;
                    }
                }
                PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type: None,
                    ..
                }
                | PropertyAccessResult::IsUnknown => {}
            }
        }

        saw_present_member && saw_fresh_empty_missing_member
    }

    fn recover_property_from_implemented_interfaces(
        &mut self,
        class_idx: NodeIndex,
        property_name: &str,
    ) -> Option<TypeId> {
        let class_node = self.ctx.arena.get(class_idx)?;
        let class = self.ctx.arena.get_class(class_node)?;
        let heritage_clauses = class.heritage_clauses.as_ref()?;

        for &clause_idx in &heritage_clauses.nodes {
            let clause_node = self.ctx.arena.get(clause_idx)?;
            let heritage = self.ctx.arena.get_heritage_clause(clause_node)?;
            if heritage.token != SyntaxKind::ImplementsKeyword as u16 {
                continue;
            }

            for &type_idx in &heritage.types.nodes {
                // Heritage clause types are ExpressionWithTypeArguments nodes.
                // Resolve the symbol from the expression, then get its instance type
                // via type_reference_symbol_type (which returns the instance type for
                // classes, not the constructor type).
                let expr_idx = if let Some(type_node) = self.ctx.arena.get(type_idx)
                    && let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node)
                {
                    expr_type_args.expression
                } else {
                    type_idx
                };

                let Some(sym_id) = self.resolve_heritage_symbol(expr_idx) else {
                    continue;
                };
                let interface_type = self.type_reference_symbol_type(sym_id);
                if interface_type == TypeId::ERROR {
                    continue;
                }
                let interface_type_eval = self.evaluate_application_type(interface_type);
                // Resolve Lazy(DefId) types through the checker's TypeEnvironment so the
                // solver can inspect the interface's actual members. Without this step the
                // solver falls back to TypeId::ANY (its "couldn't resolve" sentinel) which
                // would incorrectly suppress TS2339 for properties that don't exist at all.
                let interface_type_resolved =
                    self.resolve_type_for_property_access(interface_type_eval);
                match self.resolve_property_access_with_env(interface_type_resolved, property_name)
                {
                    PropertyAccessResult::Success { type_id, .. }
                    | PropertyAccessResult::PossiblyNullOrUndefined {
                        property_type: Some(type_id),
                        ..
                    } => {
                        // Don't recover private or protected members from implemented
                        // interfaces. When an interface extends a class with private
                        // members, those members should only be accessible on classes
                        // that actually extend that base class, not on any class that
                        // merely implements the interface.
                        if let Some(shape) = crate::query_boundaries::common::object_shape_for_type(
                            self.ctx.types,
                            interface_type_resolved,
                        ) {
                            let prop_atom = self.ctx.types.intern_string(property_name);
                            if let Some(prop_info) =
                                shape.properties.iter().find(|p| p.name == prop_atom)
                                && prop_info.visibility != Visibility::Public
                            {
                                continue;
                            }
                        }
                        return Some(type_id);
                    }
                    _ => {}
                }
            }
        }

        None
    }

    /// Check if a const enum symbol is "ambient" — declared with `declare` keyword
    /// or originating from a `.d.ts` file. Ambient const enums have no runtime
    /// representation and cannot be accessed under `isolatedModules`.
    fn is_const_enum_ambient(&self, sym: &tsz_binder::Symbol) -> bool {
        // If the file itself is a .d.ts, everything in it is ambient.
        if self.ctx.is_declaration_file() {
            return true;
        }
        // Check if all declarations are in ambient context (e.g., `declare const enum`).
        if sym.declarations.is_empty() {
            return false;
        }
        for &decl_idx in &sym.declarations {
            if !self.ctx.arena.is_in_ambient_context(decl_idx) {
                return false;
            }
        }
        true
    }

    /// Check if a node is in a type-only position (e.g., computed property name
    /// inside a type literal, interface, or type alias). In such positions,
    /// const enum values are resolved at compile time and don't need runtime access.
    fn is_in_type_only_position(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        for _ in 0..20 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };
            // If we hit a type node or type alias/interface declaration, we're in type context
            if parent_node.is_type_node()
                || parent_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                || parent_node.kind == syntax_kind_ext::INTERFACE_DECLARATION
            {
                return true;
            }
            // If we hit a statement, class member, or function-like, we're in value context
            if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                || parent_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
                || parent_node.kind == syntax_kind_ext::RETURN_STATEMENT
                || parent_node.kind == syntax_kind_ext::CALL_EXPRESSION
                || parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            {
                return false;
            }
            current = parent;
        }
        false
    }

    fn resolve_shadowed_global_value_member(
        &mut self,
        expr_idx: NodeIndex,
        property_name: &str,
    ) -> Option<TypeId> {
        let ident = self.ctx.arena.get_identifier_at(expr_idx)?;
        let sym_id = self.resolve_identifier_symbol_without_tracking(expr_idx)?;
        let symbol = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .or_else(|| self.get_cross_file_symbol(sym_id))?;

        let is_namespace = (symbol.flags & symbol_flags::NAMESPACE_MODULE) != 0;
        let value_flags_except_module = symbol_flags::VALUE & !symbol_flags::VALUE_MODULE;
        let has_other_value = (symbol.flags & value_flags_except_module) != 0;
        if !is_namespace || has_other_value {
            return None;
        }

        let is_instantiated = symbol
            .declarations
            .iter()
            .any(|&decl_idx| self.is_namespace_declaration_instantiated(decl_idx));
        if is_instantiated {
            return None;
        }

        let value_type = self.type_of_value_symbol_by_name(&ident.escaped_text);
        if value_type == TypeId::UNKNOWN || value_type == TypeId::ERROR {
            return None;
        }

        match self.resolve_property_access_with_env(value_type, property_name) {
            PropertyAccessResult::Success { type_id, .. }
            | PropertyAccessResult::PossiblyNullOrUndefined {
                property_type: Some(type_id),
                ..
            } => Some(type_id),
            _ => None,
        }
    }

    /// Get type of property access expression.
    #[allow(dead_code)]
    pub(crate) fn get_type_of_property_access(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_property_access_with_request(idx, &TypingRequest::NONE)
    }

    pub(crate) fn get_type_of_property_access_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        if self.ctx.instantiation_depth.get() >= MAX_INSTANTIATION_DEPTH {
            self.ctx.depth_exceeded.set(true);
            return TypeId::ERROR; // Max instantiation depth exceeded - propagate error
        }

        self.ctx
            .instantiation_depth
            .set(self.ctx.instantiation_depth.get() + 1);
        let result = self.get_type_of_property_access_inner(idx, request);
        self.ctx
            .instantiation_depth
            .set(self.ctx.instantiation_depth.get() - 1);
        self.instantiate_callable_result_from_request(idx, result, request)
    }

    fn missing_typescript_lib_dom_global_alias(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        let ident = self.ctx.arena.get_identifier(node)?;
        let name = ident.escaped_text.as_str();
        if !matches!(name, "window" | "self") {
            return None;
        }
        if !self.ctx.typescript_dom_replacement_loaded {
            return None;
        }
        match name {
            "window" if !self.ctx.typescript_dom_replacement_has_window => Some(name.to_string()),
            "self" if !self.ctx.typescript_dom_replacement_has_self => Some(name.to_string()),
            _ => None,
        }
    }

    /// Inner implementation of property access type resolution.
    fn get_type_of_property_access_inner(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        use crate::query_boundaries::common::PropertyAccessResult;
        let skip_flow_narrowing = request.flow.skip_flow_narrowing();

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return TypeId::ERROR; // Missing access expression data - propagate error
        };

        // Handle import.meta: emit TS1470 in files that compile to CommonJS output
        if let Some(expr_node) = self.ctx.arena.get(access.expression)
            && expr_node.kind == SyntaxKind::ImportKeyword as u16
        {
            if let Some(name_n) = self.ctx.arena.get(access.name_or_argument)
                && let Some(ident) = self.ctx.arena.get_identifier(name_n)
                && ident.escaped_text == "meta"
            {
                self.check_import_meta_in_cjs(idx);
            }
            // import.meta resolves to the global ImportMeta interface;
            // return any as a safe fallback until we resolve that global.
            return TypeId::ANY;
        }

        let factory = self.ctx.types.factory();

        // Get the property name first (needed for abstract property check regardless of object type)
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            // Preserve diagnostics on the base expression (e.g. TS2304 for `missing.`)
            // even when parser recovery could not build a property name node.
            let _ = self.get_type_of_node(access.expression);
            return TypeId::ERROR;
        };
        if let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && ident.escaped_text.is_empty()
        {
            // Preserve diagnostics on the base expression when member name is missing.
            let _ = self.get_type_of_node(access.expression);
            return TypeId::ERROR;
        }

        if let Some(missing_global) =
            self.missing_typescript_lib_dom_global_alias(access.expression)
        {
            self.error_at_node_msg(
                access.expression,
                crate::diagnostics::diagnostic_codes::CANNOT_FIND_NAME,
                &[&missing_global],
            );
            return TypeId::ERROR;
        }

        if self.ctx.checking_computed_property_name.is_some()
            && let Some(base_ident) = self.ctx.arena.get_identifier_at(access.expression)
            && base_ident.escaped_text == "Symbol"
            && let Some(prop_ident) = self.ctx.arena.get_identifier(name_node)
        {
            let symbol_value_type = self.type_of_value_symbol_by_name("Symbol");
            if symbol_value_type != TypeId::UNKNOWN && symbol_value_type != TypeId::ERROR {
                match self
                    .resolve_property_access_with_env(symbol_value_type, &prop_ident.escaped_text)
                {
                    PropertyAccessResult::Success { type_id, .. }
                    | PropertyAccessResult::PossiblyNullOrUndefined {
                        property_type: Some(type_id),
                        ..
                    } => return type_id,
                    _ => {}
                }
            }
        }

        // Check for abstract property access in constructor BEFORE evaluating types (error 2715)
        // This must happen even when `this` has type ANY
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;

            if self.is_this_expression(access.expression)
                && let Some(ref class_info) = self.ctx.enclosing_class.clone()
                && self.ctx.function_depth == 0
                && (class_info.in_constructor || self.is_in_instance_property_initializer(idx))
                && let Some(declaring_class_name) =
                    self.find_abstract_property_declaring_class(class_info.class_idx, property_name)
            {
                self.error_abstract_property_in_constructor(
                    property_name,
                    &declaring_class_name,
                    access.name_or_argument,
                );
            }
        }

        // Fast path for enum/namespace member value access (`E.Member` or `Ns.Member`).
        // This avoids the general property-access pipeline (accessibility checks,
        // type environment classification, etc.) for a very common hot path.
        // For namespaces, this is also critical for correctness: when a namespace
        // exports both an interface and a var with the same name (e.g., `Intl.DateTimeFormat`),
        // the general property-access pipeline may resolve to the interface type instead
        // of the var type, causing false TS2351 "not constructable" errors.
        if let Some(name_ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &name_ident.escaped_text;
            let is_identifier_base = self
                .ctx
                .arena
                .get(access.expression)
                .is_some_and(|expr_node| expr_node.kind == SyntaxKind::Identifier as u16);
            if is_identifier_base
                && let Some(base_sym_id) = self
                    .ctx
                    .binder
                    .resolve_identifier(self.ctx.arena, access.expression)
                && let Some(base_symbol) = self.ctx.binder.get_symbol(base_sym_id)
                && base_symbol.flags & (symbol_flags::ENUM | symbol_flags::VALUE_MODULE) != 0
                && let Some(exports) = base_symbol.exports.as_ref()
                && let Some(member_sym_id) = exports.get(property_name)
                // For namespace members, only use the fast path when the export has
                // value semantics (VARIABLE, CLASS, FUNCTION, etc.) or is an alias
                // (export import). Type-only exports (interfaces, type aliases) must go
                // through the general property-access path so that TS2708/TS2693
                // diagnostics are properly emitted.
                && self.ctx.binder.get_symbol(member_sym_id)
                    .map_or(false, |s| s.flags & (symbol_flags::VALUE | symbol_flags::ALIAS) != 0)
            {
                let is_enum = base_symbol.flags & symbol_flags::ENUM != 0;

                // TS1361/TS1362: Check if the base identifier is a type-only import.
                // resolve_identifier follows aliases, so base_sym_id is the target,
                // not the local import binding. Check the local symbol in file_locals.
                // Applies to both enum and namespace member access.
                if let Some(local_sym_id) = self.resolve_identifier_symbol(access.expression)
                    && self.alias_resolves_to_type_only(local_sym_id)
                {
                    if let Some(base_node) = self.ctx.arena.get(access.expression)
                        && let Some(base_ident) = self.ctx.arena.get_identifier(base_node)
                    {
                        self.report_wrong_meaning_diagnostic(
                            &base_ident.escaped_text,
                            access.expression,
                            crate::query_boundaries::name_resolution::NameLookupKind::Type,
                        );
                    }
                    return TypeId::ERROR;
                }

                if is_enum {
                    // TS2450: Check if enum is used before its declaration (TDZ violation).
                    // Only non-const enums are flagged (const enums are always hoisted).
                    if let Some(base_node) = self.ctx.arena.get(access.expression)
                        && let Some(base_ident) = self.ctx.arena.get_identifier(base_node)
                    {
                        let base_name = &base_ident.escaped_text;
                        if self.check_tdz_violation(base_sym_id, access.expression, base_name, true)
                        {
                            return TypeId::ERROR;
                        }
                    }

                    // TS2748: Cannot access ambient const enums when isolatedModules is enabled.
                    if self.ctx.isolated_modules()
                        && base_symbol.flags & symbol_flags::CONST_ENUM != 0
                        && self.is_const_enum_ambient(base_symbol)
                        && !self.is_in_type_only_position(idx)
                    {
                        let option_name = if self.ctx.compiler_options.verbatim_module_syntax {
                            "verbatimModuleSyntax"
                        } else {
                            "isolatedModules"
                        };
                        let msg = crate::diagnostics::format_message(
                            crate::diagnostics::diagnostic_messages::CANNOT_ACCESS_AMBIENT_CONST_ENUMS_WHEN_IS_ENABLED,
                            &[option_name],
                        );
                        self.error_at_node(
                            idx,
                            &msg,
                            crate::diagnostics::diagnostic_codes::CANNOT_ACCESS_AMBIENT_CONST_ENUMS_WHEN_IS_ENABLED,
                        );
                    }
                }

                // TS2729 for namespace member access in static property initializers:
                // `namespace Ns { export let A = 0 }` compiles to `var` (hoisted),
                // but the IIFE that populates members runs at declaration position.
                // Accessing `Ns.A` before the namespace body executes is a forward
                // reference and tsc emits TS2729 at the property name site.
                if base_symbol.flags & symbol_flags::VALUE_MODULE != 0
                    && self.is_in_static_property_initializer_ast_context(access.expression)
                    && self
                        .find_enclosing_computed_property(access.expression)
                        .is_none()
                {
                    // Check if the namespace declaration is after the usage
                    let decl_idx = if base_symbol.value_declaration.is_some() {
                        base_symbol.value_declaration
                    } else if let Some(&first_decl) = base_symbol.declarations.first() {
                        first_decl
                    } else {
                        NodeIndex::NONE
                    };
                    if decl_idx.is_some()
                        && let Some(usage_node) = self.ctx.arena.get(access.expression)
                        && let Some(decl_node) = self.ctx.arena.get(decl_idx)
                        && usage_node.pos < decl_node.pos
                    {
                        self.error_at_node(
                            access.name_or_argument,
                            &format!(
                                "Property '{}' is used before its initialization.",
                                name_ident.escaped_text
                            ),
                            tsz_common::diagnostics::diagnostic_codes::PROPERTY_IS_USED_BEFORE_ITS_INITIALIZATION,
                        );
                    }
                }

                // Enum members and namespace exports both resolve to the selected member symbol type.
                // Namespace exports may represent functions, variables, etc., each with its own symbol type.
                //
                // For merged symbols (e.g., `interface Foo` + `var Foo: FooConstructor` in a
                // namespace), `get_type_of_symbol` returns the interface type. In value position
                // (property access on namespace), we need the variable's type instead — otherwise
                // `new Ns.Foo()` would fail with TS2351 because the interface has no construct
                // signatures. Use the value declaration path for merged interface+variable symbols.
                let member_sym = self.ctx.binder.get_symbol(member_sym_id);
                let member_type = if let Some(member_sym) = member_sym
                    && member_sym.flags & symbol_flags::INTERFACE != 0
                    && member_sym.flags & symbol_flags::VARIABLE != 0
                    && member_sym.value_declaration.is_some()
                {
                    self.type_of_value_declaration_for_symbol(
                        member_sym_id,
                        member_sym.value_declaration,
                    )
                } else {
                    self.get_type_of_symbol(member_sym_id)
                };
                return self.finalize_property_access_result(
                    idx,
                    member_type,
                    skip_flow_narrowing,
                    false,
                );
            }
        }

        // Get the type of the object.
        // When checking assignment targets (skip_flow_narrowing=true), we still need
        // narrowing on the object expression. E.g., for `target.info.a_count = 3` inside
        // `if (target instanceof A2)`, `target` must narrow to A2 so we can resolve `info`.
        // Only the final property access result should skip narrowing.
        //
        // Hot path optimization: in literal equality comparisons (`obj.prop === "x"`),
        // probing the property on the non-flow object type is often enough. If the
        // property is found without flow narrowing, keep that cheaper object type and
        // avoid an additional flow walk on the object expression.
        let skip_result_flow_for_result =
            !skip_flow_narrowing && self.should_skip_property_result_flow_narrowing_for_result(idx);
        let skip_result_flow = !skip_flow_narrowing
            && (skip_result_flow_for_result
                || self.should_skip_property_result_flow_narrowing(idx));
        let skip_optional_base_flow = access.question_dot_token && skip_result_flow_for_result;

        let (original_object_type, write_presence_only) = if skip_flow_narrowing {
            let object_type_no_flow =
                self.get_type_of_write_target_base_expression(access.expression);

            let property_name_for_probe = self
                .ctx
                .arena
                .get_identifier(name_node)
                .map(|ident| ident.escaped_text.clone());
            let can_use_no_flow = if let Some(property_name) = property_name_for_probe.as_deref() {
                let evaluated_no_flow = self.evaluate_application_type(object_type_no_flow);
                let resolved_no_flow = self.resolve_type_for_property_access(evaluated_no_flow);
                !matches!(
                    self.resolve_property_access_with_env(resolved_no_flow, property_name),
                    PropertyAccessResult::PropertyNotFound { .. } | PropertyAccessResult::IsUnknown
                )
            } else {
                false
            };

            if can_use_no_flow {
                let read_object_type =
                    self.get_type_of_node_with_request(access.expression, &TypingRequest::NONE);
                if let Some(property_name) = property_name_for_probe.as_deref() {
                    let evaluated_read = self.evaluate_application_type(read_object_type);
                    let resolved_read = self.resolve_type_for_property_access(evaluated_read);
                    if self.union_write_requires_existing_named_member(resolved_read, property_name)
                    {
                        (read_object_type, false)
                    } else {
                        let read_has_property = !matches!(
                            self.resolve_property_access_with_env(resolved_read, property_name),
                            PropertyAccessResult::PropertyNotFound { .. }
                                | PropertyAccessResult::IsUnknown
                        );
                        (object_type_no_flow, !read_has_property)
                    }
                } else {
                    (object_type_no_flow, false)
                }
            } else {
                (
                    self.get_type_of_node_with_request(access.expression, &TypingRequest::NONE),
                    false,
                )
            }
        } else if skip_optional_base_flow {
            (
                self.get_type_of_write_target_base_expression(access.expression),
                false,
            )
        } else if skip_result_flow {
            let object_type_no_flow =
                self.get_type_of_write_target_base_expression(access.expression);

            let property_name_for_probe = self
                .ctx
                .arena
                .get_identifier(name_node)
                .map(|ident| ident.escaped_text.clone());
            let can_use_no_flow = if let Some(property_name) = property_name_for_probe.as_deref() {
                let evaluated_no_flow = self.evaluate_application_type(object_type_no_flow);
                let resolved_no_flow = self.resolve_type_for_property_access(evaluated_no_flow);
                !matches!(
                    self.resolve_property_access_with_env(resolved_no_flow, property_name),
                    PropertyAccessResult::PropertyNotFound { .. }
                        | PropertyAccessResult::IsUnknown
                        | PropertyAccessResult::PossiblyNullOrUndefined { .. }
                )
            } else {
                false
            };

            if can_use_no_flow {
                (object_type_no_flow, false)
            } else {
                (
                    self.get_type_of_node_with_request(access.expression, &TypingRequest::NONE),
                    false,
                )
            }
        } else {
            (
                self.get_type_of_node_with_request(access.expression, &TypingRequest::NONE),
                false,
            )
        };

        let effective_write_result = |type_id: TypeId, write_type: Option<TypeId>| -> TypeId {
            if skip_flow_narrowing {
                if write_presence_only {
                    TypeId::ANY
                } else {
                    write_type.unwrap_or(type_id)
                }
            } else {
                type_id
            }
        };

        // Evaluate Application types to resolve generic type aliases/interfaces.
        // But preserve original for error messages to maintain nominal identity (e.g., D<string>).
        //
        // For `obj?.prop ?? fallback`, defer this work: the optional-chain fast path
        // below will resolve property access through `resolve_type_for_property_access`,
        // and eagerly evaluating applications here is redundant on hot paths.
        let mut object_type = if access.question_dot_token && skip_optional_base_flow {
            original_object_type
        } else {
            self.evaluate_application_type(original_object_type)
        };

        // When the object type is `unknown` but the expression is an identifier or
        // property access whose type was not fully resolved (lazy type alias evaluation),
        // re-resolve to trigger deferred Application type expansion. This handles
        // cases where variables declared with generic type alias annotations (e.g.,
        // `type P = Proxy<string>; const ps: P`) or mapped types with Application
        // templates (e.g., `Proxify<Shape>`) have not been fully evaluated when
        // the first property access occurs.
        if object_type == TypeId::UNKNOWN
            && let Some(expr_node) = self.ctx.arena.get(access.expression)
        {
            if expr_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
                if let Some(sym_id) = self.resolve_identifier_symbol(access.expression) {
                    let sym_type = self.get_type_of_symbol(sym_id);
                    if sym_type != TypeId::UNKNOWN && sym_type != TypeId::ERROR {
                        object_type = self.evaluate_application_type(sym_type);
                    }
                }
            } else if self.ctx.arena.get_access_expr(expr_node).is_some() {
                let inner_type = self.get_type_of_property_access_with_request(
                    access.expression,
                    &TypingRequest::NONE,
                );
                if inner_type != TypeId::UNKNOWN && inner_type != TypeId::ERROR {
                    object_type = self.evaluate_application_type(inner_type);
                }
            }
        }

        // Handle optional chain continuations: for `o?.b.c`, when processing `.c`,
        // the object type from `o?.b` includes `undefined` from the optional chain.
        // But `.c` should only be reached when `o` is defined, so we strip nullish
        // types. Only do this when this access is NOT itself an optional chain
        // (`question_dot_token` is false) but is part of one (parent has `?.`).
        object_type = if !access.question_dot_token
            && super::computation::access::is_optional_chain(self.ctx.arena, access.expression)
        {
            let (non_nullish, _) = self.split_nullish_type(object_type);
            non_nullish.unwrap_or(object_type)
        } else {
            object_type
        };
        if !skip_flow_narrowing
            // When TS2454 already forced the receiver read back to its declared type,
            // a second property-read flow pass would incorrectly reapply narrowing
            // and hide follow-on property errors like TS2339.
            && !self.ctx.daa_error_nodes.contains(&access.expression.0)
            && self.ctx.arena.get(access.expression).is_some_and(|expr| {
                matches!(
                    expr.kind,
                    k if k == SyntaxKind::Identifier as u16
                        || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                )
            })
            && let Some(flow_node) = self.flow_node_for_reference_usage(idx)
        {
            object_type = self.flow_analyzer_for_property_reads().get_flow_type(
                access.expression,
                object_type,
                flow_node,
            );
        }

        let mut commonjs_namespace_override: Option<TypeId> = None;
        if object_type == TypeId::ANY
            && self.is_js_file()
            && self
                .ctx
                .arena
                .get_identifier_at(access.expression)
                .is_some_and(|ident| ident.escaped_text == "exports")
            && self
                .resolve_identifier_symbol_without_tracking(access.expression)
                .is_none()
        {
            let namespace_type = self.current_file_commonjs_namespace_type();
            object_type = namespace_type;
            commonjs_namespace_override = Some(namespace_type);
        }

        // Fast path for optional chaining on non-class receivers when the
        // property resolves successfully without diagnostics.
        //
        // This avoids the full property-access diagnostic pipeline for common
        // patterns like `opts?.timeout` / `opts?.retries` in hot call sites.
        if access.question_dot_token
            && !self
                .ctx
                .compiler_options
                .no_property_access_from_index_signature
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && !self.is_super_expression(access.expression)
        {
            let property_name = &ident.escaped_text;

            // TOP-LEVEL CACHE: check the dedicated optional_chain_cache first.
            // This is keyed by (object_type_with_nullish, prop_atom) and stores
            // the FINAL result including undefined union. On cache hit, we skip
            // split_nullish, resolve_type, contains_type_params, property lookup,
            // and union2 — eliminating 4+ RefCell borrows and HashMap lookups.
            // Only used when flow narrowing is skipped (skip_result_flow_for_result),
            // which guarantees the result is context-independent.
            if skip_result_flow_for_result {
                let oc_atom = if ident.atom != tsz_common::interner::Atom::none() {
                    ident.atom
                } else {
                    self.ctx.types.intern_string(property_name)
                };
                if let Some(&cached) = self
                    .ctx
                    .narrowing_cache
                    .optional_chain_cache
                    .borrow()
                    .get(&(object_type, oc_atom))
                {
                    return cached;
                }
            }

            let (non_nullish_base, base_nullish) = self.split_nullish_type(object_type);
            let Some(non_nullish_base) = non_nullish_base else {
                return TypeId::UNDEFINED;
            };

            // Keep class/private/protected semantics on the full path.
            if self
                .resolve_class_for_access(access.expression, non_nullish_base)
                .is_none()
            {
                let resolved_base = self.resolve_type_for_property_access(non_nullish_base);
                let prop_atom = self.ctx.types.intern_string(property_name);

                // property_cache stores Option<TypeId>: Some(id) = resolved type,
                // None = property not found (fall through for TS2339 diagnostics).
                let cached_property_type = self
                    .ctx
                    .narrowing_cache
                    .property_cache
                    .borrow()
                    .get(&(resolved_base, prop_atom))
                    .copied();
                if let Some(Some(type_id)) = cached_property_type {
                    let mut result_type = self.refine_expando_property_read_type(
                        idx,
                        access.expression,
                        property_name,
                        type_id,
                    );
                    if base_nullish.is_some()
                        && !tsz_solver::type_contains_undefined(self.ctx.types, result_type)
                    {
                        result_type = factory.union2(result_type, TypeId::UNDEFINED);
                    }
                    // Store in optional_chain_cache for instant hits next time.
                    if skip_result_flow_for_result {
                        self.ctx
                            .narrowing_cache
                            .optional_chain_cache
                            .borrow_mut()
                            .insert((object_type, prop_atom), result_type);
                    }
                    return self.finalize_property_access_result(
                        idx,
                        result_type,
                        skip_flow_narrowing,
                        skip_result_flow_for_result,
                    );
                }

                let fast_result = self.ctx.types.resolve_property_access_with_options(
                    resolved_base,
                    property_name,
                    self.ctx.compiler_options.no_unchecked_indexed_access,
                );
                let result = self.resolve_property_access_with_env_post_query(
                    resolved_base,
                    property_name,
                    fast_result,
                );
                match result {
                    PropertyAccessResult::Success {
                        type_id,
                        write_type,
                        from_index_signature,
                    } => {
                        if from_index_signature
                            && self
                                .ctx
                                .compiler_options
                                .no_property_access_from_index_signature
                            && !self
                                .union_has_explicit_property_member(resolved_base, property_name)
                        {
                            // Preserve the optional-chain fast path for regular
                            // property reads, but fall back to the full path when
                            // TS4111 must be reported.
                        } else {
                            let refined_type_id = self.refine_expando_property_read_type(
                                idx,
                                access.expression,
                                property_name,
                                type_id,
                            );
                            self.ctx
                                .narrowing_cache
                                .property_cache
                                .borrow_mut()
                                .insert((resolved_base, prop_atom), Some(refined_type_id));
                            let mut result_type =
                                effective_write_result(refined_type_id, write_type);
                            if base_nullish.is_some()
                                && !tsz_solver::type_contains_undefined(self.ctx.types, result_type)
                            {
                                result_type = factory.union2(result_type, TypeId::UNDEFINED);
                            }
                            return self.finalize_property_access_result(
                                idx,
                                result_type,
                                skip_flow_narrowing,
                                skip_result_flow_for_result,
                            );
                        }
                    }
                    PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                        self.ctx
                            .narrowing_cache
                            .property_cache
                            .borrow_mut()
                            .insert((resolved_base, prop_atom), property_type);
                        let mut result_type = property_type.unwrap_or(TypeId::ERROR);
                        if base_nullish.is_some()
                            && !tsz_solver::type_contains_undefined(self.ctx.types, result_type)
                        {
                            result_type = factory.union2(result_type, TypeId::UNDEFINED);
                        }
                        return self.finalize_property_access_result(
                            idx,
                            result_type,
                            skip_flow_narrowing,
                            false,
                        );
                    }
                    PropertyAccessResult::PropertyNotFound { .. } => {
                        self.ctx
                            .narrowing_cache
                            .property_cache
                            .borrow_mut()
                            .insert((resolved_base, prop_atom), None);
                        // Fall through to full diagnostic path.
                    }
                    PropertyAccessResult::IsUnknown => {
                        // Fall through to full diagnostic path.
                    }
                }
            }
        }

        // Deferred display_object_type computation: now that the optional-chain
        // fast path has been exhausted, compute the proper display type for error
        // messages. This preserves literal types that get_type_of_node widens.
        let mut display_object_type = if let Some(ns_type) = commonjs_namespace_override {
            ns_type
        } else if matches!(
            original_object_type,
            TypeId::STRING | TypeId::NUMBER | TypeId::BOOLEAN | TypeId::BIGINT
        ) {
            self.literal_type_from_initializer(access.expression)
                .unwrap_or(original_object_type)
        } else {
            self.enum_member_initializer_display_type(access.expression)
                .unwrap_or(original_object_type)
        };

        if self.ctx.is_js_file()
            && self.ctx.should_resolve_jsdoc()
            && let Some(ident) = self.ctx.arena.get_identifier_at(access.expression)
            && let Some(sym_id) = self.resolve_identifier_symbol_without_tracking(access.expression)
            && let Some(preferred_type) =
                self.preferred_non_js_cross_file_global_value_type(&ident.escaped_text, sym_id)
        {
            display_object_type = preferred_type;
        }

        if name_node.kind == SyntaxKind::PrivateIdentifier as u16 {
            return self.get_type_of_private_property_access(
                idx,
                access,
                access.name_or_argument,
                object_type,
                skip_flow_narrowing,
            );
        }

        let commonjs_named_props_disallowed = self.is_js_file()
            && self.is_current_file_commonjs_export_base(access.expression)
            && self
                .resolve_js_export_surface(self.ctx.current_file_idx)
                .direct_export_type
                .is_some_and(|direct_export_type| {
                    !crate::query_boundaries::js_exports::commonjs_direct_export_supports_named_props(
                        self.ctx.types,
                        direct_export_type,
                    )
                });

        let is_this_access = self.js_object_expr_is_this_or_alias(access.expression);
        let static_member_name = self
            .ctx
            .arena
            .get_identifier(name_node)
            .map(|ident| ident.escaped_text.clone())
            .or_else(|| self.current_file_commonjs_static_member_name(access.name_or_argument));

        if self.is_js_file()
            && is_this_access
            && !self.property_access_is_direct_write_target(idx)
            && let Some(member_name) = static_member_name.as_deref()
            && let Some(prior_type) = self.prior_js_this_property_assignment_type(idx, member_name)
        {
            return prior_type;
        }

        let mut js_expando_before_assignment = false;
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;
            if !commonjs_named_props_disallowed {
                js_expando_before_assignment = self.expando_property_read_before_assignment(
                    idx,
                    access.expression,
                    property_name,
                );
            }
            if js_expando_before_assignment {
                use crate::diagnostics::format_message;
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    access.name_or_argument,
                    &format_message(
                        diagnostic_messages::PROPERTY_IS_USED_BEFORE_BEING_ASSIGNED,
                        &[property_name],
                    ),
                    diagnostic_codes::PROPERTY_IS_USED_BEFORE_BEING_ASSIGNED,
                );
            }
            let is_this_global = self.is_this_resolving_to_global(access.expression);
            if self.is_global_this_like_expression(access.expression) || is_this_global {
                let base_display =
                    if self.is_global_this_expression(access.expression) || is_this_global {
                        "typeof globalThis"
                    } else {
                        "Window & typeof globalThis"
                    };
                let allow_unknown_property_fallback =
                    self.is_global_this_expression(access.expression) || is_this_global;
                let property_type = self.resolve_global_this_property_type(
                    property_name,
                    access.name_or_argument,
                    allow_unknown_property_fallback,
                    base_display,
                );
                if property_type == TypeId::ERROR {
                    return TypeId::ERROR;
                }
                // TS7017: When noImplicitAny is enabled and `this` (not the `globalThis`
                // identifier) resolves to typeof globalThis and the property is not found,
                // emit "Element implicitly has an 'any' type because type 'typeof
                // globalThis' has no index signature." — matching tsc behavior for dot
                // access. Only for `this` — the `globalThis` identifier path uses the
                // global property resolver which may return ANY for unresolved properties
                // that do exist in lib declarations.
                if is_this_global
                    && property_type == TypeId::ANY
                    && self.ctx.no_implicit_any()
                    && !self.is_js_file()
                {
                    use crate::diagnostics::{
                        diagnostic_codes, diagnostic_messages, format_message,
                    };
                    self.error_at_node(
                        access.name_or_argument,
                        &format_message(
                            diagnostic_messages::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_TYPE_HAS_NO_INDEX_SIGNATURE,
                            &["typeof globalThis"],
                        ),
                        diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_TYPE_HAS_NO_INDEX_SIGNATURE,
                    );
                }
                return self.finalize_property_access_result(
                    idx,
                    property_type,
                    skip_flow_narrowing,
                    false,
                );
            }
        }

        if self.is_js_file()
            && self
                .ctx
                .arena
                .get_identifier_at(access.expression)
                .is_some_and(|ident| ident.escaped_text == "module")
            && self.current_file_commonjs_module_identifier_is_unshadowed(access.expression)
            && self
                .ctx
                .arena
                .get_identifier(name_node)
                .is_some_and(|ident| ident.escaped_text == "exports")
        {
            return self.current_file_commonjs_module_exports_namespace_type();
        }

        if skip_flow_narrowing
            && self.is_js_file()
            && self.property_access_is_direct_write_target(idx)
            && self.current_file_commonjs_exports_target_is_unshadowed(access.expression)
        {
            let surface = self.resolve_js_export_surface(self.ctx.current_file_idx);
            let can_add_named_props = surface.direct_export_type.is_none_or(|direct_export_type| {
                crate::query_boundaries::js_exports::commonjs_direct_export_supports_named_props(
                    self.ctx.types,
                    direct_export_type,
                )
            });
            if can_add_named_props {
                return TypeId::ANY;
            }
        }

        if skip_flow_narrowing
            && self.is_js_file()
            && self.property_access_is_direct_write_target(idx)
            && let Some(base_export_name) =
                self.current_file_commonjs_export_member_name(access.expression)
        {
            let surface = self.resolve_js_export_surface(self.ctx.current_file_idx);
            if let Some(base_type) = surface.lookup_named_export(&base_export_name, self.ctx.types)
                && (tsz_solver::visitor::is_object_like_type(self.ctx.types, base_type)
                    || crate::query_boundaries::common::callable_shape_for_type(
                        self.ctx.types,
                        base_type,
                    )
                    .is_some())
            {
                return TypeId::ANY;
            }
        }

        if self.report_namespace_value_access_for_type_only_import_equals_expr(access.expression) {
            return TypeId::ERROR;
        }

        // Don't report errors for any/error types - check BEFORE accessibility
        // to prevent cascading errors when the object type is already invalid
        if object_type == TypeId::ANY {
            return TypeId::ANY;
        }
        if object_type == TypeId::ERROR {
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        }

        // Property access on `never` emits TS2339 and returns `error` type.
        // In TypeScript, `never` has no properties — accessing any property is an error.
        // Returning `error` (not `never`) matches tsc behavior: when a property doesn't
        // exist, tsc returns `errorType` which suppresses cascading diagnostics (e.g.
        // TS2322 on `ab.y = 'hello'` when `ab: never`).
        if object_type == TypeId::NEVER {
            if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                let property_name = &ident.escaped_text;
                if !property_name.starts_with('#') {
                    // Report at the property name node, not the full expression (matches tsc behavior)
                    self.error_property_not_exist_at(
                        property_name,
                        TypeId::NEVER,
                        access.name_or_argument,
                    );
                }
            }
            return TypeId::ERROR;
        }

        // Enforce private/protected access modifiers when possible.
        // Note: we do NOT return ERROR on failure — the diagnostic is already emitted,
        // and tsc continues resolving the property type so that subsequent expressions
        // on the same line are still checked (e.g., `new A().priv + new A().prot`).
        // When accessibility fails, we suppress subsequent TS2339/TS2551 "not found"
        // errors, since the property *does* exist — it's just not accessible.
        let mut accessibility_error_emitted = false;
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;
            let accessible = self.check_property_accessibility(
                access.expression,
                property_name,
                access.name_or_argument,
                object_type,
            );
            if !accessible {
                accessibility_error_emitted = true;
            }
        }

        // Check for merged class/enum/function + namespace symbols
        // When a class/enum/function merges with a namespace (same name), the symbol has both
        // value constructor flags and MODULE flags. We need to check the symbol's exports.
        // This handles value access like `Foo.value` when Foo is both a class and namespace.
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;

            // For value access to merged symbols, check the exports directly
            // This is needed because the type system doesn't track which symbol a Callable came from
            let base_expr = self.ctx.arena.skip_parenthesized(access.expression);
            if let Some(expr_node) = self.ctx.arena.get(base_expr)
                && let Some(expr_ident) = self.ctx.arena.get_identifier(expr_node)
            {
                let expr_name = &expr_ident.escaped_text;
                // Try file_locals first (fast path for top-level symbols)
                if let Some(sym_id) = self.ctx.binder.file_locals.get(expr_name)
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                {
                    // Check if this is a merged symbol (has both MODULE and value constructor flags)
                    let is_merged = (symbol.flags & symbol_flags::MODULE) != 0
                        && (symbol.flags
                            & (symbol_flags::CLASS
                                | symbol_flags::FUNCTION
                                | symbol_flags::REGULAR_ENUM))
                            != 0;

                    if is_merged
                        && let Some(exports) = symbol.exports.as_ref()
                        && let Some(member_id) = exports.get(property_name)
                    {
                        // For merged symbols, we return the type for any exported member
                        let member_type = self.get_type_of_symbol(member_id);
                        return self.finalize_property_access_result(
                            idx,
                            member_type,
                            skip_flow_narrowing,
                            false,
                        );
                    }
                }
            }
        }

        // If it's an identifier, look up the property
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;

            if self
                .report_namespace_value_access_for_type_only_import_equals_expr(access.expression)
            {
                return TypeId::ERROR;
            }

            let enum_instance_like_access = self
                .is_enum_instance_property_access(object_type, access.expression)
                || access_query::type_parameter_constraint(self.ctx.types, object_type)
                    .is_some_and(|constraint| {
                        access_query::enum_def_id(self.ctx.types, constraint).is_some()
                    });
            let hidden_qualified_namespace_member_apparent_type = self
                .qualified_namespace_member_hidden_on_exported_surface(
                    idx,
                    access.expression,
                    property_name,
                );
            let hidden_qualified_namespace_member =
                hidden_qualified_namespace_member_apparent_type.is_some();

            if !skip_flow_narrowing
                && !enum_instance_like_access
                && !hidden_qualified_namespace_member
                && let Some(obj_node) = self.ctx.arena.get(access.expression)
                && let Some(obj_ident) = self.ctx.arena.get_identifier(obj_node)
                && let Some(member_type) =
                    self.resolve_umd_global_member_by_name(&obj_ident.escaped_text, property_name)
            {
                return self.finalize_property_access_result(
                    idx,
                    member_type,
                    skip_flow_narrowing,
                    false,
                );
            }

            if !skip_flow_narrowing
                && !enum_instance_like_access
                && !hidden_qualified_namespace_member
                && let Some(member_type) =
                    self.resolve_shadowed_global_value_member(access.expression, property_name)
            {
                return self.finalize_property_access_result(
                    idx,
                    member_type,
                    skip_flow_narrowing,
                    false,
                );
            }

            // Fallback for namespace/export member accesses where type-only namespace
            // classification misses the object form but symbol resolution can still
            // identify `A.B` as a concrete exported value member.
            if !hidden_qualified_namespace_member
                && let Some(member_sym_id) = self.resolve_qualified_symbol(idx)
                && let Some(member_symbol) = self
                    .get_cross_file_symbol(member_sym_id)
                    .or_else(|| self.ctx.binder.get_symbol(member_sym_id))
            {
                // Skip type-only members (e.g., `export type { A }`, interfaces).
                // These should not be resolved as values; let the code fall
                // through to TS2693 "type only" or TS2339 "property doesn't exist" handling.
                let transitively_type_only = self
                    .is_namespace_member_transitively_type_only(access.expression, property_name);
                if !member_symbol.is_type_only
                    && !self.symbol_member_is_type_only(member_sym_id, Some(property_name))
                    && (member_symbol.flags & symbol_flags::VALUE) != 0
                    && !transitively_type_only
                {
                    let parent_sym_id = member_symbol.parent;
                    if let Some(parent_symbol) = self
                        .get_cross_file_symbol(parent_sym_id)
                        .or_else(|| self.ctx.binder.get_symbol(parent_sym_id))
                        && (parent_symbol.flags & (symbol_flags::MODULE | symbol_flags::ENUM)) != 0
                    {
                        // If the member is an enum (not an enum member), return
                        // the enum object type so property access on enum members
                        // (e.g., M3.Color.Blue) resolves correctly.
                        let member_type = if (member_symbol.flags & symbol_flags::ENUM) != 0
                            && (member_symbol.flags & symbol_flags::ENUM_MEMBER) == 0
                        {
                            self.enum_object_type(member_sym_id)
                                .unwrap_or_else(|| self.get_type_of_symbol(member_sym_id))
                        } else if (member_symbol.flags & symbol_flags::INTERFACE) != 0
                            && (member_symbol.flags & symbol_flags::VALUE) != 0
                        {
                            // When a namespace member is both an interface and a value
                            // (e.g., `interface NumberFormat` + `var NumberFormat: { new(): ... }`
                            // in namespace Intl), resolve the value declaration's type so
                            // construct signatures are available for `new NS.Member()`.
                            // This mirrors the merged-symbol resolution in get_type_of_identifier.
                            let value_decl = member_symbol.value_declaration;
                            let declarations = member_symbol.declarations.clone();
                            let preferred = self
                                .preferred_value_declaration(
                                    member_sym_id,
                                    value_decl,
                                    &declarations,
                                )
                                .unwrap_or(value_decl);
                            let mut val_type =
                                self.type_of_value_declaration_for_symbol(member_sym_id, preferred);
                            if val_type == TypeId::UNKNOWN || val_type == TypeId::ERROR {
                                for &decl_idx in &declarations {
                                    if decl_idx == preferred {
                                        continue;
                                    }
                                    let candidate = self.type_of_value_declaration_for_symbol(
                                        member_sym_id,
                                        decl_idx,
                                    );
                                    if candidate != TypeId::UNKNOWN && candidate != TypeId::ERROR {
                                        val_type = candidate;
                                        break;
                                    }
                                }
                            }
                            if val_type != TypeId::UNKNOWN && val_type != TypeId::ERROR {
                                val_type
                            } else {
                                self.get_type_of_symbol(member_sym_id)
                            }
                        } else {
                            // For merged interface+variable symbols (e.g.,
                            // `interface Foo` + `var Foo: FooConstructor`), prefer the
                            // variable's type in value position so construct signatures
                            // are visible to `new` expressions.
                            self.merged_value_type_for_symbol_if_available(member_sym_id)
                                .unwrap_or_else(|| self.get_type_of_symbol(member_sym_id))
                        };
                        if member_type != TypeId::ERROR && member_type != TypeId::UNKNOWN {
                            return self.finalize_property_access_result(
                                idx,
                                member_type,
                                skip_flow_narrowing,
                                false,
                            );
                        }
                    }
                }
            }

            if self.namespace_has_type_only_member(object_type, property_name) {
                if self.is_unresolved_import_symbol(access.expression) {
                    return TypeId::ERROR;
                }
                // Don't emit TS2693 in heritage clause context — the heritage
                // checker will emit the appropriate error (e.g., TS2689).
                if self
                    .find_enclosing_heritage_clause(access.name_or_argument)
                    .is_none()
                {
                    // Emit TS2708 for namespace member access (e.g., ns.Interface())
                    // This is "Cannot use namespace as a value"
                    // Get the namespace name from the left side of the access
                    if let Some(ns_name) = self.entity_name_text(access.expression) {
                        self.report_wrong_meaning_diagnostic(
                            &ns_name,
                            access.expression,
                            crate::query_boundaries::name_resolution::NameLookupKind::Namespace,
                        );
                    }
                    // tsc does NOT emit TS2693 for the type-only member
                    // when TS2708 was already emitted for the namespace.
                }
                return TypeId::ERROR;
            }
            if let Some(display_type) = hidden_qualified_namespace_member_apparent_type.as_deref() {
                if !access.question_dot_token
                    && !property_name.starts_with('#')
                    && !accessibility_error_emitted
                {
                    self.error_property_not_exist_with_apparent_type(
                        property_name,
                        display_type,
                        access.name_or_argument,
                    );
                }
                return TypeId::ERROR;
            }
            if self.is_namespace_value_type(object_type) && !enum_instance_like_access {
                // When the object type is a TypeQuery (typeof M) for a namespace,
                // try to resolve the property from the namespace symbol's exports.
                // This handles `var m: typeof M; m.Point` where `m` is a variable
                // typed as `typeof Namespace`.
                if let Some(ns_member_type) =
                    self.resolve_namespace_typeof_member(object_type, property_name)
                {
                    return self.finalize_property_access_result(
                        idx,
                        ns_member_type,
                        skip_flow_narrowing,
                        false,
                    );
                }
                if self.is_js_file()
                    && property_name == "prototype"
                    && self.property_access_is_direct_write_target(idx)
                {
                    return TypeId::ANY;
                }
                if self.find_enclosing_computed_property(idx).is_some()
                    && self.get_symbol_property_name_from_expr(idx).is_some()
                {
                    return TypeId::SYMBOL;
                }
                if !access.question_dot_token
                    && !property_name.starts_with('#')
                    && !accessibility_error_emitted
                {
                    // Check if the base expression is an uninstantiated namespace.
                    // tsc emits TS2708 "Cannot use namespace 'X' as a value" on the
                    // namespace identifier, not TS2339 on the property.
                    if let Some(ns_name) = self.uninstantiated_namespace_name(access.expression) {
                        self.report_wrong_meaning_diagnostic(
                            &ns_name,
                            access.expression,
                            crate::query_boundaries::name_resolution::NameLookupKind::Namespace,
                        );
                    } else {
                        self.error_property_not_exist_at(
                            property_name,
                            display_object_type,
                            access.name_or_argument,
                        );
                    }
                }
                return TypeId::ERROR;
            }

            let object_type_for_access = if enum_instance_like_access {
                self.apparent_enum_instance_type(object_type)
                    .unwrap_or_else(|| self.resolve_type_for_property_access(object_type))
            } else {
                self.resolve_type_for_property_access(object_type)
            };
            if object_type_for_access == TypeId::ANY {
                return TypeId::ANY;
            }
            if object_type_for_access == TypeId::ERROR {
                return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
            }

            // In write context (skip_flow_narrowing), skip this shortcut:
            // resolve_namespace_value_member returns the symbol's read type, which
            // doesn't account for divergent getter/setter types. The full property
            // access path below correctly uses write_type for setter parameters.
            //
            // Do this after resolving the base type for property access so cross-file
            // enum/namespace objects (e.g. imported class statics initialized to enums)
            // classify the same way as local ones.
            if !skip_flow_narrowing
                && !enum_instance_like_access
                && !hidden_qualified_namespace_member
                && let Some(member_type) =
                    self.resolve_namespace_value_member(object_type_for_access, property_name)
            {
                return self.finalize_property_access_result(
                    idx,
                    member_type,
                    skip_flow_narrowing,
                    false,
                );
            }

            if self.ctx.strict_bind_call_apply()
                && let Some(strict_method_type) = self.strict_bind_call_apply_method_type(
                    object_type_for_access,
                    access.expression,
                    property_name,
                )
            {
                return self.finalize_property_access_result(
                    idx,
                    strict_method_type,
                    skip_flow_narrowing,
                    false,
                );
            }

            if let Some(iterator_method_type) =
                self.synthesized_array_iterator_method_type(object_type_for_access, property_name)
            {
                return self.finalize_property_access_result(
                    idx,
                    iterator_method_type,
                    skip_flow_narrowing,
                    false,
                );
            }

            if self.is_super_expression(access.expression)
                && let Some((class_idx, is_static_access)) =
                    self.resolve_class_for_access(access.expression, object_type_for_access)
                && !is_static_access
                && matches!(
                    self.summarize_class_chain(class_idx)
                        .member_kind(property_name, false, true),
                    Some(ClassMemberKind::FieldLike)
                )
            {
                return TypeId::ANY;
            }

            // Use the environment-aware resolver so that array methods, boxed
            // primitive types, and other lib-registered types are available.
            let result =
                self.resolve_property_access_with_env(object_type_for_access, property_name);
            match result {
                PropertyAccessResult::Success {
                    type_id: mut prop_type,
                    write_type,
                    from_index_signature,
                } => {
                    if property_name == "exports"
                        && prop_type == TypeId::ANY
                        && self.is_js_file()
                        && let Some(obj_node) = self.ctx.arena.get(access.expression)
                        && let Some(ident) = self.ctx.arena.get_identifier(obj_node)
                        && ident.escaped_text == "module"
                        && self.current_file_commonjs_module_identifier_is_unshadowed(
                            access.expression,
                        )
                    {
                        return self.current_file_commonjs_module_exports_namespace_type();
                    }

                    // A bare type-parameter receiver can fall back to `any` here
                    // when the constraint only exposes the property on some union
                    // members. Preserve TS2339 for direct reads like `value.foo`
                    // but avoid firing after control-flow has already refined the
                    // receiver to a narrower view.
                    if !skip_flow_narrowing
                        && !from_index_signature
                        && prop_type == TypeId::ANY
                        && object_type == object_type_for_access
                        && object_type_for_access == original_object_type
                        && crate::query_boundaries::state::checking::is_type_parameter_like(
                            self.ctx.types,
                            object_type_for_access,
                        )
                        && !self.type_parameter_constraint_has_explicit_property(
                            object_type_for_access,
                            property_name,
                        )
                    {
                        self.error_property_not_exist_at(
                            property_name,
                            object_type_for_access,
                            access.name_or_argument,
                        );
                        return TypeId::ERROR;
                    }

                    // Substitute polymorphic `this` type with the receiver type.
                    // E.g., for `class C<T> { x = this; }`, accessing `c.x` where
                    // `c: C<string>` should yield `C<string>`, not raw `ThisType`.
                    if tsz_solver::contains_this_type(self.ctx.types, prop_type) {
                        prop_type = tsz_solver::substitute_this_type(
                            self.ctx.types,
                            prop_type,
                            original_object_type,
                        );
                    } else {
                        // When a method returns `this` on an intersection member,
                        // the solver's object visitor eagerly binds `this` to the
                        // structural (flattened) object — so `contains_this_type`
                        // above returns false. Re-resolve with `this` binding
                        // deferred to recover raw `ThisType`, then substitute with
                        // the nominal receiver (e.g., Thing5 instead of {a,b,c}).
                        let evaluator =
                            tsz_solver::operations::property::PropertyAccessEvaluator::new(
                                self.ctx.types,
                            );
                        evaluator.set_skip_this_binding(true);
                        let raw = evaluator
                            .resolve_property_access(object_type_for_access, property_name);
                        if let PropertyAccessResult::Success {
                            type_id: raw_type, ..
                        } = raw
                            && tsz_solver::contains_this_type(self.ctx.types, raw_type)
                        {
                            prop_type = tsz_solver::substitute_this_type(
                                self.ctx.types,
                                raw_type,
                                original_object_type,
                            );
                        }
                    }

                    if skip_flow_narrowing
                        && from_index_signature
                        && crate::query_boundaries::state::checking::is_type_parameter_like(
                            self.ctx.types,
                            object_type,
                        )
                        && !self.type_parameter_constraint_has_explicit_property(
                            object_type,
                            property_name,
                        )
                    {
                        self.error_property_not_exist_at(
                            property_name,
                            object_type,
                            access.name_or_argument,
                        );
                        return TypeId::ERROR;
                    }

                    let union_has_explicit_member = from_index_signature
                        && self.union_has_explicit_property_member(
                            object_type_for_access,
                            property_name,
                        );
                    // Check for error 4111: property access from index signature
                    if from_index_signature
                        && self
                            .ctx
                            .compiler_options
                            .no_property_access_from_index_signature
                        && !union_has_explicit_member
                    {
                        use crate::diagnostics::diagnostic_codes;
                        self.error_at_node(
                            access.name_or_argument,
                            &format!(
                                "Property '{property_name}' comes from an index signature, so it must be accessed with ['{property_name}']."
                            ),
                            diagnostic_codes::PROPERTY_COMES_FROM_AN_INDEX_SIGNATURE_SO_IT_MUST_BE_ACCESSED_WITH,
                        );
                    }
                    if skip_flow_narrowing
                        && self.union_write_requires_existing_named_member(
                            object_type_for_access,
                            property_name,
                        )
                    {
                        self.error_property_not_exist_at(
                            property_name,
                            object_type_for_access,
                            access.name_or_argument,
                        );
                        return TypeId::ERROR;
                    }
                    // When in a write context (assignment target), use the setter
                    // type if the property has divergent getter/setter types.
                    let effective_type = effective_write_result(prop_type, write_type);
                    self.finalize_property_access_result(
                        idx,
                        effective_type,
                        skip_flow_narrowing,
                        skip_result_flow_for_result,
                    )
                }

                PropertyAccessResult::PropertyNotFound { .. } => {
                    let resolved_class_access =
                        self.resolve_class_for_access(access.expression, object_type_for_access);
                    let class_chain_summary = resolved_class_access
                        .map(|(class_idx, _)| self.summarize_class_chain(class_idx));

                    if let Some(augmented_type) = self.resolve_array_global_augmentation_property(
                        object_type_for_access,
                        property_name,
                    ) {
                        return self.finalize_property_access_result(
                            idx,
                            augmented_type,
                            skip_flow_narrowing,
                            false,
                        );
                    }
                    // Check global interface augmentations for primitive wrappers
                    // and other built-in types (e.g., `interface Boolean { doStuff() }`)
                    if let Some(augmented_type) = self.resolve_general_global_augmentation_property(
                        object_type_for_access,
                        property_name,
                    ) {
                        return self.finalize_property_access_result(
                            idx,
                            augmented_type,
                            skip_flow_narrowing,
                            false,
                        );
                    }
                    // Check module augmentations (declare module "X" { interface Y { ... } })
                    // for properties added by cross-file augmentation declarations.
                    if let Some(augmented_type) = self
                        .resolve_module_augmentation_property(object_type_for_access, property_name)
                    {
                        return self.finalize_property_access_result(
                            idx,
                            augmented_type,
                            skip_flow_narrowing,
                            false,
                        );
                    }
                    // For callable/function types, check the Function interface
                    // for augmented members (e.g., declare global { interface Function { ... } })
                    if crate::query_boundaries::property_access::is_function_type(
                        self.ctx.types,
                        object_type_for_access,
                    ) && let Some(func_iface) = self.resolve_lib_type_by_name("Function")
                        && let PropertyAccessResult::Success { type_id, .. } =
                            self.resolve_property_access_with_env(func_iface, property_name)
                    {
                        return self.finalize_property_access_result(
                            idx,
                            type_id,
                            skip_flow_narrowing,
                            false,
                        );
                    }
                    if let Some((class_idx, is_static_access)) = resolved_class_access
                        && !is_static_access
                        && let Some(interface_type) = self
                            .recover_property_from_implemented_interfaces(class_idx, property_name)
                    {
                        return self.finalize_property_access_result(
                            idx,
                            interface_type,
                            skip_flow_narrowing,
                            false,
                        );
                    }
                    // Check for optional chaining (?.) - suppress TS2339 error when using optional chaining
                    if access.question_dot_token {
                        // With optional chaining, missing property results in undefined
                        return TypeId::UNDEFINED;
                    }
                    // In JS checkJs mode, unresolved CommonJS `module.exports` accesses
                    // should use the current file's export surface instead of `any`.
                    if property_name == "exports"
                        && self.is_js_file()
                        && let Some(obj_node) = self.ctx.arena.get(access.expression)
                        && let Some(ident) = self.ctx.arena.get_identifier(obj_node)
                        && ident.escaped_text == "module"
                        && self.current_file_commonjs_module_identifier_is_unshadowed(
                            access.expression,
                        )
                    {
                        return self.current_file_commonjs_module_exports_namespace_type();
                    }
                    if self.is_js_file()
                        && self.ctx.compiler_options.check_js
                        && let Some(jsdoc_type) = self
                            .enclosing_expression_statement(idx)
                            .and_then(|stmt_idx| self.js_statement_declared_type(stmt_idx))
                            .or_else(|| self.jsdoc_type_annotation_for_node_direct(idx))
                            .or_else(|| {
                                self.jsdoc_type_annotation_for_node_direct(access.expression)
                            })
                            .or_else(|| {
                                let root = self.expression_root(idx);
                                (root != idx)
                                    .then(|| self.jsdoc_type_annotation_for_node_direct(root))?
                            })
                    {
                        return jsdoc_type;
                    }
                    if self.is_js_file()
                        && self.ctx.compiler_options.check_js
                        && let Some(expr_text) = self.expression_text(idx)
                        && let Some(jsdoc_type) = self.resolve_jsdoc_assigned_value_type(&expr_text)
                    {
                        return jsdoc_type;
                    }
                    if js_expando_before_assignment {
                        return TypeId::ANY;
                    }
                    // Check for expando property reads: X.prop where X.prop = value was assigned
                    // Recover the assigned value type when we can, then fall back to `any`.
                    if !skip_flow_narrowing
                        && !commonjs_named_props_disallowed
                        && self.is_expando_property_read(access.expression, property_name)
                    {
                        if let Some(expando_type) =
                            self.expando_property_read_type(idx, access.expression, property_name)
                        {
                            return expando_type;
                        }
                        return TypeId::ANY;
                    }
                    // Check for expando function pattern: func.prop = value
                    // TypeScript allows property assignments to function/class declarations
                    // without emitting TS2339. The assigned properties become part of the
                    // function's type (expando pattern).
                    if !commonjs_named_props_disallowed
                        && self.is_expando_function_assignment(
                            idx,
                            access.expression,
                            object_type_for_access,
                        )
                    {
                        return TypeId::ANY;
                    }
                    if self.is_js_expando_object_assignment(
                        idx,
                        access.expression,
                        object_type_for_access,
                        property_name,
                    ) {
                        return TypeId::ANY;
                    }

                    // JavaScript files allow dynamic property assignment on 'this' without errors.
                    // In JS files, accessing a property on 'this' that doesn't exist should not error
                    // and should return 'any' type, matching TypeScript's behavior.
                    let has_explicit_this_context = is_this_access
                        && self
                            .current_this_type()
                            .is_some_and(|ty| ty != TypeId::ANY && ty != TypeId::UNKNOWN);
                    // When `this` type comes from a ThisType<T> marker (e.g., Vue 2
                    // Options API pattern), property access on unresolved type parameters
                    // should not emit TS2339. The type parameters will be inferred from the
                    // object literal, creating a circular dependency that tsc handles by
                    // deferring the check.
                    // Also handle intersections containing type parameters (e.g.,
                    // `Data & Readonly<Props> & Instance` from
                    // `ThisType<Data & Readonly<Props> & Instance>` before inference).
                    let this_owner_is_object_literal = self
                        .this_has_contextual_owner(access.expression)
                        .and_then(|owner_idx| self.ctx.arena.get(owner_idx))
                        .is_some_and(|owner_node| {
                            owner_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        });
                    if is_this_access
                        && this_owner_is_object_literal
                        && self.ctx.this_type_stack.last().is_some_and(|&top| {
                            access_query::is_this_type(self.ctx.types, top)
                                || crate::query_boundaries::state::checking::is_type_parameter_like(
                                    self.ctx.types,
                                    top,
                                )
                                || crate::query_boundaries::common::contains_type_parameters(
                                    self.ctx.types,
                                    top,
                                )
                        })
                    {
                        return TypeId::ANY;
                    }

                    if self.is_js_file()
                        && is_this_access
                        && skip_flow_narrowing
                        && self.property_access_is_direct_write_target(idx)
                    {
                        let object_literal_owned_this = self
                            .this_has_contextual_owner(access.expression)
                            .and_then(|owner_idx| self.ctx.arena.get(owner_idx))
                            .is_some_and(|owner_node| {
                                owner_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                            });
                        if !object_literal_owned_this {
                            return TypeId::ANY;
                        }
                    }

                    if self.is_js_file() && is_this_access && !has_explicit_this_context {
                        // Allow dynamic property on `this` in loose JS contexts, but
                        // keep checks when `this` is contextually owned by a class/object
                        // member (checkJs should still enforce member-consistent typing).
                        if self.this_has_contextual_owner(access.expression).is_none() {
                            return TypeId::ANY;
                        }
                        if self.is_jsdoc_annotated_this_member_declaration(idx) {
                            return TypeId::ANY;
                        }
                    }

                    if self.is_js_file()
                        && property_name == "prototype"
                        && self.property_access_is_direct_write_target(idx)
                    {
                        return TypeId::ANY;
                    }

                    if self.is_js_file()
                        && self.is_super_expression(access.expression)
                        && let Some((_, is_static_access)) = resolved_class_access
                        && is_static_access
                        && matches!(
                            class_chain_summary
                                .as_ref()
                                .and_then(|summary| summary.member_kind(property_name, true, true)),
                            Some(ClassMemberKind::FieldLike)
                        )
                    {
                        return TypeId::ANY;
                    }

                    // TSC does not emit TS2576 for `super.member` access. When accessing a
                    // property through `super`, TypeScript suppresses "did you mean to access
                    // the static member?" errors entirely. The TS2576 check only applies to
                    // regular instance access (e.g., `instance.y` where `y` is static), not
                    // super access. See: superAccess2.ts — `super.y()` in instance method and
                    // `super.x()` in static method produce no TS2576 errors in tsc.

                    // TS2576: instance.member where `member` exists on the class static side.
                    // Route this through the shared class summary so inherited
                    // static fields/accessors don't force another class walk.
                    if !self.is_super_expression(access.expression)
                        && let Some((_, is_static_access)) = resolved_class_access
                        && !is_static_access
                        && class_chain_summary
                            .as_ref()
                            .and_then(|summary| summary.lookup(property_name, true, true))
                            .is_some()
                    {
                        use crate::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };

                        let object_type_str =
                            self.format_type_for_assignability_message(display_object_type);
                        let static_member_name = format!("{object_type_str}.{property_name}");
                        let message = format_message(
                            diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN_TO_ACCESS_THE_STATIC_MEMBER_INSTEAD,
                            &[property_name, &object_type_str, &static_member_name],
                        );
                        // Report at the property name node, not the full expression (matches tsc behavior)
                        self.error_at_node(
                            access.name_or_argument,
                            &message,
                            diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN_TO_ACCESS_THE_STATIC_MEMBER_INSTEAD,
                        );
                        return TypeId::ERROR;
                    }

                    // Don't emit TS2339 for private fields (starting with #) - they're handled elsewhere.
                    // Also suppress when accessibility check already emitted TS2341/TS2445
                    // (property exists but is private/protected — not truly "not found").
                    // TSC also suppresses property-not-found errors for `super.member` access:
                    // when a property is not found on the super type, TypeScript does not report
                    // TS2339. For example, `super.x()` in a static method (where `x` is an
                    // instance method) and `super.y()` in an instance method (where `y` is a
                    // static method) produce no TS2339 errors in tsc (see superAccess2.ts).
                    if !property_name.starts_with('#')
                        && !accessibility_error_emitted
                        && !self.is_super_expression(access.expression)
                    {
                        if self.is_js_file()
                            && self.is_current_file_commonjs_export_base(access.expression)
                        {
                            let export_namespace_type =
                                self.current_file_commonjs_module_exports_namespace_type();
                            display_object_type = export_namespace_type;
                            if let PropertyAccessResult::Success {
                                type_id,
                                write_type,
                                ..
                            } = self.resolve_property_access_with_env(
                                export_namespace_type,
                                property_name,
                            ) {
                                return self.finalize_property_access_result(
                                    idx,
                                    effective_write_result(type_id, write_type),
                                    skip_flow_narrowing,
                                    false,
                                );
                            }
                        }
                        // Property access expressions are VALUE context - always emit TS2339.
                        // TS2694 (namespace has no exported member) is for TYPE context only,
                        // which is handled separately in type name resolution.
                        // Use display_object_type to preserve literal types in error messages
                        // while maintaining nominal identity (e.g., D<string>)
                        // Report at the property name node, not the full expression (matches tsc behavior)
                        if let Some(sym_id) = self.resolve_qualified_symbol(access.expression)
                            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                            && symbol.has_any_flags(tsz_binder::symbol_flags::ENUM)
                            && !symbol.has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER)
                        {
                            self.error_property_not_exist_on_enum(
                                property_name,
                                &symbol.escaped_name.to_string(),
                                display_object_type,
                                access.name_or_argument,
                            );
                            return TypeId::ERROR;
                        }

                        if enum_instance_like_access {
                            let enum_display: Option<String> =
                                access_query::type_parameter_constraint(
                                    self.ctx.types,
                                    display_object_type,
                                )
                                .filter(|constraint| {
                                    access_query::enum_def_id(self.ctx.types, *constraint).is_some()
                                })
                                .map(|constraint| {
                                    self.format_type_for_assignability_message(constraint)
                                })
                                .or_else(|| {
                                    access_query::enum_def_id(self.ctx.types, display_object_type)
                                        .map(|_| {
                                            self.format_type_for_assignability_message(
                                                display_object_type,
                                            )
                                        })
                                });
                            if let Some(enum_display) = enum_display {
                                self.error_property_not_exist_with_apparent_type(
                                    property_name,
                                    &enum_display,
                                    access.name_or_argument,
                                );
                            } else {
                                self.error_property_not_exist_at(
                                    property_name,
                                    display_object_type,
                                    access.name_or_argument,
                                );
                            }
                        } else {
                            self.error_property_not_exist_at(
                                property_name,
                                display_object_type,
                                access.name_or_argument,
                            );
                        }
                    }
                    TypeId::ERROR
                }

                PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type,
                    cause,
                } => {
                    // Check for optional chaining (?.)
                    if access.question_dot_token {
                        if self
                            .ctx
                            .compiler_options
                            .no_property_access_from_index_signature
                            && let (Some(non_nullish_base), _) =
                                self.split_nullish_type(object_type_for_access)
                            && let PropertyAccessResult::Success {
                                from_index_signature,
                                ..
                            } = self
                                .resolve_property_access_with_env(non_nullish_base, property_name)
                            && from_index_signature
                            && !self
                                .union_has_explicit_property_member(non_nullish_base, property_name)
                        {
                            use crate::diagnostics::diagnostic_codes;
                            self.error_at_node(
                                access.name_or_argument,
                                &format!(
                                    "Property '{property_name}' comes from an index signature, so it must be accessed with ['{property_name}']."
                                ),
                                diagnostic_codes::PROPERTY_COMES_FROM_AN_INDEX_SIGNATURE_SO_IT_MUST_BE_ACCESSED_WITH,
                            );
                        }
                        // Suppress error, return (property_type | undefined)
                        let base_type = property_type.unwrap_or(TypeId::UNKNOWN);
                        return factory.union2(base_type, TypeId::UNDEFINED);
                    }

                    // Report error based on the cause (TS2531/TS2532/TS2533 or TS18050)
                    // TS18050 is for definitely-nullish values in strict mode
                    // TS2531/2532/2533 are for possibly-nullish values in strict mode
                    use crate::diagnostics::diagnostic_codes;

                    // Suppress cascade errors when cause is ERROR/ANY/UNKNOWN
                    if cause == TypeId::ERROR || cause == TypeId::ANY || cause == TypeId::UNKNOWN {
                        return property_type.unwrap_or(TypeId::ERROR);
                    }

                    // Check if the type is entirely nullish (no non-nullish part in union)
                    let is_type_nullish = object_type_for_access == TypeId::NULL
                        || object_type_for_access == TypeId::UNDEFINED;

                    // For possibly-nullish values in non-strict mode, don't error
                    // But for definitely-nullish values in non-strict mode, fall through to error reporting below
                    if !self.ctx.compiler_options.strict_null_checks && !is_type_nullish {
                        return self.finalize_property_access_result(
                            idx,
                            property_type.unwrap_or(TypeId::ERROR),
                            skip_flow_narrowing,
                            false,
                        );
                    }
                    // Check if the expression is a literal null/undefined keyword (not a variable)
                    // TS18050 is only for `null.foo` and `undefined.bar`, not `x.foo` where x: null
                    // TS18050 is emitted even without strictNullChecks, so check first
                    let is_literal_nullish =
                        if let Some(expr_node) = self.ctx.arena.get(access.expression) {
                            expr_node.kind == SyntaxKind::NullKeyword as u16
                                || (expr_node.kind == SyntaxKind::Identifier as u16
                                    && self
                                        .ctx
                                        .arena
                                        .get_identifier(expr_node)
                                        .is_some_and(|ident| ident.escaped_text == "undefined"))
                        } else {
                            false
                        };

                    // When the expression IS a literal null/undefined keyword (e.g., null.foo or undefined.bar),
                    // emit TS18050 "The value 'X' cannot be used here."
                    if is_literal_nullish {
                        let value_name = if cause == TypeId::NULL {
                            "null"
                        } else if cause == TypeId::UNDEFINED {
                            "undefined"
                        } else {
                            "null | undefined"
                        };
                        self.error_at_node_msg(
                            access.expression,
                            diagnostic_codes::THE_VALUE_CANNOT_BE_USED_HERE,
                            &[value_name],
                        );
                        return self.finalize_property_access_result(
                            idx,
                            property_type.unwrap_or(TypeId::ERROR),
                            skip_flow_narrowing,
                            false,
                        );
                    }

                    // Without strictNullChecks, null/undefined are in every type's domain,
                    // so TS18047/TS18048/TS18049 are never emitted (matches tsc behavior).
                    // Note: TS18050 for literal null/undefined is handled above.
                    if !self.ctx.compiler_options.strict_null_checks {
                        return self.finalize_property_access_result(
                            idx,
                            property_type.unwrap_or(TypeId::ERROR),
                            skip_flow_narrowing,
                            false,
                        );
                    }

                    // Try to get the name of the expression (handles identifiers and property chains like a.b)
                    // Use specific error codes (TS18047/18048/18049) when name is available
                    let name = self.expression_text(access.expression);

                    let (code, message): (u32, String) = if let Some(ref name) = name {
                        // Use specific error codes with the variable name
                        if cause == TypeId::NULL {
                            (
                                diagnostic_codes::IS_POSSIBLY_NULL,
                                format!("'{name}' is possibly 'null'."),
                            )
                        } else if cause == TypeId::UNDEFINED {
                            (
                                diagnostic_codes::IS_POSSIBLY_UNDEFINED,
                                format!("'{name}' is possibly 'undefined'."),
                            )
                        } else {
                            (
                                diagnostic_codes::IS_POSSIBLY_NULL_OR_UNDEFINED,
                                format!("'{name}' is possibly 'null' or 'undefined'."),
                            )
                        }
                    } else {
                        // Fall back to generic error codes
                        if cause == TypeId::NULL {
                            (
                                diagnostic_codes::OBJECT_IS_POSSIBLY_NULL,
                                "Object is possibly 'null'.".to_string(),
                            )
                        } else if cause == TypeId::UNDEFINED {
                            (
                                diagnostic_codes::OBJECT_IS_POSSIBLY_UNDEFINED,
                                "Object is possibly 'undefined'.".to_string(),
                            )
                        } else {
                            (
                                diagnostic_codes::OBJECT_IS_POSSIBLY_NULL_OR_UNDEFINED,
                                "Object is possibly 'null' or 'undefined'.".to_string(),
                            )
                        }
                    };

                    // Report the error on the expression part
                    self.error_at_node(access.expression, &message, code);

                    // Error recovery: return the property type found in valid members
                    self.finalize_property_access_result(
                        idx,
                        property_type.unwrap_or(TypeId::ERROR),
                        skip_flow_narrowing,
                        false,
                    )
                }

                PropertyAccessResult::IsUnknown => {
                    // TS18046: 'x' is of type 'unknown'.
                    // Without strictNullChecks, unknown is treated like any (no error).
                    if self.error_is_of_type_unknown(access.expression) {
                        TypeId::ERROR
                    } else {
                        TypeId::ANY
                    }
                }
            }
        } else {
            TypeId::ANY
        }
    }

    fn enum_member_initializer_display_type(&mut self, expr_idx: NodeIndex) -> Option<TypeId> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol.declarations.first().copied()?
        };

        let var_decl = self.ctx.arena.get_variable_declaration_at(decl_idx)?;
        if var_decl.initializer.is_none() {
            return None;
        }

        let init_type = self.get_type_of_node(var_decl.initializer);
        self.is_enum_member_type_for_widening(init_type)
            .then_some(init_type)
    }
}
