//! Function type resolution helpers: JSDoc type predicates, enclosing type
//! parameter resolution, arguments object detection, and contextual rest
//! parameter evaluation.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{TypeId, TypeParamInfo};

impl<'a> CheckerState<'a> {
    /// Extract a type predicate from JSDoc `@returns {x is Type}` / `@return {this is Entry}`.
    ///
    /// Parse JSDoc `@return` for type predicates and build `TypePredicate` with parameter index.
    pub(crate) fn extract_jsdoc_return_type_predicate(
        &mut self,
        func_jsdoc: &Option<String>,
        params: &[tsz_solver::ParamInfo],
    ) -> Option<tsz_solver::TypePredicate> {
        use tsz_solver::{TypePredicate, TypePredicateTarget};

        let jsdoc = func_jsdoc.as_ref()?;
        let (is_asserts, param_name, type_str) = Self::jsdoc_returns_type_predicate(jsdoc)?;

        // Build the target
        let target = if param_name == "this" {
            TypePredicateTarget::This
        } else {
            let atom = self.ctx.types.intern_string(&param_name);
            TypePredicateTarget::Identifier(atom)
        };

        // Resolve the type (if present)
        let type_id = type_str.and_then(|ts| self.resolve_jsdoc_type_str(&ts));

        // Find parameter index for identifier targets
        let mut parameter_index = None;
        if let TypePredicateTarget::Identifier(name) = &target {
            parameter_index = params.iter().position(|p| p.name == Some(*name));
        }

        Some(TypePredicate {
            asserts: is_asserts,
            target,
            type_id,
            parameter_index,
        })
    }

    pub(crate) fn contextual_type_params_from_expected(
        &self,
        expected: TypeId,
    ) -> Option<Vec<TypeParamInfo>> {
        tsz_solver::type_queries::extract_contextual_type_params(self.ctx.types, expected)
    }

