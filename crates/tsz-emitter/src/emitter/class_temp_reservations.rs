use super::{NodeIndex, Printer, ScriptTarget};
use std::collections::VecDeque;
use tsz_parser::parser::NodeList;
use tsz_parser::parser::node::ClassData;
use tsz_parser::syntax::transform_utils::{
    contains_async_arrow_function, contains_super_reference, contains_this_reference,
};

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn prepare_file_level_class_temp_reservations(
        &mut self,
        statements: &NodeList,
    ) {
        self.file_level_class_temp_reservation_plan.clear();
        self.file_level_class_temp_reservations.clear();
        self.completed_file_level_class_temp_reservations.clear();

        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != super::syntax_kind_ext::CLASS_DECLARATION {
                continue;
            }
            let Some(class) = self.arena.get_class(stmt_node) else {
                continue;
            };
            let count = self.estimate_file_level_class_temp_count(class);
            if count > 0 {
                self.file_level_class_temp_reservation_plan
                    .push((stmt_idx, count));
            }
        }
    }

    pub(super) fn reserve_pending_file_level_class_temps(&mut self) {
        let plan = self.file_level_class_temp_reservation_plan.clone();
        for (class_idx, count) in plan {
            if self
                .completed_file_level_class_temp_reservations
                .contains(&class_idx)
                || self
                    .file_level_class_temp_reservations
                    .contains_key(&class_idx)
            {
                continue;
            }

            let mut names = VecDeque::new();
            for _ in 0..count {
                names.push_back(self.make_unique_name_reserved_for_nested());
            }
            self.file_level_class_temp_reservations
                .insert(class_idx, names);
        }
    }

    pub(super) fn make_class_static_temp_name(&mut self, class_idx: NodeIndex) -> String {
        if let Some(names) = self.file_level_class_temp_reservations.get_mut(&class_idx)
            && let Some(name) = names.pop_front()
        {
            return name;
        }

        self.make_unique_name_reserved_for_nested()
    }

    pub(super) fn make_class_static_temp_name_hoisted(&mut self, class_idx: NodeIndex) -> String {
        let name = self.make_class_static_temp_name(class_idx);
        self.hoisted_assignment_temps.push(name.clone());
        name
    }

    pub(super) fn finish_file_level_class_temp_reservation(&mut self, class_idx: NodeIndex) {
        if self
            .file_level_class_temp_reservation_plan
            .iter()
            .any(|(idx, _)| *idx == class_idx)
        {
            self.completed_file_level_class_temp_reservations
                .insert(class_idx);
            self.file_level_class_temp_reservations.remove(&class_idx);
        }
    }

    fn estimate_file_level_class_temp_count(&self, class: &ClassData) -> usize {
        if (self.ctx.options.target as u32) >= (ScriptTarget::ES2022 as u32) {
            return 0;
        }

        let static_initializer_nodes = self.class_static_initializer_nodes_for_temp_plan(class);
        if static_initializer_nodes.is_empty() {
            return 0;
        }

        let needs_class_reference = static_initializer_nodes.iter().any(|idx| {
            contains_this_reference(self.arena, *idx)
                || contains_async_arrow_function(self.arena, *idx)
        });
        let needs_super_reference = self.class_has_non_null_extends(class)
            && static_initializer_nodes
                .iter()
                .any(|idx| contains_super_reference(self.arena, *idx));
        let externalized_static_initializers = self.ctx.options.legacy_decorators
            && !self.collect_class_decorators(&class.modifiers).is_empty();

        usize::from(needs_class_reference)
            + usize::from(needs_super_reference && !externalized_static_initializers)
    }

    fn class_static_initializer_nodes_for_temp_plan(&self, class: &ClassData) -> Vec<NodeIndex> {
        let mut nodes = Vec::new();
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == super::syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                nodes.push(member_idx);
                continue;
            }
            if member_node.kind != super::syntax_kind_ext::PROPERTY_DECLARATION {
                continue;
            }
            let Some(prop) = self.arena.get_property_decl(member_node) else {
                continue;
            };
            if !self.arena.is_static(&prop.modifiers)
                || self
                    .arena
                    .has_modifier(&prop.modifiers, super::SyntaxKind::AbstractKeyword)
                || self
                    .arena
                    .has_modifier(&prop.modifiers, super::SyntaxKind::DeclareKeyword)
                || prop.initializer.is_none()
            {
                continue;
            }
            nodes.push(prop.initializer);
        }
        nodes
    }

    fn class_has_non_null_extends(&self, class: &ClassData) -> bool {
        class.heritage_clauses.as_ref().is_some_and(|heritage| {
            !crate::transforms::emit_utils::extends_null_literal(
                self.arena,
                &class.heritage_clauses,
            ) && heritage.nodes.iter().any(|&clause_idx| {
                self.arena
                    .get(clause_idx)
                    .and_then(|node| self.arena.get_heritage(node))
                    .is_some_and(|clause| {
                        clause.token == super::SyntaxKind::ExtendsKeyword as u16
                            && !clause.types.nodes.is_empty()
                    })
            })
        })
    }
}
