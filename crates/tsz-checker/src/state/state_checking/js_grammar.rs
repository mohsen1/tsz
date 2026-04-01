//! JS grammar checking (`TS8xxx` errors).
//!
//! Emits errors for TypeScript-only syntax used in JavaScript files.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    /// Check all statements in a JS file for TypeScript-only syntax.
    /// Emits `TS8xxx` errors for constructs that are not valid in JavaScript files.
    pub(crate) fn check_js_grammar_statements(&mut self, statements: &[NodeIndex]) {
        for &stmt_idx in statements {
            self.check_js_grammar_statement(stmt_idx);
        }
    }

    /// TS8022: Check for orphaned `@extends`/`@augments` tags not attached to a class.
    pub(crate) fn check_orphaned_extends_tags(&mut self, statements: &[NodeIndex]) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        let orphaned = self.find_orphaned_extends_tags_for_statements(statements);
        for (tag_name, pos, len) in orphaned {
            let message = format_message(
                diagnostic_messages::JSDOC_IS_NOT_ATTACHED_TO_A_CLASS,
                &[tag_name],
            );
            self.ctx.error(
                pos,
                len,
                message,
                diagnostic_codes::JSDOC_IS_NOT_ATTACHED_TO_A_CLASS,
            );
        }
    }

    /// Check a single statement for TypeScript-only syntax in JS files.
    fn check_js_grammar_statement(&mut self, stmt_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        match node.kind {
            // TS8008: Type aliases can only be used in TypeScript files
            // TSC anchors the error at the type alias name, not the whole statement.
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                let error_node = self
                    .ctx
                    .arena
                    .get_type_alias(node)
                    .map(|d| d.name)
                    .unwrap_or(stmt_idx);
                self.error_at_node(
                    error_node,
                    diagnostic_messages::TYPE_ALIASES_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    diagnostic_codes::TYPE_ALIASES_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                );
            }

            // TS8006: 'interface'/'enum'/'module'/'namespace' declarations
            // TSC anchors the error at the declaration name, not the whole statement.
            syntax_kind_ext::INTERFACE_DECLARATION => {
                let error_node = self
                    .ctx
                    .arena
                    .get_interface(node)
                    .map_or(stmt_idx, |i| i.name);
                self.error_ts_only_declaration("interface", error_node);
            }

            syntax_kind_ext::ENUM_DECLARATION => {
                let error_node = self.ctx.arena.get_enum(node).map_or(stmt_idx, |e| e.name);
                self.error_ts_only_declaration("enum", error_node);
            }

            syntax_kind_ext::MODULE_DECLARATION => {
                let keyword = self.get_module_keyword(stmt_idx, node);
                let error_node = self.ctx.arena.get_module(node).map_or(stmt_idx, |m| m.name);
                self.error_ts_only_declaration(keyword, error_node);
            }

            // TS8002: 'import ... =' can only be used in TypeScript files
            syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                self.error_at_node(
                    stmt_idx,
                    diagnostic_messages::IMPORT_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    diagnostic_codes::IMPORT_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                );
            }

            // TS8003: 'export =' can only be used in TypeScript files
            syntax_kind_ext::EXPORT_ASSIGNMENT => {
                self.error_at_node(
                    stmt_idx,
                    diagnostic_messages::EXPORT_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    diagnostic_codes::EXPORT_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                );
            }

            // Function declarations: check for type params, return type, overloads, param types
            syntax_kind_ext::FUNCTION_DECLARATION => {
                self.check_js_grammar_function(stmt_idx, node);
            }

            // Class declarations: check for type params, implements, abstract, members
            syntax_kind_ext::CLASS_DECLARATION => {
                self.check_js_grammar_class(stmt_idx, node);
            }

            // Variable statements: check for declare modifier, type annotations
            syntax_kind_ext::VARIABLE_STATEMENT => {
                self.check_js_grammar_variable_statement(stmt_idx, node);
            }

            // Export declarations may wrap other declarations
            syntax_kind_ext::EXPORT_DECLARATION => {
                if let Some(export_decl) = self.ctx.arena.get_export_decl_at(stmt_idx) {
                    // TS8006: `export type * from '...'` in JS files.
                    // tsc only emits TS8006 for namespace re-exports with `type` keyword,
                    // NOT for `export type { A }` (named specifiers with whole-declaration type).
                    if export_decl.is_type_only && export_decl.module_specifier.is_some() {
                        let message = format_message(
                            diagnostic_messages::DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                            &["export type"],
                        );
                        self.error_at_node(
                            stmt_idx,
                            &message,
                            diagnostic_codes::DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                        );
                    } else if export_decl.export_clause.is_some() {
                        let inner = export_decl.export_clause;
                        let inner_kind = self.ctx.arena.get(inner).map(|n| n.kind);
                        // For `export import X = require(...)`, emit TS8002 at the
                        // outer EXPORT_DECLARATION node so the span starts at `export`
                        // (matching tsc column offset).
                        if inner_kind == Some(syntax_kind_ext::IMPORT_EQUALS_DECLARATION) {
                            self.error_at_node(
                                stmt_idx,
                                diagnostic_messages::IMPORT_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                                diagnostic_codes::IMPORT_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                            );
                        } else if inner_kind == Some(syntax_kind_ext::NAMED_EXPORTS) {
                            // Check per-specifier `export { type foo }` in JS files
                            self.check_js_grammar_export_specifiers(inner);
                            self.check_js_grammar_statement(inner);
                        } else {
                            self.check_js_grammar_statement(inner);
                        }
                    }
                }
            }

            // Expression statements may contain function expressions and arrow functions.
            syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) {
                    self.check_js_grammar_expression(expr_stmt.expression);
                }
            }

            _ => {}
        }
    }

    /// Check per-specifier `export { type foo }` in JS files.
    /// Emits TS8006 for each specifier that has `is_type_only: true`.
    fn check_js_grammar_export_specifiers(&mut self, named_exports_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        let Some(named_exports_node) = self.ctx.arena.get(named_exports_idx) else {
            return;
        };
        let Some(named_imports_data) = self.ctx.arena.get_named_imports(named_exports_node) else {
            return;
        };
        for &specifier_idx in &named_imports_data.elements.nodes {
            let Some(specifier_node) = self.ctx.arena.get(specifier_idx) else {
                continue;
            };
            let Some(specifier_data) = self.ctx.arena.get_specifier(specifier_node) else {
                continue;
            };
            if specifier_data.is_type_only {
                let message = format_message(
                    diagnostic_messages::DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    &["export...type"],
                );
                self.error_at_node(
                    specifier_idx,
                    &message,
                    diagnostic_codes::DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                );
            }
        }
    }

    fn check_js_grammar_expression(&mut self, expr_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return;
        };

        if node.is_function_like() {
            self.check_js_grammar_function(expr_idx, node);
        }

        if node.kind == syntax_kind_ext::AS_EXPRESSION
            && let Some(assertion) = self.ctx.arena.get_type_assertion(node)
        {
            self.error_at_node(
                assertion.type_node,
                diagnostic_messages::TYPE_ASSERTION_EXPRESSIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                diagnostic_codes::TYPE_ASSERTION_EXPRESSIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
            );
        }

        for child_idx in self.ctx.arena.get_children(expr_idx) {
            if child_idx.is_some() {
                self.check_js_grammar_expression(child_idx);
            }
        }
    }

    /// Check function declaration for JS grammar errors.
    pub(crate) fn check_js_grammar_function(
        &mut self,
        func_idx: NodeIndex,
        node: &tsz_parser::parser::node::Node,
    ) {
        let Some(func) = self.ctx.arena.get_function(node) else {
            return;
        };

        self.error_if_ts_only_modifier(&func.modifiers, SyntaxKind::DeclareKeyword, "declare");
        self.error_if_ts_only_type_params(&func.type_parameters);
        self.error_if_ts_only_type_annotation(func.type_annotation);

        // TS8017: Function overload (function without body)
        let is_overload = func.body.is_none() && node.kind == syntax_kind_ext::FUNCTION_DECLARATION;
        self.error_if_ts_only_signature_without_body(is_overload, func_idx);

        // Check parameter types and modifiers
        self.check_js_grammar_parameters(&func.parameters.nodes);
    }

    /// Check class declaration for JS grammar errors.
    fn check_js_grammar_class(
        &mut self,
        _class_idx: NodeIndex,
        node: &tsz_parser::parser::node::Node,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        let Some(class) = self.ctx.arena.get_class(node) else {
            return;
        };

        self.error_if_ts_only_type_params(&class.type_parameters);

        // TS8005: 'implements' clause
        if let Some(ref heritage_clauses) = class.heritage_clauses {
            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                    continue;
                };
                if clause_node.kind != syntax_kind_ext::HERITAGE_CLAUSE {
                    continue;
                }
                let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                    continue;
                };

                if heritage.token == SyntaxKind::ImplementsKeyword as u16 {
                    self.error_at_node(
                        clause_idx,
                        diagnostic_messages::IMPLEMENTS_CLAUSES_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                        diagnostic_codes::IMPLEMENTS_CLAUSES_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    );
                }

                {
                    for &type_idx in &heritage.types.nodes {
                        let Some(type_node) = self.ctx.arena.get(type_idx) else {
                            continue;
                        };
                        if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node)
                            && let Some(type_args) = &expr_type_args.type_arguments
                            && let Some(&first_type_arg) = type_args.nodes.first()
                        {
                            self.error_at_node(
                                first_type_arg,
                                diagnostic_messages::TYPE_ARGUMENTS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                                diagnostic_codes::TYPE_ARGUMENTS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                            );
                        }
                    }
                }
            }
        }

        self.error_if_ts_only_modifier(&class.modifiers, SyntaxKind::AbstractKeyword, "abstract");
        self.error_if_ts_only_modifier(&class.modifiers, SyntaxKind::DeclareKeyword, "declare");

        // Check class members for JS grammar errors
        for &member_idx in &class.members.nodes {
            self.check_js_grammar_class_member(member_idx);
        }
    }

    /// Helper: Report TS8009 error for a TypeScript-only modifier (abstract, override, etc.).
    fn error_if_ts_only_modifier(
        &mut self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
        kind: SyntaxKind,
        name: &str,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        if self.has_modifier_kind(modifiers, kind) {
            let message = format_message(
                diagnostic_messages::THE_MODIFIER_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                &[name],
            );
            if let Some(mod_idx) = self.get_modifier_index(modifiers, kind as u16) {
                self.error_at_node(
                    mod_idx,
                    &message,
                    diagnostic_codes::THE_MODIFIER_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                );
            }
        }
    }

    /// Helper: Report TS8010 error for a TypeScript-only type annotation.
    fn error_if_ts_only_type_annotation(&mut self, type_annotation: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        if type_annotation.is_some() {
            self.error_at_node(
                type_annotation,
                diagnostic_messages::TYPE_ANNOTATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                diagnostic_codes::TYPE_ANNOTATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
            );
        }
    }

    /// Helper: Report TS8004 error for TypeScript-only type parameters.
    pub(crate) fn error_if_ts_only_type_params(
        &mut self,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        if let Some(type_params) = type_parameters
            && !type_params.nodes.is_empty()
        {
            self.error_at_node(
                type_params.nodes[0],
                diagnostic_messages::TYPE_PARAMETER_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                diagnostic_codes::TYPE_PARAMETER_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
            );
        }
    }

    /// Helper: Report TS8017 error for a signature declaration without a body.
    fn error_if_ts_only_signature_without_body(&mut self, has_no_body: bool, node_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        if has_no_body {
            self.error_at_node(
                node_idx,
                diagnostic_messages::SIGNATURE_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                diagnostic_codes::SIGNATURE_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
            );
        }
    }

    /// Helper: Report TS8009 error for optional token (?) in JavaScript.
    fn error_if_ts_only_optional(&mut self, has_question_token: bool, node_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        if has_question_token {
            let message = format_message(
                diagnostic_messages::THE_MODIFIER_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                &["?"],
            );
            if let Some(node) = self.ctx.arena.get(node_idx) {
                let optional_start = match node.kind {
                    syntax_kind_ext::METHOD_DECLARATION => self
                        .ctx
                        .arena
                        .get_method_decl(node)
                        .and_then(|method| self.ctx.arena.get(method.name))
                        .map(|name| name.end.saturating_sub(1)),
                    syntax_kind_ext::PROPERTY_DECLARATION => self
                        .ctx
                        .arena
                        .get_property_decl(node)
                        .and_then(|prop| self.ctx.arena.get(prop.name))
                        .map(|name| name.end.saturating_sub(1)),
                    _ => None,
                };
                if let Some(start) = optional_start {
                    self.error_at_position(
                        start,
                        1,
                        &message,
                        diagnostic_codes::THE_MODIFIER_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    );
                    return;
                }
            }
            self.error_at_node(
                node_idx,
                &message,
                diagnostic_codes::THE_MODIFIER_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
            );
        }
    }

    /// Helper: Report TS8006 error for TypeScript-only declarations (interface, enum, module, namespace).
    fn error_ts_only_declaration(&mut self, keyword: &str, node_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        let message = format_message(
            diagnostic_messages::DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
            &[keyword],
        );
        self.error_at_node(
            node_idx,
            &message,
            diagnostic_codes::DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
        );
    }

    /// Helper: Determine if a module declaration uses 'module' or 'namespace' keyword.
    fn get_module_keyword(
        &self,
        node_idx: NodeIndex,
        node: &tsz_parser::parser::node::Node,
    ) -> &'static str {
        let Some(module) = self.ctx.arena.get_module(node) else {
            return "module";
        };

        let Some(name_node) = self.ctx.arena.get(module.name) else {
            return "module";
        };

        // If name is a string literal, it's `module "foo"`, otherwise `namespace Foo`
        if name_node.kind == SyntaxKind::StringLiteral as u16 {
            return "module";
        }

        // Check source text for module vs namespace keyword
        let node_text = self.node_text(node_idx).unwrap_or_default();
        if node_text.starts_with("namespace") || node_text.contains("namespace ") {
            "namespace"
        } else {
            "module"
        }
    }

    /// Check a class member for JS grammar errors.
    pub(crate) fn check_js_grammar_class_member(&mut self, member_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        match node.kind {
            syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.ctx.arena.get_method_decl(node) {
                    self.check_js_grammar_accessibility_modifier(&method.modifiers, member_idx);
                    self.error_if_ts_only_modifier(
                        &method.modifiers,
                        SyntaxKind::AbstractKeyword,
                        "abstract",
                    );
                    self.error_if_ts_only_modifier(
                        &method.modifiers,
                        SyntaxKind::OverrideKeyword,
                        "override",
                    );
                    self.error_if_ts_only_modifier(
                        &method.modifiers,
                        SyntaxKind::ConstKeyword,
                        "const",
                    );
                    self.error_if_ts_only_type_params(&method.type_parameters);
                    self.error_if_ts_only_type_annotation(method.type_annotation);
                    self.error_if_ts_only_signature_without_body(method.body.is_none(), member_idx);
                    self.error_if_ts_only_optional(method.question_token, member_idx);
                    self.check_js_grammar_parameters(&method.parameters.nodes);
                }
            }

            syntax_kind_ext::CONSTRUCTOR => {
                if let Some(ctor) = self.ctx.arena.get_constructor(node) {
                    self.check_js_grammar_accessibility_modifier(&ctor.modifiers, member_idx);
                    self.error_if_ts_only_signature_without_body(ctor.body.is_none(), member_idx);
                    self.check_js_grammar_parameters(&ctor.parameters.nodes);
                }
            }

            syntax_kind_ext::PROPERTY_DECLARATION => {
                if let Some(prop) = self.ctx.arena.get_property_decl(node) {
                    self.error_if_ts_only_optional(prop.question_token, member_idx);
                    self.error_if_ts_only_modifier(
                        &prop.modifiers,
                        SyntaxKind::AbstractKeyword,
                        "abstract",
                    );
                    self.error_if_ts_only_modifier(
                        &prop.modifiers,
                        SyntaxKind::ConstKeyword,
                        "const",
                    );
                    self.error_if_ts_only_modifier(
                        &prop.modifiers,
                        SyntaxKind::ExportKeyword,
                        "export",
                    );
                    self.error_if_ts_only_modifier(
                        &prop.modifiers,
                        SyntaxKind::AsyncKeyword,
                        "async",
                    );
                    self.error_if_ts_only_type_annotation(prop.type_annotation);
                    self.check_js_grammar_accessibility_modifier(&prop.modifiers, member_idx);
                }
            }

            syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.ctx.arena.get_accessor(node) {
                    self.check_js_grammar_accessibility_modifier(&accessor.modifiers, member_idx);
                    self.error_if_ts_only_type_annotation(accessor.type_annotation);
                    self.check_js_grammar_parameters(&accessor.parameters.nodes);
                }
            }

            syntax_kind_ext::INDEX_SIGNATURE
            | syntax_kind_ext::CALL_SIGNATURE
            | syntax_kind_ext::CONSTRUCT_SIGNATURE => {
                if node.kind == syntax_kind_ext::INDEX_SIGNATURE {
                    if let Some(index_sig) = self.ctx.arena.get_index_signature(node) {
                        self.check_js_grammar_parameters(&index_sig.parameters.nodes);
                    }
                } else {
                    self.error_if_ts_only_signature_without_body(true, member_idx);
                }
            }

            _ => {}
        }
    }

    /// Check function parameters for JS grammar errors (type annotations, modifiers).
    fn check_js_grammar_parameters(&mut self, param_nodes: &[NodeIndex]) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        for &param_idx in param_nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            // TS8010: Type annotation on parameter
            if param.type_annotation.is_some() {
                self.error_at_node(
                    param.type_annotation,
                    diagnostic_messages::TYPE_ANNOTATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    diagnostic_codes::TYPE_ANNOTATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                );
            }

            // TS8012: Parameter modifiers (public/private/protected/readonly/static/export/async on params)
            if let Some(ref modifiers) = param.modifiers {
                for &mod_idx in &modifiers.nodes {
                    if let Some(mod_node) = self.ctx.arena.get(mod_idx) {
                        match mod_node.kind {
                            k if k == SyntaxKind::PublicKeyword as u16
                                || k == SyntaxKind::PrivateKeyword as u16
                                || k == SyntaxKind::ProtectedKeyword as u16
                                || k == SyntaxKind::ReadonlyKeyword as u16
                                || k == SyntaxKind::StaticKeyword as u16
                                || k == SyntaxKind::ExportKeyword as u16
                                || k == SyntaxKind::AsyncKeyword as u16 =>
                            {
                                self.error_at_node(
                                    mod_idx,
                                    diagnostic_messages::PARAMETER_MODIFIERS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                                    diagnostic_codes::PARAMETER_MODIFIERS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }

            // TS8009: Optional parameter (question token)
            if param.question_token {
                let message = crate::diagnostics::format_message(
                    diagnostic_messages::THE_MODIFIER_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    &["?"],
                );
                self.error_at_node(
                    param_idx,
                    &message,
                    diagnostic_codes::THE_MODIFIER_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                );
            }
        }
    }

    /// Check for accessibility modifiers (public/private/protected) on a declaration.
    fn check_js_grammar_accessibility_modifier(
        &mut self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
        _fallback_idx: NodeIndex,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx) {
                    let modifier_name = match mod_node.kind {
                        k if k == SyntaxKind::PublicKeyword as u16 => Some("public"),
                        k if k == SyntaxKind::PrivateKeyword as u16 => Some("private"),
                        k if k == SyntaxKind::ProtectedKeyword as u16 => Some("protected"),
                        _ => None,
                    };
                    if let Some(name) = modifier_name {
                        let message = format_message(
                            diagnostic_messages::THE_MODIFIER_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                            &[name],
                        );
                        self.error_at_node(
                            mod_idx,
                            &message,
                            diagnostic_codes::THE_MODIFIER_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                        );
                    }
                }
            }
        }
    }

    /// Check a variable statement for JS grammar errors.
    fn check_js_grammar_variable_statement(
        &mut self,
        _stmt_idx: NodeIndex,
        node: &tsz_parser::parser::node::Node,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        // VariableStatement uses VariableData { modifiers, declarations }
        let Some(var) = self.ctx.arena.get_variable(node) else {
            return;
        };

        // TS8009: 'declare' modifier on variable statement
        if self.has_declare_modifier(&var.modifiers) {
            let message = format_message(
                diagnostic_messages::THE_MODIFIER_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                &["declare"],
            );
            if let Some(mod_idx) =
                self.get_modifier_index(&var.modifiers, SyntaxKind::DeclareKeyword as u16)
            {
                self.error_at_node(
                    mod_idx,
                    &message,
                    diagnostic_codes::THE_MODIFIER_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                );
            }
        }

        // Check variable declarations for type annotations
        // VariableStatement.declarations contains VariableDeclarationList nodes
        for &list_idx in &var.declarations.nodes {
            if let Some(list_node) = self.ctx.arena.get(list_idx)
                && let Some(list) = self.ctx.arena.get_variable(list_node)
            {
                for &decl_idx in &list.declarations.nodes {
                    if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                        && let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node)
                    {
                        // TS8010: Type annotation on variable
                        if var_decl.type_annotation.is_some() {
                            self.error_at_node(
                                        var_decl.type_annotation,
                                        diagnostic_messages::TYPE_ANNOTATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                                        diagnostic_codes::TYPE_ANNOTATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                                    );
                        }
                    }
                }
            }
        }
    }

    /// Get the index of a specific modifier kind in a modifier list.
    pub(crate) fn get_modifier_index(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
        kind: u16,
    ) -> Option<NodeIndex> {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx)
                    && mod_node.kind == kind
                {
                    return Some(mod_idx);
                }
            }
        }
        None
    }
}
