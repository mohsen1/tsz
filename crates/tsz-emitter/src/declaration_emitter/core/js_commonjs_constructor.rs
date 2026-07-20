use rustc_hash::FxHashMap;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::{DeclarationEmitter, JsPrototypeAssignment};

impl<'a> DeclarationEmitter<'a> {
    /// Emit the declaration surface TypeScript uses for an actual JavaScript
    /// function whose prototype is replaced by one object literal. The
    /// function remains a function; the replacement is a merged namespace
    /// value named `prototype`.
    pub(in crate::declaration_emitter) fn emit_js_function_prototype_namespace(
        &mut self,
        name_idx: NodeIndex,
        body_idx: NodeIndex,
        is_exported: bool,
        has_late_bound_members: bool,
    ) -> bool {
        let Some(initializer) = self.js_function_prototype_namespace_initializer(
            name_idx,
            body_idx,
            has_late_bound_members,
        ) else {
            return false;
        };
        let Some(initializer_node) = self.arena.get(initializer) else {
            return false;
        };
        let Some(object) = self.arena.get_literal_expr(initializer_node) else {
            return false;
        };
        let members = object.elements.nodes.clone();

        // The namespace is emitted next to the function, before the source
        // traversal reaches the prototype assignment. Body-local comments
        // belong to the already-emitted function and must not become leading
        // comments on an out-of-order prototype member.
        if let Some(body_node) = self.arena.get(body_idx) {
            self.skip_comments_in_node(body_node.pos, body_node.end);
        }
        let resume_comment_idx = self.comment_emit_idx;
        self.comment_emit_idx = self
            .all_comments
            .partition_point(|comment| comment.end <= initializer_node.pos);

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) || (self.source_is_js_file && is_exported)
        {
            self.write("declare ");
        }
        self.write("namespace ");
        self.emit_node(name_idx);
        self.write(" {");
        self.write_line();
        self.increase_indent();

        self.write_indent();
        self.write("var prototype: ");
        if members.is_empty() {
            self.write("{};");
            self.write_line();
        } else {
            self.write("{");
            self.write_line();
            self.increase_indent();
            for member_idx in members {
                self.emit_js_commonjs_constructor_prototype_member(member_idx);
            }
            self.decrease_indent();
            self.write_indent();
            self.write("};");
            self.write_line();
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
        self.comment_emit_idx = resume_comment_idx;
        self.emitted_module_indicator |= is_exported;
        true
    }

    pub(in crate::declaration_emitter) fn has_direct_js_prototype_object_initializer(
        &self,
        name_idx: NodeIndex,
    ) -> bool {
        self.source_is_js_file
            && self
                .arena
                .get_identifier_text(name_idx)
                .and_then(|name| self.js_prototype_assignments.get(name))
                .is_some_and(|assignments| {
                    assignments.iter().any(|assignment| {
                        !assignment.receiver_is_commonjs
                            && assignment.whole_prototype
                            && assignment.initializer_is_object_literal
                    })
                })
    }

