//! `BuiltinIteratorReturn` intrinsic-alias detection helpers.

use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) const fn builtin_iterator_return_intrinsic_type(&self) -> TypeId {
        if self.ctx.compiler_options.strict_builtin_iterator_return {
            TypeId::UNDEFINED
        } else {
            TypeId::ANY
        }
    }

    pub(crate) fn is_compiler_builtin_iterator_return_alias(
        &self,
        sym_id: SymbolId,
        declarations: &[NodeIndex],
    ) -> bool {
        declarations.iter().copied().any(|decl_idx| {
            self.is_actual_lib_builtin_iterator_return_alias_declaration(sym_id, decl_idx)
        })
    }

    fn is_actual_lib_builtin_iterator_return_alias_declaration(
        &self,
        sym_id: SymbolId,
        decl_idx: NodeIndex,
    ) -> bool {
        if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
            return arenas.iter().any(|arena| {
                (self.arena_is_actual_lib(arena) || Self::arena_is_es2015_iterable_lib(arena))
                    && Self::type_alias_declaration_is_builtin_iterator_return_intrinsic(
                        arena, decl_idx,
                    )
            });
        }

        self.ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .is_some_and(|arena| {
                (self.arena_is_actual_lib(arena) || Self::arena_is_es2015_iterable_lib(arena))
                    && Self::type_alias_declaration_is_builtin_iterator_return_intrinsic(
                        arena, decl_idx,
                    )
            })
    }

    fn arena_is_actual_lib(&self, arena: &NodeArena) -> bool {
        self.ctx
            .lib_contexts
            .iter()
            .take(self.ctx.actual_lib_file_count)
            .any(|lib_ctx| std::ptr::eq(lib_ctx.arena.as_ref(), arena))
    }

    fn arena_is_es2015_iterable_lib(arena: &NodeArena) -> bool {
        arena.source_files.first().is_some_and(|source_file| {
            let normalized = source_file.file_name.replace('\\', "/");
            let basename = normalized.rsplit('/').next().unwrap_or(&normalized);
            basename == "lib.es2015.iterable.d.ts" || basename == "es2015.iterable.d.ts"
        })
    }

    fn type_alias_declaration_is_builtin_iterator_return_intrinsic(
        arena: &NodeArena,
        decl_idx: NodeIndex,
    ) -> bool {
        let Some(node) = arena.get(decl_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
            return false;
        }
        let Some(type_alias) = arena.get_type_alias(node) else {
            return false;
        };
        let alias_name = arena
            .get(type_alias.name)
            .and_then(|node| arena.get_identifier(node))
            .map(|ident| ident.escaped_text.as_str());
        if alias_name != Some("BuiltinIteratorReturn") {
            return false;
        }

        Self::type_node_is_bare_intrinsic_in_arena(arena, type_alias.type_node)
    }

    fn type_node_is_bare_intrinsic_in_arena(arena: &NodeArena, type_idx: NodeIndex) -> bool {
        let Some(node) = arena.get(type_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = arena.get_type_ref(node) else {
            return false;
        };
        if type_ref.type_arguments.is_some() {
            return false;
        }
        let Some(name_node) = arena.get(type_ref.type_name) else {
            return false;
        };
        let Some(ident) = arena.get_identifier(name_node) else {
            return false;
        };
        if ident.escaped_text != "intrinsic" {
            return false;
        }
        !Self::type_node_is_parenthesized(arena, type_idx)
    }

    fn type_node_is_parenthesized(arena: &NodeArena, type_idx: NodeIndex) -> bool {
        let Some(parent_idx) = arena.parent_of(type_idx) else {
            return false;
        };
        let Some(parent_node) = arena.get(parent_idx) else {
            return false;
        };
        parent_node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            && arena
                .get_wrapped_type(parent_node)
                .is_some_and(|wrapped| wrapped.type_node == type_idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_parser::parser::ParserState;

    fn first_statement(source: &str) -> (NodeArena, NodeIndex) {
        let mut parser =
            ParserState::new("lib.es2015.iterable.d.ts".to_string(), source.to_string());
        let source_file_idx = parser.parse_source_file();
        let arena = parser.get_arena().clone();
        let source_file = arena
            .get(source_file_idx)
            .and_then(|node| arena.get_source_file(node))
            .expect("source should parse as a source file");
        let stmt_idx = source_file
            .statements
            .nodes
            .first()
            .copied()
            .expect("source should contain a statement");
        (arena, stmt_idx)
    }

    #[test]
    fn bare_intrinsic_alias_body_is_builtin_iterator_return_intrinsic() {
        let (arena, alias_idx) = first_statement("type BuiltinIteratorReturn = intrinsic;");

        assert!(
            CheckerState::type_alias_declaration_is_builtin_iterator_return_intrinsic(
                &arena, alias_idx,
            ),
            "bare intrinsic alias body should be classified structurally"
        );
    }

    #[test]
    fn parenthesized_intrinsic_alias_body_is_not_builtin_iterator_return_intrinsic() {
        let (arena, alias_idx) = first_statement("type BuiltinIteratorReturn = (intrinsic);");

        assert!(
            !CheckerState::type_alias_declaration_is_builtin_iterator_return_intrinsic(
                &arena, alias_idx,
            ),
            "parenthesized intrinsic must be rejected from AST shape, not source text"
        );
    }
}
