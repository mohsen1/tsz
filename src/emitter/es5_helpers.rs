use super::is_valid_identifier_name;
use super::{ParamTransform, ParamTransformPlan, Printer, RestParamTransform};
use crate::parser::node::{MethodDeclData, Node};
use crate::parser::syntax_kind_ext;
use crate::parser::{NodeIndex, NodeList};
use crate::scanner::SyntaxKind;
use crate::transform_context::TransformDirective;
use crate::transforms::class_es5::ClassES5Emitter;

impl<'a> Printer<'a> {
    pub(super) fn emit_object_literal_entries_es5(&mut self, elements: &[NodeIndex]) {
        if elements.is_empty() {
            self.write("{}");
            return;
        }

        if elements.len() > 1 {
            self.write("{");
            self.write_line();
            self.increase_indent();
            for (i, &prop) in elements.iter().enumerate() {
                self.emit_object_literal_member_es5(prop);
                if i < elements.len() - 1 {
                    self.write(",");
                }
                self.write_line();
            }
            self.decrease_indent();
            self.write("}");
        } else {
            self.write("{ ");
            self.emit_object_literal_member_es5(elements[0]);
            self.write(" }");
        }
    }

    pub(super) fn emit_object_literal_member_es5(&mut self, prop_idx: NodeIndex) {
        let Some(node) = self.arena.get(prop_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                if let Some(shorthand) = self.arena.get_shorthand_property(node) {
                    self.emit(shorthand.name);
                    self.write(": ");
                    self.emit(shorthand.name);
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.arena.get_method_decl(node) {
                    self.emit(method.name);
                    self.write(": ");
                    self.emit_object_literal_method_value_es5(method);
                }
            }
            _ => self.emit(prop_idx),
        }
    }

    pub(super) fn emit_object_literal_method_value_es5(&mut self, method: &MethodDeclData) {
        if method.body.is_none() {
            self.write("function () {}");
            return;
        }

        let is_async = self.has_modifier(&method.modifiers, SyntaxKind::AsyncKeyword as u16);
        if is_async {
            self.emit_async_function_es5_body("", &method.parameters.nodes, method.body, "this");
            return;
        }

        self.write("function");
        if method.asterisk_token {
            self.write("*");
        }
        self.write(" (");
        let param_transforms = self.emit_function_parameters_es5(&method.parameters.nodes);
        self.write(") ");
        if param_transforms.has_transforms() {
            self.emit_block_with_param_prologue(method.body, &param_transforms);
        } else {
            self.emit(method.body);
        }
    }

