use super::*;

impl<'a> Printer<'a> {
    pub(super) fn collect_top_level_using_block_envs(
        &self,
        statements: &NodeList,
        start_idx: usize,
    ) -> Vec<NodeIndex> {
        let mut block_indices = Vec::new();
        for &stmt_idx in &statements.nodes[start_idx..] {
            self.collect_top_level_using_block_envs_in_statement(stmt_idx, &mut block_indices);
        }
        block_indices
    }

    fn collect_top_level_using_block_envs_in_statement(
        &self,
        stmt_idx: NodeIndex,
        block_indices: &mut Vec<NodeIndex>,
    ) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };

        match stmt_node.kind {
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = self.arena.get_block(stmt_node) {
                    let child_statements = block.statements.clone();
                    if self.block_has_using_declarations(&child_statements) {
                        block_indices.push(stmt_idx);
                    }
                    for &child_idx in &child_statements.nodes {
                        self.collect_top_level_using_block_envs_in_statement(
                            child_idx,
                            block_indices,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::CASE_BLOCK => {
                if let Some(block) = self.arena.get_block(stmt_node) {
                    let child_statements = block.statements.clone();
                    for &child_idx in &child_statements.nodes {
                        self.collect_top_level_using_block_envs_in_statement(
                            child_idx,
                            block_indices,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.arena.get_if_statement(stmt_node) {
                    self.collect_top_level_using_block_envs_in_statement(
                        if_stmt.then_statement,
                        block_indices,
                    );
                    self.collect_top_level_using_block_envs_in_statement(
                        if_stmt.else_statement,
                        block_indices,
                    );
                }
            }
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_stmt) = self.arena.get_try(stmt_node) {
                    self.collect_top_level_using_block_envs_in_statement(
                        try_stmt.try_block,
                        block_indices,
                    );
                    self.collect_top_level_using_block_envs_in_statement(
                        try_stmt.catch_clause,
                        block_indices,
                    );
                    self.collect_top_level_using_block_envs_in_statement(
                        try_stmt.finally_block,
                        block_indices,
                    );
                }
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_clause) = self.arena.get_catch_clause(stmt_node) {
                    self.collect_top_level_using_block_envs_in_statement(
                        catch_clause.block,
                        block_indices,
                    );
                }
            }
            k if k == syntax_kind_ext::FOR_STATEMENT
                || k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT =>
            {
                if let Some(loop_stmt) = self.arena.get_loop(stmt_node) {
                    self.collect_top_level_using_block_envs_in_statement(
                        loop_stmt.statement,
                        block_indices,
                    );
                }
            }
            k if k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT =>
            {
                if let Some(for_in_of) = self.arena.get_for_in_of(stmt_node) {
                    self.collect_top_level_using_block_envs_in_statement(
                        for_in_of.statement,
                        block_indices,
                    );
                }
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch_stmt) = self.arena.get_switch(stmt_node) {
                    self.collect_top_level_using_block_envs_in_statement(
                        switch_stmt.case_block,
                        block_indices,
                    );
                }
            }
            k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
                if let Some(clause) = self.arena.get_case_clause(stmt_node) {
                    let child_statements = clause.statements.clone();
                    for &child_idx in &child_statements.nodes {
                        self.collect_top_level_using_block_envs_in_statement(
                            child_idx,
                            block_indices,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled) = self.arena.get_labeled_statement(stmt_node) {
                    self.collect_top_level_using_block_envs_in_statement(
                        labeled.statement,
                        block_indices,
                    );
                }
            }
            k if k == syntax_kind_ext::WITH_STATEMENT => {
                if let Some(with_stmt) = self.arena.get_with_statement(stmt_node) {
                    self.collect_top_level_using_block_envs_in_statement(
                        with_stmt.then_statement,
                        block_indices,
                    );
                }
            }
            _ => {}
        }
    }

    pub(super) fn count_top_level_using_es5_resource_initializer_temps(
        &self,
        statements: &NodeList,
        start_idx: usize,
    ) -> usize {
        if !self.ctx.target_es5 {
            return 0;
        }

        statements.nodes[start_idx..]
            .iter()
            .copied()
            .map(|stmt_idx| self.count_top_level_using_es5_resource_temps_in_statement(stmt_idx))
            .sum()
    }

    fn count_top_level_using_es5_resource_temps_in_statement(&self, stmt_idx: NodeIndex) -> usize {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return 0;
        };

        match stmt_node.kind {
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.count_top_level_using_es5_resource_temps_in_variable(stmt_node)
            }
            k if k == syntax_kind_ext::BLOCK || k == syntax_kind_ext::CASE_BLOCK => {
                self.arena.get_block(stmt_node).map_or(0, |block| {
                    block
                        .statements
                        .nodes
                        .iter()
                        .copied()
                        .map(|child_idx| {
                            self.count_top_level_using_es5_resource_temps_in_statement(child_idx)
                        })
                        .sum()
                })
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                self.arena.get_if_statement(stmt_node).map_or(0, |if_stmt| {
                    self.count_top_level_using_es5_resource_temps_in_statement(
                        if_stmt.then_statement,
                    ) + self.count_top_level_using_es5_resource_temps_in_statement(
                        if_stmt.else_statement,
                    )
                })
            }
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                self.arena.get_try(stmt_node).map_or(0, |try_stmt| {
                    self.count_top_level_using_es5_resource_temps_in_statement(try_stmt.try_block)
                        + self.count_top_level_using_es5_resource_temps_in_statement(
                            try_stmt.catch_clause,
                        )
                        + self.count_top_level_using_es5_resource_temps_in_statement(
                            try_stmt.finally_block,
                        )
                })
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => self
                .arena
                .get_catch_clause(stmt_node)
                .map_or(0, |catch_clause| {
                    self.count_top_level_using_es5_resource_temps_in_statement(catch_clause.block)
                }),
            k if k == syntax_kind_ext::FOR_STATEMENT
                || k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT =>
            {
                self.arena.get_loop(stmt_node).map_or(0, |loop_stmt| {
                    self.count_top_level_using_es5_resource_temps_in_statement(loop_stmt.statement)
                })
            }
            k if k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT =>
            {
                self.arena.get_for_in_of(stmt_node).map_or(0, |for_in_of| {
                    self.count_top_level_using_es5_resource_temps_in_statement(for_in_of.statement)
                })
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                self.arena.get_switch(stmt_node).map_or(0, |switch_stmt| {
                    self.count_top_level_using_es5_resource_temps_in_statement(
                        switch_stmt.case_block,
                    )
                })
            }
            k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
                self.arena.get_case_clause(stmt_node).map_or(0, |clause| {
                    clause
                        .statements
                        .nodes
                        .iter()
                        .copied()
                        .map(|child_idx| {
                            self.count_top_level_using_es5_resource_temps_in_statement(child_idx)
                        })
                        .sum()
                })
            }
            k if k == syntax_kind_ext::LABELED_STATEMENT => self
                .arena
                .get_labeled_statement(stmt_node)
                .map_or(0, |labeled| {
                    self.count_top_level_using_es5_resource_temps_in_statement(labeled.statement)
                }),
            k if k == syntax_kind_ext::WITH_STATEMENT => self
                .arena
                .get_with_statement(stmt_node)
                .map_or(0, |with_stmt| {
                    self.count_top_level_using_es5_resource_temps_in_statement(
                        with_stmt.then_statement,
                    )
                }),
            _ => 0,
        }
    }

    fn count_top_level_using_es5_resource_temps_in_variable(&self, node: &Node) -> usize {
        let Some(var_stmt) = self.arena.get_variable(node) else {
            return 0;
        };

        let mut count = 0usize;
        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            let flags = decl_list_node.flags as u32;
            let is_using = (flags & tsz_parser::parser::node_flags::USING) != 0
                || tsz_parser::parser::node_flags::is_await_using(flags);
            if !is_using {
                continue;
            }

            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };
            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                count += self.count_es5_resource_expression_hoisted_temps(decl.initializer);
            }
        }
        count
    }
}
