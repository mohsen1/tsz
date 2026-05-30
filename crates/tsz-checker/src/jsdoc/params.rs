//! JSDoc parameter tag checking and validation helpers.
//!
//! This module owns:
//! - TS8024 `@param` tag name checking
//! - `@param` tag syntax validation
//! - Implicit `arguments` object detection
//! - TS7014 Closure-style function parameter type checking
//!
//! See also:
//! - `params_comment_retrieval` — JSDoc comment position/content lookup
//! - `params_type_strings` — param type string extraction, nested params, `@type` tag analysis

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    pub(crate) const JSDOC_PARAM_TAG_NAMES: [&'static str; 3] = ["param", "arg", "argument"];

    pub(crate) fn strip_jsdoc_param_tag_prefix(text: &str) -> Option<(&'static str, &str)> {
        Self::JSDOC_PARAM_TAG_NAMES
            .iter()
            .find_map(|tag| Self::strip_jsdoc_tag_prefix(text, tag).map(|rest| (*tag, rest)))
    }

    pub(crate) fn jsdoc_param_tag_offset(text: &str) -> Option<(usize, &'static str)> {
        Self::JSDOC_PARAM_TAG_NAMES
            .iter()
            .filter_map(|tag| Self::jsdoc_tag_offset(text, tag).map(|offset| (offset, *tag)))
            .min_by_key(|(offset, _)| *offset)
    }

    pub(crate) const fn jsdoc_tag_source_len(tag: &str) -> usize {
        1 + tag.len()
    }

    // =========================================================================
    // JSDoc Helpers for Implicit Any Suppression
    // =========================================================================

    /// Get the effective JSDoc param name for a parameter, using positional
    /// matching for destructured parameters (binding patterns).
    ///
    /// For named parameters, returns the identifier text. For binding patterns
    /// like `{a, b}`, looks up the @param name at `pos` from the pre-extracted
    /// `jsdoc_names` list, falling back to `parameter_name_for_error`.
    pub(crate) fn effective_jsdoc_param_name(
        &self,
        param_name: tsz_parser::parser::NodeIndex,
        jsdoc_names: &[String],
        pos: usize,
    ) -> String {
        if self.ctx.arena.get(param_name).is_some_and(|n| {
            n.kind == tsz_parser::parser::syntax_kind_ext::OBJECT_BINDING_PATTERN
                || n.kind == tsz_parser::parser::syntax_kind_ext::ARRAY_BINDING_PATTERN
        }) {
            jsdoc_names
                .get(pos)
                .cloned()
                .unwrap_or_else(|| self.parameter_name_for_error(param_name))
        } else {
            self.parameter_name_for_error(param_name)
        }
    }

    pub(crate) fn jsdoc_marks_parameter_optional(
        &self,
        function_idx: NodeIndex,
        param_idx: NodeIndex,
        param_name: NodeIndex,
    ) -> bool {
        if !self.is_js_file() {
            return false;
        }

        let Some(function_node) = self.ctx.arena.get(function_idx) else {
            return false;
        };
        let parameters = if let Some(func) = self.ctx.arena.get_function(function_node) {
            &func.parameters.nodes
        } else if let Some(method) = self.ctx.arena.get_method_decl(function_node) {
            &method.parameters.nodes
        } else if let Some(ctor) = self.ctx.arena.get_constructor(function_node) {
            &ctor.parameters.nodes
        } else {
            return false;
        };

        let Some(param_position) = parameters.iter().position(|&idx| idx == param_idx) else {
            return false;
        };
        let Some(jsdoc) = self
            .get_jsdoc_for_function(function_idx)
            .or_else(|| self.find_jsdoc_for_function(function_idx))
        else {
            return false;
        };

        let jsdoc_param_names: Vec<String> = Self::extract_jsdoc_param_names(&jsdoc)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        let pname = self.effective_jsdoc_param_name(param_name, &jsdoc_param_names, param_position);
        !Self::jsdoc_has_required_param_tag(&jsdoc, &pname)
    }

    /// TS8024: Check that JSDoc `@param` tag names match actual function parameters.
    ///
    /// For each `@param` tag, verifies that a parameter with that name exists.
    /// Emits TS8024 for names that don't match any parameter.
    /// Skips empty names (malformed tags), dotted/array names (nested property docs),
    /// and the special name "this" (JSDoc this-type annotation).
    pub(crate) fn check_jsdoc_param_tag_names(
        &mut self,
        jsdoc: &str,
        param_nodes: &[NodeIndex],
        func_idx: NodeIndex,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        use tsz_parser::syntax_kind_ext;

        self.check_jsdoc_param_tag_syntax(func_idx);

        // Collect actual parameter names and whether each is a binding pattern
        let mut actual_params: Vec<(String, bool)> = Vec::new();
        for &param_idx in param_nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            let is_binding_pattern = self.ctx.arena.get(param.name).is_some_and(|n| {
                n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                    || n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            });
            let name = self.parameter_name_for_error(param.name);
            actual_params.push((name, is_binding_pattern));
        }

        // Extract @param tag names from JSDoc (only top-level, non-dotted names)
        let jsdoc_params = Self::extract_jsdoc_param_names(jsdoc);
        let has_implicit_arguments_candidate =
            actual_params.is_empty() && self.function_uses_implicit_arguments_object(func_idx);
        // tsc resolves `@param {...T}` rest types to `T[]` only when the
        // function expression has a name in its declarative context (function
        // declaration, variable initializer, property assignment). Anonymous
        // returned function expressions (`return function() {...}`) skip that
        // resolution, so the JSDoc rest type stays as `T` (not array) and
        // tsc still emits TS8029. Mirror that here.
        let function_has_effective_name = self.function_has_effective_name(func_idx);

        // Get source text and comment position for error positioning
        let source_info = self
            .get_jsdoc_comment_pos_for_function(func_idx)
            .and_then(|pos| {
                let sf = self.ctx.arena.source_files.first()?;
                Some((pos, sf.text.clone()))
            });

        // Track which @param tags we've seen (for positional matching with destructured params)
        let mut param_tag_index = 0usize;
        for (param_name, tag_offset) in &jsdoc_params {
            // Skip "this" — JSDoc @param {type} this is a this-type annotation
            if param_name == "this" {
                continue;
            }
            // Check if this name matches any actual parameter by name
            let matches_by_name = actual_params.iter().any(|(a, _)| a == param_name);
            // Check if this @param positionally corresponds to a binding pattern
            // (destructured param like { a, b, c } accepts any @param name at that position)
            let matches_binding_pattern = actual_params
                .get(param_tag_index)
                .is_some_and(|(_, is_pattern)| *is_pattern);
            param_tag_index += 1;
            if matches_by_name || matches_binding_pattern {
                continue;
            }
            // tsc's three cases for an unmatched @param tag:
            //   1. function uses `arguments` AND the JSDoc tag is rest (`...T`)
            //      AND the function has an effective name (so tsc resolves
            //      the JSDoc rest type to `T[]` and `!isArrayType` is false)
            //      → no error.
            //   2. function uses `arguments` AND the JSDoc tag is NOT rest, OR
            //      it is rest but the function is anonymous (resolution skips
            //      the array promotion) → TS8029.
            //   3. function does not use `arguments` → TS8024.
            let jsdoc_tag_is_rest = Self::jsdoc_param_is_rest(jsdoc, param_name);
            if has_implicit_arguments_candidate && jsdoc_tag_is_rest && function_has_effective_name
            {
                continue;
            }
            let (message, code) = if has_implicit_arguments_candidate {
                (
                    format_message(
                        diagnostic_messages::JSDOC_PARAM_TAG_HAS_NAME_BUT_THERE_IS_NO_PARAMETER_WITH_THAT_NAME_IT_WOULD_MATCH,
                        &[param_name],
                    ),
                    diagnostic_codes::JSDOC_PARAM_TAG_HAS_NAME_BUT_THERE_IS_NO_PARAMETER_WITH_THAT_NAME_IT_WOULD_MATCH,
                )
            } else {
                (
                    format_message(
                        diagnostic_messages::JSDOC_PARAM_TAG_HAS_NAME_BUT_THERE_IS_NO_PARAMETER_WITH_THAT_NAME,
                        &[param_name],
                    ),
                    diagnostic_codes::JSDOC_PARAM_TAG_HAS_NAME_BUT_THERE_IS_NO_PARAMETER_WITH_THAT_NAME,
                )
            };
            // No match — emit TS8024/TS8029
            {
                // Position at the parameter name within the JSDoc comment in source
                if let Some((comment_pos, ref source_text)) = source_info {
                    // Search for the name after @param in the source text within the comment
                    let comment_start = comment_pos as usize;
                    // Find param_name after an @param tag in the comment text
                    let search_region = &source_text[comment_start..];
                    let mut name_pos = None;
                    let mut search_from = (*tag_offset).min(search_region.len());
                    while let Some((at_param, param_tag)) =
                        Self::jsdoc_param_tag_offset(&search_region[search_from..])
                    {
                        let after_param =
                            search_from + at_param + Self::jsdoc_tag_source_len(param_tag);
                        // Find the name after the @param tag (skip {type} if present)
                        if let Some(n) = Self::find_param_name_in_source(
                            &search_region[after_param..],
                            param_name,
                        ) {
                            name_pos = Some(comment_start + after_param + n);
                            break;
                        }
                        search_from = after_param;
                    }
                    if let Some(pos) = name_pos {
                        let name_len = if param_name.is_empty() {
                            1
                        } else {
                            param_name.len() as u32
                        };
                        self.ctx.error(pos as u32, name_len, message, code);
                    } else {
                        self.error_at_node(func_idx, &message, code);
                    }
                } else {
                    self.error_at_node(func_idx, &message, code);
                }
            }
        }
    }

    fn check_jsdoc_param_tag_syntax(&mut self, func_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        let Some(comment_pos) = self.get_jsdoc_comment_pos_for_function(func_idx) else {
            return;
        };
        let Some(func_node) = self.ctx.arena.get(func_idx) else {
            return;
        };
        let Some(source_file) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text = source_file.text.clone();
        let comment_start = comment_pos as usize;
        let comment_end = (func_node.pos as usize).min(source_text.len());
        if comment_start >= comment_end {
            return;
        }
        let Some(comment_text) = source_text.get(comment_start..comment_end) else {
            return;
        };

        let mut valid_param_tags = Vec::new();
        let mut line_start = comment_start;
        for chunk in comment_text.split_inclusive('\n') {
            let raw_line = chunk.trim_end_matches('\n').trim_end_matches('\r');
            let Some((at_param, param_tag)) = Self::jsdoc_param_tag_offset(raw_line) else {
                line_start += chunk.len();
                continue;
            };
            let after_param_start = at_param + Self::jsdoc_tag_source_len(param_tag);
            let after_param = &raw_line[after_param_start..];
            let trimmed_after_param = after_param.trim_start();
            let leading_ws = after_param.len() - trimmed_after_param.len();
            if !trimmed_after_param.starts_with('{') {
                if let Some(param) = Self::parse_jsdoc_param_tag(after_param) {
                    valid_param_tags.push((param.rest, None));
                }
                line_start += chunk.len();
                continue;
            }

            let type_open = after_param_start + leading_ws;
            let type_source_start = type_open + 1;
            if let Some((type_expr, _after_type)) =
                Self::parse_jsdoc_curly_type_expr(raw_line.get(type_open..).unwrap_or_default())
            {
                if let Some(error_offset) = Self::jsdoc_param_type_syntax_error_offset(type_expr) {
                    let error_pos = (line_start + type_source_start + error_offset) as u32;
                    let close_brace_expected =
                        format_message(diagnostic_messages::EXPECTED, &["}"]);
                    self.emit_jsdoc_param_syntax_diagnostic_once(
                        error_pos,
                        1,
                        &close_brace_expected,
                        diagnostic_codes::EXPECTED,
                    );
                    let message = format_message(
                        diagnostic_messages::JSDOC_PARAM_TAG_HAS_NAME_BUT_THERE_IS_NO_PARAMETER_WITH_THAT_NAME,
                        &[""],
                    );
                    self.emit_jsdoc_param_syntax_diagnostic_once(
                        error_pos,
                        1,
                        &message,
                        diagnostic_codes::JSDOC_PARAM_TAG_HAS_NAME_BUT_THERE_IS_NO_PARAMETER_WITH_THAT_NAME,
                    );
                } else if let Some(param) = Self::parse_jsdoc_param_tag(after_param) {
                    let rest_pos = param
                        .rest
                        .then_some((line_start + type_source_start) as u32);
                    valid_param_tags.push((param.rest, rest_pos));
                }
            }

            line_start += chunk.len();
        }

        for (idx, (is_rest, rest_pos)) in valid_param_tags.iter().enumerate() {
            if !*is_rest || valid_param_tags[idx + 1..].is_empty() {
                continue;
            }
            if let Some(rest_pos) = rest_pos {
                self.emit_jsdoc_param_syntax_diagnostic_once(
                    *rest_pos,
                    3,
                    diagnostic_messages::A_REST_PARAMETER_MUST_BE_LAST_IN_A_PARAMETER_LIST,
                    diagnostic_codes::A_REST_PARAMETER_MUST_BE_LAST_IN_A_PARAMETER_LIST,
                );
            }
        }
    }

    fn emit_jsdoc_param_syntax_diagnostic_once(
        &mut self,
        start: u32,
        length: u32,
        message: &str,
        code: u32,
    ) {
        if self
            .ctx
            .diagnostics
            .iter()
            .any(|diag| diag.start == start && diag.length == length && diag.code == code)
        {
            return;
        }
        self.error_at_position(start, length, message, code);
    }

    /// Whether a function has an effective name in its declarative context.
    ///
    /// True when the function declaration carries an identifier name, or
    /// when the function expression sits in a position that gives it a name
    /// (variable initializer, property/shorthand assignment, binary `=`
    /// assignment, parameter default). False for anonymous expressions in
    /// positions like `return function() {...}` or array literals.
    pub(crate) fn function_has_effective_name(&self, func_idx: NodeIndex) -> bool {
        use tsz_parser::syntax_kind_ext;
        let Some(node) = self.ctx.arena.get(func_idx) else {
            return false;
        };
        if let Some(func) = self.ctx.arena.get_function(node)
            && func.name.is_some()
        {
            return true;
        }
        let Some(ext) = self.ctx.arena.get_extended(func_idx) else {
            return false;
        };
        let Some(parent_node) = self.ctx.arena.get(ext.parent) else {
            return false;
        };
        matches!(
            parent_node.kind,
            syntax_kind_ext::VARIABLE_DECLARATION
                | syntax_kind_ext::PROPERTY_ASSIGNMENT
                | syntax_kind_ext::PARAMETER
                | syntax_kind_ext::BINARY_EXPRESSION
                | syntax_kind_ext::PROPERTY_DECLARATION
        )
    }

    fn function_uses_implicit_arguments_object(&self, func_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(func_idx) else {
            return false;
        };

        let body = if let Some(func) = self.ctx.arena.get_function(node) {
            Some(func.body)
        } else if let Some(method) = self.ctx.arena.get_method_decl(node) {
            Some(method.body)
        } else {
            self.ctx.arena.get_constructor(node).map(|ctor| ctor.body)
        };

        body.is_some_and(|body_idx| self.body_has_arguments_reference(body_idx))
    }

    /// TS7014: Check Closure-style JSDoc function parameter types for missing
    /// return annotations, e.g. `@param {function(...[*])} cb`.
    pub(crate) fn check_jsdoc_param_function_types_missing_return_type(
        &mut self,
        jsdoc: &str,
        func_idx: NodeIndex,
    ) {
        if !self.ctx.no_implicit_any() {
            return;
        }

        let Some(comment_pos) = self.get_jsdoc_comment_pos_for_function(func_idx) else {
            return;
        };
        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text = &sf.text;
        let comment_start = comment_pos as usize;
        let comment_end = self
            .ctx
            .arena
            .get(func_idx)
            .map_or(source_text.len(), |n| n.pos as usize);
        let comment_text = &source_text[comment_start..comment_end.min(source_text.len())];

        for (param_name, tag_offset) in Self::extract_jsdoc_param_names(jsdoc) {
            let search_start = tag_offset;
            let Some((rel_tag, param_tag)) =
                Self::jsdoc_param_tag_offset(&comment_text[search_start..])
            else {
                continue;
            };
            let tag_start = search_start + rel_tag;
            let after_tag = &comment_text[tag_start + Self::jsdoc_tag_source_len(param_tag)..];
            let trimmed = after_tag.trim_start();
            let leading_ws = after_tag.len() - trimmed.len();
            if !trimmed.starts_with('{') {
                continue;
            }
            let mut depth = 0usize;
            let mut type_end = None;
            for (i, ch) in trimmed.char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            type_end = Some(i);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let Some(type_end) = type_end else {
                continue;
            };
            let type_expr = trimmed[1..type_end].trim();
            let Some(rest) = type_expr.strip_prefix("function(") else {
                continue;
            };
            let mut paren_depth = 1u32;
            let mut close_idx = None;
            for (i, ch) in rest.char_indices() {
                match ch {
                    '(' => paren_depth += 1,
                    ')' => {
                        paren_depth -= 1;
                        if paren_depth == 0 {
                            close_idx = Some(i);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let Some(close_idx) = close_idx else {
                continue;
            };
            let after_close = rest[close_idx + 1..].trim();
            if after_close.starts_with(':') {
                continue;
            }

            let function_rel = tag_start + Self::jsdoc_tag_source_len(param_tag) + leading_ws + 1;
            let function_pos = comment_pos + function_rel as u32;
            self.ctx.error(
                function_pos,
                "function".len() as u32,
                crate::diagnostics::format_message(
                    crate::diagnostics::diagnostic_messages::FUNCTION_TYPE_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN_TYPE,
                    &["any"],
                ),
                crate::diagnostics::diagnostic_codes::FUNCTION_TYPE_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN_TYPE,
            );

            if param_name.is_empty() {
                continue;
            }
        }
    }

    /// Find the byte offset of a parameter name after `@param` in source text.
    ///
    /// Given the text after `@param`, skips optional `{type}` and whitespace,
    /// then checks if the next word matches `name`. Returns the byte offset
    /// of the name relative to the start of the input.
    fn find_param_name_in_source(after_param: &str, name: &str) -> Option<usize> {
        let mut rest = after_param;
        let mut offset = 0;
        // Skip whitespace
        let trimmed = rest.trim_start();
        offset += rest.len() - trimmed.len();
        rest = trimmed;
        // Skip {type} if present
        if rest.starts_with('{') {
            let mut depth = 0usize;
            for (i, ch) in rest.char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            offset += i + 1;
                            rest = &rest[i + 1..];
                            break;
                        }
                    }
                    _ => {}
                }
            }
            // Skip whitespace after type
            let trimmed = rest.trim_start();
            offset += rest.len() - trimmed.len();
            rest = trimmed;
        }
        // Strip optional [ for optional params like [name] or [name=default]
        if rest.starts_with('[') {
            offset += 1;
            rest = &rest[1..];
        }
        if name.is_empty() {
            if let Some(after_star) = rest.strip_prefix('*') {
                let ws_after_star = after_star.len() - after_star.trim_start().len();
                if after_star.trim_start().starts_with('*') {
                    return Some(offset + 1 + ws_after_star);
                }
            }
            return (!rest.is_empty()).then_some(offset);
        }
        // Check if the next word is the name
        if let Some(after_name) = rest.strip_prefix(name) {
            // Verify it's a complete word (followed by non-alphanumeric or end)
            if after_name.is_empty() || !after_name.chars().next().unwrap_or('\0').is_alphanumeric()
            {
                return Some(offset);
            }
        }
        None
    }
}

#[cfg(test)]
#[path = "../types/utilities/tests/jsdoc_params_tests.rs"]
mod tests;