    /// Check if a function body references the `arguments` object.
    /// Walks the AST recursively but stops at nested function boundaries.
    /// Used by JS files to determine if a function needs an implicit rest parameter.
    pub(crate) fn body_has_arguments_reference(&self, body: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(body) else {
            return false;
        };

        // Check if this node is an identifier named "arguments"
        if let Some(ident) = self.ctx.arena.get_identifier(node) {
            return ident.escaped_text == "arguments";
        }

        // Stop at nested function/method/class boundaries
        let k = node.kind;
        if k == syntax_kind_ext::FUNCTION_DECLARATION
            || k == syntax_kind_ext::FUNCTION_EXPRESSION
            || k == syntax_kind_ext::ARROW_FUNCTION
            || k == syntax_kind_ext::METHOD_DECLARATION
            || k == syntax_kind_ext::CLASS_DECLARATION
            || k == syntax_kind_ext::CLASS_EXPRESSION
        {
            return false;
        }

        // Walk children based on node kind
        if let Some(block) = self.ctx.arena.get_block(node) {
            for &stmt in &block.statements.nodes {
                if self.body_has_arguments_reference(stmt) {
                    return true;
                }
            }
        } else if let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) {
            if self.body_has_arguments_reference(expr_stmt.expression) {
                return true;
            }
        } else if let Some(var_stmt) = self.ctx.arena.get_variable(node) {
            for &decl in &var_stmt.declarations.nodes {
                if self.body_has_arguments_reference(decl) {
                    return true;
                }
            }
        } else if let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) {
            if self.body_has_arguments_reference(var_decl.initializer) {
                return true;
            }
        } else if let Some(ret) = self.ctx.arena.get_return_statement(node) {
            if self.body_has_arguments_reference(ret.expression) {
                return true;
            }
        } else if let Some(call) = self.ctx.arena.get_call_expr(node) {
            if self.body_has_arguments_reference(call.expression) {
                return true;
            }
            if let Some(ref args) = call.arguments {
                for &arg in &args.nodes {
                    if self.body_has_arguments_reference(arg) {
                        return true;
                    }
                }
            }
        } else if let Some(bin) = self.ctx.arena.get_binary_expr(node) {
            if self.body_has_arguments_reference(bin.left)
                || self.body_has_arguments_reference(bin.right)
            {
                return true;
            }
        } else if let Some(access) = self.ctx.arena.get_access_expr(node) {
            if self.body_has_arguments_reference(access.expression) {
                return true;
            }
            // Element access: also check the argument (e.g. arguments[0])
            if self.body_has_arguments_reference(access.name_or_argument) {
                return true;
            }
        } else if let Some(if_stmt) = self.ctx.arena.get_if_statement(node) {
            if self.body_has_arguments_reference(if_stmt.expression)
                || self.body_has_arguments_reference(if_stmt.then_statement)
                || self.body_has_arguments_reference(if_stmt.else_statement)
            {
                return true;
            }
        } else if let Some(loop_stmt) = self.ctx.arena.get_loop(node) {
            if self.body_has_arguments_reference(loop_stmt.initializer)
                || self.body_has_arguments_reference(loop_stmt.condition)
                || self.body_has_arguments_reference(loop_stmt.incrementor)
                || self.body_has_arguments_reference(loop_stmt.statement)
            {
                return true;
            }
        } else if let Some(for_in_of) = self.ctx.arena.get_for_in_of(node) {
            if self.body_has_arguments_reference(for_in_of.expression)
                || self.body_has_arguments_reference(for_in_of.statement)
            {
                return true;
            }
        } else if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
            if self.body_has_arguments_reference(paren.expression) {
                return true;
            }
        } else if let Some(unary) = self.ctx.arena.get_unary_expr(node) {
            if self.body_has_arguments_reference(unary.operand) {
                return true;
            }
        } else if let Some(unary_ex) = self.ctx.arena.get_unary_expr_ex(node) {
            if self.body_has_arguments_reference(unary_ex.expression) {
                return true;
            }
        } else if let Some(spread) = self.ctx.arena.get_spread(node) {
            if self.body_has_arguments_reference(spread.expression) {
                return true;
            }
        } else if let Some(cond) = self.ctx.arena.get_conditional_expr(node)
            && (self.body_has_arguments_reference(cond.condition)
                || self.body_has_arguments_reference(cond.when_true)
                || self.body_has_arguments_reference(cond.when_false))
        {
            return true;
        }

        false
    }

    /// Push type parameters from all enclosing generic functions/classes/interfaces.
    pub(crate) fn push_enclosing_type_parameters(
        &mut self,
        func_idx: NodeIndex,
    ) -> Vec<(String, Option<TypeId>, bool)> {
        use tsz_parser::parser::syntax_kind_ext;

        let mut enclosing_param_indices: Vec<Vec<NodeIndex>> = Vec::new();
        let mut current = func_idx;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let Some(parent) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            let type_param_nodes: Option<Vec<NodeIndex>> = match parent.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION =>
                {
                    self.ctx
                        .arena
                        .get_function(parent)
                        .and_then(|f| f.type_parameters.as_ref())
                        .map(|tp| tp.nodes.clone())
                }
                k if k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION =>
                {
                    self.ctx
                        .arena
                        .get_class(parent)
                        .and_then(|c| c.type_parameters.as_ref())
                        .map(|tp| tp.nodes.clone())
                }
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => self
                    .ctx
                    .arena
                    .get_interface(parent)
                    .and_then(|i| i.type_parameters.as_ref())
                    .map(|tp| tp.nodes.clone()),
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(parent)
                    .and_then(|m| m.type_parameters.as_ref())
                    .map(|tp| tp.nodes.clone()),
                _ => None,
            };

            if let Some(indices) = type_param_nodes {
                enclosing_param_indices.push(indices);
            }

            current = parent_idx;
        }

        if enclosing_param_indices.is_empty() {
            return Vec::new();
        }

        let mut updates = Vec::new();
        let mut added_params: Vec<NodeIndex> = Vec::new();
        let factory = self.ctx.types.factory();

        // Pass 1: Add all type parameters to scope WITHOUT constraints
        for param_indices in enclosing_param_indices.into_iter().rev() {
            for param_idx in param_indices {
                let Some(node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(data) = self.ctx.arena.get_type_parameter(node) else {
                    continue;
                };

                let name = self
                    .ctx
                    .arena
                    .get(data.name)
                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                    .map_or_else(|| "T".to_string(), |id_data| id_data.escaped_text.clone());
                let atom = self.ctx.types.intern_string(&name);

                let is_const = self
                    .ctx
                    .arena
                    .has_modifier(&data.modifiers, tsz_scanner::SyntaxKind::ConstKeyword);
                let info = tsz_solver::TypeParamInfo {
                    name: atom,
                    constraint: None,
                    default: None,
                    is_const,
                };
                let type_id = factory.type_param(info);

                let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
                updates.push((name, previous, false));
                added_params.push(param_idx);
            }
        }

        // Pass 2: Resolve constraints now that all type parameters are in scope
        for param_idx in added_params {
            let Some(node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(data) = self.ctx.arena.get_type_parameter(node) else {
                continue;
            };

            if data.constraint == NodeIndex::NONE {
                continue;
            }

            let name = self
                .ctx
                .arena
                .get(data.name)
                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                .map_or_else(|| "T".to_string(), |id_data| id_data.escaped_text.clone());
            let atom = self.ctx.types.intern_string(&name);

            let constraint_type = self.get_type_from_type_node(data.constraint);
            let constraint = (constraint_type != TypeId::ERROR).then_some(constraint_type);

            let is_const = self
                .ctx
                .arena
                .has_modifier(&data.modifiers, tsz_scanner::SyntaxKind::ConstKeyword);
            let info = tsz_solver::TypeParamInfo {
                name: atom,
                constraint,
                default: None,
                is_const,
            };
            let constrained_type_id = factory.type_param(info);
            self.ctx
                .type_parameter_scope
                .insert(name, constrained_type_id);
        }

        updates
    }

    /// Evaluate Application types in rest parameters of contextual function types.
    pub(crate) fn evaluate_contextual_rest_param_applications(
        &mut self,
        type_id: TypeId,
    ) -> TypeId {
        use tsz_solver::type_queries::get_function_shape;

        let Some(shape) = get_function_shape(self.ctx.types, type_id) else {
            return type_id;
        };

        let Some(last_param) = shape.params.last() else {
            return type_id;
        };

        if !last_param.rest {
            return type_id;
        }

        // Only try to evaluate if the rest param type is an Application
        if !tsz_solver::is_generic_application(self.ctx.types, last_param.type_id) {
            return type_id;
        }

        let evaluated_rest = self.evaluate_application_type(last_param.type_id);
        if evaluated_rest == last_param.type_id {
            return type_id;
        }

        // Create a new function shape with the evaluated rest param type
        let mut new_params = shape.params.clone();
        new_params
            .last_mut()
            .expect("new_params cloned from non-empty shape.params")
            .type_id = evaluated_rest;

        let new_shape = tsz_solver::FunctionShape {
            type_params: shape.type_params.clone(),
            params: new_params,
            this_type: shape.this_type,
            return_type: shape.return_type,
            type_predicate: shape.type_predicate.clone(),
            is_constructor: shape.is_constructor,
            is_method: shape.is_method,
        };

        self.ctx.types.function(new_shape)
    }
}
