//! JSDoc type annotation utilities for `CheckerState`.
use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::query_boundaries::type_checking_utilities as query;
use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_solver::{
    FunctionShape, IndexSignature, ObjectFlags, ObjectShape, ParamInfo, PropertyInfo, TupleElement,
    TypeId, TypePredicate, TypePredicateTarget, Visibility,
};
#[derive(Clone)]
pub(crate) struct JsdocTypedefInfo {
    pub(crate) base_type: Option<String>,
    pub(crate) properties: Vec<(String, String)>,
    /// If this is a `@callback` definition, holds the parsed parameter and return info.
    pub(crate) callback: Option<JsdocCallbackInfo>,
}
/// Parsed `@callback` information: parameter names/types and return type/predicate.
#[derive(Clone)]
pub(crate) struct JsdocCallbackInfo {
    pub(crate) params: Vec<(String, String)>, // (name, type_expr)
    pub(crate) return_type: Option<String>,   // raw return type expression
    /// Parsed type predicate from `@return {x is Type}`.
    pub(crate) predicate: Option<(bool, String, Option<String>)>, // (is_asserts, param_name, type_str)
}
impl<'a> CheckerState<'a> {
    pub(crate) fn resolve_global_jsdoc_typedef_type(&mut self, name: &str) -> Option<TypeId> {
        if !self.ctx.should_resolve_jsdoc() {
            return None;
        }

        if let Some(source_file) = self.ctx.arena.source_files.first() {
            let comments = source_file.comments.clone();
            let source_text = source_file.text.to_string();
            if let Some(ty) =
                self.resolve_jsdoc_typedef_type(name, u32::MAX, &comments, &source_text)
            {
                self.register_jsdoc_typedef_def(name, ty);
                return Some(ty);
            }
        }

        let all_arenas = self.ctx.all_arenas.clone()?;
        let all_binders = self.ctx.all_binders.clone()?;

        for (file_idx, (arena, binder)) in all_arenas.iter().zip(all_binders.iter()).enumerate() {
            if file_idx == self.ctx.current_file_idx {
                continue;
            }

            let Some(source_file) = arena.source_files.first() else {
                continue;
            };

            let comments = source_file.comments.clone();
            let source_text = source_file.text.to_string();
            let mut checker = Box::new(CheckerState::with_parent_cache(
                arena.as_ref(),
                binder.as_ref(),
                self.ctx.types,
                source_file.file_name.clone(),
                self.ctx.compiler_options.clone(),
                self,
            ));
            checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
            checker.ctx.all_arenas = Some(all_arenas.clone());
            checker.ctx.all_binders = Some(all_binders.clone());
            checker.ctx.resolved_module_paths = self.ctx.resolved_module_paths.clone();
            checker.ctx.module_specifiers = self.ctx.module_specifiers.clone();
            checker.ctx.current_file_idx = file_idx;
            if !self.ctx.cross_file_symbol_targets.borrow().is_empty() {
                *checker.ctx.cross_file_symbol_targets.borrow_mut() =
                    self.ctx.cross_file_symbol_targets.borrow().clone();
            }

            if let Some(ty) =
                checker.resolve_jsdoc_typedef_type(name, u32::MAX, &comments, &source_text)
            {
                for (&sym_id, &target_idx) in checker.ctx.cross_file_symbol_targets.borrow().iter()
                {
                    self.ctx
                        .cross_file_symbol_targets
                        .borrow_mut()
                        .insert(sym_id, target_idx);
                }
                self.register_jsdoc_typedef_def(name, ty);
                return Some(ty);
            }
        }

        None
    }