    fn js_function_prototype_namespace_initializer(
        &self,
        name_idx: NodeIndex,
        body_idx: NodeIndex,
        has_late_bound_members: bool,
    ) -> Option<NodeIndex> {
        if !self.source_is_js_file || has_late_bound_members {
            return None;
        }
        let name = self.arena.get_identifier_text(name_idx)?;
        let assignments = self.js_prototype_assignments.get(name)?;
        let [assignment] = assignments.as_slice() else {
            return None;
        };
        // Repeated or mixed-receiver prototype assignments require a
        // solver-owned union projection. Leave that family to its dedicated
        // query rather than normalizing rendered type text in the emitter.
        if assignment.receiver_is_commonjs
            || !assignment.whole_prototype
            || !assignment.initializer_is_object_literal
        {
            return None;
        }
        if self
            .js_class_static_members
            .get(name)
            .is_some_and(|members| !members.is_empty())
        {
            return None;
        }
        if self
            .js_namespace_export_aliases
            .get(name)
            .is_some_and(|aliases| {
                aliases.iter().any(|alias| {
                    alias.has_non_statement_origin
                        || alias.source_statements.is_empty()
                        || alias
                            .source_statements
                            .iter()
                            .any(|source_statement| *source_statement != assignment.statement)
                })
            })
        {
            return None;
        }
        if self
            .js_class_like_prototype_members
            .get(name)
            .is_some_and(|members| !members.is_empty())
            || self
                .js_deferred_prototype_method_statements
                .get(name)
                .is_some_and(|members| !members.is_empty())
        {
            return None;
        }
        let initializer_node = self.arena.get(assignment.expression)?;
        let object = self.arena.get_literal_expr(initializer_node)?;
        // Keep this first parity slice deliberately structural and complete:
        // richer object members need their own declaration projection. Until
        // that owner is available, retain the existing class fallback instead
        // of emitting a partial declaration.
        if object.elements.nodes.iter().any(|member_idx| {
            self.arena.get(*member_idx).is_none_or(|member| {
                member.kind != syntax_kind_ext::METHOD_DECLARATION
                    || self
                        .get_member_name_idx(*member_idx)
                        .and_then(|name_idx| self.arena.get_identifier_text(name_idx))
                        .is_none()
            })
        }) {
            return None;
        }
        if !self.body_returns_void(body_idx) {
            return None;
        }
        Some(assignment.expression)
    }

