use super::super::super::Printer;
use super::AutoAccessorInfo;
use crate::transforms::private_fields_es5::get_private_field_name;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    /// Resolve the binding name for an anonymous class expression from its parent chain.
    /// For `const C = class { ... }`, this returns `Some("C")`.
    /// Walks up: `ClassExpression` -> `VariableDeclaration` -> name identifier.
    pub(in crate::emitter) fn resolve_class_expr_binding_name(
        &self,
        class_idx: NodeIndex,
    ) -> Option<String> {
        let ext = self.arena.get_extended(class_idx)?;
        let parent_idx = ext.parent;
        if parent_idx.is_none() {
            return None;
        }
        let parent_node = self.arena.get(parent_idx)?;
        if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let decl = self.arena.get_variable_declaration(parent_node)?;
            let name_node = self.arena.get(decl.name)?;
            if name_node.kind == SyntaxKind::Identifier as u16 {
                let name = self.get_identifier_text_idx(decl.name);
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }
        None
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

    pub(in crate::emitter) fn emit_static_block_iifes(&mut self, blocks: Vec<(NodeIndex, usize)>) {
        for (static_block_idx, saved_comment_idx) in blocks {
            self.write_line();
            self.write("(() => ");
            self.comment_emit_idx = saved_comment_idx;
            if let Some(static_node) = self.arena.get(static_block_idx) {
                let prev = self.emitting_function_body_block;
                self.emitting_function_body_block = true;
                // Save and restore hoisted temps so outer-scope vars (e.g. private
                // field WeakMap names) don't get re-declared inside the IIFE body.
                let saved_temps = std::mem::take(&mut self.hoisted_assignment_temps);
                self.emit_block(static_node, static_block_idx);
                // Any temps generated inside the IIFE block have already been
                // emitted by emit_block; restore the outer-scope temps.
                self.hoisted_assignment_temps = saved_temps;
                self.emitting_function_body_block = prev;
            } else {
                self.write("{ }");
            }
            self.write(")();");
        }
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
                && !self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::DeclareKeyword)
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

    pub(in crate::emitter) fn emit_auto_accessor_methods(
        &mut self,
        node: &Node,
        storage_name: &str,
        is_static: bool,
        static_accessor_alias: Option<&str>,
        lower_auto_accessor_to_private_fields: bool,
        class_name: &str,
        property_end: u32,
    ) {
        let Some(prop) = self.arena.get_property_decl(node) else {
            return;
        };

        if lower_auto_accessor_to_private_fields {
            if is_static {
                self.write("static ");
                self.write("#");
                self.write(storage_name);
                if prop.initializer.is_some() {
                    self.write(" = ");
                    self.emit_expression(prop.initializer);
                }
                self.write(";");
            } else {
                self.write("#");
                self.write(storage_name);
                if prop.initializer.is_some() {
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
            self.emit(prop.name);
            self.write("() { return ");
            if is_static {
                self.write(if class_name.is_empty() {
                    "this"
                } else {
                    class_name
                });
                self.write(".#");
                self.write(storage_name.trim_start_matches('#'));
                self.write("; }");
                self.write_line();
                self.write("static ");
                self.write("set ");
                self.emit(prop.name);
                self.write("(value) { ");
                self.write(if class_name.is_empty() {
                    "this"
                } else {
                    class_name
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
                self.emit(prop.name);
                self.write("(value) { this.");
                self.write("#");
                self.write(storage_name.trim_start_matches('#'));
                self.write(" = value; }");
            }
            self.emit_trailing_comments(property_end);
            self.write_line();
        } else if is_static {
            let Some(alias) = static_accessor_alias else {
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
            self.emit_trailing_comments(property_end);
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
            self.emit(prop.name);
            self.write("() { return ");
            self.write_helper("__classPrivateFieldGet");
            self.write("(this, ");
            self.write(storage_name);
            self.write(", \"f\"); }");
            self.emit_trailing_comments(property_end);
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
