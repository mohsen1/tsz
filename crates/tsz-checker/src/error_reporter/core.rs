//! Core error emission helpers and type formatting utilities.

use crate::diagnostics::{Diagnostic, diagnostic_codes, format_message};
use crate::state::{CheckerState, MemberAccessLevel};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn format_extract_keyof_string_type(&mut self, ty: TypeId) -> Option<String> {
        let members = tsz_solver::type_queries::data::get_intersection_members(self.ctx.types, ty)?;
        if members.len() != 2 || !members.contains(&TypeId::STRING) {
            return None;
        }

        let other = members
            .iter()
            .copied()
            .find(|&member| member != TypeId::STRING)?;
        if !tsz_solver::type_queries::is_keyof_type(self.ctx.types, other) {
            return None;
        }

        Some(format!(
            "Extract<{}, string>",
            self.format_type_for_assignability_message(other)
        ))
    }

    fn format_annotation_like_type(&mut self, text: &str) -> String {
        let formatted = text.trim().to_string();
        if formatted.starts_with("{ ")
            && formatted.ends_with(" }")
            && formatted.contains(':')
            && !formatted.ends_with("; }")
        {
            return format!("{}; }}", &formatted[..formatted.len() - 2]);
        }
        formatted
    }

    fn should_use_evaluated_assignability_display(&self, ty: TypeId, evaluated: TypeId) -> bool {
        if ty == evaluated || evaluated == TypeId::ERROR {
            return false;
        }

        if ty == TypeId::BOOLEAN_TRUE || ty == TypeId::BOOLEAN_FALSE {
            return false;
        }

        if tsz_solver::literal_value(self.ctx.types, ty).is_some() {
            return false;
        }

        if tsz_solver::type_queries::contains_type_parameters_db(self.ctx.types, ty)
            || tsz_solver::type_queries::contains_type_parameters_db(self.ctx.types, evaluated)
        {
            return false;
        }

        if evaluated == TypeId::NEVER
            || tsz_solver::literal_value(self.ctx.types, evaluated).is_some()
        {
            return true;
        }

        if !tsz_solver::type_queries::is_index_access_type(self.ctx.types, ty)
            && !tsz_solver::type_queries::is_keyof_type(self.ctx.types, ty)
            && !tsz_solver::type_queries::is_conditional_type(self.ctx.types, ty)
            && !tsz_solver::is_generic_application(self.ctx.types, ty)
        {
            return false;
        }

        matches!(
            evaluated,
            TypeId::STRING
                | TypeId::NUMBER
                | TypeId::BOOLEAN
                | TypeId::BIGINT
                | TypeId::UNDEFINED
                | TypeId::NULL
                | TypeId::VOID
        )
    }

    fn format_structural_indexed_object_type(&mut self, ty: TypeId) -> Option<String> {
        let shape = tsz_solver::type_queries::get_object_shape(self.ctx.types, ty)?;
        if shape.string_index.is_none() && shape.number_index.is_none() {
            return None;
        }

        let mut parts = Vec::new();
        if let Some(idx) = &shape.string_index {
            let key_name = idx
                .param_name
                .map(|a| self.ctx.types.resolve_atom_ref(a).to_string())
                .unwrap_or_else(|| "x".to_string());
            parts.push(format!(
                "[{key_name}: string]: {}",
                self.format_type(idx.value_type)
            ));
        }
        if let Some(idx) = &shape.number_index {
            let key_name = idx
                .param_name
                .map(|a| self.ctx.types.resolve_atom_ref(a).to_string())
                .unwrap_or_else(|| "x".to_string());
            parts.push(format!(
                "[{key_name}: number]: {}",
                self.format_type(idx.value_type)
            ));
        }
        for prop in &shape.properties {
            let name = self.ctx.types.resolve_atom_ref(prop.name);
            let optional = if prop.optional { "?" } else { "" };
            let readonly = if prop.readonly { "readonly " } else { "" };
            parts.push(format!(
                "{readonly}{name}{optional}: {}",
                self.format_type(prop.type_id)
            ));
        }

        Some(format!("{{ {}; }}", parts.join("; ")))
    }

    pub(super) fn literal_expression_display(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;

        match node.kind {
            k if k == tsz_scanner::SyntaxKind::StringLiteral as u16
                || k == tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                let lit = self.ctx.arena.get_literal(node)?;
                Some(format!("\"{}\"", lit.text))
            }
            k if k == tsz_scanner::SyntaxKind::NumericLiteral as u16 => {
                let lit = self.ctx.arena.get_literal(node)?;
                Some(lit.text.clone())
            }
            k if k == tsz_scanner::SyntaxKind::TrueKeyword as u16 => Some("true".to_string()),
            k if k == tsz_scanner::SyntaxKind::FalseKeyword as u16 => Some("false".to_string()),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.ctx.arena.get_unary_expr(node)?;
                let operand = self.literal_expression_display(unary.operand)?;
                match unary.operator {
                    k if k == tsz_scanner::SyntaxKind::MinusToken as u16 => {
                        Some(format!("-{operand}"))
                    }
                    k if k == tsz_scanner::SyntaxKind::PlusToken as u16 => Some(operand),
                    _ => None,
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                let cond = self.ctx.arena.get_conditional_expr(node)?;
                let left = self.literal_expression_display(cond.when_true)?;
                let right = self.literal_expression_display(cond.when_false)?;
                if left == right {
                    Some(left)
                } else {
                    Some(format!("{left} | {right}"))
                }
            }
            _ => None,
        }
    }

    fn assignment_source_expression(&self, anchor_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = anchor_idx;
        let mut guard = 0;

        while current.is_some() {
            guard += 1;
            if guard > 256 {
                break;
            }

            let node = self.ctx.arena.get(current)?;
            match node.kind {
                k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                    let bin = self.ctx.arena.get_binary_expr(node)?;
                    if self.is_assignment_operator(bin.operator_token) {
                        return Some(bin.right);
                    }
                }
                k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                    let stmt = self.ctx.arena.get_expression_statement(node)?;
                    let expr = self.ctx.arena.get(stmt.expression)?;
                    let bin = self.ctx.arena.get_binary_expr(expr)?;
                    return self
                        .is_assignment_operator(bin.operator_token)
                        .then_some(bin.right);
                }
                k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                    let decl = self.ctx.arena.get_variable_declaration(node)?;
                    return decl.initializer.is_some().then_some(decl.initializer);
                }
                k if k == syntax_kind_ext::RETURN_STATEMENT => {
                    let ret = self.ctx.arena.get_return_statement(node)?;
                    return ret.expression.is_some().then_some(ret.expression);
                }
                _ => {}
            }

            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        None
    }

    fn declared_type_annotation_text_for_expression(&self, expr_idx: NodeIndex) -> Option<String> {
        fn sanitize_type_annotation_text(text: String) -> Option<String> {
            let mut text = text.trim().trim_start_matches(':').trim().to_string();
            while matches!(text.chars().last(), Some(',') | Some(';')) {
                text.pop();
                text = text.trim_end().to_string();
            }
            while matches!(text.chars().last(), Some(')')) {
                let open_count = text.chars().filter(|&ch| ch == '(').count();
                let close_count = text.chars().filter(|&ch| ch == ')').count();
                if close_count <= open_count {
                    break;
                }
                text.pop();
                text = text.trim_end().to_string();
            }
            if text.starts_with('{') || text.starts_with('[') {
                return None;
            }
            let open_count = text.chars().filter(|&ch| ch == '(').count();
            let close_count = text.chars().filter(|&ch| ch == ')').count();
            if open_count != close_count {
                return None;
            }
            (!text.is_empty()).then_some(text)
        }

        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }

        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl = self.ctx.arena.get(symbol.value_declaration)?;

        if let Some(param) = self.ctx.arena.get_parameter(decl)
            && param.type_annotation.is_some()
        {
            return self
                .node_text(param.type_annotation)
                .and_then(sanitize_type_annotation_text);
        }

        if let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl)
            && var_decl.type_annotation.is_some()
        {
            return self
                .node_text(var_decl.type_annotation)
                .and_then(sanitize_type_annotation_text);
        }

        None
    }

    pub(super) fn format_assignment_source_type_for_diagnostic(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> String {
        if let Some(expr_idx) = self.assignment_source_expression(anchor_idx) {
            if self.is_literal_sensitive_assignment_target(target)
                && let Some(display) = self.literal_expression_display(expr_idx)
            {
                return display;
            }

            if let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx) {
                return display;
            }

            let expr_type = self.get_type_of_node(expr_idx);
            let display_type = if expr_type != TypeId::ERROR {
                if self.should_widen_enum_member_assignment_source(expr_type, target) {
                    self.widen_enum_member_type(expr_type)
                } else {
                    expr_type
                }
            } else {
                source
            };
            if let Some(sym_id) = self.resolve_identifier_symbol(expr_idx)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && (symbol.flags & tsz_binder::symbol_flags::ENUM) != 0
                && (symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER) == 0
            {
                return format!("typeof {}", symbol.escaped_name);
            }
            let display_type =
                if tsz_solver::keyof_inner_type(self.ctx.types, display_type).is_some() {
                    let evaluated = self.evaluate_type_for_assignability(display_type);
                    tsz_solver::widening::widen_type(self.ctx.types, evaluated)
                } else {
                    display_type
                };
            let formatted = self.format_type_for_assignability_message(display_type);
            let resolved_for_access = self.resolve_type_for_property_access(display_type);
            let resolved = self.judge_evaluate(resolved_for_access);
            let resolver =
                tsz_solver::objects::index_signatures::IndexSignatureResolver::new(self.ctx.types);
            if !formatted.contains('{')
                && !formatted.contains('[')
                && !formatted.contains('|')
                && !formatted.contains('&')
                && !formatted.contains('<')
                && !tsz_solver::type_queries::contains_type_parameters_db(
                    self.ctx.types,
                    display_type,
                )
                && (resolver.has_index_signature(
                    resolved,
                    tsz_solver::objects::index_signatures::IndexKind::String,
                ) || resolver.has_index_signature(
                    resolved,
                    tsz_solver::objects::index_signatures::IndexKind::Number,
                ))
            {
                if let Some(structural) = self.format_structural_indexed_object_type(resolved) {
                    return structural;
                }
                return self.format_type(resolved);
            }
            if let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx)
                && !display.starts_with("keyof ")
                && !display.contains("[P in ")
                && !display.contains("[K in ")
            {
                return self.format_annotation_like_type(&display);
            }
            return formatted;
        }

        self.format_type_for_assignability_message(source)
    }

    pub(super) fn is_literal_sensitive_assignment_target(&mut self, target: TypeId) -> bool {
        let target = self.evaluate_type_for_assignability(target);
        self.is_literal_sensitive_assignment_target_inner(target)
    }

    fn is_literal_sensitive_assignment_target_inner(&self, target: TypeId) -> bool {
        if tsz_solver::literal_value(self.ctx.types, target).is_some() {
            return true;
        }
        if tsz_solver::type_queries::get_enum_def_id(self.ctx.types, target).is_some() {
            return true;
        }
        if tsz_solver::type_queries::is_symbol_or_unique_symbol(self.ctx.types, target)
            && target != TypeId::SYMBOL
        {
            return true;
        }
        if let Some(list) = tsz_solver::union_list_id(self.ctx.types, target)
            .or_else(|| tsz_solver::intersection_list_id(self.ctx.types, target))
        {
            return self
                .ctx
                .types
                .type_list(list)
                .iter()
                .copied()
                .any(|member| self.is_literal_sensitive_assignment_target_inner(member));
        }
        target == TypeId::NEVER
    }

    fn should_widen_enum_member_assignment_source(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let widened_source = self.widen_enum_member_type(source);
        if widened_source == source {
            return false;
        }

        let target = self.evaluate_type_for_assignability(target);
        tsz_solver::type_queries::get_enum_def_id(self.ctx.types, target).is_none()
            && tsz_solver::type_queries::get_union_members(self.ctx.types, target).is_none()
            && tsz_solver::type_queries::data::get_intersection_members(self.ctx.types, target)
                .is_none()
    }

    pub(super) fn unresolved_unused_renaming_property_in_type_query(
        &self,
        name: &str,
        idx: NodeIndex,
    ) -> Option<String> {
        let mut saw_type_query = false;
        let mut current = idx;
        let mut guard = 0;

        while current.is_some() {
            guard += 1;
            if guard > 256 {
                break;
            }
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::TYPE_QUERY {
                saw_type_query = true;
            }

            if matches!(
                node.kind,
                syntax_kind_ext::FUNCTION_TYPE
                    | syntax_kind_ext::CONSTRUCTOR_TYPE
                    | syntax_kind_ext::CALL_SIGNATURE
                    | syntax_kind_ext::CONSTRUCT_SIGNATURE
                    | syntax_kind_ext::METHOD_SIGNATURE
                    | syntax_kind_ext::FUNCTION_DECLARATION
                    | syntax_kind_ext::FUNCTION_EXPRESSION
                    | syntax_kind_ext::ARROW_FUNCTION
                    | syntax_kind_ext::METHOD_DECLARATION
                    | syntax_kind_ext::CONSTRUCTOR
                    | syntax_kind_ext::GET_ACCESSOR
                    | syntax_kind_ext::SET_ACCESSOR
            ) {
                if !saw_type_query {
                    return None;
                }
                return self.find_renamed_binding_property_for_name(current, name);
            }

            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        None
    }

    fn find_renamed_binding_property_for_name(
        &self,
        root: NodeIndex,
        name: &str,
    ) -> Option<String> {
        let mut stack = vec![root];
        while let Some(node_idx) = stack.pop() {
            let Some(node) = self.ctx.arena.get(node_idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::BINDING_ELEMENT
                && let Some(binding) = self.ctx.arena.get_binding_element(node)
                && binding.property_name.is_some()
                && binding.name.is_some()
                && self.ctx.arena.get_identifier_text(binding.name) == Some(name)
            {
                let prop_name = self
                    .ctx
                    .arena
                    .get_identifier_text(binding.property_name)
                    .map(str::to_string)?;
                return Some(prop_name);
            }

            stack.extend(self.ctx.arena.get_children(node_idx));
        }
        None
    }

    pub(super) fn has_more_specific_diagnostic_at_span(&self, start: u32, length: u32) -> bool {
        self.ctx.diagnostics.iter().any(|diag| {
            diag.start == start
                && diag.length == length
                && diag.code != diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        })
    }

    pub(super) fn format_type_for_assignability_message(&mut self, ty: TypeId) -> String {
        if let Some(collapsed) = self.format_union_with_collapsed_enum_display(ty) {
            return collapsed;
        }

        if let Some(enum_name) = self.format_qualified_enum_name_for_message(ty) {
            return enum_name;
        }

        if ty == TypeId::BOOLEAN_TRUE {
            return "true".to_string();
        }
        if ty == TypeId::BOOLEAN_FALSE {
            return "false".to_string();
        }

        let evaluated = self.evaluate_type_for_assignability(ty);
        if self.should_use_evaluated_assignability_display(ty, evaluated) {
            return self.format_type_for_assignability_message(evaluated);
        }

        if let Some((object_type, index_type)) =
            tsz_solver::type_queries::get_index_access_types(self.ctx.types, ty)
            && let Some(extract_display) = self.format_extract_keyof_string_type(index_type)
        {
            let object_display = self.format_type_for_assignability_message(object_type);
            return format!("{object_display}[{extract_display}]");
        }

        if let Some(extract_display) = self.format_extract_keyof_string_type(ty) {
            return extract_display;
        }

        let mut formatted = self.format_type_diagnostic(ty);

        // Preserve generic instantiations for nominal class instance names when possible.
        if !formatted.contains('<')
            && let Some(shape) = tsz_solver::type_queries::get_object_shape(self.ctx.types, ty)
            && let Some(sym_id) = shape.symbol
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            let symbol_name = symbol.escaped_name.as_str();
            if formatted == symbol_name {
                let def_id = self.ctx.get_or_create_def_id(sym_id);
                let type_param_count =
                    if let Some(type_params) = self.ctx.get_def_type_params(def_id) {
                        type_params.len()
                    } else {
                        symbol
                            .declarations
                            .iter()
                            .find_map(|decl| {
                                let node = self.ctx.arena.get(*decl)?;
                                let class = self.ctx.arena.get_class(node)?;
                                Some(class.type_parameters.as_ref().map_or(0, |p| p.nodes.len()))
                            })
                            .unwrap_or(0)
                    };
                if type_param_count > 0 && shape.properties.len() >= type_param_count {
                    // Collect non-brand properties with their names for stable sorting.
                    // FxHashMap iteration order is non-deterministic; adding __private_brand
                    // can shift hash bucket positions and reorder other properties.
                    // Sort alphabetically so that `ClassPrivate<T,U>` produces consistent
                    // type args regardless of whether private brands are present.
                    // Note: `__private_brand_` sorts before lowercase names (ASCII '_' < 'a').
                    let mut candidates: Vec<(String, TypeId)> = shape
                        .properties
                        .iter()
                        .filter_map(|prop| {
                            let name = self.ctx.types.resolve_atom_ref(prop.name).to_string();
                            if name.starts_with("__private_brand_") {
                                None
                            } else {
                                Some((name, prop.type_id))
                            }
                        })
                        .collect();
                    candidates.sort_by(|a, b| a.0.cmp(&b.0));
                    let args: Vec<String> = candidates
                        .iter()
                        .take(type_param_count)
                        .map(|(_, type_id)| self.format_type_diagnostic(*type_id))
                        .collect();
                    if args.len() == type_param_count {
                        formatted = format!("{}<{}>", symbol_name, args.join(", "));
                    }
                }
            }
        }

        // tsc commonly formats object type literals with a trailing semicolon before `}`.
        if formatted.starts_with("{ ")
            && formatted.ends_with(" }")
            && formatted.contains(':')
            && !formatted.ends_with("; }")
        {
            return format!("{}; }}", &formatted[..formatted.len() - 2]);
        }
        formatted
    }

    fn format_union_with_collapsed_enum_display(&mut self, ty: TypeId) -> Option<String> {
        let members = tsz_solver::type_queries::get_union_members(self.ctx.types, ty)?;
        if members.len() < 2 {
            return None;
        }

        let mut rendered = Vec::with_capacity(members.len());
        let mut collapsed_enum = None;

        for member in members {
            let widened = self.widen_enum_member_type(member);
            if let Some(name) = self.format_qualified_enum_name_for_message(widened) {
                match collapsed_enum.as_ref() {
                    Some(existing) if existing == &name => {}
                    None => {
                        collapsed_enum = Some(name.clone());
                        rendered.push(name);
                    }
                    Some(_) => return None,
                }
            } else {
                rendered.push(self.format_type_for_assignability_message(member));
            }
        }

        if collapsed_enum.is_some() {
            Some(rendered.join(" | "))
        } else {
            None
        }
    }

    fn format_qualified_enum_name_for_message(&mut self, ty: TypeId) -> Option<String> {
        let def_id = tsz_solver::type_queries::get_enum_def_id(self.ctx.types, ty)?;
        let sym_id = self.ctx.def_to_symbol_id_with_fallback(def_id)?;
        let mut parts = Vec::new();
        let mut current = sym_id;

        while current != tsz_binder::SymbolId::NONE {
            let symbol = self.ctx.binder.get_symbol(current)?;
            parts.push(symbol.escaped_name.clone());
            current = symbol.parent;
            if current != tsz_binder::SymbolId::NONE
                && let Some(parent) = self.ctx.binder.get_symbol(current)
                && (parent.flags
                    & (tsz_binder::symbol_flags::NAMESPACE_MODULE
                        | tsz_binder::symbol_flags::VALUE_MODULE
                        | tsz_binder::symbol_flags::ENUM))
                    == 0
            {
                break;
            }
        }

        parts.reverse();
        Some(parts.join("."))
    }

    fn is_function_like_type(&mut self, ty: TypeId) -> bool {
        let resolved = self.resolve_type_for_property_access(ty);
        let evaluated = self.judge_evaluate(resolved);
        [ty, resolved, evaluated].into_iter().any(|candidate| {
            tsz_solver::type_queries::get_function_shape(self.ctx.types, candidate).is_some()
                || tsz_solver::type_queries::get_callable_shape(self.ctx.types, candidate)
                    .is_some_and(|s| !s.call_signatures.is_empty())
                || candidate == TypeId::FUNCTION
        })
    }

    pub(super) fn first_nonpublic_constructor_param_property(
        &mut self,
        ty: TypeId,
    ) -> Option<(String, MemberAccessLevel)> {
        let resolved = self.resolve_type_for_property_access(ty);
        let evaluated = self.judge_evaluate(resolved);
        let candidates = [ty, resolved, evaluated];

        let mut symbol_candidates: Vec<tsz_binder::SymbolId> = Vec::new();
        if let Some(sym) = candidates.into_iter().find_map(|candidate| {
            tsz_solver::type_queries::get_type_shape_symbol(self.ctx.types, candidate)
        }) {
            symbol_candidates.push(sym);
        }
        let ty_name = self.format_type_for_assignability_message(ty);
        let bare = ty_name.split('<').next().unwrap_or(&ty_name);
        let simple = bare.rsplit('.').next().unwrap_or(bare).trim();
        if !simple.is_empty() && !simple.starts_with('{') && !simple.contains(' ') {
            for sym in self.ctx.binder.get_symbols().find_all_by_name(simple) {
                if !symbol_candidates.contains(&sym) {
                    symbol_candidates.push(sym);
                }
            }
        }
        if symbol_candidates.is_empty() {
            return None;
        }

        for symbol_id in symbol_candidates {
            let Some(symbol) = self.ctx.binder.get_symbol(symbol_id) else {
                continue;
            };
            for &decl_idx in &symbol.declarations {
                let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                if decl_node.kind != syntax_kind_ext::CLASS_DECLARATION
                    && decl_node.kind != syntax_kind_ext::CLASS_EXPRESSION
                {
                    continue;
                }
                let Some(class) = self.ctx.arena.get_class(decl_node) else {
                    continue;
                };
                for &member_idx in &class.members.nodes {
                    let Some(member_node) = self.ctx.arena.get(member_idx) else {
                        continue;
                    };
                    if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                        continue;
                    }
                    let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                        continue;
                    };
                    for &param_idx in &ctor.parameters.nodes {
                        let Some(param_node) = self.ctx.arena.get(param_idx) else {
                            continue;
                        };
                        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                            continue;
                        };
                        let Some(level) = self.member_access_level_from_modifiers(&param.modifiers)
                        else {
                            continue;
                        };
                        let Some(name) = self.get_property_name(param.name) else {
                            continue;
                        };
                        return Some((name, level));
                    }
                }
            }
        }

        None
    }

    pub(super) fn missing_single_required_property(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<tsz_common::interner::Atom> {
        if tsz_solver::is_primitive_type(self.ctx.types, source) {
            return None;
        }

        let source_candidates = {
            let resolved = self.resolve_type_for_property_access(source);
            let evaluated = self.judge_evaluate(resolved);
            [source, resolved, evaluated]
        };
        let target_candidates = {
            let resolved = self.resolve_type_for_property_access(target);
            let evaluated = self.judge_evaluate(resolved);
            [target, resolved, evaluated]
        };

        let source_is_function_like = self.is_function_like_type(source);

        let target_name = self.format_type_for_assignability_message(target);
        if target_name == "Callable" || target_name == "Applicable" {
            let required_name = if target_name == "Callable" {
                "call"
            } else {
                "apply"
            };
            let required_atom = self.ctx.types.intern_string(required_name);
            let source_has_prop = if source_is_function_like {
                true
            } else {
                source_candidates.iter().any(|candidate| {
                    if let Some(source_callable) =
                        tsz_solver::type_queries::get_callable_shape(self.ctx.types, *candidate)
                    {
                        source_callable
                            .properties
                            .iter()
                            .any(|p| p.name == required_atom)
                    } else if let Some(source_shape) =
                        tsz_solver::type_queries::get_object_shape(self.ctx.types, *candidate)
                    {
                        source_shape
                            .properties
                            .iter()
                            .any(|p| p.name == required_atom)
                    } else {
                        false
                    }
                })
            };
            if !source_has_prop {
                return Some(required_atom);
            }
        }

        if !source_is_function_like {
            for target_candidate in target_candidates {
                let Some(target_callable) =
                    tsz_solver::type_queries::get_callable_shape(self.ctx.types, target_candidate)
                else {
                    continue;
                };
                let Some(sym_id) = target_callable.symbol else {
                    continue;
                };
                let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                    continue;
                };
                if symbol.escaped_name == "Callable" {
                    return Some(self.ctx.types.intern_string("call"));
                }
                if symbol.escaped_name == "Applicable" {
                    return Some(self.ctx.types.intern_string("apply"));
                }
            }
        }

        for target_candidate in target_candidates {
            if let Some(target_callable) =
                tsz_solver::type_queries::get_callable_shape(self.ctx.types, target_candidate)
            {
                let required_props: Vec<_> = target_callable
                    .properties
                    .iter()
                    .filter(|p| !p.optional)
                    .collect();
                if required_props.len() == 1 {
                    let prop = required_props[0];
                    let prop_name = self.ctx.types.resolve_atom_ref(prop.name);
                    if prop_name.as_ref() == "call" || prop_name.as_ref() == "apply" {
                        let source_has_prop = if source_is_function_like {
                            true
                        } else {
                            source_candidates.iter().any(|candidate| {
                                if let Some(source_callable) =
                                    tsz_solver::type_queries::get_callable_shape(
                                        self.ctx.types,
                                        *candidate,
                                    )
                                {
                                    source_callable
                                        .properties
                                        .iter()
                                        .any(|p| p.name == prop.name)
                                } else if let Some(source_shape) =
                                    tsz_solver::type_queries::get_object_shape(
                                        self.ctx.types,
                                        *candidate,
                                    )
                                {
                                    source_shape.properties.iter().any(|p| p.name == prop.name)
                                } else {
                                    false
                                }
                            })
                        };
                        if !source_has_prop {
                            return Some(prop.name);
                        }
                    }
                }
            }
        }

        let source_with_shape = {
            let direct = source;
            let resolved = self.resolve_type_for_property_access(direct);
            let evaluated = self.judge_evaluate(resolved);
            [direct, resolved, evaluated]
                .into_iter()
                .find(|candidate| {
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, *candidate).is_some()
                })?
        };
        let target_with_shape = {
            let direct = target;
            let resolved = self.resolve_type_for_property_access(direct);
            let evaluated = self.judge_evaluate(resolved);
            [direct, resolved, evaluated]
                .into_iter()
                .find(|candidate| {
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, *candidate).is_some()
                })?
        };

        let source_shape =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, source_with_shape)?;
        let target_shape =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, target_with_shape)?;

        if target_shape.string_index.is_some() || target_shape.number_index.is_some() {
            return None;
        }

        let required_props: Vec<_> = target_shape
            .properties
            .iter()
            .filter(|p| !p.optional)
            .collect();
        if required_props.len() != 1 {
            return None;
        }

        let prop = required_props[0];
        let source_has_prop = source_shape.properties.iter().any(|p| p.name == prop.name);
        if source_has_prop {
            return None;
        }

        Some(prop.name)
    }

    /// Walk up the AST to find the appropriate diagnostic anchor for assignment errors.
    ///
    /// For variable declarations, tsc anchors TS2322 at the variable **name** (e.g.,
    /// just `x` in `let x: string = 42`), not the full statement.
    /// For assignment expressions, tsc walks up to the expression statement level.
    pub(super) fn assignment_diagnostic_anchor_idx(&self, idx: NodeIndex) -> NodeIndex {
        let mut current = idx;
        let mut saw_assignment_binary = false;
        let mut var_decl: Option<NodeIndex> = None;

        while current.is_some() {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }

            let Some(parent_node) = self.ctx.arena.get(parent) else {
                break;
            };

            // Stop at scope boundaries — never walk through function-like nodes
            // or class bodies into the enclosing scope.
            if matches!(
                parent_node.kind,
                syntax_kind_ext::FUNCTION_DECLARATION
                    | syntax_kind_ext::FUNCTION_EXPRESSION
                    | syntax_kind_ext::ARROW_FUNCTION
                    | syntax_kind_ext::METHOD_DECLARATION
                    | syntax_kind_ext::CONSTRUCTOR
                    | syntax_kind_ext::GET_ACCESSOR
                    | syntax_kind_ext::SET_ACCESSOR
                    | syntax_kind_ext::CLASS_EXPRESSION
                    | syntax_kind_ext::CLASS_DECLARATION
            ) {
                break;
            }

            // Stop at JSX element boundaries — JSX attribute errors should anchor
            // at the attribute itself, not walk up to the enclosing variable
            // declaration or expression statement. tsc anchors JSX TS2322 errors
            // at the specific attribute (e.g., `x` in `<Foo x={3} />`), not at the
            // `let p = <Foo .../>` statement.
            if matches!(
                parent_node.kind,
                syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT | syntax_kind_ext::JSX_OPENING_ELEMENT
            ) {
                break;
            }

            if parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(binary) = self.ctx.arena.get_binary_expr(parent_node)
                && self.is_assignment_operator(binary.operator_token)
            {
                if saw_assignment_binary {
                    // Chained assignment: we've already passed through one assignment
                    // binary. Return the original node so inner assignments anchor at
                    // their own LHS (e.g. `b` in `a = b = null`) rather than walking
                    // all the way up to the ExpressionStatement.
                    return idx;
                }
                saw_assignment_binary = true;
            }

            if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                if saw_assignment_binary {
                    // The error comes from an inner assignment expression within a
                    // variable initializer (e.g. `const x = a = b;`). In tsc, the
                    // diagnostic anchors at the inner assignment target (`a`), not
                    // the entire variable statement.
                    return idx;
                }
                var_decl = Some(parent);
            }

            if parent_node.kind == syntax_kind_ext::VARIABLE_STATEMENT && var_decl.is_some() {
                // tsc anchors TS2322 at the variable name, not the full statement.
                // e.g. for `let x: string = 42`, tsc underlines just `x`.
                if let Some(vd_idx) = var_decl
                    && let Some(vd) = self.ctx.arena.get_variable_declaration_at(vd_idx)
                    && vd.name.is_some()
                {
                    return vd.name;
                }
                return parent; // fallback to old behavior if name unavailable
            }

            if parent_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT && saw_assignment_binary {
                return parent;
            }

            current = parent;
        }

        // When we walked through a variable declaration but didn't reach a
        // VariableStatement (e.g., for-in/for-of), anchor at the variable name.
        if let Some(vd_idx) = var_decl {
            if let Some(vd) = self.ctx.arena.get_variable_declaration_at(vd_idx)
                && vd.name.is_some()
            {
                return vd.name;
            }
            return vd_idx;
        }
        idx
    }

    // =========================================================================
    // Fundamental Error Emitters
    // =========================================================================

    /// Report an error at a specific node.
    pub(crate) fn error_at_node(&mut self, node_idx: NodeIndex, message: &str, code: u32) {
        if let Some((start, end)) = self.get_node_span(node_idx) {
            let length = end.saturating_sub(start);
            // Use the error() function which has deduplication by (start, code)
            self.error(start, length, message.to_string(), code);
        }
    }

    /// Emit a generator-related error (TS1221/TS1222) at the `*` asterisk token.
    ///
    /// TSC's `grammarErrorOnNode(node.asteriskToken!, ...)` anchors these errors
    /// at the asterisk, not the function/method node. Since our AST stores
    /// `asterisk_token` as a `bool` (not a node), we scan backward from the
    /// name node's position in source text to locate the `*`.
    pub(crate) fn emit_generator_error_at_asterisk(
        &mut self,
        name_idx: NodeIndex,
        fallback_idx: NodeIndex,
        message: &str,
        code: u32,
    ) {
        // Try to find the `*` by scanning backward from the name node's start position
        if let Some(name_node) = self.ctx.arena.get(name_idx)
            && let Some(sf) = self.ctx.arena.source_files.first()
        {
            let text = sf.text.as_bytes();
            let name_pos = name_node.pos as usize;
            // Scan backward from the name position to find `*`
            for i in (0..name_pos).rev() {
                match text.get(i) {
                    Some(b'*') => {
                        self.error_at_position(i as u32, 1, message, code);
                        return;
                    }
                    Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') => continue,
                    _ => break, // Hit a non-whitespace, non-asterisk char — give up
                }
            }
        }
        // Fallback: error at the entire node
        self.error_at_node(fallback_idx, message, code);
    }

    /// Emit a templated diagnostic error at a node.
    ///
    /// Looks up the message template for `code` via `get_message_template`,
    /// formats it with `args`, and emits the error at `node_idx`.
    /// Panics in debug mode if the code has no registered template.
    pub(crate) fn error_at_node_msg(&mut self, node_idx: NodeIndex, code: u32, args: &[&str]) {
        use tsz_common::diagnostics::get_message_template;
        let template = get_message_template(code).unwrap_or("Unexpected checker diagnostic code.");
        let message = format_message(template, args);
        self.error_at_node(node_idx, &message, code);
    }

    /// Report an error at a specific position.
    pub(crate) fn error_at_position(&mut self, start: u32, length: u32, message: &str, code: u32) {
        self.ctx.diagnostics.push(Diagnostic::error(
            self.ctx.file_name.clone(),
            start,
            length,
            message.to_string(),
            code,
        ));
    }

    /// Report an error at the current node being processed (from resolution stack).
    /// Falls back to the start of the file if no node is in the stack.
    pub(crate) fn error_at_current_node(&mut self, message: &str, code: u32) {
        // Try to use the last node in the resolution stack
        if let Some(&node_idx) = self.ctx.node_resolution_stack.last() {
            self.error_at_node(node_idx, message, code);
        } else {
            // No current node - emit at start of file
            self.error_at_position(0, 0, message, code);
        }
    }
}