    pub(crate) fn source_file_data_for_node(
        &self,
        idx: NodeIndex,
    ) -> Option<&tsz_parser::parser::node::SourceFileData> {
        let mut current = idx;
        while current.is_some() {
            let node = self.ctx.arena.get(current)?;
            if let Some(source_file) = self.ctx.arena.get_source_file(node) {
                return Some(source_file);
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }
        None
    }
    /// Resolve `typeof X` type queries to the type of symbol X.
    pub(crate) fn resolve_type_query_type(&mut self, type_id: TypeId) -> TypeId {
        use tsz_binder::SymbolId;
        use tsz_binder::symbol_flags;
        use tsz_solver::SymbolRef;
        match query::classify_type_query(self.ctx.types, type_id) {
            query::TypeQueryKind::TypeQuery(SymbolRef(sym_id)) => {
                let is_cycle = { self.ctx.typeof_resolution_stack.borrow().contains(&sym_id) };
                if is_cycle {
                    return TypeId::ERROR;
                }
                if let Ok(mut stack) = self.ctx.typeof_resolution_stack.try_borrow_mut() {
                    stack.insert(sym_id);
                }
                let sym = SymbolId(sym_id);
                let value_decl = self.get_cross_file_symbol(sym).map_or_else(
                    || self.ctx.binder.get_symbol(sym).map(|s| s.value_declaration),
                    |s| Some(s.value_declaration),
                );
                let flags = self
                    .get_cross_file_symbol(sym)
                    .map(|s| s.flags)
                    .or_else(|| self.ctx.binder.get_symbol(sym).map(|s| s.flags))
                    .unwrap_or(0);
                let result = if self.is_merged_interface_value_symbol(sym)
                    || ((flags & symbol_flags::CLASS) != 0
                        && value_decl.is_some_and(|decl| !decl.is_none()))
                {
                    self.type_of_value_declaration_for_symbol(
                        sym,
                        value_decl.unwrap_or(NodeIndex::NONE),
                    )
                } else {
                    self.get_type_of_symbol(sym)
                };
                if let Ok(mut stack) = self.ctx.typeof_resolution_stack.try_borrow_mut() {
                    stack.remove(&sym_id);
                }
                result
            }
            query::TypeQueryKind::ApplicationWithTypeQuery {
                base_sym_ref: SymbolRef(sym_id),
                args,
            } => {
                let is_cycle = { self.ctx.typeof_resolution_stack.borrow().contains(&sym_id) };
                if is_cycle {
                    return TypeId::ERROR;
                }
                if let Ok(mut stack) = self.ctx.typeof_resolution_stack.try_borrow_mut() {
                    stack.insert(sym_id);
                }
                let base = self.ctx.create_lazy_type_ref(SymbolId(sym_id));
                if let Ok(mut stack) = self.ctx.typeof_resolution_stack.try_borrow_mut() {
                    stack.remove(&sym_id);
                }
                self.ctx.types.application(base, args)
            }
            query::TypeQueryKind::Application { .. } | query::TypeQueryKind::Other => type_id,
        }
    }
    /// Extract and parse a JSDoc `@type` annotation for the given node.
    pub(crate) fn jsdoc_type_annotation_for_node(&mut self, idx: NodeIndex) -> Option<TypeId> {
        if !self.ctx.should_resolve_jsdoc() {
            return None;
        }
        let sf = self.source_file_data_for_node(idx)?;
        let source_text: String = sf.text.to_string();
        let comments = sf.comments.clone();
        let node = self.ctx.arena.get(idx)?;
        let jsdoc = self.try_jsdoc_with_ancestor_walk(idx, &comments, &source_text)?;
        let type_expr = Self::extract_jsdoc_type_expression(&jsdoc)?;
        let type_expr = type_expr.trim();
        self.validate_jsdoc_generic_constraints_at_node(
            idx,
            node.pos,
            type_expr,
            &jsdoc,
            &comments,
            &source_text,
        );
        self.resolve_jsdoc_type_str(type_expr).or_else(|| {
            self.resolve_jsdoc_typedef_type(type_expr, node.pos, &comments, &source_text)
                .or_else(|| {
                    if let Some(resolved) = self.resolve_jsdoc_import_type_reference(type_expr) {
                        return Some(resolved);
                    }
                    if let Some(sym_id) = self.ctx.binder.file_locals.get(type_expr) {
                        let resolved = self.resolve_jsdoc_symbol_type(sym_id);
                        if resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN {
                            return Some(resolved);
                        }
                    }
                    None
                })
        })
    }
    fn jsdoc_template_constraints(jsdoc: &str) -> Vec<(String, Option<String>)> {
        let mut out = Vec::new();
        for raw_line in jsdoc.lines() {
            let trimmed = raw_line.trim().trim_start_matches('*').trim();
            let Some(rest) = trimmed.strip_prefix("@template") else {
                continue;
            };
            let rest = rest.trim();
            let (constraint, names_str) = if let Some(rest) = rest.strip_prefix('{') {
                if let Some(close_idx) = rest.find('}') {
                    (
                        Some(rest[..close_idx].trim().to_string()),
                        rest[close_idx + 1..].trim(),
                    )
                } else {
                    (None, rest)
                }
            } else {
                (None, rest)
            };
            for token in names_str.split([',', ' ', '\t']) {
                let name = token.trim();
                if name.is_empty() {
                    continue;
                }
                if name
                    .chars()
                    .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
                {
                    out.push((name.to_string(), constraint.clone()));
                }
            }
        }
        out
    }
    fn jsdoc_typedef_template_constraints_before(
        &mut self,
        typedef_name: &str,
        anchor_pos: u32,
        comments: &[tsz_common::comments::CommentRange],
        source_text: &str,
    ) -> Vec<Option<TypeId>> {
        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};
        let mut result = Vec::new();
        for comment in comments {
            if comment.end > anchor_pos || !is_jsdoc_comment(comment, source_text) {
                continue;
            }
            let content = get_jsdoc_content(comment, source_text);
            if !Self::parse_jsdoc_typedefs(&content)
                .iter()
                .any(|(name, _)| name == typedef_name)
            {
                continue;
            }
            result = Self::jsdoc_template_constraints(&content)
                .into_iter()
                .map(|(_, constraint)| {
                    constraint.and_then(|expr| self.resolve_jsdoc_type_str(&expr))
                })
                .collect();
        }
        result
    }
    fn validate_jsdoc_generic_constraints_at_node(
        &mut self,
        idx: NodeIndex,
        anchor_pos: u32,
        type_expr: &str,
        _jsdoc: &str,
        comments: &[tsz_common::comments::CommentRange],
        source_text: &str,
    ) {
        let Some(angle_idx) = Self::find_top_level_char(type_expr, '<') else {
            return;
        };
        if !type_expr.ends_with('>') {
            return;
        }
        let base_name = type_expr[..angle_idx].trim();
        let args_str = &type_expr[angle_idx + 1..type_expr.len() - 1];
        let arg_strs = Self::split_type_args_respecting_nesting(args_str);
        if arg_strs.is_empty() {
            return;
        }
        let symbol_constraints = self
            .ctx
            .binder
            .file_locals
            .get(base_name)
            .or_else(|| {
                self.ctx
                    .binder
                    .get_symbols()
                    .find_all_by_name(base_name)
                    .into_iter()
                    .find(|sym_id| {
                        self.ctx.binder.get_symbol(*sym_id).is_some_and(|symbol| {
                            (symbol.flags
                                & (symbol_flags::TYPE_ALIAS
                                    | symbol_flags::CLASS
                                    | symbol_flags::INTERFACE
                                    | symbol_flags::ENUM))
                                != 0
                        })
                    })
            })
            .map(|sym_id| self.type_reference_symbol_type_with_params(sym_id).1)
            .unwrap_or_default();
        let typedef_constraints = if symbol_constraints.is_empty() {
            self.jsdoc_typedef_template_constraints_before(
                base_name,
                anchor_pos,
                comments,
                source_text,
            )
        } else {
            Vec::new()
        };
        let constraints: Vec<Option<TypeId>> = if !symbol_constraints.is_empty() {
            symbol_constraints
                .into_iter()
                .map(|param| param.constraint)
                .collect()
        } else {
            typedef_constraints
        };
        if constraints.is_empty() {
            return;
        }
        let Some((_, comment_pos)) =
            self.try_jsdoc_with_ancestor_walk_and_pos(idx, comments, source_text)
        else {
            return;
        };
        let raw_comment = &source_text[comment_pos as usize..anchor_pos as usize];
        let Some(type_expr_offset) = raw_comment.find(type_expr) else {
            return;
        };
        let mut arg_search_offset = angle_idx + 1;
        for (arg_str, constraint) in arg_strs.iter().zip(constraints.iter()) {
            let Some(constraint) = constraint else {
                arg_search_offset += arg_str.len() + 1;
                continue;
            };
            let Some(type_arg) = self.resolve_jsdoc_type_str(arg_str.trim()) else {
                arg_search_offset += arg_str.len() + 1;
                continue;
            };
            if self.is_assignable_to(type_arg, *constraint) {
                arg_search_offset += arg_str.len() + 1;
                continue;
            }
            let widened_arg = tsz_solver::widen_literal_type(self.ctx.types, type_arg);
            let message = format_message(
                diagnostic_messages::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT,
                &[
                    &self.format_type_diagnostic(widened_arg),
                    &self.format_type_diagnostic(*constraint),
                ],
            );
            let Some(arg_rel_in_expr) = type_expr[arg_search_offset..].find(arg_str.trim()) else {
                arg_search_offset += arg_str.len() + 1;
                continue;
            };
            let arg_pos =
                comment_pos as usize + type_expr_offset + arg_search_offset + arg_rel_in_expr;
            self.ctx.error(
                arg_pos as u32,
                arg_str.trim().len() as u32,
                message,
                diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT,
            );
            arg_search_offset += arg_str.len() + 1;
        }
    }
    /// Resolve a direct leading JSDoc `@type` annotation (no parent fallback).
    pub(crate) fn jsdoc_type_annotation_for_node_direct(
        &mut self,
        idx: NodeIndex,
    ) -> Option<TypeId> {
        if !self.ctx.should_resolve_jsdoc() {
            return None;
        }
        let sf = self.source_file_data_for_node(idx)?;
        let source_text: String = sf.text.to_string();
        let comments = sf.comments.clone();
        let node = self.ctx.arena.get(idx)?;
        let jsdoc = self.try_leading_jsdoc(&comments, node.pos, &source_text)?;
        let type_expr = Self::extract_jsdoc_type_expression(&jsdoc)?;
        let type_expr = type_expr.trim();
        self.jsdoc_type_from_expression(type_expr).or_else(|| {
            self.resolve_jsdoc_typedef_type(type_expr, node.pos, &comments, &source_text)
                .or_else(|| {
                    if let Some(resolved) = self.resolve_jsdoc_import_type_reference(type_expr) {
                        return Some(resolved);
                    }
                    if let Some(sym_id) = self.ctx.binder.file_locals.get(type_expr) {
                        let resolved = self.resolve_jsdoc_symbol_type(sym_id);
                        if resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN {
                            return Some(resolved);
                        }
                    }
                    None
                })
        })
    }
    /// Extract `@satisfies` annotation and its keyword position.
    pub(crate) fn jsdoc_satisfies_annotation_with_pos(
        &mut self,
        idx: NodeIndex,
    ) -> Option<(TypeId, u32)> {
        if !self.ctx.should_resolve_jsdoc() {
            return None;
        }
        let sf = self.source_file_data_for_node(idx)?;
        let source_text: String = sf.text.to_string();
        let comments = sf.comments.clone();
        let (jsdoc, jsdoc_start) =
            self.try_jsdoc_with_ancestor_walk_and_pos(idx, &comments, &source_text)?;
        let type_expr = Self::extract_jsdoc_satisfies_expression(&jsdoc)?;
        let type_expr = type_expr.trim();
        let raw_comment = source_text.get(jsdoc_start as usize..)?;
        let tag_offset = raw_comment.find("@satisfies")? as u32;
        let keyword_pos = jsdoc_start + tag_offset + 1;
        let resolved = self.resolve_jsdoc_type_str(type_expr)?;
        Some((self.judge_evaluate(resolved), keyword_pos))
    }
    fn extract_jsdoc_satisfies_expression(jsdoc: &str) -> Option<&str> {
        let tag_pos = jsdoc.find("@satisfies")?;
        let rest = &jsdoc[tag_pos + "@satisfies".len()..];
        let open = rest.find('{')?;
        let after_open = &rest[open + 1..];
        let mut depth = 1usize;
        let mut end_idx = None;
        for (i, ch) in after_open.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end_idx = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let end_idx = end_idx?;
        Some(after_open[..end_idx].trim())
    }
    fn parse_jsdoc_import_type(type_expr: &str) -> Option<(String, Option<String>)> {
        let expr = type_expr.trim();
        let rest = expr.strip_prefix("import(")?;
        let mut rest = rest.trim_start();
        let quote = rest.chars().next()?;
        if quote != '"' && quote != '\'' && quote != '`' {
            return None;
        }
        rest = &rest[quote.len_utf8()..];
        let close_quote = rest.find(quote)?;
        let module_specifier = rest[..close_quote].trim().to_string();
        let after_quote = rest[close_quote + quote.len_utf8()..].trim_start();
        let after_quote = after_quote.strip_prefix(')')?.trim_start();
        if after_quote.is_empty() {
            return Some((module_specifier, None));
        }
        let after_dot = after_quote.strip_prefix('.')?;
        let after_dot = after_dot.trim_start();
        let mut end = 0usize;
        for (idx, ch) in after_dot.char_indices() {
            if idx == 0 {
                if !ch.is_ascii_alphabetic() && ch != '_' && ch != '$' {
                    return None;
                }
            } else if !ch.is_ascii_alphanumeric() && ch != '_' && ch != '$' {
                break;
            }
            end = idx + ch.len_utf8();
        }
        if end == 0 {
            return None;
        }
        Some((module_specifier, Some(after_dot[..end].to_string())))
    }

