//! Binding-name and `arguments` capture helpers for the async ES5 IR transformer.

use super::AsyncES5Transformer;
use crate::transforms::ir::IRNode;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl AsyncES5Transformer<'_> {
    pub(super) fn emit_arguments_capture_decl(&self, body: &mut Vec<IRNode>) {
        if self.state.captures_arguments {
            body.push(IRNode::VarDecl {
                name: self.state.arguments_capture_name.clone().into(),
                initializer: Some(Box::new(IRNode::Raw("arguments".to_string().into()))),
            });
        }
    }

    pub(super) fn fresh_arguments_capture_name(
        &self,
        body_idx: NodeIndex,
        params: &[String],
    ) -> String {
        let mut binding_names = params.to_vec();
        self.collect_body_binding_names(body_idx, &mut binding_names);

        let mut suffix = 1usize;
        loop {
            let candidate = format!("arguments_{suffix}");
            if !binding_names.iter().any(|name| name == &candidate) {
                return candidate;
            }
            suffix += 1;
        }
    }

    pub(super) fn collect_parameter_binding_names(
        &self,
        params: &tsz_parser::parser::NodeList,
        names: &mut Vec<String>,
    ) {
        for &param_idx in &params.nodes {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            self.collect_binding_name(param.name, names);
        }
    }

    pub(super) fn collect_body_binding_names(&self, idx: NodeIndex, names: &mut Vec<String>) {
        if idx.is_none() {
            return;
        }
        let Some(node) = self.arena.get(idx) else {
            return;
        };
        if let Some(source_file) = self.arena.get_source_file(node) {
            for &stmt_idx in &source_file.statements.nodes {
                self.collect_body_binding_names(stmt_idx, names);
            }
            return;
        }

        match node.kind {
            k if k == syntax_kind_ext::BLOCK || k == syntax_kind_ext::CASE_BLOCK => {
                if let Some(block) = self.arena.get_block(node) {
                    for &stmt_idx in &block.statements.nodes {
                        self.collect_body_binding_names(stmt_idx, names);
                    }
                }
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT
                || k == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                || k == syntax_kind_ext::VARIABLE_DECLARATION =>
            {
                self.collect_variable_binding_names(idx, names);
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = self.arena.get_function(node) {
                    self.collect_binding_name(func.name, names);
                }
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = self.arena.get_class(node) {
                    self.collect_binding_name(class.name, names);
                }
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_data) = self.arena.get_enum(node) {
                    self.collect_binding_name(enum_data.name, names);
                }
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                if let Some(module) = self.arena.get_module(node) {
                    self.collect_binding_name(module.name, names);
                }
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.arena.get_if_statement(node) {
                    self.collect_body_binding_names(if_stmt.then_statement, names);
                    self.collect_body_binding_names(if_stmt.else_statement, names);
                }
            }
            k if k == syntax_kind_ext::FOR_STATEMENT
                || k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT =>
            {
                if let Some(loop_data) = self.arena.get_loop(node) {
                    self.collect_variable_binding_names(loop_data.initializer, names);
                    self.collect_body_binding_names(loop_data.statement, names);
                }
            }
            k if k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT =>
            {
                if let Some(for_in_of) = self.arena.get_for_in_of(node) {
                    self.collect_variable_binding_names(for_in_of.initializer, names);
                    self.collect_body_binding_names(for_in_of.statement, names);
                }
            }
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_stmt) = self.arena.get_try(node) {
                    self.collect_body_binding_names(try_stmt.try_block, names);
                    self.collect_body_binding_names(try_stmt.catch_clause, names);
                    self.collect_body_binding_names(try_stmt.finally_block, names);
                }
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_clause) = self.arena.get_catch_clause(node) {
                    self.collect_variable_binding_names(catch_clause.variable_declaration, names);
                    self.collect_body_binding_names(catch_clause.block, names);
                }
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch_stmt) = self.arena.get_switch(node) {
                    self.collect_body_binding_names(switch_stmt.case_block, names);
                }
            }
            k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
                if let Some(clause) = self.arena.get_case_clause(node) {
                    for &stmt_idx in &clause.statements.nodes {
                        self.collect_body_binding_names(stmt_idx, names);
                    }
                }
            }
            k if k == syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled) = self.arena.get_labeled_statement(node) {
                    self.collect_body_binding_names(labeled.statement, names);
                }
            }
            k if k == syntax_kind_ext::WITH_STATEMENT => {
                if let Some(with_stmt) = self.arena.get_with_statement(node) {
                    self.collect_body_binding_names(with_stmt.then_statement, names);
                }
            }
            _ => {}
        }
    }

    pub(super) fn collect_variable_binding_names(&self, idx: NodeIndex, names: &mut Vec<String>) {
        if idx.is_none() {
            return;
        }
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            if let Some(decl) = self.arena.get_variable_declaration(node) {
                self.collect_binding_name(decl.name, names);
            }
            return;
        }

        if let Some(var_data) = self.arena.get_variable(node) {
            for &decl_idx in &var_data.declarations.nodes {
                self.collect_variable_binding_names(decl_idx, names);
            }
        }
    }

    pub(super) fn collect_binding_name(&self, name_idx: NodeIndex, names: &mut Vec<String>) {
        if name_idx.is_none() {
            return;
        }
        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };

        if name_node.is_identifier() {
            if let Some(name) = crate::transforms::emit_utils::identifier_text(self.arena, name_idx)
                && !names.contains(&name)
            {
                names.push(name);
            }
            return;
        }

        match name_node.kind {
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
            {
                if let Some(pattern) = self.arena.get_binding_pattern(name_node) {
                    for &elem_idx in &pattern.elements.nodes {
                        self.collect_binding_name(elem_idx, names);
                    }
                }
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                if let Some(elem) = self.arena.get_binding_element(name_node) {
                    self.collect_binding_name(elem.name, names);
                }
            }
            _ => {}
        }
    }
}
