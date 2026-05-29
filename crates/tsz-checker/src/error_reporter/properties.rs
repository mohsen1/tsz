//! Property-related error reporting (TS2339, TS2741, TS2540, TS7053, TS18046).

use crate::diagnostics::diagnostic_codes;
use crate::error_reporter::fingerprint_policy::{DiagnosticAnchorKind, DiagnosticRenderRequest};
use crate::error_reporter::type_display_policy::DiagnosticTypeDisplayRole;
use crate::query_boundaries::common as query;
use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn property_type_has_array_like_length(&self, type_id: TypeId) -> bool {
        let kind = crate::query_boundaries::type_checking_utilities::classify_array_like(
            self.ctx.types,
            type_id,
        );
        match kind {
            crate::query_boundaries::type_checking_utilities::ArrayLikeKind::Array(_)
            | crate::query_boundaries::type_checking_utilities::ArrayLikeKind::Tuple => true,
            crate::query_boundaries::type_checking_utilities::ArrayLikeKind::Readonly(inner) => {
                self.property_type_has_array_like_length(inner)
            }
            crate::query_boundaries::type_checking_utilities::ArrayLikeKind::Union(members) => {
                !members.is_empty()
                    && members
                        .iter()
                        .all(|&member| self.property_type_has_array_like_length(member))
            }
            crate::query_boundaries::type_checking_utilities::ArrayLikeKind::Intersection(
                members,
            ) => members
                .iter()
                .any(|&member| self.property_type_has_array_like_length(member)),
            crate::query_boundaries::type_checking_utilities::ArrayLikeKind::Other => false,
        }
    }

    fn is_global_this_surface_type(&self, type_id: TypeId) -> bool {
        let Some(shape) = query::object_shape_for_type(self.ctx.types, type_id) else {
            return false;
        };

        let has_global_this = shape.properties.iter().any(|prop| {
            self.ctx.types.resolve_atom(prop.name) == "globalThis"
                && prop.type_id == TypeId::UNKNOWN
        });
        let has_global_value = shape.properties.iter().any(|prop| {
            matches!(
                self.ctx.types.resolve_atom(prop.name).as_str(),
                "Array" | "Object" | "String" | "Number" | "Boolean" | "Function"
            )
        });

        has_global_this && has_global_value && shape.string_index.is_none()
    }

    fn fresh_empty_object_member_for_missing_union(
        &mut self,
        object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        let members = crate::query_boundaries::common::union_members(self.ctx.types, object_type)?;
        let mut saw_present_member = false;
        let mut fresh_empty_member = None;

        for &member in members.iter() {
            if member.is_nullable() {
                continue;
            }

            let evaluated_member = self.evaluate_application_type(member);
            let resolved_member = self.resolve_type_for_property_access(evaluated_member);
            match self.resolve_property_access_with_env(resolved_member, property_name) {
                crate::query_boundaries::common::PropertyAccessResult::Success { .. }
                | crate::query_boundaries::common::PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type: Some(_),
                    ..
                } => {
                    saw_present_member = true;
                }
                crate::query_boundaries::common::PropertyAccessResult::PropertyNotFound { .. } => {
                    if crate::query_boundaries::common::is_empty_object_type(
                        self.ctx.types,
                        resolved_member,
                    ) && crate::query_boundaries::common::is_fresh_object_type(
                        self.ctx.types,
                        resolved_member,
                    ) {
                        fresh_empty_member = Some(resolved_member);
                    }
                }
                crate::query_boundaries::common::PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type: None,
                    ..
                }
                | crate::query_boundaries::common::PropertyAccessResult::IsUnknown => {}
            }
        }

        if saw_present_member {
            fresh_empty_member
        } else {
            None
        }
    }

    fn should_suppress_excess_property_for_target(&mut self, target: TypeId) -> bool {
        [target, self.evaluate_type_for_assignability(target)]
            .into_iter()
            .filter_map(|candidate| {
                crate::query_boundaries::common::intersection_members(self.ctx.types, candidate)
            })
            .any(|members| {
                members.iter().any(|member| {
                    let evaluated_member = self.evaluate_type_for_assignability(*member);
                    crate::query_boundaries::common::is_primitive_type(
                        self.ctx.types,
                        evaluated_member,
                    ) || crate::query_boundaries::common::is_type_parameter_like(
                        self.ctx.types,
                        evaluated_member,
                    )
                })
            })
    }

    fn excess_property_target_annotation_for_site(
        &self,
        idx: NodeIndex,
    ) -> Option<(String, bool, Option<NodeIndex>)> {
        let mut current = idx;
        let mut from_nested_container = false;
        loop {
            let info = self.ctx.arena.node_info(current)?;
            let parent_idx = info.parent;
            let parent = self.ctx.arena.get(parent_idx)?;
            if let Some(var_decl) = self.ctx.arena.get_variable_declaration(parent)
                && var_decl.initializer == current
                && var_decl.type_annotation.is_some()
            {
                return self.node_text(var_decl.type_annotation).and_then(|text| {
                    self.sanitize_type_annotation_text_for_diagnostic(text, true)
                        .map(|text| (text, from_nested_container, Some(var_decl.type_annotation)))
                });
            }
            if parent.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                let grandparent_idx = self.ctx.arena.node_info(parent_idx)?.parent;
                let grandparent = self.ctx.arena.get(grandparent_idx)?;
                if let Some(var_decl) = self.ctx.arena.get_variable_declaration(grandparent)
                    && var_decl.initializer == parent_idx
                    && var_decl.type_annotation.is_some()
                {
                    return self.node_text(var_decl.type_annotation).and_then(|text| {
                        self.sanitize_type_annotation_text_for_diagnostic(text, true)
                            .map(|text| {
                                (text, from_nested_container, Some(var_decl.type_annotation))
                            })
                    });
                }
                if let Some(jsdoc_satisfies_text) =
                    self.jsdoc_satisfies_type_text_for_node(parent_idx)
                {
                    return Some((jsdoc_satisfies_text, from_nested_container, None));
                }
                if matches!(
                    grandparent.kind,
                    syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                        | syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                ) {
                    from_nested_container = true;
                    current = grandparent_idx;
                    continue;
                }
                return None;
            }
            if matches!(
                parent.kind,
                syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    | syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            ) {
                from_nested_container = true;
            }
            current = parent_idx;
        }
    }

    fn excess_property_target_display_for_site(
        &mut self,
        target: TypeId,
        idx: NodeIndex,
    ) -> String {
        let inferred_display = self
            .format_pick_over_all_keys_as_keyof(target)
            .unwrap_or_else(|| self.format_excess_property_target_type(target));
        if let Some((annotation_text, annotation_from_nested_container, annotation_type_node)) =
            self.excess_property_target_annotation_for_site(idx)
        {
            let annotation_display = self.format_annotation_like_type(&annotation_text);
            if self.excess_property_site_is_nested_in_nested_array_literal(idx) {
                return annotation_display;
            }
            if inferred_display.starts_with('{') && annotation_display.contains("object &") {
                return annotation_display;
            }
            if inferred_display.starts_with('{')
                && !annotation_display.contains('|')
                && !annotation_display.contains("object")
                && annotation_display.contains('&')
            {
                return annotation_display;
            }
            if annotation_display.contains('|')
                && Self::same_simple_alias_array_union_display(
                    &annotation_display,
                    &inferred_display,
                )
            {
                return annotation_display;
            }
            // When the inferred display is an anonymous object but the annotation is a
            // generic type alias (e.g. `Record<K, V>`, `Partial<T>`), prefer the annotation.
            // Only apply for generic types (containing `<`) — plain identifiers like `Item`
            // may resolve to union types, in which case tsc shows the specific union member.
            if inferred_display.starts_with('{')
                && annotation_display.contains('<')
                && !annotation_display.contains('|')
                && annotation_display
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            {
                return annotation_display;
            }
            // When the annotation is a generic application (e.g. `Partial<Record<Keys, unknown>>`)
            // and the inferred display is the same generic application but with one or more
            // type arguments expanded past their alias name (e.g.
            // `Partial<Record<"a" | "b" | "c" | "d", unknown>>`), prefer the annotation.
            // tsc preserves the user-written alias in the type-argument position.
            // Match by checking that both displays start with the same outer
            // identifier (alphanumeric/underscore prefix) followed by `<`.
            if let (Some(ann_prefix), Some(inf_prefix)) = (
                Self::generic_application_outer_name(&annotation_display),
                Self::generic_application_outer_name(&inferred_display),
            ) && ann_prefix == inf_prefix
                && !annotation_display.contains('|')
            {
                return annotation_display;
            }
            if Self::is_plain_type_alias_display(&annotation_display)
                && annotation_from_nested_container
                && annotation_type_node
                    .is_some_and(|type_node| self.annotation_type_resolves_to_union(type_node))
                && annotation_display != inferred_display
                && !inferred_display.contains('|')
            {
                return annotation_display;
            }
        }
        inferred_display
    }

    fn same_simple_alias_array_union_display(left: &str, right: &str) -> bool {
        fn normalized(display: &str) -> Option<(&str, &str)> {
            let mut parts = display.split(" | ");
            let first = parts.next()?.trim();
            let second = parts.next()?.trim();
            if parts.next().is_some() {
                return None;
            }
            if let Some(base) = first.strip_suffix("[]")
                && base == second
            {
                return Some((base, first));
            }
            if let Some(base) = second.strip_suffix("[]")
                && base == first
            {
                return Some((base, second));
            }
            None
        }

        match (normalized(left), normalized(right)) {
            (Some(l), Some(r)) => l == r,
            _ => false,
        }
    }

    fn is_plain_type_alias_display(display: &str) -> bool {
        let mut chars = display.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        (first.is_ascii_alphabetic() || first == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    }

    fn annotation_type_resolves_to_union(&mut self, type_node: NodeIndex) -> bool {
        let type_id = self.get_type_from_type_node(type_node);
        [
            type_id,
            self.resolve_ref_type(type_id),
            self.evaluate_type_with_env(type_id),
            self.resolve_type_for_property_access(type_id),
        ]
        .into_iter()
        .any(|candidate| query::union_members(self.ctx.types, candidate).is_some())
    }

    fn excess_property_site_is_nested_in_nested_array_literal(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        loop {
            let Some(parent_idx) = self.ctx.arena.parent_of(current) else {
                return false;
            };
            let Some(parent) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            if parent.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                let mut array_depth = 0;
                let mut container_idx = parent_idx;
                while let Some(grandparent_idx) = self.ctx.arena.parent_of(container_idx)
                    && let Some(grandparent) = self.ctx.arena.get(grandparent_idx)
                    && grandparent.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                {
                    array_depth += 1;
                    container_idx = grandparent_idx;
                }
                return array_depth >= 2;
            }
            current = parent_idx;
        }
    }

    /// If `display` looks like a generic application of the form `Name<...>`
    /// (with `Name` an identifier of letters/digits/underscores), return the
    /// outer name. Otherwise return `None`.
    fn generic_application_outer_name(display: &str) -> Option<&str> {
        let lt_idx = display.find('<')?;
        let prefix = &display[..lt_idx];
        if prefix.is_empty() {
            return None;
        }
        let mut chars = prefix.chars();
        let first = chars.next()?;
        if !(first.is_ascii_alphabetic() || first == '_') {
            return None;
        }
        if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return None;
        }
        Some(prefix)
    }

    fn format_pick_over_all_keys_as_keyof(&mut self, target: TypeId) -> Option<String> {
        if !self.ctx.has_lib_loaded() || self.ctx.actual_lib_file_count == 0 {
            return None;
        }
        let (base, args) =
            crate::query_boundaries::common::application_info(self.ctx.types, target).or_else(
                || {
                    let alias = self.ctx.types.get_display_alias(target)?;
                    crate::query_boundaries::common::application_info(self.ctx.types, alias)
                },
            )?;
        if args.len() != 2 {
            return None;
        }
        let base_def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, base)?;
        let is_actual_lib_pick = self
            .ctx
            .actual_lib_def_id_for_bare_name("Pick")
            .is_some_and(|def_id| def_id == base_def_id)
            || self
                .ctx
                .def_symbol_identity(base_def_id)
                .is_some_and(|(sym_id, _)| {
                    self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
                        && self
                            .get_symbol_globally(sym_id)
                            .is_some_and(|symbol| symbol.escaped_name == "Pick")
                });
        if !is_actual_lib_pick {
            return None;
        }

        let object_type = args[0];
        let key_type = args[1];
        let evaluated_object_type = self.evaluate_type_with_env(object_type);
        let shape =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, object_type)
                .or_else(|| {
                    crate::query_boundaries::common::object_shape_for_type(
                        self.ctx.types,
                        evaluated_object_type,
                    )
                })?;
        let evaluated_key_type = self.evaluate_type_with_env(key_type);
        let keys =
            crate::query_boundaries::common::union_members(self.ctx.types, evaluated_key_type)
                .or_else(|| {
                    crate::query_boundaries::common::union_members(self.ctx.types, key_type)
                })
                .unwrap_or_else(|| vec![evaluated_key_type]);
        if keys.len() != shape.properties.len() {
            return None;
        }
        let mut key_atoms = keys
            .iter()
            .copied()
            .map(|key| crate::query_boundaries::common::string_literal_value(self.ctx.types, key))
            .collect::<Option<Vec<_>>>()?;
        key_atoms.sort_unstable();
        let mut prop_atoms = shape
            .properties
            .iter()
            .map(|prop| prop.name)
            .collect::<Vec<_>>();
        prop_atoms.sort_unstable();
        if key_atoms != prop_atoms {
            return None;
        }

        let object_display = self.format_type_diagnostic(object_type);
        Some(format!("Pick<{object_display}, keyof {object_display}>"))
    }

    pub(crate) fn excess_property_diagnostic_message(
        &mut self,
        prop_name: &str,
        target: TypeId,
        idx: NodeIndex,
    ) -> (u32, String) {
        let prop_display = self
            .excess_property_name_display_for_site(idx, self.ctx.types.intern_string(prop_name))
            .unwrap_or_else(|| tsz_solver::format_excess_property_name(prop_name).into_owned());
        let type_str = self.excess_property_target_display_for_site(target, idx);
        let suggestion_target = self.strip_non_object_union_members_for_excess_display(target);
        if !self.has_syntax_parse_errors()
            && let Some(suggestion) = self
                .find_similar_property(prop_name, suggestion_target)
                .or_else(|| self.find_similar_property(prop_name, target))
        {
            return (
                diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_BUT_DOES_NOT_EXIST_IN_TYPE_DID,
                format!(
                    "Object literal may only specify known properties, but '{prop_display}' does not exist in type '{type_str}'. Did you mean to write '{suggestion}'?"
                ),
            );
        }

        (
            diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE,
            format!(
                "Object literal may only specify known properties, and '{prop_display}' does not exist in type '{type_str}'."
            ),
        )
    }

    pub(super) fn access_receiver_for_diagnostic_node(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(idx)?;
        if (node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            && let Some(access) = self.ctx.arena.get_access_expr(node)
        {
            return Some(
                self.ctx
                    .arena
                    .skip_parenthesized_and_assertions(access.expression),
            );
        }

        self.ctx
            .arena
            .node_info(idx)
            .and_then(|info| self.ctx.arena.get(info.parent))
            .and_then(|parent| self.ctx.arena.get_access_expr(parent))
            .map(|access| {
                self.ctx
                    .arena
                    .skip_parenthesized_and_assertions(access.expression)
            })
    }

    fn object_literal_initializer_display_type_for_receiver(
        &mut self,
        idx: NodeIndex,
    ) -> Option<TypeId> {
        let receiver = self.access_receiver_for_diagnostic_node(idx)?;
        let receiver_node = self.ctx.arena.get(receiver)?;
        if receiver_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let sym_id = self.resolve_identifier_symbol(receiver)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl_idx = symbol.value_declaration;
        let decl_node = self.ctx.arena.get(decl_idx)?;
        let decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        let init = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(decl.initializer);
        let init_node = self.ctx.arena.get(init)?;
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }

        let init_type = self.get_type_of_node(init);
        let init_type = self.resolve_type_for_property_access(init_type);
        crate::query_boundaries::common::object_shape_for_type(self.ctx.types, init_type)
            .filter(|shape| shape.symbol.is_none())
            .map(|_| init_type)
    }

    fn object_rest_this_omit_display_for_receiver(&mut self, idx: NodeIndex) -> Option<String> {
        let receiver = self.access_receiver_for_diagnostic_node(idx)?;
        let receiver_node = self.ctx.arena.get(receiver)?;
        if receiver_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let sym_id = self.resolve_identifier_symbol(receiver)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let mut decl_idx = symbol.value_declaration;
        let mut decl_node = self.ctx.arena.get(decl_idx)?;
        if decl_node.kind == SyntaxKind::Identifier as u16 {
            let ext = self.ctx.arena.get_extended(decl_idx)?;
            decl_idx = ext.parent;
            decl_node = self.ctx.arena.get(decl_idx)?;
        }
        if decl_node.kind != syntax_kind_ext::BINDING_ELEMENT {
            return None;
        }
        let binding_element = self.ctx.arena.get_binding_element(decl_node)?;
        if !binding_element.dot_dot_dot_token {
            return None;
        }

        let binding_ext = self.ctx.arena.get_extended(decl_idx)?;
        let pattern_idx = binding_ext.parent;
        let pattern_node = self.ctx.arena.get(pattern_idx)?;
        if pattern_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN {
            return None;
        }

        let pattern_ext = self.ctx.arena.get_extended(pattern_idx)?;
        let var_decl_node = self.ctx.arena.get(pattern_ext.parent)?;
        let var_decl = self.ctx.arena.get_variable_declaration(var_decl_node)?;
        let init_idx = self.ctx.arena.skip_parenthesized(var_decl.initializer);
        let init_node = self.ctx.arena.get(init_idx)?;
        if init_node.kind != SyntaxKind::ThisKeyword as u16 {
            return None;
        }

        let parent_type = self.get_type_of_node(init_idx);
        let mut keys = self.collect_unspreadable_prototype_names_from(parent_type);
        for name in self.collect_non_rest_property_names(pattern_idx) {
            if !keys.iter().any(|k| k == &name) {
                keys.push(name);
            }
        }
        if keys.is_empty() {
            return None;
        }

        let key_display = keys
            .iter()
            .map(|key| format!("\"{key}\""))
            .collect::<Vec<_>>()
            .join(" | ");
        Some(format!("Omit<this, {key_display}>"))
    }

    fn js_constructor_receiver_display_for_node(&mut self, idx: NodeIndex) -> Option<String> {
        if !self.is_js_file() {
            return None;
        }

        let receiver = self.access_receiver_for_diagnostic_node(idx)?;
        let receiver_node = self.ctx.arena.get(receiver)?;
        if receiver_node.kind == SyntaxKind::ThisKeyword as u16 {
            // Prefer the prototype-owner expression when `this` lives inside a
            // method assigned to a `Foo.prototype.x = function() { ... }` chain.
            if let Some(owner) = self
                .find_enclosing_non_arrow_function(receiver)
                .and_then(|func_idx| self.js_prototype_owner_expression_for_node(func_idx))
                .and_then(|owner_expr| {
                    self.js_prototype_owner_function_target(owner_expr)
                        .map(|_| owner_expr)
                })
                .and_then(|owner_expr| self.expression_text(owner_expr))
            {
                return Some(owner);
            }
            if let Some(owner) = self
                .find_enclosing_non_arrow_function(receiver)
                .and_then(|func_idx| self.find_assignment_lhs_for_rhs(func_idx))
                .and_then(|lhs_idx| {
                    let lhs_node = self.ctx.arena.get(lhs_idx)?;
                    if lhs_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        && lhs_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                    {
                        return None;
                    }
                    let access = self.ctx.arena.get_access_expr(lhs_node)?;
                    let receiver_node = self.ctx.arena.get(access.expression)?;
                    if receiver_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        || receiver_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                    {
                        return None;
                    }
                    let sym_id =
                        self.resolve_identifier_symbol(access.expression)
                            .or_else(|| {
                                self.expression_text(access.expression)
                                    .and_then(|text| self.ctx.binder.file_locals.get(text.as_str()))
                            })?;
                    let symbol = self.ctx.binder.get_symbol(sym_id)?;
                    self.ctx
                        .arena
                        .get(symbol.value_declaration)
                        .is_some_and(|decl| decl.is_function_like())
                        .then(|| self.expression_text(access.expression))
                        .flatten()
                })
            {
                return Some(format!("typeof {owner}"));
            }
            if let Some(owner) = self
                .find_enclosing_non_arrow_function(receiver)
                .and_then(|func_idx| self.find_assignment_lhs_for_rhs(func_idx))
                .and_then(|lhs_idx| {
                    let lhs_node = self.ctx.arena.get(lhs_idx)?;
                    if lhs_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        && lhs_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                    {
                        return None;
                    }
                    let lhs_access = self.ctx.arena.get_access_expr(lhs_node)?;
                    if self
                        .ctx
                        .arena
                        .get(lhs_access.expression)
                        .and_then(|owner_node| self.ctx.arena.get_access_expr(owner_node))
                        .and_then(|owner_access| {
                            self.ctx
                                .arena
                                .get_identifier_at(owner_access.name_or_argument)
                        })
                        .is_some_and(|ident| ident.escaped_text == "prototype")
                    {
                        return None;
                    }
                    let owner_text = self.expression_text(lhs_access.expression)?;
                    let owner_sym = self
                        .resolve_identifier_symbol(lhs_access.expression)
                        .or_else(|| self.resolve_qualified_symbol(lhs_access.expression))?;
                    let owner_symbol = self.ctx.binder.get_symbol(owner_sym)?;
                    if owner_symbol.has_any_flags(
                        tsz_binder::symbol_flags::FUNCTION | tsz_binder::symbol_flags::CLASS,
                    ) {
                        Some(format!("typeof {owner_text}"))
                    } else {
                        None
                    }
                })
            {
                return Some(owner);
            }
            // Fallback: a top-level (or nested) JS function with expando
            // assignments uses the function's own name as the apparent type
            // for `this`. tsc displays `Property 'X' does not exist on type
            // 'fn-name'` rather than the inferred expando object shape.
            //
            // Suppress the fallback when the function has an explicit JSDoc
            // `@type` annotation that gives it a callable type — the user
            // typed the function so the apparent `this` is whatever the
            // annotation declares (e.g. `(this: Foo) => void`), not the
            // function's own name.
            let func_idx = self.find_enclosing_non_arrow_function(receiver)?;
            if self
                .jsdoc_callable_type_annotation_for_node(func_idx)
                .is_some()
                || self
                    .get_jsdoc_for_function(func_idx)
                    .is_some_and(|jsdoc| Self::jsdoc_contains_tag(&jsdoc, "this"))
            {
                return None;
            }
            let func_node = self.ctx.arena.get(func_idx)?;
            if func_node.kind != tsz_parser::parser::syntax_kind_ext::FUNCTION_DECLARATION
                && func_node.kind != tsz_parser::parser::syntax_kind_ext::FUNCTION_EXPRESSION
            {
                return None;
            }
            let func_data = self.ctx.arena.get_function(func_node)?;
            let name_node = self.ctx.arena.get(func_data.name)?;
            return self
                .ctx
                .arena
                .get_identifier(name_node)
                .map(|ident| ident.escaped_text.clone());
        }
        if receiver_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let sym_id = self.resolve_identifier_symbol(receiver)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl_idx = symbol.value_declaration;
        let decl_node = self.ctx.arena.get(decl_idx)?;
        let decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        let init = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(decl.initializer);
        let init_node = self.ctx.arena.get(init)?;
        if init_node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return None;
        }

        let new_expr = self.ctx.arena.get_call_expr(init_node)?;
        let ctor_expr = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(new_expr.expression);
        let ctor_node = self.ctx.arena.get(ctor_expr)?;

        if ctor_node.kind == SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(ctor_node)
                .map(|ident| ident.escaped_text.clone());
        }

        if ctor_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.ctx.arena.get_access_expr(ctor_node)?;
            let name = self.ctx.arena.get(access.name_or_argument)?;
            return self
                .ctx
                .arena
                .get_identifier(name)
                .map(|ident| ident.escaped_text.clone());
        }

        None
    }

    /// When `type_id` is a `Lazy(DefId)` for a `TypeAlias` whose evaluated body
    /// is an `Enum`, return the enum's nominal name. Returns `None` when the
    /// receiver is not such an alias.
    fn alias_to_enum_display_name(&mut self, type_id: TypeId) -> Option<String> {
        let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, type_id)?;
        let def = self.ctx.definition_store.get(def_id)?;
        if def.kind != tsz_solver::def::DefKind::TypeAlias {
            return None;
        }
        let evaluated = self.evaluate_type_for_assignability(type_id);
        let enum_def_id = crate::query_boundaries::common::enum_def_id(self.ctx.types, evaluated)?;
        let sym_id = self.ctx.def_to_symbol_id(enum_def_id)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        Some(symbol.escaped_name.to_string())
    }

    fn annotation_uses_module_local_array_type(&self, annotation: &str) -> bool {
        let trimmed = annotation.trim_start();
        let Some(name) = trimmed.strip_prefix("Array<").map(|_| "Array").or_else(|| {
            trimmed
                .strip_prefix("ReadonlyArray<")
                .map(|_| "ReadonlyArray")
        }) else {
            return false;
        };
        if !self.ctx.binder.is_external_module() {
            return false;
        }

        self.ctx.binder.file_locals.get(name).is_some_and(|sym_id| {
            !self.ctx.symbol_is_from_actual_lib(sym_id)
                && self.symbol_has_declared_type_meaning(sym_id)
        })
    }

    fn import_equals_exported_namespace_receiver_display(
        &mut self,
        receiver: NodeIndex,
    ) -> Option<String> {
        let receiver = self.ctx.arena.skip_parenthesized_and_assertions(receiver);
        let receiver_node = self.ctx.arena.get(receiver)?;
        if receiver_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let sym_id = self.resolve_identifier_symbol(receiver)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl_idx = symbol.value_declaration.into_option()?;
        let decl_node = self.ctx.arena.get(decl_idx)?;
        if decl_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            return None;
        }

        let import_decl = self.ctx.arena.get_import_decl(decl_node)?;
        let module_name = self.get_require_module_specifier(import_decl.module_specifier)?;
        let declaring_file_idx = self.ctx.resolve_symbol_file_index(sym_id);
        let exports_table =
            self.resolve_effective_module_exports_from_file(&module_name, declaring_file_idx)?;
        let display_module_name =
            self.resolve_namespace_display_module_name(&exports_table, &module_name);
        let fallback_module_name = self.imported_namespace_display_module_name(&module_name);
        (display_module_name != fallback_module_name)
            .then(|| format!("typeof import(\"{display_module_name}\")"))
    }

    fn property_receiver_display_for_node(&mut self, type_id: TypeId, idx: NodeIndex) -> String {
        let idx = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        if let Some(name) = self.js_constructor_receiver_display_for_node(idx) {
            return name;
        }
        if let Some(name) = self.object_rest_this_omit_display_for_receiver(idx) {
            return name;
        }
        if let Some(receiver) = self.access_receiver_for_diagnostic_node(idx)
            && self
                .ctx
                .arena
                .get(receiver)
                .is_some_and(|node| node.kind == SyntaxKind::ThisKeyword as u16)
            && !self.is_this_in_nested_function_without_own_this_binding(receiver)
            && self.current_this_type() == Some(type_id)
            && crate::query_boundaries::common::object_shape_for_type(self.ctx.types, type_id)
                .is_some()
            && let Some(class_idx) = self.nearest_enclosing_class(receiver)
        {
            let class_name = self.get_class_name_with_type_params_from_decl(class_idx);
            return if class_name == "<anonymous>" {
                "(Anonymous class)".to_string()
            } else {
                class_name
            };
        }
        if let Some(receiver) = self.access_receiver_for_diagnostic_node(idx)
            && let Some(display) = self.import_equals_exported_namespace_receiver_display(receiver)
        {
            return display;
        }
        // When the receiver has a declared type annotation, prefer the source-text
        // annotation for the property-receiver display in cases where tsz's
        // type representation has evaluated past the user-written form:
        //   - generic instantiations (`Bar<Foo>`),
        //   - simple aliases (`Bar` where `type Bar = Omit<Foo, "c">`),
        //   - intersection annotations like `Window & typeof globalThis`
        //     (which tsz collapses to a single member during property-access
        //     evaluation; the source text faithfully preserves all members).
        // Skip annotations containing `{` (inline object literals need the
        // proper type formatter to add `| undefined` for optional members).
        // Also skip reduced intersections: tsc displays `never` for impossible
        // intersections like `A & B` with conflicting private fields or
        // discriminants, not the unreduced source annotation.
        // We deliberately do NOT trigger on `|` annotations because flow
        // narrowing legitimately replaces a union receiver with its picked
        // member, and the narrowed display is what tsc shows.
        let receiver_reduces_to_never = self.evaluate_type_for_assignability(type_id).is_never();
        if let Some(receiver) = self.access_receiver_for_diagnostic_node(idx)
            && let Some(annotation) = self.declared_type_annotation_text_for_expression(receiver)
            && annotation
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
            && !annotation.contains('{')
            && !annotation.contains('|')
            && !matches!(annotation.trim(), "any" | "unknown")
            && !receiver_reduces_to_never
            && crate::query_boundaries::common::union_members(self.ctx.types, type_id).is_none()
            && (crate::query_boundaries::common::is_generic_application(self.ctx.types, type_id)
                || self.ctx.types.get_display_alias(type_id).is_some()
                || annotation.contains('&'))
        {
            // Use the source-text annotation for both generic instantiations
            // (`bar: Bar<Foo>` → `Bar<Foo>`) and simple-alias instantiations
            // (`bar: Bar` where `type Bar = Omit<Foo, "c">` → `Bar`). tsc
            // preserves the alias name in TS2339 even for non-generic aliases;
            // tsz's `display_alias` only tracks one level back to the
            // Application, so without this annotation-bridge we expand to
            // `Omit<Foo, "c">` instead.
            //
            // Skip annotations that contain inline object literal types
            // (`Required<{ a?: 1; x: 1 }>`) — those need the proper type
            // formatter to add `| undefined` for optional properties.
            if self.annotation_uses_module_local_array_type(&annotation) {
                return annotation.trim().to_string();
            }
            return self.format_annotation_like_type(&annotation);
        }
        // When the receiver is a type alias whose body resolves to an Enum
        // (e.g. `type C1 = Color` where `Color` is an enum), tsc displays the
        // underlying enum's nominal name in TS2339 messages, not the alias.
        // The default type formatter follows the Lazy(DefId) directly to the
        // alias name, producing `'C1'` instead of `'Color'`.
        if let Some(enum_name) = self.alias_to_enum_display_name(type_id) {
            return enum_name;
        }
        if crate::query_boundaries::state::checking::is_type_parameter_like(self.ctx.types, type_id)
            && let Some(constraint) =
                crate::query_boundaries::property_access::type_parameter_constraint(
                    self.ctx.types,
                    type_id,
                )
        {
            let access_target_idx = self
                .ctx
                .arena
                .get_extended(idx)
                .map(|ext| ext.parent)
                .and_then(|parent_idx| {
                    self.ctx.arena.get(parent_idx).and_then(|node| {
                        ((node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                            && self
                                .ctx
                                .arena
                                .get_access_expr(node)
                                .is_some_and(|access| access.name_or_argument == idx))
                        .then_some(parent_idx)
                    })
                })
                .unwrap_or(idx);
            let is_direct_write_target = self
                .ctx
                .arena
                .get_extended(access_target_idx)
                .map(|ext| ext.parent)
                .and_then(|parent_idx| {
                    self.ctx
                        .arena
                        .get(parent_idx)
                        .map(|node| (parent_idx, node))
                })
                .is_some_and(|(parent_idx, parent_node)| {
                    if (parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        || parent_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                        && let Some(access) = self.ctx.arena.get_access_expr(parent_node)
                        && access.expression == access_target_idx
                    {
                        return false;
                    }

                    if parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                        && let Some(binary) = self.ctx.arena.get_binary_expr(parent_node)
                    {
                        return binary.left == access_target_idx
                            && self.is_assignment_operator(binary.operator_token);
                    }

                    if (parent_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                        || parent_node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION)
                        && let Some(unary) = self.ctx.arena.get_unary_expr(parent_node)
                    {
                        return unary.operator == SyntaxKind::PlusPlusToken as u16
                            || unary.operator == SyntaxKind::MinusMinusToken as u16;
                    }

                    let _ = parent_idx;
                    false
                });
            if is_direct_write_target {
                return self.format_type_for_assignability_message(type_id);
            }
            return self.format_type_for_assignability_message(constraint);
        }
        if self.is_js_file()
            && let Some(receiver) = self.access_receiver_for_diagnostic_node(idx)
            && let Some(receiver_node) = self.ctx.arena.get(receiver)
            && receiver_node.kind == SyntaxKind::Identifier as u16
            && let Some(ident) = self.ctx.arena.get_identifier(receiver_node)
            && let Some(shape) =
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, type_id)
            && shape.symbol.is_none()
            && self
                .resolve_identifier_symbol(receiver)
                .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                .and_then(|symbol| self.ctx.arena.get(symbol.value_declaration))
                .and_then(|decl_node| self.ctx.arena.get_variable_declaration(decl_node))
                .is_some_and(|decl| {
                    self.ctx.arena.get(decl.initializer).is_some_and(|init| {
                        init.kind == tsz_parser::parser::syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    })
                })
        {
            return format!("typeof {}", ident.escaped_text);
        }
        let diagnostic_receiver = self.access_receiver_for_diagnostic_node(idx);
        let is_direct_element_access_diagnostic = self
            .ctx
            .arena
            .get(idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            || self
                .ctx
                .arena
                .node_info(idx)
                .and_then(|info| self.ctx.arena.get(info.parent))
                .is_some_and(|node| node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION);
        let is_element_access_receiver = self
            .ctx
            .arena
            .get(idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            || self
                .ctx
                .arena
                .node_info(idx)
                .and_then(|info| self.ctx.arena.get(info.parent))
                .is_some_and(|node| node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            || diagnostic_receiver.is_some_and(|receiver| {
                self.ctx
                    .arena
                    .get(receiver)
                    .is_some_and(|node| node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                    || self
                        .ctx
                        .arena
                        .node_info(receiver)
                        .and_then(|info| self.ctx.arena.get(info.parent))
                        .is_some_and(|node| node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            });

        if is_element_access_receiver {
            if !is_direct_element_access_diagnostic
                && let Some(display) =
                    self.element_access_receiver_declared_element_display(idx, type_id)
            {
                return display;
            }
            if let Some(module_name) = self.ctx.namespace_module_names.get(&type_id) {
                return format!("typeof import(\"{module_name}\")");
            }
            let has_named_receiver_identity = self.named_type_display_name(type_id).is_some()
                || self
                    .ctx
                    .definition_store
                    .find_def_for_type(type_id)
                    .is_some()
                || crate::query_boundaries::common::lazy_def_id(self.ctx.types, type_id).is_some()
                || self.ctx.types.get_display_alias(type_id).is_some()
                || self.ctx.namespace_module_names.contains_key(&type_id);
            if has_named_receiver_identity {
                return self.format_type_for_diagnostic_role(
                    type_id,
                    DiagnosticTypeDisplayRole::PropertyReceiver,
                );
            }
            if let Some(init_type) = self.object_literal_initializer_display_type_for_receiver(idx)
            {
                let widened = self.widen_type_for_display(init_type);
                return self.format_type_diagnostic(widened);
            }
            if !self.is_js_file() {
                let evaluated = self.evaluate_type_for_assignability(type_id);
                if evaluated != type_id && self.named_type_display_name(evaluated).is_some() {
                    return self.format_type_for_assignability_message(evaluated);
                }
            }
            return self.format_type_diagnostic_structural(type_id);
        }

        if let Some(display) = self.class_first_union_property_receiver_display(type_id) {
            return display;
        }
        self.format_type_for_diagnostic_role(type_id, DiagnosticTypeDisplayRole::PropertyReceiver)
    }

    fn class_first_union_property_receiver_display(&mut self, type_id: TypeId) -> Option<String> {
        let members = crate::query_boundaries::common::union_members(self.ctx.types, type_id)?;
        if members.len() < 2 {
            return None;
        }

        let mut class_members = Vec::new();
        let mut other_members = Vec::new();
        for member in members {
            if self.get_class_decl_from_type(member).is_some() {
                class_members.push(member);
            } else {
                if member.is_intrinsic() {
                    return None;
                }
                other_members.push(member);
            }
        }
        if class_members.is_empty() || other_members.is_empty() {
            return None;
        }

        class_members.extend(other_members);
        let formatted = class_members
            .into_iter()
            .map(|member| {
                let display = self.format_type(member);
                if crate::query_boundaries::common::intersection_members(self.ctx.types, member)
                    .is_some()
                    || crate::query_boundaries::common::union_members(self.ctx.types, member)
                        .is_some()
                {
                    format!("({display})")
                } else {
                    display
                }
            })
            .collect::<Vec<_>>();
        Some(formatted.join(" | "))
    }

    /// Build a copy of a Callable's shape with ERROR property types replaced
    /// by ANY for diagnostic display. tsc renders apparent-type properties
    /// whose resolution failed (e.g. `exports.blah = exports.someProp` where
    /// `someProp` doesn't exist) as `any`, not as the internal error sentinel.
    ///
    /// Returns None when the input doesn't have a callable shape or contains
    /// no ERROR-typed properties (no rebuild needed).
    pub(crate) fn substitute_error_with_any_in_callable_shape(
        &mut self,
        type_id: TypeId,
    ) -> Option<TypeId> {
        use crate::query_boundaries::common::callable_shape_for_type_extended;
        let shape = callable_shape_for_type_extended(self.ctx.types, type_id)?;
        let needs_rewrite = shape
            .properties
            .iter()
            .any(|p| p.type_id == TypeId::ERROR || p.write_type == TypeId::ERROR);
        if !needs_rewrite {
            return None;
        }
        let mut rewritten: tsz_solver::CallableShape = shape.as_ref().clone();
        for prop in rewritten.properties.iter_mut() {
            if prop.type_id == TypeId::ERROR {
                prop.type_id = TypeId::ANY;
            }
            if prop.write_type == TypeId::ERROR {
                prop.write_type = TypeId::ANY;
            }
        }
        Some(self.ctx.types.factory().callable(rewritten))
    }

    // =========================================================================
    // Property Errors
    // =========================================================================

    /// Report a property not exist error using solver diagnostics with source tracking.
    /// If a similar property name is found on the type, emits TS2551 ("Did you mean?")
    /// instead of TS2339.
    pub fn error_property_not_exist_at(
        &mut self,
        prop_name: &str,
        type_id: TypeId,
        idx: NodeIndex,
    ) {
        // Suppress TS2339 when the type is an internal inference placeholder (__infer_*).
        // These placeholders are created during generic call inference when a type parameter
        // cannot be resolved to a concrete type. Reporting errors on these placeholders
        // produces confusing diagnostics. The actual inference/assignability issue should be
        // reported elsewhere.
        if crate::query_boundaries::common::is_bare_infer_placeholder(self.ctx.types, type_id) {
            return;
        }

        if self.actual_lib_namespace_merged_type_has_property(type_id, prop_name) {
            return;
        }

        // Suppress error if type is ERROR/ANY or an Error type wrapper.
        // This prevents cascading errors when accessing properties on error types.
        // NOTE: We do NOT suppress for UNKNOWN — accessing properties on unknown should error (TS2339).
        // NOTE: We do NOT suppress for NEVER — tsc emits TS2339 for property access on `never`
        // (e.g., after typeof narrowing exhausts all possibilities).
        if type_id == TypeId::ERROR
            || type_id == TypeId::ANY
            || crate::query_boundaries::common::is_error_type(self.ctx.types, type_id)
        {
            return;
        }

        if self.is_global_this_surface_type(type_id)
            && self.ctx.no_implicit_any()
            && !self.is_js_file()
        {
            use crate::diagnostics::{diagnostic_messages, format_message};
            self.error_at_anchor(
                idx,
                DiagnosticAnchorKind::PropertyToken,
                &format_message(
                    diagnostic_messages::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_TYPE_HAS_NO_INDEX_SIGNATURE,
                    &["typeof globalThis"],
                ),
                diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_TYPE_HAS_NO_INDEX_SIGNATURE,
            );
            return;
        }

        // Suppress TS2339 when evaluating a computed property name expression
        // during class instance type building. When a class has a self-referential
        // computed property (e.g., `[rC.x]` inside `declare class RC<T> { x: T;
        // [rC.x]: "b"; }` where `rC: RC<"a">`), the class instance type isn't
        // fully built yet, causing property access on the incomplete type to fail.
        // This is a transient state — the property will be found once the class
        // is fully built. Suppressing here avoids false positives while the
        // computed property name is being evaluated for class member resolution.
        if self.ctx.checking_computed_property_name.is_some()
            && !self.ctx.class_instance_resolution_set.is_empty()
            && crate::query_boundaries::common::application_info(
                self.ctx.types.as_type_database(),
                type_id,
            )
            .is_some()
        {
            return;
        }

        // Suppress TS2339 when the object is a type parameter whose constraint
        // resolved to ERROR or is self-referential/circular.
        //
        // Case 1: constraint == ERROR — the constraint itself already produced a
        // diagnostic (e.g., `typeof a` where `a` is out of scope → TS2552).
        //
        // Case 2: circular constraint — `T extends typeof a` where `a: T` creates
        // a self-referential chain. During two-pass type parameter resolution, the
        // placeholder T (constraint=None) is created first, then the refined T gets
        // constraint=placeholder_T. When a destructured binding `{a}: {a:T}` uses
        // the placeholder T, property access sees constraint=None but the scope has
        // a refined version whose constraint points back to this same placeholder.
        // tsc suppresses TS2339 in this case because the constraint is unresolvable.
        if crate::query_boundaries::state::checking::is_type_parameter_like(self.ctx.types, type_id)
        {
            let constraint = crate::query_boundaries::state::checking::type_parameter_constraint(
                self.ctx.types,
                type_id,
            );
            if constraint.is_some_and(|constraint| {
                constraint == TypeId::ERROR
                    || crate::query_boundaries::common::is_error_type(self.ctx.types, constraint)
            }) {
                return;
            }
            if let Some(name) = crate::query_boundaries::property_access::type_parameter_name(
                self.ctx.types,
                type_id,
            ) {
                let is_self_ref = |c: TypeId| -> bool {
                    crate::query_boundaries::state::checking::is_type_parameter_like(
                        self.ctx.types,
                        c,
                    ) && crate::query_boundaries::property_access::type_parameter_name(
                        self.ctx.types,
                        c,
                    ) == Some(name)
                };
                if constraint.is_some_and(&is_self_ref) {
                    return;
                }
                let name_str = self.ctx.types.resolve_atom(name);
                if let Some(&scope_id) = self.ctx.type_parameter_scope.get(&*name_str)
                    && scope_id != type_id
                {
                    let scope_constraint =
                        crate::query_boundaries::state::checking::type_parameter_constraint(
                            self.ctx.types,
                            scope_id,
                        );
                    if scope_constraint.is_some_and(|constraint| {
                        constraint == TypeId::ERROR
                            || crate::query_boundaries::common::is_error_type(
                                self.ctx.types,
                                constraint,
                            )
                            || is_self_ref(constraint)
                            || constraint == type_id
                    }) {
                        return;
                    }
                }
            } else if constraint.is_none() {
                // Fall back to the display-keyed scope lookup for stale placeholder copies.
                let type_display = self.format_type(type_id);
                if let Some(&scope_id) = self.ctx.type_parameter_scope.get(&type_display)
                    && scope_id != type_id
                {
                    let scope_constraint =
                        crate::query_boundaries::state::checking::type_parameter_constraint(
                            self.ctx.types,
                            scope_id,
                        );
                    if scope_constraint == Some(type_id) {
                        return;
                    }
                }
            }
        }

        // Suppress TS2339 when the file has syntax parse errors.
        // This prevents cascading errors when the parser has already reported syntax issues
        // (e.g., malformed import.defer() without parentheses → TS1005 already emitted).
        if self.has_syntax_parse_errors() {
            return;
        }

        // Suppress TS2339 when the property access is on an expression rooted in an
        // unresolved import (TS2307 was already emitted for the missing module).
        // This prevents cascading errors when a namespace import fails to resolve.
        if let Some(parent) = self.ctx.arena.get_extended(idx)
            && let Some(parent_node) = self.ctx.arena.get(parent.parent)
            && parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        {
            // Get the access expression and check if the base is an unresolved import
            if let Some(access) = self.ctx.arena.get_access_expr(parent_node) {
                // Check if the base expression is an unresolved import
                if self.is_unresolved_import_symbol(access.expression) {
                    return;
                }
                // Check if the base expression type is ERROR (indicating a failed resolution)
                let base_type = self.get_type_of_node(access.expression);
                if base_type == TypeId::ERROR {
                    return;
                }
                // Also check the full chain for unresolved imports
                if self.is_property_access_on_unresolved_import(parent.parent) {
                    return;
                }
            }
        }

        // Checked-JS function declarations support expando writes like
        // `fn.extra = value` without TS2339. Suppress the diagnostic when the
        // property name belongs to a direct write target rooted at a function
        // symbol, even if an intermediate query transiently observed the RHS type.
        if self.is_js_file()
            && self.ctx.compiler_options.check_js
            && let Some(parent) = self.ctx.arena.get_extended(idx)
            && parent.parent.is_some()
            && let Some(parent_node) = self.ctx.arena.get(parent.parent)
            && parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(parent_node)
            && access.name_or_argument == idx
            && self
                .ctx
                .arena
                .get_extended(parent.parent)
                .is_some_and(|prop_ext| {
                    let parent_idx = prop_ext.parent;
                    self.ctx
                        .arena
                        .get(parent_idx)
                        .and_then(|write_parent| {
                            if write_parent.kind == syntax_kind_ext::BINARY_EXPRESSION {
                                let binary = self.ctx.arena.get_binary_expr(write_parent)?;
                                return Some(
                                    binary.left == parent.parent
                                        && self.is_assignment_operator(binary.operator_token),
                                );
                            }
                            if write_parent.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                                || write_parent.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
                            {
                                let unary = self.ctx.arena.get_unary_expr(write_parent)?;
                                return Some(
                                    unary.operator == tsz_scanner::SyntaxKind::PlusPlusToken as u16
                                        || unary.operator
                                            == tsz_scanner::SyntaxKind::MinusMinusToken as u16,
                                );
                            }
                            Some(false)
                        })
                        .unwrap_or(false)
                })
            && let Some(obj_sym) =
                self.resolve_identifier_symbol_without_tracking(access.expression)
            && let Some(symbol) = self
                .get_cross_file_symbol(obj_sym)
                .or_else(|| self.ctx.binder.get_symbol(obj_sym))
            && symbol.has_any_flags(tsz_binder::symbol_flags::FUNCTION)
            && !symbol.has_any_flags(tsz_binder::symbol_flags::CLASS)
        {
            return;
        }

        // In JS/checkJs, `Object.defineProperty(...)` can be handled by the
        // checker’s descriptor-aware paths even when generic member lookup on
        // the global Object value misses. Suppress the fallback TS2339 here so
        // those specialized defineProperty semantics can proceed without the
        // spurious property-not-found diagnostic.
        if self.is_js_file()
            && self.ctx.compiler_options.check_js
            && prop_name == "defineProperty"
            && let Some(parent) = self.ctx.arena.get_extended(idx)
            && parent.parent.is_some()
            && let Some(parent_node) = self.ctx.arena.get(parent.parent)
            && parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(parent_node)
            && access.name_or_argument == idx
            && self.identifier_resolves_to_unshadowed_global(access.expression, "Object")
        {
            return;
        }

        // Suppress cascaded TS2339 from failed generic inference when the receiver
        // remains a union that still contains unresolved type parameters.
        // This keeps follow-on property errors from obscuring the primary root cause
        // (typically assignability/inference diagnostics).
        //
        // Only suppress when a DIRECT union member is a type parameter (e.g. T | Foo).
        // Do NOT suppress when type parameters are deeply nested inside object types
        // (e.g. string | MyInterface where MyInterface has generic base types).
        // The deep nesting case occurs with concrete unions like `string | MyArr`
        // where MyArr extends Array<string> -- the resolved object shape may contain
        // type parameters from the generic base, but the union itself is concrete.
        // NOTE: In tsc 6.0, unconstrained type parameters in unions DO trigger
        // TS2339 when the property doesn't exist on the type parameter member.
        // We no longer suppress TS2339 for unions with type parameters.

        // When a class extends `any`, tsc treats unknown member accesses as `any`
        // and does not emit TS2339. Check this before computing source location
        // to avoid unnecessary work.
        if self.class_extends_any_base(type_id) {
            return;
        }

        // Array-like generic constraints always provide `.length`; if property
        // resolution misses while recursive conditional evaluation is still
        // deferred, avoid emitting a cascaded TS2339.
        if prop_name == "length" && self.property_type_has_array_like_length(type_id) {
            return;
        }

        // Suppress TS2339 for indexed access types on generic conditional/mapped types.
        // For example, `Parameters<DataFirst>["length"]` where `Parameters<T>` is a
        // conditional type. When the type argument is generic, tsc defers the check
        // rather than emitting a false TS2339.
        if crate::query_boundaries::common::is_index_access_type(self.ctx.types, type_id) {
            return;
        }

        // Suppress TS2339 for types that are generic type parameters with conditional
        // type constraints. For example, when accessing a property on a type parameter
        // like `T extends SomeConditionalType`, the property may exist on the resolved
        // conditional type but we can't determine it until the type parameter is
        // instantiated with a concrete type.
        if crate::query_boundaries::state::checking::is_type_parameter_like(self.ctx.types, type_id)
            && crate::query_boundaries::common::type_parameter_has_conditional_constraint(
                self.ctx.types,
                type_id,
            )
        {
            return;
        }

        // Suppress TS2339 for type parameters with generic mapped type constraints.
        // For example, `T extends { [K in keyof U]: V }` where U is another type parameter.
        // The mapped type cannot be fully resolved until U is instantiated.
        if crate::query_boundaries::state::checking::is_type_parameter_like(self.ctx.types, type_id)
            && crate::query_boundaries::common::type_parameter_has_mapped_constraint(
                self.ctx.types,
                type_id,
            )
        {
            return;
        }

        // Suppress TS2339 when the type is an intersection containing type parameters
        // that haven't been resolved yet. This commonly occurs with mixin patterns where
        // the return type is `Constructor<Tagged> & T` - the instance type should have
        // properties from both sides of the intersection, but we may not resolve them
        // properly when T is still generic.
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)
        {
            let has_unresolved_type_param = members.iter().any(|&member| {
                crate::query_boundaries::state::checking::is_type_parameter_like(
                    self.ctx.types,
                    member,
                ) || crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    member,
                )
            });
            if has_unresolved_type_param {
                return;
            }
        }

        // Suppress TS2339 for types that contain conditional types which may resolve
        // to have the property once the type parameters are instantiated.
        // For example: `FirstParameter<typeof h>["foo"]` where `FirstParameter<T>` is
        // `T extends (x: infer P) => unknown ? P : unknown`. When the conditional type
        // cannot be resolved (e.g., during generic inference), tsc defers the check
        // rather than emitting a false TS2339.
        if crate::query_boundaries::common::contains_conditional_type(self.ctx.types, type_id) {
            return;
        }

        // Suppress TS2339 for indexed access types on unresolved generic conditional types.
        // For example, `FirstParameter<typeof h>['foo']` where `FirstParameter<T>` is
        // `T extends (x: infer P) => unknown ? P : unknown`. When the conditional type
        // argument is generic and cannot be resolved (e.g., during inference),
        // tsc defers the check rather than emitting a false TS2339.
        // This covers cases where the base type is an indexed access type whose
        // object type is an unresolved conditional type.
        if let Some(indexed_info) =
            crate::query_boundaries::common::get_indexed_access_type(self.ctx.types, type_id)
            && (crate::query_boundaries::common::contains_conditional_type(
                self.ctx.types,
                indexed_info.object_type,
            ) || crate::query_boundaries::common::contains_type_parameters(
                self.ctx.types,
                indexed_info.object_type,
            ))
        {
            return;
        }

        // Suppress TS2339 for types that are the result of inference-based conditional
        // types that haven't been resolved yet. This commonly occurs with patterns like
        // `type X = FirstParameter<typeof h>['foo']` where `h` is a generic function
        // and the conditional type cannot be resolved until inference completes.
        if crate::query_boundaries::common::type_is_conditional_type_result_with_unresolved_inference(
            self.ctx.types,
            type_id,
        )
        {
            return;
        }

        // Suppress TS2339 for type parameters constrained to generic functions.
        // For example, in `const h = f(g)` where `f` and `g` are generic,
        // the inferred type of `h` may contain unresolved type parameters from
        // the conditional type inference that cannot be checked for property access.
        if crate::query_boundaries::state::checking::is_type_parameter_like(self.ctx.types, type_id)
        {
            let constraint =
                crate::query_boundaries::common::type_parameter_constraint(self.ctx.types, type_id);
            if let Some(constraint_type) = constraint {
                // Only suppress if the constraint is unknown (unresolved) or contains
                // conditional types or type parameters.
                if constraint_type == TypeId::UNKNOWN
                    || crate::query_boundaries::common::contains_conditional_type(
                        self.ctx.types,
                        constraint_type,
                    )
                    || crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        constraint_type,
                    )
                {
                    return;
                }
            }
            // Note: We do NOT suppress for unconstrained type parameters with no constraint.
            // These should still report TS2339 for property access failures as tsc does.
        }

        // Suppress TS2339 for types that are intersections involving generic conditional types.
        // For example, `{ foo: T } & (T extends string ? { bar: string } : { baz: number })`
        // where the conditional type part may or may not have the property being accessed.
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)
        {
            let has_conditional_or_type_param = members.iter().any(|&member| {
                crate::query_boundaries::common::contains_conditional_type(self.ctx.types, member)
                    || crate::query_boundaries::state::checking::is_type_parameter_like(
                        self.ctx.types,
                        member,
                    )
                    || crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        member,
                    )
            });
            if has_conditional_or_type_param {
                return;
            }
        }

        if self
            .resolve_diagnostic_anchor(idx, DiagnosticAnchorKind::PropertyToken)
            .is_some()
        {
            // TS2550: Check if property exists in a newer lib version before
            // trying spelling suggestions. This matches tsc's priority order.
            if !self.has_syntax_parse_errors()
                && let Some((lib_name, override_type_name)) =
                    self.get_lib_suggestion_for_property_with_node(prop_name, type_id, idx)
            {
                let type_str = if let Some(name) = override_type_name {
                    name.to_string()
                } else {
                    self.property_receiver_display_for_node(type_id, idx)
                };
                let message = format!(
                    "Property '{prop_name}' does not exist on type '{type_str}'. Do you need to change your target library? Try changing the 'lib' compiler option to '{lib_name}' or later."
                );
                self.error_at_anchor(
                    idx,
                    DiagnosticAnchorKind::PropertyToken,
                    &message,
                    diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CH,
                );
                return;
            }

            // On files with syntax parse errors, TypeScript generally avoids TS2551
            // suggestion diagnostics and sticks with TS2339 to reduce cascades.
            let suggestion = if self.has_syntax_parse_errors() {
                None
            } else {
                self.find_similar_property(prop_name, type_id)
            };

            // For namespace types, override the type display to match TSC's
            // `typeof import("module")` format instead of the literal object shape.
            //
            // Exception: when the receiver is a CJS module whose
            // `module.exports = <callable>` produces a merged-callable apparent
            // type, tsc displays the structural form (`{ (): void; blah: any; }`)
            // rather than the namespace alias. The receiver Object cached in
            // `namespace_module_names` may be a stale snapshot from an early-call
            // race in `infer_commonjs_export_rhs_type` (returned UNDEFINED before
            // the function expression was typed). Force a fresh recompute of
            // the file's JS export surface — bypassing the resolution-set guard
            // and the cache — and, when it now yields a different callable
            // type, format that structural shape (with ERROR properties
            // rewritten to ANY for parity with tsc's display policy) instead
            // of the alias.
            //
            // See `compiler/pushTypeGetTypeOfAlias.ts` for the symptom and
            // `memory/project_pushTypeGetTypeOfAlias_modulenamespace_display.md`
            // for the iter-20/22/24/28/30/32 investigation trail.
            if self.ctx.namespace_module_names.contains_key(&type_id) {
                let recomputed_surface_type = {
                    let current_file_idx = self.ctx.current_file_idx;
                    self.ctx.js_export_surface_cache.remove(&current_file_idx);
                    let was_in_resolution = self
                        .ctx
                        .js_export_surface_resolution_set
                        .remove(&current_file_idx);
                    let result = self.js_export_surface_namespace_type(current_file_idx);
                    if was_in_resolution {
                        self.ctx
                            .js_export_surface_resolution_set
                            .insert(current_file_idx);
                    }
                    result
                };
                if let Some(merged_ty) = recomputed_surface_type
                    && merged_ty != type_id
                    && crate::query_boundaries::common::has_call_signatures(
                        self.ctx.types.as_type_database(),
                        merged_ty,
                    )
                {
                    let merged_for_display = self
                        .substitute_error_with_any_in_callable_shape(merged_ty)
                        .unwrap_or(merged_ty);
                    let type_str = self.format_type(merged_for_display);
                    let (code, message) = if let Some(ref suggestion) = suggestion {
                        (
                            diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN,
                            format!(
                                "Property '{prop_name}' does not exist on type '{type_str}'. Did you mean '{suggestion}'?"
                            ),
                        )
                    } else {
                        (
                            diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                            format!("Property '{prop_name}' does not exist on type '{type_str}'."),
                        )
                    };
                    self.error_at_anchor(idx, DiagnosticAnchorKind::PropertyToken, &message, code);
                    return;
                }
            }
            if let Some(module_name) = self.ctx.namespace_module_names.get(&type_id).cloned() {
                if let Some(members) =
                    crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)
                    && let Some(display_member) = members.into_iter().find(|&member| {
                        !self.ctx.namespace_module_names.contains_key(&member)
                            && !crate::query_boundaries::js_exports::commonjs_direct_export_supports_named_props(
                                self.ctx.types,
                                member,
                            )
                    })
                {
                    let type_str = self.format_type(display_member);
                    let (code, message) = if let Some(ref suggestion) = suggestion {
                        (
                            diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN,
                            format!(
                                "Property '{prop_name}' does not exist on type '{type_str}'. Did you mean '{suggestion}'?"
                            ),
                        )
                    } else {
                        (
                            diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                            format!("Property '{prop_name}' does not exist on type '{type_str}'."),
                        )
                    };
                    self.error_at_anchor(idx, DiagnosticAnchorKind::PropertyToken, &message, code);
                    return;
                }

                // Normalize module specifier: TSC displays resolved module names
                // without the relative path prefix (e.g., "./b" → "b").
                let display_name = module_name.strip_prefix("./").unwrap_or(&module_name);
                let display_name = strip_property_namespace_module_extension(display_name);
                let type_str = format!("typeof import(\"{display_name}\")");
                let (code, message) = if let Some(ref suggestion) = suggestion {
                    (
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN,
                        format!(
                            "Property '{prop_name}' does not exist on type '{type_str}'. Did you mean '{suggestion}'?"
                        ),
                    )
                } else {
                    (
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                        format!("Property '{prop_name}' does not exist on type '{type_str}'."),
                    )
                };
                self.error_at_anchor(idx, DiagnosticAnchorKind::PropertyToken, &message, code);
                return;
            }

            // For enum container types (e.g., `U8.nonExistent`), tsc displays
            // "typeof EnumName" for the type in the error message.
            if let Some(def_id) =
                crate::query_boundaries::common::enum_def_id(self.ctx.types, type_id)
                && let Some(sym_id) = self.ctx.def_to_symbol_id(def_id)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            {
                let enum_name = &symbol.escaped_name;
                let type_str = format!("typeof {enum_name}");
                let (code, message) = if let Some(ref suggestion) = suggestion {
                    (
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN,
                        format!(
                            "Property '{prop_name}' does not exist on type '{type_str}'. Did you mean '{suggestion}'?"
                        ),
                    )
                } else {
                    (
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                        format!("Property '{prop_name}' does not exist on type '{type_str}'."),
                    )
                };
                self.error_at_anchor(idx, DiagnosticAnchorKind::PropertyToken, &message, code);
                return;
            }

            // For namespace/module value types (e.g., `namespace M { ... }`), tsc displays
            // "typeof NamespaceName" for the type in the error message.
            if let Some(name) = self.get_namespace_typeof_name(type_id) {
                let type_str = format!("typeof {name}");
                let (code, message) = if let Some(ref suggestion) = suggestion {
                    (
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN,
                        format!(
                            "Property '{prop_name}' does not exist on type '{type_str}'. Did you mean '{suggestion}'?"
                        ),
                    )
                } else {
                    (
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                        format!("Property '{prop_name}' does not exist on type '{type_str}'."),
                    )
                };
                self.error_at_anchor(idx, DiagnosticAnchorKind::PropertyToken, &message, code);
                return;
            }

            // TS2812: If the type name matches a known DOM global and the type is
            // structurally empty, suggest including the 'dom' lib option.
            if suggestion.is_none() && self.should_suggest_dom_lib_for_type(type_id) {
                let type_display = self.property_receiver_display_for_node(type_id, idx);
                let message = format!(
                    "Property '{prop_name}' does not exist on type '{type_display}'. Try changing the 'lib' compiler option to include 'dom'."
                );
                self.error_at_anchor(
                    idx,
                    DiagnosticAnchorKind::PropertyToken,
                    &message,
                    diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_TRY_CHANGING_THE_LIB_COMPILER_OPTION_TO_INCLUDE,
                );
                return;
            }

            let type_display = self.property_receiver_display_for_node(type_id, idx);
            let (code, message) = if let Some(ref suggestion) = suggestion {
                (
                    diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN,
                    format!(
                        "Property '{prop_name}' does not exist on type '{type_display}'. Did you mean '{suggestion}'?"
                    ),
                )
            } else {
                (
                    diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                    format!("Property '{prop_name}' does not exist on type '{type_display}'."),
                )
            };
            self.error_at_anchor(idx, DiagnosticAnchorKind::PropertyToken, &message, code);
        }
    }
}

