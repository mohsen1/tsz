use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    pub(super) fn format_interface_heritage_base_name(
        &mut self,
        base_sym_id: tsz_binder::SymbolId,
        type_idx: NodeIndex,
        expr_idx: NodeIndex,
        base_name_raw: &str,
        type_arguments: Option<&tsz_parser::parser::NodeList>,
    ) -> String {
        if let Some(args) = type_arguments {
            let arg_strs: Vec<String> = args
                .nodes
                .iter()
                .map(|&arg_idx| self.format_interface_heritage_type_argument(arg_idx))
                .collect();
            return if arg_strs.is_empty() {
                base_name_raw.to_string()
            } else {
                format!("{}<{}>", base_name_raw, arg_strs.join(", "))
            };
        }

        let base_type = self.get_type_from_type_node(type_idx);
        let evaluated = self.evaluate_type_with_env(base_type);
        if crate::query_boundaries::common::is_array_or_tuple_type(self.ctx.types, evaluated)
            || crate::query_boundaries::common::is_type_query_type(self.ctx.types, evaluated)
        {
            return self.format_type_diagnostic(evaluated);
        }

        if let Some(alias_text) = self.type_alias_target_text_for_symbol(base_sym_id)
            && alias_text.starts_with("typeof ")
        {
            return alias_text;
        }

        if let Some(text) = self.node_text(expr_idx)
            && text.starts_with("typeof ")
        {
            return text.trim().to_string();
        }

        base_name_raw.to_string()
    }

    fn type_alias_target_text_for_symbol(&self, sym_id: tsz_binder::SymbolId) -> Option<String> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        for &decl_idx in &symbol.declarations {
            let decl_arena =
                self.ctx
                    .binder
                    .arena_for_declaration_or(sym_id, decl_idx, self.ctx.arena);
            let Some(node) = decl_arena.get(decl_idx) else {
                continue;
            };
            let Some(alias) = decl_arena.get_type_alias(node) else {
                continue;
            };
            let Some(type_node) = decl_arena.get(alias.type_node) else {
                continue;
            };
            let (start, end) = (type_node.pos, type_node.end);
            let Some(source_file) = decl_arena.source_files.first() else {
                continue;
            };
            let source = source_file.text.as_ref();
            let start = start as usize;
            let end = end as usize;
            if start < end && end <= source.len() {
                return Some(source[start..end].trim().to_string());
            }
        }
        None
    }

    fn format_interface_heritage_type_argument(&mut self, arg_idx: NodeIndex) -> String {
        let Some(node) = self.ctx.arena.get(arg_idx) else {
            let arg_type = self.get_type_from_type_node(arg_idx);
            return self.format_type(arg_type);
        };

        if node.kind == syntax_kind_ext::INTERSECTION_TYPE
            && let Some(text) = self.node_text(arg_idx)
        {
            return Self::normalize_heritage_type_argument_text(&text, true);
        }

        let arg_type = self.get_type_from_type_node(arg_idx);
        self.format_type(arg_type)
    }

    fn normalize_heritage_type_argument_text(text: &str, strip_trailing_gt: bool) -> String {
        let mut normalized = text.trim().to_string();
        if strip_trailing_gt
            && normalized.ends_with('>')
            && normalized.chars().filter(|&ch| ch == '>').count()
                > normalized.chars().filter(|&ch| ch == '<').count()
        {
            normalized.pop();
            normalized = normalized.trim_end().to_string();
        }
        if normalized.contains('{') && normalized.contains(':') && normalized.contains('}') {
            normalized = normalized.replace(";}", "; }");
            if let Some(close_brace) = normalized.rfind('}') {
                let before = normalized[..close_brace].trim_end();
                if !before.ends_with(';') {
                    normalized = format!("{}; }}{}", before, &normalized[close_brace + 1..]);
                }
            }
        }
        normalized
    }
}
