//! Declaration & Statement Checking Module
//!
//! Extracted from state.rs: Methods for checking source files, declarations,
//! statements, and class/interface validation. Also includes `StatementCheckCallbacks`.

use crate::state::CheckerState;
use crate::statements::StatementChecker;
use std::time::Instant;
use tracing::{Level, span};
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// Check if a name is a strict mode reserved word (ES5 §7.6.1.2).
/// These identifiers cannot be used as variable/function/class names in strict mode.
pub(crate) fn is_strict_mode_reserved_name(name: &str) -> bool {
    matches!(
        name,
        "implements"
            | "interface"
            | "let"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "static"
            | "yield"
    )
}

impl<'a> CheckerState<'a> {
    /// Check a declaration name node for strict mode reserved words.
    /// Emits TS1212 (general strict mode), TS1213 (class context), or TS1214 (module context).
    pub(crate) fn check_strict_mode_reserved_name_at(
        &mut self,
        name_idx: tsz_parser::parser::NodeIndex,
        context_node: tsz_parser::parser::NodeIndex,
    ) {
        if name_idx.is_none() || !self.is_strict_mode_for_node(context_node) {
            return;
        }
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return;
        };
        if !is_strict_mode_reserved_name(&ident.escaped_text) {
            return;
        }
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        if self.ctx.enclosing_class.is_some() {
            let message = format_message(
                diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO,
                &[&ident.escaped_text],
            );
            self.error_at_node(
                name_idx,
                &message,
                diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO,
            );
        } else if self.ctx.binder.is_external_module() {
            let message = format_message(
                diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_MODULES_ARE_AUTOMATICALLY,
                &[&ident.escaped_text],
            );
            self.error_at_node(
                name_idx,
                &message,
                diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_MODULES_ARE_AUTOMATICALLY,
            );
        } else {
            let message = format_message(
                diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE,
                &[&ident.escaped_text],
            );
            self.error_at_node(
                name_idx,
                &message,
                diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE,
            );
        }
    }

    // =========================================================================
    // Source File Checking (Full Traversal)
    // =========================================================================

    /// Check a source file and populate diagnostics (main entry point).
    ///
    /// This is the primary entry point for type checking after parsing and binding.
    /// It traverses the entire AST and performs all type checking operations.
    ///
    /// ## Checking Process:
    /// 1. Initializes the type environment
    /// 2. Traverses all top-level declarations
    /// 3. Checks all statements and expressions
    /// 4. Populates diagnostics with errors and warnings
    ///
    /// ## What Gets Checked:
    /// - Type annotations
    /// - Assignments (variable, property, return)
    /// - Function calls
    /// - Property access
    /// - Type compatibility (extends, implements)
    /// - Flow analysis (definite assignment, type narrowing)
    /// - Generic constraints
    /// - And much more...
    ///
    /// ## Diagnostics:
    /// - Errors are added to `ctx.diagnostics`
    /// - Includes error codes (`TSxxxx`) and messages
    /// - Spans point to the problematic code
    ///
    /// ## Compilation Flow:
    /// 1. **Parser**: Source code → AST
    /// 2. **Binder**: AST → Symbols (scopes, declarations)
    /// 3. **Checker** (this function): AST + Symbols → Types + Diagnostics
    ///
    /// ## TypeScript Example:
    /// ```typescript
    /// // File: example.ts
    /// let x: string = 42;  // Type error: number not assignable to string
    ///
    /// function foo(a: number): string {
    ///   return a;  // Type error: number not assignable to string
    /// }
    ///
    /// interface User {
    ///   name: string;
    /// }
    /// const user: User = { age: 25 };  // Type error: missing 'name' property
    ///
    /// // check_source_file() would find all three errors above
    /// ```
    pub fn check_source_file(&mut self, root_idx: NodeIndex) {
        let _span = span!(Level::INFO, "check_source_file", idx = ?root_idx).entered();

        // Reset per-file flags
        self.ctx.is_in_ambient_declaration_file = false;

        let Some(node) = self.ctx.arena.get(root_idx) else {
            return;
        };

        if let Some(sf) = self.ctx.arena.get_source_file(node) {
            self.resolve_compiler_options_from_source(&sf.text);
            if self.has_ts_nocheck_pragma(&sf.text) {
                return;
            }

            // `type_env` is rebuilt per file, so drop per-file symbol-resolution memoization.
            self.ctx.application_symbols_resolved.clear();
            self.ctx.application_symbols_resolution_set.clear();

            // CRITICAL FIX: Build TypeEnvironment with all symbols (including lib symbols)
            // This ensures Error, Math, JSON, etc. interfaces are registered for property resolution
            // Without this, TypeData::Ref(Error) returns ERROR, causing TS2339 false positives
            let env_start = Instant::now();
            let populated_env = self.build_type_environment();
            tracing::trace!(target: "wasm::perf", phase = "build_type_environment", ms = env_start.elapsed().as_secs_f64() * 1000.0);
            *self.ctx.type_env.borrow_mut() = populated_env.clone();
            // CRITICAL: Also populate type_environment (Rc-wrapped) for FlowAnalyzer
            // This ensures type alias narrowing works during control flow analysis
            *self.ctx.type_environment.borrow_mut() = populated_env;

            // Register boxed types (String, Number, Boolean, etc.) from lib.d.ts
            // This enables primitive property access to use lib definitions instead of hardcoded lists
            // IMPORTANT: Must run AFTER build_type_environment() because it replaces the
            // TypeEnvironment, which would erase the boxed/array type registrations.
            self.register_boxed_types();

            // Type check each top-level statement
            // Mark that we're now in the checking phase. During build_type_environment,
            // closures may be type-checked without contextual types, which would cause
            // premature TS7006 errors. The checking phase ensures contextual types are available.
            self.ctx.is_checking_statements = true;
            let stmt_start = Instant::now();

            // In .d.ts files, emit TS1036 for non-declaration top-level statements.
            // The entire file is an ambient context, so statements like break, continue,
            // return, debugger, if, while, for, etc. are not allowed.
            let is_dts = self.ctx.file_name.ends_with(".d.ts")
                || self.ctx.file_name.ends_with(".d.tsx")
                || self.ctx.file_name.ends_with(".d.mts")
                || self.ctx.file_name.ends_with(".d.cts");
            if is_dts {
                self.ctx.is_in_ambient_declaration_file = true;
            }

            for &stmt_idx in &sf.statements.nodes {
                if is_dts {
                    self.check_dts_statement_in_ambient_context(stmt_idx);
                }
                self.check_statement(stmt_idx);
            }

            self.check_reserved_await_identifier_in_module(root_idx);

            // Check for unreachable code at the source file level (TS7027)
            // Must run AFTER statement checking so types are resolved (avoids premature TS7006)
            self.check_unreachable_code_in_block(&sf.statements.nodes);
            tracing::trace!(target: "wasm::perf", phase = "check_statements", ms = stmt_start.elapsed().as_secs_f64() * 1000.0);

            let post_start = Instant::now();
            // Check for function overload implementations (2389, 2391)
            self.check_function_implementations(&sf.statements.nodes);

            // Check for export assignment with other exports (2309)
            self.check_export_assignment(&sf.statements.nodes);

            // Check for duplicate identifiers (2300)
            self.check_duplicate_identifiers();

            // Check for missing global types (2318)
            // Emits errors at file start for essential types when libs are not loaded
            self.check_missing_global_types();

            // Check triple-slash reference directives (TS6053)
            if !self.ctx.compiler_options.no_resolve {
                self.check_triple_slash_references(&sf.file_name, &sf.text);
            }

            // Check for duplicate AMD module name assignments (TS2458)
            self.check_amd_module_names(&sf.text);

            // Check for unused declarations (TS6133/TS6196)
            if self.ctx.no_unused_locals() || self.ctx.no_unused_parameters() {
                self.check_unused_declarations();
            }
            // JS grammar checks: emit TS8xxx errors for TypeScript-only syntax in JS files
            if self.is_js_file() {
                let js_start = Instant::now();
                self.check_js_grammar_statements(&sf.statements.nodes);
                tracing::trace!(target: "wasm::perf", phase = "check_js_grammar", ms = js_start.elapsed().as_secs_f64() * 1000.0);
            }

            tracing::trace!(target: "wasm::perf", phase = "post_checks", ms = post_start.elapsed().as_secs_f64() * 1000.0);
        }
    }

    fn has_ts_nocheck_pragma(&self, source: &str) -> bool {
        source
            .lines()
            .take(20)
            .any(|line| line.contains("@ts-nocheck"))
    }

    // =========================================================================
    // JS Grammar Checking (TS8xxx errors)
    // =========================================================================

    /// Check all statements in a JS file for TypeScript-only syntax.
    /// Emits `TS8xxx` errors for constructs that are not valid in JavaScript files.
    fn check_js_grammar_statements(&mut self, statements: &[NodeIndex]) {
        for &stmt_idx in statements {
            self.check_js_grammar_statement(stmt_idx);
        }
    }

    /// Check a single statement for TypeScript-only syntax in JS files.
    fn check_js_grammar_statement(&mut self, stmt_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        match node.kind {
            // TS8008: Type aliases can only be used in TypeScript files
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                self.error_at_node(
                    stmt_idx,
                    diagnostic_messages::TYPE_ALIASES_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    diagnostic_codes::TYPE_ALIASES_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                );
            }

            // TS8006: 'interface'/'enum'/'module'/'namespace' declarations
            syntax_kind_ext::INTERFACE_DECLARATION => {
                self.error_ts_only_declaration("interface", stmt_idx);
            }

            syntax_kind_ext::ENUM_DECLARATION => {
                self.error_ts_only_declaration("enum", stmt_idx);
            }

            syntax_kind_ext::MODULE_DECLARATION => {
                let keyword = self.get_module_keyword(stmt_idx, node);
                self.error_ts_only_declaration(keyword, stmt_idx);
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
                if let Some(export_decl) = self.ctx.arena.get_export_decl_at(stmt_idx)
                    && export_decl.export_clause.is_some()
                {
                    self.check_js_grammar_statement(export_decl.export_clause);
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

    fn check_js_grammar_expression(&mut self, expr_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return;
        };

        if node.is_function_like() {
            self.check_js_grammar_function(expr_idx, node);
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
                if let Some(clause_node) = self.ctx.arena.get(clause_idx)
                    && clause_node.kind == syntax_kind_ext::HERITAGE_CLAUSE
                    && let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node)
                    && heritage.token == SyntaxKind::ImplementsKeyword as u16
                {
                    self.error_at_node(
                        clause_idx,
                        diagnostic_messages::IMPLEMENTS_CLAUSES_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                        diagnostic_codes::IMPLEMENTS_CLAUSES_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    );
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
    fn error_if_ts_only_type_params(
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
                self.error_if_ts_only_signature_without_body(true, member_idx);
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

            // TS8012: Parameter modifiers (public/private/protected/readonly on constructor params)
            if let Some(ref modifiers) = param.modifiers {
                for &mod_idx in &modifiers.nodes {
                    if let Some(mod_node) = self.ctx.arena.get(mod_idx) {
                        match mod_node.kind {
                            k if k == SyntaxKind::PublicKeyword as u16
                                || k == SyntaxKind::PrivateKeyword as u16
                                || k == SyntaxKind::ProtectedKeyword as u16
                                || k == SyntaxKind::ReadonlyKeyword as u16 =>
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
    fn get_modifier_index(
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

    fn has_static_modifier_in_arena(
        &self,
        arena: &tsz_parser::parser::NodeArena,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> bool {
        self.get_modifier_index_in_arena(
            arena,
            modifiers,
            tsz_scanner::SyntaxKind::StaticKeyword as u16,
        )
        .is_some()
    }

    fn get_modifier_index_in_arena(
        &self,
        arena: &tsz_parser::parser::NodeArena,
        modifiers: &Option<tsz_parser::parser::NodeList>,
        kind: u16,
    ) -> Option<NodeIndex> {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = arena.get(mod_idx)
                    && mod_node.kind == kind
                {
                    return Some(mod_idx);
                }
            }
        }
        None
    }

    pub(crate) fn declaration_symbol_flags(
        &self,
        arena: &tsz_parser::parser::NodeArena,
        decl_idx: NodeIndex,
    ) -> Option<u32> {
        use tsz_parser::parser::node_flags;

        let decl_idx = self.resolve_duplicate_decl_node(arena, decl_idx)?;
        let node = arena.get(decl_idx)?;

        match node.kind {
            syntax_kind_ext::VARIABLE_DECLARATION => {
                let mut decl_flags = node.flags as u32;
                if (decl_flags & (node_flags::LET | node_flags::CONST)) == 0
                    && let Some(parent) = arena.get_extended(decl_idx).map(|ext| ext.parent)
                    && let Some(parent_node) = arena.get(parent)
                    && parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                {
                    decl_flags |= parent_node.flags as u32;
                }
                if (decl_flags & (node_flags::LET | node_flags::CONST)) != 0 {
                    Some(symbol_flags::BLOCK_SCOPED_VARIABLE)
                } else {
                    Some(symbol_flags::FUNCTION_SCOPED_VARIABLE)
                }
            }
            syntax_kind_ext::FUNCTION_DECLARATION => Some(symbol_flags::FUNCTION),
            syntax_kind_ext::CLASS_DECLARATION => Some(symbol_flags::CLASS),
            syntax_kind_ext::INTERFACE_DECLARATION => Some(symbol_flags::INTERFACE),
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => Some(symbol_flags::TYPE_ALIAS),
            syntax_kind_ext::ENUM_DECLARATION => {
                // Check if this is a const enum by looking for const modifier
                let is_const_enum = arena
                    .get_enum(node)
                    .and_then(|enum_decl| enum_decl.modifiers.as_ref())
                    .is_some_and(|modifiers| {
                        modifiers.nodes.iter().any(|&mod_idx| {
                            arena.get(mod_idx).is_some_and(|mod_node| {
                                mod_node.kind == tsz_scanner::SyntaxKind::ConstKeyword as u16
                            })
                        })
                    });
                if is_const_enum {
                    Some(symbol_flags::CONST_ENUM)
                } else {
                    Some(symbol_flags::REGULAR_ENUM)
                }
            }
            syntax_kind_ext::MODULE_DECLARATION => {
                // Namespaces (module declarations) can merge with functions, classes, enums
                Some(symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE)
            }
            syntax_kind_ext::GET_ACCESSOR => {
                let mut flags = symbol_flags::GET_ACCESSOR;
                if let Some(accessor) = arena.get_accessor(node)
                    && self.has_static_modifier_in_arena(arena, &accessor.modifiers)
                {
                    flags |= symbol_flags::STATIC;
                }
                Some(flags)
            }
            syntax_kind_ext::SET_ACCESSOR => {
                let mut flags = symbol_flags::SET_ACCESSOR;
                if let Some(accessor) = arena.get_accessor(node)
                    && self.has_static_modifier_in_arena(arena, &accessor.modifiers)
                {
                    flags |= symbol_flags::STATIC;
                }
                Some(flags)
            }
            syntax_kind_ext::METHOD_DECLARATION => {
                let mut flags = symbol_flags::METHOD;
                if let Some(method) = arena.get_method_decl(node)
                    && self.has_static_modifier_in_arena(arena, &method.modifiers)
                {
                    flags |= symbol_flags::STATIC;
                }
                Some(flags)
            }
            syntax_kind_ext::PROPERTY_DECLARATION => {
                let mut flags = symbol_flags::PROPERTY;
                if let Some(prop) = arena.get_property_decl(node)
                    && self.has_static_modifier_in_arena(arena, &prop.modifiers)
                {
                    flags |= symbol_flags::STATIC;
                }
                Some(flags)
            }
            syntax_kind_ext::CONSTRUCTOR => Some(symbol_flags::CONSTRUCTOR),
            syntax_kind_ext::IMPORT_CLAUSE
            | syntax_kind_ext::NAMESPACE_IMPORT
            | syntax_kind_ext::IMPORT_SPECIFIER
            | syntax_kind_ext::IMPORT_EQUALS_DECLARATION => Some(symbol_flags::ALIAS),
            syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION => {
                // 'export as namespace' creates a global alias to the module.
                // It behaves like a global value module alias.
                Some(symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::ALIAS)
            }
            _ => None,
        }
    }

    fn check_reserved_await_identifier_in_module(&mut self, source_file_idx: NodeIndex) {
        let Some(source_file_node) = self.ctx.arena.get(source_file_idx) else {
            return;
        };
        let Some(source_file) = self.ctx.arena.get_source_file(source_file_node) else {
            return;
        };

        let source_file_name = &source_file.file_name;
        let is_declaration_file = source_file.is_declaration_file
            || source_file_name.ends_with(".d.ts")
            || source_file_name.ends_with(".d.tsx")
            || source_file_name.ends_with(".d.mts")
            || source_file_name.ends_with(".d.cts")
            || self.ctx.file_name.ends_with(".d.ts")
            || self.ctx.file_name.ends_with(".d.tsx")
            || self.ctx.file_name.ends_with(".d.mts")
            || self.ctx.file_name.ends_with(".d.cts");

        if is_declaration_file {
            return;
        }

        let is_external_module = if let Some(ref map) = self.ctx.is_external_module_by_file {
            map.get(&self.ctx.file_name).copied().unwrap_or(false)
        } else {
            self.ctx.binder.is_external_module()
        };

        let has_module_indicator = self.source_file_has_module_indicator(source_file);
        let force_js_module_check = self.is_js_like_file() && has_module_indicator;

        if !is_external_module && !force_js_module_check {
            return;
        }

        let Some(await_sym_id) = self.ctx.binder.file_locals.get("await") else {
            return;
        };

        let Some(symbol) = self.ctx.binder.get_symbol(await_sym_id) else {
            return;
        };

        let mut candidate_decls = symbol.declarations.clone();
        if symbol.value_declaration.is_some() {
            candidate_decls.push(symbol.value_declaration);
        }

        candidate_decls.sort_unstable_by_key(|node| node.0);
        candidate_decls.dedup();

        for decl_idx in candidate_decls {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };

            let is_disallowed_top_level_await_decl = matches!(
                node.kind,
                syntax_kind_ext::VARIABLE_DECLARATION
                    | syntax_kind_ext::BINDING_ELEMENT
                    | syntax_kind_ext::FUNCTION_DECLARATION
                    | syntax_kind_ext::CLASS_DECLARATION
                    | syntax_kind_ext::IMPORT_CLAUSE
                    | syntax_kind_ext::IMPORT_SPECIFIER
                    | syntax_kind_ext::NAMESPACE_IMPORT
            );
            if !is_disallowed_top_level_await_decl {
                continue;
            }

            let is_plain_await_identifier = self
                .await_identifier_name_node_for_decl(decl_idx)
                .is_some_and(|name_idx| self.is_plain_await_identifier(source_file, name_idx));

            if !is_plain_await_identifier {
                continue;
            }

            let mut current = decl_idx;
            let mut is_top_level = false;
            while let Some(ext) = self.ctx.arena.get_extended(current) {
                let parent = ext.parent;
                if parent.is_none() {
                    break;
                }
                if parent == source_file_idx {
                    is_top_level = true;
                    break;
                }
                current = parent;
            }

            if !is_top_level {
                continue;
            }

            self.error_at_node(
                decl_idx,
                "Identifier expected. 'await' is a reserved word at the top-level of a module.",
                crate::diagnostics::diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_AT_THE_TOP_LEVEL_OF_A_MODULE,
            );
            break;
        }

        self.emit_top_level_await_text_fallback(source_file);
    }

    fn await_identifier_name_node_for_decl(&self, decl_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(decl_idx)?;
        match node.kind {
            syntax_kind_ext::VARIABLE_DECLARATION => self
                .ctx
                .arena
                .get_variable_declaration(node)
                .map(|decl| decl.name),
            syntax_kind_ext::BINDING_ELEMENT => self
                .ctx
                .arena
                .get_binding_element(node)
                .map(|decl| decl.name),
            syntax_kind_ext::FUNCTION_DECLARATION => {
                self.ctx.arena.get_function(node).map(|f| f.name)
            }
            syntax_kind_ext::CLASS_DECLARATION => self.ctx.arena.get_class(node).map(|c| c.name),
            syntax_kind_ext::IMPORT_CLAUSE => self
                .ctx
                .arena
                .get_import_clause(node)
                .map(|clause| clause.name),
            syntax_kind_ext::IMPORT_SPECIFIER => self
                .ctx
                .arena
                .get_specifier(node)
                .map(|specifier| specifier.name),
            syntax_kind_ext::NAMESPACE_IMPORT => self
                .ctx
                .arena
                .get_named_imports(node)
                .map(|named| named.name),
            _ => None,
        }
    }

    fn is_plain_await_identifier(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
        node_idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some((start, end)) = self.get_node_span(node_idx) else {
            return false;
        };

        source_file
            .text
            .get(start as usize..end as usize)
            .is_some_and(|text| text == "await")
    }

    fn source_file_has_module_indicator(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> bool {
        source_file.statements.nodes.iter().any(|&stmt_idx| {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                return false;
            };

            matches!(
                stmt_node.kind,
                syntax_kind_ext::EXPORT_DECLARATION
                    | syntax_kind_ext::EXPORT_ASSIGNMENT
                    | syntax_kind_ext::IMPORT_DECLARATION
            )
        })
    }

    fn emit_ts1262_at_first_await(&mut self, statement_start: u32, statement_text: &str) -> bool {
        let Some(offset) = statement_text.find("await") else {
            return false;
        };

        self.error_at_position(
            statement_start + offset as u32,
            5,
            "Identifier expected. 'await' is a reserved word at the top-level of a module.",
            crate::diagnostics::diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_AT_THE_TOP_LEVEL_OF_A_MODULE,
        );
        true
    }

    fn statement_contains_any(text: &str, patterns: &[&str]) -> bool {
        patterns.iter().any(|pattern| text.contains(pattern))
    }

    fn is_js_like_file(&self) -> bool {
        self.ctx.file_name.ends_with(".js")
            || self.ctx.file_name.ends_with(".jsx")
            || self.ctx.file_name.ends_with(".mjs")
            || self.ctx.file_name.ends_with(".cjs")
    }

    fn emit_top_level_await_text_fallback(
        &mut self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) {
        let ts1262_code =
            crate::diagnostics::diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_AT_THE_TOP_LEVEL_OF_A_MODULE;
        if self
            .ctx
            .diagnostics
            .iter()
            .any(|diag| diag.code == ts1262_code)
        {
            return;
        }

        let has_module_indicator = self.source_file_has_module_indicator(source_file);
        let is_js_like_file = self.is_js_like_file();

        let import_patterns = [
            "import await from",
            "import * as await from",
            "import { await } from",
            "import { await as await } from",
        ];
        let binding_pattern_patterns = ["var {await}", "var [await]"];
        let js_variable_patterns = ["const await", "let await", "var await"];

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };

            let Some((start, end)) = self.get_node_span(stmt_idx) else {
                continue;
            };
            let Some(stmt_text) = source_file.text.get(start as usize..end as usize) else {
                continue;
            };

            match stmt_node.kind {
                syntax_kind_ext::IMPORT_DECLARATION => {
                    if Self::statement_contains_any(stmt_text, &import_patterns)
                        && self.emit_ts1262_at_first_await(start, stmt_text)
                    {
                        return;
                    }
                }
                syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                    let has_await_import_equals = stmt_text.contains("import await =");
                    let is_require_form = stmt_text.contains("require(");
                    if has_await_import_equals
                        && (is_require_form || has_module_indicator)
                        && self.emit_ts1262_at_first_await(start, stmt_text)
                    {
                        return;
                    }
                }
                syntax_kind_ext::VARIABLE_STATEMENT => {
                    let has_binding_pattern_await =
                        Self::statement_contains_any(stmt_text, &binding_pattern_patterns);
                    let has_js_var_await = is_js_like_file
                        && Self::statement_contains_any(stmt_text, &js_variable_patterns);
                    if (has_binding_pattern_await || has_js_var_await)
                        && self.emit_ts1262_at_first_await(start, stmt_text)
                    {
                        return;
                    }
                }
                _ => {}
            }
        }

        if has_module_indicator && let Some(offset) = source_file.text.find("const await") {
            self.error_at_position(
                offset as u32 + 6,
                5,
                "Identifier expected. 'await' is a reserved word at the top-level of a module.",
                ts1262_code,
            );
        }
    }

    /// Emit TS1036 for non-declaration statements in .d.ts files.
    /// In .d.ts files the entire file is implicitly ambient, so non-declaration
    /// statements (break, continue, return, if, while, for, debugger, etc.) are not allowed.
    fn check_dts_statement_in_ambient_context(&mut self, stmt_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let is_non_declaration = matches!(
            node.kind,
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT
                || k == syntax_kind_ext::IF_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT
                || k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::FOR_STATEMENT
                || k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT
                || k == syntax_kind_ext::BREAK_STATEMENT
                || k == syntax_kind_ext::CONTINUE_STATEMENT
                || k == syntax_kind_ext::RETURN_STATEMENT
                || k == syntax_kind_ext::WITH_STATEMENT
                || k == syntax_kind_ext::SWITCH_STATEMENT
                || k == syntax_kind_ext::THROW_STATEMENT
                || k == syntax_kind_ext::TRY_STATEMENT
                || k == syntax_kind_ext::DEBUGGER_STATEMENT
                || k == syntax_kind_ext::LABELED_STATEMENT
        );

        if is_non_declaration && let Some((pos, end)) = self.ctx.get_node_span(stmt_idx) {
            self.ctx.error(
                pos,
                end - pos,
                diagnostic_messages::STATEMENTS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS.to_string(),
                diagnostic_codes::STATEMENTS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS,
            );
        }
    }

    /// Check for duplicate parameter names in a parameter list (TS2300).
    /// Check a statement and produce type errors.
    ///
    /// This method delegates to `StatementChecker` for dispatching logic,
    /// while providing actual implementations via the `StatementCheckCallbacks` trait.
    pub(crate) fn check_statement(&mut self, stmt_idx: NodeIndex) {
        StatementChecker::check(stmt_idx, self);
    }
}
