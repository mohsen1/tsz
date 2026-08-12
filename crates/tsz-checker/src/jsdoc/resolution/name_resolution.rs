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
use crate::query_boundaries::jsdoc_construction::{
    self as jsdoc_construct, jsdoc_function_type, jsdoc_literal_boolean_type,
    jsdoc_literal_number_type, jsdoc_literal_string_type, jsdoc_object_index_type,
    jsdoc_param_info, jsdoc_readonly_type, jsdoc_type_predicate,
};
use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tsz_binder::symbol_flags;
use tsz_common::numeric::parse_numeric_literal_value;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

/// How a JSDoc name reference consumes the resolved symbol.
///
/// TypeScript 7 dropped JS constructor-function inference, so a *bare* type
/// reference to a function-valued binding (`@param {fn}`) is the TS2749
/// value-used-as-type error, while a `typeof` query over the same binding
/// still legitimately yields the function's value type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::jsdoc) enum JsdocNameMode {
    /// A bare type-position reference (`@param {Name}`).
    BareTypeReference,
    /// A value-position consumer (`typeof Name`, internal export walks).
    ValuePosition,
}

/// Strip a leading and matching trailing `"` or `'` from `s` if both are
/// present. Returns the bare inner string when stripped, otherwise `None`.
fn strip_quoted_string(s: &str) -> Option<&str> {
    s.strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            s.strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        })
}

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

    fn jsdoc_type_expr_may_be_numeric_literal(type_expr: &str) -> bool {
        type_expr.bytes().any(|byte| byte.is_ascii_digit())
            && type_expr.bytes().all(|byte| {
                byte.is_ascii_hexdigit()
                    || matches!(byte, b'o' | b'O' | b'x' | b'X' | b'.' | b'_' | b'+' | b'-')
            })
    }

    pub(crate) fn resolve_jsdoc_implicit_any_builtin_type(
        &mut self,
        type_expr: &str,
    ) -> Option<TypeId> {
        match type_expr {
            "Array" | "array" => Some(jsdoc_construct::jsdoc_array_type(
                self.ctx.types,
                TypeId::ANY,
            )),
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
        let resolution_mode = Self::jsdoc_import_type_resolution_mode(type_expr);

        if let Some(member_name) = member_name {
            if let Some(sym_id) = self.resolve_jsdoc_import_member_with_mode(
                &module_specifier,
                &member_name,
                resolution_mode,
            ) {
                // `import(...).Member` (without a leading `typeof`) is a bare
                // type-position reference: `Member` must be type-eligible
                // (interface/class/enum/type-alias/namespace/typedef), not a
                // plain value export. `BareTypeReference` mode rejects a
                // plain-value symbol (returns `ERROR`) the same way a local
                // bare name reference already does; `ValuePosition` mode would
                // silently hand back the value's own type instead.
                let resolved = self
                    .resolve_jsdoc_symbol_type_with_mode(sym_id, JsdocNameMode::BareTypeReference);
                if resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN {
                    return Some(resolved);
                }
            }
            if let Some(typedef_type) =
                self.resolve_import_type_jsdoc_typedef(&module_specifier, &member_name, None)
            {
                return Some(typedef_type);
            }
            // A CommonJS expando export (`module.exports.Member = Member` /
            // `exports.Member = Member`) records no SymbolId in the binder's
            // export tables — those only track ES `export` syntax — so the
            // plain symbol lookup above never sees it. `commonjs_named_export_class_symbol_for_file`
            // (the synthesized-export-surface query boundary) recognizes the
            // expando assignment and, when its RHS is a class declaration's own
            // identifier, hands back that class's real SymbolId. This mirrors
            // the same fallback already used for the `const { X } = require(...)`
            // binding-element path in `resolve_jsdoc_commonjs_binding_element_type`.
            if let Some((export_sym_id, export_file_idx)) = self
                .resolve_js_export_named_class_symbol(
                    &module_specifier,
                    &member_name,
                    Some(self.ctx.current_file_idx),
                )
            {
                self.ctx
                    .register_symbol_file_target(export_sym_id, export_file_idx);
                let resolved = self.resolve_jsdoc_symbol_type_with_mode(
                    export_sym_id,
                    JsdocNameMode::BareTypeReference,
                );
                if resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN {
                    return Some(resolved);
                }
            }
            // Neither a type-eligible export nor a JSDoc `@typedef` named
            // `member_name` exists on the module: tsc reports TS2694
            // ("Namespace has no exported member") the same way the
            // TS-syntax `import(...).Member` resolver does. This mirrors
            // that resolver's `report_missing_import_type_member`, which the
            // string-based JSDoc parse path cannot reach directly.
            //
            // Reported once, at the member token, only by the comment-scan
            // validation pass; the lazy type computations that also resolve this
            // string are gated to a no-op here (issue #17176).
            self.emit_jsdoc_import_type_member_missing(&module_specifier, &member_name, type_expr);
            return None;
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
    ///
    /// This is the *structural* step only: unions, intersections, arrays,
    /// tuples, primitives, `function(...)`/arrow syntax, indexed access, etc. It
    /// deliberately does **not** perform named-reference resolution — a bare
    /// name such as a `@callback`, a sibling `@typedef`, or a class/interface
    /// returns `None` here. Callers resolving a member/parameter/return type
    /// expression must use [`Self::resolve_jsdoc_reference`] (the full resolver:
    /// structural step → object-literal → name resolution); using this function
    /// directly silently collapses named references to `any` (see issue
    /// #14850). It is `pub(crate)` only so `resolve_jsdoc_reference` and the
    /// recursive structural sub-parsers can call it.
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
                Some(jsdoc_construct::jsdoc_union_type(self.ctx.types, members))
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
                Some(jsdoc_construct::jsdoc_intersection_type(
                    self.ctx.types,
                    members,
                ))
            };
        }
        if type_expr == "?" {
            return Some(TypeId::ANY);
        }
        if let Some(inner) = type_expr.strip_prefix("readonly ") {
            let inner_type = self.resolve_jsdoc_type_str(inner.trim())?;
            return Some(jsdoc_readonly_type(self.ctx.types, inner_type));
        }
        if let Some(inner) = type_expr.strip_prefix('?') {
            let inner = inner.trim();
            if !inner.is_empty()
                && let Some(inner_type) = self.resolve_jsdoc_type_str(inner)
            {
                return Some(jsdoc_construct::jsdoc_union_pair_type(
                    self.ctx.types,
                    inner_type,
                    TypeId::NULL,
                ));
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
                    return Some(jsdoc_construct::jsdoc_union_pair_type(
                        self.ctx.types,
                        inner_type,
                        TypeId::NULL,
                    ));
                }
            } else if let Some(inner) = type_expr.strip_suffix('=') {
                if let Some(inner_type) = self.resolve_jsdoc_type_str(inner) {
                    return Some(jsdoc_construct::jsdoc_union_pair_type(
                        self.ctx.types,
                        inner_type,
                        TypeId::UNDEFINED,
                    ));
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
            return Some(jsdoc_construct::jsdoc_array_type(
                self.ctx.types,
                element_type,
            ));
        }
        if !type_expr.starts_with('[')
            && type_expr.ends_with(']')
            && let Some((base_str, index_str)) = Self::parse_jsdoc_index_access_segments(type_expr)
        {
            // tsc reports TS2339 when a JSDoc indexed-access type uses a
            // string-literal key the imported module doesn't export
            // (e.g. `import("./dep")["Foo"]`). For an `import(...)` base
            // we resolve the member directly via the import resolver: the
            // bare `import("./dep")` form does not round-trip through
            // `resolve_jsdoc_type_str` for ESM-only modules (no
            // commonjs-style module value type exists), so we cannot rely
            // on the structural property lookup below. #3213.
            if base_str.starts_with("import(")
                && let Some(key) = strip_quoted_string(index_str)
                && let Some((module_specifier, None)) = Self::parse_jsdoc_import_type(base_str)
                && self
                    .resolve_jsdoc_import_member(&module_specifier, key)
                    .is_none()
            {
                let display = format!("typeof import(\"{module_specifier}\")");
                let message = crate::diagnostics::format_message(
                    crate::diagnostics::diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                    &[key, &display],
                );
                let anchor = self.ctx.jsdoc_typedef_anchor_pos.get();
                self.ctx.error(
                    anchor,
                    type_expr.len() as u32,
                    message,
                    crate::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                );
                return Some(TypeId::ERROR);
            }
            let base_type = self.resolve_jsdoc_type_str(base_str)?;
            let index_type = self.resolve_jsdoc_type_str(index_str)?;
            // Same TS2339 rule for the structural case (e.g. when the base
            // resolves to a real type with a string-literal index).
            if let Some(key_atom) =
                crate::query_boundaries::common::string_literal_value(self.ctx.types, index_type)
            {
                let key = self.ctx.types.resolve_atom_ref(key_atom).to_string();
                let lookup = crate::query_boundaries::property_access::resolve_property_access(
                    self.ctx.types,
                    base_type,
                    key_atom,
                );
                if matches!(
                    lookup,
                    crate::query_boundaries::common::PropertyAccessResult::PropertyNotFound { .. }
                ) {
                    let display = self.format_type_diagnostic(base_type);
                    let message = crate::diagnostics::format_message(
                        crate::diagnostics::diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                        &[&key, &display],
                    );
                    let anchor = self.ctx.jsdoc_typedef_anchor_pos.get();
                    self.ctx.error(
                        anchor,
                        type_expr.len() as u32,
                        message,
                        crate::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                    );
                    return Some(TypeId::ERROR);
                }
            }
            return Some(jsdoc_construct::jsdoc_index_access_type(
                self.ctx.types,
                base_type,
                index_type,
            ));
        }
        if type_expr.starts_with('[') && type_expr.ends_with(']') {
            return self.parse_jsdoc_tuple_type(type_expr);
        }
        if ((type_expr.starts_with('"') && type_expr.ends_with('"'))
            || (type_expr.starts_with('\'') && type_expr.ends_with('\'')))
            && type_expr.len() >= 2
        {
            let inner = &type_expr[1..type_expr.len() - 1];
            return Some(jsdoc_literal_string_type(self.ctx.types, inner));
        }
        if type_expr == "true" {
            return Some(jsdoc_literal_boolean_type(self.ctx.types, true));
        }
        if type_expr == "false" {
            return Some(jsdoc_literal_boolean_type(self.ctx.types, false));
        }
        if Self::jsdoc_type_expr_may_be_numeric_literal(type_expr)
            && let Some(n) = parse_numeric_literal_value(type_expr)
        {
            return Some(jsdoc_literal_number_type(self.ctx.types, n));
        }
        if let Some(ty) = self.resolve_jsdoc_implicit_any_builtin_type(type_expr) {
            return Some(ty);
        }
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
                    let parts = Self::split_type_args_respecting_nesting(inner);
                    if parts.len() != 2 {
                        return None;
                    }
                    let key_str = parts[0].trim();
                    let value_str = parts[1].trim();
                    if let (Some(key_type), Some(value_type)) = (
                        self.jsdoc_type_from_expression(key_str),
                        self.jsdoc_type_from_expression(value_str),
                    ) && let Some(object_type) =
                        jsdoc_object_index_type(self.ctx.types, key_type, value_type, false, None)
                    {
                        return Some(object_type);
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
                                let keyof_t_id =
                                    jsdoc_construct::jsdoc_keyof_type(self.ctx.types, t_id);
                                let k_param = jsdoc_construct::jsdoc_type_param_info(
                                    k_atom,
                                    Some(keyof_t_id),
                                    None,
                                );
                                let k_id =
                                    jsdoc_construct::jsdoc_type_param_type(self.ctx.types, k_param);
                                let t_k_id = jsdoc_construct::jsdoc_index_access_type(
                                    self.ctx.types,
                                    t_id,
                                    k_id,
                                );
                                let template_id = jsdoc_function_type(
                                    self.ctx.types,
                                    Vec::new(),
                                    vec![jsdoc_param_info(
                                        Some(self.ctx.types.intern_string("value")),
                                        t_k_id,
                                        false,
                                        false,
                                    )],
                                    None,
                                    TypeId::VOID,
                                    None,
                                    false,
                                    false,
                                );
                                return Some(jsdoc_construct::jsdoc_mapped_type(
                                    self.ctx.types,
                                    k_param,
                                    keyof_t_id,
                                    template_id,
                                    None,
                                ));
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
                // TypeScript 7 does not accept the Closure `function(...)` form.
                // The syntax error is TS1005, reported in
                // `jsdoc/closure_function_type.rs`; the type itself must not
                // resolve, so the annotated symbol gains no contextual signature.
                // Its parameters then fall to implicit `any` (TS7006) and the
                // assignability errors a reconstructed signature used to produce
                // disappear, which is what the oracle expects.
                if let Some(rest) = type_expr.strip_prefix("function")
                    && rest.trim_start().starts_with('(')
                {
                    return None;
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
                                if !sym.has_any_flags(value_mask) {
                                    continue;
                                }
                                let operand = self.get_type_of_symbol(sym_id);
                                if operand == TypeId::ERROR {
                                    continue;
                                }
                                let keyof =
                                    jsdoc_construct::jsdoc_keyof_type(self.ctx.types, operand);
                                return Some(self.judge_evaluate(keyof));
                            }
                        }
                    }
                    if !rest.is_empty()
                        && let Some(operand) = self.resolve_jsdoc_type_str(rest)
                    {
                        let keyof = jsdoc_construct::jsdoc_keyof_type(self.ctx.types, operand);
                        return Some(self.judge_evaluate(keyof));
                    }
                }
                if let Some(angle_idx) = Self::find_top_level_char(type_expr, '<') {
                    let raw_base_name = type_expr[..angle_idx].trim();
                    let base_name = raw_base_name.strip_suffix('.').unwrap_or(raw_base_name);
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
            for tp_name in tp_str.split(',') {
                let tp_name = tp_name.trim();
                if tp_name.is_empty() {
                    continue;
                }
                let (name, constraint_str) = Self::split_jsdoc_type_param_constraint(tp_name);
                let constraint = constraint_str.and_then(|s| self.jsdoc_type_from_expression(s));
                let atom = self.ctx.types.intern_string(name);
                let info = jsdoc_construct::jsdoc_type_param_info(atom, constraint, None);
                let ty = jsdoc_construct::jsdoc_type_param_type(self.ctx.types, info);
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
                    params.push(jsdoc_param_info(atom, p_type, false, is_rest));
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

        Some(jsdoc_function_type(
            self.ctx.types,
            jsdoc_type_params,
            params,
            this_type,
            return_type,
            type_predicate,
            false,
            false,
        ))
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
        let (is_asserts, rest) = Self::split_jsdoc_asserts_prefix(return_type_str);
        if is_asserts {
            // Check for `asserts param is Type`
            if let Some((is_idx, is_end)) = Self::find_jsdoc_type_predicate_is(rest) {
                let param_name = rest[..is_idx].trim();
                let type_str = rest[is_end..].trim();
                let pred_type = self.jsdoc_type_from_expression(type_str);
                let (target, parameter_index) =
                    self.jsdoc_type_predicate_target(param_name, params_inner);
                let predicate = jsdoc_type_predicate(true, target, pred_type, parameter_index);
                return (Some(TypeId::VOID), Some(predicate));
            }
            // `asserts param` (no type)
            let param_name = rest;
            let (target, parameter_index) =
                self.jsdoc_type_predicate_target(param_name, params_inner);
            let predicate = jsdoc_type_predicate(true, target, None, parameter_index);
            return (Some(TypeId::VOID), Some(predicate));
        }

        // Try `param is Type` (non-assertion type predicate)
        if let Some((is_idx, is_end)) = Self::find_jsdoc_type_predicate_is(return_type_str) {
            let param_name = return_type_str[..is_idx].trim();
            let type_str = return_type_str[is_end..].trim();
            // Validate that param_name is a simple identifier, not a type expression
            if param_name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
            {
                let pred_type = self.jsdoc_type_from_expression(type_str);
                let (target, parameter_index) =
                    self.jsdoc_type_predicate_target(param_name, params_inner);
                let predicate = jsdoc_type_predicate(false, target, pred_type, parameter_index);
                return (Some(TypeId::BOOLEAN), Some(predicate));
            }
        }

        // Regular return type
        (self.jsdoc_type_from_expression(return_type_str), None)
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

    /// Whether `type_expr` is a bare dotted identifier chain (`A.B`, `A.B.C`)
    /// with no generics, unions, intersections, groupings, object literals,
    /// arrays, or string literals — the only shape the value-root namespace
    /// check applies to. Richer forms carry their own resolution and diagnostics.
    pub(crate) fn jsdoc_type_expr_is_plain_qualified_name(type_expr: &str) -> bool {
        let mut chars = type_expr.chars().peekable();
        let mut saw_dot = false;
        let mut segment_len = 0usize;
        while let Some(&c) = chars.peek() {
            match c {
                '.' => {
                    if segment_len == 0 {
                        return false;
                    }
                    saw_dot = true;
                    segment_len = 0;
                }
                c if c.is_alphanumeric() || c == '_' || c == '$' => {
                    segment_len += 1;
                }
                _ => return false,
            }
            chars.next();
        }
        saw_dot && segment_len > 0
    }

    /// TS7: a qualified JSDoc type name `A.B(.C…)` can only be a type when its
    /// root `A` has namespace/type meaning — a namespace/module, class, enum,
    /// interface, type alias, or import alias. A plain runtime value (`var A =
    /// {}`, `function A(){}`, `var A = class {}`) that only grew members via JS
    /// special assignments (`A.B = …`) is not a namespace in type position, so
    /// tsc emits TS2503 "Cannot find namespace 'A'". This reports whether `A`
    /// has that plain-value shape. The root is resolved the same three ways the
    /// qualified-name resolver uses (current-file locals, lib globals, and
    /// sibling script files that share the JS salsa global scope), so the check
    /// is independent of which file declares the value.
    pub(crate) fn jsdoc_qualified_root_is_plain_value(&mut self, root_name: &str) -> bool {
        if root_name.is_empty() || root_name == "globalThis" {
            return false;
        }
        let sym_id = if let Some(sym_id) = self.ctx.binder.file_locals.get(root_name) {
            sym_id
        } else if let Some(sym_id) = self
            .ctx
            .lib_contexts
            .iter()
            .find_map(|ctx| ctx.binder.file_locals.get(root_name))
        {
            sym_id
        } else if let Some((sym_id, _file_idx)) =
            self.resolve_jsdoc_cross_file_root_symbol(root_name)
        {
            sym_id
        } else {
            return false;
        };

        let lib_binders = self.get_lib_binders();
        let (flags, has_import_module, declarations) = match self
            .get_cross_file_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders))
        {
            Some(symbol) => (
                symbol.flags,
                symbol.import_module().is_some(),
                symbol.declarations.clone(),
            ),
            None => return false,
        };

        // A real namespace/type anchor (or import alias) is a legitimate
        // qualifier; only pure runtime values are rejected. `VALUE_MODULE` /
        // `NAMESPACE_MODULE` cover declaration-merged namespaces and enums.
        let member_holder_flags = symbol_flags::MODULE
            | symbol_flags::NAMESPACE_MODULE
            | symbol_flags::VALUE_MODULE
            | symbol_flags::TYPE_ALIAS
            | symbol_flags::INTERFACE
            | symbol_flags::CLASS
            | symbol_flags::ENUM
            | symbol_flags::ALIAS;
        if flags & member_holder_flags != 0 {
            return false;
        }
        if flags & symbol_flags::VALUE == 0 {
            return false;
        }
        // `var mod = require("./x")` (and other imported values) navigate into
        // their module's exports; the qualified form resolves there, not here.
        if has_import_module {
            return false;
        }
        for decl_idx in declarations {
            if !decl_idx.is_some() {
                continue;
            }
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                continue;
            };
            if var_decl.initializer.is_some()
                && self
                    .get_require_module_specifier(var_decl.initializer)
                    .is_some()
            {
                return false;
            }
        }
        true
    }

    /// Extract a JSDoc `@this {T}` tag from a function's JSDoc comment and
    /// resolve it to a `TypeId`. Returns `None` when there is no `@this` tag
    /// or the referenced type cannot be resolved.
    pub(crate) fn resolve_jsdoc_this_type(&mut self, jsdoc: &str) -> Option<TypeId> {
        let this_expr = Self::extract_jsdoc_tag_type_expression(jsdoc, "this")?;
        self.resolve_jsdoc_reference(this_expr)
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
            let resolved =
                self.resolve_jsdoc_symbol_type_with_mode(sym_id, JsdocNameMode::BareTypeReference);
            if resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN {
                return Some(resolved);
            }
        }

        if let Some(sym_id) = self.resolve_jsdoc_entity_name_symbol(name) {
            let resolved =
                self.resolve_jsdoc_symbol_type_with_mode(sym_id, JsdocNameMode::BareTypeReference);
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

        if let Some(module_specifier) = symbol.import_module()
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
        let remaining_segments: Vec<_> = segments.collect();
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
            return self.resolve_jsdoc_js_require_export_type(
                &module_specifier,
                first_member,
                &remaining_segments,
            );
        }

        let mut current_sym = self.resolve_jsdoc_import_member(&module_specifier, first_member)?;
        for segment in remaining_segments {
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

    fn resolve_jsdoc_js_require_export_type(
        &mut self,
        module_specifier: &str,
        first_member: &str,
        remaining_segments: &[&str],
    ) -> Option<TypeId> {
        let mut current_type = self
            .resolve_js_export_named_type(
                module_specifier,
                first_member,
                Some(self.ctx.current_file_idx),
            )
            .and_then(|export_type| {
                self.instance_type_from_constructor_type(export_type)
                    .or(Some(export_type))
            })
            .filter(|&export_type| export_type != TypeId::ERROR && export_type != TypeId::UNKNOWN)
            .or_else(|| {
                let export_sym_id = self
                    .resolve_jsdoc_import_member(module_specifier, first_member)
                    .or_else(|| {
                        self.resolve_named_export_via_export_equals(module_specifier, first_member)
                    })
                    .or_else(|| {
                        let mut visited_aliases = AliasCycleTracker::new();
                        self.resolve_reexported_member_symbol(
                            module_specifier,
                            first_member,
                            &mut visited_aliases,
                        )
                    })?;
                let export_type = self.resolve_jsdoc_symbol_type(export_sym_id);
                (export_type != TypeId::ERROR && export_type != TypeId::UNKNOWN)
                    .then_some(export_type)
            })?;

        for segment in remaining_segments {
            let access = self.resolve_property_access_with_env(current_type, segment);
            current_type = match access {
                crate::query_boundaries::common::PropertyAccessResult::Success {
                    type_id, ..
                } => self.resolve_type_query_type(type_id),
                _ => return None,
            };
        }

        Some(current_type)
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
                if let Some(current_file_idx) = current_file_idx
                    && !self.ctx.has_symbol_file_index(member_sym)
                {
                    self.ctx
                        .register_symbol_file_target(member_sym, current_file_idx);
                }
                current_sym = member_sym;
                continue;
            }

            if let Some(module_specifier) = symbol.import_module() {
                let mut visited_aliases = AliasCycleTracker::new();
                if let Some(member_sym) = self.resolve_reexported_member_symbol(
                    module_specifier,
                    segment,
                    &mut visited_aliases,
                ) {
                    if let Some(current_file_idx) = current_file_idx
                        && !self.ctx.has_symbol_file_index(member_sym)
                    {
                        self.ctx
                            .register_symbol_file_target(member_sym, current_file_idx);
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

    pub(crate) fn resolve_jsdoc_cross_file_root_symbol(
        &mut self,
        root_name: &str,
    ) -> Option<(tsz_binder::SymbolId, usize)> {
        let all_binders = self.ctx.all_binders.as_ref()?;
        for (file_idx, binder) in all_binders.iter().enumerate() {
            if file_idx == self.ctx.current_file_idx {
                continue;
            }
            if self.jsdoc_file_idx_is_external_module(file_idx, binder) {
                continue;
            }
            if let Some(sym_id) = binder.file_locals.get(root_name) {
                return Some((sym_id, file_idx));
            }
        }

        None
    }

    fn jsdoc_file_idx_is_external_module(
        &self,
        file_idx: usize,
        binder: &tsz_binder::BinderState,
    ) -> bool {
        if binder.is_external_module() {
            return true;
        }

        let Some(all_arenas) = self.ctx.all_arenas.as_ref() else {
            return false;
        };
        let Some(arena) = all_arenas.get(file_idx) else {
            return false;
        };
        let Some(source_file) = arena.source_files.first() else {
            return false;
        };

        if let Some(is_external_module_by_file) = self.ctx.is_external_module_by_file.as_ref()
            && let Some(is_external_module) = crate::context::lookup_is_external_module_in_map(
                is_external_module_by_file,
                &source_file.file_name,
            )
        {
            return is_external_module;
        }

        source_file.statements.nodes.iter().any(|&stmt_idx| {
            arena.get(stmt_idx).is_some_and(|stmt| {
                stmt.kind == syntax_kind_ext::IMPORT_DECLARATION
                    || stmt.kind == syntax_kind_ext::EXPORT_DECLARATION
                    || stmt.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                    || stmt.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
            })
        })
    }

    /// The `(module specifier, export name)` a destructured `require` binding
    /// reads, e.g. `const { K: Local } = require("./mod")` -> `("./mod", "K")`.
    fn jsdoc_require_binding_export_origin(
        &mut self,
        value_decl: NodeIndex,
        local_name: &str,
    ) -> Option<(String, String)> {
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
        Some((module_specifier, export_name))
    }

    /// Whether a destructured `require` binding names something usable as a
    /// bare JSDoc **type**.
    ///
    /// `tsc` allows it only when the module's `exports.X = …` assigns a class
    /// directly. A plain value (`exports.v = 1`), a function, or a value reached
    /// through another object (`var NS = {}; NS.K = class {}; exports.K = NS.K`)
    /// carries only a value meaning, and using the imported name as a type is
    /// TS2749. The discriminator has to be syntactic: the direct and indirect
    /// class exports above resolve to the *same* type, yet tsc accepts only the
    /// direct one.
    fn jsdoc_require_binding_supplies_type(
        &mut self,
        value_decl: NodeIndex,
        local_name: &str,
    ) -> bool {
        let Some((module_specifier, export_name)) =
            self.jsdoc_require_binding_export_origin(value_decl, local_name)
        else {
            return true;
        };
        self.commonjs_named_export_assigns_a_class(
            &module_specifier,
            &export_name,
            Some(self.ctx.current_file_idx),
        )
    }

    fn resolve_jsdoc_commonjs_binding_element_type(
        &mut self,
        value_decl: NodeIndex,
        local_name: &str,
    ) -> Option<TypeId> {
        let (module_specifier, export_name) =
            self.jsdoc_require_binding_export_origin(value_decl, local_name)?;

        // TS7: `module.exports = { X }` object-literal members carry only value
        // meaning. A require-destructured binding of such a member is not a type
        // alias; returning `None` routes the bare reference to the TS2749
        // value-used-as-type terminal, matching tsc.
        if self.commonjs_named_export_is_object_literal_member(
            &module_specifier,
            &export_name,
            Some(self.ctx.current_file_idx),
        ) {
            return None;
        }

        let surface_export_type = self.resolve_js_export_named_type(
            &module_specifier,
            &export_name,
            Some(self.ctx.current_file_idx),
        );
        let mut surface_fallback = None;
        if let Some(export_type) = surface_export_type {
            // Synthesize an instance type only for class-like exports (a
            // class constructor type carries construct signatures and no
            // call signatures). An expando-exported plain function
            // (`exports.f = function () { this.q = 1 }`) also carries a
            // synthesized construct signature, but TS7 dropped
            // constructor-function inference: the caller's bare-reference
            // gate must see the raw function type and report TS2749.
            if !self.jsdoc_value_is_plain_callable(export_type)
                && let Some(instance_type) = self.instance_type_from_constructor_type(export_type)
            {
                return Some(instance_type);
            }
            if export_type != TypeId::ERROR && export_type != TypeId::UNKNOWN {
                surface_fallback = Some(export_type);
            }
        }

        if let Some((export_sym_id, export_file_idx)) = self.resolve_js_export_named_class_symbol(
            &module_specifier,
            &export_name,
            Some(self.ctx.current_file_idx),
        ) {
            self.ctx
                .register_symbol_file_target(export_sym_id, export_file_idx);
            let export_type = self.resolve_jsdoc_symbol_type(export_sym_id);
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
            });

        if let Some(export_sym_id) = export_sym_id {
            let export_type = self.resolve_jsdoc_symbol_type(export_sym_id);
            if export_type != TypeId::ERROR && export_type != TypeId::UNKNOWN {
                return Some(export_type);
            }
        }

        surface_fallback
    }

    pub(in crate::jsdoc) fn resolve_jsdoc_symbol_type(
        &mut self,
        sym_id: tsz_binder::SymbolId,
    ) -> TypeId {
        self.resolve_jsdoc_symbol_type_with_mode(sym_id, JsdocNameMode::ValuePosition)
    }

    pub(in crate::jsdoc) fn resolve_jsdoc_symbol_type_with_mode(
        &mut self,
        sym_id: tsz_binder::SymbolId,
        mode: JsdocNameMode,
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
                return self.resolve_jsdoc_symbol_type_with_mode(target, mode);
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

        // TypeScript 7 dropped JS constructor-function inference: a plain function
        // symbol has no instance type, so it carries no meaning in a JSDoc type
        // position (bare `@type {fn}`, `@param {fn}`, `import("./m").fn`, etc.).
        // The reference is a value-used-as-type error (TS2749), surfaced by the
        // JSDoc diagnostic path once resolution fails here. Do not synthesize an
        // instance type from `this.prop =` assignments or prototype methods.
        // This mirrors the value-side classification already adopted for `new`
        // targets and `this` typing.

        if symbol.has_any_flags(
            symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::BLOCK_SCOPED_VARIABLE,
        ) {
            let is_enum_tagged = symbol
                .declarations
                .iter()
                .copied()
                .filter(|decl| decl.is_some())
                .any(|decl| {
                    self.jsdoc_enum_annotation_type_for_symbol_decl(sym_id, decl)
                        .is_some()
                });
            if is_enum_tagged {
                // TS7 dropped `@enum` type synthesis: the tag creates only a
                // value binding, no type. A bare reference is the TS2749
                // value-used-as-type error; `typeof` queries (ValuePosition)
                // still get the object's value type, so fall through.
                if mode == JsdocNameMode::BareTypeReference {
                    return TypeId::ERROR;
                }
            } else if symbol.value_declaration.is_some()
                && let Some(instance_type) = self.resolve_jsdoc_commonjs_binding_element_type(
                    symbol.value_declaration,
                    symbol.escaped_name.as_str(),
                )
            {
                // A `require()`-imported binding names a TYPE only when the
                // module assigns a class directly to that export. TS7 also
                // dropped constructor-function inference, so an expando-exported
                // plain function is a value too. Failing here routes the
                // reference to the TS2749 value-used-as-type terminal;
                // `typeof` queries (ValuePosition) still get the value type.
                if mode == JsdocNameMode::BareTypeReference
                    && (self.jsdoc_value_is_plain_callable(instance_type)
                        || !self.jsdoc_require_binding_supplies_type(
                            symbol.value_declaration,
                            symbol.escaped_name.as_str(),
                        ))
                {
                    return TypeId::ERROR;
                }
                return instance_type;
            }
            let value_type = self.get_type_of_symbol(sym_id);
            // TS7: a plain value variable is never a valid bare JSDoc type.
            // Two require-alias forms keep type meaning and are resolved
            // before the value-only terminal:
            //   * `const {X} = require(...)` binding elements → CommonJS
            //     binding-element path above.
            //   * `const X = require("./mod")` whole-module aliases → the
            //     instance type of the module's construct-signature export
            //     (`module.exports = Chunk`) here.
            // Everything else reaching a bare position (plain objects, `@enum`
            // values, constructor-typed globals like `Image`, class-expression
            // vars, `require(...).member` property-access consts) is value-only
            // and reports TS2749. `typeof` queries (ValuePosition) still
            // consume the value type below.
            if mode == JsdocNameMode::BareTypeReference {
                if self.jsdoc_variable_is_whole_module_require_alias(symbol.value_declaration)
                    && let Some(instance_type) =
                        self.instance_type_from_constructor_type(value_type)
                {
                    return instance_type;
                }
                return TypeId::ERROR;
            }
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
            // Note: this fallback is load-bearing for `typeof` queries, which
            // route through here for the variable's value type.
            if value_type != TypeId::ERROR && value_type != TypeId::UNKNOWN {
                return value_type;
            }
        }

        TypeId::ERROR
    }

    /// Whether a value type is a plain callable — it carries call signatures,
    /// unlike a class constructor type (construct signatures only). Under
    /// TypeScript 7, such a value has no instance type, so a *bare* JSDoc type
    /// reference to it is the TS2749 value-used-as-type error; only `typeof`
    /// queries may still consume the value type.
    fn jsdoc_value_is_plain_callable(&mut self, value_type: TypeId) -> bool {
        let resolved = self.resolve_lazy_type(value_type);
        crate::query_boundaries::common::function_shape_for_type(self.ctx.types, resolved).is_some()
            || crate::query_boundaries::common::call_signatures_for_type(self.ctx.types, resolved)
                .is_some_and(|sigs| !sigs.is_empty())
    }

    /// Whether a value symbol's declaration is a whole-module require alias
    /// (`const X = require("./mod")`) — a CommonJS import-equals binding that
    /// keeps the target module's type meaning (e.g. `module.exports = Chunk`).
    /// Property-access consts (`const X = require("./mod").member`) and
    /// destructuring binding elements are excluded: the initializer must be a
    /// bare `require(...)` call directly bound to a plain identifier.
    fn jsdoc_variable_is_whole_module_require_alias(&mut self, value_decl: NodeIndex) -> bool {
        if !value_decl.is_some() {
            return false;
        }
        let Some(node) = self.ctx.arena.get(value_decl) else {
            return false;
        };
        let decl_idx = if node.kind == SyntaxKind::Identifier as u16 {
            let Some(ext) = self.ctx.arena.get_extended(value_decl) else {
                return false;
            };
            ext.parent
        } else {
            value_decl
        };
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return false;
        }
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return false;
        };
        // The binding target must be a plain identifier: a binding pattern
        // (`const {X} = require(...)`) is a destructure, not a whole-module
        // alias, and is handled by the CommonJS binding-element path.
        if self
            .ctx
            .arena
            .get(var_decl.name)
            .is_none_or(|name| name.kind != SyntaxKind::Identifier as u16)
        {
            return false;
        }
        var_decl.initializer.is_some()
            && self
                .get_require_module_specifier(var_decl.initializer)
                .is_some()
    }

    /// Returns `true` when a bare JSDoc type-position name resolves to a symbol
    /// that has a value meaning but no type meaning. Such a reference is the
    /// "value used as a type" error (TS2749) rather than a missing-name error
    /// (TS2304). Under TypeScript 7 this notably covers plain JS functions,
    /// whose dropped constructor-function inference leaves them value-only.
    ///
    /// Only consulted on the JSDoc diagnostic path once type resolution has
    /// already failed, so a value-only hit is unambiguous.
    pub(crate) fn jsdoc_name_refers_to_value_only(&mut self, name: &str) -> bool {
        if name.is_empty() || name.contains('.') {
            return false;
        }
        let Some(sym_id) = self.resolve_jsdoc_entity_name_symbol(name) else {
            return false;
        };
        let mut visited_aliases = AliasCycleTracker::new();
        let sym_id = self
            .resolve_alias_symbol(sym_id, &mut visited_aliases)
            .unwrap_or(sym_id);
        let Some(symbol) = self
            .get_cross_file_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol(sym_id))
        else {
            return false;
        };
        symbol.has_any_flags(symbol_flags::VALUE)
            && !symbol.has_any_flags(symbol_flags::TYPE | symbol_flags::MODULE)
    }
}
