//! Global augmentation helpers for duplicate identifier checking.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    pub(super) fn check_global_augmentation_const_enum_rebind_diagnostics(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        if !self.ctx.binder.is_external_module() {
            return;
        }

        let Some(stmt_count) = self
            .ctx
            .arena
            .source_files
            .first()
            .map(|source_file| source_file.statements.nodes.len())
        else {
            return;
        };

        for stmt_i in 0..stmt_count {
            let Some(stmt_idx) = self
                .ctx
                .arena
                .source_files
                .first()
                .and_then(|source_file| source_file.statements.nodes.get(stmt_i).copied())
            else {
                break;
            };
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }
            if !stmt_node.is_global_augmentation()
                && self
                    .ctx
                    .arena
                    .get_module(stmt_node)
                    .and_then(|module| self.ctx.arena.get(module.name))
                    .and_then(|name| self.ctx.arena.get_identifier(name))
                    .is_none_or(|ident| ident.escaped_text != "global")
            {
                continue;
            }

            let Some(inner_count) = self
                .global_augmentation_body_statements(stmt_idx)
                .map(<[NodeIndex]>::len)
            else {
                continue;
            };

            for inner_i in 0..inner_count {
                let Some(enum_decl_idx) = self
                    .global_augmentation_body_statements(stmt_idx)
                    .and_then(|inner| inner.get(inner_i).copied())
                else {
                    break;
                };
                let Some(enum_node) = self.ctx.arena.get(enum_decl_idx) else {
                    continue;
                };
                if enum_node.kind != syntax_kind_ext::ENUM_DECLARATION {
                    continue;
                }
                let Some(enum_decl) = self.ctx.arena.get_enum(enum_node) else {
                    continue;
                };
                if !self
                    .ctx
                    .arena
                    .has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword)
                {
                    continue;
                }

                for &member_idx in &enum_decl.members.nodes {
                    let Some(member_node) = self.ctx.arena.get(member_idx) else {
                        continue;
                    };
                    let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
                        continue;
                    };
                    let Some(member_name) = self.ctx.arena.get(member.name).and_then(|name_node| {
                        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                            Some(ident.escaped_text.clone())
                        } else {
                            self.ctx
                                .arena
                                .get_literal(name_node)
                                .map(|literal| literal.text.clone())
                        }
                    }) else {
                        continue;
                    };
                    self.error_at_node_msg(
                        member.name,
                        diagnostic_codes::DUPLICATE_IDENTIFIER,
                        &[&member_name],
                    );
                }

                if let Some(&first_member_idx) = enum_decl.members.nodes.first()
                    && let Some(first_member_node) = self.ctx.arena.get(first_member_idx)
                    && let Some(first_member) = self.ctx.arena.get_enum_member(first_member_node)
                    && first_member.initializer.is_none()
                {
                    self.error_at_node(
                        first_member.name,
                        diagnostic_messages::IN_AN_ENUM_WITH_MULTIPLE_DECLARATIONS_ONLY_ONE_DECLARATION_CAN_OMIT_AN_INITIALIZ,
                        diagnostic_codes::IN_AN_ENUM_WITH_MULTIPLE_DECLARATIONS_ONLY_ONE_DECLARATION_CAN_OMIT_AN_INITIALIZ,
                    );
                }
            }
        }
    }

    pub(super) fn global_augmentation_body_statements(
        &self,
        module_decl_idx: NodeIndex,
    ) -> Option<&[NodeIndex]> {
        let stmt_node = self.ctx.arena.get(module_decl_idx)?;
        let module = self.ctx.arena.get_module(stmt_node)?;
        let body_node = self.ctx.arena.get(module.body)?;
        if body_node.kind != syntax_kind_ext::MODULE_BLOCK {
            return None;
        }
        let block = self.ctx.arena.get_module_block(body_node)?;
        let statements = block.statements.as_ref()?;
        Some(&statements.nodes)
    }
}
