use super::{NodeIndex, Printer, ScriptTarget};
use std::collections::VecDeque;
use tsz_parser::parser::NodeList;
use tsz_parser::parser::node::ClassData;
use tsz_parser::parser::node::NodeAccess;
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
            self.collect_file_level_class_temp_reservations(stmt_idx);
        }
    }

    fn collect_file_level_class_temp_reservations(&mut self, idx: NodeIndex) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if node.kind == super::syntax_kind_ext::CLASS_DECLARATION
            || node.kind == super::syntax_kind_ext::CLASS_EXPRESSION
        {
            if let Some(class) = self.arena.get_class(node) {
                let count = self.estimate_file_level_class_temp_count(idx, class);
                if count > 0 {
                    self.file_level_class_temp_reservation_plan
                        .push((idx, count));
                }
            }
            return;
        }

        if matches!(
            node.kind,
            k if k == super::syntax_kind_ext::FUNCTION_DECLARATION
                || k == super::syntax_kind_ext::FUNCTION_EXPRESSION
                || k == super::syntax_kind_ext::ARROW_FUNCTION
                || k == super::syntax_kind_ext::METHOD_DECLARATION
                || k == super::syntax_kind_ext::CONSTRUCTOR
                || k == super::syntax_kind_ext::GET_ACCESSOR
                || k == super::syntax_kind_ext::SET_ACCESSOR
        ) {
            return;
        }

        for child in self.arena.get_children(idx) {
            self.collect_file_level_class_temp_reservations(child);
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
                let name = self.make_unique_name_reserved_for_nested();
                if !self
                    .hoisted_file_level_class_temps
                    .iter()
                    .any(|temp| temp == &name)
                {
                    self.hoisted_file_level_class_temps.push(name.clone());
                }
                names.push_back(name);
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

        // Honour pre-allocated names reserved by reserve_es5_computed_key_inner_class_temps
        // so that the class alias gets the same slot that was counted before the key temp.
        if let Some(name) = self.preallocated_temp_names.pop_front() {
            return name;
        }

        self.make_unique_name_reserved_for_nested()
    }

    pub(super) fn make_class_static_temp_name_hoisted(&mut self, class_idx: NodeIndex) -> String {
        let name = self.make_class_static_temp_name(class_idx);
        if !self
            .hoisted_assignment_temps
            .iter()
            .any(|temp| temp == &name)
            && !self
                .hoisted_file_level_class_temps
                .iter()
                .any(|temp| temp == &name)
        {
            self.hoisted_assignment_temps.push(name.clone());
        }
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

    fn estimate_file_level_class_temp_count(
        &self,
        class_idx: NodeIndex,
        class: &ClassData,
    ) -> usize {
        if (self.ctx.options.target as u32) >= (ScriptTarget::ES2022 as u32) {
            return 0;
        }

        if (self.ctx.options.target as u32) < (ScriptTarget::ES2015 as u32)
            && self
                .arena
                .get(class_idx)
                .is_some_and(|node| node.kind == super::syntax_kind_ext::CLASS_DECLARATION)
        {
            return 0;
        }

        if !self.ctx.options.legacy_decorators && self.class_has_decorators(class) {
            return 0;
        }

        let legacy_computed_temps =
            self.estimate_legacy_decorator_computed_prefix_temp_count(class_idx, class);
        let static_initializer_nodes = self.class_static_initializer_nodes_for_temp_plan(class);
        if static_initializer_nodes.is_empty() {
            return legacy_computed_temps;
        }

        let externalized_static_initializers = self.ctx.options.legacy_decorators
            && !self.collect_class_decorators(&class.modifiers).is_empty();
        let needs_class_reference = !externalized_static_initializers
            && static_initializer_nodes.iter().any(|idx| {
                contains_this_reference(self.arena, *idx)
                    || contains_async_arrow_function(self.arena, *idx)
            });
        let needs_super_reference = !externalized_static_initializers
            && self.class_has_non_null_extends(class)
            && static_initializer_nodes
                .iter()
                .any(|idx| contains_super_reference(self.arena, *idx));

        legacy_computed_temps
            + usize::from(needs_class_reference)
            + usize::from(needs_super_reference)
    }

    fn estimate_legacy_decorator_computed_prefix_temp_count(
        &self,
        class_idx: NodeIndex,
        class: &ClassData,
    ) -> usize {
        if !self.ctx.options.legacy_decorators {
            return 0;
        }

        let mut count = 0;
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != super::syntax_kind_ext::PROPERTY_DECLARATION {
                continue;
            }
            let Some(prop) = self.arena.get_property_decl(member_node) else {
                continue;
            };
            if self.legacy_decorator_computed_property_has_base_temp(prop) {
                count += 1;
            }
        }

        if self
            .arena
            .get(class_idx)
            .is_some_and(|node| node.kind == super::syntax_kind_ext::CLASS_EXPRESSION)
            && self.legacy_class_expression_needs_computed_wrapper_temp(class)
        {
            count += 1;
        }

        count
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

    fn legacy_decorator_computed_property_has_base_temp(
        &self,
        prop: &tsz_parser::parser::node::PropertyDeclData,
    ) -> bool {
        if !self.legacy_computed_name_needs_temp(prop.name) {
            return false;
        }
        if !self.collect_class_decorators(&prop.modifiers).is_empty() {
            return false;
        }
        !self.legacy_computed_property_is_erased(prop)
    }

    fn legacy_class_expression_needs_computed_wrapper_temp(&self, class: &ClassData) -> bool {
        let mut has_pending_entry = false;
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            match member_node.kind {
                k if k == super::syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    if self.legacy_computed_property_participates_in_schedule(prop) {
                        has_pending_entry = true;
                    }
                }
                k if k == super::syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    if method.body.is_some()
                        && has_pending_entry
                        && self.legacy_computed_name_needs_temp(method.name)
                    {
                        has_pending_entry = false;
                    }
                }
                k if k == super::syntax_kind_ext::GET_ACCESSOR
                    || k == super::syntax_kind_ext::SET_ACCESSOR =>
                {
                    let Some(accessor) = self.arena.get_accessor(member_node) else {
                        continue;
                    };
                    if has_pending_entry && self.legacy_computed_name_needs_temp(accessor.name) {
                        has_pending_entry = false;
                    }
                }
                _ => {}
            }
        }
        has_pending_entry
    }

    fn legacy_computed_property_participates_in_schedule(
        &self,
        prop: &tsz_parser::parser::node::PropertyDeclData,
    ) -> bool {
        let Some(computed_expr) = self.legacy_computed_name_expression_needing_temp(prop.name)
        else {
            return false;
        };
        let has_decorators = !self.collect_class_decorators(&prop.modifiers).is_empty();
        if !self.legacy_computed_property_is_erased(prop) {
            return true;
        }
        has_decorators || !self.is_computed_name_expr_side_effect_free(computed_expr)
    }

    fn legacy_computed_property_is_erased(
        &self,
        prop: &tsz_parser::parser::node::PropertyDeclData,
    ) -> bool {
        if self
            .arena
            .has_modifier(&prop.modifiers, super::SyntaxKind::AbstractKeyword)
            || self
                .arena
                .has_modifier(&prop.modifiers, super::SyntaxKind::DeclareKeyword)
        {
            return true;
        }

        let is_private = self
            .arena
            .get(prop.name)
            .is_some_and(|n| n.kind == super::SyntaxKind::PrivateIdentifier as u16);
        let has_accessor = self
            .arena
            .has_modifier(&prop.modifiers, super::SyntaxKind::AccessorKeyword);
        prop.initializer.is_none() && !is_private && !has_accessor
    }

    fn legacy_computed_name_needs_temp(&self, name_idx: NodeIndex) -> bool {
        self.legacy_computed_name_expression_needing_temp(name_idx)
            .is_some()
    }

    fn legacy_computed_name_expression_needing_temp(
        &self,
        name_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let name_node = self.arena.get(name_idx)?;
        if name_node.kind != super::syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }
        let computed = self.arena.get_computed_property(name_node)?;
        let expr_node = self.arena.get(computed.expression)?;
        let is_constant = expr_node.kind == super::SyntaxKind::StringLiteral as u16
            || expr_node.kind == super::SyntaxKind::NumericLiteral as u16
            || expr_node.kind == super::SyntaxKind::NoSubstitutionTemplateLiteral as u16;
        (!is_constant).then_some(computed.expression)
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
