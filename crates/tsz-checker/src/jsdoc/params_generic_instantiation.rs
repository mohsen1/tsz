use crate::state::CheckerState;

/// Find the byte offset of a top-level `=>` arrow in a JSDoc type
/// expression, ignoring arrows nested inside `<>`, `()`, `{}`, `[]` or
/// quoted strings.
fn find_top_level_jsdoc_arrow(expr: &str) -> Option<usize> {
    let mut angle_depth = 0u32;
    let mut paren_depth = 0u32;
    let mut brace_depth = 0u32;
    let mut square_depth = 0u32;
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let bytes = expr.as_bytes();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        let ch = bytes[i] as char;
        let next = bytes[i + 1] as char;
        match ch {
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            _ if in_single_quote || in_double_quote => {
                i += 1;
                continue;
            }
            '<' => angle_depth += 1,
            '>' if angle_depth > 0 => angle_depth -= 1,
            '(' => paren_depth += 1,
            ')' if paren_depth > 0 => paren_depth -= 1,
            '{' => brace_depth += 1,
            '}' if brace_depth > 0 => brace_depth -= 1,
            '[' => square_depth += 1,
            ']' if square_depth > 0 => square_depth -= 1,
            '=' if next == '>'
                && angle_depth == 0
                && paren_depth == 0
                && brace_depth == 0
                && square_depth == 0 =>
            {
                return Some(i);
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Whether a JSDoc type expression is a single (possibly dotted, possibly
/// generic) identifier reference: `Foo`, `Foo.Bar`, `Foo<X>`. Returns
/// false for compound types (function, object literal, union, etc.).
fn is_jsdoc_simple_type_name(expr: &str) -> bool {
    if expr.is_empty() {
        return false;
    }
    let Some(first) = expr.chars().next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != '_' && first != '$' {
        return false;
    }
    let mut angle_depth = 0u32;
    for ch in expr.chars() {
        match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '$' | '.' => {}
            '<' => angle_depth += 1,
            '>' if angle_depth > 0 => angle_depth -= 1,
            ',' | ' ' if angle_depth > 0 => {}
            _ => return false,
        }
    }
    true
}

impl<'a> CheckerState<'a> {
    pub(crate) fn report_jsdoc_param_generic_instantiation_errors(
        &mut self,
        type_expr: &str,
        type_expr_start: u32,
    ) -> bool {
        fn find_top_level_arrow(expr: &str) -> Option<usize> {
            let mut angle_depth = 0u32;
            let mut paren_depth = 0u32;
            let mut brace_depth = 0u32;
            let mut square_depth = 0u32;
            let mut in_single_quote = false;
            let mut in_double_quote = false;
            let bytes = expr.as_bytes();
            let mut i = 0usize;
            while i + 1 < bytes.len() {
                let ch = bytes[i] as char;
                let next = bytes[i + 1] as char;
                match ch {
                    '\'' if !in_double_quote => in_single_quote = !in_single_quote,
                    '"' if !in_single_quote => in_double_quote = !in_double_quote,
                    _ if in_single_quote || in_double_quote => {
                        i += 1;
                        continue;
                    }
                    '<' => angle_depth += 1,
                    '>' if angle_depth > 0 => angle_depth -= 1,
                    '(' => paren_depth += 1,
                    ')' if paren_depth > 0 => paren_depth -= 1,
                    '{' => brace_depth += 1,
                    '}' if brace_depth > 0 => brace_depth -= 1,
                    '[' => square_depth += 1,
                    ']' if square_depth > 0 => square_depth -= 1,
                    '=' if next == '>'
                        && angle_depth == 0
                        && paren_depth == 0
                        && brace_depth == 0
                        && square_depth == 0 =>
                    {
                        return Some(i);
                    }
                    _ => {}
                }
                i += 1;
            }
            None
        }

        let mut reported = false;
        let mut expr = type_expr.trim();
        let mut template_params: Vec<String> = Vec::new();
        let mut expr_offset = 0usize;

        // Parse leading `<T, U, ...>` so nested arg checks know in-scope names.
        if expr.starts_with('<') {
            let mut depth = 0u32;
            let mut close_idx = None;
            for (i, ch) in expr.char_indices() {
                match ch {
                    '<' => depth += 1,
                    '>' => {
                        if depth == 0 {
                            break;
                        }
                        depth -= 1;
                        if depth == 0 {
                            close_idx = Some(i);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if let Some(close_idx) = close_idx {
                let template_slice = &expr[1..close_idx];
                for raw in Self::split_type_args_respecting_nesting(template_slice) {
                    let raw = raw.trim();
                    if raw.is_empty() {
                        continue;
                    }
                    let (name, _) = Self::split_jsdoc_type_param_constraint(raw);
                    if !name.is_empty() {
                        template_params.push(name.to_string());
                    }
                }
                let raw_tail = &expr[close_idx + 1..];
                let ws = raw_tail.len().saturating_sub(raw_tail.trim_start().len());
                let tail = raw_tail.trim_start();
                if tail.starts_with('(') {
                    expr_offset += close_idx + 1 + ws;
                    expr = tail;
                }
            }
        }

        if let Some(arrow_idx) = find_top_level_arrow(expr) {
            let params_str = expr[..arrow_idx].trim();
            if params_str.starts_with('(') && params_str.ends_with(')') {
                let params_inner = &params_str[1..params_str.len() - 1];
                let mut search_offset = 0usize;
                for param in Self::split_top_level_params(params_inner) {
                    let Some(rel) = params_inner[search_offset..].find(param) else {
                        continue;
                    };
                    let param_start = search_offset + rel;
                    search_offset = param_start + param.len();

                    let mut param_text = param.trim();
                    if param_text.is_empty() {
                        continue;
                    }
                    if let Some(stripped) = param_text.strip_prefix("...") {
                        param_text = stripped.trim();
                    }
                    let Some(colon_idx) = Self::find_top_level_char(param_text, ':') else {
                        continue;
                    };
                    let param_type = param_text[colon_idx + 1..].trim();
                    if param_type.is_empty() {
                        continue;
                    }
                    let Some(param_rel) = expr[param_start..].find(param_type) else {
                        continue;
                    };
                    let param_type_offset = param_start + param_rel;
                    reported |= self.report_jsdoc_simple_generic_instantiation_errors(
                        param_type,
                        type_expr_start + expr_offset as u32 + param_type_offset as u32,
                        &template_params,
                    );
                }
            }
            return reported;
        }

        reported
            || self.report_jsdoc_simple_generic_instantiation_errors(
                expr,
                type_expr_start + expr_offset as u32,
                &template_params,
            )
    }

    fn report_jsdoc_simple_generic_instantiation_errors(
        &mut self,
        type_expr: &str,
        type_expr_start: u32,
        template_params: &[String],
    ) -> bool {
        let is_simple_type_name = |expr: &str| -> bool {
            if expr.is_empty() {
                return false;
            }
            let Some(first) = expr.chars().next() else {
                return false;
            };
            if !first.is_ascii_alphabetic() && first != '_' && first != '$' {
                return false;
            }
            let mut angle_depth = 0u32;
            for ch in expr.chars() {
                match ch {
                    'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '$' | '.' => {}
                    '<' => angle_depth += 1,
                    '>' if angle_depth > 0 => angle_depth -= 1,
                    ',' | ' ' if angle_depth > 0 => {}
                    _ => return false,
                }
            }
            true
        };

        let mut reported = false;
        let Some(angle_idx) = Self::find_top_level_char(type_expr, '<') else {
            return false;
        };
        if !type_expr.ends_with('>') {
            return false;
        }

        let base_name = type_expr[..angle_idx].trim();
        let args_str = &type_expr[angle_idx + 1..type_expr.len() - 1];
        let arg_strs = Self::split_type_args_respecting_nesting(args_str);
        if arg_strs.is_empty() {
            return false;
        }

        let resolved_type_symbol = self
            .ctx
            .binder
            .file_locals
            .get(base_name)
            .and_then(|sym_id| {
                self.ctx.binder.get_symbol(sym_id).and_then(|symbol| {
                    ((symbol.flags
                        & (tsz_binder::symbol_flags::TYPE_ALIAS
                            | tsz_binder::symbol_flags::CLASS
                            | tsz_binder::symbol_flags::INTERFACE
                            | tsz_binder::symbol_flags::ENUM))
                        != 0)
                        .then_some(sym_id)
                })
            })
            .or_else(|| {
                self.ctx
                    .binder
                    .get_symbols()
                    .find_all_by_name(base_name)
                    .iter()
                    .copied()
                    .find(|&sym_id| {
                        self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                            (symbol.flags
                                & (tsz_binder::symbol_flags::TYPE_ALIAS
                                    | tsz_binder::symbol_flags::CLASS
                                    | tsz_binder::symbol_flags::INTERFACE
                                    | tsz_binder::symbol_flags::ENUM))
                                != 0
                        })
                    })
            });

        let resolved_non_generic =
            if let Some((_, params)) = self.resolve_global_jsdoc_typedef_info(base_name) {
                params.is_empty()
            } else if let Some(sym_id) = resolved_type_symbol {
                // `type_reference_symbol_type_with_params` only sees AST-level
                // `<T>` lists; for JS classes declared with `@template T` JSDoc
                // (no syntax-level params), use the reference helper which
                // also surfaces JSDoc-derived params.
                self.get_reference_type_params_for_symbol(sym_id, base_name)
                    .is_empty()
            } else {
                matches!(base_name, "Void" | "Undefined")
            };

        // JSDoc treats `Object<K, V>` as a record-shaped indexed type
        // (`{ [k: K]: V }`), even though the lib `interface Object` declaration
        // has no type parameters. tsc accepts this without TS2315 in JS files.
        let is_jsdoc_object_record = base_name == "Object" && arg_strs.len() == 2;
        if resolved_non_generic && !is_jsdoc_object_record {
            let base_offset = type_expr[..angle_idx].rfind(base_name).unwrap_or(0);
            let message = crate::diagnostics::format_message(
                crate::diagnostics::diagnostic_messages::TYPE_IS_NOT_GENERIC,
                &[base_name],
            );
            self.error_at_position(
                type_expr_start + base_offset as u32,
                base_name.len() as u32,
                &message,
                crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_GENERIC,
            );
            reported = true;
        }

        let mut arg_search_offset = angle_idx + 1;
        for arg in &arg_strs {
            let arg_name = arg.trim();
            if arg_name.is_empty()
                || template_params.iter().any(|name| name == arg_name)
                || self.ctx.type_parameter_scope.contains_key(arg_name)
                || !is_simple_type_name(arg_name)
            {
                arg_search_offset += arg.len() + 1;
                continue;
            }
            let Some(arg_rel) = type_expr[arg_search_offset..].find(arg_name) else {
                arg_search_offset += arg.len() + 1;
                continue;
            };
            if self.resolve_jsdoc_type_str(arg_name).is_none() {
                // Suppress TS2304 when `arg_name` matches an `@template`
                // declaration whose AST scope contains the reference site.
                // tsc accepts class/function/typedef-level `@template T` as
                // in-scope for any JSDoc reference within the AST subtree
                // led by the same JSDoc comment, but NOT for an unrelated
                // standalone typedef elsewhere in the file. See
                // `emit_jsdoc_cannot_find_name` for the same guard.
                let arg_pos = type_expr_start + (arg_search_offset + arg_rel) as u32;
                if self.is_js_file()
                    && self.source_file_declares_jsdoc_template_at(arg_name, arg_pos)
                {
                    arg_search_offset += arg.len() + 1;
                    continue;
                }
                let message = crate::diagnostics::format_message(
                    crate::diagnostics::diagnostic_messages::CANNOT_FIND_NAME,
                    &[arg_name],
                );
                self.error_at_position(
                    type_expr_start + (arg_search_offset + arg_rel) as u32,
                    arg_name.len() as u32,
                    &message,
                    crate::diagnostics::diagnostic_codes::CANNOT_FIND_NAME,
                );
                reported = true;
            }
            arg_search_offset += arg.len() + 1;
        }

        reported
    }

    /// Walk a JSDoc type expression and emit TS2304 for any unresolved
    /// simple-name leaves found inside compound type structures
    /// (object-literal property values, arrow function parameter and
    /// return types, parenthesized types, array element types, union
    /// and intersection branches, and the type-arguments of generic
    /// instantiations).
    ///
    /// Top-level simple-name expressions (e.g. `@param {Missing}`) are
    /// not reported here: the existing simple-name path in
    /// `jsdoc/diagnostics.rs` already handles that case. This helper
    /// covers the compound-type cases that path skips.
    ///
    /// `template_params` lists in-scope `@template` names so that
    /// type-parameter references inside the same JSDoc comment are not
    /// flagged. `comment_pos`/`comment_end`/`source_text` are passed
    /// through to `emit_jsdoc_cannot_find_name` for diagnostic
    /// positioning and spelling-suggestion handling.
    pub(crate) fn report_jsdoc_unresolved_inner_type_leaves(
        &mut self,
        type_expr: &str,
        comment_pos: u32,
        comment_end: u32,
        source_text: &str,
        template_params: &[String],
    ) -> bool {
        let trimmed = type_expr.trim();
        if trimmed.is_empty() {
            return false;
        }
        if is_jsdoc_simple_type_name(trimmed) {
            // Top-level simple names are handled by the existing path.
            return false;
        }
        self.walk_jsdoc_type_for_unresolved_leaves(
            trimmed,
            comment_pos,
            comment_end,
            source_text,
            template_params,
        )
    }

    fn walk_jsdoc_type_for_unresolved_leaves(
        &mut self,
        expr: &str,
        comment_pos: u32,
        comment_end: u32,
        source_text: &str,
        template_params: &[String],
    ) -> bool {
        let trimmed = expr.trim();
        if trimmed.is_empty() {
            return false;
        }
        // Strip JSDoc decorators: `!T` (non-null), `?T` (nullable), `T=` (optional).
        let trimmed = trimmed
            .trim_start_matches('!')
            .trim_start_matches('?')
            .trim();
        let trimmed = trimmed.trim_end_matches('=').trim();
        if trimmed.is_empty() {
            return false;
        }

        let (is_asserts, predicate_remainder) = Self::split_jsdoc_asserts_prefix(trimmed);
        let predicate_text = if is_asserts {
            predicate_remainder
        } else {
            trimmed
        };
        if let Some((_is_pos, is_end)) = Self::find_jsdoc_type_predicate_is(predicate_text) {
            let type_part = predicate_text[is_end..].trim();
            return self.walk_jsdoc_type_for_unresolved_leaves(
                type_part,
                comment_pos,
                comment_end,
                source_text,
                template_params,
            );
        }
        if is_asserts {
            return false;
        }

        // Array suffix `T[]`.
        if let Some(stripped) = trimmed.strip_suffix("[]") {
            return self.walk_jsdoc_type_for_unresolved_leaves(
                stripped,
                comment_pos,
                comment_end,
                source_text,
                template_params,
            );
        }

        // Arrow function `(params) => ret`, `() => ret`, or
        // `<T, U>(params) => ret` (generic signature). When a generic
        // signature prefix is present, the names it declares are in scope
        // for the params and the return type, so they must be appended to
        // `template_params` before we recurse — otherwise a body like
        // `<T>(p: T) => T` would emit a spurious TS2304 for `T`.
        if let Some(arrow_idx) = find_top_level_jsdoc_arrow(trimmed) {
            let mut reported = false;
            let params_str = trimmed[..arrow_idx].trim();
            let ret_str = trimmed[arrow_idx + 2..].trim();
            let (signature_template_params, params_str) =
                Self::split_jsdoc_signature_template_params(params_str);
            let active_template_params = if signature_template_params.is_empty() {
                None
            } else {
                let mut merged = template_params.to_vec();
                merged.extend(signature_template_params);
                Some(merged)
            };
            let template_params = active_template_params.as_deref().unwrap_or(template_params);
            if params_str.starts_with('(') && params_str.ends_with(')') {
                let inner = &params_str[1..params_str.len() - 1];
                for param in Self::split_top_level_params(inner) {
                    let param_text = param.trim();
                    if param_text.is_empty() {
                        continue;
                    }
                    let param_text = param_text
                        .strip_prefix("...")
                        .map(str::trim)
                        .unwrap_or(param_text);
                    let type_part =
                        if let Some(colon_idx) = Self::find_top_level_char(param_text, ':') {
                            param_text[colon_idx + 1..].trim()
                        } else {
                            // Bare type position (no `name:` form).
                            param_text
                        };
                    reported |= self.walk_jsdoc_type_for_unresolved_leaves(
                        type_part,
                        comment_pos,
                        comment_end,
                        source_text,
                        template_params,
                    );
                }
            }
            reported |= self.walk_jsdoc_type_for_unresolved_leaves(
                ret_str,
                comment_pos,
                comment_end,
                source_text,
                template_params,
            );
            return reported;
        }

        // Parenthesized type `(T)` (no top-level arrow — already handled above).
        if trimmed.starts_with('(') && trimmed.ends_with(')') {
            let inner = &trimmed[1..trimmed.len() - 1];
            return self.walk_jsdoc_type_for_unresolved_leaves(
                inner,
                comment_pos,
                comment_end,
                source_text,
                template_params,
            );
        }

        // Union `T | U` or intersection `T & U`.
        if let Some(parts) = Self::split_top_level_binary(trimmed, '|')
            .or_else(|| Self::split_top_level_binary(trimmed, '&'))
        {
            let mut reported = false;
            for part in parts {
                reported |= self.walk_jsdoc_type_for_unresolved_leaves(
                    part,
                    comment_pos,
                    comment_end,
                    source_text,
                    template_params,
                );
            }
            return reported;
        }

        // Mapped object type `{ [P in Keys]: Template }`. The template is in a
        // nested scope where the mapped type parameter is valid.
        if let Some((type_param_name, constraint, template)) =
            Self::jsdoc_mapped_type_parts(trimmed)
        {
            let mut reported = false;
            let constraint = constraint.trim();
            let constraint = constraint
                .strip_prefix("keyof ")
                .unwrap_or(constraint)
                .trim();
            let constraint = constraint
                .strip_prefix("typeof ")
                .unwrap_or(constraint)
                .trim();
            reported |= self.walk_jsdoc_type_for_unresolved_leaves(
                constraint,
                comment_pos,
                comment_end,
                source_text,
                template_params,
            );
            let mut nested_template_params = template_params.to_vec();
            nested_template_params.push(type_param_name.to_string());
            reported |= self.walk_jsdoc_type_for_unresolved_leaves(
                template,
                comment_pos,
                comment_end,
                source_text,
                &nested_template_params,
            );
            return reported;
        }

        // Object literal `{ a: T, b: U }`.
        if trimmed.starts_with('{') && trimmed.ends_with('}') {
            let inner = &trimmed[1..trimmed.len() - 1];
            let mut reported = false;
            for prop in Self::split_object_properties(inner) {
                let prop = prop.trim();
                if prop.is_empty() {
                    continue;
                }
                let Some(colon_idx) = Self::find_top_level_char(prop, ':') else {
                    continue;
                };
                let value = prop[colon_idx + 1..].trim();
                reported |= self.walk_jsdoc_type_for_unresolved_leaves(
                    value,
                    comment_pos,
                    comment_end,
                    source_text,
                    template_params,
                );
            }
            return reported;
        }

        // Generic instantiation `Foo<X, Y>`: descend into each type-arg.
        if let Some(angle_idx) = Self::find_top_level_char(trimmed, '<')
            && trimmed.ends_with('>')
        {
            let args = &trimmed[angle_idx + 1..trimmed.len() - 1];
            let mut reported = false;
            for arg in Self::split_type_args_respecting_nesting(args) {
                reported |= self.walk_jsdoc_type_for_unresolved_leaves(
                    arg,
                    comment_pos,
                    comment_end,
                    source_text,
                    template_params,
                );
            }
            return reported;
        }

        // Leaf: simple identifier name.
        if is_jsdoc_simple_type_name(trimmed) {
            if trimmed.contains('.') {
                return false;
            }
            if is_jsdoc_intrinsic_type_name(trimmed) {
                return false;
            }
            // Skip in-scope `@template` parameters.
            if template_params.iter().any(|t| t == trimmed) {
                return false;
            }
            // Built-in primitive type names. The resolver returns
            // `TypeId::UNKNOWN` for the literal `unknown` keyword by design,
            // and the unresolved heuristic below treats `UNKNOWN` as a "could
            // not resolve" sentinel, so this leaf would otherwise emit a
            // spurious TS2304 for valid JSDoc like `@type {(v: unknown) => …}`.
            if matches!(
                trimmed,
                "string"
                    | "String"
                    | "number"
                    | "Number"
                    | "boolean"
                    | "Boolean"
                    | "bigint"
                    | "BigInt"
                    | "any"
                    | "unknown"
                    | "undefined"
                    | "Undefined"
                    | "null"
                    | "Null"
                    | "void"
                    | "Void"
                    | "never"
                    | "symbol"
                    | "Symbol"
                    | "this"
            ) {
                return false;
            }
            let resolved = self.resolve_jsdoc_type_str(trimmed);
            let unresolved = resolved.is_none_or(|ty| {
                ty == tsz_solver::TypeId::ERROR || ty == tsz_solver::TypeId::UNKNOWN
            });
            if unresolved {
                self.emit_jsdoc_cannot_find_name(trimmed, comment_pos, comment_end, source_text);
                return true;
            }
        }
        false
    }

    fn jsdoc_mapped_type_parts(type_expr: &str) -> Option<(&str, &str, &str)> {
        let inner = type_expr.strip_prefix('{')?.strip_suffix('}')?.trim();
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
        let mut after_bracket = inner[close_bracket + 1..].trim();
        if let Some(rest) = after_bracket.strip_prefix("-?") {
            after_bracket = rest.trim();
        } else if let Some(rest) = after_bracket.strip_prefix('?') {
            after_bracket = rest.trim();
        }
        let template = after_bracket.strip_prefix(':')?.trim();
        let in_idx = Self::find_jsdoc_diagnostic_mapped_in_keyword(header)?;
        let type_param_name = header[..in_idx].trim();
        let constraint = header[in_idx + 2..].trim();
        (!type_param_name.is_empty() && !constraint.is_empty() && !template.is_empty()).then_some((
            type_param_name,
            constraint,
            template,
        ))
    }

    fn find_jsdoc_diagnostic_mapped_in_keyword(header: &str) -> Option<usize> {
        for (idx, _) in header.match_indices("in") {
            let before = header[..idx].chars().next_back();
            let after = header[idx + 2..].chars().next();
            if before.is_some_and(char::is_whitespace) && after.is_some_and(char::is_whitespace) {
                return Some(idx);
            }
        }
        None
    }

    fn split_jsdoc_signature_template_params(params_str: &str) -> (Vec<String>, &str) {
        let trimmed = params_str.trim();
        let Some(rest) = trimmed.strip_prefix('<') else {
            return (Vec::new(), params_str);
        };

        let mut depth = 1u32;
        for (rel_idx, ch) in rest.char_indices() {
            match ch {
                '<' => depth += 1,
                '>' => {
                    depth -= 1;
                    if depth == 0 {
                        let end_idx = 1 + rel_idx;
                        let template_slice = &trimmed[1..end_idx];
                        let names = Self::split_type_args_respecting_nesting(template_slice)
                            .into_iter()
                            .filter_map(Self::jsdoc_signature_template_param_name)
                            .collect();
                        return (names, trimmed[end_idx + 1..].trim());
                    }
                }
                _ => {}
            }
        }

        (Vec::new(), params_str)
    }

    fn jsdoc_signature_template_param_name(raw: &str) -> Option<String> {
        let (name, _constraint) = Self::split_jsdoc_type_param_constraint(raw.trim());
        let name = name.split('=').next().unwrap_or(name).trim();
        if !name.is_empty()
            && name
                .chars()
                .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
        {
            Some(name.to_string())
        } else {
            None
        }
    }
}

fn is_jsdoc_intrinsic_type_name(name: &str) -> bool {
    matches!(
        name,
        "any"
            | "bigint"
            | "boolean"
            | "false"
            | "never"
            | "null"
            | "number"
            | "object"
            | "Object"
            | "string"
            | "symbol"
            | "this"
            | "true"
            | "undefined"
            | "unknown"
            | "void"
    )
}
