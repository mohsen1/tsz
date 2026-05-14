//! Async ES2015 parameter-shadowing helpers.
//!
//! Kept separate from `helpers_async.rs` so the main async emitter stays under
//! the file-size ceiling.

use super::super::*;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn async_generator_parameter_binding_names(
        &self,
        params: &[NodeIndex],
    ) -> Vec<String> {
        let mut names = Vec::new();
        for &param_idx in params {
            if let Some(param) = self.arena.get_parameter_at(param_idx) {
                self.collect_binding_names(param.name, &mut names);
            }
        }
        names
    }

    pub(in crate::emitter) fn async_generator_shadowed_var_names(
        &self,
        body: NodeIndex,
        parameter_names: &[String],
    ) -> Vec<String> {
        let mut names = Vec::new();
        if !parameter_names.is_empty() {
            self.collect_async_generator_shadowed_var_names(body, parameter_names, &mut names);
        }
        names
    }

    fn collect_async_generator_shadowed_var_names(
        &self,
        idx: NodeIndex,
        parameter_names: &[String],
        names: &mut Vec<String>,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::BLOCK || k == syntax_kind_ext::CASE_BLOCK => {
                if let Some(block) = self.arena.get_block(node) {
                    for &stmt_idx in &block.statements.nodes {
                        self.collect_async_generator_shadowed_var_names(
                            stmt_idx,
                            parameter_names,
                            names,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = self.arena.get_variable(node) {
                    for &decl_list_idx in &var_stmt.declarations.nodes {
                        self.collect_async_generator_shadowed_decl_list_names(
                            decl_list_idx,
                            parameter_names,
                            names,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.arena.get_if_statement(node) {
                    self.collect_async_generator_shadowed_var_names(
                        if_stmt.then_statement,
                        parameter_names,
                        names,
                    );
                    self.collect_async_generator_shadowed_var_names(
                        if_stmt.else_statement,
                        parameter_names,
                        names,
                    );
                }
            }
            k if k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT
                || k == syntax_kind_ext::FOR_STATEMENT =>
            {
                if let Some(loop_stmt) = self.arena.get_loop(node) {
                    self.collect_async_generator_shadowed_decl_list_names(
                        loop_stmt.initializer,
                        parameter_names,
                        names,
                    );
                    self.collect_async_generator_shadowed_var_names(
                        loop_stmt.statement,
                        parameter_names,
                        names,
                    );
                }
            }
            k if k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT =>
            {
                if let Some(for_in_of) = self.arena.get_for_in_of(node) {
                    self.collect_async_generator_shadowed_decl_list_names(
                        for_in_of.initializer,
                        parameter_names,
                        names,
                    );
                    self.collect_async_generator_shadowed_var_names(
                        for_in_of.statement,
                        parameter_names,
                        names,
                    );
                }
            }
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_stmt) = self.arena.get_try(node) {
                    self.collect_async_generator_shadowed_var_names(
                        try_stmt.try_block,
                        parameter_names,
                        names,
                    );
                    if let Some(catch_node) = self.arena.get(try_stmt.catch_clause)
                        && let Some(catch_clause) = self.arena.get_catch_clause(catch_node)
                        && !self.catch_binding_shadows_async_parameter(
                            catch_clause.variable_declaration,
                            parameter_names,
                        )
                    {
                        self.collect_async_generator_shadowed_var_names(
                            catch_clause.block,
                            parameter_names,
                            names,
                        );
                    }
                    self.collect_async_generator_shadowed_var_names(
                        try_stmt.finally_block,
                        parameter_names,
                        names,
                    );
                }
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch_stmt) = self.arena.get_switch(node) {
                    self.collect_async_generator_shadowed_var_names(
                        switch_stmt.case_block,
                        parameter_names,
                        names,
                    );
                }
            }
            k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
                if let Some(case_clause) = self.arena.get_case_clause(node) {
                    for &stmt_idx in &case_clause.statements.nodes {
                        self.collect_async_generator_shadowed_var_names(
                            stmt_idx,
                            parameter_names,
                            names,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled) = self.arena.get_labeled_statement(node) {
                    self.collect_async_generator_shadowed_var_names(
                        labeled.statement,
                        parameter_names,
                        names,
                    );
                }
            }
            _ => {}
        }
    }

    fn collect_async_generator_shadowed_decl_list_names(
        &self,
        decl_list_idx: NodeIndex,
        parameter_names: &[String],
        names: &mut Vec<String>,
    ) {
        let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
            return;
        };
        if decl_list_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            return;
        }
        let flags = decl_list_node.flags as u32;
        if flags
            & (tsz_parser::parser::node_flags::LET
                | tsz_parser::parser::node_flags::CONST
                | tsz_parser::parser::node_flags::USING)
            != 0
        {
            return;
        }
        let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
            return;
        };

        let mut list_names = Vec::new();
        for &decl_idx in &decl_list.declarations.nodes {
            if let Some(decl_node) = self.arena.get(decl_idx)
                && let Some(decl) = self.arena.get_variable_declaration(decl_node)
            {
                self.collect_binding_names(decl.name, &mut list_names);
            }
        }
        if list_names
            .iter()
            .any(|name| parameter_names.iter().any(|param| param == name))
        {
            for name in list_names {
                if !names.iter().any(|existing| existing == &name) {
                    names.push(name);
                }
            }
        }
    }

    fn catch_binding_shadows_async_parameter(
        &self,
        variable_declaration: NodeIndex,
        parameter_names: &[String],
    ) -> bool {
        let Some(var_node) = self.arena.get(variable_declaration) else {
            return false;
        };
        let Some(var_decl) = self.arena.get_variable_declaration(var_node) else {
            return false;
        };
        let mut names = Vec::new();
        self.collect_binding_names(var_decl.name, &mut names);
        names
            .iter()
            .any(|name| parameter_names.iter().any(|param| param == name))
    }
}