include!("properties/diagnostic_methods_tail.rs");

fn strip_property_namespace_module_extension(module_name: &str) -> &str {
    const EXTS: &[&str] = &[
        ".d.ts", ".d.mts", ".d.cts", ".js", ".ts", ".jsx", ".tsx", ".mjs", ".cjs", ".mts", ".cts",
    ];
    for ext in EXTS {
        if let Some(stripped) = module_name.strip_suffix(ext) {
            return stripped;
        }
    }
    module_name
}

/// Match tsc's `^(?:EventTarget|Node|(?:HTML[a-zA-Z]*)?Element)$` regex used by
/// `containerSeemsToBeEmptyDomElement` to detect DOM element-like type names.
fn is_dom_element_like_name(name: &str) -> bool {
    if name == "EventTarget" || name == "Node" || name == "Element" {
        return true;
    }
    if let Some(prefix) = name.strip_suffix("Element")
        && let Some(rest) = prefix.strip_prefix("HTML")
        && rest.chars().all(|c| c.is_ascii_alphabetic())
    {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use crate::diagnostics::diagnostic_codes;

    fn diagnostics_for_source(source: &str) -> Vec<u32> {
        crate::test_utils::check_source_codes(source)
    }

    /// TS2339 must be suppressed for property access on type parameters with
    /// circular `typeof` constraints (`T extends typeof a` where `a: T`).
    /// This applies to both direct parameters and destructured bindings.
    #[test]
    fn ts2339_suppressed_for_circular_typeof_constraint_direct_param() {
        // Direct parameter: `a: T` where `T extends typeof a`
        let diags = diagnostics_for_source("function f<T extends typeof a>(a: T) { a.b; }");
        assert!(
            !diags.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
            "TS2339 should be suppressed for direct param with circular typeof constraint, got: {diags:?}"
        );
    }

    #[test]
    fn ts2339_suppressed_for_circular_typeof_constraint_destructured_param() {
        // Destructured parameter: `{a}: {a:T}` where `T extends typeof a`
        let diags = diagnostics_for_source("function f<T extends typeof a>({a}: {a:T}) { a.b; }");
        assert!(
            !diags.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
            "TS2339 should be suppressed for destructured param with circular typeof constraint, got: {diags:?}"
        );
    }

    #[test]
    fn ts2339_suppressed_for_circular_typeof_constraint_array_destructured_param() {
        // Array destructured parameter: `[a]: T[]` where `T extends typeof a`
        let diags = diagnostics_for_source("function f<T extends typeof a>([a]: T[]) { a.b; }");
        assert!(
            !diags.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
            "TS2339 should be suppressed for array-destructured param with circular typeof constraint, got: {diags:?}"
        );
    }

    #[test]
    fn ts2339_not_suppressed_for_unconstrained_type_param() {
        // Unconstrained type parameter should still emit TS2339
        let diags = diagnostics_for_source("function f<T>(a: T) { a.b; }");
        assert!(
            diags.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
            "TS2339 should be emitted for unconstrained type param, got: {diags:?}"
        );
    }
}
