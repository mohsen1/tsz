//! Child-node traversal dispatch for the lowering pass.
//!
//! Contains `visit_children`, the large match-dispatch that recursively visits
//! child nodes of each AST node kind.  Extracted from `lowering_pass.rs` to
//! keep the main file focused on transform-producing `visit_*` methods.

use super::*;

impl<'a> LoweringPass<'a> {
    /// Visit all children of a node
    pub(super) fn visit_children(&mut self, idx: NodeIndex) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::SOURCE_FILE => {
                if let Some(sf) = self.arena.get_source_file(node) {
                    for &stmt in &sf.statements.nodes {
                        self.visit(stmt);
                    }
                }
            }
            k if k == syntax_kind_ext::BLOCK || k == syntax_kind_ext::CASE_BLOCK => {
                if let Some(block) = self.get_block_like(node) {
                    let statements = block.statements.nodes.clone();
                    for stmt in statements {
                        self.visit(stmt);
                    }
                }
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = self.arena.get_variable(node) {
                    for &decl_list in &var_stmt.declarations.nodes {
                        self.visit(decl_list);
                    }
                }
            }
            k if k == syntax_kind_ext::VARIABLE_DECLARATION_LIST => {
                if let Some(decl_list) = self.arena.get_variable(node) {
                    if (node.flags as u32 & tsz_parser::parser::node_flags::USING) != 0
                        && !self.ctx.options.target.supports_es2025()
                    {
                        self.transforms.helpers_mut().add_disposable_resource = true;
                        self.transforms.helpers_mut().dispose_resources = true;
                    }

                    if self.ctx.target_es5 {
                        self.transforms.insert(
                            idx,
                            TransformDirective::ES5VariableDeclarationList { decl_list: idx },
                        );

                        let need_downlevel_read = self.ctx.options.downlevel_iteration
                            && decl_list.declarations.nodes.iter().any(|&decl_idx| {
                                if let Some(decl_node) = self.arena.get(decl_idx) {
                                    if let Some(decl) =
                                        self.arena.get_variable_declaration(decl_node)
                                    {
                                        if decl.initializer.is_none() {
                                            return false;
                                        }

                                        self.arena.get(decl.name).is_some_and(|name_node| {
                                            name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                                        })
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                }
                            });

                        if need_downlevel_read {
                            self.transforms.helpers_mut().read = true;
                        }
                    }
                    for &decl in &decl_list.declarations.nodes {
                        self.visit(decl);
                    }
                }
            }
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                if let Some(decl) = self.arena.get_variable_declaration(node) {
                    self.visit(decl.name);
                    if decl.initializer.is_some() {
                        self.visit(decl.initializer);
                    }
                }
            }
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.arena.get_expression_statement(node) {
                    self.visit(expr_stmt.expression);
                }
            }
            k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                if let Some(export_assign) = self.arena.get_export_assignment(node) {
                    self.visit(export_assign.expression);
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(node) {
                    self.visit(call.expression);
                    if let Some(ref args) = call.arguments {
                        for &arg_idx in &args.nodes {
                            self.visit(arg_idx);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.arena.get_binary_expr(node) {
                    // If this is an assignment (=) with an array/object literal on the left,
                    // mark as assignment target so we don't treat it as spread-in-array-literal
                    let is_destructuring_assignment = bin.operator_token
                        == tsz_scanner::SyntaxKind::EqualsToken as u16
                        && self.arena.get(bin.left).is_some_and(|n| {
                            n.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                                || n.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        });
                    if is_destructuring_assignment {
                        let prev = self.in_assignment_target;
                        self.in_assignment_target = true;
                        self.visit(bin.left);
                        self.in_assignment_target = prev;
                    } else {
                        self.visit(bin.left);
                    }
                    self.visit(bin.right);
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(access) = self.arena.get_access_expr(node) {
                    self.visit(access.expression);
                    self.visit(access.name_or_argument);
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                if let Some(prop) = self.arena.get_property_assignment(node) {
                    self.visit(prop.name);
                    self.visit(prop.initializer);
                }
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                if let Some(prop) = self.arena.get_property_decl(node) {
                    // Set __metadata helper when a decorated property exists
                    if self.ctx.options.legacy_decorators
                        && self.ctx.options.emit_decorator_metadata
                        && prop.modifiers.as_ref().is_some_and(|m| {
                            m.nodes.iter().any(|&mod_idx| {
                                self.arena
                                    .get(mod_idx)
                                    .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                            })
                        })
                    {
                        self.transforms.helpers_mut().metadata = true;
                    }
                    if let Some(mods) = &prop.modifiers {
                        for &mod_idx in &mods.nodes {
                            self.visit(mod_idx);
                        }
                    }
                    self.visit(prop.name);
                    if prop.initializer.is_some() {
                        self.visit(prop.initializer);
                    }
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.arena.get_method_decl(node) {
                    let is_async_method = method.modifiers.as_ref().is_some_and(|mods| {
                        mods.nodes.iter().any(|&mod_idx| {
                            self.arena
                                .get(mod_idx)
                                .is_some_and(|n| n.kind == SyntaxKind::AsyncKeyword as u16)
                        })
                    });
                    if is_async_method && self.ctx.needs_async_lowering && method.body.is_some() {
                        if method.asterisk_token {
                            // Async generator method: needs __asyncGenerator + __await
                            if self.ctx.target_es5 {
                                self.mark_async_helpers();
                            }
                            self.mark_async_generator_helpers();
                        } else {
                            // Non-generator async method: needs __awaiter
                            // (ES2015/ES2016 use __awaiter + generators via yield,
                            //  ES5 additionally needs __generator)
                            self.mark_async_helpers();
                        }
                    }
                    // Set __metadata helper when a decorated method WITH a body exists.
                    // Overload signatures (no body) are not emitted as __decorate targets.
                    let is_overload = !method.body.is_some();
                    if !is_overload
                        && self.ctx.options.legacy_decorators
                        && self.ctx.options.emit_decorator_metadata
                        && method.modifiers.as_ref().is_some_and(|m| {
                            m.nodes.iter().any(|&mod_idx| {
                                self.arena
                                    .get(mod_idx)
                                    .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                            })
                        })
                    {
                        self.transforms.helpers_mut().metadata = true;
                    }
                    if let Some(mods) = &method.modifiers {
                        // For overload signatures (no body), save/restore the decorate
                        // flag to prevent decorator visits from triggering helper emission.
                        let prev_decorate = if is_overload {
                            Some(self.transforms.helpers().decorate)
                        } else {
                            None
                        };
                        for &mod_idx in &mods.nodes {
                            self.visit(mod_idx);
                        }
                        if let Some(prev) = prev_decorate {
                            self.transforms.helpers_mut().decorate = prev;
                        }
                    }
                    self.visit(method.name);
                    for &param_idx in &method.parameters.nodes {
                        self.visit(param_idx);
                    }
                    if method.body.is_some() {
                        if self.ctx.target_es5 {
                            let cn = self.compute_this_capture_name_with_params(
                                method.body,
                                Some(&method.parameters),
                            );
                            self.enclosing_function_bodies.push(method.body);
                            self.enclosing_capture_names.push(cn);
                        }
                        self.visit(method.body);
                        if self.ctx.target_es5 {
                            self.enclosing_function_bodies.pop();
                            self.enclosing_capture_names.pop();
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                if let Some(ctor) = self.arena.get_constructor(node) {
                    // Skip decorator processing for constructor modifiers.
                    // Constructor decorators are errors — tsc doesn't emit
                    // __decorate helpers for them.
                    if let Some(mods) = &ctor.modifiers {
                        let prev_decorate = self.transforms.helpers().decorate;
                        for &mod_idx in &mods.nodes {
                            self.visit(mod_idx);
                        }
                        // Restore decorate flag — don't let constructor decorators
                        // trigger helper emission
                        self.transforms.helpers_mut().decorate = prev_decorate;
                    }
                    for &param_idx in &ctor.parameters.nodes {
                        self.visit(param_idx);
                    }
                    if ctor.body.is_some() {
                        if self.ctx.target_es5 {
                            let cn = self.compute_this_capture_name_with_params(
                                ctor.body,
                                Some(&ctor.parameters),
                            );
                            self.enclosing_function_bodies.push(ctor.body);
                            self.enclosing_capture_names.push(cn);
                        }
                        self.visit(ctor.body);
                        if self.ctx.target_es5 {
                            self.enclosing_function_bodies.pop();
                            self.enclosing_capture_names.pop();
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    if let Some(mods) = &accessor.modifiers {
                        for &mod_idx in &mods.nodes {
                            self.visit(mod_idx);
                        }
                    }
                    self.visit(accessor.name);
                    for &param_idx in &accessor.parameters.nodes {
                        self.visit(param_idx);
                    }
                    if accessor.body.is_some() {
                        if self.ctx.target_es5 {
                            let cn = self.compute_this_capture_name_with_params(
                                accessor.body,
                                Some(&accessor.parameters),
                            );
                            self.enclosing_function_bodies.push(accessor.body);
                            self.enclosing_capture_names.push(cn);
                        }
                        self.visit(accessor.body);
                        if self.ctx.target_es5 {
                            self.enclosing_function_bodies.pop();
                            self.enclosing_capture_names.pop();
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => {
                if let Some(func) = self.arena.get_function(node) {
                    for &param_idx in &func.parameters.nodes {
                        self.visit(param_idx);
                    }
                    if func.body.is_some() {
                        if self.ctx.target_es5 {
                            let cn = self.compute_this_capture_name_with_params(
                                func.body,
                                Some(&func.parameters),
                            );
                            self.enclosing_function_bodies.push(func.body);
                            self.enclosing_capture_names.push(cn);
                        }
                        self.visit(func.body);
                        if self.ctx.target_es5 {
                            self.enclosing_function_bodies.pop();
                            self.enclosing_capture_names.pop();
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::CLASS_EXPRESSION => {
                if let Some(class_data) = self.arena.get_class(node) {
                    // TC39 (non-legacy) decorator detection for class expressions
                    let target_supports_native_decorators =
                        self.ctx.options.target == tsz_common::ScriptTarget::ESNext;
                    let has_tc39_decorators = !self.ctx.options.legacy_decorators
                        && !target_supports_native_decorators
                        && self.class_has_decorators(class_data);

                    if has_tc39_decorators {
                        let needs_prop_key = self.class_has_computed_decorated_member(class_data);
                        let needs_set_function_name =
                            self.class_has_private_decorated_member(class_data);
                        let has_class_decorators =
                            class_data.modifiers.as_ref().is_some_and(|mods| {
                                mods.nodes.iter().any(|&mod_idx| {
                                    self.arena
                                        .get(mod_idx)
                                        .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                                })
                            });
                        let helpers = self.transforms.helpers_mut();
                        helpers.es_decorate = true;
                        helpers.run_initializers = true;
                        if needs_prop_key {
                            helpers.prop_key = true;
                        }
                        if needs_set_function_name || has_class_decorators {
                            helpers.set_function_name = true;
                        }
                    }

                    let needs_es5_transform = self.ctx.target_es5;
                    if has_tc39_decorators && !needs_es5_transform {
                        // TC39 decorator transform for class expressions
                        // Determine function name for __setFunctionName from context
                        let fn_name = self.infer_class_expression_function_name(idx, node);
                        self.transforms.insert(
                            idx,
                            TransformDirective::TC39Decorators {
                                class_node: idx,
                                function_name: fn_name,
                            },
                        );
                    } else if needs_es5_transform {
                        self.transforms.insert(
                            idx,
                            TransformDirective::ES5ClassExpression { class_node: idx },
                        );
                        let heritage = self.get_extends_heritage(&class_data.heritage_clauses);
                        self.mark_class_helpers(idx, heritage);
                    } else if self.ctx.needs_es2022_lowering
                        && (self.class_has_auto_accessor_members(class_data)
                            || self.class_has_private_members(class_data))
                    {
                        let heritage = self.get_extends_heritage(&class_data.heritage_clauses);
                        self.mark_class_helpers(idx, heritage);
                    }
                    if let Some(mods) = &class_data.modifiers {
                        for &mod_idx in &mods.nodes {
                            self.visit(mod_idx);
                        }
                    }
                    for &member in &class_data.members.nodes {
                        self.visit(member);
                    }
                }
            }
            k if k == syntax_kind_ext::PARAMETER => {
                if let Some(param) = self.arena.get_parameter(node) {
                    if let Some(mods) = &param.modifiers {
                        // Check if any modifier is a decorator — set __param helper flag
                        if self.ctx.options.legacy_decorators {
                            let has_decorator = mods.nodes.iter().any(|&mod_idx| {
                                self.arena
                                    .get(mod_idx)
                                    .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                            });
                            if has_decorator {
                                self.transforms.helpers_mut().param = true;
                            }
                        }
                        for &mod_idx in &mods.nodes {
                            self.visit(mod_idx);
                        }
                    }
                    self.visit(param.name);
                    if param.initializer.is_some() {
                        self.visit(param.initializer);
                    }
                }
            }
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
            {
                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    if (self.ctx.target_es5 || self.ctx.needs_es2018_lowering)
                        && node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        && pattern.elements.nodes.iter().any(|&elem_idx| {
                            let Some(elem_node) = self.arena.get(elem_idx) else {
                                return false;
                            };
                            self.arena
                                .get_binding_element(elem_node)
                                .is_some_and(|elem| elem.dot_dot_dot_token)
                        })
                    {
                        self.transforms.helpers_mut().rest = true;
                    }
                    for &elem in &pattern.elements.nodes {
                        self.visit(elem);
                    }
                }
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                if let Some(elem) = self.arena.get_binding_element(node) {
                    if elem.property_name.is_some() {
                        self.visit(elem.property_name);
                    }
                    self.visit(elem.name);
                    if elem.initializer.is_some() {
                        self.visit(elem.initializer);
                    }
                }
            }
            k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                if let Some(computed) = self.arena.get_computed_property(node) {
                    self.visit(computed.expression);
                }
            }
            k if k == syntax_kind_ext::DECORATOR => {
                if let Some(decorator) = self.arena.get_decorator(node) {
                    self.visit(decorator.expression);
                }
                if self.ctx.options.legacy_decorators {
                    self.transforms.helpers_mut().decorate = true;
                    // Note: __metadata helper is set at the member level, not here,
                    // to avoid emitting it for class-only decorators without members.
                }
            }
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 && self.ctx.target_es5 => {
                self.transforms.insert(
                    idx,
                    TransformDirective::ES5TemplateLiteral { template_node: idx },
                );
            }
            k if k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                if let Some(assertion) = self.arena.get_type_assertion(node) {
                    self.visit(assertion.expression);
                }
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr_ex(node) {
                    self.visit(unary.expression);
                }
            }
            k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
                if self.ctx.target_es5 {
                    self.transforms.insert(
                        idx,
                        TransformDirective::ES5TemplateLiteral { template_node: idx },
                    );
                    self.transforms.helpers_mut().make_template_object = true;
                }
                if let Some(tagged) = self.arena.get_tagged_template(node) {
                    self.visit(tagged.tag);
                    self.visit(tagged.template);
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                if self.ctx.target_es5 {
                    self.transforms.insert(
                        idx,
                        TransformDirective::ES5TemplateLiteral { template_node: idx },
                    );
                }
                if let Some(template) = self.arena.get_template_expr(node) {
                    self.visit(template.head);
                    for &span_idx in &template.template_spans.nodes {
                        self.visit(span_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_SPAN => {
                if let Some(span) = self.arena.get_template_span(node) {
                    self.visit(span.expression);
                    self.visit(span.literal);
                }
            }
            k if k == syntax_kind_ext::SPREAD_ELEMENT
                || k == syntax_kind_ext::SPREAD_ASSIGNMENT =>
            {
                if let Some(spread) = self.arena.get_spread(node) {
                    self.visit(spread.expression);
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.arena.get_parenthesized(node) {
                    self.visit(paren.expression);
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION =>
            {
                if let Some(unary) = self.arena.get_unary_expr(node) {
                    self.visit(unary.operand);
                }
            }
            k if k == syntax_kind_ext::AWAIT_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr_ex(node) {
                    self.visit(unary.expression);
                }
            }
            k if k == syntax_kind_ext::YIELD_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr_ex(node) {
                    self.visit(unary.expression);
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.arena.get_conditional_expr(node) {
                    self.visit(cond.condition);
                    self.visit(cond.when_true);
                    self.visit(cond.when_false);
                }
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                if let Some(lit) = self.arena.get_literal_expr(node) {
                    // Skip transform if this is the left side of a destructuring assignment
                    if !self.in_assignment_target
                        && self.ctx.target_es5
                        && self.needs_es5_object_literal_transform(&lit.elements.nodes)
                    {
                        self.transforms.insert(
                            idx,
                            TransformDirective::ES5ObjectLiteral {
                                object_literal: idx,
                            },
                        );
                        // Mark __assign helper if object spread is detected
                        if lit
                            .elements
                            .nodes
                            .iter()
                            .any(|&idx| emit_utils::is_spread_element(self.arena, idx))
                        {
                            self.transforms.helpers_mut().assign = true;
                        }
                    }

                    for &elem in &lit.elements.nodes {
                        self.visit(elem);
                    }
                }
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                if let Some(lit) = self.arena.get_literal_expr(node) {
                    // Add ES5ArrayLiteral directive if targeting ES5 and spread elements are present.
                    // Skip if this is the left side of a destructuring assignment
                    // (e.g., [...rest] = arr) since that's not a real array literal.
                    let has_spread = !self.in_assignment_target
                        && self.needs_es5_array_literal_transform(&lit.elements.nodes);
                    if self.ctx.target_es5 && has_spread {
                        self.transforms.insert(
                            idx,
                            TransformDirective::ES5ArrayLiteral { array_literal: idx },
                        );
                        // Flag that __spreadArray helper is needed
                        self.transforms.helpers_mut().spread_array = true;
                        // When downlevelIteration is enabled, spread on iterables
                        // needs __read to convert iterator results to arrays.
                        if self.ctx.options.downlevel_iteration {
                            self.transforms.helpers_mut().read = true;
                        }
                    }

                    for &elem in &lit.elements.nodes {
                        self.visit(elem);
                    }
                }
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.arena.get_if_statement(node) {
                    self.visit(if_stmt.expression);
                    self.visit(if_stmt.then_statement);
                    if if_stmt.else_statement.is_some() {
                        self.visit(if_stmt.else_statement);
                    }
                }
            }
            k if k == syntax_kind_ext::FOR_STATEMENT
                || k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT =>
            {
                if let Some(loop_data) = self.arena.get_loop(node) {
                    self.visit(loop_data.initializer);
                    self.visit(loop_data.condition);
                    self.visit(loop_data.incrementor);
                    self.visit(loop_data.statement);
                }
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.arena.get_return_statement(node)
                    && ret.expression.is_some()
                {
                    self.visit(ret.expression);
                }
            }
            k if k == syntax_kind_ext::THROW_STATEMENT => {
                if let Some(thr) = self.arena.get_return_statement(node)
                    && thr.expression.is_some()
                {
                    self.visit(thr.expression);
                }
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch) = self.arena.get_switch(node) {
                    self.visit(switch.expression);
                    self.visit(switch.case_block);
                }
            }
            k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
                if let Some(clause) = self.arena.get_case_clause(node) {
                    if clause.expression.is_some() {
                        self.visit(clause.expression);
                    }
                    for &stmt in &clause.statements.nodes {
                        self.visit(stmt);
                    }
                }
            }
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_stmt) = self.arena.get_try(node) {
                    self.visit(try_stmt.try_block);
                    if try_stmt.catch_clause.is_some() {
                        self.visit(try_stmt.catch_clause);
                    }
                    if try_stmt.finally_block.is_some() {
                        self.visit(try_stmt.finally_block);
                    }
                }
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch) = self.arena.get_catch_clause(node) {
                    if catch.variable_declaration.is_some() {
                        self.visit(catch.variable_declaration);
                    }
                    self.visit(catch.block);
                }
            }
            _ => {}
        }
    }
}
