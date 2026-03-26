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
//! - Typedef resolution (`resolve_jsdoc_typedef_type`, `type_from_jsdoc_typedef`)
//! - Symbol resolution (`resolve_jsdoc_symbol_type`, `resolve_jsdoc_entity_name_symbol`)
//! - Generic instantiation (`resolve_jsdoc_generic_type`)
//! - Import type resolution (`resolve_jsdoc_import_type_reference`)

use super::types::{JsdocCallbackInfo, JsdocTypedefInfo};
use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{
    FunctionShape, IndexSignature, ObjectShape, ParamInfo, PropertyInfo, TupleElement, TypeId,
    TypePredicate, TypePredicateTarget, Visibility,
};
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

    pub(super) fn jsdoc_concrete_callable_type_from_expr(
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
            let candidate = self.evaluate_application_type(candidate);
            if tsz_solver::type_queries::get_function_shape(self.ctx.types, candidate).is_some()
                || tsz_solver::type_queries::get_call_signatures(self.ctx.types, candidate)
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

    pub(super) fn resolve_jsdoc_import_type_reference(
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
    /// Parse a JSDoc-style `@type` expression into a concrete type.
    pub(crate) fn jsdoc_type_from_expression(&mut self, type_expr: &str) -> Option<TypeId> {
        let type_expr = type_expr.trim();
        if let Some(parts) = Self::split_top_level_binary(type_expr, '|') {
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
        if let Some(parts) = Self::split_top_level_binary(type_expr, '&') {
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
        if let Some(inner) = type_expr.strip_prefix('?') {
            let inner = inner.trim();
            if !inner.is_empty()
                && let Some(inner_type) = self.resolve_jsdoc_type_str(inner)
            {
                let factory = self.ctx.types.factory();
                return Some(factory.union(vec![inner_type, TypeId::NULL]));
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
                    return Some(factory.union(vec![inner_type, TypeId::NULL]));
                }
            } else if let Some(inner) = type_expr.strip_suffix('=') {
                if let Some(inner_type) = self.resolve_jsdoc_type_str(inner) {
                    let factory = self.ctx.types.factory();
                    return Some(factory.union(vec![inner_type, TypeId::UNDEFINED]));
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
        let factory = self.ctx.types.factory();
        match type_expr {
            "string" | "String" => Some(TypeId::STRING),
            "number" | "Number" => Some(TypeId::NUMBER),
            "boolean" | "Boolean" => Some(TypeId::BOOLEAN),
            "bigint" | "BigInt" => Some(TypeId::BIGINT),
            "object" => Some(TypeId::OBJECT),
            "any" | "*" => Some(TypeId::ANY),
            "unknown" => Some(TypeId::UNKNOWN),
            "undefined" | "Undefined" => Some(TypeId::UNDEFINED),
            "null" | "Null" => Some(TypeId::NULL),
            "void" | "Void" => Some(TypeId::VOID),
            "never" => Some(TypeId::NEVER),
            "symbol" | "Symbol" => Some(TypeId::SYMBOL),
            "this" => Some(self.ctx.types.this_type()),
            "Array" | "array" => Some(factory.array(TypeId::ANY)),
            _ => {
                if let Some(tp) = self.ctx.type_parameter_scope.get(type_expr) {
                    return Some(*tp);
                }
                if let Some(resolved) = self.resolve_jsdoc_import_type_reference(type_expr) {
                    return Some(resolved);
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
                                use tsz_solver::{
                                    FunctionShape, MappedType, ParamInfo, TypeParamInfo,
                                };
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
                            .jsdoc_type_from_expression(return_type_str)
                            .unwrap_or(TypeId::VOID);
                        use tsz_solver::{FunctionShape, ParamInfo};
                        let mut params = Vec::new();
                        let mut ok = true;
                        let mut is_constructor = false;
                        let mut constructor_return = None;
                        if !params_inner.is_empty() {
                            let mut arg_index = 0u32;
                            for p in params_inner.split(',') {
                                let p = p.trim();
                                if let Some(new_ret) = p.strip_prefix("new:") {
                                    is_constructor = true;
                                    let ret_str = new_ret.trim();
                                    constructor_return = self.jsdoc_type_from_expression(ret_str);
                                    arg_index += 1; // TSC skips arg0 for 'new:'
                                    continue;
                                }
                                let is_rest = p.starts_with("...");
                                let effective_p = if is_rest { &p[3..] } else { p };
                                if let Some(p_type) = self.jsdoc_type_from_expression(effective_p) {
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
                                this_type: None,
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
        let mut params_ok = true;
        if !params_inner.is_empty() {
            for p in params_inner.split(',') {
                let p = p.trim();
                let (name, t_str) = if let Some(colon) = p.find(':') {
                    (Some(p[..colon].trim()), p[colon + 1..].trim())
                } else {
                    (None, p)
                };
                if let Some(p_type) = self.jsdoc_type_from_expression(t_str) {
                    let atom = name.map(|n| self.ctx.types.intern_string(n));
                    params.push(ParamInfo {
                        name: atom,
                        type_id: p_type,
                        optional: false,
                        rest: false,
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
            this_type: None,
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
        if let Some(sf) = self.ctx.arena.source_files.first() {
            if sf.comments.is_empty() {
                return None;
            }
            let comments = sf.comments.clone();
            let source_text: String = sf.text.to_string();
            if let Some(ty) =
                self.resolve_jsdoc_typedef_type(name, u32::MAX, &comments, &source_text)
            {
                self.register_jsdoc_typedef_def(name, ty);
                return Some(ty);
            }
        }

        // 3c. File-local symbols (classes, interfaces, type aliases, enums, imports)
        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            let resolved = self.resolve_jsdoc_symbol_type(sym_id);
            if resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN {
                return Some(resolved);
            }
        }
        None
    }

    fn resolve_jsdoc_qualified_type_name(&mut self, name: &str) -> Option<TypeId> {
        if let Some(sym_id) = self.resolve_jsdoc_entity_name_symbol(name) {
            let resolved = self.resolve_jsdoc_symbol_type(sym_id);
            if resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN {
                return Some(resolved);
            }
        }

        self.resolve_jsdoc_assigned_value_type(name)
    }

    fn resolve_jsdoc_entity_name_symbol(&self, name: &str) -> Option<tsz_binder::SymbolId> {
        let mut segments = name.split('.');
        let root_name = segments.next()?;
        let mut current_sym = self.ctx.binder.file_locals.get(root_name).or_else(|| {
            self.ctx
                .lib_contexts
                .iter()
                .find_map(|ctx| ctx.binder.file_locals.get(root_name))
        })?;
        let lib_binders = self.get_lib_binders();

        for segment in segments {
            let mut visited_aliases = Vec::new();
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
                current_sym = member_sym;
                continue;
            }

            if let Some(ref module_specifier) = symbol.import_module {
                let mut visited_aliases = Vec::new();
                if let Some(member_sym) = self.resolve_reexported_member_symbol(
                    module_specifier,
                    segment,
                    &mut visited_aliases,
                ) {
                    current_sym = member_sym;
                    continue;
                }
            }

            if symbol.flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0
                && let Some(member_sym) = self.resolve_namespace_member_from_all_binders(
                    symbol.escaped_name.as_str(),
                    segment,
                )
            {
                current_sym = member_sym;
                continue;
            }

            return None;
        }

        let mut visited_aliases = Vec::new();
        Some(
            self.resolve_alias_symbol(current_sym, &mut visited_aliases)
                .unwrap_or(current_sym),
        )
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
                let mut visited_aliases = Vec::new();
                self.resolve_reexported_member_symbol(
                    &module_specifier,
                    &export_name,
                    &mut visited_aliases,
                )
            })?;

        let export_type = self.resolve_jsdoc_symbol_type(export_sym_id);
        (export_type != TypeId::ERROR && export_type != TypeId::UNKNOWN).then_some(export_type)
    }

    pub(super) fn resolve_jsdoc_symbol_type(&mut self, sym_id: tsz_binder::SymbolId) -> TypeId {
        let Some(symbol) = self
            .get_cross_file_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol(sym_id))
            .cloned()
        else {
            return TypeId::ERROR;
        };

        if (symbol.flags & symbol_flags::ALIAS) != 0 {
            let mut visited_aliases = Vec::new();
            if let Some(target) = self.resolve_alias_symbol(sym_id, &mut visited_aliases) {
                return self.resolve_jsdoc_symbol_type(target);
            }
        }

        if (symbol.flags & symbol_flags::TYPE_PARAMETER) != 0 {
            return self.type_reference_symbol_type(sym_id);
        }

        if (symbol.flags
            & (symbol_flags::TYPE_ALIAS
                | symbol_flags::CLASS
                | symbol_flags::INTERFACE
                | symbol_flags::ENUM))
            != 0
        {
            return self.type_reference_symbol_type(sym_id);
        }

        if (symbol.flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)) != 0 {
            let namespace_type = self.get_type_of_symbol(sym_id);
            if namespace_type != TypeId::ERROR && namespace_type != TypeId::UNKNOWN {
                return namespace_type;
            }
        }

        if (symbol.flags & symbol_flags::FUNCTION) != 0 && symbol.value_declaration.is_some() {
            let constructor_type = self.get_type_of_symbol(sym_id);
            if let Some(instance_type) = self.synthesize_js_constructor_instance_type(
                symbol.value_declaration,
                constructor_type,
                &[],
            ) {
                return instance_type;
            }
        }

        if (symbol.flags
            & (symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::BLOCK_SCOPED_VARIABLE))
            != 0
        {
            if symbol.value_declaration.is_some()
                && let Some(instance_type) = self.resolve_jsdoc_commonjs_binding_element_type(
                    symbol.value_declaration,
                    symbol.escaped_name.as_str(),
                )
            {
                return instance_type;
            }
            let value_type = self.get_type_of_symbol(sym_id);
            if let Some(instance_type) = self.instance_type_from_constructor_type(value_type) {
                return instance_type;
            }
            // Fall back to the raw value type for non-constructor variables.
            if value_type != TypeId::ERROR && value_type != TypeId::UNKNOWN {
                return value_type;
            }
        }

        TypeId::ERROR
    }

    pub(crate) fn resolve_jsdoc_assigned_value_type(&mut self, name: &str) -> Option<TypeId> {
        let prototype_type = self.resolve_jsdoc_prototype_assignment_type(name);

        for raw_idx in 0..self.ctx.arena.len() {
            let idx = NodeIndex(raw_idx as u32);
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.kind != tsz_parser::parser::syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let Some(binary) = self.ctx.arena.get_binary_expr(node) else {
                continue;
            };
            if binary.operator_token != tsz_scanner::SyntaxKind::EqualsToken as u16 {
                continue;
            }
            if self.expression_text(binary.left).as_deref() != Some(name) {
                continue;
            }
            if let Some(jsdoc_type) = self.jsdoc_type_annotation_for_node(idx) {
                return Some(
                    self.combine_jsdoc_instance_and_prototype_type(jsdoc_type, prototype_type),
                );
            }
            if let Some(stmt_idx) = self.enclosing_expression_statement(idx)
                && let Some(jsdoc_type) = self.js_statement_declared_type(stmt_idx).or_else(|| {
                    let sf = self.source_file_data_for_node(stmt_idx)?;
                    let source_text = sf.text.to_string();
                    let comments = sf.comments.clone();
                    let jsdoc =
                        self.try_jsdoc_with_ancestor_walk(stmt_idx, &comments, &source_text)?;
                    self.resolve_jsdoc_type_from_comment(&jsdoc, self.ctx.arena.get(stmt_idx)?.pos)
                })
            {
                return Some(
                    self.combine_jsdoc_instance_and_prototype_type(jsdoc_type, prototype_type),
                );
            }
            let left_root = self.expression_root(binary.left);
            if left_root != binary.left
                && let Some(jsdoc_type) = self.jsdoc_type_annotation_for_node(left_root)
            {
                return Some(
                    self.combine_jsdoc_instance_and_prototype_type(jsdoc_type, prototype_type),
                );
            }
        }

        prototype_type
    }

    fn resolve_jsdoc_prototype_assignment_type(&mut self, name: &str) -> Option<TypeId> {
        let prototype_name = format!("{name}.prototype");

        for raw_idx in 0..self.ctx.arena.len() {
            let idx = NodeIndex(raw_idx as u32);
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.kind != tsz_parser::parser::syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let Some(binary) = self.ctx.arena.get_binary_expr(node) else {
                continue;
            };
            if binary.operator_token != tsz_scanner::SyntaxKind::EqualsToken as u16 {
                continue;
            }
            if self.expression_text(binary.left).as_deref() != Some(prototype_name.as_str()) {
                continue;
            }

            let rhs = self.ctx.arena.skip_parenthesized(binary.right);
            let Some(rhs_node) = self.ctx.arena.get(rhs) else {
                continue;
            };
            if rhs_node.kind != tsz_parser::parser::syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                continue;
            }

            let resolved = self.get_type_of_node(rhs);
            if resolved != TypeId::ANY && resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN {
                return Some(resolved);
            }
        }

        None
    }

    fn combine_jsdoc_instance_and_prototype_type(
        &mut self,
        instance_type: TypeId,
        prototype_type: Option<TypeId>,
    ) -> TypeId {
        let Some(prototype_type) = prototype_type else {
            return instance_type;
        };

        if matches!(instance_type, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN) {
            return prototype_type;
        }
        if matches!(
            prototype_type,
            TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN
        ) || instance_type == prototype_type
        {
            return instance_type;
        }

        self.ctx
            .types
            .factory()
            .intersection2(instance_type, prototype_type)
    }
    fn ensure_jsdoc_typedef_def(
        &mut self,
        name: &str,
        body_type: TypeId,
        type_params: &[tsz_solver::TypeParamInfo],
    ) -> tsz_solver::def::DefId {
        use tsz_solver::def::{DefKind, DefinitionInfo};

        if type_params.is_empty()
            && let Some(def_id) = self.ctx.definition_store.find_type_alias_by_body(body_type)
        {
            return def_id;
        }

        let atom_name = self.ctx.types.intern_string(name);
        if let Some(candidates) = self.ctx.definition_store.find_defs_by_name(atom_name) {
            for def_id in candidates {
                if let Some(def) = self.ctx.definition_store.get(def_id)
                    && matches!(def.kind, DefKind::TypeAlias)
                    && def.body == Some(body_type)
                    && def.type_params.as_slice() == type_params
                {
                    return def_id;
                }
            }
        }

        let info = DefinitionInfo::type_alias(atom_name, type_params.to_vec(), body_type);
        self.ctx.definition_store.register(info)
    }

    /// Register a DefId for a JSDoc `@typedef` so the type formatter can find the alias name.
    pub(super) fn register_jsdoc_typedef_def(&mut self, name: &str, body_type: TypeId) {
        let _ = self.ensure_jsdoc_typedef_def(name, body_type, &[]);
    }

    fn ensure_jsdoc_instantiated_display_def(
        &mut self,
        name: &str,
        type_id: TypeId,
    ) -> tsz_solver::def::DefId {
        use tsz_solver::def::{DefKind, DefinitionInfo};

        let atom_name = self.ctx.types.intern_string(name);
        if let Some(def_id) = self.ctx.definition_store.find_def_for_type(type_id)
            && let Some(def) = self.ctx.definition_store.get(def_id)
            && matches!(def.kind, DefKind::TypeAlias)
            && def.name == atom_name
        {
            return def_id;
        }

        let def_id = self
            .ctx
            .definition_store
            .register(DefinitionInfo::type_alias(atom_name, Vec::new(), type_id));
        self.ctx
            .definition_store
            .register_type_to_def(type_id, def_id);
        def_id
    }
    /// Resolve a generic JSDoc type reference: `Name<Arg1, Arg2, ...>`.
    fn resolve_jsdoc_generic_type(
        &mut self,
        base_name: &str,
        type_args: Vec<TypeId>,
    ) -> Option<TypeId> {
        if let Some(instantiated) = self.resolve_jsdoc_generic_typedef_type(base_name, &type_args) {
            return Some(instantiated);
        }

        // Look up the base type in file_locals (includes merged lib types like Partial, Record)
        let sym_id = if let Some(sym_id) = self.ctx.binder.file_locals.get(base_name) {
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            if (symbol.flags
                & (symbol_flags::TYPE_ALIAS
                    | symbol_flags::CLASS
                    | symbol_flags::INTERFACE
                    | symbol_flags::ENUM))
                == 0
            {
                return None;
            }
            sym_id
        } else {
            let symbols = self.ctx.binder.get_symbols();
            symbols
                .find_all_by_name(base_name)
                .iter()
                .copied()
                .find(|&sym_id| {
                    self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                        (symbol.flags
                            & (symbol_flags::TYPE_ALIAS
                                | symbol_flags::CLASS
                                | symbol_flags::INTERFACE
                                | symbol_flags::ENUM))
                            != 0
                    })
                })?
        };
        let (body_type, type_params) = self.type_reference_symbol_type_with_params(sym_id);
        if body_type == TypeId::ERROR {
            return None;
        }
        if type_params.is_empty() || type_args.is_empty() {
            return Some(body_type);
        }
        // Directly instantiate the type body with the provided type arguments.
        // Do NOT evaluate here — the caller (jsdoc_satisfies_annotation_with_pos)
        // calls judge_evaluate, which will expand mapped types while preserving
        // Lazy(DefId) references in value positions for correct type name display.
        use tsz_solver::instantiate_generic;
        let instantiated = instantiate_generic(self.ctx.types, body_type, &type_params, &type_args);
        Some(instantiated)
    }
    fn parse_jsdoc_tuple_type(&mut self, type_expr: &str) -> Option<TypeId> {
        let inner = type_expr[1..type_expr.len() - 1].trim();
        if inner.is_empty() {
            return Some(self.ctx.types.factory().tuple(Vec::new()));
        }

        let mut elements = Vec::new();
        for elem_str in Self::split_type_args_respecting_nesting(inner) {
            let mut elem = elem_str.trim();
            if elem.is_empty() {
                continue;
            }

            let mut rest = false;
            if let Some(stripped) = elem.strip_prefix("...") {
                rest = true;
                elem = stripped.trim();
            }

            let (name, optional, type_str) = if let Some(colon_idx) =
                Self::find_top_level_char(elem, ':')
            {
                let raw_name = elem[..colon_idx].trim();
                let type_str = elem[colon_idx + 1..].trim();
                let (raw_name, optional) = if let Some(stripped) = raw_name.strip_suffix('?') {
                    (stripped.trim(), true)
                } else {
                    (raw_name, false)
                };
                let name = (!raw_name.is_empty()).then(|| self.ctx.types.intern_string(raw_name));
                (name, optional, type_str)
            } else if !rest && elem.ends_with('?') {
                (None, true, elem[..elem.len() - 1].trim())
            } else {
                (None, false, elem)
            };

            let type_id = self.resolve_jsdoc_type_str(type_str)?;
            elements.push(TupleElement {
                type_id,
                name,
                optional,
                rest,
            });
        }

        Some(self.ctx.types.factory().tuple(elements))
    }
    /// Parse an inline object literal type: `{ propName: Type, ... }`.
    fn parse_jsdoc_object_literal_type(&mut self, type_expr: &str) -> Option<TypeId> {
        if let Some(mapped) = self.parse_jsdoc_mapped_type(type_expr) {
            return Some(mapped);
        }

        let inner = type_expr[1..type_expr.len() - 1].trim();
        if inner.is_empty() {
            return Some(self.ctx.types.factory().object(Vec::new()));
        }
        // Split properties by ',' or ';' at top level
        let prop_strs = Self::split_object_properties(inner);
        let mut properties = Vec::new();
        for prop_str in &prop_strs {
            let prop_str = prop_str.trim();
            if prop_str.is_empty() {
                continue;
            }
            if let Some(paren_idx) = Self::find_top_level_char(prop_str, '(') {
                let colon_idx = Self::find_top_level_char(prop_str, ':');
                if colon_idx.is_none_or(|idx| paren_idx < idx) {
                    if paren_idx == 0 {
                        if let Some(func_ty) = self.parse_jsdoc_call_signature(prop_str) {
                            return Some(func_ty);
                        }
                    } else if let Some(prop) =
                        self.parse_jsdoc_method_signature(prop_str, paren_idx, &properties)
                    {
                        properties.push(prop);
                        continue;
                    }
                }
            }
            if let Some(colon_idx) = Self::find_top_level_char(prop_str, ':') {
                let raw_name = prop_str[..colon_idx].trim();
                let type_str = prop_str[colon_idx + 1..].trim();
                let (name, optional) = if let Some(stripped) = raw_name.strip_suffix('?') {
                    (stripped, true)
                } else {
                    (raw_name, false)
                };
                if !name.is_empty() {
                    let prop_type = self.resolve_jsdoc_type_str(type_str)?;
                    let name_atom = self.ctx.types.intern_string(name);
                    properties.push(PropertyInfo {
                        name: name_atom,
                        type_id: prop_type,
                        write_type: prop_type,
                        optional,
                        readonly: false,
                        is_method: false,
                        is_class_prototype: false,
                        visibility: Visibility::Public,
                        parent_id: None,
                        declaration_order: (properties.len() + 1) as u32,
                    });
                }
            }
        }
        if properties.is_empty() {
            return None;
        }
        Some(self.ctx.types.factory().object(properties))
    }

    fn parse_jsdoc_mapped_type(&mut self, type_expr: &str) -> Option<TypeId> {
        let inner = type_expr[1..type_expr.len() - 1].trim();
        if !inner.starts_with('[') {
            return None;
        }

        let mut square_depth = 0u32;
        let mut close_bracket = None;
        for (idx, ch) in inner.char_indices() {
            match ch {
                '[' => square_depth += 1,
                ']' => {
                    square_depth = square_depth.saturating_sub(1);
                    if square_depth == 0 {
                        close_bracket = Some(idx);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close_bracket = close_bracket?;
        let header = inner[1..close_bracket].trim();
        let template_str = inner[close_bracket + 1..].trim().strip_prefix(':')?.trim();

        let in_idx = header.find(" in ")?;
        let type_param_name = header[..in_idx].trim();
        let constraint_str = header[in_idx + 4..].trim();
        if type_param_name.is_empty() || constraint_str.is_empty() || template_str.is_empty() {
            return None;
        }

        let constraint = self.resolve_jsdoc_type_str(constraint_str)?;
        let atom = self.ctx.types.intern_string(type_param_name);
        let type_param = tsz_solver::TypeParamInfo {
            name: atom,
            constraint: Some(constraint),
            default: None,
            is_const: false,
        };
        let type_param_id = self.ctx.types.factory().type_param(type_param);
        let previous = self
            .ctx
            .type_parameter_scope
            .insert(type_param_name.to_string(), type_param_id);
        let template = self.resolve_jsdoc_type_str(template_str);
        if let Some(previous) = previous {
            self.ctx
                .type_parameter_scope
                .insert(type_param_name.to_string(), previous);
        } else {
            self.ctx.type_parameter_scope.remove(type_param_name);
        }

        template.map(|template| {
            self.ctx.types.factory().mapped(tsz_solver::MappedType {
                type_param,
                constraint,
                name_type: None,
                template,
                readonly_modifier: None,
                optional_modifier: None,
            })
        })
    }

    /// Parse a named method signature from a JSDoc object property string.
    /// Parse a call signature `(params): RetType` and return a function TypeId.
    fn parse_jsdoc_call_signature(&mut self, prop_str: &str) -> Option<TypeId> {
        use tsz_solver::{FunctionShape, ParamInfo};
        let after_open = &prop_str[1..];
        let mut depth = 1u32;
        let mut close_idx = None;
        for (i, ch) in after_open.char_indices() {
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
        let close_idx = close_idx?;
        let params_inner = after_open[..close_idx].trim();
        let after_close = after_open[close_idx + 1..].trim();
        let return_type = if let Some(rest) = after_close.strip_prefix(':') {
            self.jsdoc_type_from_expression(rest.trim())
                .unwrap_or(TypeId::VOID)
        } else {
            TypeId::VOID
        };
        let mut params = Vec::new();
        if !params_inner.is_empty() {
            for p in Self::split_top_level_params(params_inner) {
                let p = p.trim();
                if p.is_empty() {
                    continue;
                }
                let (name, t_str) = if let Some(colon) = p.find(':') {
                    (Some(p[..colon].trim()), p[colon + 1..].trim())
                } else {
                    (None, p)
                };
                let p_type = self
                    .jsdoc_type_from_expression(t_str)
                    .unwrap_or(TypeId::ANY);
                let atom = name.map(|n| self.ctx.types.intern_string(n));
                params.push(ParamInfo {
                    name: atom,
                    type_id: p_type,
                    optional: false,
                    rest: false,
                });
            }
        }
        let shape = FunctionShape {
            type_params: Vec::new(),
            params,
            this_type: None,
            return_type,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        };
        Some(self.ctx.types.factory().function(shape))
    }
    fn parse_jsdoc_method_signature(
        &mut self,
        prop_str: &str,
        paren_idx: usize,
        existing_props: &[PropertyInfo],
    ) -> Option<PropertyInfo> {
        use tsz_solver::{FunctionShape, ParamInfo};
        let method_name = prop_str[..paren_idx].trim();
        if method_name.is_empty() {
            return None;
        }
        // Handle optional method: `name?(...)`
        let (method_name, optional) = if let Some(stripped) = method_name.strip_suffix('?') {
            (stripped.trim(), true)
        } else {
            (method_name, false)
        };
        // Find the matching close paren
        let after_open = &prop_str[paren_idx + 1..];
        let mut depth = 1u32;
        let mut close_idx = None;
        for (i, ch) in after_open.char_indices() {
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
        let close_idx = close_idx?;
        let params_inner = after_open[..close_idx].trim();
        let after_close = after_open[close_idx + 1..].trim();
        // Return type follows ':'
        let return_type = if let Some(rest) = after_close.strip_prefix(':') {
            let return_type_str = rest.trim();
            self.jsdoc_type_from_expression(return_type_str)
                .unwrap_or(TypeId::VOID)
        } else {
            TypeId::VOID
        };
        // Parse parameters
        let mut params = Vec::new();
        if !params_inner.is_empty() {
            for p in Self::split_top_level_params(params_inner) {
                let p = p.trim();
                if p.is_empty() {
                    continue;
                }
                let (name, t_str) = if let Some(colon) = p.find(':') {
                    (Some(p[..colon].trim()), p[colon + 1..].trim())
                } else {
                    (None, p)
                };
                let p_type = self
                    .jsdoc_type_from_expression(t_str)
                    .unwrap_or(TypeId::ANY);
                let atom = name.map(|n| self.ctx.types.intern_string(n));
                params.push(ParamInfo {
                    name: atom,
                    type_id: p_type,
                    optional: false,
                    rest: false,
                });
            }
        }
        let shape = FunctionShape {
            type_params: Vec::new(),
            params,
            this_type: None,
            return_type,
            type_predicate: None,
            is_constructor: false,
            is_method: true,
        };
        let method_type = self.ctx.types.factory().function(shape);
        let name_atom = self.ctx.types.intern_string(method_name);
        Some(PropertyInfo {
            name: name_atom,
            type_id: method_type,
            write_type: method_type,
            optional,
            readonly: false,
            is_method: true,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: (existing_props.len() + 1) as u32,
        })
    }
    /// Resolve a `@typedef` referenced by name from JSDoc comments.
    ///
    /// In tsc, `@typedef`/`@callback` declarations are hoisted to file scope,
    /// so forward references (usage before definition) are valid.  We scan all
    /// comments in the file regardless of position, matching tsc's behavior.
    pub(crate) fn resolve_jsdoc_typedef_type(
        &mut self,
        type_expr: &str,
        _anchor_pos: u32,
        comments: &[tsz_common::comments::CommentRange],
        source_text: &str,
    ) -> Option<TypeId> {
        self.resolve_jsdoc_typedef_info(type_expr, comments, source_text)
            .map(|(body_type, _)| body_type)
            .or(Some(TypeId::ANY))
    }

    pub(crate) fn resolve_jsdoc_typedef_info(
        &mut self,
        type_expr: &str,
        comments: &[tsz_common::comments::CommentRange],
        source_text: &str,
    ) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};

        // Re-entrancy guard: recursive @typedef like `@typedef {... | Json[]} Json`
        // causes type_from_jsdoc_typedef → jsdoc_type_from_expression →
        // resolve_jsdoc_type_name → resolve_jsdoc_typedef_type infinite loop.
        // If we're already resolving this typedef, return None so the caller
        // falls through to the file_locals symbol lookup which returns a Lazy
        // placeholder that properly defers the recursive reference.
        if self
            .ctx
            .jsdoc_typedef_resolving
            .borrow()
            .contains(type_expr)
        {
            return None;
        }

        let mut best_def: Option<JsdocTypedefInfo> = None;
        for comment in comments {
            if !is_jsdoc_comment(comment, source_text) {
                continue;
            }
            let content = get_jsdoc_content(comment, source_text);
            for (name, typedef_info) in Self::parse_jsdoc_typedefs(&content) {
                if name != type_expr {
                    continue;
                }
                best_def = Some(typedef_info);
            }
        }
        let typedef_info = best_def?;

        // Mark this typedef as being resolved to prevent re-entrancy.
        self.ctx
            .jsdoc_typedef_resolving
            .borrow_mut()
            .insert(type_expr.to_owned());

        let result = self.type_from_jsdoc_typedef(typedef_info);

        self.ctx
            .jsdoc_typedef_resolving
            .borrow_mut()
            .remove(type_expr);

        if let Some((ty, _)) = result.as_ref() {
            self.register_jsdoc_typedef_def(type_expr, *ty);
        }
        result
    }
    fn type_from_jsdoc_typedef(
        &mut self,
        info: JsdocTypedefInfo,
    ) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        let factory = self.ctx.types.factory();
        let mut type_param_infos = Vec::with_capacity(info.template_params.len());
        let mut scope_updates = Vec::with_capacity(info.template_params.len());
        for template in &info.template_params {
            let constraint = template
                .constraint
                .as_deref()
                .and_then(|expr| self.resolve_jsdoc_type_str(expr));
            let atom = self.ctx.types.intern_string(&template.name);
            let param = tsz_solver::TypeParamInfo {
                name: atom,
                constraint,
                default: None,
                is_const: false,
            };
            let type_id = factory.type_param(param);
            let previous = self
                .ctx
                .type_parameter_scope
                .insert(template.name.clone(), type_id);
            type_param_infos.push(param);
            scope_updates.push((template.name.clone(), previous));
        }

        let result = if let Some(cb) = info.callback {
            self.type_from_jsdoc_callback(cb, &type_param_infos)
        } else {
            self.type_from_jsdoc_object_typedef(info)
        };

        for (name, previous) in scope_updates.into_iter().rev() {
            if let Some(previous) = previous {
                self.ctx.type_parameter_scope.insert(name, previous);
            } else {
                self.ctx.type_parameter_scope.remove(&name);
            }
        }

        result.map(|type_id| (type_id, type_param_infos))
    }

    fn type_from_jsdoc_callback(
        &mut self,
        cb: JsdocCallbackInfo,
        type_params: &[tsz_solver::TypeParamInfo],
    ) -> Option<TypeId> {
        let factory = self.ctx.types.factory();
        let mut params = Vec::new();
        let mut this_type = None;
        let nested_entries: Vec<(String, String, bool)> = cb
            .params
            .iter()
            .filter_map(|param| {
                (param.name.contains('.') || param.name.contains("[]")).then_some((
                    param.name.clone(),
                    param.type_expr.clone().unwrap_or_else(|| "any".to_string()),
                    param.optional,
                ))
            })
            .collect();

        for param in &cb.params {
            if param.name.contains('.') || param.name.contains("[]") {
                continue;
            }

            let raw_type_expr = param.type_expr.clone().unwrap_or_else(|| "any".to_string());
            let effective_expr = raw_type_expr.trim_end_matches('=').trim();
            let effective_expr = if param.rest {
                effective_expr.trim_start_matches("...").trim()
            } else {
                effective_expr
            };

            let is_object_base = effective_expr == "Object" || effective_expr == "object";
            let is_array_object_base = effective_expr == "Object[]"
                || effective_expr == "object[]"
                || effective_expr == "Array.<Object>"
                || effective_expr == "Array.<object>"
                || effective_expr == "Array<Object>"
                || effective_expr == "Array<object>";

            let mut type_id =
                if (is_object_base || is_array_object_base) && !nested_entries.is_empty() {
                    self.build_nested_param_object_type_from_entries(
                        &nested_entries,
                        &param.name,
                        is_array_object_base,
                    )
                    .or_else(|| self.jsdoc_type_from_expression(effective_expr))
                    .unwrap_or(TypeId::ANY)
                } else {
                    self.jsdoc_type_from_expression(effective_expr)
                        .unwrap_or(TypeId::ANY)
                };

            if param.rest {
                type_id = factory.array(type_id);
            }

            if param.name == "this" {
                this_type = Some(type_id);
                continue;
            }

            let name_atom = self.ctx.types.intern_string(&param.name);
            params.push(ParamInfo {
                name: Some(name_atom),
                type_id,
                optional: param.optional,
                rest: param.rest,
            });
        }

        let mut type_predicate = None;
        let return_type = if let Some((is_asserts, param_name, type_str)) = cb.predicate {
            let pred_type = type_str
                .as_deref()
                .and_then(|s| self.jsdoc_type_from_expression(s));
            let target = if param_name == "this" {
                TypePredicateTarget::This
            } else {
                let atom = self.ctx.types.intern_string(&param_name);
                TypePredicateTarget::Identifier(atom)
            };
            let parameter_index = if param_name != "this" {
                params.iter().position(|param| {
                    param
                        .name
                        .is_some_and(|name| name == self.ctx.types.intern_string(&param_name))
                })
            } else {
                None
            };
            type_predicate = Some(TypePredicate {
                asserts: is_asserts,
                target,
                type_id: pred_type,
                parameter_index,
            });
            if is_asserts {
                TypeId::VOID
            } else {
                TypeId::BOOLEAN
            }
        } else if let Some(ref ret_expr) = cb.return_type {
            self.jsdoc_type_from_expression(ret_expr)
                .unwrap_or(TypeId::ANY)
        } else {
            TypeId::VOID
        };

        Some(factory.function(FunctionShape {
            type_params: type_params.to_vec(),
            params,
            this_type,
            return_type,
            type_predicate,
            is_constructor: false,
            is_method: false,
        }))
    }

    fn type_from_jsdoc_object_typedef(&mut self, info: JsdocTypedefInfo) -> Option<TypeId> {
        let factory = self.ctx.types.factory();
        let base_type = if let Some(base_type_expr) = &info.base_type {
            let expr = base_type_expr.trim();
            if expr != "Object" && expr != "object" {
                return self.resolve_jsdoc_type_str(expr);
            }
            None
        } else {
            None
        };
        let mut top_level = Vec::new();
        let mut nested_entries = Vec::new();
        for prop in info.properties {
            if prop.name.contains('.') {
                nested_entries.push((prop.name, prop.type_expr, prop.optional));
            } else {
                top_level.push(prop);
            }
        }
        let mut prop_infos = Vec::with_capacity(top_level.len());
        for prop in top_level {
            let mut prop_type = if prop.type_expr.trim().is_empty() {
                TypeId::ANY
            } else {
                self.jsdoc_type_from_expression(&prop.type_expr)
                    .unwrap_or(TypeId::ANY)
            };
            let effective_expr = prop.type_expr.trim_end_matches('=').trim();
            let is_array_object_base = effective_expr == "Object[]"
                || effective_expr == "object[]"
                || effective_expr == "Array.<Object>"
                || effective_expr == "Array.<object>"
                || effective_expr == "Array<Object>"
                || effective_expr == "Array<object>";
            if let Some(built) = self.build_nested_param_object_type_from_entries(
                &nested_entries,
                &prop.name,
                is_array_object_base,
            ) {
                prop_type = built;
            }
            if prop.optional
                && self.ctx.strict_null_checks()
                && prop_type != TypeId::ANY
                && prop_type != TypeId::UNDEFINED
            {
                prop_type = factory.union2(prop_type, TypeId::UNDEFINED);
            }
            let name_atom = self.ctx.types.intern_string(&prop.name);
            prop_infos.push(PropertyInfo {
                name: name_atom,
                type_id: prop_type,
                write_type: prop_type,
                optional: prop.optional,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
            });
        }
        let object_type = if !prop_infos.is_empty() {
            Some(factory.object(prop_infos))
        } else {
            None
        };
        match (object_type, base_type) {
            (Some(obj), Some(base)) => Some(factory.intersection2(obj, base)),
            (Some(obj), None) => Some(obj),
            (None, Some(base)) => Some(base),
            (None, None) => None,
        }
    }

    fn resolve_jsdoc_generic_typedef_type(
        &mut self,
        base_name: &str,
        type_args: &[TypeId],
    ) -> Option<TypeId> {
        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};

        let source_file = self.ctx.arena.source_files.first()?;
        let mut best_def = None;
        for comment in &source_file.comments {
            if !is_jsdoc_comment(comment, &source_file.text) {
                continue;
            }
            let content = get_jsdoc_content(comment, &source_file.text);
            for (name, typedef_info) in Self::parse_jsdoc_typedefs(&content) {
                if name == base_name {
                    best_def = Some(typedef_info);
                }
            }
        }

        let (body_type, type_params) = self.type_from_jsdoc_typedef(best_def?)?;
        if type_params.is_empty() || type_args.is_empty() {
            return Some(body_type);
        }

        use tsz_solver::instantiate_generic;
        let instantiated = instantiate_generic(self.ctx.types, body_type, &type_params, type_args);
        let args_display = type_args
            .iter()
            .map(|&arg| self.format_type_diagnostic(arg))
            .collect::<Vec<_>>()
            .join(", ");
        let display_name = format!("{base_name}<{args_display}>");
        let _ = self.ensure_jsdoc_instantiated_display_def(&display_name, instantiated);
        Some(instantiated)
    }
    // NOTE: jsdoc_has_readonly_tag, jsdoc_access_level, find_orphaned_extends_tags_for_statements,
    // is_in_different_function_scope, find_function_body_end are in lookup.rs
}
