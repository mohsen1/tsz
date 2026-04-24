//! JSDoc @extends/@augments/@implements helpers and heritage clause utilities.

use super::super::class_checker::format_property_name_for_diagnostic;
use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
use crate::query_boundaries::class::{
    should_report_member_type_mismatch, should_report_member_type_mismatch_bivariant,
};
use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

/// A JSDoc template parameter: `(name, has_default, constraint_expr)`.
type JsDocTemplateParam = (String, bool, Option<String>);

impl<'a> CheckerState<'a> {
    pub(crate) fn check_jsdoc_extends_tag_type_arguments(&mut self, class_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        let Some((_tag, type_expr, type_pos)) =
            self.attached_jsdoc_extends_or_augments_tag(class_idx)
        else {
            return;
        };

        let type_expr = type_expr.trim();
        if type_expr.is_empty() {
            return;
        }

        let (base_name, arg_count) = if let Some(angle_idx) = type_expr.find('<') {
            if !type_expr.ends_with('>') {
                return;
            }
            let base_name = type_expr[..angle_idx].trim();
            if base_name.is_empty() {
                return;
            }
            let arg_count =
                Self::split_jsdoc_type_arguments(&type_expr[angle_idx + 1..type_expr.len() - 1])
                    .len();
            (base_name.to_string(), arg_count)
        } else {
            (type_expr.to_string(), 0)
        };

        let type_params = self.type_params_for_jsdoc_extends_name(&base_name);
        if type_params.is_empty() {
            return;
        }

        let max_expected = type_params.len();
        let min_required = type_params
            .iter()
            .filter(|param| param.default.is_none())
            .count();
        if arg_count >= min_required && arg_count <= max_expected {
            return;
        }

        let display_name = Self::format_generic_display_name_with_interner(
            &base_name,
            &type_params,
            self.ctx.types,
        );
        let (message, code) = if min_required < max_expected {
            (
                format_message(
                    diagnostic_messages::GENERIC_TYPE_REQUIRES_BETWEEN_AND_TYPE_ARGUMENTS,
                    &[
                        &display_name,
                        &min_required.to_string(),
                        &max_expected.to_string(),
                    ],
                ),
                diagnostic_codes::GENERIC_TYPE_REQUIRES_BETWEEN_AND_TYPE_ARGUMENTS,
            )
        } else {
            (
                format_message(
                    diagnostic_messages::GENERIC_TYPE_REQUIRES_TYPE_ARGUMENT_S,
                    &[&display_name, &max_expected.to_string()],
                ),
                diagnostic_codes::GENERIC_TYPE_REQUIRES_TYPE_ARGUMENT_S,
            )
        };

        self.ctx
            .error(type_pos, base_name.len() as u32, message, code);
    }

    /// Validate JSDoc `@extends` type arguments against their type-parameter
    /// constraints. Emits TS2344 when a supplied argument does not satisfy
    /// the constraint declared on the target's `@template {Constraint} Name`.
    pub(crate) fn check_jsdoc_extends_tag_type_argument_constraints(
        &mut self,
        class_idx: NodeIndex,
    ) {
        let Some((_tag, type_expr, type_pos)) =
            self.attached_jsdoc_extends_or_augments_tag(class_idx)
        else {
            return;
        };
        let type_expr = type_expr.trim();
        let Some(angle_idx) = type_expr.find('<') else {
            return;
        };
        if !type_expr.ends_with('>') {
            return;
        }
        let base_name = type_expr[..angle_idx].trim().to_string();
        if base_name.is_empty() {
            return;
        }

        let inner = &type_expr[angle_idx + 1..type_expr.len() - 1];
        let inner_base_offset = (angle_idx + 1) as u32;
        let args: Vec<(String, u32)> = Self::split_jsdoc_type_arguments_with_offsets(inner)
            .into_iter()
            .map(|(s, o)| (s.to_string(), o))
            .collect();
        if args.is_empty() {
            return;
        }

        let Some(params) = self.resolve_jsdoc_extends_target_template_params(&base_name) else {
            return;
        };
        if params.is_empty() {
            return;
        }
        let max_expected = params.len();
        let min_required = params
            .iter()
            .filter(|(_, has_default, _)| !*has_default)
            .count();
        if args.len() < min_required || args.len() > max_expected {
            return;
        }

        for ((_name, _has_default, constraint_expr), (arg_raw, arg_rel_offset)) in
            params.iter().zip(args.iter())
        {
            let Some(constraint_expr) = constraint_expr else {
                continue;
            };
            let constraint_opt = self
                .jsdoc_type_from_expression(constraint_expr)
                .or_else(|| self.resolve_jsdoc_type_str(constraint_expr));
            let Some(constraint) = constraint_opt else {
                continue;
            };
            let cleaned = Self::normalize_jsdoc_type_fragment(arg_raw);
            if cleaned.is_empty() {
                continue;
            }
            let Some(arg_type) = self.resolve_jsdoc_type_str(&cleaned) else {
                continue;
            };

            let evaluated_arg = self.evaluate_type_for_assignability(arg_type);
            let evaluated_constraint = self.evaluate_type_for_assignability(constraint);
            if !self.jsdoc_extends_object_violates_constraint(evaluated_arg, evaluated_constraint) {
                continue;
            }

            let arg_display = self.format_type_diagnostic(arg_type);
            let constraint_display = self.format_type_diagnostic(constraint);
            let message = format!(
                "Type '{arg_display}' does not satisfy the constraint '{constraint_display}'."
            );
            let arg_source_pos = type_pos + inner_base_offset + *arg_rel_offset;
            let length = arg_raw.len() as u32;
            self.ctx.error(
                arg_source_pos,
                length,
                message,
                diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT,
            );
        }
    }

