use crate::state::CheckerState;

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
                    let name = raw
                        .split_once(" extends ")
                        .map_or(raw, |(name, _)| name)
                        .trim();
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
                self.type_reference_symbol_type_with_params(sym_id)
                    .1
                    .is_empty()
            } else {
                matches!(base_name, "Void" | "Undefined")
            };

        if resolved_non_generic {
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
}
