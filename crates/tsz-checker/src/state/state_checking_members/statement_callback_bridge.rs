use crate::context::TypingRequest;
use crate::state::CheckerState;
use crate::statements::StatementCheckCallbacks;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

/// Implementation of `StatementCheckCallbacks` for `CheckerState`.
///
/// This provides the actual implementation of statement checking operations
/// that `StatementChecker` delegates to. Each callback method calls the
/// corresponding method on `CheckerState`.
impl<'a> StatementCheckCallbacks for CheckerState<'a> {
    fn arena(&self) -> &tsz_parser::parser::node::NodeArena {
        self.ctx.arena
    }

    fn get_type_of_node(&mut self, idx: NodeIndex) -> TypeId {
        CheckerState::get_type_of_node(self, idx)
    }

    fn get_type_of_node_with_request(&mut self, idx: NodeIndex, request: &TypingRequest) -> TypeId {
        CheckerState::get_type_of_node_with_request(self, idx, request)
    }

    fn get_type_of_node_no_narrowing(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_node_with_request(idx, &TypingRequest::for_write_context())
    }

    fn get_type_of_node_no_narrowing_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        let request = request.contextual_opt(None).write();
        self.get_type_of_node_with_request(idx, &request)
    }

    fn check_variable_statement(&mut self, stmt_idx: NodeIndex) {
        CheckerState::check_variable_statement(self, stmt_idx);
    }

    fn check_variable_statement_with_request(
        &mut self,
        stmt_idx: NodeIndex,
        request: &TypingRequest,
    ) {
        CheckerState::check_variable_statement_with_request(self, stmt_idx, request);
    }

    fn check_variable_declaration_list(&mut self, list_idx: NodeIndex) {
        CheckerState::check_variable_declaration_list(self, list_idx);
    }

    fn check_variable_declaration_list_with_request(
        &mut self,
        list_idx: NodeIndex,
        request: &TypingRequest,
    ) {
        CheckerState::check_variable_declaration_list_with_request(self, list_idx, request);
    }

    fn check_variable_declaration(&mut self, decl_idx: NodeIndex) {
        CheckerState::check_variable_declaration(self, decl_idx);
    }

    fn check_variable_declaration_with_request(
        &mut self,
        decl_idx: NodeIndex,
        request: &TypingRequest,
    ) {
        CheckerState::check_variable_declaration_with_request(self, decl_idx, request);
    }

    fn check_return_statement(&mut self, stmt_idx: NodeIndex) {
        CheckerState::check_return_statement(self, stmt_idx);
    }

    fn check_return_statement_with_request(
        &mut self,
        stmt_idx: NodeIndex,
        _request: &TypingRequest,
    ) {
        CheckerState::check_return_statement(self, stmt_idx);
    }

    fn check_function_implementations(&mut self, stmts: &[NodeIndex]) {
        CheckerState::check_function_implementations(self, stmts);
    }

    fn check_function_declaration(&mut self, func_idx: NodeIndex) {
        CheckerState::check_function_declaration_callback(self, func_idx);
    }

    fn check_class_declaration(&mut self, class_idx: NodeIndex) {
        // Note: DeclarationChecker::check_class_declaration handles TS2564 (property
        // initialization) but CheckerState::check_class_declaration also handles it
        // more comprehensively (with parameter properties, derived classes, etc.).
        // We skip the DeclarationChecker delegation for classes to avoid duplicate
        // TS2564 emissions. DeclarationChecker::check_class_declaration is tested
        // independently via its own test suite.
        CheckerState::check_class_declaration(self, class_idx);
    }

    fn check_interface_declaration(&mut self, iface_idx: NodeIndex) {
        // Delegate to DeclarationChecker first
        let mut checker = crate::declarations::DeclarationChecker::new(&mut self.ctx);
        checker.check_interface_declaration(iface_idx);

        // Continue with comprehensive interface checking in CheckerState
        CheckerState::check_interface_declaration(self, iface_idx);
    }

    fn check_import_declaration(&mut self, import_idx: NodeIndex) {
        CheckerState::check_import_declaration(self, import_idx);
    }

    fn check_import_equals_declaration(&mut self, import_idx: NodeIndex) {
        CheckerState::check_import_equals_declaration(self, import_idx);
    }

    fn check_export_declaration(&mut self, export_idx: NodeIndex) {
        if let Some(export_decl) = self.ctx.arena.get_export_decl_at(export_idx) {
            if export_decl.is_default_export && self.is_inside_namespace_declaration(export_idx) {
                // tsc points TS1319 at the `default` keyword for class/function
                // declarations, but at the `export` keyword for expression exports.
                // Use the default keyword position when available (class/function);
                // fall back to the export node (expression exports).
                if let Some(default_pos) = export_decl.default_keyword_pos {
                    // Check if this is a class/function declaration export
                    // by looking at the export clause node kind
                    let has_declaration = self
                        .ctx
                        .arena
                        .get(export_decl.export_clause)
                        .is_some_and(|n| {
                            matches!(
                                n.kind,
                                syntax_kind_ext::CLASS_DECLARATION
                                    | syntax_kind_ext::FUNCTION_DECLARATION
                                    | syntax_kind_ext::CLASS_EXPRESSION
                            )
                        });
                    if has_declaration {
                        self.error_at_position(
                            default_pos,
                            7, // length of "default"
                            crate::diagnostics::diagnostic_messages::A_DEFAULT_EXPORT_CAN_ONLY_BE_USED_IN_AN_ECMASCRIPT_STYLE_MODULE,
                            crate::diagnostics::diagnostic_codes::A_DEFAULT_EXPORT_CAN_ONLY_BE_USED_IN_AN_ECMASCRIPT_STYLE_MODULE,
                        );
                    } else {
                        self.error_at_node(
                            export_idx,
                            crate::diagnostics::diagnostic_messages::A_DEFAULT_EXPORT_CAN_ONLY_BE_USED_IN_AN_ECMASCRIPT_STYLE_MODULE,
                            crate::diagnostics::diagnostic_codes::A_DEFAULT_EXPORT_CAN_ONLY_BE_USED_IN_AN_ECMASCRIPT_STYLE_MODULE,
                        );
                    }
                } else {
                    self.error_at_node(
                        export_idx,
                        crate::diagnostics::diagnostic_messages::A_DEFAULT_EXPORT_CAN_ONLY_BE_USED_IN_AN_ECMASCRIPT_STYLE_MODULE,
                        crate::diagnostics::diagnostic_codes::A_DEFAULT_EXPORT_CAN_ONLY_BE_USED_IN_AN_ECMASCRIPT_STYLE_MODULE,
                    );
                }
                // tsc does not further resolve the exported expression when
                // the export default is invalid in a namespace context.
                return;
            }

            // TS1194: Export declarations are not permitted in a namespace.
            // `export { } from "mod"` is NEVER allowed in any namespace (even declare);
            // `export { }` (no from) is only disallowed in non-ambient namespaces.
            if self.is_inside_namespace_declaration(export_idx) {
                let has_from = export_decl.module_specifier.is_some();
                let is_named = self
                    .ctx
                    .arena
                    .get(export_decl.export_clause)
                    .is_some_and(|n| n.kind == syntax_kind_ext::NAMED_EXPORTS);
                let is_ambient = self.ctx.is_declaration_file()
                    || self.ctx.arena.is_in_ambient_context(export_idx);
                if has_from || (is_named && !is_ambient) {
                    let report_idx = if has_from {
                        export_decl.module_specifier
                    } else {
                        export_idx
                    };
                    self.error_at_node(
                        report_idx,
                        crate::diagnostics::diagnostic_messages::EXPORT_DECLARATIONS_ARE_NOT_PERMITTED_IN_A_NAMESPACE,
                        crate::diagnostics::diagnostic_codes::EXPORT_DECLARATIONS_ARE_NOT_PERMITTED_IN_A_NAMESPACE,
                    );
                }
            }

            // TS2880: Warn about deprecated `assert` keyword
            self.check_import_attributes_deprecated_assert(export_decl.attributes);

            // TS2823: Import attributes require specific module options
            self.check_import_attributes_module_option(export_decl.attributes);

            // TS2322: Check export attribute values against global ImportAttributes interface
            self.check_import_attributes_assignability(export_decl.attributes);

            // Check module specifier for unresolved modules (TS2792)
            if export_decl.module_specifier.is_some() {
                self.check_export_module_specifier(export_idx);
            }

            // Check the wrapped declaration
            if export_decl.export_clause.is_some() {
                let clause_idx = export_decl.export_clause;
                let expected_type = if export_decl.is_default_export {
                    self.jsdoc_type_annotation_for_node(export_idx)
                } else {
                    None
                };

                {
                    let skip_clause_expression_check = export_decl.module_specifier.is_some()
                        && self
                            .ctx
                            .arena
                            .get(clause_idx)
                            .is_some_and(|n| n.kind == SyntaxKind::Identifier as u16);
                    if !skip_clause_expression_check {
                        self.check_statement(clause_idx);
                    }
                }

                if let Some(et) = expected_type {
                    let request = crate::context::TypingRequest::with_contextual_type(et);
                    let actual_type = self.get_type_of_node_with_request(clause_idx, &request);
                    self.check_assignable_or_report(actual_type, et, clause_idx);
                    if let Some(expr_node) = self.ctx.arena.get(clause_idx)
                        && expr_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    {
                        self.check_object_literal_excess_properties(actual_type, et, clause_idx);
                    }
                }

                // CJS+VMS checks for all exports (TS1287/TS1295)
                // These take priority over ESM-specific VMS checks.
                // TSC skips these for .d.ts files.
                let mut cjs_vms_emitted = false;
                if self.ctx.compiler_options.verbatim_module_syntax
                    && !self.ctx.is_declaration_file()
                    && !export_decl.is_type_only
                    && !self.is_inside_namespace_declaration(export_idx)
                {
                    let clause_kind = self.ctx.arena.get(clause_idx).map(|n| n.kind);
                    let clause_is_value_decl = clause_kind.is_some_and(|k| {
                        k == syntax_kind_ext::FUNCTION_DECLARATION
                            || k == syntax_kind_ext::CLASS_DECLARATION
                            || k == syntax_kind_ext::VARIABLE_STATEMENT
                            || k == syntax_kind_ext::ENUM_DECLARATION
                    });
                    let clause_is_type_decl = clause_kind.is_some_and(|k| {
                        k == syntax_kind_ext::INTERFACE_DECLARATION
                            || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    });
                    let clause_is_namespace =
                        clause_kind.is_some_and(|k| k == syntax_kind_ext::MODULE_DECLARATION);
                    // Type declarations (interface/type alias) are erased —
                    // no CJS VMS error needed.
                    // NamedExports (export { ... }) are handled separately by the
                    // ESM VMS checks below — skip them here.
                    if clause_is_value_decl {
                        cjs_vms_emitted =
                            self.check_verbatim_module_syntax_cjs_export(export_idx, false, true);
                    } else if clause_is_namespace {
                        // Namespace with values → TS1287; type-only namespace → skip
                        let has_values = self.namespace_has_value_declarations(clause_idx);
                        if has_values {
                            cjs_vms_emitted = self
                                .check_verbatim_module_syntax_cjs_export(export_idx, false, true);
                        }
                    } else if export_decl.is_default_export && !clause_is_type_decl {
                        // export default <expr> in CJS → TS1295
                        cjs_vms_emitted =
                            self.check_verbatim_module_syntax_cjs_export(export_idx, false, false);
                    }
                }

                // TS1284/TS1285: export default VMS checks (ESM mode only)
                if export_decl.is_default_export && !cjs_vms_emitted {
                    self.check_verbatim_module_syntax_export_default(clause_idx);
                }

                // TS1269: Cannot use 'export import' on a type or type-only namespace
                // when 'isolatedModules' or 'verbatimModuleSyntax' is enabled.
                if let Some(clause_node) = self.ctx.arena.get(clause_idx)
                    && clause_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                {
                    self.check_export_import_equals_type_only(export_idx, clause_idx);
                }

                if self
                    .ctx
                    .arena
                    .get(clause_idx)
                    .is_some_and(|n| n.kind == syntax_kind_ext::NAMED_EXPORTS)
                {
                    // TS2207: Check for specifier-level `type` modifier when
                    // `export type { ... }` is used at the statement level.
                    if export_decl.is_type_only {
                        self.check_type_modifier_on_type_only_export(clause_idx);
                    }

                    if export_decl.module_specifier.is_none()
                        && (!self.is_inside_namespace_declaration(export_idx)
                            || self.is_inside_global_augmentation(export_idx))
                    {
                        self.check_local_named_exports(clause_idx);
                    }

                    // TS1205: Re-exporting a type under verbatimModuleSyntax
                    if !export_decl.is_type_only {
                        self.check_verbatim_module_syntax_named_exports(
                            clause_idx,
                            export_decl.module_specifier,
                        );
                    }
                }
            }
        }
    }

    fn check_type_alias_declaration(&mut self, type_alias_idx: NodeIndex) {
        // Keep type-node validation and indexed-access diagnostics wired via CheckerState.
        CheckerState::check_type_alias_declaration(self, type_alias_idx);

        if let Some(node) = self.ctx.arena.get(type_alias_idx) {
            // Continue with comprehensive type alias checking
            if let Some(type_alias) = self.ctx.arena.get_type_alias(node) {
                // TS1212: Check type alias name for strict mode reserved words
                self.check_strict_mode_reserved_name_at(type_alias.name, type_alias_idx);

                // TS2457: Type alias name cannot be reserved names
                if let Some(name_node) = self.ctx.arena.get(type_alias.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    && matches!(ident.escaped_text.as_str(), "undefined" | "void")
                {
                    use crate::diagnostics::diagnostic_codes;
                    let msg = format!("Type alias name cannot be '{}'.", ident.escaped_text);
                    self.error_at_node(
                        type_alias.name,
                        &msg,
                        diagnostic_codes::TYPE_ALIAS_NAME_CANNOT_BE,
                    );
                }
                // TS2795: Check for `intrinsic` keyword in type alias body.
                // In TSC, `intrinsic` is parsed as a keyword (not a type reference) when it
                // appears as the direct body of a type alias. Only the 4 built-in string
                // mapping types (Uppercase, Lowercase, Capitalize, Uncapitalize) may use it.
                // For non-built-in aliases, emit TS2795 and skip name resolution (which would
                // otherwise emit TS2304 since `intrinsic` isn't a real type name).
                let body_is_intrinsic_keyword =
                    self.is_bare_intrinsic_type_ref(type_alias.type_node);
                if body_is_intrinsic_keyword {
                    // Check if the alias name is one of the 4 built-in string intrinsics
                    let alias_name = self
                        .ctx
                        .arena
                        .get(type_alias.name)
                        .and_then(|n| self.ctx.arena.get_identifier(n))
                        .map(|id| id.escaped_text.as_str());
                    let is_builtin = matches!(
                        alias_name,
                        Some("Uppercase" | "Lowercase" | "Capitalize" | "Uncapitalize")
                    );
                    if !is_builtin {
                        use crate::diagnostics::diagnostic_codes;
                        self.error_at_node(
                            type_alias.type_node,
                            "The 'intrinsic' keyword can only be used to declare compiler provided intrinsic types.",
                            diagnostic_codes::THE_INTRINSIC_KEYWORD_CAN_ONLY_BE_USED_TO_DECLARE_COMPILER_PROVIDED_INTRINSIC_TY,
                        );
                    }
                }

                let (_params, updates) = self.push_type_parameters(&type_alias.type_parameters);
                // Check for unused type parameters (TS6133)
                self.check_unused_type_params(&type_alias.type_parameters, type_alias_idx);
                // The core type-alias checker already validated the body type node,
                // including missing-name resolution. Re-running that here after the
                // circular-alias marker is popped can re-enter recursive alias graphs.
                let _ = body_is_intrinsic_keyword;
                self.check_type_for_parameter_properties(type_alias.type_node);
                self.pop_type_parameters(updates);
            }
        }
    }
    fn check_enum_duplicate_members(&mut self, enum_idx: NodeIndex) {
        // TS1042: async modifier cannot be used on enum declarations
        if let Some(node) = self.ctx.arena.get(enum_idx)
            && let Some(enum_data) = self.ctx.arena.get_enum(node)
        {
            self.check_async_modifier_on_declaration(&enum_data.modifiers);
            // TS1212: Check enum name for strict mode reserved words
            self.check_strict_mode_reserved_name_at(enum_data.name, enum_idx);
        }

        // Delegate to DeclarationChecker first
        let mut checker = crate::declarations::DeclarationChecker::new(&mut self.ctx);
        checker.check_enum_declaration(enum_idx);

        // TS18033: Check computed enum member values are assignable to number.
        self.check_computed_enum_member_values(enum_idx);

        // Continue with enum duplicate members checking
        CheckerState::check_enum_duplicate_members(self, enum_idx);
    }

    fn check_module_declaration(&mut self, module_idx: NodeIndex) {
        if let Some(node) = self.ctx.arena.get(module_idx) {
            // Delegate to DeclarationChecker first
            let mut checker = crate::declarations::DeclarationChecker::new(&mut self.ctx);
            checker.check_module_declaration(module_idx);

            // TS1540: 'module' keyword deprecated for namespace declarations
            self.check_module_keyword_deprecated(module_idx);

            // Check module body and modifiers
            if let Some(module) = self.ctx.arena.get_module(node) {
                // TS1212: Check module/namespace name for strict mode reserved words
                self.check_strict_mode_reserved_name_at(module.name, module_idx);

                // TS1042: async modifier cannot be used on module/namespace declarations
                self.check_async_modifier_on_declaration(&module.modifiers);

                let is_ambient = self.has_declare_modifier(&module.modifiers);
                if module.body.is_some() {
                    self.check_module_body(module.body);
                }

                // TS1038: Check for 'declare' modifiers inside ambient module/namespace
                // TS1039: Check for initializers in ambient contexts
                // Even if we don't fully check the body, we still need to emit these errors
                if is_ambient && module.body.is_some() {
                    self.check_declare_modifiers_in_ambient_body(module.body);
                    self.check_initializers_in_ambient_body(module.body);

                    // TS2300/TS2309: Check for duplicate export assignments even in ambient modules
                    // TS2300: Check for duplicate import aliases even in ambient modules
                    // TS2303: Check for circular import aliases in ambient modules
                    // Need to extract statements from module body
                    if let Some(body_node) = self.ctx.arena.get(module.body)
                        && body_node.kind == tsz_parser::parser::syntax_kind_ext::MODULE_BLOCK
                        && let Some(block) = self.ctx.arena.get_module_block(body_node)
                        && let Some(ref statements) = block.statements
                    {
                        self.check_export_assignment(&statements.nodes);
                        self.check_import_alias_duplicates(&statements.nodes);
                        // Check import equals declarations for circular imports (TS2303)
                        for &stmt_idx in &statements.nodes {
                            if let Some(stmt_node) = self.ctx.arena.get(stmt_idx)
                                && stmt_node.kind == tsz_parser::parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                                    self.check_import_equals_declaration(stmt_idx);
                                }
                        }
                    }
                }

                // TS2300: Check for duplicate import aliases in non-ambient modules too
                // This handles namespace { import X = ...; import X = ...; }
                if !is_ambient
                    && module.body.is_some()
                    && let Some(body_node) = self.ctx.arena.get(module.body)
                    && body_node.kind == tsz_parser::parser::syntax_kind_ext::MODULE_BLOCK
                    && let Some(block) = self.ctx.arena.get_module_block(body_node)
                    && let Some(ref statements) = block.statements
                {
                    self.check_import_alias_duplicates(&statements.nodes);
                }

                // TS2300: Check for "prototype" exports in namespace-class merges.
                // Classes have an implicit static `prototype` property, so a namespace
                // merged with a class must not export a member named "prototype".
                self.check_namespace_prototype_conflict(module_idx);
            }
        }
    }

    fn check_expression_statement(&mut self, _stmt_idx: NodeIndex, expr_idx: NodeIndex) {
        if !self.ctx.compiler_options.verbatim_module_syntax || self.ctx.is_declaration_file() {
            return;
        }

        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return;
        };

        // TS1295: dynamic import() in CJS+VMS
        if expr_node.kind == syntax_kind_ext::CALL_EXPRESSION {
            let is_import_call = self
                .ctx
                .arena
                .get_call_expr(expr_node)
                .and_then(|call| self.ctx.arena.get(call.expression))
                .is_some_and(|callee| callee.kind == SyntaxKind::ImportKeyword as u16);
            if is_import_call && self.is_current_file_commonjs_for_vms() {
                self.error_at_node(
                    expr_idx,
                    crate::diagnostics::diagnostic_messages::ECMASCRIPT_IMPORTS_AND_EXPORTS_CANNOT_BE_WRITTEN_IN_A_COMMONJS_FILE_UNDER_VERBAT_2,
                    crate::diagnostics::diagnostic_codes::ECMASCRIPT_IMPORTS_AND_EXPORTS_CANNOT_BE_WRITTEN_IN_A_COMMONJS_FILE_UNDER_VERBAT_2,
                );
            }
        }

        // TS2748: property access on ambient const enum (e.g. `F.A;`)
        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(expr_node)
        {
            let left_idx = access.expression;
            if let Some(left_node) = self.ctx.arena.get(left_idx)
                && left_node.kind == SyntaxKind::Identifier as u16
                && let Some(sym_id) = self.resolve_identifier_symbol(left_idx)
                && self.is_ambient_const_enum_symbol(sym_id)
            {
                let msg = crate::diagnostics::format_message(
                                crate::diagnostics::diagnostic_messages::CANNOT_ACCESS_AMBIENT_CONST_ENUMS_WHEN_IS_ENABLED,
                                &["verbatimModuleSyntax"],
                            );
                self.error_at_node(
                                expr_idx,
                                &msg,
                                crate::diagnostics::diagnostic_codes::CANNOT_ACCESS_AMBIENT_CONST_ENUMS_WHEN_IS_ENABLED,
                            );
            }
        }
    }

    fn check_await_expression(&mut self, expr_idx: NodeIndex) {
        CheckerState::check_await_expression(self, expr_idx);
    }

    fn check_for_await_statement(&mut self, stmt_idx: NodeIndex) {
        CheckerState::check_for_await_statement(self, stmt_idx);
    }

    fn check_truthy_or_falsy(&mut self, node_idx: NodeIndex) {
        CheckerState::check_truthy_or_falsy(self, node_idx);
    }

    fn check_callable_truthiness(&mut self, cond_expr: NodeIndex, body: Option<NodeIndex>) {
        CheckerState::check_callable_truthiness(self, cond_expr, body);
    }

    fn is_true_condition(&self, condition_idx: NodeIndex) -> bool {
        CheckerState::is_true_condition(self, condition_idx)
    }

    fn is_false_condition(&self, condition_idx: NodeIndex) -> bool {
        CheckerState::is_false_condition(self, condition_idx)
    }

    fn report_unreachable_statement(&mut self, stmt_idx: NodeIndex) {
        if !self.ctx.is_unreachable {
            return;
        }

        // Delegate to a helper that checks should_skip.
        // Match TSC's isStatementKindThatDoesNotAffectControlFlow:
        // - Skip type-only declarations (interface, type alias)
        // - Skip function declarations (hoisted)
        // - Skip const enums when preserveConstEnums is false (erased, no runtime code)
        // - Skip non-instantiated module declarations (ambient/declare modules)
        // - Skip empty statements and blocks
        // - Skip var declarations without initializers (hoisted)
        let should_skip = if let Some(node) = self.ctx.arena.get(stmt_idx) {
            if node.kind == syntax_kind_ext::EMPTY_STATEMENT
                || node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                || node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                || node.kind == syntax_kind_ext::BLOCK
            {
                true
            } else if node.kind == syntax_kind_ext::ENUM_DECLARATION {
                // Const enums are erased unless preserveConstEnums is set
                if let Some(enum_data) = self.ctx.arena.get_enum(node) {
                    let is_const = self
                        .ctx
                        .arena
                        .has_modifier(&enum_data.modifiers, SyntaxKind::ConstKeyword);
                    is_const && !self.ctx.compiler_options.preserve_const_enums
                } else {
                    false
                }
            } else if node.kind == syntax_kind_ext::MODULE_DECLARATION {
                // Skip only ambient (declare) modules; namespace with executable code is instantiated
                if let Some(module_data) = self.ctx.arena.get_module(node) {
                    let is_ambient = self
                        .ctx
                        .arena
                        .has_modifier(&module_data.modifiers, SyntaxKind::DeclareKeyword);
                    is_ambient || self.ctx.arena.get(module_data.body).is_none()
                } else {
                    false
                }
            } else {
                CheckerState::is_var_without_initializer(self, stmt_idx, node)
            }
        } else {
            false
        };

        if !should_skip && !self.ctx.has_reported_unreachable {
            if self.ctx.compiler_options.allow_unreachable_code != Some(false) {
                return;
            }
            self.error_at_node(
                stmt_idx,
                crate::diagnostics::diagnostic_messages::UNREACHABLE_CODE_DETECTED,
                crate::diagnostics::diagnostic_codes::UNREACHABLE_CODE_DETECTED,
            );
            self.ctx.has_reported_unreachable = true;
        }
    }

    fn check_for_in_expression_type(&mut self, expr_type: TypeId, expression: NodeIndex) {
        CheckerState::check_for_in_expression_type(self, expr_type, expression);
    }

    fn compute_for_in_variable_type(&mut self, expr_type: TypeId) -> TypeId {
        CheckerState::compute_for_in_variable_type(self, expr_type)
    }

    fn assign_for_in_of_initializer_types(
        &mut self,
        decl_list_idx: NodeIndex,
        loop_var_type: TypeId,
        is_for_in: bool,
    ) {
        CheckerState::assign_for_in_of_initializer_types(
            self,
            decl_list_idx,
            loop_var_type,
            is_for_in,
        );
    }

    fn for_of_element_type(&mut self, expr_type: TypeId, is_async: bool) -> TypeId {
        CheckerState::for_of_element_type(self, expr_type, is_async)
    }

    fn check_for_of_iterability(
        &mut self,
        expr_type: TypeId,
        expr_idx: NodeIndex,
        await_modifier: bool,
    ) {
        CheckerState::check_for_of_iterability(self, expr_type, expr_idx, await_modifier);
    }

    fn check_for_in_of_expression_initializer(
        &mut self,
        initializer: NodeIndex,
        element_type: TypeId,
        is_for_of: bool,
        has_await_modifier: bool,
    ) {
        CheckerState::check_for_in_of_expression_initializer(
            self,
            initializer,
            element_type,
            is_for_of,
            has_await_modifier,
        );
    }

    fn check_for_in_destructuring_pattern(&mut self, initializer: NodeIndex) {
        CheckerState::check_for_in_destructuring_pattern(self, initializer);
    }

    fn check_for_in_expression_destructuring(&mut self, initializer: NodeIndex) {
        CheckerState::check_for_in_expression_destructuring(self, initializer);
    }

    fn begin_for_of_self_reference_tracking(&mut self, decl_list_idx: NodeIndex) -> usize {
        CheckerState::begin_for_of_self_reference_tracking(self, decl_list_idx)
    }

    fn end_for_of_self_reference_tracking(&mut self, tracked_symbol_count: usize) {
        CheckerState::end_for_of_self_reference_tracking(self, tracked_symbol_count);
    }

    fn check_for_of_self_reference_circularity(
        &mut self,
        decl_list_idx: NodeIndex,
        expression_idx: NodeIndex,
    ) {
        CheckerState::check_for_of_self_reference_circularity(self, decl_list_idx, expression_idx);
    }

    fn check_statement(&mut self, stmt_idx: NodeIndex) {
        // This calls back to the main check_statement which will delegate to StatementChecker
        CheckerState::check_statement(self, stmt_idx);
    }

    fn check_statement_with_request(&mut self, stmt_idx: NodeIndex, request: &TypingRequest) {
        CheckerState::check_statement_with_request(self, stmt_idx, request);
    }

    fn check_switch_exhaustiveness(
        &mut self,
        _stmt_idx: NodeIndex,
        expression: NodeIndex,
        _case_block: NodeIndex,
        has_default: bool,
    ) {
        // If there's a default clause, the switch is syntactically exhaustive
        if has_default {
            return;
        }

        // Evaluate discriminant type (populates type caches needed by flow analysis)
        let _ = self.get_type_of_node(expression);

        // Note: exhaustiveness narrowing for switch is handled at the function level
        // in control flow analysis (TS2366), not at the switch statement level.
        //
        // This is because:
        // 1. Code after the switch might handle missing cases
        // 2. The return type might accept undefined (e.g., number | undefined)
        // 3. Exhaustiveness must be checked in the context of the entire function
        //
        // The FlowAnalyzer uses no_match_type to correctly narrow types within
        // subsequent code blocks, but the error emission happens elsewhere.
    }

    fn get_type_of_case_expression(&mut self, case_expr: NodeIndex, switch_type: TypeId) -> TypeId {
        // Set the switch expression type as contextual type for the case expression.
        // In tsc, `getContextualType` returns the switch discriminant type for case
        // clause expressions. This enables excess property checking (TS2353) when
        // the case expression is an object literal.
        let request = TypingRequest::with_contextual_type(switch_type);
        self.get_type_of_node_with_request(case_expr, &request)
    }

    fn get_type_of_case_expression_with_request(
        &mut self,
        case_expr: NodeIndex,
        switch_type: TypeId,
        request: &TypingRequest,
    ) -> TypeId {
        let request = request.read().contextual(switch_type);
        self.get_type_of_node_with_request(case_expr, &request)
    }

    fn check_switch_case_comparable(
        &mut self,
        switch_type: TypeId,
        case_type: TypeId,
        switch_expr: NodeIndex,
        case_expr: NodeIndex,
    ) {
        // Skip if either type is error/any/unknown to avoid cascade errors
        if switch_type == TypeId::ERROR
            || case_type == TypeId::ERROR
            || switch_type == TypeId::ANY
            || case_type == TypeId::ANY
            || switch_type == TypeId::UNKNOWN
            || case_type == TypeId::UNKNOWN
        {
            return;
        }

        // Check excess properties for object literal case expressions (TS2353).
        // In tsc, the switch discriminant type serves as the contextual type for
        // case expressions. When a case expression is an object literal, tsc checks
        // for excess properties against the switch type and emits TS2353 instead of
        // TS2678. If excess property errors are emitted, skip the comparability check.
        if let Some(case_node) = self.ctx.arena.get(case_expr)
            && case_node.kind == tsz_parser::parser::syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
        {
            let diag_count_before = self.ctx.diagnostics.len();
            self.check_object_literal_excess_properties(case_type, switch_type, case_expr);
            if self.ctx.diagnostics.len() > diag_count_before {
                // Excess property errors were emitted (TS2353); skip TS2678.
                return;
            }
        }

        // Use literal type for the switch expression if available, since
        // get_type_of_node widens literals (e.g., 12 -> number).
        // tsc's checkExpression preserves literal types for comparability checks.
        let effective_switch_type = self
            .literal_type_from_initializer(switch_expr)
            .unwrap_or(switch_type);

        // Use literal type for the case expression if available, since
        // get_type_of_node widens literals (e.g., "c" -> string).
        let effective_case_type = self
            .literal_type_from_initializer(case_expr)
            .unwrap_or(case_type);

        // Check if the types are comparable (assignable in either direction).
        // Types are comparable if they overlap — i.e., at least one direction works.
        // For example, "a" is comparable to "a" | "b" | "c" because "a" <: union.
        // TypeScript unconditionally allows 'null' and 'undefined' as the case type.
        let is_comparable = effective_case_type == tsz_solver::TypeId::NULL
            || effective_case_type == tsz_solver::TypeId::UNDEFINED
            || self.is_type_comparable_to(effective_case_type, effective_switch_type);

        if !is_comparable {
            // TS2678: Type 'X' is not comparable to type 'Y'
            let case_str = self.format_type(effective_case_type);
            let switch_str = self.format_type(effective_switch_type);
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node_msg(
                case_expr,
                diagnostic_codes::TYPE_IS_NOT_COMPARABLE_TO_TYPE,
                &[&case_str, &switch_str],
            );
        }
    }

    fn check_with_statement(&mut self, stmt_idx: NodeIndex) {
        CheckerState::check_with_statement(self, stmt_idx);
    }

    fn check_break_statement(&mut self, stmt_idx: NodeIndex) {
        CheckerState::check_break_statement(self, stmt_idx);
    }

    fn check_continue_statement(&mut self, stmt_idx: NodeIndex) {
        CheckerState::check_continue_statement(self, stmt_idx);
    }

    fn is_unreachable(&self) -> bool {
        self.ctx.is_unreachable
    }

    fn set_unreachable(&mut self, value: bool) {
        self.ctx.is_unreachable = value;
    }

    fn has_reported_unreachable(&self) -> bool {
        self.ctx.has_reported_unreachable
    }

    fn set_reported_unreachable(&mut self, value: bool) {
        self.ctx.has_reported_unreachable = value;
    }

    fn statement_falls_through(&mut self, stmt_idx: NodeIndex) -> bool {
        CheckerState::statement_falls_through(self, stmt_idx)
    }

    fn call_expression_terminates_control_flow(&mut self, expr_idx: NodeIndex) -> bool {
        CheckerState::call_expression_terminates_control_flow(self, expr_idx)
    }

    fn report_unreachable_code_at_node(&mut self, node_idx: NodeIndex) {
        if self.ctx.compiler_options.allow_unreachable_code != Some(false) {
            return;
        }
        self.error_at_node(
            node_idx,
            crate::diagnostics::diagnostic_messages::UNREACHABLE_CODE_DETECTED,
            crate::diagnostics::diagnostic_codes::UNREACHABLE_CODE_DETECTED,
        );
    }

    fn enter_iteration_statement(&mut self) {
        self.ctx.iteration_depth += 1;
    }

    fn leave_iteration_statement(&mut self) {
        self.ctx.iteration_depth = self.ctx.iteration_depth.saturating_sub(1);
    }

    fn enter_switch_statement(&mut self) {
        self.ctx.switch_depth += 1;
    }

    fn leave_switch_statement(&mut self) {
        self.ctx.switch_depth = self.ctx.switch_depth.saturating_sub(1);
    }

    fn save_and_reset_control_flow_context(&mut self) -> (u32, u32, bool) {
        let saved = (
            self.ctx.iteration_depth,
            self.ctx.switch_depth,
            self.ctx.had_outer_loop,
        );
        // If we were in a loop/switch, or already had an outer loop, mark it
        if self.ctx.iteration_depth > 0 || self.ctx.switch_depth > 0 || self.ctx.had_outer_loop {
            self.ctx.had_outer_loop = true;
        }
        self.ctx.iteration_depth = 0;
        self.ctx.switch_depth = 0;
        saved
    }

    fn restore_control_flow_context(&mut self, saved: (u32, u32, bool)) {
        self.ctx.iteration_depth = saved.0;
        self.ctx.switch_depth = saved.1;
        self.ctx.had_outer_loop = saved.2;
    }

    fn enter_labeled_statement(
        &mut self,
        label: String,
        is_iteration: bool,
        label_node: NodeIndex,
    ) {
        self.ctx.label_stack.push(crate::context::LabelInfo {
            name: label,
            is_iteration,
            function_depth: self.ctx.function_depth,
            referenced: false,
            label_node,
        });
    }

    fn leave_labeled_statement(&mut self) {
        if let Some(label_info) = self.ctx.label_stack.pop() {
            // TS7028: Unused label
            if !label_info.referenced
                && self.ctx.compiler_options.allow_unused_labels == Some(false)
            {
                self.error_at_node(
                    label_info.label_node,
                    crate::diagnostics::diagnostic_messages::UNUSED_LABEL,
                    crate::diagnostics::diagnostic_codes::UNUSED_LABEL,
                );
            }
        }
    }

    fn get_node_text(&self, idx: NodeIndex) -> Option<String> {
        // For identifiers (like label names), get the identifier data and resolve the text
        let ident = self.ctx.arena.get_identifier_at(idx)?;
        // Use the resolved text from the identifier data
        Some(self.ctx.arena.resolve_identifier_text(ident).to_string())
    }

    fn check_declaration_in_statement_position(&mut self, stmt_idx: NodeIndex) {
        use tsz_parser::parser::node_flags;

        // Unwrap through labeled statements to find the actual inner statement.
        // e.g. `if (true) label: const c8 = 0;` — tsc reports TS1156 on `c8`.
        let mut inner_idx = stmt_idx;
        loop {
            let Some(inner_node) = self.ctx.arena.get(inner_idx) else {
                return;
            };
            if inner_node.kind == syntax_kind_ext::LABELED_STATEMENT
                && let Some(labeled) = self.ctx.arena.get_labeled_statement(inner_node)
            {
                inner_idx = labeled.statement;
                continue;
            }
            break;
        }

        let Some(node) = self.ctx.arena.get(inner_idx) else {
            return;
        };

        // TS1156: '{0}' declarations can only be declared inside a block.
        // This fires when a const/let/interface/type declaration appears as
        // the body of a control flow statement (if/while/for) without braces.
        let decl_kind = match node.kind {
            syntax_kind_ext::INTERFACE_DECLARATION => Some("interface"),
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => Some("type"),
            syntax_kind_ext::VARIABLE_STATEMENT => {
                // Check the VariableDeclarationList for const/let flags
                if let Some(var_data) = self.ctx.arena.get_variable(node) {
                    let list_idx = var_data
                        .declarations
                        .nodes
                        .first()
                        .copied()
                        .unwrap_or(NodeIndex::NONE);
                    if let Some(list_node) = self.ctx.arena.get(list_idx) {
                        let flags = list_node.flags as u32;
                        // Check USING first — AWAIT_USING (6) includes CONST bit
                        if (flags & node_flags::AWAIT_USING) == node_flags::AWAIT_USING {
                            Some("await using")
                        } else if flags & node_flags::USING != 0 {
                            Some("using")
                        } else if flags & node_flags::CONST != 0 {
                            Some("const")
                        } else if flags & node_flags::LET != 0 {
                            Some("let")
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some(kind_name) = decl_kind {
            // tsc does not emit TS1156 when there are parse errors on the same
            // construct (e.g. TS1128 already reported for the malformed syntax).
            if self.has_parse_errors() {
                return;
            }
            let msg = format!("'{kind_name}' declarations can only be declared inside a block.");

            // tsc reports TS1156 at the declaration's name identifier, not the keyword.
            // For `type Foo = ...`, tsc points at `Foo`, not `type`.
            let error_node = self
                .get_declaration_name_node(inner_idx)
                .unwrap_or(inner_idx);

            self.error_at_node(
                error_node,
                &msg,
                crate::diagnostics::diagnostic_codes::DECLARATIONS_CAN_ONLY_BE_DECLARED_INSIDE_A_BLOCK,
            );
        }
    }

    fn check_label_on_declaration(&mut self, label_idx: NodeIndex, statement_idx: NodeIndex) {
        // TS1344: In strict mode with target >= ES2015, a label is not allowed
        // before declaration statements or variable statements.
        // This matches TSC's checkStrictModeLabeledStatement in binder.ts.
        if !self.ctx.compiler_options.target.supports_es2015() {
            return;
        }
        if !self.is_strict_mode_for_node(label_idx) {
            return;
        }

        let Some(stmt_node) = self.ctx.arena.get(statement_idx) else {
            return;
        };

        // isDeclarationStatement || isVariableStatement
        let is_declaration_or_variable = matches!(
            stmt_node.kind,
            syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::INTERFACE_DECLARATION
                | syntax_kind_ext::TYPE_ALIAS_DECLARATION
                | syntax_kind_ext::ENUM_DECLARATION
                | syntax_kind_ext::MODULE_DECLARATION
                | syntax_kind_ext::IMPORT_DECLARATION
                | syntax_kind_ext::EXPORT_DECLARATION
                | syntax_kind_ext::VARIABLE_STATEMENT
        );

        if is_declaration_or_variable {
            self.error_at_node(
                label_idx,
                "'A label is not allowed here.",
                crate::diagnostics::diagnostic_codes::A_LABEL_IS_NOT_ALLOWED_HERE,
            );
        }
    }

    fn check_grammar_module_element_context(&mut self, stmt_idx: NodeIndex) -> bool {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        // Suppress grammar errors when file has parse errors (matches tsc behavior)
        if self.ctx.has_syntax_parse_errors {
            return false;
        }

        // Check if the parent is a valid module-element context (SourceFile or ModuleBlock).
        // For import-equals inside `export import X = N;`, the direct parent is
        // EXPORT_DECLARATION — look through it to the grandparent.
        let parent_idx = self.ctx.arena.get_extended(stmt_idx).map(|ext| ext.parent);
        let parent_kind = parent_idx
            .and_then(|p| self.ctx.arena.get(p))
            .map(|p| p.kind);
        let effective_parent_kind = if matches!(parent_kind, Some(k) if k == syntax_kind_ext::EXPORT_DECLARATION)
        {
            parent_idx
                .and_then(|p| self.ctx.arena.get_extended(p))
                .and_then(|ext| self.ctx.arena.get(ext.parent))
                .map(|p| p.kind)
        } else {
            parent_kind
        };

        let is_valid_context = match effective_parent_kind {
            Some(k) if k == syntax_kind_ext::SOURCE_FILE || k == syntax_kind_ext::MODULE_BLOCK => {
                true
            }
            None => true, // Top-level
            _ => false,
        };

        if is_valid_context {
            return false;
        }

        // Determine which error to emit based on the statement kind
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };

        let (message, code) = match node.kind {
            k if k == syntax_kind_ext::IMPORT_DECLARATION
                || k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION =>
            {
                // In JS files, use TS1473 "...top level of a module" (no namespaces in JS).
                // In TS files, use TS1232 "...top level of a namespace or module".
                if self.is_js_file() {
                    (
                        diagnostic_messages::AN_IMPORT_DECLARATION_CAN_ONLY_BE_USED_AT_THE_TOP_LEVEL_OF_A_MODULE,
                        diagnostic_codes::AN_IMPORT_DECLARATION_CAN_ONLY_BE_USED_AT_THE_TOP_LEVEL_OF_A_MODULE,
                    )
                } else {
                    (
                        diagnostic_messages::AN_IMPORT_DECLARATION_CAN_ONLY_BE_USED_AT_THE_TOP_LEVEL_OF_A_NAMESPACE_OR_MODULE,
                        diagnostic_codes::AN_IMPORT_DECLARATION_CAN_ONLY_BE_USED_AT_THE_TOP_LEVEL_OF_A_NAMESPACE_OR_MODULE,
                    )
                }
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                if let Some(export_decl) = self.ctx.arena.get_export_decl_at(stmt_idx) {
                    // Check if the export wraps a class/function declaration
                    // (e.g., `export function foo()`, `export default class C`).
                    // In tsc, these get TS1184 "Modifiers cannot appear here" instead.
                    let clause_kind = self
                        .ctx
                        .arena
                        .get(export_decl.export_clause)
                        .map(|n| n.kind);
                    let is_class_or_function_or_variable = matches!(
                        clause_kind,
                        Some(k) if k == syntax_kind_ext::CLASS_DECLARATION
                            || k == syntax_kind_ext::CLASS_EXPRESSION
                            || k == syntax_kind_ext::FUNCTION_DECLARATION
                            || k == syntax_kind_ext::VARIABLE_STATEMENT
                    );

                    let is_namespace_or_module = matches!(
                        clause_kind,
                        Some(k) if k == syntax_kind_ext::MODULE_DECLARATION
                    );

                    if is_namespace_or_module {
                        // Namespace/module gets its own error (TS1235/TS1234) from
                        // check_module_declaration. Don't also emit TS1233 for the export.
                        return false;
                    } else if is_class_or_function_or_variable {
                        // TS1184: Modifiers cannot appear here.
                        (
                            diagnostic_messages::MODIFIERS_CANNOT_APPEAR_HERE,
                            diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
                        )
                    } else if export_decl.is_default_export {
                        // TS1258: A default export must be at the top level
                        (
                            diagnostic_messages::A_DEFAULT_EXPORT_MUST_BE_AT_THE_TOP_LEVEL_OF_A_FILE_OR_MODULE_DECLARATION,
                            diagnostic_codes::A_DEFAULT_EXPORT_MUST_BE_AT_THE_TOP_LEVEL_OF_A_FILE_OR_MODULE_DECLARATION,
                        )
                    } else {
                        // TS1233: An export declaration can only be used at the top level
                        (
                            diagnostic_messages::AN_EXPORT_DECLARATION_CAN_ONLY_BE_USED_AT_THE_TOP_LEVEL_OF_A_NAMESPACE_OR_MODULE,
                            diagnostic_codes::AN_EXPORT_DECLARATION_CAN_ONLY_BE_USED_AT_THE_TOP_LEVEL_OF_A_NAMESPACE_OR_MODULE,
                        )
                    }
                } else {
                    (
                        diagnostic_messages::AN_EXPORT_DECLARATION_CAN_ONLY_BE_USED_AT_THE_TOP_LEVEL_OF_A_NAMESPACE_OR_MODULE,
                        diagnostic_codes::AN_EXPORT_DECLARATION_CAN_ONLY_BE_USED_AT_THE_TOP_LEVEL_OF_A_NAMESPACE_OR_MODULE,
                    )
                }
            }
            k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                (
                    diagnostic_messages::AN_EXPORT_ASSIGNMENT_MUST_BE_AT_THE_TOP_LEVEL_OF_A_FILE_OR_MODULE_DECLARATION,
                    diagnostic_codes::AN_EXPORT_ASSIGNMENT_MUST_BE_AT_THE_TOP_LEVEL_OF_A_FILE_OR_MODULE_DECLARATION,
                )
            }
            _ => return false,
        };

        self.error_at_node(stmt_idx, message, code);
        true
    }
}