    /// Return `true` if `arg_ty` fails the object-shape constraint `constraint_ty`.
    /// Compares each required constraint property against the argument's
    /// matching property. Missing required properties and incompatible
    /// property types both count as violations.
    fn jsdoc_extends_object_violates_constraint(
        &mut self,
        arg_ty: TypeId,
        constraint_ty: TypeId,
    ) -> bool {
        let arg_shape =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, arg_ty);
        let constraint_shape =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, constraint_ty);
        let (Some(arg_shape), Some(constraint_shape)) = (arg_shape, constraint_shape) else {
            return false;
        };
        let arg_props: rustc_hash::FxHashMap<_, _> = arg_shape
            .properties
            .iter()
            .map(|p| (p.name, p))
            .collect();
        for constraint_prop in &constraint_shape.properties {
            let Some(arg_prop) = arg_props.get(&constraint_prop.name) else {
                if !constraint_prop.optional {
                    return true;
                }
                continue;
            };
            let arg_eval = self.evaluate_type_for_assignability(arg_prop.type_id);
            let constraint_eval = self.evaluate_type_for_assignability(constraint_prop.type_id);
            if !self.is_assignable_to(arg_eval, constraint_eval) {
                return true;
            }
        }
        false
    }

    /// Split a JSDoc type argument list (the text between `<` and `>`) at
    /// top-level commas, returning each fragment with its byte offset in the
    /// input so the emitter can anchor diagnostics at the original source
    /// position.
    fn split_jsdoc_type_arguments_with_offsets(type_args: &str) -> Vec<(&str, u32)> {
        let mut parts = Vec::new();
        let mut start = 0usize;
        let mut angle_depth = 0usize;
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut brace_depth = 0usize;

        for (idx, ch) in type_args.char_indices() {
            match ch {
                '<' => angle_depth += 1,
                '>' => angle_depth = angle_depth.saturating_sub(1),
                '(' => paren_depth += 1,
                ')' => paren_depth = paren_depth.saturating_sub(1),
                '[' => bracket_depth += 1,
                ']' => bracket_depth = bracket_depth.saturating_sub(1),
                '{' => brace_depth += 1,
                '}' => brace_depth = brace_depth.saturating_sub(1),
                ',' if angle_depth == 0
                    && paren_depth == 0
                    && bracket_depth == 0
                    && brace_depth == 0 =>
                {
                    let part = &type_args[start..idx];
                    if !part.trim().is_empty() {
                        parts.push((part, start as u32));
                    }
                    start = idx + ch.len_utf8();
                }
                _ => {}
            }
        }

        let tail = &type_args[start..];
        if !tail.trim().is_empty() {
            parts.push((tail, start as u32));
        }

        parts
    }

    /// Normalize a raw JSDoc type fragment by stripping `\n *` line
    /// continuations and collapsing surrounding whitespace. Input like
    /// `{\n *     a: string,\n *     b: string\n * }` becomes
    /// `{ a: string, b: string }`, parseable as a single object-literal type.
    fn normalize_jsdoc_type_fragment(raw: &str) -> String {
        let mut out = String::with_capacity(raw.len());
        let mut last_was_space = false;
        let mut at_line_start = false;
        for ch in raw.chars() {
            if ch == '\n' || ch == '\r' {
                at_line_start = true;
                if !last_was_space && !out.is_empty() {
                    out.push(' ');
                    last_was_space = true;
                }
                continue;
            }
            if at_line_start && ch.is_whitespace() {
                continue;
            }
            if at_line_start && ch == '*' {
                at_line_start = false;
                continue;
            }
            at_line_start = false;
            if ch.is_whitespace() {
                if !last_was_space && !out.is_empty() {
                    out.push(' ');
                    last_was_space = true;
                }
                continue;
            }
            out.push(ch);
            last_was_space = false;
        }
        out.trim().to_string()
    }

    /// For a class/interface referenced from a JSDoc `@extends` tag, return
    /// its type parameters as `(name, has_default, constraint_expr)` tuples.
    /// `constraint_expr` is the textual JSDoc constraint from
    /// `@template {Constraint} Name` on the target's declaration or `None`
    /// when unconstrained or declared without a JSDoc constraint.
    fn resolve_jsdoc_extends_target_template_params(
        &mut self,
        base_name: &str,
    ) -> Option<Vec<(String, bool, Option<String>)>> {
        use tsz_binder::symbol_flags;

        let sym_id = self.ctx.binder.file_locals.get(base_name).or_else(|| {
            self.ctx
                .binder
                .get_symbols()
                .find_all_by_name(base_name)
                .iter()
                .copied()
                .find(|&candidate| {
                    let mut visited_aliases = AliasCycleTracker::new();
                    let resolved = self
                        .resolve_alias_symbol(candidate, &mut visited_aliases)
                        .unwrap_or(candidate);
                    self.ctx.binder.get_symbol(resolved).is_some_and(|symbol| {
                        (symbol.flags
                            & (symbol_flags::TYPE_ALIAS
                                | symbol_flags::CLASS
                                | symbol_flags::INTERFACE
                                | symbol_flags::ENUM))
                            != 0
                    })
                })
        })?;

        let decl_idx = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .and_then(|symbol| symbol.declarations.first().copied())?;

        let sf = self.ctx.arena.source_files.first()?;
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let node = self.ctx.arena.get(decl_idx)?;
        let jsdoc = self.try_leading_jsdoc(comments, node.pos, source_text)?;
        let parsed = Self::parse_jsdoc_template_params_with_constraints(&jsdoc);
        if parsed.is_empty() {
            None
        } else {
            Some(parsed)
        }
    }

    /// Parse `@template [{Constraint}] Name[,Name…]` lines from a JSDoc
    /// comment. Supports the `{Constraint}` prefix with balanced-brace
    /// matching so object-literal constraints (`{Foo: {...}}`) are captured
    /// intact. Names sharing a line share the constraint.
    fn parse_jsdoc_template_params_with_constraints(
        jsdoc: &str,
    ) -> Vec<(String, bool, Option<String>)> {
        let mut out: Vec<(String, bool, Option<String>)> = Vec::new();
        for line in jsdoc.lines() {
            let trimmed = line.trim().trim_start_matches('*').trim();
            let Some(rest) = trimmed.strip_prefix("@template") else {
                continue;
            };
            let mut rest = rest.trim_start();

            let constraint = if rest.starts_with('{') {
                let body = &rest[1..];
                let mut depth = 1usize;
                let mut close = None;
                for (idx, ch) in body.char_indices() {
                    match ch {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                close = Some(idx);
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                let Some(close) = close else { continue };
                let expr = body[..close].trim().to_string();
                rest = body[close + 1..].trim_start();
                if expr.is_empty() { None } else { Some(expr) }
            } else {
                None
            };

            for token in rest.split([',', ' ', '\t']) {
                let name = token.trim();
                if name.is_empty() || name == "const" {
                    continue;
                }
                if !name
                    .chars()
                    .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
                {
                    break;
                }
                if out.iter().any(|(existing, _, _)| existing == name) {
                    continue;
                }
                out.push((name.to_string(), false, constraint.clone()));
            }
        }
        out
    }

    pub(crate) fn check_missing_jsdoc_extends_type_arguments(
        &mut self,
        class_idx: NodeIndex,
        class_data: &tsz_parser::parser::node::ClassData,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_binder::symbol_flags;

        let Some((_type_idx, expr_idx, has_type_args)) =
            self.first_extends_clause_expression_info(class_data)
        else {
            return;
        };
        if has_type_args {
            return;
        }
        if self
            .attached_jsdoc_extends_or_augments_tag(class_idx)
            .is_some()
        {
            return;
        }

        let Some(heritage_sym) = self.resolve_heritage_symbol(expr_idx) else {
            return;
        };
        let name = self
            .heritage_name_text(expr_idx)
            .unwrap_or_else(|| "<expression>".to_string());
        if (self.ctx.has_lib_loaded() && self.ctx.symbol_is_from_lib(heritage_sym))
            || self.is_well_known_lib_type_name(&name)
            || self
                .get_cross_file_symbol(heritage_sym)
                .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::VARIABLE))
        {
            return;
        }

        let type_params = self.type_params_for_heritage_symbol(heritage_sym);
        if type_params.is_empty() {
            return;
        }

        let max_expected = type_params.len();
        let min_required = type_params
            .iter()
            .filter(|param| param.default.is_none())
            .count();
        if min_required == 0 {
            return;
        }

        let (message, code) = if min_required < max_expected {
            (
                format_message(
                    diagnostic_messages::EXPECTED_TYPE_ARGUMENTS_PROVIDE_THESE_WITH_AN_EXTENDS_TAG_2,
                    &[&min_required.to_string(), &max_expected.to_string()],
                ),
                diagnostic_codes::EXPECTED_TYPE_ARGUMENTS_PROVIDE_THESE_WITH_AN_EXTENDS_TAG_2,
            )
        } else {
            let display_name = Self::format_generic_display_name_with_interner(
                &name,
                &type_params,
                self.ctx.types,
            );
            (
                format_message(
                    diagnostic_messages::EXPECTED_TYPE_ARGUMENTS_PROVIDE_THESE_WITH_AN_EXTENDS_TAG,
                    &[&display_name],
                ),
                diagnostic_codes::EXPECTED_TYPE_ARGUMENTS_PROVIDE_THESE_WITH_AN_EXTENDS_TAG,
            )
        };

        self.error_at_node(expr_idx, &message, code);
    }

    pub(crate) fn attached_jsdoc_extends_or_augments_tag(
        &self,
        class_idx: NodeIndex,
    ) -> Option<(&'static str, String, u32)> {
        let sf = self.ctx.arena.source_files.first()?;
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let node = self.ctx.arena.get(class_idx)?;

        use tsz_common::comments::{get_leading_comments_from_cache, is_jsdoc_comment};
        let leading = get_leading_comments_from_cache(comments, node.pos, source_text);
        let comment = leading.last()?;
        if !is_jsdoc_comment(comment, source_text) {
            return None;
        }

        let comment_text = comment.get_text(source_text);
        for tag in ["augments", "extends"] {
            let needle = format!("@{tag}");
            for (match_pos, _) in comment_text.match_indices(&needle) {
                let after = match_pos + needle.len();
                if after >= comment_text.len() {
                    continue;
                }
                let next_ch = comment_text[after..]
                    .chars()
                    .next()
                    .expect("after < len checked above");
                if next_ch.is_ascii_alphanumeric() {
                    continue;
                }
                let rest = comment_text[after..].trim_start();
                if rest.is_empty() {
                    continue;
                }

                let rest_offset = rest.as_ptr() as usize - comment_text.as_ptr() as usize;
                if rest.starts_with('{') {
                    // Walk brace-balanced so nested `{...}` inside the
                    // annotation (e.g. `@extends {A<{x:number}>}`) are kept
                    // intact. The previous `rest.find('}')` truncated at the
                    // inner closing `}` and silently dropped the remainder.
                    let inner = &rest[1..];
                    let mut depth = 1usize;
                    let mut close = None;
                    for (idx, ch) in inner.char_indices() {
                        match ch {
                            '{' => depth += 1,
                            '}' => {
                                depth -= 1;
                                if depth == 0 {
                                    close = Some(idx);
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    let close = close?;
                    let raw = &inner[..close];
                    let type_expr = raw.trim();
                    if type_expr.is_empty() {
                        continue;
                    }
                    let start_in_raw = raw.find(type_expr).unwrap_or(0);
                    let type_offset = rest_offset + 1 + start_in_raw;
                    return Some((tag, type_expr.to_string(), comment.pos + type_offset as u32));
                }

                let mut end = rest.len();
                let mut angle_depth = 0usize;
                let mut paren_depth = 0usize;
                let mut bracket_depth = 0usize;
                let mut brace_depth = 0usize;
                for (idx, ch) in rest.char_indices() {
                    match ch {
                        '<' => angle_depth += 1,
                        '>' => angle_depth = angle_depth.saturating_sub(1),
                        '(' => paren_depth += 1,
                        ')' => paren_depth = paren_depth.saturating_sub(1),
                        '[' => bracket_depth += 1,
                        ']' => bracket_depth = bracket_depth.saturating_sub(1),
                        '{' => brace_depth += 1,
                        '}' => brace_depth = brace_depth.saturating_sub(1),
                        '*' if angle_depth == 0
                            && paren_depth == 0
                            && bracket_depth == 0
                            && brace_depth == 0 =>
                        {
                            end = idx;
                            break;
                        }
                        c if c.is_whitespace()
                            && angle_depth == 0
                            && paren_depth == 0
                            && bracket_depth == 0
                            && brace_depth == 0 =>
                        {
                            end = idx;
                            break;
                        }
                        _ => {}
                    }
                }
                let raw = &rest[..end];
                let type_expr = raw.trim();
                if type_expr.is_empty() {
                    continue;
                }
                let start_in_raw = raw.find(type_expr).unwrap_or(0);
                let type_offset = rest_offset + start_in_raw;
                return Some((tag, type_expr.to_string(), comment.pos + type_offset as u32));
            }
        }

        None
    }

    /// Returns `true` when the class has a JSDoc `@augments`/`@extends` tag
    /// whose type argument is empty (e.g. `/** @augments */`).  tsc treats
    /// such a tag as an invalid override of the structural `extends` clause,
    /// which prevents base-class property merging.
    pub(crate) fn has_empty_jsdoc_augments_tag(&self, class_idx: NodeIndex) -> bool {
        let sf = match self.ctx.arena.source_files.first() {
            Some(sf) => sf,
            None => return false,
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let node = match self.ctx.arena.get(class_idx) {
            Some(n) => n,
            None => return false,
        };

        use tsz_common::comments::{get_leading_comments_from_cache, is_jsdoc_comment};
        let leading = get_leading_comments_from_cache(comments, node.pos, source_text);
        let comment = match leading.last() {
            Some(c) => c,
            None => return false,
        };
        if !is_jsdoc_comment(comment, source_text) {
            return false;
        }

        let comment_text = comment.get_text(source_text);
        for tag in ["augments", "extends"] {
            let needle = format!("@{tag}");
            for (match_pos, _) in comment_text.match_indices(&needle) {
                let after = match_pos + needle.len();
                if after >= comment_text.len() {
                    return true;
                }
                let next_ch = comment_text[after..]
                    .chars()
                    .next()
                    .expect("after < len checked above");
                if next_ch.is_ascii_alphanumeric() {
                    continue;
                }
                if self
                    .attached_jsdoc_extends_or_augments_tag(class_idx)
                    .is_none()
                {
                    return true;
                }
                return false;
            }
        }
        false
    }

    fn type_params_for_jsdoc_extends_name(
        &mut self,
        base_name: &str,
    ) -> Vec<tsz_solver::TypeParamInfo> {
        use tsz_binder::symbol_flags;

        if let Some((_, type_params)) = self.resolve_global_jsdoc_typedef_info(base_name) {
            return type_params;
        }

        let Some(sym_id) = self.ctx.binder.file_locals.get(base_name).or_else(|| {
            self.ctx
                .binder
                .get_symbols()
                .find_all_by_name(base_name)
                .iter()
                .copied()
                .find(|&candidate| {
                    let mut visited_aliases = AliasCycleTracker::new();
                    let resolved = self
                        .resolve_alias_symbol(candidate, &mut visited_aliases)
                        .unwrap_or(candidate);
                    self.ctx.binder.get_symbol(resolved).is_some_and(|symbol| {
                        (symbol.flags
                            & (symbol_flags::TYPE_ALIAS
                                | symbol_flags::CLASS
                                | symbol_flags::INTERFACE
                                | symbol_flags::ENUM))
                            != 0
                    })
                })
        }) else {
            return Vec::new();
        };

        self.type_params_for_heritage_symbol(sym_id)
    }

    fn type_params_for_heritage_symbol(
        &mut self,
        sym_id: tsz_binder::SymbolId,
    ) -> Vec<tsz_solver::TypeParamInfo> {
        let mut type_params = self.get_type_params_for_symbol(sym_id);
        if type_params.is_empty() {
            let mut visited_aliases = AliasCycleTracker::new();
            if let Some(resolved) = self.resolve_alias_symbol(sym_id, &mut visited_aliases) {
                type_params = self.get_type_params_for_symbol(resolved);
            }
        }
        type_params
    }

    fn split_jsdoc_type_arguments(type_args: &str) -> Vec<&str> {
        let mut parts = Vec::new();
        let mut start = 0;
        let mut angle_depth = 0usize;
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut brace_depth = 0usize;

        for (idx, ch) in type_args.char_indices() {
            match ch {
                '<' => angle_depth += 1,
                '>' => angle_depth = angle_depth.saturating_sub(1),
                '(' => paren_depth += 1,
                ')' => paren_depth = paren_depth.saturating_sub(1),
                '[' => bracket_depth += 1,
                ']' => bracket_depth = bracket_depth.saturating_sub(1),
                '{' => brace_depth += 1,
                '}' => brace_depth = brace_depth.saturating_sub(1),
                ',' if angle_depth == 0
                    && paren_depth == 0
                    && bracket_depth == 0
                    && brace_depth == 0 =>
                {
                    let part = type_args[start..idx].trim();
                    if !part.is_empty() {
                        parts.push(part);
                    }
                    start = idx + ch.len_utf8();
                }
                _ => {}
            }
        }

        let tail = type_args[start..].trim();
        if !tail.is_empty() {
            parts.push(tail);
        }

        parts
    }

    fn first_extends_clause_expression_info(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> Option<(NodeIndex, NodeIndex, bool)> {
        use tsz_scanner::SyntaxKind;

        let heritage = class_data.heritage_clauses.as_ref()?;
        for &clause_idx in &heritage.nodes {
            let clause_node = self.ctx.arena.get(clause_idx)?;
            if clause_node.kind != syntax_kind_ext::HERITAGE_CLAUSE {
                continue;
            }
            let clause = self.ctx.arena.get_heritage_clause(clause_node)?;
            if clause.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            let type_idx = *clause.types.nodes.first()?;
            let type_node = self.ctx.arena.get(type_idx)?;
            if type_node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS
                && let Some(ewta) = self.ctx.arena.get_expr_type_args(type_node)
            {
                let has_type_args = ewta
                    .type_arguments
                    .as_ref()
                    .is_some_and(|args| !args.nodes.is_empty());
                return Some((type_idx, ewta.expression, has_type_args));
            }

            let has_type_args = self
                .ctx
                .arena
                .get_call_expr(type_node)
                .and_then(|call| call.type_arguments.as_ref())
                .is_some_and(|args| !args.nodes.is_empty());
            return Some((type_idx, type_idx, has_type_args));
        }

        None
    }

    /// Get the base class name from the `extends` clause of a class declaration.
    pub(crate) fn get_extends_clause_name(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let heritage = class_data.heritage_clauses.as_ref()?;
        for &clause_idx in &heritage.nodes {
            let clause_node = self.ctx.arena.get(clause_idx)?;
            if clause_node.kind != syntax_kind_ext::HERITAGE_CLAUSE {
                continue;
            }
            let clause = self.ctx.arena.get_heritage_clause(clause_node)?;
            // Check if this is an extends clause (not implements)
            if clause.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            // Get the first type in the extends clause
            let first_type_idx = clause.types.nodes.first()?;
            let type_node = self.ctx.arena.get(*first_type_idx)?;
            // ExpressionWithTypeArguments — get the expression part
            if type_node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS
                && let Some(ewta) = self.ctx.arena.get_expr_type_args(type_node)
            {
                return self.get_leftmost_identifier_name(ewta.expression);
            }
            // Direct identifier
            return self.get_leftmost_identifier_name(*first_type_idx);
        }
        None
    }

    // ============================================================================
    // JSDoc @implements checking
    // ============================================================================

    /// Extract type names from `@implements` JSDoc tags on a class declaration.
    /// Supports both `@implements {TypeName}` and `@implements TypeName` syntax.
    /// Returns a list of type name strings plus positions for empty tags that should emit TS1003.
    fn extract_jsdoc_implements_names(&self, class_idx: NodeIndex) -> (Vec<String>, Vec<u32>) {
        let Some(sf) = self.ctx.arena.source_files.first() else {
            return (Vec::new(), Vec::new());
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;

        let jsdoc_anchor_idx = self
            .ctx
            .arena
            .get_extended(class_idx)
            .map(|ext| ext.parent)
            .filter(|parent| {
                self.ctx
                    .arena
                    .get(*parent)
                    .is_some_and(|node| node.kind == syntax_kind_ext::EXPORT_DECLARATION)
            })
            .unwrap_or(class_idx);

        let Some(effective_pos) =
            self.effective_jsdoc_pos_for_node(jsdoc_anchor_idx, comments, source_text)
        else {
            return (Vec::new(), Vec::new());
        };

        let Some((jsdoc, jsdoc_start)) =
            self.try_leading_jsdoc_with_pos(comments, effective_pos, source_text)
        else {
            return (Vec::new(), Vec::new());
        };
        let leading = tsz_common::comments::get_leading_comments_from_cache(
            comments,
            effective_pos,
            source_text,
        );
        let raw_comment = leading
            .last()
            .and_then(|comment| source_text.get(comment.pos as usize..comment.end as usize))
            .unwrap_or("");

        let mut names = Vec::new();
        let mut missing_positions = Vec::new();
        let needle = "@implements";
        let raw_offsets: Vec<usize> = raw_comment
            .match_indices(needle)
            .filter_map(|(pos, _)| {
                let after = pos + needle.len();
                if after < raw_comment.len()
                    && raw_comment[after..]
                        .chars()
                        .next()
                        .is_some_and(|ch| ch.is_ascii_alphanumeric())
                {
                    None
                } else {
                    Some(pos)
                }
            })
            .collect();

        let mut tag_index = 0usize;
        for (pos, _) in jsdoc.match_indices(needle) {
            let after = pos + needle.len();
            if after < jsdoc.len()
                && jsdoc[after..]
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_alphanumeric())
            {
                continue;
            }
            let raw_pos = raw_offsets.get(tag_index).copied();
            tag_index += 1;

            // Skip whitespace after @implements
            let rest = jsdoc.get(after..).unwrap_or("").trim_start();
            if rest.is_empty() {
                if let Some(raw_pos) = raw_pos {
                    missing_positions.push(jsdoc_start + raw_pos as u32 + needle.len() as u32);
                }
                continue;
            }

            // Extract type name — either `{TypeName}` or `TypeName`
            let type_name = if rest.starts_with('{') {
                // Find matching }
                if let Some(close) = rest.find('}') {
                    rest[1..close].trim()
                } else {
                    continue;
                }
            } else {
                // Take until whitespace or end of line
                let end = rest
                    .find(|c: char| c.is_whitespace() || c == '*')
                    .unwrap_or(rest.len());
                rest[..end].trim()
            };

            if !type_name.is_empty() {
                names.push(type_name.to_string());
            }
        }
        (names, missing_positions)
    }

    /// Check JSDoc `@implements` tags on a class declaration (JS files only).
    /// This is the JSDoc equivalent of syntactic `implements` clauses.
    /// Reports TS2420 (missing interface members), TS2416 (incompatible member types),
    /// and TS2720 (implementing a class instead of extending).
    pub(crate) fn check_jsdoc_implements_clauses(
        &mut self,
        class_idx: NodeIndex,
        class_data: &tsz_parser::parser::node::ClassData,
    ) {
        // Only check in JS files
        if !self.ctx.is_js_file() {
            return;
        }

        // Abstract classes don't need to implement interface members
        if self.has_abstract_modifier(&class_data.modifiers) {
            return;
        }

        let (implements_names, missing_positions) = self.extract_jsdoc_implements_names(class_idx);
        for pos in missing_positions {
            let already_emitted = self
                .ctx
                .diagnostics
                .iter()
                .any(|d| d.code == diagnostic_codes::IDENTIFIER_EXPECTED && d.start == pos);
            if !already_emitted {
                self.emit_error_at(
                    pos,
                    0,
                    diagnostic_messages::IDENTIFIER_EXPECTED,
                    diagnostic_codes::IDENTIFIER_EXPECTED,
                );
            }
        }
        if implements_names.is_empty() {
            return;
        }

        // Get class name for error messages
        let class_name = if class_data.name.is_some() {
            if let Some(name_node) = self.ctx.arena.get(class_data.name) {
                if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                    ident.escaped_text.clone()
                } else {
                    String::from("<anonymous>")
                }
            } else {
                String::from("<anonymous>")
            }
        } else {
            String::from("<anonymous>")
        };
        let class_error_idx = if class_data.name.is_some() {
            class_data.name
        } else {
            class_idx
        };

        // Get the class instance type — this includes JS constructor this-properties
        let class_instance_type = self.get_class_instance_type(class_idx, class_data);

        // Collect class member names from instance type shape for existence checks
        let mut class_member_names: rustc_hash::FxHashSet<String> =
            rustc_hash::FxHashSet::default();
        let mut class_member_type_map: rustc_hash::FxHashMap<String, TypeId> =
            rustc_hash::FxHashMap::default();
        if let Some(shape) = crate::query_boundaries::common::object_shape_for_type(
            self.ctx.types,
            class_instance_type,
        ) {
            for prop in &shape.properties {
                let name = self.ctx.types.resolve_atom(prop.name);
                class_member_names.insert(name.clone());
                class_member_type_map.insert(name, prop.type_id);
            }
        }

        for target_name in &implements_names {
            // Resolve the target symbol - first try flat lookup, then qualified name resolution
            let sym_id = if let Some(sym) = self.ctx.binder.file_locals.get(target_name) {
                Some(sym)
            } else if target_name.contains('.') {
                // Try to resolve as a qualified name (e.g., "NS.I" from @import * as NS)
                self.resolve_jsdoc_entity_name_symbol(target_name)
            } else {
                None
            };

            let Some(sym_id) = sym_id else {
                continue;
            };
            let lib_binders = self.get_lib_binders();
            let Some((symbol_flags, symbol_declarations, target_display_name)) = self
                .get_cross_file_symbol(sym_id)
                .or_else(|| self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders))
                .map(|symbol| {
                    (
                        symbol.flags,
                        symbol.declarations.clone(),
                        symbol.escaped_name.clone(),
                    )
                })
            else {
                continue;
            };

            let is_class = (symbol_flags & tsz_binder::symbol_flags::CLASS) != 0;

            // Check for private/protected members (TS2720 — should extend, not implement)
            let mut has_private_members = false;
            if is_class {
                for &decl_idx in &symbol_declarations {
                    if let Some(node) = self.ctx.arena.get(decl_idx)
                        && node.kind == syntax_kind_ext::CLASS_DECLARATION
                        && let Some(base_class_data) = self.ctx.arena.get_class(node)
                        && self.class_has_private_or_protected_members(base_class_data)
                    {
                        has_private_members = true;
                    }
                }
            }

            if has_private_members {
                let message = format!(
                    "Class '{class_name}' incorrectly implements class '{target_display_name}'. Did you mean to extend '{target_display_name}' and inherit its members as a subclass?"
                );
                self.error_at_node(
                    class_error_idx,
                    &message,
                    diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_CLASS_DID_YOU_MEAN_TO_EXTEND_AND_INHERIT_ITS_MEMBER,
                );
                continue;
            }

            // Get the interface/class type and check members.
            // For classes, get_type_of_symbol returns the constructor type, so we need
            // to use get_class_instance_type to get the instance shape with members.
            let interface_type = if is_class {
                // Find the class declaration and get its instance type
                let mut instance_type = None;
                for &decl_idx in &symbol_declarations {
                    if let Some(node) = self.ctx.arena.get(decl_idx)
                        && node.kind == syntax_kind_ext::CLASS_DECLARATION
                        && let Some(target_class_data) = self.ctx.arena.get_class(node)
                    {
                        instance_type =
                            Some(self.get_class_instance_type(decl_idx, target_class_data));
                        break;
                    }
                }
                instance_type.unwrap_or(TypeId::ERROR)
            } else {
                let raw_type = self.get_type_of_symbol(sym_id);
                self.evaluate_type_for_assignability(raw_type)
            };

            let mut missing_members: Vec<String> = Vec::new();
            let mut incompatible_members: Vec<(String, String, String)> = Vec::new();
            let mut interface_has_index_signature = false;

            if let Some(shape) = crate::query_boundaries::common::object_shape_for_type(
                self.ctx.types,
                interface_type,
            ) {
                if shape.string_index.is_some() || shape.number_index.is_some() {
                    interface_has_index_signature = true;
                }

                for prop in &shape.properties {
                    let member_name = self.ctx.types.resolve_atom(prop.name);
                    let interface_member_type = prop.type_id;

                    // Skip optional properties
                    if prop.optional {
                        continue;
                    }

                    // Check if class has this member
                    if let Some(&class_member_type) = class_member_type_map.get(&member_name) {
                        // Check type compatibility.
                        // Methods use bivariant relation; properties use regular assignability.
                        let mismatch_fn = if prop.is_method {
                            should_report_member_type_mismatch_bivariant
                        } else {
                            should_report_member_type_mismatch
                        };
                        if interface_member_type != TypeId::ANY
                            && class_member_type != TypeId::ANY
                            && interface_member_type != TypeId::ERROR
                            && class_member_type != TypeId::ERROR
                            && mismatch_fn(
                                self,
                                class_member_type,
                                interface_member_type,
                                class_idx,
                            )
                        {
                            let expected_str = self.format_type(interface_member_type);
                            let actual_str = self.format_type(class_member_type);
                            incompatible_members.push((
                                member_name.clone(),
                                expected_str,
                                actual_str,
                            ));
                        }
                    } else {
                        missing_members.push(member_name);
                    }
                }
            }

            // Check index signatures
            if interface_has_index_signature {
                let class_has_index_signature =
                    class_data.members.nodes.iter().any(|&member_idx| {
                        if let Some(member_node) = self.ctx.arena.get(member_idx) {
                            member_node.kind == syntax_kind_ext::INDEX_SIGNATURE
                        } else {
                            false
                        }
                    });

                if !class_has_index_signature && missing_members.is_empty() {
                    // tsc emits just the top-level message; index signature detail is a sub-diagnostic
                    self.error_at_node(
                        class_error_idx,
                        &format!(
                            "Class '{class_name}' incorrectly implements interface '{target_display_name}'."
                        ),
                        diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,
                    );
                }
            }

            // Report missing members
            let diagnostic_code = if is_class {
                diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_CLASS_DID_YOU_MEAN_TO_EXTEND_AND_INHERIT_ITS_MEMBER
            } else {
                diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE
            };

            if !missing_members.is_empty() {
                let missing_message = if missing_members.len() == 1 {
                    format!(
                        "Property '{}' is missing in type '{}' but required in type '{}'.",
                        missing_members[0], class_name, target_display_name
                    )
                } else {
                    let formatted_list = if missing_members.len() > 4 {
                        let first_four = missing_members
                            .iter()
                            .take(4)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("{}, and {} more", first_four, missing_members.len() - 4)
                    } else {
                        missing_members.join(", ")
                    };
                    format!(
                        "Type '{class_name}' is missing the following properties from type '{target_display_name}': {formatted_list}"
                    )
                };

                let full_message = if is_class {
                    format!(
                        "Class '{class_name}' incorrectly implements class '{target_display_name}'. Did you mean to extend '{target_display_name}' and inherit its members as a subclass?\n  {missing_message}"
                    )
                } else {
                    format!(
                        "Class '{class_name}' incorrectly implements interface '{target_display_name}'.\n  {missing_message}"
                    )
                };

                self.error_at_node(class_error_idx, &full_message, diagnostic_code);
            }

            // Report incompatible member types (TS2416)
            for (member_name, expected, actual) in incompatible_members {
                // For JSDoc @implements, we don't have a specific member node to point to,
                // so use the class name node for the error location.
                // Find the class member node if possible for better error location
                let error_node_idx = class_data
                    .members
                    .nodes
                    .iter()
                    .find_map(|&member_idx| {
                        if let Some(name) = self.get_member_name(member_idx)
                            && name == member_name
                        {
                            if let Some(member_node) = self.ctx.arena.get(member_idx) {
                                self.get_member_name_node(member_node)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .unwrap_or(class_error_idx);

                let display_name = format_property_name_for_diagnostic(&member_name);
                self.error_at_node(
                    error_node_idx,
                    &format!(
                        "Property '{display_name}' in type '{class_name}' is not assignable to the same property in base type '{target_display_name}'."
                    ),
                    diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE,
                );
                self.report_type_not_assignable_detail(
                    error_node_idx,
                    &actual,
                    &expected,
                    diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE,
                );
            }
        }
    }

    /// Check whether a class extends a base class with the same name as the
    /// given implements target. E.g., `class D extends C<string> implements C<number>`
    /// has `C` as both the extends base and the implements target.
    pub(crate) fn class_extends_same_base(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
        implements_name: &str,
    ) -> bool {
        let Some(ref heritage_clauses) = class_data.heritage_clauses else {
            return false;
        };
        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            for &type_idx in &heritage.types.nodes {
                if let Some(name) = self.heritage_name_text(type_idx)
                    && name == implements_name
                {
                    return true;
                }
                // Also check ExpressionWithTypeArguments
                if let Some(type_node) = self.ctx.arena.get(type_idx)
                    && let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node)
                    && let Some(name) = self.heritage_name_text(expr_type_args.expression)
                    && name == implements_name
                {
                    return true;
                }
            }
        }
        false
    }

    // NOTE: check_abstract_members_from_type, find_abstract_members_in_type,
    // collect_class_names_from_instance_type, and is_property_abstract_via_parent
    // are in class_abstract_checker.rs
}
