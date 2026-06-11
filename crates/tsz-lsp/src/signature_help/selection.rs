use super::*;
use tsz_checker::state::CheckerState;

impl<'a> SignatureHelpProvider<'a> {
    pub(super) fn tuple_union_variants(text: &str) -> Vec<String> {
        let parts = Self::split_top_level_text(text, '|');
        let mut out = Vec::with_capacity(parts.len());
        for part in parts {
            let trimmed = part.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                out.push(trimmed.to_string());
            }
        }
        out
    }

    pub(super) fn tuple_variant_parameters(
        tuple_variant: &str,
        base_name: &str,
    ) -> Option<Vec<ParameterInformation>> {
        let inner = tuple_variant
            .trim()
            .strip_prefix('[')?
            .strip_suffix(']')?
            .trim();
        if inner.is_empty() {
            return Some(Vec::new());
        }

        let parts = Self::split_top_level_text(inner, ',');
        let mut params = Vec::with_capacity(parts.len());
        for (idx, raw) in parts.into_iter().enumerate() {
            let raw = raw.trim();
            if raw.is_empty() {
                continue;
            }

            let (name, ty, is_optional, is_rest) =
                if let Some(colon_idx) = Self::find_top_level_char(raw, ':') {
                    let lhs = raw[..colon_idx].trim();
                    let rhs = raw[colon_idx + 1..].trim();
                    let mut name = lhs.trim();
                    let is_rest = name.starts_with("...");
                    if is_rest {
                        name = name.trim_start_matches("...").trim();
                    }
                    let is_optional = name.ends_with('?');
                    if is_optional {
                        name = name.trim_end_matches('?').trim();
                    }
                    let fallback = if is_rest {
                        base_name.to_string()
                    } else {
                        format!("{base_name}_{idx}")
                    };
                    let name = if name.is_empty() {
                        fallback
                    } else {
                        name.to_string()
                    };
                    (name, rhs.to_string(), is_optional, is_rest)
                } else if let Some(rest_ty) = raw.strip_prefix("...") {
                    (
                        base_name.to_string(),
                        rest_ty.trim().to_string(),
                        false,
                        true,
                    )
                } else {
                    (format!("{base_name}_{idx}"), raw.to_string(), false, false)
                };

            if ty.is_empty() {
                continue;
            }
            let optional = if is_optional { "?" } else { "" };
            let rest = if is_rest { "..." } else { "" };
            let label = format!("{rest}{name}{optional}: {ty}");
            params.push(ParameterInformation {
                name,
                label,
                documentation: None,
                is_optional,
                is_rest,
            });
        }

        Some(params)
    }

    pub(super) fn split_top_level_text(text: &str, separator: char) -> Vec<String> {
        let mut out = Vec::new();
        let mut start = 0usize;
        let bytes = text.as_bytes();
        let sep = separator as u8;
        let mut paren = 0i32;
        let mut bracket = 0i32;
        let mut brace = 0i32;
        let mut angle = 0i32;

        for (idx, &byte) in bytes.iter().enumerate() {
            match byte {
                b'(' => paren += 1,
                b')' => paren = paren.saturating_sub(1),
                b'[' => bracket += 1,
                b']' => bracket = bracket.saturating_sub(1),
                b'{' => brace += 1,
                b'}' => brace = brace.saturating_sub(1),
                b'<' => angle += 1,
                b'>' if idx == 0 || bytes[idx - 1] != b'=' => {
                    angle = angle.saturating_sub(1);
                }
                _ => {}
            }
            if byte == sep && paren == 0 && bracket == 0 && brace == 0 && angle == 0 {
                out.push(text[start..idx].trim().to_string());
                start = idx + 1;
            }
        }
        out.push(text[start..].trim().to_string());
        out
    }

    pub(super) fn find_top_level_char(text: &str, needle: char) -> Option<usize> {
        let bytes = text.as_bytes();
        let mut paren = 0i32;
        let mut bracket = 0i32;
        let mut brace = 0i32;
        let mut angle = 0i32;

        for (idx, &byte) in bytes.iter().enumerate() {
            match byte {
                b'(' => paren += 1,
                b')' => paren = paren.saturating_sub(1),
                b'[' => bracket += 1,
                b']' => bracket = bracket.saturating_sub(1),
                b'{' => brace += 1,
                b'}' => brace = brace.saturating_sub(1),
                b'<' => angle += 1,
                b'>' if idx == 0 || bytes[idx - 1] != b'=' => {
                    angle = angle.saturating_sub(1);
                }
                _ => {}
            }
            if byte == needle as u8 && paren == 0 && bracket == 0 && brace == 0 && angle == 0 {
                return Some(idx);
            }
        }

        None
    }

    pub(super) fn source_signature_type_texts(
        &self,
        decl_idx: NodeIndex,
    ) -> Option<(Vec<Option<String>>, Option<String>)> {
        let decl_node = self.arena.get(decl_idx)?;

        if let Some(function) = self.arena.get_function(decl_node) {
            let param_types = function
                .parameters
                .nodes
                .iter()
                .map(|&param_idx| {
                    let param_node = self.arena.get(param_idx)?;
                    let param = self.arena.get_parameter(param_node)?;
                    self.type_node_text(param.type_annotation)
                })
                .collect::<Vec<_>>();
            let return_type = self.type_node_text(function.type_annotation).or_else(|| {
                self.inferred_return_type_text_from_body(
                    function.body,
                    &function.parameters.nodes,
                    &param_types,
                )
            });
            return Some((param_types, return_type));
        }

        if let Some(method) = self.arena.get_method_decl(decl_node) {
            let param_types = method
                .parameters
                .nodes
                .iter()
                .map(|&param_idx| {
                    let param_node = self.arena.get(param_idx)?;
                    let param = self.arena.get_parameter(param_node)?;
                    self.type_node_text(param.type_annotation)
                })
                .collect::<Vec<_>>();
            let return_type = self.type_node_text(method.type_annotation).or_else(|| {
                self.inferred_return_type_text_from_body(
                    method.body,
                    &method.parameters.nodes,
                    &param_types,
                )
            });
            return Some((param_types, return_type));
        }

        None
    }

    pub(super) fn inferred_return_type_text_from_body(
        &self,
        body_idx: NodeIndex,
        parameter_nodes: &[NodeIndex],
        parameter_type_texts: &[Option<String>],
    ) -> Option<String> {
        let body_node = self.arena.get(body_idx)?;
        let block = self.arena.get_block(body_node)?;
        let [statement_idx] = block.statements.nodes.as_slice() else {
            return None;
        };
        let statement_node = self.arena.get(*statement_idx)?;
        let return_stmt = self.arena.get_return_statement(statement_node)?;
        let expr_name = self
            .arena
            .get_identifier_text(return_stmt.expression)?
            .trim();

        parameter_nodes
            .iter()
            .zip(parameter_type_texts.iter())
            .find_map(|(&param_idx, type_text)| {
                let param_node = self.arena.get(param_idx)?;
                let param = self.arena.get_parameter(param_node)?;
                (self.arena.get_identifier_text(param.name)? == expr_name)
                    .then(|| type_text.clone())
                    .flatten()
            })
    }

    pub(super) fn type_node_text(&self, type_idx: NodeIndex) -> Option<String> {
        if !type_idx.is_some() {
            return None;
        }
        let type_node = self.arena.get(type_idx)?;
        let start = type_node.pos as usize;
        let end = type_node.end.min(self.source_text.len() as u32) as usize;
        (start < end).then(|| Self::normalize_source_type_text(self.source_text[start..end].trim()))
    }

    pub(super) fn normalize_source_type_text(text: &str) -> String {
        let mut text = text.trim().to_string();
        while let Some(last) = text.chars().last() {
            let should_trim = match last {
                ',' | ';' | '=' => true,
                '(' => Self::has_unmatched_trailing_opener(&text, '(', ')'),
                '[' => Self::has_unmatched_trailing_opener(&text, '[', ']'),
                '{' => Self::has_unmatched_trailing_opener(&text, '{', '}'),
                '<' => Self::has_unmatched_trailing_opener(&text, '<', '>'),
                ')' => Self::has_unmatched_trailing_closer(&text, '(', ')'),
                ']' => Self::has_unmatched_trailing_closer(&text, '[', ']'),
                '}' => Self::has_unmatched_trailing_closer(&text, '{', '}'),
                '>' => Self::has_unmatched_trailing_closer(&text, '<', '>'),
                _ => false,
            };
            if !should_trim {
                break;
            }
            text.pop();
            text = text.trim_end().to_string();
        }

        text
    }

    pub(super) fn has_unmatched_trailing_closer(text: &str, open: char, close: char) -> bool {
        text.chars().filter(|&ch| ch == close).count()
            > text.chars().filter(|&ch| ch == open).count()
    }

    pub(super) fn has_unmatched_trailing_opener(text: &str, open: char, close: char) -> bool {
        text.chars().filter(|&ch| ch == open).count()
            > text.chars().filter(|&ch| ch == close).count()
    }

    pub(super) fn select_active_signature(
        &self,
        signatures: &[SignatureCandidate],
        arg_count: usize,
        active_parameter: u32,
        supplied_argument_types: &[String],
    ) -> u32 {
        if signatures.is_empty() {
            return 0;
        }

        let desired = if arg_count == 0 {
            0
        } else {
            arg_count.max(active_parameter as usize + 1)
        };

        let mut best_idx = 0usize;
        let mut best_score = usize::MAX;
        let mut best_type_penalty = usize::MAX;
        let mut best_rest_penalty = usize::MAX;
        let mut best_total_params = usize::MAX;
        let is_trailing_argument_slot = arg_count > 0 && (active_parameter as usize) >= arg_count;

        for (idx, sig) in signatures.iter().enumerate() {
            let min_params = sig.required_params;
            let max_params = if sig.has_rest {
                usize::MAX
            } else {
                sig.total_params
            };
            let mut score = if desired < min_params {
                min_params.saturating_sub(desired)
            } else if desired > max_params {
                desired.saturating_sub(max_params)
            } else {
                0
            };
            if is_trailing_argument_slot {
                let active_index = active_parameter as usize;
                let trailing_slot_penalty = if active_index < sig.total_params {
                    usize::from(sig.has_rest)
                } else if sig.has_rest {
                    0
                } else {
                    2
                };
                score = score.saturating_add(trailing_slot_penalty);
            }
            let rest_penalty = usize::from(sig.has_rest);
            let type_penalty =
                self.argument_type_penalty(sig, active_parameter, supplied_argument_types);

            if score < best_score
                || (score == best_score && type_penalty < best_type_penalty)
                || (score == best_score
                    && type_penalty == best_type_penalty
                    && rest_penalty < best_rest_penalty)
                || (score == best_score
                    && type_penalty == best_type_penalty
                    && rest_penalty == best_rest_penalty
                    && sig.total_params < best_total_params)
            {
                best_idx = idx;
                best_score = score;
                best_type_penalty = type_penalty;
                best_rest_penalty = rest_penalty;
                best_total_params = sig.total_params;
            }
        }

        best_idx as u32
    }

    pub(super) fn argument_type_texts(
        &self,
        call_site: &CallSite<'_>,
        checker: &mut CheckerState<'_>,
    ) -> Vec<String> {
        let mut types = Vec::new();
        match call_site {
            CallSite::Regular(call_expr) => {
                let Some(args) = call_expr.arguments.as_ref() else {
                    return Vec::new();
                };

                for &arg_idx in &args.nodes {
                    let Some(arg_node) = self.arena.get(arg_idx) else {
                        continue;
                    };
                    if arg_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                        continue;
                    }
                    let arg_type_id = checker.get_type_of_node(arg_idx);
                    let arg_type = checker.resolve_lazy_type(arg_type_id);
                    let mut text = checker.format_type(arg_type);
                    let start = arg_node.pos as usize;
                    let end = (arg_node.end as usize).min(self.source_text.len());
                    let raw_literal = if start < end {
                        Some(self.source_text[start..end].trim())
                    } else {
                        None
                    };
                    // Preserve source literal text when available so overload
                    // scoring can distinguish e.g. '' from "hi" | "bye".
                    if let Some(raw) = raw_literal
                        && Self::string_literal_value(raw).is_some()
                    {
                        text = raw.to_string();
                    }
                    if (text == "any" || text == "unknown" || text == "error")
                        && let Some(raw) = raw_literal
                    {
                        if Self::is_numeric_literal_type(raw) {
                            text = "number".to_string();
                        } else if Self::string_literal_value(raw).is_some() {
                            text = raw.to_string();
                        } else if raw == "true" || raw == "false" {
                            text = "boolean".to_string();
                        }
                    }
                    types.push(text);
                }
            }
            CallSite::TaggedTemplate(tagged) => {
                // Tagged template signatures always receive a template strings array as
                // the first argument followed by `${}` expression values.
                types.push("TemplateStringsArray".to_string());
                let Some(tmpl_node) = self.arena.get(tagged.template) else {
                    return types;
                };
                let Some(tmpl_expr) = self.arena.get_template_expr(tmpl_node) else {
                    return types;
                };
                for &span_idx in &tmpl_expr.template_spans.nodes {
                    let Some(span_node) = self.arena.get(span_idx) else {
                        continue;
                    };
                    let Some(span_data) = self.arena.get_template_span(span_node) else {
                        continue;
                    };
                    let expr_idx = span_data.expression;
                    let Some(expr_node) = self.arena.get(expr_idx) else {
                        continue;
                    };
                    let expr_type_id = checker.get_type_of_node(expr_idx);
                    let expr_type = checker.resolve_lazy_type(expr_type_id);
                    let mut text = checker.format_type(expr_type);
                    let start = expr_node.pos as usize;
                    let end = (expr_node.end as usize).min(self.source_text.len());
                    let raw_literal = if start < end {
                        Some(self.source_text[start..end].trim())
                    } else {
                        None
                    };
                    if let Some(raw) = raw_literal
                        && Self::string_literal_value(raw).is_some()
                    {
                        text = raw.to_string();
                    }
                    if (text == "any" || text == "unknown" || text == "error")
                        && let Some(raw) = raw_literal
                    {
                        if Self::is_numeric_literal_type(raw) {
                            text = "number".to_string();
                        } else if Self::string_literal_value(raw).is_some() {
                            text = raw.to_string();
                        } else if raw == "true" || raw == "false" {
                            text = "boolean".to_string();
                        }
                    }
                    types.push(text);
                }
            }
        }
        types
    }

    pub(super) fn infer_type_param_substitutions_from_arguments(
        &self,
        signatures: &mut [SignatureCandidate],
        supplied_argument_types: &[String],
    ) {
        if supplied_argument_types.is_empty() {
            return;
        }

        for sig in signatures.iter_mut() {
            if (sig.type_params.is_empty() && sig.type_param_substitutions.is_empty())
                || sig.info.parameters.is_empty()
            {
                continue;
            }

            let substitution_pairs = sig.type_param_substitutions.clone();
            let mut repeated_identifier_type_counts: FxHashMap<String, usize> =
                FxHashMap::default();
            for param in &sig.info.parameters {
                if let Some((_, ty)) = param.label.rsplit_once(':') {
                    let ty = ty.trim();
                    if Self::is_identifier_like_type_name(ty) {
                        *repeated_identifier_type_counts
                            .entry(ty.to_string())
                            .or_insert(0) += 1;
                    }
                }
            }
            let mut inferred: FxHashMap<String, String> = FxHashMap::default();
            for (arg_index, arg_type_text) in supplied_argument_types.iter().enumerate() {
                if arg_type_text.is_empty()
                    || arg_type_text == "error"
                    || arg_type_text == "unknown"
                {
                    continue;
                }

                let param_idx = if arg_index < sig.info.parameters.len() {
                    arg_index
                } else if sig.has_rest {
                    sig.info.parameters.len().saturating_sub(1)
                } else {
                    continue;
                };

                let Some((_, param_ty)) = sig.info.parameters[param_idx].label.rsplit_once(':')
                else {
                    continue;
                };
                let param_ty = param_ty.trim();
                if sig.type_params.iter().any(|tp| tp == param_ty)
                    || sig
                        .type_param_substitutions
                        .iter()
                        .any(|(name, _)| name == param_ty)
                {
                    inferred
                        .entry(param_ty.to_string())
                        .or_insert_with(|| arg_type_text.clone());
                    continue;
                }

                if Self::is_literal_type_text(arg_type_text)
                    && Self::is_identifier_like_type_name(param_ty)
                    && repeated_identifier_type_counts
                        .get(param_ty)
                        .copied()
                        .unwrap_or(0)
                        >= 2
                {
                    inferred
                        .entry(param_ty.to_string())
                        .or_insert_with(|| arg_type_text.clone());
                    continue;
                }

                for (type_param_name, current_substitution) in &substitution_pairs {
                    if param_ty == current_substitution
                        && Self::is_literal_narrowing_for_base_type(
                            arg_type_text,
                            current_substitution,
                        )
                    {
                        inferred
                            .entry(type_param_name.clone())
                            .or_insert_with(|| arg_type_text.clone());
                    }
                }
            }

            if inferred.is_empty() {
                continue;
            }

            for (name, substitution) in inferred {
                if let Some(existing) = sig
                    .type_param_substitutions
                    .iter_mut()
                    .find(|(existing_name, _)| *existing_name == name)
                {
                    existing.1 = substitution;
                } else {
                    sig.type_param_substitutions.push((name, substitution));
                }
            }
        }
    }

    pub(super) fn is_identifier_like_type_name(type_name: &str) -> bool {
        if type_name.is_empty() || type_name.contains('.') || type_name.contains('<') {
            return false;
        }
        let mut chars = type_name.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        if !(first.is_ascii_alphabetic() || first == '_') {
            return false;
        }
        if !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
            return false;
        }
        !matches!(
            type_name,
            "string"
                | "number"
                | "boolean"
                | "bigint"
                | "symbol"
                | "any"
                | "unknown"
                | "never"
                | "void"
                | "object"
                | "null"
                | "undefined"
                | "true"
                | "false"
        )
    }

    pub(super) fn is_literal_type_text(type_text: &str) -> bool {
        let trimmed = type_text.trim();
        Self::string_literal_value(trimmed).is_some()
            || Self::is_numeric_literal_type(trimmed)
            || Self::is_bigint_literal_type(trimmed)
            || trimmed == "true"
            || trimmed == "false"
    }

    pub(super) fn is_literal_narrowing_for_base_type(
        arg_type_text: &str,
        base_type_text: &str,
    ) -> bool {
        let base = base_type_text.trim();
        let arg = arg_type_text.trim();
        if Self::string_literal_value(arg).is_some() {
            return base == "string";
        }
        if Self::is_bigint_literal_type(arg) {
            return base == "bigint";
        }
        if arg == "true" || arg == "false" {
            return base == "boolean";
        }
        if Self::is_numeric_literal_type(arg) {
            return base == "number";
        }
        false
    }

    pub(super) fn argument_type_penalty(
        &self,
        sig: &SignatureCandidate,
        _active_parameter: u32,
        supplied_argument_types: &[String],
    ) -> usize {
        if supplied_argument_types.is_empty() || sig.info.parameters.is_empty() {
            return 0;
        }

        let mut penalty = 0usize;
        for (arg_index, arg_type_text) in supplied_argument_types.iter().enumerate() {
            if arg_type_text.is_empty() || arg_type_text == "error" {
                continue;
            }
            let Some(arg_kind) = Self::primitive_kind_from_type_text(arg_type_text) else {
                continue;
            };

            let param_idx = if arg_index < sig.info.parameters.len() {
                arg_index
            } else if sig.has_rest {
                sig.info.parameters.len().saturating_sub(1)
            } else {
                penalty += 1;
                continue;
            };

            let Some((_, param_ty)) = sig.info.parameters[param_idx].label.rsplit_once(':') else {
                continue;
            };
            let param_ty = param_ty.trim();
            if let Some(arg_string_literal) = Self::string_literal_value(arg_type_text)
                && let Some(matches) =
                    Self::string_literal_union_contains(param_ty, arg_string_literal)
            {
                if !matches {
                    penalty += 1;
                }
                continue;
            }
            let Some(param_mask) = Self::primitive_kind_mask_from_type_text(param_ty) else {
                continue;
            };
            let arg_mask = Self::primitive_mask(arg_kind);
            if (param_mask & arg_mask) == 0 {
                penalty += 1;
            }
        }

        penalty
    }

    pub(super) const fn primitive_mask(kind: PrimitiveKind) -> u8 {
        match kind {
            PrimitiveKind::String => 0b0001,
            PrimitiveKind::Number => 0b0010,
            PrimitiveKind::Boolean => 0b0100,
            PrimitiveKind::BigInt => 0b1000,
        }
    }

    pub(super) fn primitive_kind_from_type_text(text: &str) -> Option<PrimitiveKind> {
        let mask = Self::primitive_kind_mask_from_type_text(text)?;
        if mask & Self::primitive_mask(PrimitiveKind::Number) != 0 {
            return Some(PrimitiveKind::Number);
        }
        if mask & Self::primitive_mask(PrimitiveKind::String) != 0 {
            return Some(PrimitiveKind::String);
        }
        if mask & Self::primitive_mask(PrimitiveKind::Boolean) != 0 {
            return Some(PrimitiveKind::Boolean);
        }
        if mask & Self::primitive_mask(PrimitiveKind::BigInt) != 0 {
            return Some(PrimitiveKind::BigInt);
        }
        None
    }

    pub(super) fn primitive_kind_mask_from_type_text(text: &str) -> Option<u8> {
        let normalized = text.trim().trim_matches(|c| c == '(' || c == ')');
        if normalized.is_empty() {
            return None;
        }

        let mut mask = 0u8;
        for part in normalized
            .split('|')
            .map(str::trim)
            .filter(|p| !p.is_empty())
        {
            if part == "string" || part.starts_with('"') || part.starts_with('\'') {
                mask |= Self::primitive_mask(PrimitiveKind::String);
                continue;
            }
            if part == "number" || Self::is_numeric_literal_type(part) {
                mask |= Self::primitive_mask(PrimitiveKind::Number);
                continue;
            }
            if part == "boolean" || part == "true" || part == "false" {
                mask |= Self::primitive_mask(PrimitiveKind::Boolean);
                continue;
            }
            if part == "bigint" || Self::is_bigint_literal_type(part) {
                mask |= Self::primitive_mask(PrimitiveKind::BigInt);
            }
        }

        (mask != 0).then_some(mask)
    }

    pub(super) fn is_numeric_literal_type(text: &str) -> bool {
        let mut chars = text.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        let rest = if first == '-' { chars.as_str() } else { text };
        if rest.is_empty() {
            return false;
        }
        rest.chars()
            .all(|ch| ch.is_ascii_digit() || ch == '.' || ch == 'e' || ch == 'E' || ch == '+')
    }

    pub(super) fn is_bigint_literal_type(text: &str) -> bool {
        let Some(stripped) = text.strip_suffix('n') else {
            return false;
        };
        !stripped.is_empty() && stripped.chars().all(|ch| ch.is_ascii_digit() || ch == '-')
    }

    pub(super) fn string_literal_value(text: &str) -> Option<&str> {
        let trimmed = text.trim();
        if trimmed.len() < 2 {
            return None;
        }
        let bytes = trimmed.as_bytes();
        let quote = bytes[0];
        if (quote == b'"' || quote == b'\'') && bytes[trimmed.len() - 1] == quote {
            return Some(&trimmed[1..trimmed.len() - 1]);
        }
        None
    }

    // Returns:
    // - Some(true) when parameter type is a pure union of string literals
    //   and includes the argument literal
    // - Some(false) when parameter type is a pure union of string literals
    //   and does not include the argument literal
    // - None when parameter type is not a pure string-literal union
    pub(super) fn string_literal_union_contains(
        param_type_text: &str,
        arg_literal_value: &str,
    ) -> Option<bool> {
        let mut seen = false;
        for part in param_type_text
            .split('|')
            .map(str::trim)
            .filter(|p| !p.is_empty())
        {
            let literal = Self::string_literal_value(part)?;
            seen = true;
            if literal == arg_literal_value {
                return Some(true);
            }
        }
        seen.then_some(false)
    }
}
