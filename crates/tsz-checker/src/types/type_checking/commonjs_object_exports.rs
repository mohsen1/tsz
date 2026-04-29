//! CommonJS object-literal export helpers used by duplicate/conflict checks.

use crate::diagnostics::diagnostic_codes;
use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    pub(crate) fn check_target_file_commonjs_object_exports_conflicting_with_module_augmentations(
        &mut self,
    ) {
        let Some(source_file) = self.ctx.arena.source_files.first() else {
            return;
        };

        let export_names: Vec<(String, NodeIndex)> = source_file
            .statements
            .nodes
            .iter()
            .flat_map(|&stmt_idx| self.commonjs_object_literal_export_names(stmt_idx))
            .collect();

        for (export_name, name_idx) in export_names {
            let conflict_decls =
                self.module_augmentation_conflict_declarations_for_current_file(&export_name);
            // Skip when the augmentation declaration is one that legitimately
            // merges with the exported value:
            //   - FUNCTION: function/namespace merge.
            //   - INTERFACE: interface augmenting a class/interface.
            //   - NAMESPACE_MODULE / VALUE_MODULE: namespace augmenting a class
            //     for static-side members or merging with a value module.
            // Classes exported through `module.exports = { Class }` are valid
            // merge targets for these forms, so the duplicate-identifier check
            // must not fire.
            const MERGEABLE_AUGMENTATION_FLAGS: u32 = symbol_flags::FUNCTION
                | symbol_flags::INTERFACE
                | symbol_flags::NAMESPACE_MODULE
                | symbol_flags::VALUE_MODULE;
            if conflict_decls.is_empty()
                || conflict_decls
                    .iter()
                    .any(|(_, flags, _, _, _)| (*flags & MERGEABLE_AUGMENTATION_FLAGS) != 0)
            {
                continue;
            }

            self.error_at_node_msg(
                name_idx,
                diagnostic_codes::DUPLICATE_IDENTIFIER,
                &[&export_name],
            );
        }
    }

    pub(crate) fn commonjs_object_literal_export_declarations_in_file(
        &self,
        file_idx: usize,
        name: &str,
    ) -> Vec<(NodeIndex, u32, bool)> {
        let arena = self.ctx.get_arena_for_file(file_idx as u32);
        let Some(source_file) = arena.source_files.first() else {
            return Vec::new();
        };

        let mut declarations = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            let Some(rhs_expr) = self.commonjs_object_literal_export_rhs(arena, stmt_idx) else {
                continue;
            };
            let Some(rhs_node) = arena.get(rhs_expr) else {
                continue;
            };
            if rhs_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                continue;
            }
            let Some(object_lit) = arena.get_literal_expr(rhs_node) else {
                continue;
            };

            for &element_idx in &object_lit.elements.nodes {
                let Some(element_node) = arena.get(element_idx) else {
                    continue;
                };
                let name_idx = match element_node.kind {
                    syntax_kind_ext::PROPERTY_ASSIGNMENT => arena
                        .get_property_assignment(element_node)
                        .map(|prop| prop.name),
                    syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => arena
                        .get_shorthand_property(element_node)
                        .map(|prop| prop.name),
                    _ => None,
                };
                let Some(name_idx) = name_idx else {
                    continue;
                };
                let Some(member_name) =
                    crate::types_domain::queries::core::get_literal_property_name(arena, name_idx)
                else {
                    continue;
                };
                if member_name == name {
                    declarations.push((name_idx, symbol_flags::FUNCTION_SCOPED_VARIABLE, true));
                }
            }
        }

        declarations
    }

    fn commonjs_object_literal_export_names(
        &self,
        stmt_idx: NodeIndex,
    ) -> Vec<(String, NodeIndex)> {
        let Some(rhs_expr) = self.commonjs_object_literal_export_rhs(self.ctx.arena, stmt_idx)
        else {
            return Vec::new();
        };
        let Some(rhs_node) = self.ctx.arena.get(rhs_expr) else {
            return Vec::new();
        };
        if rhs_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return Vec::new();
        }
        let Some(object_lit) = self.ctx.arena.get_literal_expr(rhs_node) else {
            return Vec::new();
        };

        let mut names = Vec::new();
        for &element_idx in &object_lit.elements.nodes {
            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            let name_idx = match element_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_property_assignment(element_node)
                    .map(|prop| prop.name),
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_shorthand_property(element_node)
                    .map(|prop| prop.name),
                _ => None,
            };
            let Some(name_idx) = name_idx else {
                continue;
            };
            let Some(name) = crate::types_domain::queries::core::get_literal_property_name(
                self.ctx.arena,
                name_idx,
            ) else {
                continue;
            };
            names.push((name, name_idx));
        }

        names
    }

    fn commonjs_object_literal_export_rhs(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        stmt_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let stmt_node = arena.get(stmt_idx)?;
        if stmt_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT {
            arena.get_expression_statement(stmt_node).and_then(|stmt| {
                self.direct_commonjs_module_export_assignment_rhs(arena, stmt.expression)
            })
        } else if stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
            self.direct_commonjs_module_export_rhs_from_variable_statement(arena, stmt_idx)
        } else {
            None
        }
    }
}
