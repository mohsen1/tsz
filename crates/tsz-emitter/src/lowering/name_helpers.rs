//! Name inference and capture helpers for the lowering pass.

use super::*;

impl<'a> LoweringPass<'a> {
    /// Compute the capture variable name for `_this` in a given scope.
    /// Keep trying TypeScript's `_this`, `_this_1`, `_this_2`, ... pattern until
    /// the generated name does not collide with a binding in the same scope.
    pub(super) fn compute_this_capture_name(&self, body_idx: NodeIndex) -> Arc<str> {
        self.compute_this_capture_name_with_params(body_idx, None)
    }

    /// Compute capture name, also checking function parameters for collision.
    pub(super) fn compute_this_capture_name_with_params(
        &self,
        body_idx: NodeIndex,
        params: Option<&NodeList>,
    ) -> Arc<str> {
        let mut suffix = 0usize;
        loop {
            let candidate = if suffix == 0 {
                "_this".to_string()
            } else {
                format!("_this_{suffix}")
            };
            if !self.scope_has_name(body_idx, &candidate)
                && !self.params_have_name(params, &candidate)
            {
                return Arc::from(candidate);
            }
            suffix += 1;
        }
    }

    /// Check if any parameter in the list has the given name.
    pub(super) fn params_have_name(&self, params: Option<&NodeList>, name: &str) -> bool {
        let Some(params) = params else {
            return false;
        };
        for &param_idx in &params.nodes {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            if let Some(param) = self.arena.get_parameter(param_node)
                && self.get_identifier_text_ref(param.name) == Some(name)
            {
                return true;
            }
        }
        false
    }

    /// Check if a function body (block or source file) contains a variable
    /// declaration or parameter with the given name at its direct scope level.
    pub(super) fn scope_has_name(&self, body_idx: NodeIndex, name: &str) -> bool {
        let Some(node) = self.arena.get(body_idx) else {
            return false;
        };

        let statements = if let Some(block) = self.arena.get_block(node) {
            &block.statements
        } else if let Some(sf) = self.arena.get_source_file(node) {
            &sf.statements
        } else {
            return false;
        };

        for &stmt_idx in &statements.nodes {
            let Some(stmt) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt.kind == syntax_kind_ext::VARIABLE_STATEMENT
                && let Some(var_stmt_data) = self.arena.get_variable(stmt)
                && self.variable_declaration_list_has_name(&var_stmt_data.declarations, name)
            {
                return true;
            }
            if stmt.kind == syntax_kind_ext::FUNCTION_DECLARATION
                && let Some(func) = self.arena.get_function(stmt)
                && self.get_identifier_text_ref(func.name) == Some(name)
            {
                return true;
            }
        }

        false
    }

    fn variable_declaration_list_has_name(&self, declarations: &NodeList, name: &str) -> bool {
        for &decl_list_idx in &declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            if let Some(decl_list_data) = self.arena.get_variable(decl_list_node) {
                for &decl_idx in &decl_list_data.declarations.nodes {
                    let Some(decl_node) = self.arena.get(decl_idx) else {
                        continue;
                    };
                    if self.variable_declaration_has_name(decl_node, name) {
                        return true;
                    }
                }
            }
            if self.variable_declaration_has_name(decl_list_node, name) {
                return true;
            }
        }
        false
    }

    fn variable_declaration_has_name(&self, decl_node: &Node, name: &str) -> bool {
        self.arena
            .get_variable_declaration(decl_node)
            .is_some_and(|decl| self.get_identifier_text_ref(decl.name) == Some(name))
    }

    /// Infer the function name for a class expression used in named evaluation.
    /// Looks at the source text context to find the assignment target name.
    /// Returns the name string for `__setFunctionName(_classThis, name)`.
    pub(super) fn infer_class_expression_function_name(
        &self,
        _class_idx: tsz_parser::parser::NodeIndex,
        class_node: &tsz_parser::parser::node::Node,
    ) -> Option<String> {
        let text: &str = self.arena.source_files.iter().find_map(|sf| {
            if (class_node.pos as usize) < sf.text.len() {
                Some(sf.text.as_ref())
            } else {
                None
            }
        })?;
        let class_pos = class_node.pos as usize;

        let trimmed = self.skip_class_name_prefix_noise(&text[..class_pos.min(text.len())]);

        if let Some(prefix) = trimmed.strip_suffix("default")
            && prefix.trim_end().ends_with("export")
        {
            return Some("default".to_string());
        }

        if let Some(prefix) = trimmed.strip_suffix('=')
            && prefix.trim_end().ends_with("export")
        {
            return Some(String::new());
        }

        let assignment_stripped =
            if trimmed.ends_with("||=") || trimmed.ends_with("&&=") || trimmed.ends_with("??=") {
                trimmed[..trimmed.len() - 3].trim_end()
            } else if trimmed.ends_with('=') && !trimmed.ends_with("==") {
                trimmed[..trimmed.len() - 1].trim_end()
            } else {
                return None;
            };

        let ident_end = assignment_stripped.len();
        let ident_start = assignment_stripped
            .rfind(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '$')
            .map(|p| p + 1)
            .unwrap_or(0);
        let name = &assignment_stripped[ident_start..ident_end];
        if !name.is_empty() && name.as_bytes()[0].is_ascii_alphabetic()
            || name.starts_with('_')
            || name.starts_with('$')
        {
            Some(name.to_string())
        } else {
            None
        }
    }

    fn skip_class_name_prefix_noise<'text>(&self, before_class: &'text str) -> &'text str {
        let mut scan = before_class.trim_end();
        loop {
            let prev = scan;
            scan = scan.trim_end();
            if scan.ends_with(')') {
                let mut depth = 1;
                let mut p = scan.len() - 2;
                while p > 0 && depth > 0 {
                    match scan.as_bytes()[p] {
                        b')' => depth += 1,
                        b'(' => depth -= 1,
                        _ => {}
                    }
                    if depth > 0 {
                        p -= 1;
                    }
                }
                scan = scan[..p].trim_end();
            }
            if let Some(at_pos) = scan.rfind('@') {
                let ident = scan[at_pos + 1..].trim();
                if !ident.is_empty()
                    && ident
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$')
                {
                    scan = scan[..at_pos].trim_end();
                }
            }
            while scan.ends_with('(') {
                scan = scan[..scan.len() - 1].trim_end();
            }
            if scan == prev {
                break;
            }
        }
        scan
    }
}
