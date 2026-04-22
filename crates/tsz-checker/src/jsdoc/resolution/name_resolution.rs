//! JSDoc type annotation resolution — converting JSDoc type expressions to `TypeId`.
//!
//! This module owns the **authoritative JSDoc reference-resolution kernel**:
//!
//! - `resolve_jsdoc_reference` — the ONE canonical entry point for resolving
//!   any JSDoc type expression to a `TypeId`. All callers should use this
//!   instead of re-deriving the resolution chain.
//!
//! Internal resolution components (called by the kernel, not directly):
//! - Type expression parsing (`jsdoc_type_from_expression`)
//! - Type name resolution (`resolve_jsdoc_type_name`)
//! - Symbol resolution (`resolve_jsdoc_symbol_type`, `resolve_jsdoc_entity_name_symbol`)
//! - Arrow function parsing (`parse_jsdoc_arrow_function_type`)

use crate::context::{is_declaration_file_name, is_js_file_name};
use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{IndexSignature, ObjectShape, TypeId, TypePredicate};
impl<'a> CheckerState<'a> {
    pub(crate) fn enclosing_expression_statement(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        for _ in 0..6 {
            let ext = self.ctx.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                return None;
            }
            let parent_node = self.ctx.arena.get(parent)?;
            if parent_node.kind == tsz_parser::parser::syntax_kind_ext::EXPRESSION_STATEMENT {
                return Some(parent);
            }
            current = parent;
        }
        None
    }

    pub(crate) fn expression_root(&self, idx: NodeIndex) -> NodeIndex {
        let mut current = idx;
        for _ in 0..8 {
            let Some(node) = self.ctx.arena.get(current) else {
                return current;
            };
            match node.kind {
                k if k == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || k == tsz_parser::parser::syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
                {
                    let Some(access) = self.ctx.arena.get_access_expr(node) else {
                        return current;
                    };
                    current = access.expression;
                }
                _ => return current,
            }
        }
        current
    }

    fn jsdoc_type_expr_is_broad_function(type_expr: &str) -> bool {
        let trimmed = type_expr.trim();
        trimmed.eq_ignore_ascii_case("function") || trimmed.eq_ignore_ascii_case("Function")
    }

    pub(crate) fn resolve_jsdoc_implicit_any_builtin_type(
        &mut self,
        type_expr: &str,
    ) -> Option<TypeId> {
        let factory = self.ctx.types.factory();
        match type_expr {
            "Array" | "array" => Some(factory.array(TypeId::ANY)),
            "Function" | "function" => self.resolve_jsdoc_global_implicit_any_type("Function"),
            "Object" => {
                if self.ctx.no_implicit_any() {
                    // When noImplicitAny is true, fall through to normal symbol resolution
                    // which finds the global Object interface (like `{}`)
                    None
                } else {
                    // When noImplicitAny is false (default), JSDoc Object → any (matches tsc)
                    Some(TypeId::ANY)
                }
            }
            "object" => Some(TypeId::OBJECT),
            "Promise" | "promise" => self.resolve_jsdoc_global_implicit_any_type("Promise"),
            _ => None,
        }
    }

    fn resolve_jsdoc_global_implicit_any_type(&mut self, name: &str) -> Option<TypeId> {
        let lib_binders = self.get_lib_binders();
        let sym_id = self
            .ctx
            .binder
            .get_global_type_with_libs(name, &lib_binders)?;
        let (body_type, type_params) = self.type_reference_symbol_type_with_params(sym_id);
        if body_type == TypeId::ERROR || body_type == TypeId::UNKNOWN {
            return None;
        }
        if type_params.is_empty() {
            return Some(body_type);
        }

        Some(crate::query_boundaries::common::instantiate_generic(
            self.ctx.types,
            body_type,
            &type_params,
            &vec![TypeId::ANY; type_params.len()],
        ))
    }

    fn strip_jsdoc_outer_parens(type_expr: &str) -> &str {
        let mut expr = type_expr.trim();
        loop {
            if expr.len() < 2 || !expr.starts_with('(') || !expr.ends_with(')') {
                return expr;
            }

            let mut depth = 0u32;
            let mut wraps_entire_expr = true;
            for (idx, ch) in expr.char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        if depth == 0 {
                            wraps_entire_expr = false;
                            break;
                        }
                        depth -= 1;
                        if depth == 0 && idx + ch.len_utf8() != expr.len() {
                            wraps_entire_expr = false;
                            break;
                        }
                    }
                    _ => {}
                }
            }

            if !wraps_entire_expr || depth != 0 {
                return expr;
            }

            expr = expr[1..expr.len() - 1].trim();
        }
    }

    pub(crate) fn resolve_jsdoc_type_from_comment(
        &mut self,
        jsdoc: &str,
        anchor_pos: u32,
    ) -> Option<TypeId> {
        let type_expr = Self::extract_jsdoc_type_expression(jsdoc)?.trim();
        let prev_anchor = self.ctx.jsdoc_typedef_anchor_pos.get();
        self.ctx.jsdoc_typedef_anchor_pos.set(anchor_pos);
        let result = self.resolve_jsdoc_reference(type_expr);
        self.ctx.jsdoc_typedef_anchor_pos.set(prev_anchor);
        result
    }

    pub(in crate::jsdoc) fn jsdoc_concrete_callable_type_from_expr(
        &mut self,
        type_expr: &str,
        _anchor_pos: u32,
        _comments: &[tsz_common::comments::CommentRange],
        _source_text: &str,
    ) -> Option<TypeId> {
        let type_expr = type_expr.trim();
        if type_expr.is_empty() || Self::jsdoc_type_expr_is_broad_function(type_expr) {
            return None;
        }

        // Use the authoritative resolution kernel instead of duplicating
        // the resolution chain (expression → typedef → import → file_locals).
        let ty = self.resolve_jsdoc_reference(type_expr)?;

        // Try multiple evaluation strategies to find a callable shape
        let candidates = [
            ty,
            self.judge_evaluate(ty),
            self.evaluate_contextual_type(ty),
        ];

        for candidate in candidates {
            let candidate = self.resolve_lazy_type(candidate);
            let candidate = self.evaluate_application_type(candidate);
            if crate::query_boundaries::common::function_shape_for_type(self.ctx.types, candidate)
                .is_some()
                || crate::query_boundaries::common::callable_shape_for_type(
                    self.ctx.types,
                    candidate,
                )
                .is_some()
                || crate::query_boundaries::common::call_signatures_for_type(
                    self.ctx.types,
                    candidate,
                )
                .is_some_and(|sigs| !sigs.is_empty())
            {
                return Some(candidate);
            }
        }

        None
    }

    // NOTE: jsdoc_callable_type_annotation_for_node, jsdoc_callable_type_annotation_for_node_direct,
    // resolve_global_jsdoc_typedef_type, source_file_data_for_node, resolve_type_query_type,
    // jsdoc_type_annotation_for_node are in lookup.rs
    // NOTE: validate_jsdoc_generic_constraints_at_node, jsdoc_type_annotation_for_node_direct,
    // jsdoc_satisfies_annotation_with_pos are in lookup.rs

    pub(in crate::jsdoc) fn resolve_jsdoc_import_type_reference(
        &mut self,
        type_expr: &str,
    ) -> Option<TypeId> {
        let (module_specifier, member_name) = Self::parse_jsdoc_import_type(type_expr)?;

        if let Some(member_name) = member_name {
            let sym_id = self.resolve_jsdoc_import_member(&module_specifier, &member_name)?;
            let resolved = self.resolve_jsdoc_symbol_type(sym_id);
            return (resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN).then_some(resolved);
        }

        self.commonjs_module_value_type(&module_specifier, Some(self.ctx.current_file_idx))
            .and_then(|module_type| {
                self.instance_type_from_constructor_type(module_type)
                    .or(Some(module_type))
            })
    }

    pub(crate) fn resolve_jsdoc_typeof_import_reference_parts(
        &mut self,
        module_specifier: &str,
        segments: &[(usize, String)],
    ) -> Result<TypeId, (usize, String)> {
        let mut current = self
            .commonjs_module_value_type(module_specifier, Some(self.ctx.current_file_idx))
            .or_else(|| self.build_typeof_import_namespace_type(module_specifier, None))
            .ok_or_else(|| {
                segments
                    .first()
                    .cloned()
                    .unwrap_or((0, String::from("import")))
            })?;

        if segments.is_empty() {
            return Ok(current);
        }

        for (offset, segment) in segments {
            let access = self.resolve_property_access_with_env(current, segment);
            current = match access {
                crate::query_boundaries::common::PropertyAccessResult::Success {
                    type_id, ..
                } => self.resolve_type_query_type(type_id),
                _ => return Err((*offset, segment.clone())),
            };
        }

        Ok(current)
    }
    /// Parse a JSDoc-style `@type` expression into a concrete type.
    pub(crate) fn jsdoc_type_from_expression(&mut self, type_expr: &str) -> Option<TypeId> {
        let type_expr = type_expr.trim();
        // Skip union/intersection splitting for `function(...)` types, since the
        // return type (after `:`) may contain `|`/`&` that would be incorrectly
        // split at the top level. e.g., `function("a"|"b"): 3|4` must not become
        // `function(...): 3` | `4`.
        let starts_with_function =
            type_expr.starts_with("function") && type_expr[8..].trim_start().starts_with('(');
        if let Some(conditional) = self.parse_jsdoc_conditional_type(type_expr) {
            return Some(conditional);
        }
        if !starts_with_function && let Some(parts) = Self::split_top_level_binary(type_expr, '|') {
            let mut members = Vec::new();
            for part in &parts {
                members.push(self.resolve_jsdoc_type_str(part.trim())?);
            }
            return if members.len() == 1 {
                Some(members.remove(0))
            } else {
                let factory = self.ctx.types.factory();
                Some(factory.union(members))
            };
        }
        if !starts_with_function && let Some(parts) = Self::split_top_level_binary(type_expr, '&') {
            let mut members = Vec::new();
            for part in &parts {
                members.push(self.resolve_jsdoc_type_str(part.trim())?);
            }
            return if members.len() == 1 {
                Some(members.remove(0))
            } else {
                let factory = self.ctx.types.factory();
                Some(factory.intersection(members))
            };
        }
        if type_expr == "?" {
            return Some(TypeId::ANY);
        }
        if let Some(inner) = type_expr.strip_prefix("readonly ") {
            let inner_type = self.resolve_jsdoc_type_str(inner.trim())?;
            return Some(self.ctx.types.factory().readonly_type(inner_type));
        }
        if let Some(inner) = type_expr.strip_prefix('?') {
            let inner = inner.trim();
            if !inner.is_empty()
                && let Some(inner_type) = self.resolve_jsdoc_type_str(inner)
            {
                let factory = self.ctx.types.factory();
                return Some(factory.union2(inner_type, TypeId::NULL));
            }
        }
        if let Some(inner) = type_expr.strip_prefix('!') {
            let inner = inner.trim();
            if !inner.is_empty() {
                return self.resolve_jsdoc_type_str(inner);
            }
        }
        if type_expr.len() > 1 && !type_expr.ends_with("[]") {
            if let Some(inner) = type_expr.strip_suffix('?') {
                if let Some(inner_type) = self.resolve_jsdoc_type_str(inner) {
                    let factory = self.ctx.types.factory();
                    return Some(factory.union2(inner_type, TypeId::NULL));
                }
            } else if let Some(inner) = type_expr.strip_suffix('=') {
                if let Some(inner_type) = self.resolve_jsdoc_type_str(inner) {
                    let factory = self.ctx.types.factory();
                    return Some(factory.union2(inner_type, TypeId::UNDEFINED));
                }
            } else if let Some(inner) = type_expr.strip_suffix('!') {
                return self.resolve_jsdoc_type_str(inner);
            }
        }
        if type_expr.ends_with("[]") && !type_expr.starts_with('[') {
            let inner = &type_expr[..type_expr.len() - 2];
            let inner = if inner.starts_with('(') && inner.ends_with(')') && inner.len() >= 2 {
                &inner[1..inner.len() - 1]
            } else {
                inner
            };
            let element_type = self.resolve_jsdoc_type_str(inner)?;
            let factory = self.ctx.types.factory();
            return Some(factory.array(element_type));
        }
        if !type_expr.starts_with('[')
            && type_expr.ends_with(']')
            && let Some((base_str, index_str)) = Self::parse_jsdoc_index_access_segments(type_expr)
        {
            let base_type = self.resolve_jsdoc_type_str(base_str)?;
            let index_type = self.resolve_jsdoc_type_str(index_str)?;
            return Some(self.ctx.types.factory().index_access(base_type, index_type));
        }
        if type_expr.starts_with('[') && type_expr.ends_with(']') {
            return self.parse_jsdoc_tuple_type(type_expr);
        }
        if ((type_expr.starts_with('"') && type_expr.ends_with('"'))
            || (type_expr.starts_with('\'') && type_expr.ends_with('\'')))
            && type_expr.len() >= 2
        {
            let inner = &type_expr[1..type_expr.len() - 1];
            let factory = self.ctx.types.factory();
            return Some(factory.literal_string(inner));
        }
        if type_expr == "true" {
            let factory = self.ctx.types.factory();
            return Some(factory.literal_boolean(true));
        }
        if type_expr == "false" {
            let factory = self.ctx.types.factory();
            return Some(factory.literal_boolean(false));
        }
        if let Ok(n) = type_expr.parse::<f64>()
            && type_expr
                .chars()
                .all(|c| c.is_ascii_digit() || c == '.' || c == '-')
        {
            let factory = self.ctx.types.factory();
            return Some(factory.literal_number(n));
        }
        if let Some(ty) = self.resolve_jsdoc_implicit_any_builtin_type(type_expr) {
            return Some(ty);
        }
        let factory = self.ctx.types.factory();
        match type_expr {
            "string" | "String" => Some(TypeId::STRING),
            "number" | "Number" => Some(TypeId::NUMBER),
            "boolean" | "Boolean" => Some(TypeId::BOOLEAN),
            "bigint" | "BigInt" => Some(TypeId::BIGINT),
            "any" | "*" => Some(TypeId::ANY),
            "unknown" => Some(TypeId::UNKNOWN),
            "undefined" | "Undefined" => Some(TypeId::UNDEFINED),
            "null" | "Null" => Some(TypeId::NULL),
            "void" | "Void" => Some(TypeId::VOID),
            "never" => Some(TypeId::NEVER),
            "symbol" | "Symbol" => Some(TypeId::SYMBOL),
            "this" => Some(self.ctx.types.this_type()),
            _ => {
                if let Some(tp) = self.ctx.type_parameter_scope.get(type_expr) {
                    return Some(*tp);
                }
                if let Some((module_specifier, segments)) =
                    Self::parse_jsdoc_typeof_import_query(type_expr)
                {
                    return Some(
                        self.resolve_jsdoc_typeof_import_reference_parts(
                            &module_specifier,
                            &segments,
                        )
                        .unwrap_or(TypeId::ERROR),
                    );
                }
                if let Some(resolved) = self.resolve_jsdoc_import_type_reference(type_expr) {
                    return Some(resolved);
                }
                if let Some(rest) = type_expr.strip_prefix("typeof") {
                    let rest = rest.trim();
                    if !rest.is_empty() {
                        if let Some((module_specifier, segments)) =
                            Self::parse_jsdoc_typeof_import_query(type_expr)
                        {
                            return Some(
                                self.resolve_jsdoc_typeof_import_reference_parts(
                                    &module_specifier,
                                    &segments,
                                )
                                .unwrap_or(TypeId::ERROR),
                            );
                        }
                        if let Some(sym_id) = self.resolve_jsdoc_entity_name_symbol(rest) {
                            let resolved = self.resolve_jsdoc_symbol_type(sym_id);
                            if resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN {
                                return Some(resolved);
                            }
                        }
                    }
                }
                let obj_map_inner = type_expr
                    .strip_prefix("Object.<")
                    .and_then(|rest| rest.strip_suffix(">"))
                    .or_else(|| {
                        type_expr
                            .strip_prefix("Object<")
                            .and_then(|rest| rest.strip_suffix(">"))
                    });
                if let Some(inner) = obj_map_inner {
                    let mut parts = inner.split(',');
                    let key_str = parts.next().unwrap_or("").trim();
                    let value_str = parts.next().unwrap_or("").trim();
                    if let (Some(key_type), Some(value_type)) = (
                        self.jsdoc_type_from_expression(key_str),
                        self.jsdoc_type_from_expression(value_str),
                    ) {
                        let mut shape = ObjectShape::default();
                        if key_type == TypeId::STRING {
                            shape.string_index = Some(IndexSignature {
                                key_type,
                                value_type,
                                readonly: false,
                                param_name: None,
                            });
                            return Some(factory.object_with_index(shape));
                        } else if key_type == TypeId::NUMBER {
                            shape.number_index = Some(IndexSignature {
                                key_type,
                                value_type,
                                readonly: false,
                                param_name: None,
                            });
                            return Some(factory.object_with_index(shape));
                        }
                    }
                }
                if type_expr.starts_with("{[")
                    && type_expr.contains("in keyof")
                    && type_expr.contains("=>")
                {
                    let expr = type_expr.replace(" ", "");
                    if expr.starts_with("{[")
                        && expr.ends_with("}")
                        && let Some(in_idx) = expr.find("inkeyof")
                    {
                        let k_name = &expr[2..in_idx];
                        if let Some(close_bracket) = expr.find("]:") {
                            let t_name = &expr[in_idx + "inkeyof".len()..close_bracket];
                            let k_atom = self.ctx.types.intern_string(k_name);
                            if let Some(&t_id) = self.ctx.type_parameter_scope.get(t_name) {
                                use crate::query_boundaries::common::{
                                    FunctionShape, MappedType, ParamInfo,
                                };
                                use tsz_solver::TypeParamInfo;
                                let keyof_t_id = factory.keyof(t_id);
                                let k_param = TypeParamInfo {
                                    name: k_atom,
                                    constraint: Some(keyof_t_id),
                                    default: None,
                                    is_const: false,
                                };
                                let k_id = factory.type_param(k_param);
                                let t_k_id = factory.index_access(t_id, k_id);
                                let func_shape = FunctionShape {
                                    type_params: Vec::new(),
                                    params: vec![ParamInfo {
                                        name: Some(self.ctx.types.intern_string("value")),
                                        type_id: t_k_id,
                                        optional: false,
                                        rest: false,
                                    }],
                                    this_type: None,
                                    return_type: TypeId::VOID,
                                    type_predicate: None,
                                    is_constructor: false,
                                    is_method: false,
                                };
                                let template_id = factory.function(func_shape);
                                return Some(factory.mapped(MappedType {
                                    type_param: k_param,
                                    constraint: keyof_t_id,
                                    name_type: None,
                                    template: template_id,
                                    readonly_modifier: None,
                                    optional_modifier: None,
                                }));
                            }
                        }
                    }
                }
                // Parse arrow function types: `(params) => ReturnType` or `<T>(params) => ReturnType`
                if (type_expr.starts_with('(') || type_expr.starts_with('<'))
                    && type_expr.contains("=>")
                    && let Some(result) = self.parse_jsdoc_arrow_function_type(type_expr)
                {
                    return Some(result);
                }
                if let Some(rest) = type_expr.strip_prefix("function") {
                    let rest = rest.trim_start();
                    if !rest.starts_with('(') {
                        return None;
                    }
                    let rest = &rest[1..];
                    let mut depth = 1u32;
                    let mut close_idx = None;
                    for (i, ch) in rest.char_indices() {
                        match ch {
                            '(' => depth += 1,
                            ')' => {
                                depth -= 1;
                                if depth == 0 {
                                    close_idx = Some(i);
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    if let Some(close) = close_idx {
                        let params_inner = rest[..close].trim();
                        let after_close = rest[close + 1..].trim();
                        let return_type_str = after_close
                            .strip_prefix(':')
                            .map(|s| s.trim())
                            .unwrap_or("void");
                        let return_type = self
                            .resolve_jsdoc_reference(return_type_str)
                            .unwrap_or(TypeId::VOID);
                        use tsz_solver::{FunctionShape, ParamInfo};
                        let mut params = Vec::new();
                        let mut this_type = None;
                        let mut ok = true;
                        let mut is_constructor = false;
                        let mut constructor_return = None;
                        if !params_inner.is_empty() {
                            let mut arg_index = 0u32;
                            for p in Self::split_top_level_params(params_inner) {
                                let p = p.trim();
                                if let Some(new_ret) = p.strip_prefix("new:") {
                                    is_constructor = true;
                                    let ret_str = new_ret.trim();
                                    constructor_return = self.resolve_jsdoc_reference(ret_str);
                                    arg_index += 1; // TSC skips arg0 for 'new:'
                                    continue;
                                }
                                if let Some(this_param) = p.strip_prefix("this:") {
                                    this_type = self.resolve_jsdoc_reference(this_param.trim());
                                    continue;
                                }
                                let is_rest = p.starts_with("...");
                                let effective_p = if is_rest { &p[3..] } else { p };
                                if let Some(p_type) = self.resolve_jsdoc_reference(effective_p) {
                                    let type_id = if is_rest {
                                        let factory = self.ctx.types.factory();
                                        factory.array(p_type)
                                    } else {
                                        p_type
                                    };
                                    let name =
                                        self.ctx.types.intern_string(&format!("arg{arg_index}"));
                                    arg_index += 1;
                                    params.push(ParamInfo {
                                        name: Some(name),
                                        type_id,
                                        optional: false,
                                        rest: is_rest,
                                    });
                                } else {
                                    ok = false;
                                    break;
                                }
                            }
                        }
                        if ok {
                            let final_return = if is_constructor {
                                constructor_return.unwrap_or(return_type)
                            } else {
                                return_type
                            };
                            let shape = FunctionShape {
                                type_params: Vec::new(),
                                params,
                                this_type,
                                return_type: final_return,
                                type_predicate: None,
                                is_constructor,
                                is_method: false,
                            };
                            return Some(factory.function(shape));
                        }
                    }
                }
                if let Some(rest) = type_expr.strip_prefix("keyof") {
                    let rest = rest.trim_start();
                    if let Some(name) = rest.strip_prefix("typeof") {
                        let name = name.trim();
                        if !name.is_empty()
                            && name
                                .chars()
                                .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
                        {
                            let symbols = self.ctx.binder.get_symbols();
                            let candidates = symbols.find_all_by_name(name);
                            for &sym_id in candidates {
                                let Some(sym) = symbols.get(sym_id) else {
                                    continue;
                                };
                                let value_mask = symbol_flags::FUNCTION_SCOPED_VARIABLE
                                    | symbol_flags::BLOCK_SCOPED_VARIABLE
                                    | symbol_flags::FUNCTION
                                    | symbol_flags::CLASS
                                    | symbol_flags::ENUM
                                    | symbol_flags::VALUE_MODULE;
                                if (sym.flags & value_mask) == 0 {
                                    continue;
                                }
                                let operand = self.get_type_of_symbol(sym_id);
                                if operand == TypeId::ERROR {
                                    continue;
                                }
                                let keyof = factory.keyof(operand);
                                return Some(self.judge_evaluate(keyof));
                            }
                        }
                    }
                    if !rest.is_empty()
                        && let Some(operand) = self.resolve_jsdoc_type_str(rest)
                    {
                        let keyof = factory.keyof(operand);
                        return Some(self.judge_evaluate(keyof));
                    }
                }
                if let Some(angle_idx) = Self::find_top_level_char(type_expr, '<') {
                    let base_name = type_expr[..angle_idx].trim();
                    if type_expr.ends_with('>') {
                        let args_str = &type_expr[angle_idx + 1..type_expr.len() - 1];
                        let arg_strs = Self::split_type_args_respecting_nesting(args_str);
                        let mut type_args = Vec::new();
                        for arg in &arg_strs {
                            type_args.push(self.resolve_jsdoc_type_str(arg.trim())?);
                        }
                        return self.resolve_jsdoc_generic_type(base_name, type_args);
                    }
                }
                None
            }
        }
    }
    /// Parse an arrow function type expression from JSDoc.
    ///
    /// Handles:
    /// - `(params) => ReturnType`
    /// - `<T>(params) => ReturnType` (generic arrow types)
    /// - `(x: boolean) => asserts x` (assertion predicates)
    /// - `(x: unknown) => x is string` (type predicates)
    fn parse_jsdoc_arrow_function_type(&mut self, type_expr: &str) -> Option<TypeId> {
        use tsz_solver::{FunctionShape, ParamInfo};

        // Extract generic type parameters if present: `<T, U>(params) => ReturnType`
        let (type_params_str, rest) = if type_expr.starts_with('<') {
            // Find the matching `>` (respecting nesting)
            let mut depth = 0u32;
            let mut close_idx = None;
            for (i, ch) in type_expr.char_indices() {
                match ch {
                    '<' => depth += 1,
                    '>' => {
                        depth -= 1;
                        if depth == 0 {
                            close_idx = Some(i);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let close = close_idx?;
            let tp_str = &type_expr[1..close];
            let rest = type_expr[close + 1..].trim();
            (Some(tp_str), rest)
        } else {
            (None, type_expr)
        };

        // Now `rest` should be `(params) => ReturnType`
        let arrow_idx = rest.find("=>")?;
        let params_str = rest[..arrow_idx].trim();
        if !params_str.starts_with('(') || !params_str.ends_with(')') {
            return None;
        }
        let params_inner = params_str[1..params_str.len() - 1].trim();
        let return_type_str = rest[arrow_idx + 2..].trim();

        // Parse type parameters
        let mut type_param_updates = Vec::new();
        let mut jsdoc_type_params = Vec::new();
        if let Some(tp_str) = type_params_str {
            let factory = self.ctx.types.factory();
            for tp_name in tp_str.split(',') {
                let tp_name = tp_name.trim();
                if tp_name.is_empty() {
                    continue;
                }
                // Handle constraints: `T extends Foo`
                let (name, constraint_str) = if let Some(ext_idx) = tp_name.find(" extends ") {
                    (&tp_name[..ext_idx], Some(&tp_name[ext_idx + 9..]))
                } else {
                    (tp_name, None)
                };
                let constraint =
                    constraint_str.and_then(|s| self.jsdoc_type_from_expression(s.trim()));
                let atom = self.ctx.types.intern_string(name);
                let info = tsz_solver::TypeParamInfo {
                    name: atom,
                    constraint,
                    default: None,
                    is_const: false,
                };
                let ty = factory.type_param(info);
                jsdoc_type_params.push(info);
                let previous = self.ctx.type_parameter_scope.insert(name.to_string(), ty);
                type_param_updates.push((name.to_string(), previous));
            }
        }

        // Parse return type, handling type predicates
        let (return_type, type_predicate) =
            self.parse_jsdoc_arrow_return_type(return_type_str, params_inner);

        // Parse parameters (before restoring type param scope so T is still in scope)
        let mut params = Vec::new();
        let mut this_type = None;
        let mut params_ok = true;
        if !params_inner.is_empty() {
            for raw_param in Self::split_top_level_params(params_inner) {
                let p = raw_param.trim();
                let is_rest = p.starts_with("...");
                let effective_p = if is_rest { &p[3..] } else { p };
                let (name, t_str) = if let Some(colon) = effective_p.find(':') {
                    (
                        Some(effective_p[..colon].trim()),
                        effective_p[colon + 1..].trim(),
                    )
                } else {
                    (None, effective_p)
                };
                if let Some(p_type) = self.resolve_jsdoc_reference(t_str) {
                    if name == Some("this") {
                        this_type = Some(p_type);
                        continue;
                    }
                    let atom = name.map(|n| self.ctx.types.intern_string(n));
                    params.push(ParamInfo {
                        name: atom,
                        type_id: p_type,
                        optional: false,
                        rest: is_rest,
                    });
                } else {
                    params_ok = false;
                    break;
                }
            }
        }

        // Restore type parameter scope
        for (name, previous) in type_param_updates {
            if let Some(prev) = previous {
                self.ctx.type_parameter_scope.insert(name, prev);
            } else {
                self.ctx.type_parameter_scope.remove(&name);
            }
        }

        let return_type = return_type?;
        if !params_ok {
            return None;
        }

        let factory = self.ctx.types.factory();
        let shape = FunctionShape {
            type_params: jsdoc_type_params,
            params,
            this_type,
            return_type,
            type_predicate,
            is_constructor: false,
            is_method: false,
        };
        Some(factory.function(shape))
    }

    /// Parse the return type of a JSDoc arrow function, handling type predicates.
    ///
    /// Handles:
    /// - Regular types: `string`, `number`, etc.
    /// - `asserts param` (assertion without type)
    /// - `asserts param is Type` (assertion with type)
    /// - `param is Type` (type predicate)
    fn parse_jsdoc_arrow_return_type(
        &mut self,
        return_type_str: &str,
        params_inner: &str,
    ) -> (Option<TypeId>, Option<tsz_solver::TypePredicate>) {
        // Try `asserts param` or `asserts param is Type`
        if let Some(rest) = return_type_str.strip_prefix("asserts ") {
            let rest = rest.trim();
            // Check for `asserts param is Type`
            if let Some(is_idx) = Self::find_word_boundary(rest, " is ") {
                let param_name = rest[..is_idx].trim();
                let type_str = rest[is_idx + 4..].trim();
                let pred_type = self.jsdoc_type_from_expression(type_str);
                let (target, parameter_index) =
                    self.jsdoc_type_predicate_target(param_name, params_inner);
                let predicate = TypePredicate {
                    asserts: true,
                    target,
                    type_id: pred_type,
                    parameter_index,
                };
                return (Some(TypeId::VOID), Some(predicate));
            }
            // `asserts param` (no type)
            let param_name = rest;
            let (target, parameter_index) =
                self.jsdoc_type_predicate_target(param_name, params_inner);
            let predicate = TypePredicate {
                asserts: true,
                target,
                type_id: None,
                parameter_index,
            };
            return (Some(TypeId::VOID), Some(predicate));
        }

        // Try `param is Type` (non-assertion type predicate)
        if let Some(is_idx) = Self::find_word_boundary(return_type_str, " is ") {
            let param_name = return_type_str[..is_idx].trim();
            let type_str = return_type_str[is_idx + 4..].trim();
            // Validate that param_name is a simple identifier, not a type expression
            if param_name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
            {
                let pred_type = self.jsdoc_type_from_expression(type_str);
                let (target, parameter_index) =
                    self.jsdoc_type_predicate_target(param_name, params_inner);
                let predicate = TypePredicate {
                    asserts: false,
                    target,
                    type_id: pred_type,
                    parameter_index,
                };
                return (Some(TypeId::BOOLEAN), Some(predicate));
            }
        }

        // Regular return type
        (self.jsdoc_type_from_expression(return_type_str), None)
    }

    /// Find ` is ` at a word boundary (not inside a type expression).
    fn find_word_boundary(s: &str, needle: &str) -> Option<usize> {
        s.find(needle)
    }

    /// Build a `TypePredicateTarget` from a parameter name.
    fn jsdoc_type_predicate_target(
        &self,
        param_name: &str,
        params_inner: &str,
    ) -> (tsz_solver::TypePredicateTarget, Option<usize>) {
        use tsz_solver::TypePredicateTarget;
        if param_name == "this" {
            (TypePredicateTarget::This, None)
        } else {
            let atom = self.ctx.types.intern_string(param_name);
            let parameter_index = if !params_inner.is_empty() {
                params_inner.split(',').position(|p| {
                    let p = p.trim();
                    if let Some(colon) = p.find(':') {
                        p[..colon].trim() == param_name
                    } else {
                        p == param_name
                    }
                })
            } else {
                None
            };
            (TypePredicateTarget::Identifier(atom), parameter_index)
        }
    }

    /// **Authoritative JSDoc reference-resolution kernel.**
    ///
    /// This is the ONE canonical entry point for resolving any JSDoc type expression
    /// string to a `TypeId`. All JSDoc callers (annotation lookup, callable type
    /// resolution, param type resolution) must use this function instead of
    /// re-deriving the resolution chain.
    ///
    /// Resolution order:
    /// 1. Structural parse (`jsdoc_type_from_expression`): unions, intersections,
    ///    arrays, tuples, primitives, literals, arrows, `function(...)`, generics,
    ///    `import("...")` references, `keyof typeof`, type parameter scope lookup.
    /// 2. Object literal parse: `{prop: Type, ...}`.
    /// 3. Type name resolution (`resolve_jsdoc_type_name`):
    ///    a. Qualified names (`A.B.C`) via module/namespace member walk.
    ///    b. File-local symbols (classes, interfaces, type aliases, enums).
    ///    c. `@typedef` / `@callback` from JSDoc comments in current file.
    ///    d. `@import` tag bindings via file-local symbol resolution.
    ///
    /// Callers must NOT add their own fallback chains after calling this function.
    /// If a resolution path is missing, it should be added HERE.
    pub(crate) fn resolve_jsdoc_reference(&mut self, type_expr: &str) -> Option<TypeId> {
        let type_expr = Self::strip_jsdoc_outer_parens(type_expr);
        if type_expr.is_empty() {
            return None;
        }
        // 1. Structural parse (handles unions, arrays, primitives, imports, generics, etc.)
        if let Some(ty) = self.jsdoc_type_from_expression(type_expr) {
            return Some(ty);
        }
        // 2. Object literal types: `{prop: Type, ...}`
        //    Note: @param {{ x: T }} handles nested braces separately,
        //    so this is only tried for top-level object literals.
        if type_expr.starts_with('{')
            && type_expr.ends_with('}')
            && let Some(ty) = self.parse_jsdoc_object_literal_type(type_expr)
        {
            return Some(ty);
        }
        // 3. Type name resolution (qualified names, file-local symbols, typedefs)
        self.resolve_jsdoc_type_name(type_expr)
    }

    /// Backward-compatible alias for `resolve_jsdoc_reference`.
    ///
    /// All internal callers within the JSDoc subsystem should prefer
    /// `resolve_jsdoc_reference` directly. This alias exists so that
    /// callers outside the JSDoc subsystem (e.g., `jsdoc_type_from_expression`
    /// recursive calls) continue to work without churn.
    pub(crate) fn resolve_jsdoc_type_str(&mut self, type_expr: &str) -> Option<TypeId> {
        self.resolve_jsdoc_reference(type_expr)
    }

    /// Resolve a simple type name from the symbol table or @typedef declarations.
    ///
    /// This is an internal helper called by `resolve_jsdoc_reference` (step 3).
    /// Do NOT call this directly — use `resolve_jsdoc_reference` instead.
    fn resolve_jsdoc_type_name(&mut self, name: &str) -> Option<TypeId> {
        // 3a. Qualified names (e.g., `Namespace.Type.Member`)
        if name.contains('.')
            && let Some(resolved) = self.resolve_jsdoc_qualified_type_name(name)
        {
            return Some(resolved);
        }

        // 3b. @typedef / @callback resolution from JSDoc comments (takes precedence over
        // file-local values in JSDoc type-position lookups).
        // This matches tsc behavior where a JSDoc typedef can shadow an ambient value
        // with the same name in JSDoc annotation contexts.
        if let Some((ty, _)) = self.resolve_global_jsdoc_typedef_info(name) {
            self.register_jsdoc_typedef_def(name, ty);
            return Some(ty);
        }

        // 3c. File-local symbols (classes, interfaces, type aliases, enums, imports)
        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            let resolved = self.resolve_jsdoc_symbol_type(sym_id);
            if resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN {
                return Some(resolved);
            }
        }

        if let Some(sym_id) = self.resolve_jsdoc_entity_name_symbol(name) {
            let resolved = self.resolve_jsdoc_symbol_type(sym_id);
            if resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN {
                return Some(resolved);
            }
        }
        None
    }

    fn jsdoc_module_specifier_prefers_direct_type_exports(&self, module_specifier: &str) -> bool {
        let Some(target_file_idx) = self
            .ctx
            .resolve_import_target_from_file(self.ctx.current_file_idx, module_specifier)
        else {
            return false;
        };
        let arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let Some(source_file) = arena.source_files.first() else {
            return false;
        };
        source_file.is_declaration_file
            || is_declaration_file_name(&source_file.file_name)
            || !is_js_file_name(&source_file.file_name)
    }

    fn jsdoc_direct_module_member_symbol(
        &self,
        current_sym: tsz_binder::SymbolId,
        segment: &str,
    ) -> Option<tsz_binder::SymbolId> {
        let lib_binders = self.get_lib_binders();
        let symbol = self.get_cross_file_symbol(current_sym).or_else(|| {
            self.ctx
                .binder
                .get_symbol_with_libs(current_sym, &lib_binders)
        })?;

        if let Some(module_specifier) = symbol.import_module.as_deref()
            && self.jsdoc_module_specifier_prefers_direct_type_exports(module_specifier)
        {
            return self.resolve_jsdoc_import_member(module_specifier, segment);
        }

        let decl = symbol.value_declaration;
        if decl.is_none() {
            return None;
        }
        let decl_node = self.ctx.arena.get(decl)?;
        let decl = if decl_node.kind == SyntaxKind::Identifier as u16 {
            let parent = self.ctx.arena.get_extended(decl)?.parent;
            if parent.is_some()
                && self
                    .ctx
                    .arena
                    .get(parent)
                    .is_some_and(|node| node.kind == syntax_kind_ext::VARIABLE_DECLARATION)
            {
                parent
            } else {
                decl
            }
        } else {
            decl
        };

        let var_decl = self
            .ctx
            .arena
            .get(decl)
            .and_then(|node| self.ctx.arena.get_variable_declaration(node))?;
        if var_decl.initializer.is_none() {
            return None;
        }
        let module_specifier = self.get_require_module_specifier(var_decl.initializer)?;
        if !self.jsdoc_module_specifier_prefers_direct_type_exports(&module_specifier) {
            return None;
        }
        self.resolve_jsdoc_import_member(&module_specifier, segment)
    }

    fn resolve_jsdoc_qualified_type_name(&mut self, name: &str) -> Option<TypeId> {
        if let Some(resolved) = self.resolve_jsdoc_require_qualified_type_name(name) {
            return Some(resolved);
        }

        if let Some(sym_id) = self.resolve_jsdoc_entity_name_symbol(name) {
            let resolved = self.resolve_jsdoc_symbol_type(sym_id);
            if resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN {
                return Some(resolved);
            }
        }

        // Anonymous @typedef takes precedence over assigned value type.
        // In tsc, `@typedef {type}` without a name creates a type alias
        // named after the following declaration.
        if let Some(resolved) = self.resolve_anonymous_typedef_for_name(name) {
            return Some(resolved);
        }

        self.resolve_jsdoc_assigned_value_type(name)
    }

    fn resolve_jsdoc_require_qualified_type_name(&mut self, name: &str) -> Option<TypeId> {
        let mut segments = name.split('.');
        let root_name = segments.next()?;
        let first_member = segments.next()?;
        let root_sym = self.ctx.binder.file_locals.get(root_name).or_else(|| {
            self.ctx
                .lib_contexts
                .iter()
                .find_map(|ctx| ctx.binder.file_locals.get(root_name))
        })?;
        let root_symbol = self.ctx.binder.get_symbol(root_sym)?;

        let decl = root_symbol.value_declaration;
        if decl.is_none() {
            return None;
        }
        let decl_node = self.ctx.arena.get(decl)?;
        let decl = if decl_node.kind == SyntaxKind::Identifier as u16 {
            let parent = self.ctx.arena.get_extended(decl)?.parent;
            if parent.is_some()
                && self
                    .ctx
                    .arena
                    .get(parent)
                    .is_some_and(|node| node.kind == syntax_kind_ext::VARIABLE_DECLARATION)
            {
                parent
            } else {
                decl
            }
        } else {
            decl
        };
        let var_decl = self
            .ctx
            .arena
            .get(decl)
            .and_then(|node| self.ctx.arena.get_variable_declaration(node))?;
        if var_decl.initializer.is_none() {
            return None;
        }
        let module_specifier = self.get_require_module_specifier(var_decl.initializer)?;
        if !self.jsdoc_module_specifier_prefers_direct_type_exports(&module_specifier) {
            return None;
        }

        let mut current_sym = self.resolve_jsdoc_import_member(&module_specifier, first_member)?;
        for segment in segments {
            let lib_binders = self.get_lib_binders();
            let mut visited_aliases = AliasCycleTracker::new();
            current_sym = self
                .resolve_alias_symbol(current_sym, &mut visited_aliases)
                .unwrap_or(current_sym);
            let symbol = self.get_cross_file_symbol(current_sym).or_else(|| {
                self.ctx
                    .binder
                    .get_symbol_with_libs(current_sym, &lib_binders)
            })?;
            current_sym = symbol
                .exports
                .as_ref()
                .and_then(|exports| exports.get(segment))
                .or_else(|| {
                    symbol
                        .members
                        .as_ref()
                        .and_then(|members| members.get(segment))
                })?;
        }

        let resolved = self.resolve_jsdoc_symbol_type(current_sym);
        (resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN).then_some(resolved)
    }

    pub(crate) fn resolve_jsdoc_entity_name_symbol(
        &mut self,
        name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        let mut segments = name.split('.');
        let root_name = segments.next()?;
        let (mut current_sym, current_file_idx) =
            if let Some(sym_id) = self.ctx.binder.file_locals.get(root_name) {
                (sym_id, Some(self.ctx.current_file_idx))
            } else if let Some(sym_id) = self
                .ctx
                .lib_contexts
                .iter()
                .find_map(|ctx| ctx.binder.file_locals.get(root_name))
            {
                (sym_id, None)
            } else if let Some((sym_id, file_idx)) =
                self.resolve_jsdoc_cross_file_root_symbol(root_name)
            {
                (sym_id, Some(file_idx))
            } else {
                return None;
            };
        if let Some(current_file_idx) = current_file_idx {
            self.ctx
                .register_symbol_file_target(current_sym, current_file_idx);
        }
        let lib_binders = self.get_lib_binders();

        for segment in segments {
            if let Some(member_sym) = self.jsdoc_direct_module_member_symbol(current_sym, segment) {
                if let Some(current_file_idx) = current_file_idx {
                    // Cross-file import-member resolution already registers the owning file.
                    // Preserve that mapping and only stamp local symbols that have no owner yet.
                    if !self.ctx.has_symbol_file_index(member_sym) {
                        self.ctx
                            .register_symbol_file_target(member_sym, current_file_idx);
                    }
                }
                current_sym = member_sym;
                continue;
            }

            let mut visited_aliases = AliasCycleTracker::new();
            current_sym = self
                .resolve_alias_symbol(current_sym, &mut visited_aliases)
                .unwrap_or(current_sym);

            let symbol = self.get_cross_file_symbol(current_sym).or_else(|| {
                self.ctx
                    .binder
                    .get_symbol_with_libs(current_sym, &lib_binders)
            })?;

            if let Some(member_sym) = symbol
                .exports
                .as_ref()
                .and_then(|exports| exports.get(segment))
                .or_else(|| {
                    symbol
                        .members
                        .as_ref()
                        .and_then(|members| members.get(segment))
                })
            {
                if let Some(current_file_idx) = current_file_idx {
                    if !self.ctx.has_symbol_file_index(member_sym) {
                        self.ctx
                            .register_symbol_file_target(member_sym, current_file_idx);
                    }
                }
                current_sym = member_sym;
                continue;
            }

            if let Some(ref module_specifier) = symbol.import_module {
                let mut visited_aliases = AliasCycleTracker::new();
                if let Some(member_sym) = self.resolve_reexported_member_symbol(
                    module_specifier,
                    segment,
                    &mut visited_aliases,
                ) {
                    if let Some(current_file_idx) = current_file_idx {
                        if !self.ctx.has_symbol_file_index(member_sym) {
                            self.ctx
                                .register_symbol_file_target(member_sym, current_file_idx);
                        }
                    }
                    current_sym = member_sym;
                    continue;
                }
            }

            if symbol.has_any_flags(symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)
                && let Some(member_sym) = self.resolve_namespace_member_from_all_binders(
                    symbol.escaped_name.as_str(),
                    segment,
                )
            {
                if let Some(current_file_idx) = current_file_idx {
                    self.ctx
                        .register_symbol_file_target(member_sym, current_file_idx);
                }
                current_sym = member_sym;
                continue;
            }

            return None;
        }

        let mut visited_aliases = AliasCycleTracker::new();
        Some(
            self.resolve_alias_symbol(current_sym, &mut visited_aliases)
                .unwrap_or(current_sym),
        )
    }

    fn resolve_jsdoc_cross_file_root_symbol(
        &mut self,
        root_name: &str,
    ) -> Option<(tsz_binder::SymbolId, usize)> {
        let all_binders = self.ctx.all_binders.as_ref()?;
        for (file_idx, binder) in all_binders.iter().enumerate() {
            if file_idx == self.ctx.current_file_idx {
                continue;
            }
            if let Some(sym_id) = binder.file_locals.get(root_name) {
                return Some((sym_id, file_idx));
            }
        }

        None
    }

    fn resolve_jsdoc_commonjs_binding_element_type(
        &mut self,
        value_decl: NodeIndex,
        local_name: &str,
    ) -> Option<TypeId> {
        let node = self.ctx.arena.get(value_decl)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let be_idx = self.ctx.arena.get_extended(value_decl)?.parent;
        let be_node = self.ctx.arena.get(be_idx)?;
        if be_node.kind != syntax_kind_ext::BINDING_ELEMENT {
            return None;
        }
        let be_data = self.ctx.arena.get_binding_element(be_node)?;

        let pat_idx = self.ctx.arena.get_extended(be_idx)?.parent;
        let pat_node = self.ctx.arena.get(pat_idx)?;
        if pat_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN {
            return None;
        }

        let var_decl_idx = self.ctx.arena.get_extended(pat_idx)?.parent;
        let var_decl_node = self.ctx.arena.get(var_decl_idx)?;
        let var_decl = self.ctx.arena.get_variable_declaration(var_decl_node)?;
        if !var_decl.initializer.is_some() {
            return None;
        }

        let module_specifier = self.get_require_module_specifier(var_decl.initializer)?;
        let export_name = if be_data.property_name.is_some() {
            self.get_identifier_text_from_idx(be_data.property_name)?
        } else {
            local_name.to_string()
        };

        let export_type = self.resolve_js_export_named_type(
            &module_specifier,
            &export_name,
            Some(self.ctx.current_file_idx),
        );
        if let Some(export_type) = export_type {
            if let Some(instance_type) = self.instance_type_from_constructor_type(export_type) {
                return Some(instance_type);
            }
            if export_type != TypeId::ERROR && export_type != TypeId::UNKNOWN {
                return Some(export_type);
            }
        }

        let export_sym_id = self
            .resolve_cross_file_export_from_file(
                &module_specifier,
                &export_name,
                Some(self.ctx.current_file_idx),
            )
            .or_else(|| {
                self.ctx
                    .binder
                    .module_exports
                    .get(&module_specifier)
                    .and_then(|exports| exports.get(&export_name))
            })
            .or_else(|| {
                self.resolve_named_export_via_export_equals(&module_specifier, &export_name)
            })
            .or_else(|| {
                let mut visited_aliases = AliasCycleTracker::new();
                self.resolve_reexported_member_symbol(
                    &module_specifier,
                    &export_name,
                    &mut visited_aliases,
                )
            })?;

        let export_type = self.resolve_jsdoc_symbol_type(export_sym_id);
        (export_type != TypeId::ERROR && export_type != TypeId::UNKNOWN).then_some(export_type)
    }

    pub(in crate::jsdoc) fn resolve_jsdoc_symbol_type(
        &mut self,
        sym_id: tsz_binder::SymbolId,
    ) -> TypeId {
        let Some(symbol) = self
            .get_cross_file_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol(sym_id))
            .cloned()
        else {
            return TypeId::ERROR;
        };

        if symbol.has_any_flags(symbol_flags::ALIAS) {
            let mut visited_aliases = AliasCycleTracker::new();
            if let Some(target) = self.resolve_alias_symbol(sym_id, &mut visited_aliases) {
                if target == sym_id {
                    // Some unresolved aliases (notably synthetic JSDoc @import aliases)
                    // can legitimately resolve to themselves. Re-entering with the same
                    // symbol would recurse forever and overflow the stack.
                    return TypeId::ERROR;
                }
                return self.resolve_jsdoc_symbol_type(target);
            }
        }

        if symbol.has_any_flags(symbol_flags::TYPE_PARAMETER) {
            return self.type_reference_symbol_type(sym_id);
        }

        if symbol.has_any_flags(
            symbol_flags::TYPE_ALIAS
                | symbol_flags::CLASS
                | symbol_flags::INTERFACE
                | symbol_flags::ENUM,
        ) {
            return self.type_reference_symbol_type(sym_id);
        }

        if symbol.has_any_flags(symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) {
            let namespace_type = self.get_type_of_symbol(sym_id);
            if namespace_type != TypeId::ERROR && namespace_type != TypeId::UNKNOWN {
                return namespace_type;
            }
        }

        if symbol.has_any_flags(symbol_flags::FUNCTION) && symbol.value_declaration.is_some() {
            let constructor_type = self.get_type_of_symbol(sym_id);
            if let Some(instance_type) = self.synthesize_js_constructor_instance_type(
                symbol.value_declaration,
                constructor_type,
                &[],
            ) {
                return instance_type;
            }
        }

        if symbol.has_any_flags(
            symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::BLOCK_SCOPED_VARIABLE,
        ) {
            if let Some(enum_type) = symbol
                .declarations
                .iter()
                .copied()
                .filter(|decl| decl.is_some())
                .find_map(|decl| self.jsdoc_enum_annotation_type_for_symbol_decl(sym_id, decl))
            {
                return enum_type;
            }
            if symbol.value_declaration.is_some()
                && let Some(instance_type) = self.resolve_jsdoc_commonjs_binding_element_type(
                    symbol.value_declaration,
                    symbol.escaped_name.as_str(),
                )
            {
                return instance_type;
            }
            let value_type = self.get_type_of_symbol(sym_id);
            let prefer_value_type = symbol.value_declaration.is_some()
                && self.jsdoc_declared_value_symbol_prefers_value_type(
                    sym_id,
                    symbol.value_declaration,
                );
            if !prefer_value_type
                && let Some(instance_type) = self.instance_type_from_constructor_type(value_type)
            {
                return instance_type;
            }
            // Fall back to the raw value type for non-constructor variables.
            if value_type != TypeId::ERROR && value_type != TypeId::UNKNOWN {
                return value_type;
            }
        }

        TypeId::ERROR
    }
}