    /// Check if a property member has a computed property name
    pub(super) fn is_computed_property_member(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        let name_idx = match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                self.arena.get_property_assignment(node).map(|p| p.name)
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                self.arena.get_method_decl(node).map(|m| m.name)
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                self.arena.get_accessor(node).map(|a| a.name)
            }
            _ => None,
        };

        if let Some(name_idx) = name_idx
            && let Some(name_node) = self.arena.get(name_idx)
        {
            return name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME;
        }
        false
    }

    /// Emit ES5-compatible object literal with computed properties
    /// Pattern: { [k]: v } → (_a = {}, _a[k] = v, _a)
    /// Pattern: { a: 1, [k]: v, b: 2 } → (_a = { a: 1 }, _a[k] = v, _a.b = 2, _a)
    pub(super) fn emit_object_literal_es5(&mut self, elements: &[NodeIndex]) {
        if elements.is_empty() {
            self.write("{}");
            return;
        }

        // Find the index of the first computed property
        let first_computed_idx = elements
            .iter()
            .position(|&idx| {
                self.is_computed_property_member(idx) || {
                    self.arena
                        .get(idx)
                        .map(|n| {
                            n.kind == syntax_kind_ext::SPREAD_ASSIGNMENT
                                || n.kind == syntax_kind_ext::SPREAD_ELEMENT
                        })
                        .unwrap_or(false)
                }
            })
            .unwrap_or(elements.len());

        if first_computed_idx == elements.len() {
            self.emit_object_literal_entries_es5(elements);
            return;
        }

        // Get temp variable name
        let temp_var = self.ctx.destructuring_state.next_temp_var();

        self.write("(");
        self.write(&temp_var);
        self.write(" = ");

        // Emit initial non-computed properties as the object literal
        if first_computed_idx > 0 {
            self.emit_object_literal_entries_es5(&elements[..first_computed_idx]);
        } else {
            self.write("{}");
        }

        // Emit remaining properties as assignments
        for i in first_computed_idx..elements.len() {
            let prop_idx = elements[i];
            self.write(", ");
            self.emit_property_assignment_es5(prop_idx, &temp_var);
        }

        // Return the temp variable
        self.write(", ");
        self.write(&temp_var);
        self.write(")");
    }

    /// Emit a property assignment in ES5 computed property transform
    pub(super) fn emit_property_assignment_es5(&mut self, prop_idx: NodeIndex, temp_var: &str) {
        let Some(node) = self.arena.get(prop_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                if let Some(prop) = self.arena.get_property_assignment(node) {
                    self.emit_assignment_target_es5(prop.name, temp_var);
                    self.write(" = ");
                    self.emit(prop.initializer);
                }
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                if let Some(shorthand) = self.arena.get_shorthand_property(node) {
                    self.write(temp_var);
                    self.write(".");
                    self.write_identifier_text(shorthand.name);
                    self.write(" = ");
                    self.write_identifier_text(shorthand.name);
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.arena.get_method_decl(node) {
                    self.emit_assignment_target_es5(method.name, temp_var);
                    self.write(" = ");
                    self.emit_object_literal_method_value_es5(method);
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    self.write("Object.defineProperty(");
                    self.write(temp_var);
                    self.write(", ");
                    self.emit_property_key_string(accessor.name);
                    self.write(", { get: function () ");
                    self.emit(accessor.body);
                    self.write(", enumerable: true, configurable: true })");
                }
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    self.write("Object.defineProperty(");
                    self.write(temp_var);
                    self.write(", ");
                    self.emit_property_key_string(accessor.name);
                    self.write(", { set: function (");
                    self.emit_function_parameters_js(&accessor.parameters.nodes);
                    self.write(") ");
                    self.emit(accessor.body);
                    self.write(", enumerable: true, configurable: true })");
                }
            }
            k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                // Spread: { ...x } → Object.assign(_a, x)
                if let Some(spread) = self.arena.get_spread(node) {
                    self.write("Object.assign(");
                    self.write(temp_var);
                    self.write(", ");
                    self.emit(spread.expression);
                    self.write(")");
                }
            }
            k if k == syntax_kind_ext::SPREAD_ELEMENT => {
                // Spread: { ...x } → Object.assign(_a, x)
                if let Some(spread) = self.arena.unary_exprs_ex.get(node.data_index as usize) {
                    self.write("Object.assign(");
                    self.write(temp_var);
                    self.write(", ");
                    self.emit_expression(spread.expression);
                    self.write(")");
                }
            }
            _ => {}
        }
    }

    /// Emit assignment target for ES5 computed property transform
    /// For computed: _a[expr]
    /// For regular: _a.name
    pub(super) fn emit_assignment_target_es5(&mut self, name_idx: NodeIndex, temp_var: &str) {
        self.write(temp_var);

        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            // Computed property: _a[expr]
            if let Some(computed) = self.arena.get_computed_property(name_node) {
                self.write("[");
                self.emit(computed.expression);
                self.write("]");
            }
        } else if name_node.kind == SyntaxKind::Identifier as u16 {
            // Regular identifier: _a.name
            self.write(".");
            self.write_identifier_text(name_idx);
        } else if name_node.kind == SyntaxKind::StringLiteral as u16 {
            // String literal: _a["name"]
            if let Some(lit) = self.arena.get_literal(name_node) {
                self.write("[\"");
                self.write(&lit.text);
                self.write("\"]");
            }
        } else if name_node.kind == SyntaxKind::NumericLiteral as u16 {
            // Numeric literal: _a[123]
            if let Some(lit) = self.arena.get_literal(name_node) {
                self.write("[");
                self.write(&lit.text);
                self.write("]");
            }
        }
    }

    /// Emit property key as a string for Object.defineProperty
    pub(super) fn emit_property_key_string(&mut self, name_idx: NodeIndex) {
        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            // Computed property: emit the expression directly
            if let Some(computed) = self.arena.get_computed_property(name_node) {
                self.emit(computed.expression);
            }
        } else if name_node.kind == SyntaxKind::Identifier as u16 {
            self.write("\"");
            self.write_identifier_text(name_idx);
            self.write("\"");
        } else if name_node.kind == SyntaxKind::StringLiteral as u16 {
            if let Some(lit) = self.arena.get_literal(name_node) {
                self.write("\"");
                self.write(&lit.text);
                self.write("\"");
            }
        } else if name_node.kind == SyntaxKind::NumericLiteral as u16
            && let Some(lit) = self.arena.get_literal(name_node)
        {
            self.write(&lit.text);
        }
    }

    /// Emit ES5-compatible function expression for arrow function
    /// Arrow: (x) => x + 1  →  function (x) { return x + 1; }
    pub(super) fn emit_arrow_function_es5(
        &mut self,
        _node: &Node,
        func: &crate::parser::node::FunctionData,
        captures_this: bool,
    ) {
        let needs_this_capture = captures_this;
        let parent_this_expr = if self.ctx.arrow_state.this_capture_depth > 0 {
            "_this"
        } else {
            "this"
        };

        if needs_this_capture {
            self.write("(function (_this) { return ");
            self.ctx.arrow_state.this_capture_depth += 1;
        }

        if func.is_async {
            let this_expr = if needs_this_capture {
                "_this"
            } else {
                parent_this_expr
            };
            self.emit_async_function_es5(func, "", this_expr);
        } else {
            self.write("function (");
            let param_transforms = self.emit_function_parameters_es5(&func.parameters.nodes);
            self.write(") ");

            // If body is not a block (concise arrow), wrap with return
            let body_node = self.arena.get(func.body);
            let is_block = body_node
                .map(|n| n.kind == syntax_kind_ext::BLOCK)
                .unwrap_or(false);
            let needs_param_prologue = param_transforms.has_transforms();

            if is_block {
                // Check if it's a simple single-return block
                if let Some(block_node) = self.arena.get(func.body) {
                    if let Some(block) = self.arena.get_block(block_node) {
                        if !needs_param_prologue
                            && block.statements.nodes.len() == 1
                            && self.is_simple_return_statement(block.statements.nodes[0])
                        {
                            self.emit_single_line_block(func.body);
                        } else if needs_param_prologue {
                            self.emit_block_with_param_prologue(func.body, &param_transforms);
                        } else {
                            self.emit(func.body);
                        }
                    } else if needs_param_prologue {
                        self.emit_block_with_param_prologue(func.body, &param_transforms);
                    } else {
                        self.emit(func.body);
                    }
                } else if needs_param_prologue {
                    self.emit_block_with_param_prologue(func.body, &param_transforms);
                } else {
                    self.emit(func.body);
                }
            } else if needs_param_prologue {
                self.write("{");
                self.write_line();
                self.increase_indent();
                self.emit_param_prologue(&param_transforms);
                self.write("return ");
                self.emit(func.body);
                self.write(";");
                self.write_line();
                self.decrease_indent();
                self.write("}");
            } else {
                // Concise body: (x) => x + 1  →  function (x) { return x + 1; }
                self.write("{ return ");
                self.emit(func.body);
                self.write("; }");
            }
        }

        if needs_this_capture {
            self.ctx.arrow_state.this_capture_depth -= 1;
            self.write("; })(");
            self.write(parent_this_expr);
            self.write("))");
        }
    }

    pub(super) fn emit_function_expression_es5_params(&mut self, node: &Node) {
        let Some(func) = self.arena.get_function(node) else {
            return;
        };

        self.write("function");

        if func.asterisk_token {
            self.write("*");
        }

        // Name (if any) - add space before open paren whether or not there's a name
        if !func.name.is_none() {
            self.write_space();
            self.emit(func.name);
        }

        // Space before ( for TypeScript compatibility: function (x) vs function(x)
        self.write(" ");

        // Parameters (without types for JavaScript)
        self.write("(");
        let param_transforms = self.emit_function_parameters_es5(&func.parameters.nodes);
        self.write(") ");

        // Emit body - check if it's a simple single-statement body
        let body_node = self.arena.get(func.body);
        let is_simple_body = if let Some(body) = body_node {
            if let Some(block) = self.arena.get_block(body) {
                // Single return statement = simple body
                block.statements.nodes.len() == 1
                    && self.is_simple_return_statement(block.statements.nodes[0])
            } else {
                false
            }
        } else {
            false
        };

        if param_transforms.has_transforms() {
            self.emit_block_with_param_prologue(func.body, &param_transforms);
        } else if is_simple_body {
            self.emit_single_line_block(func.body);
        } else {
            self.emit(func.body);
        }
    }

    pub(super) fn emit_function_declaration_es5_params(&mut self, node: &Node) {
        let Some(func) = self.arena.get_function(node) else {
            return;
        };

        // Skip ambient declarations (declare function)
        if self.has_declare_modifier(&func.modifiers) {
            return;
        }

        // For JavaScript emit: skip declaration-only functions (no body)
        if func.body.is_none() {
            return;
        }

        self.write("function");

        if func.asterisk_token {
            self.write("*");
        }

        // Name
        if !func.name.is_none() {
            self.write_space();
            self.emit(func.name);
        }

        // Parameters - only emit names, not types for JavaScript
        self.write("(");
        let param_transforms = self.emit_function_parameters_es5(&func.parameters.nodes);
        self.write(")");

        // No return type for JavaScript

        self.write_space();
        if param_transforms.has_transforms() {
            self.emit_block_with_param_prologue(func.body, &param_transforms);
        } else {
            self.emit(func.body);
        }
    }

    /// Emit an async function transformed to ES5 __awaiter/__generator pattern
    pub(super) fn emit_async_function_es5(
        &mut self,
        func: &crate::parser::node::FunctionData,
        func_name: &str,
        this_expr: &str,
    ) {
        self.emit_async_function_es5_body(func_name, &func.parameters.nodes, func.body, this_expr);
    }

    pub(super) fn emit_async_function_es5_body(
        &mut self,
        func_name: &str,
        params: &[NodeIndex],
        body: NodeIndex,
        this_expr: &str,
    ) {
        // function name(params) {
        self.write("function");
        if !func_name.is_empty() {
            self.write_space();
            self.write(func_name);
        }
        self.write("(");
        let param_transforms = self.emit_function_parameters_es5(params);
        self.write(") {");
        self.write_line();
        self.increase_indent();

        self.emit_param_prologue(&param_transforms);

        // Emit indented __awaiter body
        //     return __awaiter(this, void 0, void 0, function () {
        //         return __generator(this, function (_a) { ... });
        //     });
        let mut async_emitter = crate::transforms::async_es5::AsyncES5Emitter::new(self.arena);
        // Transform emitter handles its own indentation inside __awaiter
        async_emitter.set_indent_level(self.writer.indent_level() + 1);
        if let Some(text) = self.source_text_for_map()
            && self.writer.has_source_map()
        {
            async_emitter.set_source_map_context(text, self.writer.current_source_index());
        }
        async_emitter.set_lexical_this(this_expr != "this");

        let generator_body = if async_emitter.body_contains_await(body) {
            async_emitter.emit_generator_body_with_await(body)
        } else {
            async_emitter.emit_simple_generator_body(body)
        };
        let generator_mappings = async_emitter.take_mappings();

        // Write with surrounding __awaiter wrapper
        self.write("return __awaiter(");
        self.write(this_expr);
        self.write(", void 0, void 0, function () {");
        self.write_line();
        self.increase_indent();
        if !generator_mappings.is_empty() && self.writer.has_source_map() {
            self.writer.write("");
            let base_line = self.writer.current_line();
            let base_column = self.writer.current_column();
            self.writer
                .add_offset_mappings(base_line, base_column, &generator_mappings);
            self.writer.write(&generator_body);
        } else {
            self.write(&generator_body);
        }
        self.decrease_indent();
        self.write_line();
        self.write("});");
        self.write_line();
        self.decrease_indent();
        self.write("}");
    }

    pub(super) fn function_parameters_need_es5_transform(&self, params: &[NodeIndex]) -> bool {
        params.iter().any(|&param_idx| {
            let Some(param_node) = self.arena.get(param_idx) else {
                return false;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                return false;
            };

            param.dot_dot_dot_token
                || !param.initializer.is_none()
                || self.is_binding_pattern(param.name)
        })
    }

    pub(super) fn emit_function_parameters_es5(
        &mut self,
        params: &[NodeIndex],
    ) -> ParamTransformPlan {
        let mut plan = ParamTransformPlan::default();
        let mut first = true;

        for (index, &param_idx) in params.iter().enumerate() {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };

            if param.dot_dot_dot_token {
                let rest_target = param.name;
                let rest_is_pattern = self.is_binding_pattern(rest_target);
                let rest_name = if rest_is_pattern {
                    self.get_temp_var_name()
                } else {
                    self.get_identifier_text(rest_target)
                };

                if !rest_name.is_empty() {
                    plan.rest = Some(RestParamTransform {
                        name: rest_name,
                        pattern: if rest_is_pattern {
                            Some(rest_target)
                        } else {
                            None
                        },
                        index,
                    });
                }
                break;
            }

            if !first {
                self.write(", ");
            }
            first = false;

            if self.is_binding_pattern(param.name) {
                let temp_name = self.get_temp_var_name();
                self.write(&temp_name);
                plan.params.push(ParamTransform {
                    name: temp_name,
                    pattern: Some(param.name),
                    initializer: if param.initializer.is_none() {
                        None
                    } else {
                        Some(param.initializer)
                    },
                });
            } else {
                self.emit(param.name);
                if !param.initializer.is_none() {
                    let name = self.get_identifier_text(param.name);
                    if !name.is_empty() {
                        plan.params.push(ParamTransform {
                            name,
                            pattern: None,
                            initializer: Some(param.initializer),
                        });
                    }
                }
            }
        }

        plan
    }

    /// Emit an ES5-compatible class expression by wrapping the class IIFE in an expression.
    pub(super) fn emit_class_expression_es5(&mut self, class_node: NodeIndex) {
        let Some(node) = self.arena.get(class_node) else {
            return;
        };
        let Some(class_data) = self.arena.get_class(node) else {
            return;
        };

        let mut es5_emitter = ClassES5Emitter::new(self.arena);
        es5_emitter.set_indent_level(0);
        if let Some(text) = self.source_text_for_map() {
            if self.writer.has_source_map() {
                es5_emitter.set_source_map_context(text, self.writer.current_source_index());
            } else {
                es5_emitter.set_source_text(text);
            }
        }

        let (class_name, es5_output) = if !class_data.name.is_none() {
            let candidate = self.get_identifier_text(class_data.name);
            if candidate.is_empty() || !is_valid_identifier_name(&candidate) {
                let temp_name = self.get_temp_var_name();
                let output = es5_emitter.emit_class_with_name(class_node, &temp_name);
                (temp_name, output)
            } else {
                let output = es5_emitter.emit_class(class_node);
                (candidate, output)
            }
        } else {
            let temp_name = self.get_temp_var_name();
            let output = es5_emitter.emit_class_with_name(class_node, &temp_name);
            (temp_name, output)
        };
        let es5_mappings = es5_emitter.take_mappings();

        self.write("(function () {");
        self.write_line();
        self.increase_indent();

        if !es5_mappings.is_empty() && self.writer.has_source_map() {
            let base_line = self.writer.current_line();
            let column_offset = self.writer.indent_width();
            self.writer.add_mappings_with_line_column_offset(
                base_line,
                column_offset,
                &es5_mappings,
            );
        }

        for line in es5_output.lines() {
            if !line.is_empty() {
                self.write(line);
            }
            self.write_line();
        }

        self.write("return ");
        self.write(&class_name);
        self.write(";");
        self.write_line();

        self.decrease_indent();
        self.write("})()");
    }

    /// Check if any class in the statements (recursively) extends another class
    pub(super) fn needs_extends_helper(&self, statements: &NodeList) -> bool {
        if let Some(needed) = self.needs_extends_helper_from_transforms() {
            return needed;
        }

        for &stmt_idx in &statements.nodes {
            if self.statement_needs_extends(stmt_idx) {
                return true;
            }
        }
        false
    }

    pub(super) fn needs_extends_helper_from_transforms(&self) -> Option<bool> {
        let mut saw_class = false;
        for (_, directive) in self.transforms.iter() {
            if let Some(needed) = self.directive_needs_extends_helper(directive) {
                saw_class = true;
                if needed {
                    return Some(true);
                }
            }
        }

        if saw_class { Some(false) } else { None }
    }

    pub(super) fn directive_needs_extends_helper(
        &self,
        directive: &TransformDirective,
    ) -> Option<bool> {
        match directive {
            TransformDirective::ES5Class {
                class_node,
                heritage,
                ..
            } => {
                if heritage.is_some() {
                    return Some(true);
                }
                Some(self.class_has_extends_node(*class_node))
            }
            TransformDirective::ES5ClassExpression { class_node } => {
                Some(self.class_has_extends_node(*class_node))
            }
            TransformDirective::CommonJSExportDefaultClassES5 { class_node } => {
                Some(self.class_has_extends_node(*class_node))
            }
            TransformDirective::CommonJSExport { inner, .. } => {
                self.directive_needs_extends_helper(inner)
            }
            TransformDirective::Chain(directives) => {
                let mut saw_class = false;
                for directive in directives {
                    if let Some(needed) = self.directive_needs_extends_helper(directive) {
                        saw_class = true;
                        if needed {
                            return Some(true);
                        }
                    }
                }

                if saw_class { Some(false) } else { None }
            }
            _ => None,
        }
    }

    pub(super) fn class_has_extends_node(&self, class_idx: NodeIndex) -> bool {
        let Some(class_node) = self.arena.get(class_idx) else {
            return false;
        };
        let Some(class_data) = self.arena.get_class(class_node) else {
            return false;
        };
        self.class_has_extends(&class_data.heritage_clauses)
    }

    pub(super) fn needs_class_private_field_helpers(&self) -> bool {
        for (_, directive) in self.transforms.iter() {
            if self.directive_has_private_members(directive) {
                return true;
            }
        }
        false
    }

    pub(super) fn directive_has_private_members(&self, directive: &TransformDirective) -> bool {
        match directive {
            TransformDirective::ES5Class { class_node, .. } => {
                self.class_has_private_members(*class_node)
            }
            TransformDirective::ES5ClassExpression { class_node } => {
                self.class_has_private_members(*class_node)
            }
            TransformDirective::CommonJSExportDefaultClassES5 { class_node } => {
                self.class_has_private_members(*class_node)
            }
            TransformDirective::CommonJSExport { inner, .. } => {
                self.directive_has_private_members(inner)
            }
            TransformDirective::Chain(directives) => directives
                .iter()
                .any(|directive| self.directive_has_private_members(directive)),
            _ => false,
        }
    }

    pub(super) fn class_has_private_members(&self, class_idx: NodeIndex) -> bool {
        let Some(class_node) = self.arena.get(class_idx) else {
            return false;
        };
        let Some(class_data) = self.arena.get_class(class_node) else {
            return false;
        };

        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    if let Some(prop) = self.arena.get_property_decl(member_node)
                        && crate::transforms::private_fields_es5::is_private_identifier(
                            self.arena, prop.name,
                        )
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = self.arena.get_method_decl(member_node)
                        && crate::transforms::private_fields_es5::is_private_identifier(
                            self.arena,
                            method.name,
                        )
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    if let Some(accessor) = self.arena.get_accessor(member_node)
                        && crate::transforms::private_fields_es5::is_private_identifier(
                            self.arena,
                            accessor.name,
                        )
                    {
                        return true;
                    }
                }
                _ => {}
            }
        }

        false
    }

    pub(super) fn has_es5_transforms(&self) -> bool {
        self.transforms
            .iter()
            .any(|(_, directive)| Self::directive_has_es5(directive))
    }

    pub(super) fn directive_has_es5(directive: &TransformDirective) -> bool {
        match directive {
            TransformDirective::ES5Class { .. }
            | TransformDirective::ES5ClassExpression { .. }
            | TransformDirective::ES5Namespace { .. }
            | TransformDirective::ES5Enum { .. }
            | TransformDirective::ES5ArrowFunction { .. }
            | TransformDirective::ES5AsyncFunction { .. }
            | TransformDirective::ES5ForOf { .. }
            | TransformDirective::ES5ObjectLiteral { .. }
            | TransformDirective::ES5VariableDeclarationList { .. }
            | TransformDirective::ES5FunctionParameters { .. }
            | TransformDirective::ES5TemplateLiteral { .. }
            | TransformDirective::CommonJSExportDefaultClassES5 { .. } => true,
            TransformDirective::CommonJSExport { inner, .. } => Self::directive_has_es5(inner),
            TransformDirective::Chain(directives) => directives.iter().any(Self::directive_has_es5),
            _ => false,
        }
    }

    pub(super) fn needs_values_helper(&self) -> bool {
        self.arena.nodes.iter().any(|node| {
            if node.kind != syntax_kind_ext::FOR_OF_STATEMENT {
                return false;
            }

            if let Some(for_in_of) = self.arena.get_for_in_of(node) {
                return !for_in_of.await_modifier;
            }

            false
        })
    }

    pub(super) fn needs_rest_helper(&self) -> bool {
        self.arena.nodes.iter().any(|node| {
            if node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN {
                return false;
            }

            let Some(pattern) = self.arena.get_binding_pattern(node) else {
                return false;
            };

            for &elem_idx in &pattern.elements.nodes {
                let Some(elem_node) = self.arena.get(elem_idx) else {
                    continue;
                };
                let Some(elem) = self.arena.get_binding_element(elem_node) else {
                    continue;
                };
                if elem.dot_dot_dot_token {
                    return true;
                }
            }

            false
        })
    }

    pub(super) fn needs_async_helpers(&self) -> bool {
        self.arena.nodes.iter().any(|node| {
            if let Some(func) = self.arena.get_function(node) {
                return func.is_async;
            }

            if node.kind == syntax_kind_ext::METHOD_DECLARATION
                && let Some(method) = self.arena.get_method_decl(node)
            {
                return self.has_modifier(&method.modifiers, SyntaxKind::AsyncKeyword as u16);
            }

            false
        })
    }

    pub(super) fn tagged_template_var_name(&self, idx: NodeIndex) -> String {
        format!("__templateObject_{}", idx.0)
    }

    pub(super) fn collect_tagged_template_vars(&self) -> Vec<String> {
        if self.transforms.helpers_populated() {
            return self.collect_tagged_template_vars_from_transforms();
        }

        let mut vars = Vec::new();
        for (idx, node) in self.arena.nodes.iter().enumerate() {
            if node.kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION {
                vars.push(self.tagged_template_var_name(NodeIndex(idx as u32)));
            }
        }
        vars
    }

    pub(super) fn collect_tagged_template_vars_from_transforms(&self) -> Vec<String> {
        let mut vars = Vec::new();
        for (&idx, directive) in self.transforms.iter() {
            if !matches!(directive, TransformDirective::ES5TemplateLiteral { .. }) {
                continue;
            }

            let Some(node) = self.arena.get(idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION {
                vars.push(self.tagged_template_var_name(idx));
            }
        }
        vars
    }

    /// Check if a statement contains a class that extends another (recursive)
    pub(super) fn statement_needs_extends(&self, stmt_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(stmt_idx) else {
            return false;
        };

        match node.kind {
            // Class declaration
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class_data) = self.arena.get_class(node)
                    && self.class_has_extends(&class_data.heritage_clauses)
                {
                    return true;
                }
                false
            }
            // Expression statement - might contain IIFE with class
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.arena.get_expression_statement(node) {
                    self.expression_needs_extends(expr_stmt.expression)
                } else {
                    false
                }
            }
            // Block - recurse into statements
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = self.arena.get_block(node) {
                    self.needs_extends_helper(&block.statements)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Check if an expression contains a class that extends another (recursive)
    pub(super) fn expression_needs_extends(&self, expr_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(expr_idx) else {
            return false;
        };

        match node.kind {
            // Call expression - check arguments and the called function
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(node) {
                    // Check the function being called
                    if self.expression_needs_extends(call.expression) {
                        return true;
                    }
                    // Check arguments
                    if let Some(ref args) = call.arguments {
                        for &arg_idx in &args.nodes {
                            if self.expression_needs_extends(arg_idx) {
                                return true;
                            }
                        }
                    }
                }
                false
            }
            // Parenthesized expression
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.arena.get_parenthesized(node) {
                    self.expression_needs_extends(paren.expression)
                } else {
                    false
                }
            }
            // Arrow function - check body
            k if k == syntax_kind_ext::ARROW_FUNCTION => {
                if let Some(func) = self.arena.get_function(node) {
                    self.statement_needs_extends(func.body)
                } else {
                    false
                }
            }
            // Function expression - check body
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => {
                if let Some(func) = self.arena.get_function(node) {
                    self.statement_needs_extends(func.body)
                } else {
                    false
                }
            }
            // Class expression
            k if k == syntax_kind_ext::CLASS_EXPRESSION => {
                if let Some(class_data) = self.arena.get_class(node) {
                    self.class_has_extends(&class_data.heritage_clauses)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Check if a class has an extends clause
    pub(super) fn class_has_extends(&self, heritage_clauses: &Option<NodeList>) -> bool {
        let Some(clauses) = heritage_clauses else {
            return false;
        };
        for &clause_idx in &clauses.nodes {
            let Some(clause_node) = self.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage_data) = self.arena.get_heritage(clause_node) else {
                continue;
            };
            if heritage_data.token == SyntaxKind::ExtendsKeyword as u16 {
                return true;
            }
        }
        false
    }

    /// Emit the __extends helper function
    pub(super) fn emit_extends_helper(&mut self) {
        // TypeScript's ES5 __extends helper
        self.write("var __extends = (this && this.__extends) || (function () {");
        self.write_line();
        self.increase_indent();

        self.write("var extendStatics = function (d, b) {");
        self.write_line();
        self.increase_indent();

        self.write("extendStatics = Object.setPrototypeOf ||");
        self.write_line();
        self.write(
            "    ({ __proto__: [] } instanceof Array && function (d, b) { d.__proto__ = b; }) ||",
        );
        self.write_line();
        self.write("    function (d, b) { for (var p in b) if (Object.prototype.hasOwnProperty.call(b, p)) d[p] = b[p]; };");
        self.write_line();
        self.write("return extendStatics(d, b);");
        self.write_line();

        self.decrease_indent();
        self.write("};");
        self.write_line();

        self.write("return function (d, b) {");
        self.write_line();
        self.increase_indent();

        self.write("if (typeof b !== \"function\" && b !== null)");
        self.write_line();
        self.write("    throw new TypeError(\"Class extends value \" + String(b) + \" is not a constructor or null\");");
        self.write_line();
        self.write("extendStatics(d, b);");
        self.write_line();
        self.write("function __() { this.constructor = d; }");
        self.write_line();
        self.write(
            "d.prototype = b === null ? Object.create(b) : (__.prototype = b.prototype, new __());",
        );
        self.write_line();

        self.decrease_indent();
        self.write("};");
        self.write_line();

        self.decrease_indent();
        self.write("})();");
        self.write_line();
    }
}
