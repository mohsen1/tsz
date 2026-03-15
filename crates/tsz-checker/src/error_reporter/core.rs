//! Core error emission helpers and type formatting utilities.

use crate::diagnostics::diagnostic_codes;
use crate::query_boundaries::diagnostics as query;
use crate::state::{CheckerState, MemberAccessLevel};
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn param_matches_property_key_literal(&self, prop_name: Atom, ty: TypeId) -> bool {
        let prop_name = self.ctx.types.resolve_atom_ref(prop_name);
        if self.ctx.types.literal_string(prop_name.as_ref()) == ty {
            return true;
        }
        prop_name
            .parse::<f64>()
            .ok()
            .is_some_and(|num| self.ctx.types.literal_number(num) == ty)
    }

    fn normalize_excess_display_type_for_property(
        &self,
        prop_name: Option<Atom>,
        ty: TypeId,
    ) -> TypeId {
        let ty = self.normalize_excess_display_type(ty);
        let Some(prop_name) = prop_name else {
            return ty;
        };

        if let Some(shape) = query::function_shape(self.ctx.types, ty) {
            let params: Vec<_> = shape
                .params
                .iter()
                .map(|param| {
                    let normalized = self.normalize_excess_display_type(param.type_id);
                    let type_id = if self.param_matches_property_key_literal(prop_name, normalized)
                    {
                        normalized
                    } else {
                        tsz_solver::widen_literal_type(self.ctx.types, normalized)
                    };
                    tsz_solver::ParamInfo {
                        type_id,
                        ..param.clone()
                    }
                })
                .collect();

            if params.iter().zip(shape.params.iter()).all(|(a, b)| a == b) {
                ty
            } else {
                self.ctx
                    .types
                    .factory()
                    .function(tsz_solver::FunctionShape {
                        type_params: shape.type_params.clone(),
                        params,
                        this_type: shape.this_type,
                        return_type: shape.return_type,
                        type_predicate: shape.type_predicate.clone(),
                        is_constructor: shape.is_constructor,
                        is_method: shape.is_method,
                    })
            }
        } else {
            ty
        }
    }

    pub(super) fn widen_function_like_display_type(&mut self, type_id: TypeId) -> TypeId {
        let constructor_display_def = self
            .ctx
            .definition_store
            .find_def_for_type(type_id)
            .and_then(|def_id| {
                self.ctx
                    .definition_store
                    .get(def_id)
                    .filter(|def| matches!(def.kind, tsz_solver::def::DefKind::ClassConstructor))
                    .map(|_| def_id)
            });

        let type_id = self.evaluate_type_with_env(type_id);
        if tsz_solver::is_generic_application(self.ctx.types, type_id) {
            let widened = tsz_solver::operations::widening::widen_type(self.ctx.types, type_id);
            if let Some(def_id) = constructor_display_def {
                self.ctx
                    .definition_store
                    .register_type_to_def(widened, def_id);
            }
            return widened;
        }
        let type_id = self.resolve_type_for_property_access(type_id);
        let type_id = self.resolve_lazy_type(type_id);
        let type_id = self.evaluate_application_type(type_id);
        let widened = tsz_solver::operations::widening::widen_type(self.ctx.types, type_id);
        if let Some(def_id) = constructor_display_def {
            self.ctx
                .definition_store
                .register_type_to_def(widened, def_id);
        }
        widened
    }

    fn terminal_assignment_source_expression(&self, expr_idx: NodeIndex) -> NodeIndex {
        let mut current = expr_idx;
        let mut guard = 0;

        loop {
            guard += 1;
            if guard > 256 {
                return current;
            }

            let expr = self.ctx.arena.skip_parenthesized_and_assertions(current);
            let Some(node) = self.ctx.arena.get(expr) else {
                return current;
            };
            if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                return expr;
            }
            let Some(bin) = self.ctx.arena.get_binary_expr(node) else {
                return expr;
            };
            if !self.is_assignment_operator(bin.operator_token) {
                return expr;
            }
            current = bin.right;
        }
    }

    fn normalize_excess_display_type(&self, ty: TypeId) -> TypeId {
        let ty = tsz_solver::evaluate_type(self.ctx.types, ty);
        if let Some(app) = query::type_application(self.ctx.types, ty) {
            let args: Vec<_> = app
                .args
                .iter()
                .map(|&arg| self.normalize_excess_display_type(arg))
                .collect();
            if args == app.args {
                ty
            } else {
                self.ctx.types.factory().application(app.base, args)
            }
        } else if let Some(shape) = query::function_shape(self.ctx.types, ty) {
            let params: Vec<_> = shape
                .params
                .iter()
                .map(|param| tsz_solver::ParamInfo {
                    type_id: self.normalize_excess_display_type(param.type_id),
                    ..param.clone()
                })
                .collect();
            let return_type = self.normalize_excess_display_type(shape.return_type);
            if params.iter().zip(shape.params.iter()).all(|(a, b)| a == b)
                && return_type == shape.return_type
            {
                ty
            } else {
                self.ctx
                    .types
                    .factory()
                    .function(tsz_solver::FunctionShape {
                        type_params: shape.type_params.clone(),
                        params,
                        this_type: shape.this_type,
                        return_type,
                        type_predicate: shape.type_predicate.clone(),
                        is_constructor: shape.is_constructor,
                        is_method: shape.is_method,
                    })
            }
        } else if let Some(members) = query::union_members(self.ctx.types, ty) {
            self.ctx.types.factory().union_preserve_members(
                members
                    .iter()
                    .map(|&member| self.normalize_excess_display_type(member))
                    .collect(),
            )
        } else if let Some(members) = query::intersection_members(self.ctx.types, ty) {
            self.ctx.types.factory().intersection(
                members
                    .iter()
                    .map(|&member| self.normalize_excess_display_type(member))
                    .collect(),
            )
        } else {
            ty
        }
    }

    fn normalize_assignability_display_type(&mut self, ty: TypeId) -> TypeId {
        let ty = self
            .materialize_finite_mapped_type_for_display(ty)
            .unwrap_or(ty);
        let ty = if tsz_solver::type_queries::is_index_access_type(self.ctx.types, ty)
            && tsz_solver::type_queries::contains_type_parameters_db(self.ctx.types, ty)
        {
            ty
        } else {
            tsz_solver::evaluate_type(self.ctx.types, ty)
        };

        if let Some(app) = query::type_application(self.ctx.types, ty) {
            let args: Vec<_> = app
                .args
                .iter()
                .map(|&arg| self.normalize_assignability_display_type(arg))
                .collect();
            if args == app.args {
                ty
            } else {
                self.ctx.types.factory().application(app.base, args)
            }
        } else if let Some(shape) = query::function_shape(self.ctx.types, ty) {
            let params: Vec<_> = shape
                .params
                .iter()
                .map(|param| tsz_solver::ParamInfo {
                    type_id: self.normalize_assignability_display_type(param.type_id),
                    ..param.clone()
                })
                .collect();
            let return_type = self.normalize_assignability_display_type(shape.return_type);
            if params.iter().zip(shape.params.iter()).all(|(a, b)| a == b)
                && return_type == shape.return_type
            {
                ty
            } else {
                self.ctx
                    .types
                    .factory()
                    .function(tsz_solver::FunctionShape {
                        type_params: shape.type_params.clone(),
                        params,
                        this_type: shape.this_type,
                        return_type,
                        type_predicate: shape.type_predicate.clone(),
                        is_constructor: shape.is_constructor,
                        is_method: shape.is_method,
                    })
            }
        } else if let Some(shape) = tsz_solver::type_queries::get_object_shape(self.ctx.types, ty) {
            let mut shape = shape.as_ref().clone();
            let mut changed = false;
            for prop in &mut shape.properties {
                let normalized_read = self.normalize_assignability_display_type(prop.type_id);
                let normalized_write = self.normalize_assignability_display_type(prop.write_type);
                let stripped_read = normalized_read;
                let stripped_write = normalized_write;
                changed |= stripped_read != prop.type_id || stripped_write != prop.write_type;
                prop.type_id = stripped_read;
                prop.write_type = stripped_write;
            }
            if let Some(index) = shape.string_index.as_mut() {
                let normalized = self.normalize_assignability_display_type(index.value_type);
                changed |= normalized != index.value_type;
                index.value_type = normalized;
            }
            if let Some(index) = shape.number_index.as_mut() {
                let normalized = self.normalize_assignability_display_type(index.value_type);
                changed |= normalized != index.value_type;
                index.value_type = normalized;
            }
            if changed {
                self.ctx.types.factory().object_with_index(shape)
            } else {
                ty
            }
        } else if let Some(members) = query::union_members(self.ctx.types, ty) {
            self.ctx.types.factory().union_preserve_members(
                members
                    .iter()
                    .map(|&member| self.normalize_assignability_display_type(member))
                    .collect(),
            )
        } else if let Some(members) = query::intersection_members(self.ctx.types, ty) {
            self.ctx.types.factory().intersection(
                members
                    .iter()
                    .map(|&member| self.normalize_assignability_display_type(member))
                    .collect(),
            )
        } else {
            ty
        }
    }

    fn split_optional_object_for_excess_display(&self, ty: TypeId) -> TypeId {
        let ty = tsz_solver::evaluate_type(self.ctx.types, ty);
        if let Some(members) = query::union_members(self.ctx.types, ty) {
            let non_undefined: Vec<_> = members
                .iter()
                .copied()
                .filter(|member| *member != TypeId::UNDEFINED)
                .collect();
            if non_undefined.len() == 1 && non_undefined.len() != members.len() {
                return non_undefined[0];
            }
        }
        ty
    }

    /// For TS2353 diagnostics on union targets, strip non-object members (primitives,
    /// undefined, null, void, never, etc.) so the displayed type matches tsc.
    /// For example, `IProps | number` becomes `IProps`, and
    /// `{ testBool?: boolean | undefined; } | undefined` becomes `{ testBool?: boolean | undefined; }`.
    fn strip_non_object_union_members_for_excess_display(&self, ty: TypeId) -> TypeId {
        let ty = tsz_solver::evaluate_type(self.ctx.types, ty);
        if let Some(members) = query::union_members(self.ctx.types, ty) {
            let object_like: Vec<_> = members
                .iter()
                .copied()
                .filter(|member| {
                    let evaluated = tsz_solver::evaluate_type(self.ctx.types, *member);
                    !tsz_solver::is_primitive_type(self.ctx.types, evaluated)
                })
                .collect();
            // Only strip if we actually removed something and have at least one member left
            if !object_like.is_empty() && object_like.len() < members.len() {
                if object_like.len() == 1 {
                    return object_like[0];
                }
                return tsz_solver::utils::union_or_single(self.ctx.types, object_like);
            }
        }
        ty
    }

    fn split_wildcard_object_for_excess_display(&mut self, ty: TypeId) -> Option<String> {
        let ty = self
            .materialize_finite_mapped_type_for_display(ty)
            .unwrap_or(ty);
        let ty = self.split_optional_object_for_excess_display(ty);
        let shape = tsz_solver::type_queries::get_object_shape(self.ctx.types, ty)?;
        if shape.string_index.is_some() || shape.number_index.is_some() {
            return None;
        }

        let wildcard_name = self.ctx.types.intern_string("*");
        let mut wildcard_props = Vec::new();
        let mut named_props = Vec::new();

        for prop in &shape.properties {
            let mut cloned = prop.clone();
            cloned.type_id =
                self.normalize_excess_display_type_for_property(Some(cloned.name), cloned.type_id);
            cloned.write_type = self
                .normalize_excess_display_type_for_property(Some(cloned.name), cloned.write_type);
            if cloned.name == wildcard_name {
                wildcard_props.push(cloned);
            } else {
                named_props.push(cloned);
            }
        }

        if wildcard_props.is_empty() || named_props.is_empty() {
            return None;
        }

        let named_obj = self.ctx.types.factory().object(named_props);
        let wildcard_obj = self.ctx.types.factory().object(wildcard_props);
        Some(format!(
            "{} & {}",
            self.format_type_diagnostic(named_obj),
            self.format_type_diagnostic(wildcard_obj)
        ))
    }

    fn materialize_finite_mapped_type_for_display(&mut self, ty: TypeId) -> Option<TypeId> {
        if let Some((mapped_id, mapped)) = query::mapped_type(self.ctx.types, ty) {
            let names =
                crate::query_boundaries::state::checking::collect_finite_mapped_property_names(
                    self.ctx.types,
                    mapped_id,
                )?;
            let mut names: Vec<_> = names.into_iter().collect();
            names.sort_by(|a, b| {
                self.ctx
                    .types
                    .resolve_atom_ref(*a)
                    .cmp(&self.ctx.types.resolve_atom_ref(*b))
            });

            let mut properties = Vec::with_capacity(names.len());
            for name in names {
                let property_name = self.ctx.types.resolve_atom_ref(name).to_string();
                let type_id =
                    crate::query_boundaries::state::checking::get_finite_mapped_property_type(
                        self.ctx.types,
                        mapped_id,
                        &property_name,
                    )?;
                let type_id = self.normalize_excess_display_type_for_property(Some(name), type_id);
                let mut property = tsz_solver::PropertyInfo::new(name, type_id);
                property.optional =
                    mapped.optional_modifier == Some(tsz_solver::MappedModifier::Add);
                property.readonly =
                    mapped.readonly_modifier == Some(tsz_solver::MappedModifier::Add);
                properties.push(property);
            }

            Some(self.ctx.types.factory().object(properties))
        } else if let Some(members) = query::intersection_members(self.ctx.types, ty) {
            let mut changed = false;
            let remapped: Vec<_> = members
                .iter()
                .map(|&member| {
                    if let Some(materialized) =
                        self.materialize_finite_mapped_type_for_display(member)
                    {
                        changed = true;
                        materialized
                    } else {
                        member
                    }
                })
                .collect();
            changed.then(|| self.ctx.types.factory().intersection(remapped))
        } else if let Some(members) = query::union_members(self.ctx.types, ty) {
            let mut changed = false;
            let remapped: Vec<_> = members
                .iter()
                .map(|&member| {
                    if let Some(materialized) =
                        self.materialize_finite_mapped_type_for_display(member)
                    {
                        changed = true;
                        materialized
                    } else {
                        member
                    }
                })
                .collect();
            changed.then(|| self.ctx.types.factory().union(remapped))
        } else {
            None
        }
    }

    pub(crate) fn format_excess_property_target_type(&mut self, ty: TypeId) -> String {
        if let Some(display) = self.split_wildcard_object_for_excess_display(ty) {
            return display;
        }

        // For union targets, tsc strips non-object members (primitives like number,
        // undefined, null, etc.) from the displayed type. Excess property checking
        // only applies to object-like members, so the diagnostic should reference
        // only those members rather than the full union.
        let ty = self.strip_non_object_union_members_for_excess_display(ty);

        if let Some(members) = query::intersection_members(self.ctx.types, ty) {
            let mut changed = false;
            let parts: Vec<String> = members
                .iter()
                .map(|&member| {
                    if let Some(materialized) =
                        self.materialize_finite_mapped_type_for_display(member)
                    {
                        changed = true;
                        self.format_type_diagnostic(materialized)
                    } else {
                        self.format_type_diagnostic(member)
                    }
                })
                .collect();
            if changed {
                return parts.join(" & ");
            }
        }

        let display_ty = self
            .materialize_finite_mapped_type_for_display(ty)
            .unwrap_or(ty);
        self.format_type_diagnostic(display_ty)
    }

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
        let mut formatted = text.trim().to_string();
        if formatted.starts_with("{ ")
            && formatted.ends_with(" }")
            && formatted.contains(':')
            && !formatted.ends_with("; }")
        {
            formatted = format!("{}; }}", &formatted[..formatted.len() - 2]);
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

        if parts.is_empty() {
            return Some("{}".to_string());
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
                let escaped = lit
                    .text
                    .replace('\\', "\\\\")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
                    .replace('\t', "\\t");
                Some(format!("\"{escaped}\""))
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

    pub(super) fn assignment_source_expression(&self, anchor_idx: NodeIndex) -> Option<NodeIndex> {
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
                        return Some(self.terminal_assignment_source_expression(bin.right));
                    }
                }
                k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                    let stmt = self.ctx.arena.get_expression_statement(node)?;
                    let expr = self.ctx.arena.get(stmt.expression)?;
                    let bin = self.ctx.arena.get_binary_expr(expr)?;
                    return self
                        .is_assignment_operator(bin.operator_token)
                        .then_some(self.terminal_assignment_source_expression(bin.right));
                }
                k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                    let decl = self.ctx.arena.get_variable_declaration(node)?;
                    return decl
                        .initializer
                        .is_some()
                        .then_some(self.terminal_assignment_source_expression(decl.initializer));
                }
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let prop = self.ctx.arena.get_property_assignment(node)?;
                    return prop.initializer.is_some().then_some(prop.initializer);
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    let prop = self.ctx.arena.get_shorthand_property(node)?;
                    return prop.name.is_some().then_some(prop.name);
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

    pub(crate) fn assignment_source_is_return_expression(&self, anchor_idx: NodeIndex) -> bool {
        let mut current = anchor_idx;
        let mut guard = 0;

        while current.is_some() {
            guard += 1;
            if guard > 256 {
                break;
            }

            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            if node.kind == syntax_kind_ext::RETURN_STATEMENT {
                return true;
            }

            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        false
    }

    fn is_property_assignment_initializer(&self, anchor_idx: NodeIndex) -> bool {
        let current = self.ctx.arena.skip_parenthesized_and_assertions(anchor_idx);
        let Some(ext) = self.ctx.arena.get_extended(current) else {
            return false;
        };
        let parent_idx = ext.parent;
        let Some(parent) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        parent.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
            && self
                .ctx
                .arena
                .get_property_assignment(parent)
                .is_some_and(|prop| prop.initializer == current)
    }

    fn direct_diagnostic_source_expression(&self, anchor_idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(anchor_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(node)
            && self.is_assignment_operator(binary.operator_token)
        {
            return None;
        }
        let is_expression_like = matches!(
            node.kind,
            k if k == SyntaxKind::Identifier as u16
                || k == SyntaxKind::ThisKeyword as u16
                || k == SyntaxKind::SuperKeyword as u16
                || k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::RegularExpressionLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                || k == syntax_kind_ext::CALL_EXPRESSION
                || k == syntax_kind_ext::NEW_EXPRESSION
                || k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION
                || k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::BINARY_EXPRESSION
                || k == syntax_kind_ext::CONDITIONAL_EXPRESSION
                || k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
                || k == syntax_kind_ext::NON_NULL_EXPRESSION
                || k == syntax_kind_ext::AWAIT_EXPRESSION
                || k == syntax_kind_ext::YIELD_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::CLASS_EXPRESSION
                || k == syntax_kind_ext::TEMPLATE_EXPRESSION
        );
        if !is_expression_like {
            return None;
        }

        let parent_idx = self.ctx.arena.get_extended(expr_idx)?.parent;
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return Some(expr_idx);
        };

        if parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.ctx.arena.get_binary_expr(parent_node)
            && self.is_assignment_operator(bin.operator_token)
            && bin.left == expr_idx
        {
            return None;
        }

        if parent_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
            && let Some(prop) = self.ctx.arena.get_property_assignment(parent_node)
            && prop.name == expr_idx
        {
            return None;
        }

        if parent_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
            && let Some(prop) = self.ctx.arena.get_shorthand_property(parent_node)
            && prop.name == expr_idx
        {
            return None;
        }

        // Class property declaration names are not source expressions.
        // When TS2322 is anchored at the property name (e.g., `y` in `y: string = 42`),
        // the source expression is the initializer, not the name identifier.
        // Without this guard, get_type_of_node on the name triggers identifier
        // resolution → TS2304 "Cannot find name" false positive.
        if parent_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
            && let Some(prop) = self.ctx.arena.get_property_decl(parent_node)
            && prop.name == expr_idx
        {
            return None;
        }

        // JSX attribute names are not source expressions.
        // When TS2322 is anchored at an attribute name (e.g., `x` in `<Comp x={10} />`),
        // the error reporter must not call get_type_of_node on the attribute name
        // identifier, which would trigger TS2304 "Cannot find name".
        if parent_node.kind == syntax_kind_ext::JSX_ATTRIBUTE
            && let Some(attr) = self.ctx.arena.get_jsx_attribute(parent_node)
            && attr.name == expr_idx
        {
            return None;
        }

        Some(expr_idx)
    }

    fn declared_type_annotation_text_for_expression_with_options(
        &self,
        expr_idx: NodeIndex,
        allow_object_shapes: bool,
    ) -> Option<String> {
        fn sanitize_type_annotation_text(
            text: String,
            allow_object_shapes: bool,
        ) -> Option<String> {
            let mut text = text.trim().trim_start_matches(':').trim().to_string();
            // If the extracted text contains a newline, take only the first line —
            // the node span may extend past the declaration.
            if let Some(nl) = text.find('\n') {
                text = text[..nl].trim_end().to_string();
            }
            // Strip trailing `=` that leaks from variable initializers
            // (e.g. `string | number = ""` → `string | number`).
            if text.ends_with('=') {
                text.pop();
                text = text.trim_end().to_string();
            }
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
            if !allow_object_shapes && (text.starts_with('{') || text.starts_with('[')) {
                return None;
            }
            let open_count = text.chars().filter(|&ch| ch == '(').count();
            let close_count = text.chars().filter(|&ch| ch == ')').count();
            if open_count != close_count {
                return None;
            }
            if text.is_empty() {
                return None;
            }
            // TypeScript `&` binds tighter than `|`. Add precedence parentheses
            // so annotation text like `A & B | C & D` becomes `(A & B) | (C & D)`.
            if text.contains(" & ") && text.contains(" | ") {
                text = parenthesize_intersection_in_union_text(&text);
            }
            Some(text)
        }

        /// Add parentheses around `&`-joined groups in a top-level `|` union.
        fn parenthesize_intersection_in_union_text(text: &str) -> String {
            let mut parts = Vec::new();
            let mut current = String::new();
            let mut depth = 0u32;

            for (i, ch) in text.char_indices() {
                match ch {
                    '(' | '<' | '[' => {
                        depth += 1;
                        current.push(ch);
                    }
                    ')' | '>' | ']' => {
                        depth = depth.saturating_sub(1);
                        current.push(ch);
                    }
                    '|' if depth == 0
                        && text.get(i.saturating_sub(1)..i) == Some(" ")
                        && text.get(i + 1..i + 2) == Some(" ") =>
                    {
                        parts.push(current.trim().to_string());
                        current = String::new();
                    }
                    _ => {
                        current.push(ch);
                    }
                }
            }
            parts.push(current.trim().to_string());

            let formatted: Vec<String> = parts
                .into_iter()
                .map(|part| {
                    if part.contains(" & ") && !part.starts_with('(') {
                        format!("({part})")
                    } else {
                        part
                    }
                })
                .collect();
            formatted.join(" | ")
        }

        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }

        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let mut declarations = Vec::new();
        if let Some(decl) = self.ctx.arena.get(symbol.value_declaration) {
            declarations.push(decl);
        }
        for decl_idx in &symbol.declarations {
            if let Some(decl) = self.ctx.arena.get(*decl_idx)
                && !declarations
                    .iter()
                    .any(|existing| existing.pos == decl.pos && existing.end == decl.end)
            {
                declarations.push(decl);
            }
        }

        for decl in declarations {
            if let Some(param) = self.ctx.arena.get_parameter(decl)
                && param.type_annotation.is_some()
            {
                let mut text = self
                    .node_text(param.type_annotation)
                    .and_then(|text| sanitize_type_annotation_text(text, allow_object_shapes))?;
                if param.question_token
                    && self.ctx.strict_null_checks()
                    && !text.contains("undefined")
                {
                    if text.contains("=>") {
                        text = format!("({text}) | undefined");
                    } else {
                        text.push_str(" | undefined");
                    }
                }
                return Some(text);
            }

            if let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl)
                && var_decl.type_annotation.is_some()
            {
                return self
                    .node_text(var_decl.type_annotation)
                    .and_then(|text| sanitize_type_annotation_text(text, allow_object_shapes));
            }
        }

        None
    }

    pub(crate) fn declared_type_annotation_text_for_expression(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        self.declared_type_annotation_text_for_expression_with_options(expr_idx, false)
    }

    fn declared_diagnostic_source_annotation_text(&self, expr_idx: NodeIndex) -> Option<String> {
        self.declared_type_annotation_text_for_expression_with_options(expr_idx, true)
    }

    fn should_prefer_declared_source_annotation_display(
        &mut self,
        expr_idx: NodeIndex,
        expr_type: TypeId,
        annotation_text: &str,
    ) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return false;
        }

        let display_type =
            self.widen_function_like_display_type(self.widen_type_for_display(expr_type));
        let formatted = self.format_type_for_assignability_message(display_type);
        let resolved = self.resolve_type_for_property_access(display_type);
        let evaluated = self.judge_evaluate(resolved);
        let resolver =
            tsz_solver::objects::index_signatures::IndexSignatureResolver::new(self.ctx.types);
        let has_index_signature = resolver.has_index_signature(
            evaluated,
            tsz_solver::objects::index_signatures::IndexKind::String,
        ) || resolver.has_index_signature(
            evaluated,
            tsz_solver::objects::index_signatures::IndexKind::Number,
        );
        if !formatted.starts_with('{') && !has_index_signature {
            return false;
        }

        let annotation = annotation_text.trim();
        annotation.contains('&') || !annotation.starts_with('{')
    }

    fn format_declared_annotation_for_diagnostic(&self, annotation_text: &str) -> String {
        let mut formatted = annotation_text.trim().to_string();
        if formatted.contains(':') {
            formatted = formatted.replace(" }", "; }");
        }
        formatted
    }

    pub(crate) fn format_type_diagnostic_structural(&self, ty: TypeId) -> String {
        let mut formatter = self.ctx.create_diagnostic_type_formatter();
        formatter.format(ty).into_owned()
    }

    fn synthesized_object_parent_display_name(&self, ty: TypeId) -> Option<String> {
        use tsz_binder::symbol_flags;
        use tsz_solver::type_queries::get_object_shape_id;

        let shape_id = get_object_shape_id(self.ctx.types, ty)?;
        let shape = self.ctx.types.object_shape(shape_id);
        let has_js_ctor_brand = shape.properties.iter().any(|prop| {
            self.ctx
                .types
                .resolve_atom_ref(prop.name)
                .starts_with("__js_ctor_brand_")
        });
        let mut parent_ids = shape.properties.iter().filter_map(|prop| prop.parent_id);
        let parent_sym = parent_ids.next()?;
        if parent_ids.any(|other| other != parent_sym) {
            return None;
        }

        let symbol = self.get_cross_file_symbol(parent_sym)?;
        if !has_js_ctor_brand
            && (symbol.flags & (symbol_flags::FUNCTION | symbol_flags::CLASS)) == 0
        {
            return None;
        }

        Some(symbol.escaped_name.clone())
    }

    pub(crate) fn format_property_receiver_type_for_diagnostic(&self, ty: TypeId) -> String {
        if let Some(name) = self.synthesized_object_parent_display_name(ty) {
            return name;
        }
        if self.ctx.definition_store.find_def_for_type(ty).is_none()
            && self
                .ctx
                .definition_store
                .find_type_alias_by_body(ty)
                .is_some()
        {
            return self.format_type_diagnostic_structural(ty);
        }
        self.format_type_diagnostic(ty)
    }

    fn jsdoc_annotated_expression_display(
        &mut self,
        expr_idx: NodeIndex,
        target: TypeId,
    ) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = expr_idx;
        loop {
            if self
                .ctx
                .arena
                .node_info(current)
                .and_then(|info| self.ctx.arena.get(info.parent))
                .is_some_and(|parent| {
                    matches!(
                        parent.kind,
                        syntax_kind_ext::PROPERTY_ASSIGNMENT
                            | syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
                            | syntax_kind_ext::METHOD_DECLARATION
                            | syntax_kind_ext::GET_ACCESSOR
                            | syntax_kind_ext::SET_ACCESSOR
                    )
                })
            {
                return None;
            }
            if let Some(type_id) = self.jsdoc_type_annotation_for_node_direct(current) {
                let display_type = self.widen_function_like_display_type(type_id);
                return Some(self.format_assignability_type_for_message(display_type, target));
            }

            let node = self.ctx.arena.get(current)?;
            if node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                return None;
            }

            let paren = self.ctx.arena.get_parenthesized(node)?;
            current = paren.expression;
        }
    }

    fn empty_array_literal_source_type_display(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }
        let literal = self.ctx.arena.get_literal_expr(node)?;
        if !literal.elements.nodes.is_empty() {
            return None;
        }
        Some(if self.ctx.strict_null_checks() {
            "never[]".to_string()
        } else {
            "undefined[]".to_string()
        })
    }

    fn object_literal_source_type_display(&mut self, expr_idx: NodeIndex) -> Option<String> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }

        let literal = self.ctx.arena.get_literal_expr(node)?;
        let mut parts = Vec::new();
        for child_idx in literal.elements.nodes.iter().copied() {
            let child = self.ctx.arena.get(child_idx)?;
            let prop = self.ctx.arena.get_property_assignment(child)?;
            let name_node = self.ctx.arena.get(prop.name)?;
            let display_name = match name_node.kind {
                k if k == tsz_scanner::SyntaxKind::Identifier as u16 => self
                    .ctx
                    .arena
                    .get_identifier(name_node)?
                    .escaped_text
                    .clone(),
                k if k == tsz_scanner::SyntaxKind::StringLiteral as u16
                    || k == tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
                {
                    let lit = self.ctx.arena.get_literal(name_node)?;
                    format!("\"{}\"", lit.text)
                }
                k if k == tsz_scanner::SyntaxKind::NumericLiteral as u16 => {
                    self.ctx.arena.get_literal(name_node)?.text.clone()
                }
                _ => return None,
            };
            let value_type = self.get_type_of_node(prop.initializer);
            if value_type == TypeId::ERROR {
                return None;
            }

            // For display purposes, preserve literal types from the AST.
            // tsc shows `{ x: "hello" }` (literal) in error messages even though
            // the property type is widened to `string` for type checking.
            let display_value = if let Some(init_node) = self.ctx.arena.get(prop.initializer) {
                match init_node.kind {
                    k if k == tsz_scanner::SyntaxKind::StringLiteral as u16
                        || k == tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
                    {
                        self.ctx
                            .arena
                            .get_literal(init_node)
                            .map(|lit| format!("\"{}\"", lit.text))
                    }
                    _ => None,
                }
            } else {
                None
            };

            let value_display = display_value.unwrap_or_else(|| {
                self.format_type_for_assignability_message(self.widen_type_for_display(value_type))
            });
            parts.push(format!("{display_name}: {value_display}"));
        }

        if parts.is_empty() {
            return Some("{}".to_string());
        }

        Some(format!("{{ {}; }}", parts.join("; ")))
    }

    pub(super) fn format_assignment_source_type_for_diagnostic(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> String {
        if source == TypeId::UNDEFINED
            && self.ctx.arena.get(anchor_idx).is_some_and(|node| {
                node.kind == tsz_parser::parser::syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
            })
        {
            return self.format_assignability_type_for_message(source, target);
        }

        if let Some(display) = self.jsdoc_annotated_expression_display(anchor_idx, target) {
            return display;
        }

        if self.is_literal_sensitive_assignment_target(target)
            && let Some(display) = self.literal_expression_display(anchor_idx)
        {
            return display;
        }

        if let Some(expr_idx) = self.direct_diagnostic_source_expression(anchor_idx) {
            if self.is_literal_sensitive_assignment_target(target)
                && let Some(display) = self.literal_expression_display(expr_idx)
            {
                return display;
            }

            if let Some(display) = self.empty_array_literal_source_type_display(expr_idx) {
                return display;
            }

            if let Some(display) = self.object_literal_source_type_display(expr_idx) {
                return display;
            }

            let expr_type = self.get_type_of_node(expr_idx);
            // Only use the node-derived type when it plausibly represents the
            // source of the assignment, not the target.  For-of loops pass the
            // element type as `source` but anchor the diagnostic at the loop
            // variable whose node type equals the *target* (declared variable
            // type), not the source.  When the node type matches the target but
            // not the source, the anchor is the assignment target — skip
            // node-based resolution to avoid confusing "Type 'X' is not
            // assignable to type 'X'" messages.
            let node_is_target_not_source = expr_type == target && expr_type != source;
            let node_type_matches_source = expr_type != TypeId::ERROR && !node_is_target_not_source;
            if node_type_matches_source {
                if let Some(annotation_text) =
                    self.declared_diagnostic_source_annotation_text(expr_idx)
                    && self.should_prefer_declared_source_annotation_display(
                        expr_idx,
                        expr_type,
                        &annotation_text,
                    )
                {
                    return self.format_declared_annotation_for_diagnostic(&annotation_text);
                }
                let display_type =
                    if self.should_widen_enum_member_assignment_source(expr_type, target) {
                        self.widen_enum_member_type(expr_type)
                    } else {
                        expr_type
                    };
                let display_type = self.widen_function_like_display_type(display_type);
                let display_type =
                    if tsz_solver::keyof_inner_type(self.ctx.types, display_type).is_some() {
                        let evaluated = self.evaluate_type_for_assignability(display_type);
                        tsz_solver::widening::widen_type(self.ctx.types, evaluated)
                    } else {
                        tsz_solver::widening::widen_type(self.ctx.types, display_type)
                    };
                return self.format_assignability_type_for_message(display_type, target);
            }

            if node_type_matches_source {
                if let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx) {
                    return display;
                }
            }
        }

        if let Some(expr_idx) = self.assignment_source_expression(anchor_idx) {
            if let Some(display) = self.literal_expression_display(expr_idx)
                && (self.is_literal_sensitive_assignment_target(target)
                    || (self.assignment_source_is_return_expression(anchor_idx)
                        && tsz_solver::type_queries::contains_type_parameters_db(
                            self.ctx.types,
                            target,
                        )
                        && !self.is_property_assignment_initializer(expr_idx)))
            {
                return display;
            }

            if let Some(display) = self.empty_array_literal_source_type_display(expr_idx) {
                return display;
            }

            if let Some(display) = self.object_literal_source_type_display(expr_idx) {
                return display;
            }

            let expr_type = self.get_type_of_node(expr_idx);
            if expr_type != TypeId::ERROR
                && let Some(annotation_text) =
                    self.declared_diagnostic_source_annotation_text(expr_idx)
                && self.should_prefer_declared_source_annotation_display(
                    expr_idx,
                    expr_type,
                    &annotation_text,
                )
            {
                return self.format_declared_annotation_for_diagnostic(&annotation_text);
            }
            let display_type = if expr_type != TypeId::ERROR {
                let widened_expr_type = self.widen_type_for_display(expr_type);
                if self.should_widen_enum_member_assignment_source(widened_expr_type, target) {
                    self.widen_enum_member_type(widened_expr_type)
                } else {
                    widened_expr_type
                }
            } else {
                source
            };
            let display_type = self.widen_function_like_display_type(display_type);

            if let Some(sym_id) = self.resolve_identifier_symbol(expr_idx)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && (symbol.flags & tsz_binder::symbol_flags::ENUM) != 0
                && (symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER) == 0
            {
                return self.format_assignability_type_for_message(display_type, target);
            }

            if expr_type == TypeId::ERROR
                && let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx)
            {
                return display;
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
                // Don't use annotation text for union types — the TypeFormatter
                // reorders null/undefined to the end to match tsc's display.
                // Annotation text preserves the user's original order which
                // differs from tsc's canonical display.
                && !display.contains(" | ")
            {
                if tsz_solver::type_queries::get_enum_def_id(self.ctx.types, display_type).is_some()
                {
                    return self.format_assignability_type_for_message(display_type, target);
                }
                return self.format_annotation_like_type(&display);
            }
            return formatted;
        }

        self.format_assignability_type_for_message(source, target)
    }

    pub(super) fn format_nested_assignment_source_type_for_diagnostic(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> String {
        if let Some(expr_idx) = self.direct_diagnostic_source_expression(anchor_idx) {
            if let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx) {
                return display;
            }

            if let Some(display) = self.empty_array_literal_source_type_display(expr_idx) {
                return display;
            }

            if let Some(display) = self.object_literal_source_type_display(expr_idx) {
                return display;
            }

            let expr_type = self.get_type_of_node(expr_idx);
            if expr_type != TypeId::ERROR {
                let widened_expr_type = self.widen_type_for_display(expr_type);
                let display_type =
                    if self.should_widen_enum_member_assignment_source(widened_expr_type, target) {
                        self.widen_enum_member_type(widened_expr_type)
                    } else {
                        widened_expr_type
                    };
                let display_type = self.widen_function_like_display_type(display_type);
                return self.format_assignability_type_for_message(display_type, target);
            }
        }

        if let Some(expr_idx) = self.assignment_source_expression(anchor_idx) {
            if let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx) {
                return display;
            }

            if let Some(display) = self.empty_array_literal_source_type_display(expr_idx) {
                return display;
            }

            if let Some(display) = self.object_literal_source_type_display(expr_idx) {
                return display;
            }

            let expr_type = self.get_type_of_node(expr_idx);
            let display_type = if expr_type != TypeId::ERROR {
                let widened_expr_type = self.widen_type_for_display(expr_type);
                if self.should_widen_enum_member_assignment_source(widened_expr_type, target) {
                    self.widen_enum_member_type(widened_expr_type)
                } else {
                    widened_expr_type
                }
            } else {
                source
            };
            let display_type = self.widen_function_like_display_type(display_type);
            return self.format_assignability_type_for_message(display_type, target);
        }

        self.format_assignability_type_for_message(source, target)
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
        // Template literal types (e.g., `:${string}:`) expect specific string
        // patterns — preserving the source literal in the diagnostic is more
        // informative than showing widened `string`.
        if tsz_solver::is_template_literal_type(self.ctx.types, target) {
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

    pub(crate) fn has_diagnostic_code_within_span(&self, start: u32, end: u32, code: u32) -> bool {
        self.ctx
            .diagnostics
            .iter()
            .any(|diag| diag.code == code && diag.start >= start && diag.start < end)
    }

    pub(crate) fn format_type_for_assignability_message(&mut self, ty: TypeId) -> String {
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

        let display_ty = self.normalize_assignability_display_type(ty);
        let mut formatted = self.format_type_diagnostic(display_ty);

        // Preserve generic instantiations for nominal class instance names when possible.
        if !formatted.contains('<')
            && let Some(shape) =
                tsz_solver::type_queries::get_object_shape(self.ctx.types, display_ty)
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
                    // Recover instantiation args from actual value-carrying members first.
                    // Methods tend to encode `T` as `() => T`, which produces noisy
                    // displays like `C<() => string>` instead of `C<string>`.
                    let build_candidates = |predicate: fn(&tsz_solver::PropertyInfo) -> bool| {
                        let mut candidates: Vec<(String, TypeId)> = shape
                            .properties
                            .iter()
                            .filter(|prop| predicate(prop))
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
                        candidates
                    };
                    let mut candidates =
                        build_candidates(|prop| !prop.is_method && !prop.is_class_prototype);
                    if candidates.len() < type_param_count {
                        candidates = build_candidates(|prop| !prop.is_method);
                    }
                    if candidates.len() < type_param_count {
                        candidates = build_candidates(|prop| !prop.is_class_prototype);
                    }
                    if candidates.len() < type_param_count {
                        candidates = build_candidates(|_| true);
                    }
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
            formatted = format!("{}; }}", &formatted[..formatted.len() - 2]);
        }
        formatted
    }

    pub(crate) fn format_assignability_type_for_message(
        &mut self,
        ty: TypeId,
        other: TypeId,
    ) -> String {
        if let Some(enum_name) = self.format_disambiguated_enum_name_for_assignment(ty, other) {
            return enum_name;
        }
        self.format_type_for_assignability_message(ty)
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
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let mut parts = vec![symbol.escaped_name.clone()];
        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol.declarations.first().copied()?
        };
        let mut current = self.ctx.arena.get_extended(decl_idx)?.parent;

        while current.is_some() {
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module_decl) = self.ctx.arena.get_module(node)
                && let Some(name) = self.ctx.arena.get_identifier_text(module_decl.name)
            {
                parts.push(name.to_string());
            }

            current = self.ctx.arena.get_extended(current)?.parent;
        }

        if parts.len() == 1 {
            let mut current = symbol.parent;
            while current != tsz_binder::SymbolId::NONE {
                let parent = self.ctx.binder.get_symbol(current)?;
                if (parent.flags
                    & (tsz_binder::symbol_flags::NAMESPACE_MODULE
                        | tsz_binder::symbol_flags::VALUE_MODULE
                        | tsz_binder::symbol_flags::ENUM))
                    == 0
                {
                    break;
                }
                parts.push(parent.escaped_name.clone());
                current = parent.parent;
            }
        }

        parts.reverse();
        Some(parts.join("."))
    }

    fn format_disambiguated_enum_name_for_assignment(
        &mut self,
        ty: TypeId,
        other: TypeId,
    ) -> Option<String> {
        let ty_sym = self.enum_symbol_from_enumish_type(ty)?;
        let other_sym = self.enum_symbol_from_enumish_type(other)?;
        let ty_symbol = self.ctx.binder.get_symbol(ty_sym)?;
        let other_symbol = self.ctx.binder.get_symbol(other_sym)?;

        if ty_symbol.escaped_name != other_symbol.escaped_name {
            return Some(ty_symbol.escaped_name.clone());
        }

        if self.is_exported_external_module_enum_symbol(ty_sym)
            && let Some(module_name) = self.module_specifier_for_symbol(ty_sym)
        {
            return Some(format!(
                "import(\"{module_name}\").{}",
                ty_symbol.escaped_name
            ));
        }

        self.format_qualified_enum_name_for_message(ty)
    }

    fn is_exported_external_module_enum_symbol(&self, sym_id: tsz_binder::SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        symbol.is_exported
            && symbol.decl_file_idx != u32::MAX
            && self
                .ctx
                .get_binder_for_file(symbol.decl_file_idx as usize)
                .is_some_and(tsz_binder::BinderState::is_external_module)
    }

    fn module_specifier_for_symbol(&self, sym_id: tsz_binder::SymbolId) -> Option<String> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if let Some(specifier) = self.ctx.module_specifiers.get(&symbol.decl_file_idx) {
            return Some(specifier.clone());
        }

        let arena = self.ctx.get_arena_for_file(symbol.decl_file_idx);
        let source_file = arena.source_files.first()?;
        let file_name = &source_file.file_name;
        let stem = file_name
            .rsplit_once('.')
            .map(|(base, _)| base)
            .unwrap_or(file_name);
        let basename = stem.rsplit_once('/').map(|(_, name)| name).unwrap_or(stem);
        Some(basename.to_string())
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

        let missing_required_props: Vec<_> = target_shape
            .properties
            .iter()
            .filter(|p| !p.optional)
            .filter(|prop| !source_shape.properties.iter().any(|p| p.name == prop.name))
            .collect();
        if missing_required_props.len() != 1 {
            return None;
        }

        Some(missing_required_props[0].name)
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
}