    fn resolve_jsdoc_import_type_reference(&mut self, type_expr: &str) -> Option<TypeId> {
        let (module_specifier, member_name) = Self::parse_jsdoc_import_type(type_expr)?;

        if let Some(member_name) = member_name {
            let sym_id = self.resolve_jsdoc_import_member(&module_specifier, &member_name)?;
            let resolved = self.resolve_jsdoc_symbol_type(sym_id);
            return (resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN).then_some(resolved);
        }

        self.commonjs_module_value_type(&module_specifier, Some(self.ctx.current_file_idx))
            .map(|module_type| {
                self.instance_type_from_constructor_type(module_type)
                    .unwrap_or(module_type)
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
                        let mut shape = ObjectShape {
                            flags: ObjectFlags::empty(),
                            properties: Vec::new(),
                            string_index: None,
                            number_index: None,
                            symbol: None,
                        };
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
                                let k_id = factory.type_param(k_param.clone());
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
                if type_expr.starts_with('(')
                    && type_expr.contains("=>")
                    && let Some(arrow_idx) = type_expr.find("=>")
                {
                    let params_str = type_expr[..arrow_idx].trim();
                    if params_str.starts_with('(') && params_str.ends_with(')') {
                        let params_inner = params_str[1..params_str.len() - 1].trim();
                        let return_type_str = type_expr[arrow_idx + 2..].trim();
                        if let Some(return_type) = self.jsdoc_type_from_expression(return_type_str)
                        {
                            use tsz_solver::{FunctionShape, ParamInfo};
                            let mut params = Vec::new();
                            let mut ok = true;
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
                                        ok = false;
                                        break;
                                    }
                                }
                            }
                            if ok {
                                let shape = FunctionShape {
                                    type_params: Vec::new(),
                                    params,
                                    this_type: None,
                                    return_type,
                                    type_predicate: None,
                                    is_constructor: false,
                                    is_method: false,
                                };
                                return Some(factory.function(shape));
                            }
                        }
                    }
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
                            for sym_id in candidates {
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
    /// Resolve a JSDoc type expression string to a `TypeId`, trying all strategies.
    pub(crate) fn resolve_jsdoc_type_str(&mut self, type_expr: &str) -> Option<TypeId> {
        let type_expr = type_expr.trim();
        if let Some(ty) = self.jsdoc_type_from_expression(type_expr) {
            return Some(ty);
        }
        //    because @param {{ x: T }} already handles nested braces separately,
        //    and adding this to the general parser would change @param behavior.
        if type_expr.starts_with('{')
            && type_expr.ends_with('}')
            && let Some(ty) = self.parse_jsdoc_object_literal_type(type_expr)
        {
            return Some(ty);
        }
        // 3. Try file-local symbols (type aliases, classes, interfaces — includes merged lib types)
        self.resolve_jsdoc_type_name(type_expr)
    }
    /// Resolve a simple type name from the symbol table or @typedef declarations.
    fn resolve_jsdoc_type_name(&mut self, name: &str) -> Option<TypeId> {
        if name.contains('.')
            && let Some(resolved) = self.resolve_jsdoc_qualified_type_name(name)
        {
            return Some(resolved);
        }

        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            let resolved = self.resolve_jsdoc_symbol_type(sym_id);
            if resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN {
                return Some(resolved);
            }
        }
        // Try @typedef resolution from JSDoc comments
        if let Some(sf) = self.ctx.arena.source_files.first() {
            let comments = sf.comments.clone();
            let source_text: String = sf.text.to_string();
            if let Some(ty) =
                self.resolve_jsdoc_typedef_type(name, u32::MAX, &comments, &source_text)
            {
                self.register_jsdoc_typedef_def(name, ty);
                return Some(ty);
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

    fn resolve_jsdoc_symbol_type(&mut self, sym_id: tsz_binder::SymbolId) -> TypeId {
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
            let value_type = self.get_type_of_symbol(sym_id);
            // Return the value type directly, matching TSC's getTypeFromJSDocValueReference
            // which calls getTypeOfSymbol without instance type extraction.
            // Instance type extraction for JS constructor functions is handled by the
            // FUNCTION branch above via synthesize_js_constructor_instance_type.
            if value_type != TypeId::ERROR && value_type != TypeId::UNKNOWN {
                return value_type;
            }
        }

        TypeId::ERROR
    }

    fn resolve_jsdoc_assigned_value_type(&mut self, name: &str) -> Option<TypeId> {
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
            if let Some(resolved) = self.resolve_jsdoc_type_from_value_expression(binary.right) {
                return Some(
                    self.combine_jsdoc_instance_and_prototype_type(resolved, prototype_type),
                );
            }
        }

        prototype_type
    }

    fn resolve_jsdoc_type_from_value_expression(&mut self, expr_idx: NodeIndex) -> Option<TypeId> {
        let expr_idx = self.ctx.arena.skip_parenthesized(expr_idx);

        if let Some(sym_id) = self
            .ctx
            .binder
            .get_node_symbol(expr_idx)
            .or_else(|| self.resolve_identifier_symbol(expr_idx))
            .or_else(|| self.resolve_qualified_symbol(expr_idx))
        {
            let resolved = self.resolve_jsdoc_symbol_type(sym_id);
            if resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN {
                return Some(resolved);
            }
        }

        let node = self.ctx.arena.get(expr_idx)?;
        match node.kind {
            k if k == tsz_parser::parser::syntax_kind_ext::FUNCTION_EXPRESSION
                || k == tsz_parser::parser::syntax_kind_ext::FUNCTION_DECLARATION =>
            {
                let constructor_type = self.get_type_of_node(expr_idx);
                self.synthesize_js_constructor_instance_type(expr_idx, constructor_type, &[])
            }
            k if k == tsz_parser::parser::syntax_kind_ext::CLASS_EXPRESSION
                || k == tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION =>
            {
                let sym_id = self.ctx.binder.get_node_symbol(expr_idx)?;
                let resolved = self.type_reference_symbol_type(sym_id);
                (resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN).then_some(resolved)
            }
            _ => None,
        }
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
    /// Register a DefId for a JSDoc `@typedef` so the type formatter can find the alias name.
    fn register_jsdoc_typedef_def(&mut self, name: &str, body_type: TypeId) {
        use tsz_solver::def::DefinitionInfo;
        // Avoid duplicate registration if called multiple times for the same typedef
        if self
            .ctx
            .definition_store
            .find_type_alias_by_body(body_type)
            .is_some()
        {
            return;
        }
        let atom_name = self.ctx.types.intern_string(name);
        let info = DefinitionInfo::type_alias(atom_name, Vec::new(), body_type);
        self.ctx.definition_store.register(info);
    }
    /// Resolve a generic JSDoc type reference: `Name<Arg1, Arg2, ...>`.
    fn resolve_jsdoc_generic_type(
        &mut self,
        base_name: &str,
        type_args: Vec<TypeId>,
    ) -> Option<TypeId> {
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
                .into_iter()
                .find(|sym_id| {
                    self.ctx.binder.get_symbol(*sym_id).is_some_and(|symbol| {
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
    /// Find the first occurrence of a character at the top level.
    fn find_top_level_char(s: &str, target: char) -> Option<usize> {
        let mut angle_depth = 0u32;
        let mut paren_depth = 0u32;
        let mut brace_depth = 0u32;
        let mut square_depth = 0u32;
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        for (i, ch) in s.char_indices() {
            if ch == '\'' && !in_double_quote {
                in_single_quote = !in_single_quote;
                continue;
            }
            if ch == '"' && !in_single_quote {
                in_double_quote = !in_double_quote;
                continue;
            }
            if in_single_quote || in_double_quote {
                continue;
            }
            if ch == target
                && angle_depth == 0
                && paren_depth == 0
                && brace_depth == 0
                && square_depth == 0
            {
                return Some(i);
            }
            match ch {
                '<' => angle_depth += 1,
                '>' if angle_depth > 0 => angle_depth -= 1,
                '(' => paren_depth += 1,
                ')' if paren_depth > 0 => paren_depth -= 1,
                '{' => brace_depth += 1,
                '}' if brace_depth > 0 => brace_depth -= 1,
                '[' => square_depth += 1,
                ']' if square_depth > 0 => square_depth -= 1,
                _ => {}
            }
        }
        None
    }
    /// Split a type expression on a top-level binary operator (`|` or `&`).
    fn split_top_level_binary(s: &str, op: char) -> Option<Vec<&str>> {
        let mut parts = Vec::new();
        let mut start = 0;
        let mut angle_depth = 0u32;
        let mut paren_depth = 0u32;
        let mut brace_depth = 0u32;
        let mut square_depth = 0u32;
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        for (i, ch) in s.char_indices() {
            match ch {
                '\'' if !in_double_quote => in_single_quote = !in_single_quote,
                '"' if !in_single_quote => in_double_quote = !in_double_quote,
                _ if in_single_quote || in_double_quote => continue,
                '<' => angle_depth += 1,
                '>' if angle_depth > 0 => angle_depth -= 1,
                '(' => paren_depth += 1,
                ')' if paren_depth > 0 => paren_depth -= 1,
                '{' => brace_depth += 1,
                '}' if brace_depth > 0 => brace_depth -= 1,
                '[' => square_depth += 1,
                ']' if square_depth > 0 => square_depth -= 1,
                c if c == op
                    && angle_depth == 0
                    && paren_depth == 0
                    && brace_depth == 0
                    && square_depth == 0 =>
                {
                    parts.push(&s[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
        if parts.is_empty() {
            return None; // no split found — not a binary type
        }
        parts.push(&s[start..]);
        Some(parts)
    }
    /// Split a comma-separated list of type arguments, respecting `<>`, `()`, `{}` nesting.
    fn split_type_args_respecting_nesting(s: &str) -> Vec<&str> {
        let mut parts = Vec::new();
        let mut start = 0;
        let mut angle_depth = 0u32;
        let mut paren_depth = 0u32;
        let mut brace_depth = 0u32;
        let mut square_depth = 0u32;
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        for (i, ch) in s.char_indices() {
            match ch {
                '\'' if !in_double_quote => in_single_quote = !in_single_quote,
                '"' if !in_single_quote => in_double_quote = !in_double_quote,
                _ if in_single_quote || in_double_quote => continue,
                '<' => angle_depth += 1,
                '>' if angle_depth > 0 => angle_depth -= 1,
                '(' => paren_depth += 1,
                ')' if paren_depth > 0 => paren_depth -= 1,
                '{' => brace_depth += 1,
                '}' if brace_depth > 0 => brace_depth -= 1,
                '[' => square_depth += 1,
                ']' if square_depth > 0 => square_depth -= 1,
                ',' if angle_depth == 0
                    && paren_depth == 0
                    && brace_depth == 0
                    && square_depth == 0 =>
                {
                    parts.push(&s[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
        if start < s.len() {
            parts.push(&s[start..]);
        }
        parts
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
    /// Split parameter list by commas at the top level.
    fn split_top_level_params(s: &str) -> Vec<&str> {
        let mut parts = Vec::new();
        let mut start = 0;
        let mut angle_depth = 0u32;
        let mut paren_depth = 0u32;
        let mut brace_depth = 0u32;
        let mut square_depth = 0u32;
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        for (i, ch) in s.char_indices() {
            match ch {
                '\'' if !in_double_quote => in_single_quote = !in_single_quote,
                '"' if !in_single_quote => in_double_quote = !in_double_quote,
                _ if in_single_quote || in_double_quote => continue,
                '<' => angle_depth += 1,
                '>' if angle_depth > 0 => angle_depth -= 1,
                '(' => paren_depth += 1,
                ')' if paren_depth > 0 => paren_depth -= 1,
                '{' => brace_depth += 1,
                '}' if brace_depth > 0 => brace_depth -= 1,
                '[' => square_depth += 1,
                ']' if square_depth > 0 => square_depth -= 1,
                ',' if angle_depth == 0
                    && paren_depth == 0
                    && brace_depth == 0
                    && square_depth == 0 =>
                {
                    parts.push(&s[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
        if start < s.len() {
            parts.push(&s[start..]);
        }
        parts
    }
    /// Split object literal properties by ',' or ';' at the top level.
    fn split_object_properties(s: &str) -> Vec<&str> {
        let mut parts = Vec::new();
        let mut start = 0;
        let mut angle_depth = 0u32;
        let mut paren_depth = 0u32;
        let mut brace_depth = 0u32;
        let mut square_depth = 0u32;
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        for (i, ch) in s.char_indices() {
            match ch {
                '\'' if !in_double_quote => in_single_quote = !in_single_quote,
                '"' if !in_single_quote => in_double_quote = !in_double_quote,
                _ if in_single_quote || in_double_quote => continue,
                '<' => angle_depth += 1,
                '>' if angle_depth > 0 => angle_depth -= 1,
                '(' => paren_depth += 1,
                ')' if paren_depth > 0 => paren_depth -= 1,
                '{' => brace_depth += 1,
                '}' if brace_depth > 0 => brace_depth -= 1,
                '[' => square_depth += 1,
                ']' if square_depth > 0 => square_depth -= 1,
                ',' | ';'
                    if angle_depth == 0
                        && paren_depth == 0
                        && brace_depth == 0
                        && square_depth == 0 =>
                {
                    parts.push(&s[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
        if start < s.len() {
            parts.push(&s[start..]);
        }
        parts
    }
    /// Resolve a `@typedef` referenced by name from preceding JSDoc comments.
    pub(crate) fn resolve_jsdoc_typedef_type(
        &mut self,
        type_expr: &str,
        anchor_pos: u32,
        comments: &[tsz_common::comments::CommentRange],
        source_text: &str,
    ) -> Option<TypeId> {
        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};
        let mut best_def: Option<JsdocTypedefInfo> = None;
        for comment in comments {
            if comment.end > anchor_pos {
                continue;
            }
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
        // If the typedef's base type couldn't be resolved, return `any` as fallback.
        // TS2304 is emitted eagerly by `check_jsdoc_typedef_base_types()` during the
        // post-checking phase, so we don't emit it here to avoid duplicates.
        let result = self
            .type_from_jsdoc_typedef(typedef_info)
            .or(Some(TypeId::ANY));
        if let Some(ty) = result {
            self.register_jsdoc_typedef_def(type_expr, ty);
        }
        result
    }
    fn type_from_jsdoc_typedef(&mut self, info: JsdocTypedefInfo) -> Option<TypeId> {
        // Handle @callback definitions — build a function type.
        if let Some(cb) = info.callback {
            let mut params = Vec::with_capacity(cb.params.len());
            for (name, type_expr) in &cb.params {
                let is_rest = type_expr.starts_with("...");
                let effective_expr = if is_rest {
                    &type_expr[3..]
                } else {
                    type_expr.as_str()
                };
                let base_type = self
                    .jsdoc_type_from_expression(effective_expr)
                    .unwrap_or(TypeId::ANY);
                let type_id = if is_rest {
                    let factory = self.ctx.types.factory();
                    factory.array(base_type)
                } else {
                    base_type
                };
                let name_atom = self.ctx.types.intern_string(name);
                params.push(ParamInfo {
                    name: Some(name_atom),
                    type_id,
                    optional: false,
                    rest: is_rest,
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
                    cb.params.iter().position(|(n, _)| n == &param_name)
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
            let shape = FunctionShape {
                type_params: Vec::new(),
                params,
                this_type: None,
                return_type,
                type_predicate,
                is_constructor: false,
                is_method: false,
            };
            let factory = self.ctx.types.factory();
            return Some(factory.function(shape));
        }
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
        // Group properties: dotted names like "icons.image32" become nested object
        // properties on the parent property "icons".
        // First pass: collect nested properties by parent name.
        let mut top_level: Vec<(String, String)> = Vec::new();
        let mut nested: std::collections::BTreeMap<String, Vec<(String, String)>> =
            std::collections::BTreeMap::new();
        for (name, prop_type_expr) in info.properties {
            if let Some(dot_pos) = name.find('.') {
                let parent = name[..dot_pos].to_string();
                let child = name[dot_pos + 1..].to_string();
                nested
                    .entry(parent)
                    .or_default()
                    .push((child, prop_type_expr));
            } else {
                top_level.push((name, prop_type_expr));
            }
        }
        let mut prop_infos = Vec::with_capacity(top_level.len());
        for (name, prop_type_expr) in top_level {
            let mut prop_type = if prop_type_expr.trim().is_empty() {
                TypeId::ANY
            } else {
                self.jsdoc_type_from_expression(&prop_type_expr)
                    .unwrap_or(TypeId::ANY)
            };
            if let Some(children) = nested.remove(&name) {
                let mut child_props = Vec::with_capacity(children.len());
                for (child_name, child_type_expr) in children {
                    let child_type = if child_type_expr.trim().is_empty() {
                        TypeId::ANY
                    } else {
                        self.jsdoc_type_from_expression(&child_type_expr)
                            .unwrap_or(TypeId::ANY)
                    };
                    let child_atom = self.ctx.types.intern_string(&child_name);
                    child_props.push(PropertyInfo {
                        name: child_atom,
                        type_id: child_type,
                        write_type: child_type,
                        optional: false,
                        readonly: false,
                        is_method: false,
                        is_class_prototype: false,
                        visibility: Visibility::Public,
                        parent_id: None,
                        declaration_order: 0,
                    });
                }
                let factory = self.ctx.types.factory();
                prop_type = factory.object(child_props);
            }
            let name_atom = self.ctx.types.intern_string(&name);
            prop_infos.push(PropertyInfo {
                name: name_atom,
                type_id: prop_type,
                write_type: prop_type,
                optional: false,
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
    pub(crate) fn parse_jsdoc_typedefs(jsdoc: &str) -> Vec<(String, JsdocTypedefInfo)> {
        let mut typedefs = Vec::new();
        let mut current_name: Option<String> = None;
        let mut current_info = JsdocTypedefInfo {
            base_type: None,
            properties: Vec::new(),
            callback: None,
        };
        for raw_line in jsdoc.lines() {
            let line = raw_line.trim_start_matches('*').trim();
            if line.is_empty() || !line.starts_with('@') {
                continue;
            }
            if let Some(rest) = line.strip_prefix("@import") {
                for (local_name, specifier, import_name) in Self::parse_jsdoc_import_tag(rest) {
                    let import_type = if import_name == "*" || import_name == "default" {
                        format!("import(\"{specifier}\")")
                    } else {
                        format!("import(\"{specifier}\").{import_name}")
                    };
                    if let Some(previous_name) = current_name.take() {
                        typedefs.push((previous_name, current_info));
                        current_info = JsdocTypedefInfo {
                            base_type: None,
                            properties: Vec::new(),
                            callback: None,
                        };
                    }
                    typedefs.push((
                        local_name,
                        JsdocTypedefInfo {
                            base_type: Some(import_type),
                            properties: Vec::new(),
                            callback: None,
                        },
                    ));
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("@typedef") {
                if let Some((name, base_type)) = Self::parse_jsdoc_typedef_definition(rest) {
                    if let Some(previous_name) = current_name.take() {
                        typedefs.push((previous_name, current_info));
                        current_info = JsdocTypedefInfo {
                            base_type: None,
                            properties: Vec::new(),
                            callback: None,
                        };
                    }
                    current_name = Some(name);
                    current_info.base_type = base_type;
                    current_info.properties.clear();
                    current_info.callback = None;
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("@callback") {
                let name = rest.trim().to_string();
                if !name.is_empty()
                    && name
                        .chars()
                        .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
                {
                    if let Some(previous_name) = current_name.take() {
                        typedefs.push((previous_name, current_info));
                    }
                    current_name = Some(name);
                    current_info = JsdocTypedefInfo {
                        base_type: None,
                        properties: Vec::new(),
                        callback: Some(JsdocCallbackInfo {
                            params: Vec::new(),
                            return_type: None,
                            predicate: None,
                        }),
                    };
                }
                continue;
            }
            if current_info.callback.is_some() {
                if let Some(rest) = line.strip_prefix("@param") {
                    let rest = rest.trim();
                    if rest.starts_with('{')
                        && let Some(end) = rest[1..].find('}')
                    {
                        let type_expr = rest[1..1 + end].trim().to_string();
                        let after = rest[2 + end..].trim();
                        let name = after.split_whitespace().next().unwrap_or("").to_string();
                        if !name.is_empty()
                            && let Some(ref mut cb) = current_info.callback
                        {
                            cb.params.push((name, type_expr));
                        }
                    }
                    continue;
                }
                if let Some(rest) = line
                    .strip_prefix("@returns")
                    .or_else(|| line.strip_prefix("@return"))
                {
                    let rest = rest.trim();
                    if rest.starts_with('{')
                        && let Some(end) = rest[1..].find('}')
                    {
                        let type_expr = rest[1..1 + end].trim();
                        let predicate =
                            Self::jsdoc_returns_type_predicate_from_type_expr(type_expr);
                        if let Some(ref mut cb) = current_info.callback {
                            cb.return_type = Some(type_expr.to_string());
                            cb.predicate = predicate;
                        }
                    }
                    continue;
                }
            }
            if let Some((name, prop_type)) = Self::parse_jsdoc_property_type(line)
                && current_name.is_some()
            {
                current_info.properties.push((name, prop_type));
            }
        }
        if let Some(previous_name) = current_name.take() {
            typedefs.push((previous_name, current_info));
        }
        typedefs
    }
    /// Parse a type predicate from a JSDoc type expression (`x is T`, `asserts x is T`).
    fn jsdoc_returns_type_predicate_from_type_expr(
        type_expr: &str,
    ) -> Option<(bool, String, Option<String>)> {
        let (is_asserts, remainder) = if let Some(after) = type_expr.strip_prefix("asserts ") {
            (true, after.trim())
        } else {
            (false, type_expr)
        };
        if let Some(is_pos) = remainder.find(" is ") {
            let param_name = remainder[..is_pos].trim();
            let type_str = remainder[is_pos + 4..].trim();
            if !param_name.is_empty()
                && (param_name == "this"
                    || param_name
                        .chars()
                        .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()))
                && !type_str.is_empty()
            {
                return Some((
                    is_asserts,
                    param_name.to_string(),
                    Some(type_str.to_string()),
                ));
            }
        } else if is_asserts {
            let param_name = remainder;
            if !param_name.is_empty()
                && (param_name == "this"
                    || param_name
                        .chars()
                        .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()))
            {
                return Some((true, param_name.to_string(), None));
            }
        }
        None
    }
    fn parse_jsdoc_import_tag(rest: &str) -> Vec<(String, String, String)> {
        let rest = rest.trim();
        let mut results = Vec::new();
        if let Some(from_idx) = rest.rfind("from") {
            let before_from = rest[..from_idx].trim();
            let after_from = rest[from_idx + 4..].trim();
            let quote = after_from.chars().next().unwrap_or(' ');
            if quote == '"' || quote == '\'' || quote == '`' {
                let specifier = after_from[1..]
                    .split(quote)
                    .next()
                    .unwrap_or("")
                    .to_string();
                if before_from.starts_with('{') && before_from.ends_with('}') {
                    let inner = &before_from[1..before_from.len() - 1];
                    for part in inner.split(',') {
                        let part = part.trim();
                        if part.is_empty() {
                            continue;
                        }
                        let parts: Vec<&str> = part.split(" as ").collect();
                        if parts.len() == 2 {
                            results.push((
                                parts[1].trim().to_string(),
                                specifier.clone(),
                                parts[0].trim().to_string(),
                            ));
                        } else {
                            results.push((part.to_string(), specifier.clone(), part.to_string()));
                        }
                    }
                } else if let Some(ns_name) = before_from.strip_prefix("* as ") {
                    let ns_name = ns_name.trim().to_string();
                    if !ns_name.is_empty() {
                        results.push((ns_name, specifier, "*".to_string()));
                    }
                } else {
                    let default_name = before_from.to_string();
                    if !default_name.is_empty() {
                        results.push((default_name, specifier, "default".to_string()));
                    }
                }
            }
        }
        results
    }
    fn parse_jsdoc_typedef_definition(line: &str) -> Option<(String, Option<String>)> {
        let mut rest = line.trim();
        if rest.is_empty() {
            return None;
        }
        let base_type = if rest.starts_with('{') {
            let (expr, after_expr) = Self::parse_jsdoc_curly_type_expr(rest)?;
            rest = after_expr.trim();
            Some(expr.trim().to_string())
        } else {
            None
        };
        let name = rest.split_whitespace().next()?;
        Some((name.to_string(), base_type))
    }
    fn parse_jsdoc_property_type(line: &str) -> Option<(String, String)> {
        let mut rest = line.trim();
        if !rest.starts_with("@property") {
            return None;
        }
        rest = &rest["@property".len()..];
        rest = rest.trim();
        let prop_type = if rest.starts_with('{') {
            let (expr, after_expr) = Self::parse_jsdoc_curly_type_expr(rest)?;
            rest = after_expr.trim();
            expr.trim().to_string()
        } else {
            "any".to_string()
        };
        let name = rest
            .split_whitespace()
            .next()
            .map(|name| {
                name.trim_end_matches(',')
                    .trim()
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .split('=')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            })
            .filter(|name| !name.is_empty())?;
        Some((name, prop_type))
    }
    pub(crate) fn parse_jsdoc_curly_type_expr(line: &str) -> Option<(&str, &str)> {
        if !line.starts_with('{') {
            return None;
        }
        let mut depth = 0usize;
        for (idx, ch) in line.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some((&line[1..idx], &line[idx + 1..]));
                    }
                }
                _ => {}
            }
        }
        None
    }
    fn extract_jsdoc_type_expression(jsdoc: &str) -> Option<&str> {
        let typedef_pos = jsdoc.find("@typedef");
        let mut tag_pos = jsdoc.find("@type");
        while let Some(pos) = tag_pos {
            let next_char = jsdoc[pos + "@type".len()..].chars().next();
            if !next_char.is_some_and(|c| c.is_alphabetic()) {
                if let Some(td_pos) = typedef_pos
                    && td_pos < pos
                {
                    let typedef_rest = &jsdoc[td_pos + "@typedef".len()..pos];
                    let mut has_non_object_base = false;
                    if let Some(open) = typedef_rest.find('{')
                        && let Some(close) = typedef_rest[open..].find('}')
                    {
                        let base = typedef_rest[open + 1..open + close].trim();
                        if base != "Object" && base != "object" && !base.is_empty() {
                            has_non_object_base = true;
                        }
                    }
                    if !has_non_object_base {
                        return None; // The @type is absorbed by the @typedef
                    }
                }
                break;
            }
            tag_pos = jsdoc[pos + 1..].find("@type").map(|p| p + pos + 1);
        }
        let tag_pos = tag_pos?;
        let rest = &jsdoc[tag_pos + "@type".len()..];
        // Try braced form first: @type {expression}
        if let Some(open) = rest.find('{') {
            let after_open = &rest[open + 1..];
            let mut depth = 1usize;
            let mut end_idx = None;
            for (i, ch) in after_open.char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end_idx = Some(i);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if let Some(end_idx) = end_idx {
                return Some(after_open[..end_idx].trim());
            }
        }
        // Braceless form: @type expression (rest of line after whitespace)
        // Used in tsc for inline types like `@type () => string` or
        // `@type ({ type: 'foo' } | { type: 'bar' }) & { prop: number }`.
        let rest = rest.trim_start();
        if rest.is_empty() || rest.starts_with('@') || rest.starts_with('*') {
            return None;
        }
        // Take the rest of the line (up to end-of-line, closing comment, or next @tag)
        let end = rest
            .find('\n')
            .or_else(|| rest.find("*/"))
            .unwrap_or(rest.len());
        let expr = rest[..end].trim().trim_end_matches('*').trim();
        if expr.is_empty() { None } else { Some(expr) }
    }
    /// Check if a node has a JSDoc `@readonly` tag.
    pub(crate) fn jsdoc_has_readonly_tag(&self, idx: NodeIndex) -> bool {
        let Some(sf) = self.ctx.arena.source_files.first() else {
            return false;
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let Some(jsdoc) = self.try_leading_jsdoc(
            comments,
            self.ctx.arena.get(idx).map_or(0, |n| n.pos),
            source_text,
        ) else {
            return false;
        };
        Self::jsdoc_contains_tag(&jsdoc, "readonly")
    }
    /// Get the access level from JSDoc `@private` / `@protected` / `@public` tags.
    pub(crate) fn jsdoc_access_level(
        &self,
        idx: NodeIndex,
    ) -> Option<crate::state::MemberAccessLevel> {
        let sf = self.ctx.arena.source_files.first()?;
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let jsdoc = self.try_leading_jsdoc(
            comments,
            self.ctx.arena.get(idx).map_or(0, |n| n.pos),
            source_text,
        )?;
        if Self::jsdoc_contains_tag(&jsdoc, "private") {
            Some(crate::state::MemberAccessLevel::Private)
        } else if Self::jsdoc_contains_tag(&jsdoc, "protected") {
            Some(crate::state::MemberAccessLevel::Protected)
        } else {
            None
        }
    }
    /// Check if a JSDoc comment string contains a specific `@tag`.
    fn jsdoc_contains_tag(jsdoc: &str, tag_name: &str) -> bool {
        let needle = format!("@{tag_name}");
        for pos_match in jsdoc.match_indices(&needle) {
            let after = pos_match.0 + needle.len();
            if after >= jsdoc.len() {
                return true;
            }
            let next_ch = jsdoc[after..].chars().next().unwrap();
            if !next_ch.is_ascii_alphanumeric() {
                return true;
            }
        }
        false
    }
    /// Scan statements for `@extends`/`@augments` not on class declarations (TS8022).
    pub(crate) fn find_orphaned_extends_tags_for_statements(
        &self,
        statements: &[NodeIndex],
    ) -> Vec<(&'static str, u32, u32)> {
        use tsz_parser::parser::syntax_kind_ext;
        let Some(sf) = self.ctx.arena.source_files.first() else {
            return Vec::new();
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let mut results = Vec::new();
        let mut handled_comment_positions = Vec::new();
        // Phase 1: Check each top-level statement's leading JSDoc
        for &stmt_idx in statements {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::CLASS_DECLARATION
                || node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                if let Some(jsdoc) = self.try_leading_jsdoc(comments, node.pos, source_text)
                    && (Self::jsdoc_contains_tag(&jsdoc, "augments")
                        || Self::jsdoc_contains_tag(&jsdoc, "extends"))
                {
                    handled_comment_positions.push(node.pos);
                }
                continue;
            }
            let Some(jsdoc) = self.try_leading_jsdoc(comments, node.pos, source_text) else {
                continue;
            };
            let tag = if Self::jsdoc_contains_tag(&jsdoc, "augments") {
                "augments"
            } else if Self::jsdoc_contains_tag(&jsdoc, "extends") {
                "extends"
            } else {
                continue;
            };
            handled_comment_positions.push(node.pos);
            let (pos, len) = if node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
                if let Some(func) = self.ctx.arena.get_function(node)
                    && let Some(name_node) = self.ctx.arena.get(func.name)
                {
                    (name_node.pos, name_node.end - name_node.pos)
                } else {
                    (node.pos, node.end - node.pos)
                }
            } else {
                (node.pos, node.end - node.pos)
            };
            results.push((tag, pos, len));
        }
        // Phase 2: Check for dangling JSDoc comments not attached to any statement
        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};
        for comment in comments {
            if !is_jsdoc_comment(comment, source_text) {
                continue;
            }
            if handled_comment_positions
                .iter()
                .any(|&stmt_pos| comment.end <= stmt_pos)
            {
                continue;
            }
            let content = get_jsdoc_content(comment, source_text);
            let tag = if Self::jsdoc_contains_tag(&content, "augments") {
                "augments"
            } else if Self::jsdoc_contains_tag(&content, "extends") {
                "extends"
            } else {
                continue;
            };
            let is_leading_of_any_stmt = statements.iter().any(|&stmt_idx| {
                if let Some(n) = self.ctx.arena.get(stmt_idx)
                    && let Some((_, leading_pos)) =
                        self.try_leading_jsdoc_with_pos(comments, n.pos, source_text)
                {
                    return leading_pos == comment.pos;
                }
                false
            });
            if is_leading_of_any_stmt {
                continue;
            }
            let needle = format!("@{tag}");
            let (pos, len) = if let Some(offset) = source_text
                .get(comment.pos as usize..comment.end as usize)
                .and_then(|s| s.find(&needle))
            {
                let tag_pos = comment.pos + offset as u32;
                (tag_pos, needle.len() as u32)
            } else {
                (comment.pos, comment.end - comment.pos)
            };
            results.push((tag, pos, len));
        }
        results
    }
}
