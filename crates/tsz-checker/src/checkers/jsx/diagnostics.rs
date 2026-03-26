//! JSX diagnostics rendering: display target building, type formatting for
//! error messages, tag name text extraction, and text-children checks.

use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::ClassData;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    // ── JSX Display Target ────────────────────────────────────────────────

    /// Get the unevaluated Lazy(DefId) type for JSX.IntrinsicAttributes.
    pub(crate) fn get_intrinsic_attributes_lazy_type(&mut self) -> Option<TypeId> {
        let jsx_sym_id = self.get_jsx_namespace_type()?;
        let lib_binders = self.get_lib_binders();
        let symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(jsx_sym_id, &lib_binders)?;
        let exports = symbol.exports.as_ref()?;
        let ia_sym_id = exports.get("IntrinsicAttributes")?;
        let ty = self.type_reference_symbol_type(ia_sym_id);
        let evaluated = self.evaluate_type_with_env(ty);
        if evaluated == TypeId::ANY || evaluated == TypeId::ERROR || evaluated == TypeId::UNKNOWN {
            return None;
        }
        Some(ty)
    }

    fn get_intrinsic_class_attributes_lazy_type(&mut self) -> Option<TypeId> {
        let jsx_sym_id = self.get_jsx_namespace_type()?;
        let lib_binders = self.get_lib_binders();
        let symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(jsx_sym_id, &lib_binders)?;
        let exports = symbol.exports.as_ref()?;
        let ica_sym_id = exports.get("IntrinsicClassAttributes")?;
        // Preserve the generic reference shape for aliases like
        // `type IntrinsicClassAttributes<T> = IntrinsicClassAttributesAlias<T>`.
        // Eagerly resolving the alias body here erases the generic reference we
        // need to instantiate with the component instance type.
        Some(self.resolve_symbol_as_lazy_type(ica_sym_id))
    }

    pub(crate) fn get_intrinsic_class_attributes_type_for_component(
        &mut self,
        component_type: TypeId,
    ) -> Option<TypeId> {
        let ica = self.get_intrinsic_class_attributes_lazy_type()?;
        let inst = self.get_class_instance_type_for_component(component_type)?;
        let app = self.ctx.types.factory().application(ica, vec![inst]);
        let evaluated = self.normalize_jsx_required_props_target(app);
        if evaluated == TypeId::ANY || evaluated == TypeId::ERROR {
            return None;
        }
        Some(app)
    }

    /// Build pre-formatted display string for JSX TS2322 messages.
    /// Returns e.g. `IntrinsicAttributes & PropsType` with correct member order.
    pub(crate) fn build_jsx_display_target(
        &mut self,
        props_type: TypeId,
        component_type: Option<TypeId>,
    ) -> String {
        self.build_jsx_display_target_with_preferred_props(props_type, component_type, None)
    }

    pub(crate) fn build_jsx_display_target_with_preferred_props(
        &mut self,
        props_type: TypeId,
        component_type: Option<TypeId>,
        preferred_props_display: Option<&str>,
    ) -> String {
        let mut parts = Vec::new();
        if let Some(ia) = self.get_intrinsic_attributes_lazy_type() {
            parts.push(self.format_type(ia));
        }
        if let Some(comp) = component_type
            && let Some(intrinsic_class_attrs) =
                self.get_intrinsic_class_attributes_type_for_component(comp)
        {
            parts.push(self.format_type(intrinsic_class_attrs));
        }
        // Skip empty object types (`{}`) in the display — tsc simplifies
        // `IntrinsicAttributes & {}` to just `IntrinsicAttributes`.
        let props_str = preferred_props_display
            .map(str::to_owned)
            .unwrap_or_else(|| self.format_type(props_type));
        if props_str != "{}" {
            parts.push(props_str);
        }
        parts.join(" & ")
    }

    fn get_class_instance_type_for_component(&mut self, component_type: TypeId) -> Option<TypeId> {
        let sigs =
            tsz_solver::type_queries::get_construct_signatures(self.ctx.types, component_type)?;
        let sig = sigs.first()?;
        if sig.return_type == TypeId::ANY || sig.return_type == TypeId::ERROR {
            return None;
        }
        Some(sig.return_type)
    }

    // ── JSX Component Props Display Text ──────────────────────────────────

    pub(super) fn get_jsx_component_props_display_text(
        &mut self,
        tag_name_idx: NodeIndex,
    ) -> Option<String> {
        let sym_id = self.resolve_identifier_symbol(tag_name_idx)?;
        let props_name = self.get_element_attributes_property_name_with_check(None)?;
        if props_name.is_empty() {
            return None;
        }
        self.get_jsx_component_props_display_text_for_symbol(sym_id, &props_name)
    }

    fn get_jsx_component_props_display_text_for_symbol(
        &mut self,
        sym_id: SymbolId,
        props_name: &str,
    ) -> Option<String> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let mut decls = Vec::new();
        if symbol.value_declaration.is_some() {
            decls.push(symbol.value_declaration);
        }
        decls.extend(symbol.declarations.iter().copied());

        for decl_idx in decls {
            if let Some(display) =
                self.get_jsx_component_props_display_text_from_declaration(decl_idx, props_name)
            {
                return Some(display);
            }
        }
        None
    }

    fn get_jsx_component_props_display_text_from_declaration(
        &mut self,
        decl_idx: NodeIndex,
        props_name: &str,
    ) -> Option<String> {
        let mut decl_idx = decl_idx;
        let mut decl_node = self.ctx.arena.get(decl_idx)?;
        if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
            && let Some(parent) = self.ctx.arena.get_extended(decl_idx).map(|ext| ext.parent)
            && parent.is_some()
        {
            decl_idx = parent;
            decl_node = self.ctx.arena.get(decl_idx)?;
        }

        match decl_node.kind {
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                let decl = self.ctx.arena.get_variable_declaration(decl_node)?;
                if decl.type_annotation.is_none() {
                    return None;
                }
                self.get_jsx_component_props_display_text_from_type_node(
                    decl.type_annotation,
                    props_name,
                )
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                let iface = self.ctx.arena.get_interface(decl_node)?;
                self.get_jsx_component_props_display_text_from_members(&iface.members, props_name)
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                let alias = self.ctx.arena.get_type_alias(decl_node)?;
                self.get_jsx_component_props_display_text_from_type_node(
                    alias.type_node,
                    props_name,
                )
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION =>
            {
                let class = self.ctx.arena.get_class(decl_node)?;
                self.get_jsx_component_props_display_text_from_class(class, props_name)
            }
            _ => None,
        }
    }

    fn get_jsx_component_props_display_text_from_class(
        &mut self,
        class: &ClassData,
        props_name: &str,
    ) -> Option<String> {
        if props_name != "props" {
            return None;
        }

        let type_params = class.type_parameters.as_ref()?;
        let first_param_idx = *type_params.nodes.first()?;
        let first_param_node = self.ctx.arena.get(first_param_idx)?;
        let first_param = self.ctx.arena.get_type_parameter(first_param_node)?;
        if first_param.constraint == NodeIndex(0) {
            return None;
        }

        self.format_jsx_props_display_text_from_type_node(first_param.constraint)
    }

    fn get_jsx_component_props_display_text_from_type_node(
        &mut self,
        type_node_idx: NodeIndex,
        props_name: &str,
    ) -> Option<String> {
        let type_node = self.ctx.arena.get(type_node_idx)?;
        match type_node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let type_ref = self.ctx.arena.get_type_ref(type_node)?;
                let TypeSymbolResolution::Type(target_sym_id) =
                    self.resolve_identifier_symbol_in_type_position(type_ref.type_name)
                else {
                    return None;
                };
                self.get_jsx_component_props_display_text_for_symbol(target_sym_id, props_name)
            }
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                let type_lit = self.ctx.arena.get_type_literal(type_node)?;
                self.get_jsx_component_props_display_text_from_members(
                    &type_lit.members,
                    props_name,
                )
            }
            _ => None,
        }
    }

    fn get_jsx_component_props_display_text_from_members(
        &mut self,
        members: &tsz_parser::parser::NodeList,
        props_name: &str,
    ) -> Option<String> {
        for &member_idx in &members.nodes {
            let member_node = self.ctx.arena.get(member_idx)?;
            if member_node.kind != syntax_kind_ext::CONSTRUCT_SIGNATURE {
                continue;
            }
            let sig = self.ctx.arena.get_signature(member_node)?;
            let return_type_idx = sig.type_annotation;
            let return_type_node = self.ctx.arena.get(return_type_idx)?;
            if return_type_node.kind != syntax_kind_ext::TYPE_LITERAL {
                continue;
            }
            let type_lit = self.ctx.arena.get_type_literal(return_type_node)?;
            for &instance_member_idx in &type_lit.members.nodes {
                let instance_member_node = self.ctx.arena.get(instance_member_idx)?;
                if instance_member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
                    continue;
                }
                let prop_sig = self.ctx.arena.get_signature(instance_member_node)?;
                let prop_name_text = self.node_text(prop_sig.name)?.trim().to_string();
                if prop_name_text != props_name || prop_sig.type_annotation.is_none() {
                    continue;
                }
                return self.format_jsx_props_display_text_from_type_node(prop_sig.type_annotation);
            }
        }
        None
    }

    fn format_jsx_props_display_text_from_type_node(
        &mut self,
        type_node_idx: NodeIndex,
    ) -> Option<String> {
        let type_node = self.ctx.arena.get(type_node_idx)?;
        if type_node.kind == syntax_kind_ext::INTERSECTION_TYPE {
            let composite = self.ctx.arena.get_composite_type(type_node)?;
            let parts: Vec<String> = composite
                .types
                .nodes
                .iter()
                .filter_map(|&member_idx| {
                    let member_type = self.get_type_from_type_node(member_idx);
                    let formatted = self.format_type(member_type);
                    (!formatted.is_empty()).then_some(formatted)
                })
                .collect();
            if !parts.is_empty() {
                return Some(parts.join(" & "));
            }
        }

        let type_id = self.get_type_from_type_node(type_node_idx);
        Some(self.format_type(type_id))
    }

    // ── JSX Tag Name Text ─────────────────────────────────────────────────

    /// Get the text of a JSX tag name for error messages.
    pub(crate) fn get_jsx_tag_name_text(&self, tag_name_idx: NodeIndex) -> String {
        let Some(tag_name_node) = self.ctx.arena.get(tag_name_idx) else {
            return "unknown".to_string();
        };

        // Simple identifier
        if let Some(ident) = self.ctx.arena.get_identifier(tag_name_node) {
            return ident.escaped_text.as_str().to_owned();
        }

        // `this` keyword
        if tag_name_node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16 {
            return "this".to_string();
        }

        // Property access expression — reconstruct from the access expression structure
        // to preserve exact formatting (e.g., `obj. MemberClassComponent` with the space).
        // We can't use node_text() directly because the parser's PROPERTY_ACCESS_EXPRESSION
        // node span in JSX tag position may extend into trailing JSX tokens (` />`).
        if let Some(access) = self.ctx.arena.get_access_expr(tag_name_node) {
            let expr_text = self.get_jsx_tag_name_text(access.expression);
            let name_text = self
                .ctx
                .arena
                .get(access.name_or_argument)
                .and_then(|n| self.ctx.arena.get_identifier(n))
                .map(|id| id.escaped_text.as_str().to_owned())
                .unwrap_or_default();

            // Preserve whitespace between expression end and name start (includes dot + spaces)
            // get_node_span returns (start, end) — we need end of expression, start of name
            if let Some((_, expr_end)) = self.get_node_span(access.expression)
                && let Some((name_start, _)) = self.get_node_span(access.name_or_argument)
            {
                let source = self.ctx.arena.source_files.first().map(|f| f.text.as_ref());
                if let Some(src) = source {
                    let between =
                        &src[expr_end as usize..std::cmp::min(name_start as usize, src.len())];
                    return format!("{expr_text}{between}{name_text}");
                }
            }

            return format!("{expr_text}.{name_text}");
        }

        // Fallback: use raw source text, trimming trailing JSX tokens
        self.node_text(tag_name_idx)
            .map(|t| t.trim_end().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    }

    // ── JSX Text Children Check ───────────────────────────────────────────

    /// Check TS2747: component doesn't accept text as child elements.
    /// When JSX children include text nodes but the `children` prop type doesn't
    /// include `string`, emit TS2747 at each text child position.
    pub(crate) fn check_jsx_text_children_accepted(
        &mut self,
        props_type: TypeId,
        tag_name_idx: NodeIndex,
        text_child_indices: &[NodeIndex],
    ) {
        use crate::query_boundaries::common::PropertyAccessResult;

        let resolved = self.resolve_type_for_property_access(props_type);
        let children_prop_name = self.get_jsx_children_prop_name();
        let children_type =
            match self.resolve_property_access_with_env(resolved, &children_prop_name) {
                PropertyAccessResult::Success { type_id, .. } => type_id,
                _ => return,
            };
        let children_type = self.evaluate_type_with_env(children_type);
        if children_type == TypeId::ANY || children_type == TypeId::ERROR {
            return;
        }

        // Check if `string` is assignable to the children type.
        if self.is_assignable_to(TypeId::STRING, children_type) {
            return;
        }

        // Get component name for the diagnostic message.
        let component_name = self.get_jsx_tag_name_text(tag_name_idx);
        let children_type_str = self.format_type(children_type);

        use crate::diagnostics::diagnostic_codes;
        for &text_idx in text_child_indices {
            self.error_at_node_msg(
                text_idx,
                diagnostic_codes::COMPONENTS_DONT_ACCEPT_TEXT_AS_CHILD_ELEMENTS_TEXT_IN_JSX_HAS_THE_TYPE_STRING_BU,
                &[&component_name, &children_prop_name, &children_type_str],
            );
        }
    }
}
