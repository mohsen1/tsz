//! Isolated declarations checking (`TS9xxx` series).
//!
//! When `--isolatedDeclarations` is enabled, exported declarations must have
//! explicit type annotations so that declaration emit can work without
//! cross-file type inference.

use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    /// Top-level entry point: check all exported declarations for isolated declarations compliance.
    pub(crate) fn check_isolated_declarations(&mut self, stmts: &[NodeIndex]) {
        if !self.ctx.isolated_declarations() || self.ctx.is_declaration_file() {
            return;
        }

        // In script files (no import/export), all top-level declarations are
        // visible and need declaration emit. In module files, only exported
        // declarations need checking.
        let is_script = !self.file_has_module_syntax(stmts);

        for &stmt_idx in stmts {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            match node.kind {
                syntax_kind_ext::VARIABLE_STATEMENT => {
                    if is_script || self.is_declaration_exported(self.ctx.arena, stmt_idx) {
                        self.check_isolated_decl_variable_statement(stmt_idx);
                        // Also check for expando assignments on variable-declared functions
                        self.check_isolated_decl_expando_variable(stmt_idx, stmts);
                    }
                }
                syntax_kind_ext::EXPORT_DECLARATION => {
                    if let Some(export_decl) = self.ctx.arena.get_export_decl(node) {
                        let export_clause = export_decl.export_clause;
                        if export_clause.is_some()
                            && let Some(exported_node) = self.ctx.arena.get(export_clause)
                        {
                            match exported_node.kind {
                                syntax_kind_ext::VARIABLE_STATEMENT => {
                                    self.check_isolated_decl_variable_statement(export_clause);
                                }
                                syntax_kind_ext::FUNCTION_DECLARATION => {
                                    self.check_isolated_decl_function(export_clause);
                                    self.check_isolated_decl_expando_function(export_clause, stmts);
                                }
                                syntax_kind_ext::CLASS_DECLARATION => {
                                    self.check_isolated_decl_class(export_clause);
                                }
                                syntax_kind_ext::ENUM_DECLARATION => {
                                    self.check_isolated_decl_enum(export_clause);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                syntax_kind_ext::FUNCTION_DECLARATION => {
                    if is_script || self.is_declaration_exported(self.ctx.arena, stmt_idx) {
                        self.check_isolated_decl_function(stmt_idx);
                        self.check_isolated_decl_expando_function(stmt_idx, stmts);
                    }
                }
                syntax_kind_ext::CLASS_DECLARATION => {
                    if is_script || self.is_declaration_exported(self.ctx.arena, stmt_idx) {
                        self.check_isolated_decl_class(stmt_idx);
                    }
                }
                syntax_kind_ext::ENUM_DECLARATION => {
                    if is_script || self.is_declaration_exported(self.ctx.arena, stmt_idx) {
                        self.check_isolated_decl_enum(stmt_idx);
                    }
                }
                _ => {}
            }
        }

        // Check default exports
        self.check_isolated_decl_default_exports(stmts);
    }

    /// Check if a file has module syntax (imports, exports).
    fn file_has_module_syntax(&self, stmts: &[NodeIndex]) -> bool {
        for &stmt_idx in stmts {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            match node.kind {
                syntax_kind_ext::IMPORT_DECLARATION
                | syntax_kind_ext::EXPORT_DECLARATION
                | syntax_kind_ext::EXPORT_ASSIGNMENT => return true,
                k if k == syntax_kind_ext::VARIABLE_STATEMENT
                    || k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::INTERFACE_DECLARATION
                    || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    || k == syntax_kind_ext::ENUM_DECLARATION =>
                {
                    if self.is_declaration_exported(self.ctx.arena, stmt_idx) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// Check variable statement declarations for TS9010 (variable needs type annotation).
    fn check_isolated_decl_variable_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };
        let Some(stmt) = self.ctx.arena.get_variable(node) else {
            return;
        };
        // Check if it's a `const` declaration
        let is_const = self.is_const_variable_statement(stmt_idx);

        for &list_idx in &stmt.declarations.nodes {
            let Some(list_node) = self.ctx.arena.get(list_idx) else {
                continue;
            };
            let Some(decl_list) = self.ctx.arena.get_variable(list_node) else {
                continue;
            };
            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                let Some(decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                    continue;
                };

                // TS9019: Binding elements can't be exported directly
                // TSC emits per element name, not per whole pattern.
                if self.is_binding_pattern(decl.name) {
                    self.report_isolated_decl_binding_elements(decl.name);
                    continue;
                }

                if decl.type_annotation.is_some() || decl.initializer.is_none() {
                    continue;
                }

                // Check initializer for inferrability
                if let Some(init_node) = self.ctx.arena.get(decl.initializer) {
                    // TS9017: non-const array literals
                    if init_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION && !is_const {
                        self.error_at_node(
                            decl.initializer,
                            diagnostic_messages::ONLY_CONST_ARRAYS_CAN_BE_INFERRED_WITH_ISOLATEDDECLARATIONS,
                            diagnostic_codes::ONLY_CONST_ARRAYS_CAN_BE_INFERRED_WITH_ISOLATEDDECLARATIONS,
                        );
                        continue;
                    }

                    // TS9018: arrays with spread elements (even const)
                    if init_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                        && self.array_has_spread(decl.initializer)
                    {
                        self.error_at_node(
                                decl.initializer,
                                diagnostic_messages::ARRAYS_WITH_SPREAD_ELEMENTS_CANT_INFERRED_WITH_ISOLATEDDECLARATIONS,
                                diagnostic_codes::ARRAYS_WITH_SPREAD_ELEMENTS_CANT_INFERRED_WITH_ISOLATEDDECLARATIONS,
                            );
                        continue;
                    }

                    // TS9018: `[...] as const` with spread elements
                    if init_node.kind == syntax_kind_ext::AS_EXPRESSION
                        && let Some(assertion) = self.ctx.arena.get_type_assertion(init_node)
                        && let Some(inner_node) = self.ctx.arena.get(assertion.expression)
                        && inner_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                        && self.array_has_spread(assertion.expression)
                    {
                        self.error_at_node(
                                        assertion.expression,
                                        diagnostic_messages::ARRAYS_WITH_SPREAD_ELEMENTS_CANT_INFERRED_WITH_ISOLATEDDECLARATIONS,
                                        diagnostic_codes::ARRAYS_WITH_SPREAD_ELEMENTS_CANT_INFERRED_WITH_ISOLATEDDECLARATIONS,
                                    );
                        continue;
                    }
                }

                // Template expressions infer as `string` for non-const variables.
                // For `const`, the type is a template literal type which can't be
                // inferred in isolated declarations mode.
                if let Some(init_node2) = self.ctx.arena.get(decl.initializer)
                    && !is_const
                    && init_node2.kind == syntax_kind_ext::TEMPLATE_EXPRESSION
                {
                    continue;
                }

                // tsc emits TS9010 only when the initializer type genuinely
                // can't be inferred.
                if self.is_isolated_decl_type_inferrable(decl.initializer) {
                    // For function/arrow/class expressions in variable decls,
                    // check for TS9007 (missing return type)
                    self.check_isolated_decl_function_in_variable(decl.initializer);
                    continue;
                }

                self.error_at_node(
                    decl.name,
                    diagnostic_messages::VARIABLE_MUST_HAVE_AN_EXPLICIT_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
                    diagnostic_codes::VARIABLE_MUST_HAVE_AN_EXPLICIT_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
                );
            }
        }
    }

    /// Check function/arrow in variable initializer for TS9007 (missing return type).
    fn check_isolated_decl_function_in_variable(&mut self, init_idx: NodeIndex) {
        let Some(init_node) = self.ctx.arena.get(init_idx) else {
            return;
        };

        match init_node.kind {
            syntax_kind_ext::FUNCTION_EXPRESSION | syntax_kind_ext::ARROW_FUNCTION => {
                if let Some(func) = self.ctx.arena.get_function(init_node) {
                    if func.type_annotation.is_none() && func.body.is_some() {
                        let error_target = if func.name.is_some() {
                            func.name
                        } else {
                            init_idx
                        };
                        self.error_at_node(
                            error_target,
                            diagnostic_messages::FUNCTION_MUST_HAVE_AN_EXPLICIT_RETURN_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
                            diagnostic_codes::FUNCTION_MUST_HAVE_AN_EXPLICIT_RETURN_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
                        );
                    }
                    // Also check parameters for TS9011
                    self.check_isolated_decl_function_params(&func.parameters);
                }
            }
            syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(init_node) {
                    self.check_isolated_decl_function_in_variable(paren.expression);
                }
            }
            syntax_kind_ext::AS_EXPRESSION | syntax_kind_ext::SATISFIES_EXPRESSION => {
                if let Some(assertion) = self.ctx.arena.get_type_assertion(init_node) {
                    self.check_isolated_decl_function_in_variable(assertion.expression);
                }
            }
            _ => {}
        }
    }

    /// Check function declaration for TS9007 (missing return type).
    fn check_isolated_decl_function(&mut self, func_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(func_idx) else {
            return;
        };
        let Some(func) = self.ctx.arena.get_function(node) else {
            return;
        };

        if func.type_annotation.is_none() && func.body.is_some() {
            // For standalone/variable-assigned functions, always emit TS9007 when no return type.
            // TSC requires explicit return types on all exported functions in isolatedDeclarations mode,
            // regardless of whether they return a value or void.
            let error_node = if func.name.is_some() {
                func.name
            } else {
                func_idx
            };
            self.error_at_node(
                error_node,
                diagnostic_messages::FUNCTION_MUST_HAVE_AN_EXPLICIT_RETURN_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
                diagnostic_codes::FUNCTION_MUST_HAVE_AN_EXPLICIT_RETURN_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
            );
        }

        // Check parameters for TS9011
        self.check_isolated_decl_function_params(&func.parameters);
    }

    /// Check if a function body contains return statements with values.
    /// Returns false for empty bodies or bodies with only `return;` (void returns).
    fn body_has_value_return(&self, body_idx: NodeIndex) -> bool {
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return false;
        };
        // For arrow functions, the body might be an expression (concise body)
        if body_node.kind != syntax_kind_ext::BLOCK {
            // Concise arrow body: `() => expr` — the expr is the return value
            return true;
        }
        // Block body: scan for return statements with expressions
        let Some(block) = self.ctx.arena.get_block(body_node) else {
            return false;
        };
        self.stmts_have_value_return(&block.statements.nodes)
    }

    /// Recursively check if statements contain a return with a value.
    fn stmts_have_value_return(&self, stmts: &[NodeIndex]) -> bool {
        for &stmt_idx in stmts {
            if self.stmt_has_value_return(stmt_idx) {
                return true;
            }
        }
        false
    }

    /// Check if a single statement contains a return with a value.
    fn stmt_has_value_return(&self, stmt_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };
        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.ctx.arena.get_return_statement(node) {
                    ret.expression.is_some()
                } else {
                    false
                }
            }
            syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.ctx.arena.get_if_statement(node) {
                    self.stmt_has_value_return(if_stmt.then_statement)
                        || (if_stmt.else_statement.is_some()
                            && self.stmt_has_value_return(if_stmt.else_statement))
                } else {
                    false
                }
            }
            syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    self.stmts_have_value_return(&block.statements.nodes)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Check function parameters for TS9011 (parameter needs type annotation).
    fn check_isolated_decl_function_params(&mut self, params: &tsz_parser::parser::NodeList) {
        for &param_idx in &params.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            if param.type_annotation.is_none()
                && param.initializer.is_some()
                && !self.is_isolated_decl_simple_param_default(param.initializer)
            {
                let error_node = self.isolated_decl_param_annotation_target(param);
                self.error_at_node(
                    error_node,
                    diagnostic_messages::PARAMETER_MUST_HAVE_AN_EXPLICIT_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
                    diagnostic_codes::PARAMETER_MUST_HAVE_AN_EXPLICIT_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
                );
            }

            // Also recurse into function expression defaults to check their inner params
            if param.initializer.is_some()
                && let Some(init_node) = self.ctx.arena.get(param.initializer)
                && (init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || init_node.kind == syntax_kind_ext::ARROW_FUNCTION)
                && let Some(func) = self.ctx.arena.get_function(init_node)
            {
                self.check_isolated_decl_function_params(&func.parameters);
            }
        }
    }

    fn isolated_decl_param_annotation_target(
        &self,
        param: &tsz_parser::parser::node::ParameterData,
    ) -> NodeIndex {
        let Some(init_node) = self.ctx.arena.get(param.initializer) else {
            return param.name;
        };
        if (init_node.kind == syntax_kind_ext::AS_EXPRESSION
            || init_node.kind == syntax_kind_ext::SATISFIES_EXPRESSION
            || init_node.kind == syntax_kind_ext::TYPE_ASSERTION)
            && let Some(assertion) = self.ctx.arena.get_type_assertion(init_node)
            && assertion.type_node.is_some()
        {
            return assertion.type_node;
        }
        // TSC points at the initializer expression, not the parameter name
        param.initializer
    }

    /// Check if a parameter default is simple enough to not need a type annotation.
    /// tsc allows simple literals, but flags complex expressions and function expressions
    /// without return types.
    fn is_isolated_decl_simple_param_default(&self, init_idx: NodeIndex) -> bool {
        let Some(init_node) = self.ctx.arena.get(init_idx) else {
            return true;
        };
        match init_node.kind {
            k if k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::BigIntLiteral as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == SyntaxKind::VoidKeyword as u16 =>
            {
                true
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.ctx.arena.get_unary_expr(init_node) {
                    (unary.operator == SyntaxKind::MinusToken as u16
                        || unary.operator == SyntaxKind::PlusToken as u16)
                        && self.ctx.arena.get(unary.operand).is_some_and(|operand| {
                            operand.kind == SyntaxKind::NumericLiteral as u16
                                || operand.kind == SyntaxKind::BigIntLiteral as u16
                        })
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION =>
            {
                // Function expression as default — always inferrable for parameter type
                // (the type is the function signature shape, e.g. `() => void`)
                true
            }
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                // `x as T` / `x satisfies T` changes the exported parameter surface
                // away from a trivially inferrable literal/default shape, so
                // isolated declarations requires an explicit parameter annotation.
                false
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION =>
            {
                true
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                // Template expressions always infer as `string`
                true
            }
            _ => false,
        }
    }

    /// Check exported class for isolated declarations issues.
    fn check_isolated_decl_class(&mut self, class_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(class_idx) else {
            return;
        };
        let Some(class) = self.ctx.arena.get_class(node) else {
            return;
        };
        // TS9021: extends clause with expression
        if let Some(ref heritage) = class.heritage_clauses {
            self.check_isolated_decl_heritage_clauses(heritage);
        }

        // Check class members
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            match member_node.kind {
                syntax_kind_ext::METHOD_DECLARATION => {
                    self.check_isolated_decl_method(member_idx);
                }
                syntax_kind_ext::PROPERTY_DECLARATION => {
                    self.check_isolated_decl_property(member_idx);
                }
                _ => {
                    // GET_ACCESSOR/SET_ACCESSOR: checked via TS7006/TS7010/TS7032
                }
            }
        }
    }

    /// Check heritage clauses for TS9021 (extends with expression).
    fn check_isolated_decl_heritage_clauses(&mut self, heritage: &tsz_parser::parser::NodeList) {
        for &clause_idx in &heritage.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            if let Some(heritage_clause) = self.ctx.arena.get_heritage_clause(clause_node) {
                // Only check "extends" clauses, not "implements"
                if heritage_clause.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }
                for &type_idx in &heritage_clause.types.nodes {
                    let Some(type_node) = self.ctx.arena.get(type_idx) else {
                        continue;
                    };
                    // Heritage type nodes may be ExpressionWithTypeArguments or direct expressions
                    let expr_idx = if let Some(expr_with_args) =
                        self.ctx.arena.get_expr_type_args(type_node)
                    {
                        expr_with_args.expression
                    } else {
                        // The parser stored the expression directly (not wrapped)
                        type_idx
                    };
                    if let Some(expr_node) = self.ctx.arena.get(expr_idx) {
                        // If the extends expression is not a simple identifier or property access, report TS9021
                        if expr_node.kind != SyntaxKind::Identifier as u16
                            && expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        {
                            self.error_at_node(
                                expr_idx,
                                diagnostic_messages::EXTENDS_CLAUSE_CANT_CONTAIN_AN_EXPRESSION_WITH_ISOLATEDDECLARATIONS,
                                diagnostic_codes::EXTENDS_CLAUSE_CANT_CONTAIN_AN_EXPRESSION_WITH_ISOLATEDDECLARATIONS,
                            );
                        }
                    }
                }
            }
        }
    }

    /// Check class method for TS9007 (missing return type).
    fn check_isolated_decl_method(&mut self, method_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(method_idx) else {
            return;
        };
        let Some(method) = self.ctx.arena.get_method_decl(node) else {
            return;
        };

        // Skip private methods
        if let Some(ref modifiers) = method.modifiers
            && self
                .ctx
                .arena
                .has_modifier_ref(Some(modifiers), SyntaxKind::PrivateKeyword)
        {
            return;
        }
        // Skip #private methods
        if let Some(name_node) = self.ctx.arena.get(method.name)
            && name_node.kind == SyntaxKind::PrivateIdentifier as u16
        {
            return;
        }

        if method.type_annotation.is_none()
            && method.body.is_some()
            && self.body_has_value_return(method.body)
        {
            self.error_at_node(
                method.name,
                diagnostic_messages::FUNCTION_MUST_HAVE_AN_EXPLICIT_RETURN_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
                diagnostic_codes::FUNCTION_MUST_HAVE_AN_EXPLICIT_RETURN_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
            );
        }
        // Note: TS9011 (parameter check) is intentionally not called for class methods.
        // TSC handles class method parameter defaults differently from standalone functions.
    }

    /// Check class property for TS9012 (missing type annotation on property).
    fn check_isolated_decl_property(&mut self, prop_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(prop_idx) else {
            return;
        };
        let Some(prop) = self.ctx.arena.get_property_decl(node) else {
            return;
        };

        // Skip private properties
        if let Some(ref modifiers) = prop.modifiers
            && self
                .ctx
                .arena
                .has_modifier_ref(Some(modifiers), SyntaxKind::PrivateKeyword)
        {
            return;
        }
        // Skip #private properties
        if let Some(name_node) = self.ctx.arena.get(prop.name)
            && name_node.kind == SyntaxKind::PrivateIdentifier as u16
        {
            return;
        }

        if prop.type_annotation.is_some() || prop.initializer.is_none() {
            return;
        }

        // Check if property is readonly (affects template expression inferrability)
        let is_readonly = prop.modifiers.as_ref().is_some_and(|m| {
            self.ctx
                .arena
                .has_modifier_ref(Some(m), SyntaxKind::ReadonlyKeyword)
        });

        // Template expressions are inferrable as `string` for non-readonly properties.
        // For `readonly` properties, the type is a template literal type (not inferrable).
        if let Some(init_node) = self.ctx.arena.get(prop.initializer)
            && is_readonly
            && init_node.kind == syntax_kind_ext::TEMPLATE_EXPRESSION
        {
            self.error_at_node(
                    prop.name,
                    diagnostic_messages::PROPERTY_MUST_HAVE_AN_EXPLICIT_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
                    diagnostic_codes::PROPERTY_MUST_HAVE_AN_EXPLICIT_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
                );
            return;
        }

        // Check if initializer is inferrable (literals, as const, etc.)
        if self.is_isolated_decl_property_inferrable(prop.initializer) {
            // Even if the property value is inferrable, check function expressions
            // for missing return types
            if let Some(init_node) = self.ctx.arena.get(prop.initializer)
                && (init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || init_node.kind == syntax_kind_ext::ARROW_FUNCTION)
                && let Some(func) = self.ctx.arena.get_function(init_node)
                && func.type_annotation.is_none()
                && func.body.is_some()
                && self.body_has_value_return(func.body)
            {
                self.error_at_node(
                    prop.name,
                    diagnostic_messages::FUNCTION_MUST_HAVE_AN_EXPLICIT_RETURN_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
                    diagnostic_codes::FUNCTION_MUST_HAVE_AN_EXPLICIT_RETURN_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
                );
            }
            return;
        }

        self.error_at_node(
            prop.name,
            diagnostic_messages::PROPERTY_MUST_HAVE_AN_EXPLICIT_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
            diagnostic_codes::PROPERTY_MUST_HAVE_AN_EXPLICIT_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
        );
    }

    /// Check if a property initializer is inferrable for isolated declarations.
    /// For class properties: literals, as const, template literals are ok.
    /// Unlike variables, class properties don't have const vs let distinction.
    fn is_isolated_decl_property_inferrable(&self, init_idx: NodeIndex) -> bool {
        let Some(init_node) = self.ctx.arena.get(init_idx) else {
            return false;
        };
        match init_node.kind {
            k if k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::BigIntLiteral as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                true
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => self
                .ctx
                .arena
                .get_unary_expr(init_node)
                .is_some_and(|unary| {
                    (unary.operator == SyntaxKind::MinusToken as u16
                        || unary.operator == SyntaxKind::PlusToken as u16)
                        && self.ctx.arena.get(unary.operand).is_some_and(|operand| {
                            operand.kind == SyntaxKind::NumericLiteral as u16
                                || operand.kind == SyntaxKind::BigIntLiteral as u16
                        })
                }),
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                // For `as const`: check if the inner expression is inferrable.
                // `1 as const` → inferrable (literal type). `template as const` → NOT inferrable.
                // For `as Type` (non-const): always inferrable (type is explicit).
                if let Some(assertion) = self.ctx.arena.get_type_assertion(init_node) {
                    let is_const = self
                        .ctx
                        .arena
                        .get(assertion.type_node)
                        .is_some_and(|tn| tn.kind == SyntaxKind::ConstKeyword as u16);
                    if is_const {
                        // `as const`: inner must be a simple literal
                        self.is_isolated_decl_const_assertion_inferrable(assertion.expression)
                    } else {
                        // `as Type`: type is explicit
                        true
                    }
                } else {
                    false
                }
            }
            // Template expressions (with substitutions) infer as `string` type
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => true,
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => self
                .ctx
                .arena
                .get_parenthesized(init_node)
                .is_some_and(|paren| self.is_isolated_decl_property_inferrable(paren.expression)),
            k if k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::CLASS_EXPRESSION =>
            {
                true
            }
            _ => false,
        }
    }

    /// Check if an expression under `as const` is inferrable.
    /// Simple literals are inferrable (their literal type is deterministic).
    /// Template expressions with substitutions are NOT (require evaluation).
    fn is_isolated_decl_const_assertion_inferrable(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        match expr_node.kind {
            k if k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::BigIntLiteral as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                true
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => self
                .ctx
                .arena
                .get_unary_expr(expr_node)
                .is_some_and(|unary| {
                    (unary.operator == SyntaxKind::MinusToken as u16
                        || unary.operator == SyntaxKind::PlusToken as u16)
                        && self.ctx.arena.get(unary.operand).is_some_and(|operand| {
                            operand.kind == SyntaxKind::NumericLiteral as u16
                                || operand.kind == SyntaxKind::BigIntLiteral as u16
                        })
                }),
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION =>
            {
                // `[...] as const` or `{...} as const` — inferrable if elements are simple
                !self.has_non_inferrable_elements(expr_idx)
            }
            // Template expressions under `as const` are NOT inferrable
            // (literal type requires evaluation)
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => false,
            _ => false,
        }
    }

    /// Check for TS9023: assigning properties to exported functions.
    fn check_isolated_decl_expando_function(&mut self, func_idx: NodeIndex, stmts: &[NodeIndex]) {
        // Get the function name
        let Some(func_node) = self.ctx.arena.get(func_idx) else {
            return;
        };
        let Some(func) = self.ctx.arena.get_function(func_node) else {
            return;
        };
        let func_name = self
            .ctx
            .arena
            .get_identifier_at(func.name)
            .map(|id| id.escaped_text.clone());
        let Some(func_name) = func_name else {
            return;
        };

        self.scan_expando_assignments(&func_name, stmts);
    }

    /// Check for TS9023 on variable-declared functions (const arrows, function expressions).
    fn check_isolated_decl_expando_variable(&mut self, stmt_idx: NodeIndex, stmts: &[NodeIndex]) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };
        let Some(var_data) = self.ctx.arena.get_variable(node) else {
            return;
        };
        for &list_idx in &var_data.declarations.nodes {
            let Some(list_node) = self.ctx.arena.get(list_idx) else {
                continue;
            };
            let Some(decl_list) = self.ctx.arena.get_variable(list_node) else {
                continue;
            };
            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                let Some(decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                // Check if the initializer is a function expression or arrow function
                if let Some(init_node) = self.ctx.arena.get(decl.initializer)
                    && (init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                        || init_node.kind == syntax_kind_ext::ARROW_FUNCTION)
                {
                    // Get the variable name and scan for property assignments
                    if let Some(name_ident) = self.ctx.arena.get_identifier_at(decl.name) {
                        let var_name = name_ident.escaped_text.clone();
                        self.scan_expando_assignments(&var_name, stmts);
                    }
                }
            }
        }
    }

    /// Scan statements for property assignments to a named function/variable (TS9023).
    fn scan_expando_assignments(&mut self, func_name: &str, stmts: &[NodeIndex]) {
        let mut seen_props = std::collections::HashSet::new();
        for &stmt_idx in stmts {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(expr_data) = self.ctx.arena.get_expression_statement(node) else {
                continue;
            };
            let expr_idx = expr_data.expression;
            let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
                continue;
            };
            if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let Some(bin) = self.ctx.arena.get_binary_expr(expr_node) else {
                continue;
            };
            if bin.operator_token != SyntaxKind::EqualsToken as u16 {
                continue;
            }
            if let Some(left_node) = self.ctx.arena.get(bin.left)
                && left_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && let Some(access) = self.ctx.arena.get_access_expr(left_node)
                && let Some(target_node) = self.ctx.arena.get(access.expression)
                && target_node.kind == SyntaxKind::Identifier as u16
                && let Some(ident) = self.ctx.arena.get_identifier_at(access.expression)
                && ident.escaped_text == func_name
            {
                // Get property name for deduplication
                let prop_name = self
                    .ctx
                    .arena
                    .get_identifier_at(access.name_or_argument)
                    .map(|id| id.escaped_text.clone());
                if let Some(name) = prop_name
                    && !seen_props.insert(name)
                {
                    // Already reported for this property
                    continue;
                }
                self.error_at_node(
                                            stmt_idx,
                                            diagnostic_messages::ASSIGNING_PROPERTIES_TO_FUNCTIONS_WITHOUT_DECLARING_THEM_IS_NOT_SUPPORTED_WITH_I,
                                            diagnostic_codes::ASSIGNING_PROPERTIES_TO_FUNCTIONS_WITHOUT_DECLARING_THEM_IS_NOT_SUPPORTED_WITH_I,
                                        );
            }
        }
    }

    /// Check enum for TS9020 (enum member initializers must be computable).
    fn check_isolated_decl_enum(&mut self, enum_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(enum_idx) else {
            return;
        };
        let Some(enum_data) = self.ctx.arena.get_enum(node) else {
            return;
        };

        for &member_idx in &enum_data.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
                continue;
            };
            if member.initializer.is_none() {
                continue;
            }
            if !self.is_isolated_decl_enum_value(member.initializer, enum_idx) {
                self.error_at_node(
                    member.name,
                    diagnostic_messages::ENUM_MEMBER_INITIALIZERS_MUST_BE_COMPUTABLE_WITHOUT_REFERENCES_TO_EXTERNAL_SYMBO,
                    diagnostic_codes::ENUM_MEMBER_INITIALIZERS_MUST_BE_COMPUTABLE_WITHOUT_REFERENCES_TO_EXTERNAL_SYMBO,
                );
            }
        }
    }

    /// Check if an enum member initializer is computable without external references.
    fn is_isolated_decl_enum_value(&self, init_idx: NodeIndex, enum_idx: NodeIndex) -> bool {
        let Some(init_node) = self.ctx.arena.get(init_idx) else {
            return true;
        };
        match init_node.kind {
            k if k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::StringLiteral as u16 =>
            {
                true
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => self
                .ctx
                .arena
                .get_unary_expr(init_node)
                .is_some_and(|unary| self.is_isolated_decl_enum_value(unary.operand, enum_idx)),
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.ctx.arena.get_binary_expr(init_node) {
                    self.is_isolated_decl_enum_value(bin.left, enum_idx)
                        && self.is_isolated_decl_enum_value(bin.right, enum_idx)
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => self
                .ctx
                .arena
                .get_parenthesized(init_node)
                .is_some_and(|paren| self.is_isolated_decl_enum_value(paren.expression, enum_idx)),
            k if k == SyntaxKind::Identifier as u16 => {
                // An unqualified identifier — check if it refers to a member of the same enum
                self.is_same_enum_member_reference(init_idx, enum_idx)
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                // E.g. `Flag.AB` or `Flag["A"]`
                if let Some(access) = self.ctx.arena.get_access_expr(init_node) {
                    self.is_same_enum_access(access.expression, enum_idx)
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.ctx.arena.get_access_expr(init_node) {
                    self.is_same_enum_access(access.expression, enum_idx)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Check if an identifier refers to a member of the same enum.
    fn is_same_enum_member_reference(&self, id_idx: NodeIndex, enum_idx: NodeIndex) -> bool {
        let Some(ident) = self.ctx.arena.get_identifier_at(id_idx) else {
            return false;
        };
        let name = &ident.escaped_text;

        // Check if this name is a member of the current enum
        let Some(enum_node) = self.ctx.arena.get(enum_idx) else {
            return false;
        };
        let Some(enum_data) = self.ctx.arena.get_enum(enum_node) else {
            return false;
        };
        for &member_idx in &enum_data.members.nodes {
            if let Some(member_node) = self.ctx.arena.get(member_idx)
                && let Some(member) = self.ctx.arena.get_enum_member(member_node)
                && let Some(member_ident) = self.ctx.arena.get_identifier_at(member.name)
                && member_ident.escaped_text == *name
            {
                return true;
            }
        }
        false
    }

    /// Check if a property access like `EnumName.Member` refers to the same enum.
    fn is_same_enum_access(&self, expr_idx: NodeIndex, enum_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(ident) = self.ctx.arena.get_identifier_at(expr_idx) else {
            return false;
        };
        // Check if the identifier matches the enum name
        let Some(enum_node) = self.ctx.arena.get(enum_idx) else {
            return false;
        };
        let Some(enum_data) = self.ctx.arena.get_enum(enum_node) else {
            return false;
        };
        if let Some(enum_ident) = self.ctx.arena.get_identifier_at(enum_data.name) {
            return enum_ident.escaped_text == ident.escaped_text;
        }
        false
    }

    /// Check default exports for TS9037, TS9013.
    fn check_isolated_decl_default_exports(&mut self, stmts: &[NodeIndex]) {
        for &stmt_idx in stmts {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };

            match node.kind {
                // `export default <expr>` via ExportAssignment (export = expr)
                syntax_kind_ext::EXPORT_ASSIGNMENT => {
                    if let Some(export_assign) = self.ctx.arena.get_export_assignment(node)
                        && !export_assign.is_export_equals
                    {
                        self.check_isolated_decl_default_export_expr(export_assign.expression);
                    }
                }
                // `export default <expr>` via ExportDeclaration
                syntax_kind_ext::EXPORT_DECLARATION => {
                    if let Some(export_decl) = self.ctx.arena.get_export_decl(node)
                        && export_decl.is_default_export
                        && export_decl.export_clause.is_some()
                    {
                        self.check_isolated_decl_default_export_expr(export_decl.export_clause);
                    }
                }
                _ => {}
            }
        }
    }

    /// Check a default export expression for TS9037/TS9013.
    fn check_isolated_decl_default_export_expr(&mut self, expr_idx: NodeIndex) {
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return;
        };
        match expr_node.kind {
            // Function/arrow/class expressions are ok if they have proper annotations
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::CLASS_EXPRESSION =>
            {
                return;
            }
            // Literals are ok
            k if k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::BigIntLiteral as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                return;
            }
            // Identifiers that refer to values are ok (e.g. `export default a`)
            k if k == SyntaxKind::Identifier as u16 => {
                return;
            }
            // Object/array literals need TS9013 (expression type can't be inferred)
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.error_at_node(
                    expr_idx,
                    diagnostic_messages::EXPRESSION_TYPE_CANT_BE_INFERRED_WITH_ISOLATEDDECLARATIONS,
                    diagnostic_codes::EXPRESSION_TYPE_CANT_BE_INFERRED_WITH_ISOLATEDDECLARATIONS,
                );
                return;
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                // TS9017 for non-const arrays in default exports
                self.error_at_node(
                    expr_idx,
                    diagnostic_messages::ONLY_CONST_ARRAYS_CAN_BE_INFERRED_WITH_ISOLATEDDECLARATIONS,
                    diagnostic_codes::ONLY_CONST_ARRAYS_CAN_BE_INFERRED_WITH_ISOLATEDDECLARATIONS,
                );
                return;
            }
            // `as const` expressions
            k if k == syntax_kind_ext::AS_EXPRESSION => {
                // Check if the expression inside is an array/object literal with `as const`
                if let Some(assertion) = self.ctx.arena.get_type_assertion(expr_node) {
                    // Check if this is `<something> as const` where something has non-inferrable expressions
                    if let Some(inner_node) = self.ctx.arena.get(assertion.expression)
                        && (inner_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                            || inner_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
                    {
                        // Check if inner elements contain non-inferrable expressions
                        if self.has_non_inferrable_elements(assertion.expression) {
                            self.error_at_node(
                                    assertion.expression,
                                    diagnostic_messages::EXPRESSION_TYPE_CANT_BE_INFERRED_WITH_ISOLATEDDECLARATIONS,
                                    diagnostic_codes::EXPRESSION_TYPE_CANT_BE_INFERRED_WITH_ISOLATEDDECLARATIONS,
                                );
                        }
                    }
                }
                return;
            }
            _ => {}
        }

        // For binary expressions and other complex expressions: TS9037 for default exports
        self.error_at_node(
            expr_idx,
            diagnostic_messages::DEFAULT_EXPORTS_CANT_BE_INFERRED_WITH_ISOLATEDDECLARATIONS,
            diagnostic_codes::DEFAULT_EXPORTS_CANT_BE_INFERRED_WITH_ISOLATEDDECLARATIONS,
        );
    }

    /// Check if an array/object literal contains non-inferrable elements.
    fn has_non_inferrable_elements(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            if let Some(obj) = self.ctx.arena.get_literal_expr(expr_node) {
                for &elem_idx in &obj.elements.nodes {
                    if let Some(elem_node) = self.ctx.arena.get(elem_idx)
                        && let Some(prop_assign) = self.ctx.arena.get_property_assignment(elem_node)
                        && !self.is_isolated_decl_simple_value(prop_assign.initializer)
                    {
                        return true;
                    }
                }
            }
        } else if expr_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            && let Some(arr) = self.ctx.arena.get_literal_expr(expr_node)
        {
            for &elem_idx in &arr.elements.nodes {
                if !self.is_isolated_decl_simple_value(elem_idx) {
                    return true;
                }
            }
        }
        false
    }

    /// Check if a value is simple enough for isolated declarations inference.
    fn is_isolated_decl_simple_value(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return true;
        };
        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::BigIntLiteral as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                true
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION =>
            {
                !self.has_non_inferrable_elements(idx)
            }
            // `expr as Type` or `expr satisfies Type` with explicit type is inferrable
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                true
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                // -1, +1 etc
                true
            }
            _ => false,
        }
    }

    /// Check if a variable statement uses `const`.
    fn is_const_variable_statement(&self, stmt_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };
        let Some(var_data) = self.ctx.arena.get_variable(node) else {
            return false;
        };
        // Check declaration list for CONST flag via node flags
        for &list_idx in &var_data.declarations.nodes {
            if let Some(list_node) = self.ctx.arena.get(list_idx)
                && list_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            {
                use tsz_parser::parser::flags::node_flags;
                if list_node.flags as u32 & node_flags::CONST != 0 {
                    return true;
                }
            }
        }
        false
    }

    /// Check if a node is a binding pattern (array or object destructuring).
    fn is_binding_pattern(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
            || node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
    }

    /// Report TS9019 for each named binding element in a binding pattern.
    fn report_isolated_decl_binding_elements(&mut self, pattern_idx: NodeIndex) {
        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };
        let Some(pattern) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return;
        };
        let element_indices: Vec<NodeIndex> = pattern.elements.nodes.clone();
        for elem_idx in element_indices {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            if elem_node.kind == syntax_kind_ext::BINDING_ELEMENT {
                if let Some(binding_elem) = self.ctx.arena.get_binding_element(elem_node) {
                    let name_idx = binding_elem.name;
                    if self.is_binding_pattern(name_idx) {
                        self.report_isolated_decl_binding_elements(name_idx);
                    } else if name_idx.is_some() {
                        self.error_at_node(
                            name_idx,
                            diagnostic_messages::BINDING_ELEMENTS_CANT_BE_EXPORTED_DIRECTLY_WITH_ISOLATEDDECLARATIONS,
                            diagnostic_codes::BINDING_ELEMENTS_CANT_BE_EXPORTED_DIRECTLY_WITH_ISOLATEDDECLARATIONS,
                        );
                    }
                }
            }
        }
    }

    /// Check if an array literal has spread elements.
    fn array_has_spread(&self, arr_idx: NodeIndex) -> bool {
        let Some(arr_node) = self.ctx.arena.get(arr_idx) else {
            return false;
        };
        let Some(arr) = self.ctx.arena.get_literal_expr(arr_node) else {
            return false;
        };
        for &elem_idx in &arr.elements.nodes {
            if let Some(elem_node) = self.ctx.arena.get(elem_idx)
                && elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT
            {
                return true;
            }
        }
        false
    }

    /// Check for TS9022: class expressions in exported positions.
    pub(crate) fn check_isolated_decl_class_expressions(&mut self, stmts: &[NodeIndex]) {
        if !self.ctx.isolated_declarations() || self.ctx.is_declaration_file() {
            return;
        }

        for &stmt_idx in stmts {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            match node.kind {
                syntax_kind_ext::VARIABLE_STATEMENT => {
                    if self.is_declaration_exported(self.ctx.arena, stmt_idx) {
                        self.scan_for_class_expressions(stmt_idx);
                    }
                }
                syntax_kind_ext::EXPORT_DECLARATION => {
                    // Unwrap EXPORT_DECLARATION to check the inner variable statement
                    if let Some(export_decl) = self.ctx.arena.get_export_decl(node) {
                        let export_clause = export_decl.export_clause;
                        if export_clause.is_some()
                            && let Some(inner_node) = self.ctx.arena.get(export_clause)
                            && inner_node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                        {
                            self.scan_for_class_expressions(export_clause);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Scan a variable statement for class expressions (TS9022).
    fn scan_for_class_expressions(&mut self, stmt_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };
        let Some(var_data) = self.ctx.arena.get_variable(node) else {
            return;
        };
        for &list_idx in &var_data.declarations.nodes {
            let Some(list_node) = self.ctx.arena.get(list_idx) else {
                continue;
            };
            let Some(decl_list) = self.ctx.arena.get_variable(list_node) else {
                continue;
            };
            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                let Some(decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                if decl.initializer.is_some() {
                    self.check_for_class_expression(decl.initializer);
                }
            }
        }
    }

    /// Check an expression for class expressions recursively (TS9022).
    fn check_for_class_expression(&mut self, expr_idx: NodeIndex) {
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return;
        };
        match expr_node.kind {
            syntax_kind_ext::CLASS_EXPRESSION => {
                self.error_at_node(
                    expr_idx,
                    diagnostic_messages::INFERENCE_FROM_CLASS_EXPRESSIONS_IS_NOT_SUPPORTED_WITH_ISOLATEDDECLARATIONS,
                    diagnostic_codes::INFERENCE_FROM_CLASS_EXPRESSIONS_IS_NOT_SUPPORTED_WITH_ISOLATEDDECLARATIONS,
                );
            }
            syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                if let Some(arr) = self.ctx.arena.get_literal_expr(expr_node) {
                    for &elem_idx in &arr.elements.nodes {
                        self.check_for_class_expression(elem_idx);
                    }
                }
            }
            syntax_kind_ext::AS_EXPRESSION | syntax_kind_ext::SATISFIES_EXPRESSION => {
                if let Some(assertion) = self.ctx.arena.get_type_assertion(expr_node) {
                    self.check_for_class_expression(assertion.expression);
                }
            }
            syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(expr_node) {
                    self.check_for_class_expression(paren.expression);
                }
            }
            _ => {}
        }
    }

    /// Check for TS9026: module augmentation requires preserving imports.
    pub(crate) fn check_isolated_decl_augmentations(&mut self, stmts: &[NodeIndex]) {
        if !self.ctx.isolated_declarations() || self.ctx.is_declaration_file() {
            return;
        }

        // Look for `declare module './xxx' { ... }` augmentations in the file
        let has_augmentation = stmts.iter().any(|&stmt_idx| {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                return false;
            };
            if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                return false;
            }
            let Some(module) = self.ctx.arena.get_module(node) else {
                return false;
            };
            // Module augmentation: name is a string literal and body exists
            let Some(name_node) = self.ctx.arena.get(module.name) else {
                return false;
            };
            name_node.kind == SyntaxKind::StringLiteral as u16 && module.body.is_some()
        });

        if !has_augmentation {
            return;
        }

        // If there are augmentations, check imports that need preserving
        for &stmt_idx in stmts {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                // This import may be needed for augmentations
                self.error_at_node(
                    stmt_idx,
                    diagnostic_messages::DECLARATION_EMIT_FOR_THIS_FILE_REQUIRES_PRESERVING_THIS_IMPORT_FOR_AUGMENTATIONS,
                    diagnostic_codes::DECLARATION_EMIT_FOR_THIS_FILE_REQUIRES_PRESERVING_THIS_IMPORT_FOR_AUGMENTATIONS,
                );
            }
        }
    }
}
