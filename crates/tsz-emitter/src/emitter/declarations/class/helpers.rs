use super::super::super::Printer;
use super::AutoAccessorEmitOptions;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    /// Resolve the named-evaluation target for an anonymous class expression from
    /// its parent chain. For `const C = class { ... }` and
    /// `(C = class { ... })`, this returns `Some("C")`.
    pub(in crate::emitter) fn resolve_class_expr_binding_name(
        &self,
        class_idx: NodeIndex,
    ) -> Option<String> {
        let mut current = class_idx;
        let mut hops = 0;

        while hops < 8 {
            let parent_idx = self.arena.get_extended(current)?.parent;
            if parent_idx.is_none() {
                return None;
            }
            let parent_node = self.arena.get(parent_idx)?;

            match parent_node.kind {
                syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    let paren = self.arena.get_parenthesized(parent_node)?;
                    if paren.expression != current {
                        return None;
                    }
                    current = parent_idx;
                    hops += 1;
                }
                syntax_kind_ext::TYPE_ASSERTION
                | syntax_kind_ext::AS_EXPRESSION
                | syntax_kind_ext::SATISFIES_EXPRESSION => {
                    let assertion = self.arena.get_type_assertion(parent_node)?;
                    if assertion.expression != current {
                        return None;
                    }
                    current = parent_idx;
                    hops += 1;
                }
                syntax_kind_ext::NON_NULL_EXPRESSION => {
                    let non_null = self.arena.get_unary_expr_ex(parent_node)?;
                    if non_null.expression != current {
                        return None;
                    }
                    current = parent_idx;
                    hops += 1;
                }
                syntax_kind_ext::VARIABLE_DECLARATION => {
                    let decl = self.arena.get_variable_declaration(parent_node)?;
                    if decl.initializer != current {
                        return None;
                    }
                    return self.identifier_binding_name(decl.name);
                }
                syntax_kind_ext::PARAMETER => {
                    let param = self.arena.get_parameter(parent_node)?;
                    if param.initializer != current {
                        return None;
                    }
                    return self.identifier_binding_name(param.name);
                }
                syntax_kind_ext::BINARY_EXPRESSION => {
                    let binary = self.arena.get_binary_expr(parent_node)?;
                    if binary.right != current
                        || binary.operator_token != SyntaxKind::EqualsToken as u16
                    {
                        return None;
                    }
                    return self.identifier_binding_name(binary.left);
                }
                _ => return None,
            }
        }

        None
    }

    fn identifier_binding_name(&self, name_idx: NodeIndex) -> Option<String> {
        let name_node = self.arena.get(name_idx)?;
        if name_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let name = self.get_identifier_text_idx(name_idx);
        (!name.is_empty()).then_some(name)
    }

    pub(in crate::emitter) fn emit_class_expr_set_function_name_comma_item(
        &mut self,
        temp: &str,
        name: &str,
    ) {
        self.write(",");
        self.write_line();
        self.increase_indent();
        self.write_helper("__setFunctionName");
        self.write("(");
        self.write(temp);
        self.write(", ");
        self.emit_string_literal_text(name);
        self.write(")");
        self.decrease_indent();
    }

    /// Emit deferred static block IIFEs as `(() => { ... })();`.
    /// Check if a computed property name expression is side-effect-free.
    /// Looks through type assertions and parenthesized expressions to find
    /// the core expression, then checks if it's a literal/identifier/keyword.
    pub(in crate::emitter) fn is_computed_name_expr_side_effect_free(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return true;
        };
        let k = expr_node.kind;
        // Simple side-effect-free expressions
        if k == SyntaxKind::Identifier as u16
            || k == SyntaxKind::PrivateIdentifier as u16
            || k == SyntaxKind::StringLiteral as u16
            || k == SyntaxKind::NumericLiteral as u16
            || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            || k == SyntaxKind::TrueKeyword as u16
            || k == SyntaxKind::FalseKeyword as u16
            || k == SyntaxKind::NullKeyword as u16
            || k == SyntaxKind::UndefinedKeyword as u16
        {
            return true;
        }
        // Type assertions: `<T>expr`, `expr as T`, `expr satisfies T` — look through
        if (k == syntax_kind_ext::TYPE_ASSERTION
            || k == syntax_kind_ext::AS_EXPRESSION
            || k == syntax_kind_ext::SATISFIES_EXPRESSION)
            && let Some(assertion) = self.arena.get_type_assertion(expr_node)
        {
            return self.is_computed_name_expr_side_effect_free(assertion.expression);
        }
        // Parenthesized expression: `(expr)` — look through
        if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.arena.get_parenthesized(expr_node)
        {
            return self.is_computed_name_expr_side_effect_free(paren.expression);
        }
        false
    }

    pub(in crate::emitter) fn emit_static_block_iife_expression(
        &mut self,
        static_block_idx: NodeIndex,
        saved_comment_idx: usize,
    ) {
        if self.ctx.target_es5 {
            self.write("(function () ");
        } else {
            self.write("(() => ");
        }
        self.comment_emit_idx = saved_comment_idx;
        if let Some(static_node) = self.arena.get(static_block_idx) {
            let prev = self.emitting_function_body_block;
            let saved_await_as_yield = self.ctx.emit_await_as_yield;
            self.emitting_function_body_block = true;
            self.ctx.emit_await_as_yield = true;
            // Save and restore hoisted temps so outer-scope vars (e.g. private
            // field WeakMap names) don't get re-declared inside the IIFE body.
            let saved_temps = std::mem::take(&mut self.hoisted_assignment_temps);
            self.emit_block(static_node, static_block_idx);
            // Any temps generated inside the IIFE block have already been
            // emitted by emit_block; restore the outer-scope temps.
            self.hoisted_assignment_temps = saved_temps;
            self.ctx.emit_await_as_yield = saved_await_as_yield;
            self.emitting_function_body_block = prev;
        } else {
            self.write("{ }");
        }
        self.write(")()");
    }

    pub(in crate::emitter) fn emit_static_block_iifes(&mut self, blocks: Vec<(NodeIndex, usize)>) {
        for (static_block_idx, saved_comment_idx) in blocks {
            self.write_line();
            self.emit_static_block_iife_expression(static_block_idx, saved_comment_idx);
            self.write(";");
        }
    }

    pub(in crate::emitter) fn emit_static_block_iifes_with_context(
        &mut self,
        blocks: Vec<(NodeIndex, usize)>,
        this_alias: Option<&str>,
        super_base_alias: Option<&str>,
    ) {
        let prev_this_alias = self.scoped_static_this_alias.clone();
        let prev_super_alias = self.scoped_static_super_base_alias.clone();
        self.scoped_static_this_alias = this_alias.map(std::sync::Arc::from);
        self.scoped_static_super_base_alias = super_base_alias.map(std::sync::Arc::from);
        self.emit_static_block_iifes(blocks);
        self.scoped_static_this_alias = prev_this_alias;
        self.scoped_static_super_base_alias = prev_super_alias;
    }

    pub(in crate::emitter) fn emit_static_block_iife_comma_items(
        &mut self,
        blocks: Vec<(NodeIndex, usize)>,
    ) {
        for (static_block_idx, saved_comment_idx) in blocks {
            self.write(",");
            self.write_line();
            self.increase_indent();
            self.emit_static_block_iife_expression(static_block_idx, saved_comment_idx);
            self.decrease_indent();
        }
    }

    pub(in crate::emitter) fn emit_static_block_iife_comma_items_with_context(
        &mut self,
        blocks: Vec<(NodeIndex, usize)>,
        this_alias: Option<&str>,
        super_base_alias: Option<&str>,
    ) {
        let prev_this_alias = self.scoped_static_this_alias.clone();
        let prev_super_alias = self.scoped_static_super_base_alias.clone();
        self.scoped_static_this_alias = this_alias.map(std::sync::Arc::from);
        self.scoped_static_super_base_alias = super_base_alias.map(std::sync::Arc::from);
        self.emit_static_block_iife_comma_items(blocks);
        self.scoped_static_this_alias = prev_this_alias;
        self.scoped_static_super_base_alias = prev_super_alias;
    }

    pub(in crate::emitter) fn class_has_auto_accessor_members(
        &self,
        class: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let Some(prop_data) = self.arena.get_property_decl(member_node) else {
                continue;
            };
            if self
                .arena
                .has_modifier(&prop_data.modifiers, SyntaxKind::AccessorKeyword)
                && !self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::AbstractKeyword)
                && self
                    .arena
                    .get(prop_data.name)
                    .is_none_or(|n| n.kind != SyntaxKind::PrivateIdentifier as u16)
                && !self.arena.is_declare(&prop_data.modifiers)
            {
                let Some(name_node) = self.arena.get(prop_data.name) else {
                    continue;
                };
                if name_node.kind == SyntaxKind::Identifier as u16 {
                    return true;
                }
            }
        }
        false
    }

    pub(in crate::emitter) fn class_has_decorators(
        &self,
        class: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        if class.modifiers.as_ref().is_some_and(|mods| {
            mods.nodes.iter().any(|&mod_idx| {
                self.arena
                    .get(mod_idx)
                    .is_some_and(|node| node.kind == syntax_kind_ext::DECORATOR)
            })
        }) {
            return true;
        }

        class.members.nodes.iter().any(|&member_idx| {
            let Some(member_node) = self.arena.get(member_idx) else {
                return false;
            };
            let modifiers = match member_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .arena
                    .get_method_decl(member_node)
                    .and_then(|member| member.modifiers.as_ref()),
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                    .arena
                    .get_property_decl(member_node)
                    .and_then(|member| member.modifiers.as_ref()),
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    self.arena
                        .get_accessor(member_node)
                        .and_then(|member| member.modifiers.as_ref())
                }
                _ => None,
            };
            modifiers.is_some_and(|mods| {
                mods.nodes.iter().any(|&mod_idx| {
                    self.arena
                        .get(mod_idx)
                        .is_some_and(|node| node.kind == syntax_kind_ext::DECORATOR)
                })
            })
        })
    }

    pub(in crate::emitter) fn emit_auto_accessor_methods(
        &mut self,
        node: &Node,
        storage_name: &str,
        is_static: bool,
        options: AutoAccessorEmitOptions<'_>,
    ) {
        let Some(prop) = self.arena.get_property_decl(node) else {
            return;
        };
        let computed_name_temp = if options.lower_to_private_fields {
            self.auto_accessor_computed_name_temp(prop.name)
        } else {
            None
        };

        if options.lower_to_private_fields {
            if is_static {
                self.write("static ");
                self.write("#");
                self.write(storage_name);
                if prop.initializer.is_some() && !options.omit_storage_initializer {
                    self.write(" = ");
                    self.emit_expression(prop.initializer);
                }
                self.write(";");
            } else {
                self.write("#");
                self.write(storage_name);
                if prop.initializer.is_some() && !options.omit_storage_initializer {
                    self.write(" = ");
                    self.emit_expression(prop.initializer);
                }
                self.write(";");
            }
            self.write_line();

            if is_static {
                self.write("static ");
            }
            self.write("get ");
            self.emit_auto_accessor_name(prop.name, computed_name_temp.as_deref(), true);
            self.write("() { return ");
            if is_static {
                self.write(if options.class_name.is_empty() {
                    "this"
                } else {
                    options.class_name
                });
                self.write(".#");
                self.write(storage_name.trim_start_matches('#'));
                self.write("; }");
                self.write_line();
                self.write("static ");
                self.write("set ");
                self.emit_auto_accessor_name(prop.name, computed_name_temp.as_deref(), false);
                self.write("(value) { ");
                self.write(if options.class_name.is_empty() {
                    "this"
                } else {
                    options.class_name
                });
                self.write(".#");
                self.write(storage_name.trim_start_matches('#'));
                self.write(" = value; }");
            } else {
                self.write("this.");
                self.write("#");
                self.write(storage_name.trim_start_matches('#'));
                self.write("; }");
                self.write_line();
                self.write("set ");
                self.emit_auto_accessor_name(prop.name, computed_name_temp.as_deref(), false);
                self.write("(value) { this.");
                self.write("#");
                self.write(storage_name.trim_start_matches('#'));
                self.write(" = value; }");
            }
            self.emit_trailing_comments(options.property_end);
            self.write_line();
        } else if is_static {
            let Some(alias) = options.static_accessor_alias else {
                return;
            };
            self.write("static ");
            self.write("get ");
            self.emit(prop.name);
            self.write("() { return ");
            self.write_helper("__classPrivateFieldGet");
            self.write("(");
            self.write(alias);
            self.write(", ");
            self.write(alias);
            self.write(", \"f\", ");
            self.write(storage_name);
            self.write("); }");
            self.emit_trailing_comments(options.property_end);
            self.write_line();
            self.write("static ");
            self.write("set ");
            self.emit(prop.name);
            self.write("(value) { ");
            self.write_helper("__classPrivateFieldSet");
            self.write("(");
            self.write(alias);
            self.write(", ");
            self.write(alias);
            self.write(", value, \"f\", ");
            self.write(storage_name);
            self.write("); }");
        } else {
            self.write("get ");
            self.emit_auto_accessor_weakmap_name(prop.name, options.computed_storage_inits);
            self.write("() { return ");
            self.write_helper("__classPrivateFieldGet");
            self.write("(this, ");
            self.write(storage_name);
            self.write(", \"f\"); }");
            self.emit_trailing_comments(options.property_end);
            self.write_line();
            self.write("set ");
            self.emit(prop.name);
            self.write("(value) { ");
            self.write_helper("__classPrivateFieldSet");
            self.write("(this, ");
            self.write(storage_name);
            self.write(", value, \"f\"); }");
        }
    }

    fn emit_auto_accessor_weakmap_name(&mut self, name_idx: NodeIndex, storage_inits: &[String]) {
        if storage_inits.is_empty() {
            self.emit(name_idx);
            return;
        }
        let Some(name_node) = self.arena.get(name_idx) else {
            self.emit(name_idx);
            return;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            self.emit(name_idx);
            return;
        }
        let Some(computed) = self.arena.get_computed_property(name_node) else {
            self.emit(name_idx);
            return;
        };

        self.write("[(");
        for (i, init) in storage_inits.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.write(init);
        }
        self.write(", ");
        if let Some(temp) = self
            .computed_prop_temp_map
            .get(&computed.expression)
            .cloned()
        {
            self.write(&temp);
            self.write(" = ");
        }
        self.emit_expression(computed.expression);
        self.write(")]");
    }

    fn auto_accessor_computed_name_temp(&mut self, name_idx: NodeIndex) -> Option<String> {
        let name_node = self.arena.get(name_idx)?;
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }
        let computed = self.arena.get_computed_property(name_node)?;
        if self.is_computed_name_expr_side_effect_free(computed.expression) {
            return None;
        }
        Some(self.make_unique_name_hoisted())
    }

    fn emit_auto_accessor_name(
        &mut self,
        name_idx: NodeIndex,
        computed_temp: Option<&str>,
        initialize_temp: bool,
    ) {
        let Some(temp) = computed_temp else {
            self.emit(name_idx);
            return;
        };
        let Some(name_node) = self.arena.get(name_idx) else {
            self.emit(name_idx);
            return;
        };
        let Some(computed) = self.arena.get_computed_property(name_node) else {
            self.emit(name_idx);
            return;
        };

        self.write("[");
        self.write(temp);
        if initialize_temp {
            self.write(" = ");
            self.emit_expression(computed.expression);
        }
        self.write("]");
    }

    /// Parser recovery parity for malformed class members like:
    /// `var constructor() { }`
    /// which TypeScript preserves as:
    /// `var constructor;`
    /// `() => { };`
    pub(in crate::emitter) fn class_var_function_recovery_name(
        &self,
        class_node: &Node,
    ) -> Option<String> {
        let text = self.source_text?;
        let start = std::cmp::min(class_node.pos as usize, text.len());
        let end = std::cmp::min(class_node.end as usize, text.len());
        if start >= end {
            return None;
        }

        let slice = &text[start..end];
        let mut i = 0usize;
        let bytes = slice.as_bytes();

        while i < bytes.len() {
            if bytes[i].is_ascii_whitespace() {
                i += 1;
                continue;
            }
            if i + 3 > bytes.len() || &bytes[i..i + 3] != b"var" {
                i += 1;
                continue;
            }
            i += 3;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let ident_start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$')
            {
                i += 1;
            }
            if ident_start == i {
                continue;
            }
            let ident = String::from_utf8_lossy(&bytes[ident_start..i]).to_string();
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] != b'(' {
                continue;
            }
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] != b')' {
                continue;
            }
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] != b'{' {
                continue;
            }
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] != b'}' {
                continue;
            }

            return Some(ident);
        }

        None
    }
}