    pub(in crate::declaration_emitter) fn emit_js_commonjs_constructor_prototype_class(
        &mut self,
        name_idx: NodeIndex,
    ) -> bool {
        let Some(export_name) = self.js_commonjs_export_name_text(name_idx) else {
            return false;
        };
        let prototype_members = self.js_prototype_object_members_for_export_name(&export_name);
        if prototype_members.is_empty() {
            return false;
        }

        self.write_indent();
        self.write("export class ");
        self.write(&export_name);
        self.write(" {");
        self.write_line();
        self.increase_indent();

        for member_idx in prototype_members {
            self.emit_js_commonjs_constructor_prototype_member(member_idx);
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
        self.emitted_module_indicator = true;
        true
    }

    fn emit_js_commonjs_constructor_prototype_member(&mut self, member_idx: NodeIndex) {
        let Some(member_node) = self.arena.get(member_idx) else {
            return;
        };
        let before_jsdoc_len = self.writer.len();
        let saved_comment_idx = self.comment_emit_idx;
        self.emit_leading_jsdoc_comments(member_node.pos);
        let before_member_len = self.writer.len();

        if let Some(prop) = self.arena.get_property_assignment(member_node) {
            if let Some(type_text) = self
                .resolve_declaration_type_text(&[prop.initializer], Some(prop.initializer))
                .map(|resolved| resolved.emitted_type_text)
                .or_else(|| self.allowlisted_initializer_type_text(prop.initializer))
            {
                self.write_indent();
                self.emit_node(prop.name);
                self.write(": ");
                self.write(&type_text);
                self.write(";");
                self.write_line();
            }
        } else {
            self.emit_class_member(member_idx);
        }

        if self.writer.len() == before_member_len {
            self.writer.truncate(before_jsdoc_len);
            self.comment_emit_idx = saved_comment_idx;
            self.skip_comments_in_node(member_node.pos, member_node.end);
        }
    }

    pub(super) fn js_prototype_object_members_for_export_name(&self, name: &str) -> Vec<NodeIndex> {
        let Some(assignment) = self
            .js_prototype_assignments
            .get(name)
            .and_then(|assignments| {
                assignments.iter().find(|assignment| {
                    assignment.whole_prototype && assignment.initializer_is_object_literal
                })
            })
        else {
            return Vec::new();
        };
        self.arena
            .get(assignment.expression)
            .and_then(|node| self.arena.get_literal_expr(node))
            .map_or_else(Vec::new, |object| object.elements.nodes.clone())
    }

    pub(in crate::declaration_emitter) fn collect_js_prototype_assignments(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> FxHashMap<String, Vec<JsPrototypeAssignment>> {
        let mut assignments = FxHashMap::<String, Vec<JsPrototypeAssignment>>::default();
        let mut active_commonjs_exports = FxHashMap::<String, String>::default();
        if !self.source_is_js_file {
            return assignments;
        }
        for &stmt_idx in &source_file.statements.nodes {
            if self
                .js_module_exports_assignment_initializer(stmt_idx)
                .is_some()
            {
                active_commonjs_exports.clear();
            }
            if let Some((export_name_idx, initializer)) =
                self.js_commonjs_named_export_for_statement(stmt_idx)
            {
                let export_name = self.get_identifier_text(export_name_idx).or_else(|| {
                    self.arena
                        .get_literal_text(export_name_idx)
                        .map(str::to_owned)
                });
                let initializer = self
                    .arena
                    .skip_parenthesized_and_assertions_and_comma(initializer);
                if let Some(export_name) = export_name {
                    if let Some(local_name) = self.get_identifier_text(initializer) {
                        active_commonjs_exports.insert(export_name, local_name);
                    } else {
                        active_commonjs_exports.remove(&export_name);
                    }
                }
            }
            if let Some((mut name, mut assignment)) =
                self.js_prototype_assignment_for_statement(stmt_idx)
            {
                if assignment.receiver_is_commonjs
                    && let Some(local_name) = active_commonjs_exports.get(&name)
                {
                    name.clone_from(local_name);
                    assignment.receiver_aliases_local = true;
                }
                assignments.entry(name).or_default().push(assignment);
            }
        }
        assignments
    }

    fn js_prototype_assignment_for_statement(
        &self,
        stmt_idx: NodeIndex,
    ) -> Option<(String, JsPrototypeAssignment)> {
        let stmt_node = self.arena.get(stmt_idx)?;
        if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
            return None;
        }
        let expr_stmt = self.arena.get_expression_statement(stmt_node)?;
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_stmt.expression);
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let binary = self.arena.get_binary_expr(expr_node)?;
        if binary.operator_token != SyntaxKind::EqualsToken as u16 {
            return None;
        }
        let lhs = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(binary.left);
        let lhs_node = self.arena.get(lhs)?;
        if lhs_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && lhs_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return None;
        }
        let access = self.arena.get_access_expr(lhs_node)?;
        let property_is_prototype = self
            .arena
            .get_identifier_text(access.name_or_argument)
            .or_else(|| self.arena.get_literal_text(access.name_or_argument))
            == Some("prototype");
        let (receiver, whole_prototype, member_name) = if property_is_prototype {
            (access.expression, true, None)
        } else {
            let prototype_receiver = self
                .arena
                .skip_parenthesized_and_assertions_and_comma(access.expression);
            let prototype_receiver_node = self.arena.get(prototype_receiver)?;
            if prototype_receiver_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && prototype_receiver_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            {
                return None;
            }
            let prototype_access = self.arena.get_access_expr(prototype_receiver_node)?;
            if self
                .arena
                .get_identifier_text(prototype_access.name_or_argument)
                .or_else(|| {
                    self.arena
                        .get_literal_text(prototype_access.name_or_argument)
                })
                != Some("prototype")
            {
                return None;
            }
            (
                prototype_access.expression,
                false,
                Some(access.name_or_argument),
            )
        };
        let (name, receiver_is_commonjs) = if let Some(name) = self.get_identifier_text(receiver) {
            (name, false)
        } else {
            (self.module_exports_property_reference_name(receiver)?, true)
        };
        let initializer = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(binary.right);
        let initializer_node = self.arena.get(initializer)?;
        Some((
            name,
            JsPrototypeAssignment {
                statement: stmt_idx,
                expression: initializer,
                receiver_is_commonjs,
                receiver_aliases_local: false,
                whole_prototype,
                member_name,
                initializer_is_object_literal: initializer_node.kind
                    == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION,
            },
        ))
    }
}
