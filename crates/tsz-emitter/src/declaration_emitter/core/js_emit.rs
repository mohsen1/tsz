use crate::enums::evaluator::EnumEvaluator;
use crate::output::source_writer::{SourcePosition, SourceWriter};
use crate::type_cache_view::TypeCacheView;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tracing::debug;
use tsz_binder::{BinderState, SymbolId};
use tsz_common::comments::CommentRange;
use tsz_common::diagnostics::Diagnostic;
use tsz_parser::parser::node::{MethodDeclData, Node, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeInterner;
use tsz_solver::type_queries;

use super::DeclarationEmitter;

impl<'a> DeclarationEmitter<'a> {
    /// Emit a `declare class X { private constructor(); ... }` for a variable
    /// that has `.prototype` member assignments (the JS class-like heuristic).
    pub(in crate::declaration_emitter) fn emit_js_class_like_heuristic_if_needed(
        &mut self,
        name_idx: NodeIndex,
        is_exported: bool,
    ) -> bool {
        let Some(name) = self.get_identifier_text(name_idx) else {
            return false;
        };
        let Some(members) = self.js_class_like_prototype_members.get(&name).cloned() else {
            return false;
        };
        if members.is_empty() {
            return false;
        }

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write("class ");
        self.emit_node(name_idx);
        self.write(" {");
        self.write_line();
        self.increase_indent();

        self.write_indent();
        self.write("private constructor();");
        self.write_line();

        for (member_name, initializer) in &members {
            self.emit_js_class_like_member(*member_name, *initializer);
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
        true
    }

    pub(in crate::declaration_emitter) fn emit_js_class_like_member(
        &mut self,
        name_idx: NodeIndex,
        initializer: NodeIndex,
    ) {
        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };
        if init_node.kind == syntax_kind_ext::ARROW_FUNCTION
            || init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
        {
            self.emit_js_synthetic_class_method(name_idx, initializer);
            return;
        }

        self.write_indent();
        self.emit_node(name_idx);
        self.write(": ");

        if let Some(type_id) = self.get_node_type_or_names(&[initializer]) {
            let printed = self.print_type_id(type_id);
            self.write(&printed);
        } else {
            match init_node.kind {
                k if k == SyntaxKind::NumericLiteral as u16 => {
                    self.write("number");
                }
                k if k == SyntaxKind::StringLiteral as u16 => {
                    self.write("string");
                }
                k if k == SyntaxKind::BigIntLiteral as u16 => {
                    self.write("bigint");
                }
                k if k == SyntaxKind::TrueKeyword as u16
                    || k == SyntaxKind::FalseKeyword as u16 =>
                {
                    self.write("boolean");
                }
                k if k == SyntaxKind::NullKeyword as u16 => {
                    self.write("null");
                }
                k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                    self.write("{}");
                }
                k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                    self.write("any[]");
                }
                _ => {
                    self.write("any");
                }
            }
        }
        self.write(";");
        self.write_line();
    }

    /// Emit a function/arrow initializer's type annotation directly from the AST
    /// when it has an explicit return type. This preserves source-level type alias
    /// references in type parameter constraints and binding pattern formatting
    /// (including trailing commas) that the `TypePrinter` would otherwise expand.
    pub(in crate::declaration_emitter) fn emit_arrow_fn_type_from_ast(
        &mut self,
        initializer: NodeIndex,
    ) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && init_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return false;
        }
        let Some(func) = self.arena.get_function(init_node) else {
            return false;
        };
        if func.type_annotation.is_none() {
            return false;
        }
        self.write(": ");
        // Emit type parameters from AST
        if let Some(ref type_params) = func.type_parameters {
            if !type_params.nodes.is_empty() {
                self.emit_type_parameters(type_params);
            }
        }
        // Emit parameters from AST
        self.write("(");
        self.emit_parameters_with_body(&func.parameters, func.body);
        self.write(") => ");
        // Emit return type from AST
        self.emit_type(func.type_annotation);
        true
    }

    pub(in crate::declaration_emitter) fn emit_js_object_literal_namespace_if_possible(
        &mut self,
        decl_name: NodeIndex,
        initializer: NodeIndex,
        is_exported: bool,
    ) -> bool {
        if !self.source_is_js_file || !initializer.is_some() {
            return false;
        }

        let Some(name_node) = self.arena.get(decl_name) else {
            return false;
        };
        if name_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }

        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let Some(object) = self.arena.get_literal_expr(init_node) else {
            return false;
        };
        if object.elements.nodes.is_empty() {
            return false;
        }

        for &member_idx in &object.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                return false;
            };
            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let Some(prop) = self.arena.get_property_assignment(member_node) else {
                        return false;
                    };
                    let Some(prop_name_node) = self.arena.get(prop.name) else {
                        return false;
                    };
                    if prop_name_node.kind != SyntaxKind::Identifier as u16 {
                        return false;
                    }
                    if !self.js_namespace_object_member_initializer_supported(prop.initializer) {
                        return false;
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.arena.get_method_decl(member_node) else {
                        return false;
                    };
                    let Some(method_name_node) = self.arena.get(method.name) else {
                        return false;
                    };
                    if method_name_node.kind != SyntaxKind::Identifier as u16 {
                        return false;
                    }
                }
                _ => return false,
            }
        }

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write("namespace ");
        self.emit_node(decl_name);
        self.write(" {");
        self.write_line();
        self.increase_indent();

        for &member_idx in &object.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let Some(prop) = self.arena.get_property_assignment(member_node) else {
                        continue;
                    };
                    let Some(init_node) = self.arena.get(prop.initializer) else {
                        continue;
                    };
                    if init_node.kind == syntax_kind_ext::ARROW_FUNCTION
                        || init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    {
                        let Some(func) = self.arena.get_function(init_node) else {
                            continue;
                        };
                        self.emit_js_namespace_function_member(
                            prop.name,
                            func.type_parameters.as_ref(),
                            &func.parameters,
                            func.body,
                            func.type_annotation,
                        );
                    } else if let Some(type_text) =
                        self.js_namespace_value_member_type_text(prop.initializer)
                    {
                        self.emit_js_namespace_value_member(prop.name, &type_text);
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    self.emit_js_namespace_function_member(
                        method.name,
                        method.type_parameters.as_ref(),
                        &method.parameters,
                        method.body,
                        method.type_annotation,
                    );
                }
                _ => {}
            }
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
        true
    }

    pub(in crate::declaration_emitter) fn has_synthetic_js_expression_declaration(
        &self,
        stmt_idx: NodeIndex,
    ) -> bool {
        self.js_deferred_function_export_statements
            .contains_key(&stmt_idx)
            || self
                .js_deferred_value_export_statements
                .contains_key(&stmt_idx)
            || self
                .js_static_method_augmentation_statements
                .contains_key(&stmt_idx)
    }

    pub(in crate::declaration_emitter) fn emit_js_synthetic_expression_statement(
        &mut self,
        stmt_idx: NodeIndex,
    ) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };

        if let Some((name_idx, initializer, is_exported)) = self
            .js_deferred_function_export_statements
            .get(&stmt_idx)
            .copied()
        {
            self.emit_js_synthetic_function_declaration(name_idx, initializer, is_exported);
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            return;
        }

        if let Some((name_idx, initializer, is_exported)) = self
            .js_deferred_value_export_statements
            .get(&stmt_idx)
            .copied()
        {
            self.emit_js_synthetic_value_declaration(name_idx, initializer, is_exported);
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            return;
        }

        if let Some(group) = self
            .js_static_method_augmentation_statements
            .get(&stmt_idx)
            .cloned()
        {
            self.emit_js_static_method_augmentation_namespace(&group);
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
        }
    }

    pub(in crate::declaration_emitter) fn emit_js_synthetic_function_declaration(
        &mut self,
        name_idx: NodeIndex,
        initializer: NodeIndex,
        is_exported: bool,
    ) {
        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };
        if init_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && init_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return;
        }
        let Some(func) = self.arena.get_function(init_node) else {
            return;
        };

        let jsdoc = self.function_like_jsdoc_for_node(initializer);

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write("function ");
        self.emit_node(name_idx);

        let jsdoc_template_params = if func
            .type_parameters
            .as_ref()
            .is_none_or(|type_params| type_params.nodes.is_empty())
        {
            jsdoc
                .as_deref()
                .map(Self::parse_jsdoc_template_params)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        if let Some(ref type_params) = func.type_parameters {
            if !type_params.nodes.is_empty() {
                self.emit_type_parameters(type_params);
            } else if !jsdoc_template_params.is_empty() {
                self.emit_jsdoc_template_parameters(&jsdoc_template_params);
            }
        } else if !jsdoc_template_params.is_empty() {
            self.emit_jsdoc_template_parameters(&jsdoc_template_params);
        }

        self.write("(");
        self.emit_parameters_with_body(&func.parameters, func.body);
        self.write(")");

        if func.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(func.type_annotation);
        } else if let Some(return_type_text) = jsdoc
            .as_deref()
            .and_then(Self::parse_jsdoc_return_type_text)
        {
            self.write(": ");
            self.write(&return_type_text);
        } else if let (Some(interner), Some(cache)) = (&self.type_interner, &self.type_cache) {
            let func_type_id = cache
                .node_types
                .get(&initializer.0)
                .copied()
                .or_else(|| self.get_node_type_or_names(&[name_idx, initializer]));
            if let Some(func_type_id) = func_type_id
                && let Some(return_type_id) = type_queries::get_return_type(*interner, func_type_id)
            {
                if return_type_id == tsz_solver::types::TypeId::ANY
                    && func.body.is_some()
                    && self.body_returns_void(func.body)
                {
                    self.write(": void");
                } else {
                    self.write(": ");
                    self.write(&self.print_type_id(return_type_id));
                }
            } else if func.body.is_some() && self.body_returns_void(func.body) {
                self.write(": void");
            }
        } else if func.body.is_some() && self.body_returns_void(func.body) {
            self.write(": void");
        }

        self.write(";");
        self.write_line();
        if is_exported {
            self.emitted_module_indicator = true;
        }
    }

    pub(in crate::declaration_emitter) fn emit_js_synthetic_value_declaration(
        &mut self,
        name_idx: NodeIndex,
        initializer: NodeIndex,
        is_exported: bool,
    ) {
        if is_exported && self.emit_js_synthetic_class_expression_declaration(name_idx, initializer)
        {
            return;
        }

        let type_text = if is_exported {
            self.js_synthetic_export_value_type_text(initializer)
        } else {
            self.js_namespace_value_member_type_text(initializer)
        };
        let Some(type_text) = type_text else {
            return;
        };

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write(if is_exported {
            self.js_synthetic_export_value_keyword(initializer)
        } else {
            "var "
        });
        self.emit_node(name_idx);
        self.write(": ");
        self.write(&type_text);
        self.write(";");
        self.write_line();
        if is_exported {
            self.emitted_module_indicator = true;
        }
    }

    pub(in crate::declaration_emitter) fn emit_js_synthetic_class_expression_declaration(
        &mut self,
        name_idx: NodeIndex,
        initializer: NodeIndex,
    ) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::CLASS_EXPRESSION {
            return false;
        }
        let Some(class) = self.arena.get_class(init_node) else {
            return false;
        };

        let Some(export_name) = self.get_identifier_text(name_idx) else {
            return false;
        };
        let Some(class_name) = self.get_identifier_text(class.name) else {
            return false;
        };
        if class_name != export_name {
            return false;
        }

        let is_abstract = self
            .arena
            .has_modifier(&class.modifiers, SyntaxKind::AbstractKeyword);

        self.write_indent();
        self.write("export ");
        if self.should_emit_declare_keyword(true) {
            self.write("declare ");
        }
        if is_abstract {
            self.write("abstract ");
        }
        self.write("class ");
        self.emit_node(name_idx);

        if let Some(ref type_params) = class.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }
        if let Some(ref heritage) = class.heritage_clauses {
            self.emit_heritage_clauses(heritage);
        }

        self.write(" {");
        self.write_line();
        self.increase_indent();

        self.class_has_constructor_overloads = false;
        self.class_extends_another = class.heritage_clauses.as_ref().is_some_and(|hc| {
            hc.nodes.iter().any(|&clause_idx| {
                self.arena
                    .get_heritage_clause_at(clause_idx)
                    .is_some_and(|h| h.token == SyntaxKind::ExtendsKeyword as u16)
            })
        });
        self.method_names_with_overloads = FxHashSet::default();

        self.emit_parameter_properties(&class.members);
        if self.class_has_private_identifier_member(&class.members) {
            self.write_indent();
            self.write("#private;");
            self.write_line();
        }

        for &member_idx in &class.members.nodes {
            let before_jsdoc_len = self.writer.len();
            let saved_comment_idx = self.comment_emit_idx;
            if let Some(member_node) = self.arena.get(member_idx) {
                self.emit_leading_jsdoc_comments(member_node.pos);
            }
            let before_member_len = self.writer.len();
            self.emit_class_member(member_idx);
            if self.writer.len() == before_member_len {
                self.writer.truncate(before_jsdoc_len);
                self.comment_emit_idx = saved_comment_idx;
                if let Some(member_node) = self.arena.get(member_idx) {
                    self.skip_comments_in_node(member_node.pos, member_node.end);
                }
            }
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
        self.emitted_module_indicator = true;
        true
    }

    pub(in crate::declaration_emitter) fn emit_js_static_method_augmentation_namespace(
        &mut self,
        group: &crate::declaration_emitter::helpers::JsStaticMethodAugmentationGroup,
    ) {
        let Some(class_node) = self.arena.get(group.class_idx) else {
            return;
        };
        let Some(class) = self.arena.get_class(class_node) else {
            return;
        };
        let Some(method_node) = self.arena.get(group.method_idx) else {
            return;
        };
        let Some(method) = self.arena.get_method_decl(method_node) else {
            return;
        };

        self.write_indent();
        if group.class_is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(group.class_is_exported) {
            self.write("declare ");
        }
        self.write("namespace ");
        self.emit_node(class.name);
        self.write(" {");
        self.write_line();
        self.increase_indent();

        self.emit_js_namespace_function_member(
            method.name,
            method.type_parameters.as_ref(),
            &method.parameters,
            method.body,
            method.type_annotation,
        );

        self.write_indent();
        self.write("namespace ");
        self.emit_node(method.name);
        self.write(" {");
        self.write_line();
        self.increase_indent();

        for &(prop_name, initializer) in &group.properties {
            let Some(init_node) = self.arena.get(initializer) else {
                continue;
            };
            if init_node.kind == syntax_kind_ext::ARROW_FUNCTION
                || init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            {
                let Some(func) = self.arena.get_function(init_node) else {
                    continue;
                };
                self.emit_js_namespace_function_member(
                    prop_name,
                    func.type_parameters.as_ref(),
                    &func.parameters,
                    func.body,
                    func.type_annotation,
                );
            } else if let Some(type_text) = self.js_namespace_value_member_type_text(initializer) {
                self.emit_js_namespace_value_member(prop_name, &type_text);
            }
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    pub(in crate::declaration_emitter) fn emit_js_synthetic_prototype_class_if_needed(
        &mut self,
        name_idx: NodeIndex,
        is_exported: bool,
    ) {
        let Some(name) = self.get_identifier_text(name_idx) else {
            return;
        };
        let Some(methods) = self
            .js_deferred_prototype_method_statements
            .get(&name)
            .cloned()
        else {
            return;
        };
        if methods.is_empty() {
            return;
        }

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write("class ");
        self.emit_node(name_idx);
        self.write(" {");
        self.write_line();
        self.increase_indent();

        for (method_name, initializer) in methods {
            self.emit_js_synthetic_class_method(method_name, initializer);
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    pub(in crate::declaration_emitter) fn emit_js_synthetic_class_method(
        &mut self,
        name_idx: NodeIndex,
        initializer: NodeIndex,
    ) {
        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };
        if init_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && init_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return;
        }
        let Some(func) = self.arena.get_function(init_node) else {
            return;
        };

        let jsdoc = self.function_like_jsdoc_for_node(initializer);

        self.write_indent();
        self.emit_node(name_idx);

        let jsdoc_template_params = if func
            .type_parameters
            .as_ref()
            .is_none_or(|type_params| type_params.nodes.is_empty())
        {
            jsdoc
                .as_deref()
                .map(Self::parse_jsdoc_template_params)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        if let Some(ref type_params) = func.type_parameters {
            if !type_params.nodes.is_empty() {
                self.emit_type_parameters(type_params);
            } else if !jsdoc_template_params.is_empty() {
                self.emit_jsdoc_template_parameters(&jsdoc_template_params);
            }
        } else if !jsdoc_template_params.is_empty() {
            self.emit_jsdoc_template_parameters(&jsdoc_template_params);
        }

        self.write("(");
        self.emit_parameters_with_body(&func.parameters, func.body);
        self.write(")");

        if func.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(func.type_annotation);
        } else if let Some(return_type_text) = jsdoc
            .as_deref()
            .and_then(Self::parse_jsdoc_return_type_text)
        {
            self.write(": ");
            self.write(&return_type_text);
        } else if let (Some(interner), Some(cache)) = (&self.type_interner, &self.type_cache) {
            let func_type_id = cache
                .node_types
                .get(&initializer.0)
                .copied()
                .or_else(|| self.get_node_type_or_names(&[name_idx, initializer]));
            if let Some(func_type_id) = func_type_id
                && let Some(return_type_id) = type_queries::get_return_type(*interner, func_type_id)
            {
                if return_type_id == tsz_solver::types::TypeId::ANY
                    && func.body.is_some()
                    && self.body_returns_void(func.body)
                {
                    self.write(": void");
                } else {
                    self.write(": ");
                    self.write(&self.print_type_id(return_type_id));
                }
            } else if func.body.is_some() && self.body_returns_void(func.body) {
                self.write(": void");
            }
        } else if func.body.is_some() && self.body_returns_void(func.body) {
            self.write(": void");
        }

        self.write(";");
        self.write_line();
    }

    pub(in crate::declaration_emitter) fn emit_js_namespace_function_member(
        &mut self,
        name_idx: NodeIndex,
        type_params: Option<&NodeList>,
        parameters: &NodeList,
        body_idx: NodeIndex,
        type_annotation: NodeIndex,
    ) {
        self.write_indent();
        self.write("function ");
        self.emit_node(name_idx);
        if let Some(type_params) = type_params
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }
        self.write("(");
        self.emit_parameters_with_body(parameters, body_idx);
        self.write(")");
        if type_annotation.is_some() {
            self.write(": ");
            self.emit_type(type_annotation);
        } else if body_idx.is_some() && self.body_returns_void(body_idx) {
            self.write(": void");
        } else if !self.source_is_declaration_file {
            self.write(": any");
        }
        self.write(";");
        self.write_line();
    }

    pub(in crate::declaration_emitter) fn emit_js_namespace_value_member(
        &mut self,
        name_idx: NodeIndex,
        type_text: &str,
    ) {
        self.write_indent();
        self.write("let ");
        self.emit_node(name_idx);
        self.write(": ");
        self.write(type_text);
        self.write(";");
        self.write_line();
    }

    pub(in crate::declaration_emitter) fn js_namespace_value_member_type_text(
        &self,
        initializer: NodeIndex,
    ) -> Option<String> {
        let init_node = self.arena.get(initializer)?;
        match init_node.kind {
            k if k == SyntaxKind::StringLiteral as u16 => Some("string".to_string()),
            k if k == SyntaxKind::NumericLiteral as u16 => Some("number".to_string()),
            k if k == SyntaxKind::BigIntLiteral as u16 => Some("bigint".to_string()),
            k if k == SyntaxKind::TrueKeyword as u16 => Some("boolean".to_string()),
            k if k == SyntaxKind::FalseKeyword as u16 => Some("boolean".to_string()),
            k if k == SyntaxKind::NullKeyword as u16 => Some("null".to_string()),
            k if k == SyntaxKind::UndefinedKeyword as u16 => Some("undefined".to_string()),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if self.is_negative_literal(init_node) {
                    Some("number".to_string())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub(crate) fn js_synthetic_export_value_type_text(
        &self,
        initializer: NodeIndex,
    ) -> Option<String> {
        self.js_synthetic_export_value_type_text_inner(initializer, 0)
    }

    pub(in crate::declaration_emitter) fn js_synthetic_export_value_type_text_inner(
        &self,
        initializer: NodeIndex,
        depth: u8,
    ) -> Option<String> {
        if depth > 4 {
            return None;
        }

        let init_node = self.arena.get(initializer)?;
        if init_node.kind == SyntaxKind::UndefinedKeyword as u16
            || self.is_void_expression(init_node)
        {
            return None;
        }

        match init_node.kind {
            k if k == SyntaxKind::StringLiteral as u16 => {
                return self.get_source_slice_no_semi(init_node.pos, init_node.end);
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                return self.get_source_slice_no_semi(init_node.pos, init_node.end);
            }
            k if k == SyntaxKind::BigIntLiteral as u16 => {
                return self.get_source_slice_no_semi(init_node.pos, init_node.end);
            }
            k if k == SyntaxKind::TrueKeyword as u16 => return Some("true".to_string()),
            k if k == SyntaxKind::FalseKeyword as u16 => return Some("false".to_string()),
            k if k == SyntaxKind::NullKeyword as u16 => return Some("null".to_string()),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if self.is_negative_literal(init_node) {
                    return self.get_source_slice_no_semi(init_node.pos, init_node.end);
                }
            }
            _ => {}
        }

        if let Some(type_id) = self.get_node_type_or_names(&[initializer])
            && type_id != tsz_solver::types::TypeId::ANY
        {
            return Some(self.print_type_id(type_id));
        }

        if init_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(reference_text) = self.nameable_constructor_expression_text(initializer)
            && let Some(assigned_initializer) =
                self.js_assigned_initializer_for_value_reference(initializer)
        {
            let printed =
                self.js_synthetic_export_value_type_text_inner(assigned_initializer, depth + 1)?;
            return Some(self.rewrite_recursive_js_class_expression_type(
                assigned_initializer,
                &reference_text,
                printed,
            ));
        }

        match init_node.kind {
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION =>
            {
                self.infer_fallback_type_text(initializer)
            }
            _ => self.js_namespace_value_member_type_text(initializer),
        }
    }

    pub(crate) fn is_void_expression(&self, node: &Node) -> bool {
        node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            && self
                .arena
                .get_unary_expr(node)
                .is_some_and(|unary| unary.operator == SyntaxKind::VoidKeyword as u16)
    }

    pub(in crate::declaration_emitter) fn js_synthetic_export_value_keyword(
        &self,
        initializer: NodeIndex,
    ) -> &'static str {
        let resolved_initializer = self
            .js_assigned_initializer_for_value_reference(initializer)
            .unwrap_or(initializer);
        let Some(init_node) = self.arena.get(resolved_initializer) else {
            return "const ";
        };
        if init_node.kind == syntax_kind_ext::CLASS_EXPRESSION {
            "var "
        } else {
            "const "
        }
    }

    pub(in crate::declaration_emitter) fn rewrite_recursive_js_class_expression_type(
        &self,
        initializer: NodeIndex,
        reference_text: &str,
        printed: String,
    ) -> String {
        let Some(class_expr) = self.arena.get_class_at(initializer) else {
            return printed;
        };

        let mut rewritten = printed;
        for &member_idx in &class_expr.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::METHOD_DECLARATION {
                continue;
            }
            let Some(method) = self.arena.get_method_decl(member_node) else {
                continue;
            };
            if method.type_annotation.is_some()
                || !method.body.is_some()
                || !self.function_body_returns_new_reference(method.body, reference_text)
            {
                continue;
            }
            let Some(method_name) = self.get_identifier_text(method.name) else {
                continue;
            };
            rewritten = rewritten.replacen(
                &format!("{method_name}(): any;"),
                &format!("{method_name}(): /*elided*/ any;"),
                1,
            );
        }

        rewritten
    }

    pub(in crate::declaration_emitter) fn function_body_returns_new_reference(
        &self,
        body_idx: NodeIndex,
        reference_text: &str,
    ) -> bool {
        let Some(body_node) = self.arena.get(body_idx) else {
            return false;
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return false;
        };

        block
            .statements
            .nodes
            .iter()
            .copied()
            .any(|stmt_idx| self.statement_returns_new_reference(stmt_idx, reference_text))
    }

    pub(in crate::declaration_emitter) fn statement_returns_new_reference(
        &self,
        stmt_idx: NodeIndex,
        reference_text: &str,
    ) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };

        match stmt_node.kind {
            k if k == syntax_kind_ext::RETURN_STATEMENT => self
                .arena
                .get_return_statement(stmt_node)
                .is_some_and(|ret| {
                    self.expression_is_new_reference(ret.expression, reference_text)
                }),
            k if k == syntax_kind_ext::BLOCK => {
                self.arena.get_block(stmt_node).is_some_and(|block| {
                    block
                        .statements
                        .nodes
                        .iter()
                        .copied()
                        .any(|nested| self.statement_returns_new_reference(nested, reference_text))
                })
            }
            k if k == syntax_kind_ext::IF_STATEMENT => self
                .arena
                .get_if_statement(stmt_node)
                .is_some_and(|if_data| {
                    self.statement_returns_new_reference(if_data.then_statement, reference_text)
                        || (if_data.else_statement.is_some()
                            && self.statement_returns_new_reference(
                                if_data.else_statement,
                                reference_text,
                            ))
                }),
            _ => false,
        }
    }

    pub(in crate::declaration_emitter) fn expression_is_new_reference(
        &self,
        expr_idx: NodeIndex,
        reference_text: &str,
    ) -> bool {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return false;
        }
        let Some(new_expr) = self.arena.get_call_expr(expr_node) else {
            return false;
        };
        self.nameable_constructor_expression_text(new_expr.expression)
            .as_deref()
            == Some(reference_text)
    }

    pub(in crate::declaration_emitter) fn preferred_binding_source_type(
        &self,
        type_annotation: NodeIndex,
        initializer: NodeIndex,
        related_nodes: &[NodeIndex],
    ) -> Option<tsz_solver::types::TypeId> {
        if type_annotation.is_some()
            && let Some(type_id) = self.get_node_type(type_annotation)
        {
            return Some(type_id);
        }
        if initializer.is_some()
            && let Some(type_id) = self.get_node_type(initializer)
        {
            return Some(type_id);
        }
        self.get_node_type_or_names(related_nodes)
    }

    pub(in crate::declaration_emitter) fn destructuring_property_lookup_text(
        &self,
        node_idx: NodeIndex,
    ) -> Option<String> {
        let node = self.arena.get(node_idx)?;

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => self.get_identifier_text(node_idx),
            k if k == SyntaxKind::StringLiteral as u16 => {
                self.arena.get_literal(node).map(|lit| lit.text.clone())
            }
            k if k == SyntaxKind::NumericLiteral as u16 => self
                .arena
                .get_literal(node)
                .map(|lit| Self::normalize_numeric_literal(lit.text.as_ref())),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.arena.get_unary_expr(node)?;
                let operand_idx = self
                    .arena
                    .skip_parenthesized_and_assertions_and_comma(unary.operand);
                let operand_node = self.arena.get(operand_idx)?;
                if operand_node.kind != SyntaxKind::NumericLiteral as u16 {
                    return None;
                }
                let literal = self.arena.get_literal(operand_node)?;
                let normalized = Self::normalize_numeric_literal(literal.text.as_ref());
                match unary.operator {
                    k if k == SyntaxKind::MinusToken as u16 => Some(format!("-{normalized}")),
                    k if k == SyntaxKind::PlusToken as u16 => Some(normalized),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn array_binding_element_type(
        &self,
        tuple_elements: Option<&[tsz_solver::types::TupleElement]>,
        tuple_index: usize,
        array_element_type: Option<tsz_solver::types::TypeId>,
    ) -> Option<tsz_solver::types::TypeId> {
        if let Some(tuple_elements) = tuple_elements
            && let Some(tuple_element) = tuple_elements.get(tuple_index)
        {
            let mut type_id = if tuple_element.rest {
                self.type_interner.and_then(|interner| {
                    type_queries::get_array_element_type(interner, tuple_element.type_id)
                        .or(Some(tuple_element.type_id))
                })?
            } else {
                tuple_element.type_id
            };
            if tuple_element.optional
                && let Some(interner) = self.type_interner
            {
                type_id = interner.union(vec![type_id, tsz_solver::types::TypeId::UNDEFINED]);
            }
            return Some(type_id);
        }

        if array_element_type == Some(tsz_solver::types::TypeId::NEVER) {
            return Some(tsz_solver::types::TypeId::UNDEFINED);
        }

        array_element_type
    }

    pub(in crate::declaration_emitter) fn array_rest_binding_type(
        &self,
        source_type: Option<tsz_solver::types::TypeId>,
        tuple_elements: Option<&[tsz_solver::types::TupleElement]>,
        tuple_index: usize,
        array_element_type: Option<tsz_solver::types::TypeId>,
    ) -> Option<tsz_solver::types::TypeId> {
        let interner = self.type_interner?;

        if tuple_index == 0
            && let Some(source_type) = source_type
            && let Some(union_type) =
                type_queries::get_tuple_element_type_union(interner, source_type)
        {
            return Some(interner.array(union_type));
        }

        if let Some(tuple_elements) = tuple_elements {
            let remaining = tuple_elements
                .get(tuple_index..)
                .map_or_else(Vec::new, ToOwned::to_owned);
            return Some(interner.tuple(remaining));
        }

        array_element_type.map(|element_type| interner.array(element_type))
    }

    pub(in crate::declaration_emitter) fn object_binding_element_type(
        &self,
        source_type: Option<tsz_solver::types::TypeId>,
        element: &tsz_parser::parser::node::BindingElementData,
    ) -> Option<tsz_solver::types::TypeId> {
        let interner = self.type_interner?;
        let source_type = type_queries::unwrap_readonly(interner, source_type?);
        let property_name_idx = if element.property_name.is_some() {
            element.property_name
        } else {
            element.name
        };
        let property_name = self.destructuring_property_lookup_text(property_name_idx)?;
        let property =
            type_queries::find_property_in_type_by_str(interner, source_type, &property_name)?;
        if property.optional {
            Some(interner.union(vec![property.type_id, tsz_solver::types::TypeId::UNDEFINED]))
        } else {
            Some(property.type_id)
        }
    }

    pub(in crate::declaration_emitter) fn collect_typed_bindings_recursive(
        &self,
        node_idx: NodeIndex,
        source_type: Option<tsz_solver::types::TypeId>,
        bindings: &mut Vec<(NodeIndex, Option<tsz_solver::types::TypeId>)>,
    ) {
        let Some(node) = self.arena.get(node_idx) else {
            return;
        };

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                let type_id = source_type
                    .filter(|type_id| *type_id != tsz_solver::types::TypeId::ANY)
                    .or_else(|| self.get_node_type(node_idx))
                    .or_else(|| self.get_type_via_symbol(node_idx));
                bindings.push((node_idx, type_id));
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                if let Some(element) = self.arena.get_binding_element(node) {
                    let effective_type = source_type
                        .filter(|type_id| *type_id != tsz_solver::types::TypeId::ANY)
                        .or_else(|| {
                            if element.initializer.is_some() {
                                self.get_node_type(element.initializer)
                            } else {
                                None
                            }
                        })
                        .or_else(|| {
                            self.get_node_type_or_names(&[
                                node_idx,
                                element.name,
                                element.initializer,
                            ])
                        });
                    self.collect_typed_bindings_recursive(element.name, effective_type, bindings);
                }
            }
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                let tuple_elements = self.type_interner.and_then(|interner| {
                    source_type
                        .and_then(|type_id| type_queries::get_tuple_elements(interner, type_id))
                });
                let array_element_type = self.type_interner.and_then(|interner| {
                    source_type.and_then(|type_id| {
                        type_queries::get_array_element_type(interner, type_id).or_else(|| {
                            type_queries::get_tuple_element_type_union(interner, type_id)
                        })
                    })
                });

                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    let mut tuple_index = 0usize;
                    for &element_idx in &pattern.elements.nodes {
                        let Some(element_node) = self.arena.get(element_idx) else {
                            continue;
                        };
                        if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                            tuple_index += 1;
                            continue;
                        }
                        if element_node.kind != syntax_kind_ext::BINDING_ELEMENT {
                            continue;
                        }
                        let Some(element) = self.arena.get_binding_element(element_node) else {
                            continue;
                        };
                        let element_type = if element.dot_dot_dot_token {
                            self.array_rest_binding_type(
                                source_type,
                                tuple_elements.as_deref(),
                                tuple_index,
                                array_element_type,
                            )
                        } else {
                            self.array_binding_element_type(
                                tuple_elements.as_deref(),
                                tuple_index,
                                array_element_type,
                            )
                        };
                        self.collect_typed_bindings_recursive(element_idx, element_type, bindings);
                        if !element.dot_dot_dot_token {
                            tuple_index += 1;
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    for &element_idx in &pattern.elements.nodes {
                        let Some(element_node) = self.arena.get(element_idx) else {
                            continue;
                        };
                        if element_node.kind != syntax_kind_ext::BINDING_ELEMENT {
                            continue;
                        }
                        let Some(element) = self.arena.get_binding_element(element_node) else {
                            continue;
                        };
                        let element_type = if element.dot_dot_dot_token {
                            source_type
                        } else {
                            self.object_binding_element_type(source_type, element)
                        };
                        self.collect_typed_bindings_recursive(element_idx, element_type, bindings);
                    }
                }
            }
            _ => {}
        }
    }

    pub(in crate::declaration_emitter) fn collect_flattened_binding_entries(
        &self,
        pattern_idx: NodeIndex,
        source_type: Option<tsz_solver::types::TypeId>,
    ) -> Vec<(NodeIndex, Option<tsz_solver::types::TypeId>)> {
        let mut bindings = Vec::new();
        self.collect_typed_bindings_recursive(pattern_idx, source_type, &mut bindings);
        bindings
    }

    pub(in crate::declaration_emitter) fn emit_flattened_binding_type_annotation(
        &mut self,
        ident_idx: NodeIndex,
        type_id: Option<tsz_solver::types::TypeId>,
    ) {
        let type_id = type_id
            .or_else(|| self.get_node_type(ident_idx))
            .or_else(|| self.get_type_via_symbol(ident_idx));
        self.write(": ");
        if let Some(type_id) = type_id {
            self.write(&self.print_type_id(type_id));
        } else {
            self.write("any");
        }
    }

    /// Emits flattened variable declarations for destructuring patterns.
    ///
    /// In .d.ts files, destructuring like `export const { a, b } = obj;`
    /// must be flattened into individual declarations:
    /// `export declare const a: Type;`
    /// `export declare const b: Type;`
    pub(in crate::declaration_emitter) fn emit_flattened_variable_declaration(
        &mut self,
        decl_idx: NodeIndex,
        keyword: &str,
        is_exported: bool,
    ) {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return;
        };
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return;
        };
        let bindings = self.collect_flattened_binding_entries(
            decl.name,
            self.preferred_binding_source_type(
                decl.type_annotation,
                decl.initializer,
                &[decl_idx, decl.name, decl.initializer],
            ),
        );
        if bindings.is_empty() {
            return;
        }

        self.write_indent();
        if is_exported && (!self.inside_declare_namespace || self.ambient_module_has_scope_marker) {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write(keyword);
        self.write(" ");

        for (index, (ident_idx, type_id)) in bindings.into_iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            self.emit_node(ident_idx);
            self.emit_flattened_binding_type_annotation(ident_idx, type_id);
        }
        self.write(";");
        self.write_line();
    }

    pub(in crate::declaration_emitter) fn emit_parameter_property_modifiers(
        &mut self,
        modifiers: &Option<NodeList>,
    ) -> bool {
        let mut is_private = false;
        if let Some(modifiers) = modifiers {
            for &mod_idx in &modifiers.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    match mod_node.kind {
                        k if k == SyntaxKind::PrivateKeyword as u16 => {
                            self.write("private ");
                            is_private = true;
                        }
                        k if k == SyntaxKind::ProtectedKeyword as u16 => {
                            self.write("protected ");
                        }
                        k if k == SyntaxKind::ReadonlyKeyword as u16 => {
                            self.write("readonly ");
                        }
                        k if k == SyntaxKind::OverrideKeyword as u16 => {
                            // tsc strips `override` in .d.ts output.
                        }
                        _ => {}
                    }
                }
            }
        }
        is_private
    }

    /// Check if an initializer is a simple reference (identifier or qualified name)
    /// to a local import-equals alias (e.g. `import b = a.foo`).
    /// Returns the text to use after `typeof` if so (e.g. `"b"`).
    ///
    /// tsc emits `typeof <alias>` for variables initialized with an import-equals
    /// alias target rather than expanding the resolved type. This preserves the
    /// declarative reference in the .d.ts output.
    pub(in crate::declaration_emitter) fn initializer_import_alias_typeof_text(
        &self,
        initializer: NodeIndex,
    ) -> Option<String> {
        let binder = self.binder?;
        let init_node = self.arena.get(initializer)?;

        // Only handle simple identifier references.
        if init_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let ident = self.arena.get_identifier(init_node)?;
        let name = &ident.escaped_text;

        // Resolve the identifier by walking the scope chain from the enclosing scope.
        // The binder's node_symbols map only contains declaration-site mappings, not
        // usage-site references. We need to walk scopes to find the symbol for `b`.
        let scope_id = binder.find_enclosing_scope(self.arena, initializer)?;
        let sym_id = self.resolve_name_in_scope_chain(binder, scope_id, name)?;
        let sym = binder.symbols.get(sym_id)?;

        // Check if this symbol is an alias (import-equals creates ALIAS symbols)
        if sym.flags & tsz_binder::symbol_flags::ALIAS == 0 {
            return None;
        }

        // Verify that at least one declaration is an import-equals declaration
        let has_import_equals_decl = sym.declarations.iter().any(|&decl_idx| {
            self.arena
                .get(decl_idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION)
        });

        if !has_import_equals_decl {
            return None;
        }

        // tsc only emits `typeof alias` when the alias target is a function, class,
        // enum, or module — NOT when it targets a plain variable. For plain variables
        // (e.g. `import b = a.x` where `x` is `var x = 10`), tsc resolves and emits
        // the actual type (e.g. `number`).
        if self.import_alias_targets_plain_variable(binder, sym) {
            return None;
        }

        Some(name.clone())
    }

    /// Check whether an import-equals alias resolves to a plain variable.
    /// Returns `true` when the alias target's symbol has only VARIABLE flags
    /// (not FUNCTION, CLASS, ENUM, or MODULE).
    pub(in crate::declaration_emitter) fn import_alias_targets_plain_variable(
        &self,
        binder: &BinderState,
        alias_sym: &tsz_binder::Symbol,
    ) -> bool {
        use tsz_binder::symbol_flags;

        // Find the import-equals declaration to get the entity name reference.
        let import_decl_idx = alias_sym.declarations.iter().copied().find(|&decl_idx| {
            self.arena
                .get(decl_idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION)
        });
        let import_decl_idx = match import_decl_idx {
            Some(idx) => idx,
            None => return false,
        };
        let import_node = match self.arena.get(import_decl_idx) {
            Some(n) => n,
            None => return false,
        };
        let import_data = match self.arena.get_import_decl(import_node) {
            Some(d) => d,
            None => return false,
        };

        // module_specifier is the entity name (e.g. `a.x` or just `a`).
        // Resolve it to find the target symbol.
        let target_sym_id =
            self.resolve_entity_name_to_symbol(binder, import_data.module_specifier);
        let target_sym_id = match target_sym_id {
            Some(id) => id,
            None => return false,
        };
        let target_sym = match binder.symbols.get(target_sym_id) {
            Some(s) => s,
            None => return false,
        };

        // A "plain variable" has VARIABLE flags but not FUNCTION, CLASS, ENUM, or MODULE.
        let non_variable_value = symbol_flags::FUNCTION
            | symbol_flags::CLASS
            | symbol_flags::REGULAR_ENUM
            | symbol_flags::CONST_ENUM
            | symbol_flags::VALUE_MODULE
            | symbol_flags::NAMESPACE_MODULE;
        target_sym.flags & symbol_flags::VARIABLE != 0 && target_sym.flags & non_variable_value == 0
    }

    /// Resolve a qualified entity name (e.g. `a.x`) to its final symbol by walking
    /// through namespace exports. For a simple identifier, resolve via scope chain.
    pub(in crate::declaration_emitter) fn resolve_entity_name_to_symbol(
        &self,
        binder: &BinderState,
        entity_name: NodeIndex,
    ) -> Option<SymbolId> {
        let node = self.arena.get(entity_name)?;

        if node.kind == SyntaxKind::Identifier as u16 {
            // Simple identifier — resolve via scope chain from the entity name's location.
            let ident = self.arena.get_identifier(node)?;
            let scope_id = binder.find_enclosing_scope(self.arena, entity_name)?;
            self.resolve_name_in_scope_chain(binder, scope_id, &ident.escaped_text)
        } else if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            // Qualified name (e.g. `a.x`) — resolve left side first, then look up right
            // in the left symbol's exports.
            let qn = self.arena.get_qualified_name(node)?;
            let left_sym_id = self.resolve_entity_name_to_symbol(binder, qn.left)?;
            let left_sym = binder.symbols.get(left_sym_id)?;
            let right_node = self.arena.get(qn.right)?;
            let right_ident = self.arena.get_identifier(right_node)?;
            let right_name = &right_ident.escaped_text;

            // Look up in exports table of the left symbol.
            if let Some(exports) = &left_sym.exports {
                if let Some(sym_id) = exports.get(right_name) {
                    return Some(sym_id);
                }
            }
            None
        } else {
            None
        }
    }

    /// Walk the scope chain from `scope_id` upward, looking for a symbol with the given name.
    pub(in crate::declaration_emitter) fn resolve_name_in_scope_chain(
        &self,
        binder: &BinderState,
        start_scope: tsz_binder::scopes::ScopeId,
        name: &str,
    ) -> Option<SymbolId> {
        let mut scope_id = start_scope;
        let mut iterations = 0;
        while scope_id.is_some() {
            iterations += 1;
            if iterations > 100 {
                break;
            }
            let scope = binder.scopes.get(scope_id.0 as usize)?;
            if let Some(sym_id) = scope.table.get(name) {
                return Some(sym_id);
            }
            scope_id = scope.parent;
        }
        None
    }

    // Export/import emission → exports.rs
    // Type emission and utility helpers → helpers.rs
}
