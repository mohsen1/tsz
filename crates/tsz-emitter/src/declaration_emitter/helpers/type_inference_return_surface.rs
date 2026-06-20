//! Return-surface helpers for source call declaration inference.

use super::super::DeclarationEmitter;
use tsz_binder::symbol_flags;
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
            && self.type_contains_conditional_alias_application_for_inferred_emit(type_id)
        {
            return Some(self.print_type_id_for_inferred_declaration(type_id));
        }
        // A single, top-level conditional type-alias reference (e.g. an inferred
        // `const x = f(arg): Cond<string>`) is preserved as-written rather than
        // expanded, mirroring `tsc`'s declaration emit. Expansion stays in force
        // when the conditional alias is only *nested* inside a larger synthesized
        // return type, where the alias name does not stand for the whole type.
        if self.source_type_contains_conditional_alias_application(source_arena, type_annotation, 0)
        {
            if self.return_annotation_is_preservable_local_conditional_alias(
                source_arena,
                type_annotation,
            ) {
                return Some(type_text.to_string());
            }
            if let Some(type_id) = self.get_node_type_or_names(&[expr_idx]) {
                return Some(self.print_type_id_expanded_for_inferred_declaration(type_id));
            }
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
                    Self::type_node_is_conditional_after_parens(source_arena, alias_type, depth + 1)
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

    /// Returns true when `type_annotation` is exactly a single top-level
    /// reference to a conditional-bodied type alias that is declared **and
    /// exported** in the current emit file. In that situation the alias name is
    /// itself emitted into the `.d.ts`, so the inferred declaration can name the
    /// alias application (e.g. `Cond<string>`) instead of expanding its
    /// conditional body — matching `tsc`'s "recursive conditional alias
    /// preserved" declaration emit.
    ///
    /// The check intentionally bails out when:
    /// - the callee is not declared in the current file (cross-file aliases are
    ///   handled by the import-aware type-id path), or
    /// - the conditional alias is only nested inside a larger return type, or
    /// - the alias is not exported (so the `.d.ts` would not contain a
    ///   declaration the reference could resolve to).
    pub(in crate::declaration_emitter) fn return_annotation_is_preservable_local_conditional_alias(
        &self,
        source_arena: &NodeArena,
        type_annotation: NodeIndex,
    ) -> bool {
        if !std::ptr::eq(source_arena, self.arena) {
            return false;
        }
        let Some(binder) = self.binder else {
            return false;
        };
        let Some(top_reference) =
            self.single_top_level_type_reference(source_arena, type_annotation)
        else {
            return false;
        };
        let Some(sym_id) = self.declaration_type_symbol_from_type_node(source_arena, top_reference)
        else {
            return false;
        };
        let Some(symbol) = binder.symbols.get(sym_id) else {
            return false;
        };
        // The alias must be an exported type alias: declaration emit only emits a
        // (re-usable) `type Alias = …` for exported aliases, so an inferred
        // reference can resolve against it. Non-exported aliases stay expanded.
        if symbol.flags & symbol_flags::TYPE_ALIAS == 0
            || !(symbol.is_exported || symbol.has_any_flags(symbol_flags::EXPORT_VALUE))
        {
            return false;
        }
        // Only conditional-bodied aliases are mis-expanded today; non-conditional
        // alias applications already round-trip through the normal reuse path.
        self.with_symbol_declarations(sym_id, |alias_arena, decl_idx| {
            let decl_node = alias_arena.get(decl_idx)?;
            if decl_node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                return None;
            }
            let alias = alias_arena.get_type_alias(decl_node)?;
            Some(Self::type_node_is_conditional_after_parens(
                alias_arena,
                alias.type_node,
                0,
            ))
        })
        .unwrap_or(false)
    }

    /// Strips enclosing parentheses and returns the node index when
    /// `type_idx` is exactly one top-level `TYPE_REFERENCE`. Returns `None` for
    /// composite types (unions, intersections, object literals, function types,
    /// …) where a contained alias reference does not name the whole type.
    fn single_top_level_type_reference(
        &self,
        source_arena: &NodeArena,
        type_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        // Skip enclosing parentheses; the bound guards against a malformed arena
        // with a parenthesis cycle.
        let mut type_idx = type_idx;
        for _ in 0..=16 {
            let type_node = source_arena.get(type_idx)?;
            if type_node.kind != syntax_kind_ext::PARENTHESIZED_TYPE {
                return (type_node.kind == syntax_kind_ext::TYPE_REFERENCE).then_some(type_idx);
            }
            type_idx = source_arena.get_wrapped_type(type_node)?.type_node;
        }
        None
    }

    fn type_node_is_conditional_after_parens(
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
            return Self::type_node_is_conditional_after_parens(
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
            Some(Self::type_node_is_conditional_after_parens(
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

    fn first_function_return_annotation_is_preservable(source: &str) -> bool {
        use crate::type_cache_view::TypeCacheView;
        use tsz_binder::BinderState;
        use tsz_solver::construction::TypeInterner;

        let mut parser = ParserState::new("preserve.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(&parser.arena, root);
        let interner = TypeInterner::new();
        let type_cache = TypeCacheView::default();
        let emitter =
            DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);

        let annotation = parser
            .arena
            .nodes
            .iter()
            .find_map(|node| {
                if node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
                    return None;
                }
                let func = parser.arena.get_function(node)?;
                func.type_annotation
                    .is_some()
                    .then_some(func.type_annotation)
            })
            .expect("function return annotation");

        emitter.return_annotation_is_preservable_local_conditional_alias(&parser.arena, annotation)
    }

    #[test]
    fn exported_single_conditional_alias_reference_is_preservable() {
        assert!(first_function_return_annotation_is_preservable(
            r#"
export type Cond<T> = T extends string ? { s: T } : { n: T };
declare function make<T>(t: T): Cond<T>;
"#,
        ));
    }

    #[test]
    fn unexported_conditional_alias_reference_is_not_preservable() {
        assert!(!first_function_return_annotation_is_preservable(
            r#"
type Cond<T> = T extends string ? { s: T } : { n: T };
declare function make<T>(t: T): Cond<T>;
"#,
        ));
    }

    #[test]
    fn exported_non_conditional_alias_reference_is_not_preservable() {
        // Non-conditional alias applications already round-trip through the
        // normal reuse path, so they are intentionally excluded.
        assert!(!first_function_return_annotation_is_preservable(
            r#"
export type Plain<T> = { value: T };
declare function make<T>(t: T): Plain<T>;
"#,
        ));
    }

    #[test]
    fn nested_conditional_alias_reference_is_not_preservable() {
        // The conditional alias is not the *whole* return type, so the alias name
        // does not stand for the inferred type and must not be preserved here.
        assert!(!first_function_return_annotation_is_preservable(
            r#"
export type Cond<T> = T extends string ? { s: T } : { n: T };
declare function wrap<T>(t: T): { value: Cond<T> };
"#,
        ));
    }
}
