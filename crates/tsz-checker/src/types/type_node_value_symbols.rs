use super::type_node::TypeNodeChecker;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl TypeNodeChecker<'_, '_> {
    pub(super) fn declared_type_annotation_for_value_symbol(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<NodeIndex> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let mut decl = symbol.value_declaration;
        if decl.is_none() {
            decl = symbol.primary_declaration()?;
        }
        let decl_node = self.ctx.arena.get(decl)?;
        if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
            return var_decl
                .type_annotation
                .is_some()
                .then_some(var_decl.type_annotation);
        }
        if decl_node.kind == syntax_kind_ext::PARAMETER {
            let param = self.ctx.arena.get_parameter(decl_node)?;
            return param
                .type_annotation
                .is_some()
                .then_some(param.type_annotation);
        }
        if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let parent = self.ctx.arena.get_extended(decl)?.parent;
            let parent_node = self.ctx.arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::PARAMETER {
                let param = self.ctx.arena.get_parameter(parent_node)?;
                return (param.name == decl && param.type_annotation.is_some())
                    .then_some(param.type_annotation);
            }
            if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                let var_decl = self.ctx.arena.get_variable_declaration(parent_node)?;
                return (var_decl.name == decl && var_decl.type_annotation.is_some())
                    .then_some(var_decl.type_annotation);
            }
        }
        None
    }

    pub(super) fn is_direct_typeof_annotation_for_symbol(
        &self,
        annotation_idx: NodeIndex,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        let Some(annotation_node) = self.ctx.arena.get(annotation_idx) else {
            return false;
        };
        if annotation_node.kind != syntax_kind_ext::TYPE_QUERY {
            return false;
        }
        let Some(type_query) = self.ctx.arena.get_type_query(annotation_node) else {
            return false;
        };
        self.ctx
            .binder
            .get_node_symbol(type_query.expr_name)
            .or_else(|| {
                self.ctx
                    .binder
                    .resolve_identifier(self.ctx.arena, type_query.expr_name)
            })
            == Some(sym_id)
    }

    /// Resolve a value symbol from a node index (`file_locals` + libs, with enum flags).
    ///
    /// Extended variant used by `compute_type` fallback and `mapped_type` resolvers
    /// that also checks `lib_contexts` and includes `REGULAR_ENUM/CONST_ENUM` flags.
    pub(crate) fn resolve_value_symbol_with_libs(&self, node_idx: NodeIndex) -> Option<u32> {
        use tsz_binder::symbol_flags;

        let ident = self.ctx.arena.get_identifier_at(node_idx)?;
        let name = ident.escaped_text.as_str();

        if let Some(sym_id) = self.ctx.binder.file_locals.get(name)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && (symbol.flags
                & (symbol_flags::VALUE
                    | symbol_flags::ALIAS
                    | symbol_flags::REGULAR_ENUM
                    | symbol_flags::CONST_ENUM))
                != 0
        {
            return Some(sym_id.0);
        }

        for lib_ctx in self.ctx.lib_contexts.iter() {
            if let Some(lib_sym_id) = lib_ctx.binder.file_locals.get(name)
                && let Some(symbol) = lib_ctx.binder.get_symbol(lib_sym_id)
                && (symbol.flags
                    & (symbol_flags::VALUE
                        | symbol_flags::ALIAS
                        | symbol_flags::REGULAR_ENUM
                        | symbol_flags::CONST_ENUM))
                    != 0
            {
                self.ctx
                    .register_symbol_file_target(lib_sym_id, symbol.decl_file_idx as usize);
                return Some(lib_sym_id.0);
            }
        }

        None
    }
}
