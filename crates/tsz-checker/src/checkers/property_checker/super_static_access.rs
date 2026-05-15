use crate::classes_domain::class_summary::ClassMemberKind;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    pub(super) fn super_non_method_access_requires_es5_diagnostic(
        &mut self,
        object_expr: NodeIndex,
        class_idx: NodeIndex,
        property_name: &str,
        is_static: bool,
    ) -> bool {
        self.is_super_expression(object_expr)
            && !self.ctx.compiler_options.target.supports_es2015()
            && (matches!(
                self.class_chain_member_kind_name_only(class_idx, property_name, is_static, true)
                    .map(|(kind, _)| kind),
                Some(ClassMemberKind::FieldLike)
            ) || self.class_chain_has_accessor_member(class_idx, property_name, is_static))
    }

    pub(super) fn super_static_block_reads_base_expando(
        &mut self,
        class_idx: NodeIndex,
        property_name: &str,
    ) -> bool {
        if self.class_chain_declares_static_member(class_idx, property_name) {
            return false;
        }

        let base_name = self.get_class_name_from_decl(class_idx);
        if base_name.is_empty() {
            return false;
        }

        let Some(source_file) = self.ctx.arena.source_files.first() else {
            return false;
        };

        source_file
            .statements
            .nodes
            .iter()
            .copied()
            .any(|stmt_idx| {
                let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                    return false;
                };
                let Some(expr_stmt) = self.ctx.arena.get_expression_statement(stmt_node) else {
                    return false;
                };
                let Some(expr_node) = self.ctx.arena.get(expr_stmt.expression) else {
                    return false;
                };
                let Some(binary) = self.ctx.arena.get_binary_expr(expr_node) else {
                    return false;
                };
                if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                    return false;
                }

                let Some(lhs_node) = self.ctx.arena.get(binary.left) else {
                    return false;
                };
                let Some(access) = self.ctx.arena.get_access_expr(lhs_node) else {
                    return false;
                };
                let Some(base_node) = self.ctx.arena.get(access.expression) else {
                    return false;
                };
                let Some(base_ident) = self.ctx.arena.get_identifier(base_node) else {
                    return false;
                };
                if base_ident.escaped_text != base_name {
                    return false;
                }

                self.ctx
                    .arena
                    .get(access.name_or_argument)
                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                    .is_some_and(|name_ident| name_ident.escaped_text == property_name)
            })
    }

    fn class_chain_declares_static_member(
        &self,
        class_idx: NodeIndex,
        property_name: &str,
    ) -> bool {
        use rustc_hash::FxHashSet;

        let mut current = class_idx;
        let mut visited: FxHashSet<NodeIndex> = FxHashSet::default();

        while visited.insert(current) {
            if self.class_declares_static_member(current, property_name) {
                return true;
            }

            let Some(base_idx) = self.get_base_class_idx(current) else {
                break;
            };
            current = base_idx;
        }

        false
    }

    fn class_chain_has_accessor_member(
        &mut self,
        class_idx: NodeIndex,
        property_name: &str,
        is_static: bool,
    ) -> bool {
        use rustc_hash::FxHashSet;

        let mut current = Some(class_idx);
        let mut visited = FxHashSet::default();
        while let Some(current_idx) = current {
            if !visited.insert(current_idx) {
                break;
            }
            let Some(class_data) = self.ctx.arena.get_class_at(current_idx) else {
                break;
            };
            for &member_idx in &class_data.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind != syntax_kind_ext::GET_ACCESSOR
                    && member_node.kind != syntax_kind_ext::SET_ACCESSOR
                {
                    continue;
                }
                let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                    continue;
                };
                if self.has_static_modifier(&accessor.modifiers) != is_static {
                    continue;
                }
                if self
                    .get_property_name(accessor.name)
                    .is_some_and(|name| name == property_name)
                {
                    return true;
                }
            }
            current = self.get_base_class_idx(current_idx);
        }
        false
    }

    fn class_declares_static_member(&self, class_idx: NodeIndex, property_name: &str) -> bool {
        let Some(class_node) = self.ctx.arena.get(class_idx) else {
            return false;
        };
        let Some(class_data) = self.ctx.arena.get_class(class_node) else {
            return false;
        };

        class_data.members.nodes.iter().copied().any(|member_idx| {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                return false;
            };

            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                    .ctx
                    .arena
                    .get_property_decl(member_node)
                    .is_some_and(|prop| {
                        self.has_static_modifier(&prop.modifiers)
                            && self
                                .get_property_name(prop.name)
                                .is_some_and(|name| name == property_name)
                    }),
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(member_node)
                    .is_some_and(|method| {
                        self.has_static_modifier(&method.modifiers)
                            && self
                                .get_property_name(method.name)
                                .is_some_and(|name| name == property_name)
                    }),
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    self.ctx
                        .arena
                        .get_accessor(member_node)
                        .is_some_and(|accessor| {
                            self.has_static_modifier(&accessor.modifiers)
                                && self
                                    .get_property_name(accessor.name)
                                    .is_some_and(|name| name == property_name)
                        })
                }
                _ => false,
            }
        })
    }
}
