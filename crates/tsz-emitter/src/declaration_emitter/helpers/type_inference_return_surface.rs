//! Return-surface helpers for source call declaration inference.

use super::super::DeclarationEmitter;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn call_expression_declared_return_surface_text(
        &self,
        expr_idx: NodeIndex,
        source_arena: &NodeArena,
        type_annotation: NodeIndex,
        type_text: &str,
        explicit_type_args: &[String],
        has_call_site_type_param_substitutions: bool,
    ) -> Option<String> {
        if Self::leading_type_reference_name(type_text)
            .is_some_and(Self::is_builtin_conditional_utility_type_name)
            && let Some(type_id) = self.get_node_type_or_names(&[expr_idx])
        {
            return Some(self.print_type_id_expanded_for_inferred_declaration(type_id));
        }
        if let Some(type_id) = self.get_node_type_or_names(&[expr_idx])
            && self.type_contains_conditional_alias_application_for_inferred_emit(type_id, 0)
        {
            return Some(self.print_type_id_for_inferred_declaration(type_id));
        }
        if self.source_type_contains_conditional_alias_application(source_arena, type_annotation, 0)
            && let Some(type_id) = self.get_node_type_or_names(&[expr_idx])
        {
            return Some(self.print_type_id_expanded_for_inferred_declaration(type_id));
        }
        if explicit_type_args.is_empty()
            && !has_call_site_type_param_substitutions
            && std::ptr::eq(source_arena, self.arena)
            && let Some(type_id) = self.get_node_type_or_names(&[type_annotation])
            && let Some(surface) = self.inferred_declaration_mapped_constraint_surface(type_id)
        {
            return Some(self.print_type_id_for_inferred_declaration(surface));
        }
        None
    }

    pub(in crate::declaration_emitter) fn source_type_contains_conditional_alias_application(
        &self,
        source_arena: &NodeArena,
        type_idx: NodeIndex,
        depth: u8,
    ) -> bool {
        if depth > 32 {
            return false;
        }
        let Some(type_node) = source_arena.get(type_idx) else {
            return false;
        };
        if type_node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = source_arena.get_type_ref(type_node)
            && let Some(alias_name) =
                self.return_surface_type_reference_name_text(source_arena, type_ref.type_name)
            && (self
                .find_type_alias_type_node_in_arena(source_arena, &alias_name)
                .is_some_and(|alias_type| {
                    self.type_node_is_conditional_after_parens(source_arena, alias_type, depth + 1)
                })
                || self.type_reference_resolves_to_conditional_alias(
                    source_arena,
                    type_idx,
                    depth + 1,
                ))
        {
            return true;
        }
        source_arena
            .get_children(type_idx)
            .into_iter()
            .any(|child_idx| {
                self.source_type_contains_conditional_alias_application(
                    source_arena,
                    child_idx,
                    depth + 1,
                )
            })
    }

    fn type_node_is_conditional_after_parens(
        &self,
        source_arena: &NodeArena,
        type_idx: NodeIndex,
        depth: u8,
    ) -> bool {
        if depth > 32 {
            return false;
        }
        let Some(type_node) = source_arena.get(type_idx) else {
            return false;
        };
        if type_node.kind == syntax_kind_ext::CONDITIONAL_TYPE {
            return true;
        }
        if type_node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            && let Some(wrapped) = source_arena.get_wrapped_type(type_node)
        {
            return self.type_node_is_conditional_after_parens(
                source_arena,
                wrapped.type_node,
                depth + 1,
            );
        }
        false
    }

    fn type_reference_resolves_to_conditional_alias(
        &self,
        source_arena: &NodeArena,
        type_idx: NodeIndex,
        depth: u8,
    ) -> bool {
        if depth > 32 {
            return false;
        }
        let Some(sym_id) = self.declaration_type_symbol_from_type_node(source_arena, type_idx)
        else {
            return false;
        };
        self.with_symbol_declarations(sym_id, |alias_arena, decl_idx| {
            let decl_node = alias_arena.get(decl_idx)?;
            let alias = alias_arena.get_type_alias(decl_node)?;
            Some(self.type_node_is_conditional_after_parens(
                alias_arena,
                alias.type_node,
                depth + 1,
            ))
        })
        .unwrap_or(false)
    }

    fn return_surface_type_reference_name_text(
        &self,
        source_arena: &NodeArena,
        name_idx: NodeIndex,
    ) -> Option<String> {
        let name_node = source_arena.get(name_idx)?;
        if name_node.kind == SyntaxKind::Identifier as u16 {
            return self.identifier_text_from_arena(source_arena, name_idx);
        }
        if name_node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qualified = source_arena.get_qualified_name(name_node)?;
            return self.identifier_text_from_arena(source_arena, qualified.right);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_parser::parser::ParserState;

    #[test]
    fn source_type_detects_nested_conditional_alias_application() {
        let mut parser = ParserState::new(
            "return-surface.ts".to_string(),
            r#"
type Next<T, Fn> = Fn extends (value: T) => unknown ? (value: T) => ReturnType<Fn> : never;
interface Box<T> {
    pipe<Fn extends (value: T) => unknown>(fn: Fn): Box<Next<T, Fn>>;
}
"#
            .to_string(),
        );
        parser.parse_source_file();
        let emitter = DeclarationEmitter::new(&parser.arena);
        let return_type = parser
            .arena
            .nodes
            .iter()
            .enumerate()
            .find_map(|(idx, node)| {
                (node.kind == syntax_kind_ext::TYPE_REFERENCE)
                    .then_some(NodeIndex(idx as u32))
                    .filter(|&idx| {
                        emitter
                            .source_slice_from_arena(&parser.arena, idx)
                            .is_some_and(|text| text.trim() == "Box<Next<T, Fn>>")
                    })
            })
            .expect("method return type");

        assert!(emitter.source_type_contains_conditional_alias_application(
            &parser.arena,
            return_type,
            0
        ));
    }
}
