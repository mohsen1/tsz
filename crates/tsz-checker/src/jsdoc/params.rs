//! JSDoc parameter validation, comment finding, and text parsing utilities.
//!
//! This module owns:
//! - TS8024 `@param` tag name checking
//! - JSDoc comment position/content lookup (ancestor walk, leading comment search)
//! - Pure text-level JSDoc parsing helpers (param names, type expressions, etc.)
//! - Nested `@param` object type construction
//! - `@type` tag analysis (callable detection, broad function check)

use super::types::JsdocParamTagInfo;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
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
            // No match — emit TS8024
            {
                let message = format_message(
                    diagnostic_messages::JSDOC_PARAM_TAG_HAS_NAME_BUT_THERE_IS_NO_PARAMETER_WITH_THAT_NAME,
                    &[param_name],
                );
                // Position at the parameter name within the JSDoc comment in source
                if let Some((comment_pos, ref source_text)) = source_info {
                    // Search for the name after @param in the source text within the comment
                    let comment_start = comment_pos as usize;
                    // Find param_name after an @param tag in the comment text
                    let search_region = &source_text[comment_start..];
                    let mut name_pos = None;
                    let mut search_from = (*tag_offset).min(search_region.len());
                    while let Some(at_param) = search_region[search_from..].find("@param") {
                        let after_param = search_from + at_param + "@param".len();
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
                        self.ctx.error(
                            pos as u32,
                            name_len,
                            message,
                            diagnostic_codes::JSDOC_PARAM_TAG_HAS_NAME_BUT_THERE_IS_NO_PARAMETER_WITH_THAT_NAME,
                        );
                    } else {
                        self.error_at_node(
                            func_idx,
                            &message,
                            diagnostic_codes::JSDOC_PARAM_TAG_HAS_NAME_BUT_THERE_IS_NO_PARAMETER_WITH_THAT_NAME,
                        );
                    }
                } else {
                    self.error_at_node(
                        func_idx,
                        &message,
                        diagnostic_codes::JSDOC_PARAM_TAG_HAS_NAME_BUT_THERE_IS_NO_PARAMETER_WITH_THAT_NAME,
                    );
                }
            }
        }
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
            let Some(rel_tag) = comment_text[search_start..].find("@param") else {
                continue;
            };
            let tag_start = search_start + rel_tag;
            let after_tag = &comment_text[tag_start + "@param".len()..];
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

            let function_rel = tag_start + "@param".len() + leading_ws + 1;
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

    /// Get the byte position of the JSDoc comment for a function node.
    ///
    /// Returns `Some(pos)` where pos is the byte offset of `/**` in the source.
    pub(crate) fn get_jsdoc_comment_pos_for_function(&self, func_idx: NodeIndex) -> Option<u32> {
        use tsz_common::comments::is_jsdoc_comment;

        let sf = self.ctx.arena.source_files.first()?;
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let func_node = self.ctx.arena.get(func_idx)?;

        // Check inline JSDoc
        if let Some(comment) = comments
            .iter()
            .find(|c| c.pos <= func_node.pos && func_node.pos < c.end)
            && is_jsdoc_comment(comment, source_text)
        {
            return Some(comment.pos);
        }

        // Check leading comments
        for comment in comments.iter().rev() {
            if comment.end <= func_node.pos && is_jsdoc_comment(comment, source_text) {
                // Check that there's nothing but whitespace between comment and node
                let between = &source_text[comment.end as usize..func_node.pos as usize];
                if between.trim().is_empty() {
                    return Some(comment.pos);
                }
            }
        }

        if func_node.kind == tsz_parser::parser::syntax_kind_ext::FUNCTION_DECLARATION {
            // For `export function f(...)`, the JSDoc is before `export` but
            // func_node.pos is at `function`. Check the parent ExportDeclaration.
            if let Some(ext) = self.ctx.arena.get_extended(func_idx)
                && let Some(parent_node) = self.ctx.arena.get(ext.parent)
                && parent_node.kind == tsz_parser::parser::syntax_kind_ext::EXPORT_DECLARATION
            {
                for comment in comments.iter().rev() {
                    if comment.end <= parent_node.pos && is_jsdoc_comment(comment, source_text) {
                        let between = &source_text[comment.end as usize..parent_node.pos as usize];
                        if between.trim().is_empty() {
                            return Some(comment.pos);
                        }
                    }
                }
            }
            return None;
        }

        // Walk up parent chain (for const f = ...)
        let mut current = func_idx;
        for _ in 0..4 {
            let ext = self.ctx.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }
            let parent_node = self.ctx.arena.get(parent)?;
            for comment in comments.iter().rev() {
                if comment.end <= parent_node.pos && is_jsdoc_comment(comment, source_text) {
                    let between = &source_text[comment.end as usize..parent_node.pos as usize];
                    if between.trim().is_empty() {
                        return Some(comment.pos);
                    }
                }
            }
            current = parent;
        }

        None
    }

    /// Get the `JSDoc` comment content for a function node.
    ///
    /// Walks up the parent chain from the function node to find the `JSDoc`
    /// comment. For variable-assigned functions (e.g., `const f = () => {}`),
    /// the `JSDoc` is on the variable statement, not the function itself.
    ///
    /// Returns the raw `JSDoc` content (without `/**` and `*/` delimiters).
    pub(crate) fn get_jsdoc_for_function(&self, func_idx: NodeIndex) -> Option<String> {
        if self.is_js_file() && !self.ctx.compiler_options.check_js {
            return None;
        }
        self.find_jsdoc_for_function(func_idx)
    }

    /// Resolve a function's JSDoc `@type` annotation into a callable type when
    /// it actually carries call signatures (including callback typedef aliases).
    ///
    /// Broad object-ish annotations like `Function` should not count here
    /// because they do not provide concrete parameter types and should still
    /// allow TS7006 to fire.
    pub(crate) fn jsdoc_callable_type_annotation_for_function(
        &mut self,
        func_idx: NodeIndex,
    ) -> Option<TypeId> {
        if !self.ctx.should_resolve_jsdoc() {
            return None;
        }

        let sf = self.source_file_data_for_node(func_idx)?;
        if sf.comments.is_empty() {
            return None;
        }

        let source_text = sf.text.to_string();
        let comments = sf.comments.clone();
        let node = self.ctx.arena.get(func_idx)?;
        let jsdoc = self.get_jsdoc_for_function(func_idx)?;
        let type_expr = Self::extract_jsdoc_type_expression(&jsdoc)?;
        self.jsdoc_concrete_callable_type_from_expr(type_expr, node.pos, &comments, &source_text)
    }

    pub(crate) fn jsdoc_type_tag_references_callback_typedef(
        &self,
        func_idx: NodeIndex,
        jsdoc: &str,
    ) -> bool {
        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};

        let Some(type_expr) = Self::jsdoc_extract_type_tag_expr(jsdoc) else {
            return false;
        };
        let Some(sf) = self.source_file_data_for_node(func_idx) else {
            return false;
        };

        // Scan all comments — @callback/@typedef are hoisted to file scope
        // in tsc, so forward references must be supported.
        for comment in &sf.comments {
            if !is_jsdoc_comment(comment, &sf.text) {
                continue;
            }
            let content = get_jsdoc_content(comment, &sf.text);
            if Self::parse_jsdoc_typedefs(&content)
                .into_iter()
                .any(|(name, info)| name == type_expr && info.callback.is_some())
            {
                return true;
            }
        }

        false
    }

    /// Find the JSDoc comment for a function node without checking compiler options.
    ///
    /// Used by `get_jsdoc_for_function` (which adds a `check_js` guard) and by
    /// TS8024 validation which needs JSDoc lookup independent of the checker's
    /// `check_js` state (the driver-level `check_js` controls JS file inclusion).
    pub(crate) fn find_jsdoc_for_function(&self, func_idx: NodeIndex) -> Option<String> {
        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};

        let sf = self.ctx.arena.source_files.first()?;
        let source_text: &str = &sf.text;
        let comments = &sf.comments;

        // Try the function node itself first
        let func_node = self.ctx.arena.get(func_idx)?;

        // For inline JSDoc (comment overlapping with node position)
        if let Some(comment) = comments
            .iter()
            .find(|c| c.pos <= func_node.pos && func_node.pos < c.end)
            && is_jsdoc_comment(comment, source_text)
        {
            return Some(get_jsdoc_content(comment, source_text));
        }

        if func_node.kind == tsz_parser::parser::syntax_kind_ext::FUNCTION_DECLARATION {
            if let Some(jsdoc) = self.try_leading_jsdoc(comments, func_node.pos, source_text) {
                return Some(jsdoc);
            }
            // For `export function f(...)`, the JSDoc is before the `export` keyword
            // but func_node.pos is at `function`. Walk up to the parent
            // (ExportDeclaration) to find the JSDoc there.
            if let Some(ext) = self.ctx.arena.get_extended(func_idx)
                && let Some(parent_node) = self.ctx.arena.get(ext.parent)
                && parent_node.kind == tsz_parser::parser::syntax_kind_ext::EXPORT_DECLARATION
            {
                return self.try_leading_jsdoc(comments, parent_node.pos, source_text);
            }
            return None;
        }

        // Try leading comments, then walk up the parent chain for
        // `const f = value => ...` where JSDoc is on the `const` line.
        self.try_jsdoc_with_ancestor_walk(func_idx, comments, source_text)
    }

    /// Try to find a leading JSDoc comment for a node, walking up to 4 ancestors.
    ///
    /// First checks `idx` itself, then walks the parent chain up to 4 levels.
    /// Returns the first JSDoc content found, or `None`.
    pub(crate) fn effective_jsdoc_pos_for_node(
        &self,
        idx: NodeIndex,
        comments: &[tsz_common::comments::CommentRange],
        source_text: &str,
    ) -> Option<u32> {
        let node = self.ctx.arena.get(idx)?;
        let mut pos = node.pos as usize;
        let end = node.end as usize;

        while pos < end {
            let remaining = source_text.get(pos..end)?;
            let trimmed = remaining.trim_start_matches(char::is_whitespace);
            if trimmed.len() != remaining.len() {
                pos += remaining.len() - trimmed.len();
                continue;
            }

            if let Some(comment) = comments
                .iter()
                .find(|comment| comment.pos as usize == pos && comment.end as usize <= end)
            {
                pos = comment.end as usize;
                continue;
            }

            break;
        }

        Some(pos as u32)
    }

    pub(crate) fn try_jsdoc_with_ancestor_walk(
        &self,
        idx: NodeIndex,
        comments: &[tsz_common::comments::CommentRange],
        source_text: &str,
    ) -> Option<String> {
        let jsdoc = self.try_leading_jsdoc(
            comments,
            self.effective_jsdoc_pos_for_node(idx, comments, source_text)?,
            source_text,
        );
        if jsdoc.is_some() {
            return jsdoc;
        }
        let mut current = idx;
        for _ in 0..4 {
            let ext = self.ctx.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }
            // Stop before checking statement-level containers whose "leading
            // JSDoc" belongs to their first child statement, not to the node
            // we started the walk from. Without this guard, `var res` in:
            //   /** @type {Foo} */ export const x = ...
            //   var res = x()
            // would inherit Foo through SourceFile's leading-comment position.
            if let Some(parent_node) = self.ctx.arena.get(parent) {
                use tsz_parser::parser::syntax_kind_ext as sk;
                if matches!(
                    parent_node.kind,
                    sk::SOURCE_FILE
                        | sk::BLOCK
                        | sk::MODULE_BLOCK
                        | sk::CASE_CLAUSE
                        | sk::DEFAULT_CLAUSE
                ) {
                    break;
                }
            }
            let jsdoc = self.try_leading_jsdoc(
                comments,
                self.effective_jsdoc_pos_for_node(parent, comments, source_text)?,
                source_text,
            );
            if jsdoc.is_some() {
                return jsdoc;
            }
            current = parent;
        }
        None
    }

    /// Like `try_jsdoc_with_ancestor_walk` but also returns the absolute start
    /// position of the JSDoc comment in the source file.
    ///
    /// This is needed for `@satisfies` to compute the `@satisfies` keyword offset.
    pub(crate) fn try_jsdoc_with_ancestor_walk_and_pos(
        &self,
        idx: NodeIndex,
        comments: &[tsz_common::comments::CommentRange],
        source_text: &str,
    ) -> Option<(String, u32)> {
        if let Some((content, pos)) = self.try_leading_jsdoc_with_pos(
            comments,
            self.effective_jsdoc_pos_for_node(idx, comments, source_text)?,
            source_text,
        ) {
            return Some((content, pos));
        }
        let mut current = idx;
        for _ in 0..4 {
            let ext = self.ctx.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }
            // Same container guard as try_jsdoc_with_ancestor_walk.
            if let Some(parent_node) = self.ctx.arena.get(parent) {
                use tsz_parser::parser::syntax_kind_ext as sk;
                if matches!(
                    parent_node.kind,
                    sk::SOURCE_FILE
                        | sk::BLOCK
                        | sk::MODULE_BLOCK
                        | sk::CASE_CLAUSE
                        | sk::DEFAULT_CLAUSE
                ) {
                    break;
                }
            }
            if let Some((content, pos)) = self.try_leading_jsdoc_with_pos(
                comments,
                self.effective_jsdoc_pos_for_node(parent, comments, source_text)?,
                source_text,
            ) {
                return Some((content, pos));
            }
            current = parent;
        }
        None
    }

    /// Try to find a leading `JSDoc` comment and its start position.
    pub(crate) fn try_leading_jsdoc_with_pos(
        &self,
        comments: &[tsz_common::comments::CommentRange],
        pos: u32,
        source_text: &str,
    ) -> Option<(String, u32)> {
        use tsz_common::comments::{
            get_jsdoc_content, get_leading_comments_from_cache, is_jsdoc_comment,
        };

        let leading = get_leading_comments_from_cache(comments, pos, source_text);
        if let Some(comment) = leading.last() {
            let end = comment.end as usize;
            let check = pos as usize;
            if end <= check
                && source_text
                    .get(end..check)
                    .is_some_and(|gap| gap.chars().all(char::is_whitespace))
                && is_jsdoc_comment(comment, source_text)
            {
                return Some((get_jsdoc_content(comment, source_text), comment.pos));
            }
        }
        None
    }

    /// Check if `idx` or an ancestor has a leading JSDoc comment containing `@satisfies`.
    ///
    /// This is used by contextual typing code paths that need to treat inline JSDoc
    /// wrappers like `/** @satisfies ... */ expr` as an explicit typing boundary.
    pub(crate) fn has_satisfies_jsdoc_comment(&self, idx: NodeIndex) -> bool {
        let sf = match self.ctx.arena.source_files.first() {
            Some(sf) => sf,
            None => return false,
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;

        if let Some(jsdoc) = self.try_jsdoc_with_ancestor_walk(idx, comments, source_text) {
            return jsdoc.contains("@satisfies");
        }

        false
    }

    /// Try to find a leading `JSDoc` comment before a given position.
    pub(crate) fn try_leading_jsdoc(
        &self,
        comments: &[tsz_common::comments::CommentRange],
        pos: u32,
        source_text: &str,
    ) -> Option<String> {
        use tsz_common::comments::{
            get_jsdoc_content, get_leading_comments_from_cache, is_jsdoc_comment,
        };

        let leading = get_leading_comments_from_cache(comments, pos, source_text);
        if let Some(comment) = leading.last() {
            let end = comment.end as usize;
            let check = pos as usize;
            if end <= check
                && source_text
                    .get(end..check)
                    .is_some_and(|gap| gap.chars().all(char::is_whitespace))
                && is_jsdoc_comment(comment, source_text)
            {
                return Some(get_jsdoc_content(comment, source_text));
            }
        }
        None
    }

    /// Check if a parameter node has an inline `/** @type {T} */` `JSDoc` annotation.
    ///
    /// In TypeScript, parameters can have inline `JSDoc` type annotations like:
    ///   `function foo(/** @type {string} */ msg, /** @type {number} */ count)`
    /// These annotations suppress TS7006 because the parameter type is provided via `JSDoc`.
    pub(crate) fn param_has_inline_jsdoc_type(&self, param_idx: NodeIndex) -> bool {
        let sf = match self.ctx.arena.source_files.first() {
            Some(sf) => sf,
            None => return false,
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;

        let param_node = match self.ctx.arena.get(param_idx) {
            Some(n) => n,
            None => return false,
        };

        // Look for a JSDoc comment that ends right before or overlaps the parameter position
        if let Some(content) = self.try_leading_jsdoc(comments, param_node.pos, source_text) {
            // Check if the JSDoc contains @type {something}
            return content.contains("@type");
        }

        false
    }

    /// Check if a node is inside a JSDoc `@type` cast parenthesized expression.
    ///
    /// Walks up the parent chain looking for a `PARENTHESIZED_EXPRESSION` with a
    /// leading `/** @type {...} */` JSDoc comment. This is used to suppress TS7006
    /// for arrow/function parameters inside JSDoc type casts like:
    ///   `/** @type {import("./foo").Bar} */({ doer: q => q })`
    ///
    /// Even if the import type can't be fully resolved, the explicit @type
    /// annotation means the user intended to type the expression.
    pub(crate) fn is_inside_jsdoc_type_cast(&self, idx: NodeIndex) -> bool {
        let sf = match self.ctx.arena.source_files.first() {
            Some(sf) => sf,
            None => return false,
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;

        let mut current = idx;
        // Walk up at most 8 levels to find an enclosing JSDoc @type cast.
        // Typical nesting: arrow -> (params) -> property -> obj_literal -> paren_expr
        for _ in 0..8 {
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
            if parent_node.kind == tsz_parser::parser::syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(jsdoc) = self.try_leading_jsdoc(comments, parent_node.pos, source_text)
                && jsdoc.contains("@type")
            {
                return true;
            }
            current = parent;
        }
        false
    }

    /// Check if a node has a `/** @override */` JSDoc annotation.
    pub(crate) fn has_jsdoc_override_tag(&self, idx: NodeIndex) -> bool {
        if !self.is_js_file() {
            return false;
        }

        let sf = match self.ctx.arena.source_files.first() {
            Some(sf) => sf,
            None => return false,
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;

        self.try_jsdoc_with_ancestor_walk(idx, comments, source_text)
            .is_some_and(|content| content.contains("@override"))
    }

    /// Check if a `JSDoc` comment has a `@param {type}` annotation for the given parameter name.
    ///
    /// Returns true if the `JSDoc` contains `@param {someType} paramName`.
    pub(crate) fn jsdoc_has_param_type(jsdoc: &str, param_name: &str) -> bool {
        Self::extract_jsdoc_param_type_string(jsdoc, param_name).is_some()
    }

    /// Returns true if the JSDoc contains a `@param` tag for `param_name` that
    /// makes the parameter required (not optional).
    ///
    /// JSDoc optional param syntax (returns false for these):
    /// - `@param {Type=} name` — optional type suffix
    /// - `@param {Type} [name]` — brackets around name
    /// - `@param {Type} [name=default]` — brackets with default
    ///
    /// Non-optional `@param` tags (returns true):
    /// - `@param name` — name-only, no type
    /// - `@param {Type} name` — standard typed param
    pub(crate) fn jsdoc_has_required_param_tag(jsdoc: &str, param_name: &str) -> bool {
        for chunk in jsdoc.split_inclusive('\n') {
            let trimmed = chunk.trim_end_matches('\n').trim();

            let effective = Self::skip_backtick_quoted(trimmed);

            if let Some(rest) = effective.strip_prefix("@param")
                && let Some(param) = Self::parse_jsdoc_param_tag(rest)
                && param.name == param_name
                && !param.optional
            {
                return true;
            }
        }
        false
    }

    /// Extract the type expression string from a `@param {type} name` JSDoc tag.
    ///
    /// Returns the type expression (e.g., "Object.<string, boolean>") for the given
    /// parameter name, or None if no matching `@param` tag is found.
    pub(crate) fn extract_jsdoc_param_type_string(jsdoc: &str, param_name: &str) -> Option<String> {
        let mut in_param = false;
        let mut param_text = String::new();
        for chunk in jsdoc.split_inclusive('\n') {
            let trimmed = chunk.trim_end_matches('\n').trim();

            let effective = Self::skip_backtick_quoted(trimmed);

            if effective.starts_with('@') {
                if in_param {
                    if let Some((type_expr, _)) =
                        Self::extract_jsdoc_param_type_expr_from_param_tag(&param_text, param_name)
                    {
                        return Some(type_expr);
                    }
                    param_text.clear();
                }
                if let Some(rest) = effective.strip_prefix("@param") {
                    in_param = true;
                    param_text = rest.to_string();
                } else {
                    in_param = false;
                }
            } else if in_param {
                // Continuation line for multi-line @param
                param_text.push(' ');
                param_text.push_str(trimmed);
            }
        }

        if in_param
            && let Some((type_expr, _)) =
                Self::extract_jsdoc_param_type_expr_from_param_tag(&param_text, param_name)
        {
            return Some(type_expr);
        }

        None
    }

    /// Resolve the type from a JSDoc `@param {Type} name` annotation for a specific parameter.
    ///
    /// Extracts the type expression string from the `@param` tag matching `param_name`,
    /// then resolves it to a `TypeId` using the JSDoc type expression parser.
    ///
    /// Handles JSDoc optional parameter syntax:
    pub(crate) fn resolve_jsdoc_param_type_with_pos(
        &mut self,
        jsdoc: &str,
        param_name: &str,
        jsdoc_comment_start: Option<u32>,
    ) -> Option<tsz_solver::TypeId> {
        let (type_expr, type_expr_offset) =
            Self::extract_jsdoc_param_type_expr_with_span(jsdoc, param_name)?;
        // Handle {Type=} suffix which means optional (Type | undefined)
        let is_optional_type = type_expr.ends_with('=');
        let effective_type_expr = if is_optional_type {
            let mut expr = type_expr;
            expr.pop();
            expr
        } else {
            type_expr
        };
        // Handle {...Type} rest parameter prefix
        let is_rest = effective_type_expr.starts_with("...");
        let effective_type_expr = if is_rest {
            effective_type_expr[3..].to_string()
        } else {
            effective_type_expr
        };

        // Generic JSDoc type references like {C} should emit TS2314 when C
        // requires type arguments and none were provided.
        let base_type_expr = effective_type_expr.as_str();
        if let Some(comment_start) = jsdoc_comment_start
            && let Some((display_name, required_count)) =
                self.required_generic_count_for_jsdoc_type_name(base_type_expr)
        {
            let diag_start = comment_start + type_expr_offset as u32 + 4;
            self.error_generic_type_requires_type_arguments_at_span(
                &display_name,
                required_count,
                diag_start,
                base_type_expr.len() as u32,
            );
            return Some(tsz_solver::TypeId::ERROR);
        }

        let mut base_type = self.resolve_jsdoc_type_str(&effective_type_expr)?;

        // Handle JSDoc destructured parameter type literals.
        // When the base type is Object/object (possibly with []), nested @param tags
        // like `@param {string} opts.x` define the actual object shape.
        let trimmed_expr = effective_type_expr.trim();
        let is_object_base = trimmed_expr == "Object" || trimmed_expr == "object";
        let is_array_object_base = trimmed_expr == "Object[]"
            || trimmed_expr == "object[]"
            || trimmed_expr == "Array.<Object>"
            || trimmed_expr == "Array.<object>"
            || trimmed_expr == "Array<Object>"
            || trimmed_expr == "Array<object>";

        if (is_object_base || is_array_object_base)
            && let Some(built) =
                self.build_nested_param_object_type(jsdoc, param_name, is_array_object_base)
        {
            base_type = built;
        }

        // For rest params ({...Type}), wrap in array
        if is_rest {
            base_type = self.ctx.types.factory().array(base_type);
        }

        // Check if parameter is optional via bracket syntax [name] or [name=default]
        let is_optional_name = Self::is_jsdoc_param_optional_by_brackets(jsdoc, param_name);
        if (is_optional_type || is_optional_name)
            && self.ctx.strict_null_checks()
            && base_type != tsz_solver::TypeId::ANY
            && base_type != tsz_solver::TypeId::UNDEFINED
        {
            Some(
                self.ctx
                    .types
                    .factory()
                    .union2(base_type, tsz_solver::TypeId::UNDEFINED),
            )
        } else {
            Some(base_type)
        }
    }

    /// Check if a JSDoc `@param` tag has a rest type prefix (`{...Type}`).
    pub(crate) fn jsdoc_param_is_rest(jsdoc: &str, param_name: &str) -> bool {
        Self::extract_jsdoc_param_type_expr_with_span(jsdoc, param_name)
            .is_some_and(|(expr, _)| expr.starts_with("..."))
    }

    fn required_generic_count_for_jsdoc_type_name(
        &mut self,
        type_expr: &str,
    ) -> Option<(String, usize)> {
        use tsz_binder::symbol_flags;

        if !Self::is_plain_jsdoc_type_name(type_expr) {
            return None;
        }
        if self
            .resolve_jsdoc_implicit_any_builtin_type(type_expr)
            .is_some()
        {
            return None;
        }

        let sym_id = self.ctx.binder.file_locals.get(type_expr)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags
            & (symbol_flags::TYPE_ALIAS
                | symbol_flags::CLASS
                | symbol_flags::INTERFACE
                | symbol_flags::ENUM)
            == 0
        {
            return None;
        }

        let type_params = self.get_type_params_for_symbol(sym_id);
        let required_count = type_params.iter().filter(|p| p.default.is_none()).count();
        if required_count == 0 {
            return None;
        }

        Some((
            Self::format_generic_display_name_with_interner(
                type_expr,
                &type_params,
                self.ctx.types,
            ),
            required_count,
        ))
    }

    fn is_plain_jsdoc_type_name(name: &str) -> bool {
        let mut chars = name.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        if !(first == '$' || first == '_' || first.is_ascii_alphabetic()) {
            return false;
        }
        chars.all(|ch| ch == '$' || ch == '_' || ch.is_ascii_alphanumeric())
    }

    /// Extract nested `@param` properties for a destructured parameter.
    ///
    /// Given a parent parameter name like `opts`, extracts entries like:
    /// - `@param {string} opts.x` → ("x", "string", false)
    /// - `@param {string=} opts.y` → ("y", "string", true)  (= suffix)
    /// - `@param {string} [opts.z]` → ("z", "string", true)  (bracket syntax)
    /// - `@param {string} [opts.w="hi"]` → ("w", "string", true) (bracket + default)
    ///
    /// Build an object type from nested `@param` properties, handling arbitrary nesting depth.
    ///
    /// For `@param {object} opts` with nested `@param {string} opts.x` and
    /// `@param {object} opts.nested` with `@param {number} opts.nested.y`,
    /// this builds `{ x: string; nested: { y: number } }`.
    ///
    /// When `is_array` is true, wraps the result in an array type.
    fn build_nested_param_object_type(
        &mut self,
        jsdoc: &str,
        parent_name: &str,
        is_array: bool,
    ) -> Option<tsz_solver::TypeId> {
        let entries = Self::collect_jsdoc_nested_param_entries(jsdoc);
        self.build_nested_param_object_type_from_entries(&entries, parent_name, is_array)
    }

    pub(crate) fn build_nested_param_object_type_from_entries(
        &mut self,
        entries: &[(String, String, bool)],
        parent_name: &str,
        is_array: bool,
    ) -> Option<tsz_solver::TypeId> {
        let nested = Self::extract_jsdoc_nested_param_properties_from_entries(entries, parent_name);
        if nested.is_empty() {
            return None;
        }
        let mut properties = Vec::new();
        for (prop_name, prop_type_expr, is_prop_optional) in &nested {
            let (eff_type, opt_from_type) = if prop_type_expr.ends_with('=') {
                (&prop_type_expr[..prop_type_expr.len() - 1], true)
            } else {
                (prop_type_expr.as_str(), false)
            };

            // Check if this property itself is an object/Object with sub-properties
            let eff_trimmed = eff_type.trim();
            let is_sub_object = eff_trimmed == "Object" || eff_trimmed == "object";
            let is_sub_array_object = eff_trimmed == "Object[]"
                || eff_trimmed == "object[]"
                || eff_trimmed == "Array.<Object>"
                || eff_trimmed == "Array.<object>"
                || eff_trimmed == "Array<Object>"
                || eff_trimmed == "Array<object>";

            let prop_type_id = if is_sub_object || is_sub_array_object {
                // Build the full dotted parent name for recursive lookup
                let sub_parent = if is_array {
                    format!("{parent_name}[].{prop_name}")
                } else {
                    format!("{parent_name}.{prop_name}")
                };
                // Recursively build the nested object type
                self.build_nested_param_object_type_from_entries(
                    entries,
                    &sub_parent,
                    is_sub_array_object,
                )
                .or_else(|| self.jsdoc_type_from_expression(eff_type))
            } else {
                self.jsdoc_type_from_expression(eff_type)
            };

            if let Some(mut prop_type_id) = prop_type_id {
                let is_optional = *is_prop_optional || opt_from_type;
                if is_optional
                    && self.ctx.strict_null_checks()
                    && prop_type_id != tsz_solver::TypeId::ANY
                    && prop_type_id != tsz_solver::TypeId::UNDEFINED
                {
                    prop_type_id = self
                        .ctx
                        .types
                        .factory()
                        .union2(prop_type_id, tsz_solver::TypeId::UNDEFINED);
                }
                let name_atom = self.ctx.types.intern_string(prop_name);
                properties.push(tsz_solver::PropertyInfo {
                    name: name_atom,
                    type_id: prop_type_id,
                    write_type: prop_type_id,
                    optional: is_optional,
                    readonly: false,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: tsz_solver::Visibility::Public,
                    parent_id: None,
                    declaration_order: (properties.len() + 1) as u32,
                    is_string_named: false,
                });
            }
        }
        if properties.is_empty() {
            return None;
        }
        let obj_type = self.ctx.types.factory().object(properties);
        if is_array {
            Some(self.ctx.types.factory().array(obj_type))
        } else {
            Some(obj_type)
        }
    }

    /// - `@param {string} opts[].x` → ("x", "string", false) (array element property)
    ///
    /// Only extracts immediate child properties (one level of nesting).
    #[cfg(test)]
    fn extract_jsdoc_nested_param_properties(
        jsdoc: &str,
        parent_name: &str,
    ) -> Vec<(String, String, bool)> {
        let entries = Self::collect_jsdoc_nested_param_entries(jsdoc);
        Self::extract_jsdoc_nested_param_properties_from_entries(&entries, parent_name)
    }

    fn collect_jsdoc_nested_param_entries(jsdoc: &str) -> Vec<(String, String, bool)> {
        let mut result = Vec::new();

        for line in jsdoc.lines() {
            let trimmed = line.trim();
            let effective = Self::skip_backtick_quoted(trimmed);

            let Some(rest) = effective.strip_prefix("@param") else {
                continue;
            };
            let rest = rest.trim();

            // Parse {type} name pattern
            if !rest.starts_with('{') {
                continue;
            }
            let Some((type_expr, after_type)) = Self::parse_jsdoc_curly_type_expr(rest) else {
                continue;
            };
            let name_part = after_type.split_whitespace().next().unwrap_or("");

            // Check for bracket syntax [opts.x] or [opts.x=default]
            let (bare_name, is_bracket_optional) = if name_part.starts_with('[') {
                let inner = name_part.trim_start_matches('[');
                let bare = inner.split('=').next().unwrap_or(inner);
                let bare = bare.trim_end_matches(']');
                (bare, true)
            } else {
                (name_part, false)
            };

            if !bare_name.contains('.') && !bare_name.contains("[]") {
                continue;
            }

            result.push((
                bare_name.to_string(),
                type_expr.trim().to_string(),
                is_bracket_optional,
            ));
        }
        result
    }

    fn extract_jsdoc_nested_param_properties_from_entries(
        entries: &[(String, String, bool)],
        parent_name: &str,
    ) -> Vec<(String, String, bool)> {
        let mut result = Vec::new();
        let dot_prefix = format!("{parent_name}.");
        let array_dot_prefix = format!("{parent_name}[].");

        for (full_name, type_expr, is_bracket_optional) in entries {
            let prop_name = if let Some(prop) = full_name.strip_prefix(&dot_prefix) {
                if prop.contains('.') || prop.contains("[]") {
                    continue;
                }
                prop
            } else if let Some(prop) = full_name.strip_prefix(&array_dot_prefix) {
                if prop.contains('.') || prop.contains("[]") {
                    continue;
                }
                prop
            } else {
                continue;
            };

            if prop_name.is_empty() {
                continue;
            }

            result.push((
                prop_name.to_string(),
                type_expr.clone(),
                *is_bracket_optional,
            ));
        }

        result
    }

    /// Check if a JSDoc `@param` uses bracket syntax indicating optionality.
    ///
    /// Returns `true` for `@param {Type} [name]` or `@param {Type} [name=default]`.
    pub(crate) fn is_jsdoc_param_optional_by_brackets(jsdoc: &str, param_name: &str) -> bool {
        for line in jsdoc.lines() {
            let trimmed = line.trim();
            let effective = Self::skip_backtick_quoted(trimmed);
            if let Some(rest) = effective.strip_prefix("@param") {
                let rest = rest.trim();
                // Check the name part after optional {type}
                let name_part_str = if rest.starts_with('{') {
                    // @param {type} [name] or @param {type} [name=default]
                    if let Some((_type_expr, after_type)) = Self::parse_jsdoc_curly_type_expr(rest)
                    {
                        after_type.split_whitespace().next().unwrap_or("")
                    } else {
                        continue;
                    }
                } else {
                    // @param [name] — no type, just bracket-optional name
                    rest.split_whitespace().next().unwrap_or("")
                };
                if name_part_str.starts_with('[') {
                    // Extract the bare name from [name] or [name=default]
                    let inner = name_part_str.trim_start_matches('[');
                    let bare = inner.split('=').next().unwrap_or(inner);
                    let bare = bare.trim_end_matches(']');
                    if bare == param_name {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Extract all `@param` tag names from a JSDoc comment.
    ///
    /// Returns a list of `(name, byte_offset)` pairs where `byte_offset` is the
    /// offset of the `@param` tag within the JSDoc text (used for error positioning).
    /// Handles `@param {type} name`, `@param name {type}`, and nested/dotted names
    /// like `opts.x` (only returns the top-level portion before the dot).
    pub(crate) fn extract_jsdoc_param_names(jsdoc: &str) -> Vec<(String, usize)> {
        let mut result = Vec::new();
        let mut in_param = false;
        let mut param_text = String::new();
        let mut param_offset = 0usize;

        for line in jsdoc.lines() {
            let trimmed = line.trim();
            let effective = Self::skip_backtick_quoted(trimmed);

            if effective.starts_with('@') {
                if in_param {
                    if let Some(name) = Self::extract_param_name_from_tag(&param_text) {
                        result.push((name, param_offset));
                    }
                    param_text.clear();
                }
                if let Some(rest) = effective.strip_prefix("@param") {
                    in_param = true;
                    // Calculate offset of this @param in the original JSDoc string
                    // Find this line in the original to get byte offset
                    if let Some(line_start) = jsdoc.find(line)
                        && let Some(tag_pos) = line[..].find("@param")
                    {
                        param_offset = line_start + tag_pos;
                    }
                    param_text = rest.to_string();
                } else {
                    in_param = false;
                }
            } else if in_param {
                param_text.push(' ');
                param_text.push_str(trimmed);
            }
        }
        // Process the last @param if any
        if in_param && let Some(name) = Self::extract_param_name_from_tag(&param_text) {
            result.push((name, param_offset));
        }
        result
    }

    /// Extract the parameter name from a `@param` tag body (the text after `@param`).
    ///
    /// Handles:
    /// - `{type} name` → "name"
    /// - `{type} name description` → "name"
    /// - `{type} [name]` → "name"
    /// - `{type} [name=default]` → "name"
    /// - `{type} opts.x` → "opts" (nested/dotted → top-level only, skipped)
    /// - `{type} opts[].x` → "opts" (array dotted → skipped)
    /// - `name {type}` → "name"
    fn extract_param_name_from_tag(tag_body: &str) -> Option<String> {
        let parsed = Self::parse_jsdoc_param_tag(tag_body)?;
        if parsed.name.contains('.') || parsed.name.contains("[]") {
            return None;
        }
        let decoded = Self::decode_unicode_escapes(&parsed.name);
        if decoded.is_empty() {
            return Some(String::new()); // Empty name — still a @param tag
        }
        Some(decoded)
    }

    pub(crate) fn parse_jsdoc_param_tag(tag_body: &str) -> Option<JsdocParamTagInfo> {
        let rest = tag_body.trim();
        if rest.is_empty() {
            return None;
        }

        let (type_expr, name_token) = if rest.starts_with('{') {
            let (expr, after_type) = Self::parse_jsdoc_curly_type_expr(rest)?;
            (
                Some(expr.trim().to_string()),
                after_type.split_whitespace().next().unwrap_or(""),
            )
        } else {
            let first = rest.split_whitespace().next().unwrap_or("");
            let inline_type = rest.find('{').and_then(|idx| {
                Self::parse_jsdoc_curly_type_expr(&rest[idx..])
                    .map(|(expr, _)| expr.trim().to_string())
            });
            (inline_type, first)
        };

        let bracket_optional = name_token.starts_with('[');
        let mut name = name_token.trim_start_matches('[');
        name = name.split('=').next().unwrap_or(name);
        name = name.trim_end_matches(']');
        name = name.trim_matches('`');
        if name == "*" {
            if rest.starts_with('{') {
                return Some(JsdocParamTagInfo {
                    name: String::new(),
                    type_expr,
                    optional: false,
                    rest: false,
                });
            }
            return None;
        }
        let name = Self::decode_unicode_escapes(name.trim_start_matches("..."));
        if name.is_empty() {
            return None;
        }

        let type_optional = type_expr
            .as_deref()
            .is_some_and(|expr| expr.trim_end().ends_with('='));
        let rest = type_expr
            .as_deref()
            .is_some_and(|expr| expr.trim_start().starts_with("..."));

        Some(JsdocParamTagInfo {
            name,
            type_expr,
            optional: bracket_optional || type_optional,
            rest,
        })
    }

    /// Skip leading JSDoc decoration and backtick-quoted sections in a `JSDoc` line.
    ///
    /// Lines like `* @param {string} z` or `` `@param` @param {string} z ``
    /// contain comment decoration or backtick-quoted text before the real
    /// `@param` tag. This function strips those leading sections so the real
    /// tag can be detected.
    pub(crate) fn skip_backtick_quoted(s: &str) -> &str {
        let mut rest = s;
        loop {
            rest = rest.trim_start();
            if let Some(after_star) = rest.strip_prefix('*') {
                let is_jsdoc_decoration = after_star.is_empty()
                    || after_star
                        .chars()
                        .next()
                        .is_some_and(|ch| ch.is_whitespace() || ch == '@');
                if is_jsdoc_decoration {
                    rest = after_star;
                    continue;
                }
            }
            if rest.starts_with('`') {
                // Find matching closing backtick
                if let Some(end) = rest[1..].find('`') {
                    rest = &rest[end + 2..];
                    continue;
                }
            }
            break;
        }
        rest
    }

    /// Like `extract_jsdoc_param_type_expr_from_param_tag`, but returns the matching type expression
    /// and its byte offset within a full JSDoc block.
    fn extract_jsdoc_param_type_expr_with_span(
        jsdoc: &str,
        param_name: &str,
    ) -> Option<(String, usize)> {
        let mut in_param = false;
        let mut param_text = String::new();
        let mut text_offset = 0usize;
        let mut line_start = 0usize;

        for chunk in jsdoc.split_inclusive('\n') {
            let raw_line = chunk.trim_end_matches('\n').trim_end_matches('\r');
            let trimmed = raw_line.trim();
            let effective = Self::skip_backtick_quoted(trimmed);

            if effective.starts_with('@') {
                if in_param {
                    if let Some((expr, local_offset)) =
                        Self::extract_jsdoc_param_type_expr_from_param_tag(&param_text, param_name)
                    {
                        return Some((expr, text_offset + local_offset));
                    }
                    param_text.clear();
                }
                if let Some(rest) = effective.strip_prefix("@param") {
                    in_param = true;
                    param_text = rest.to_string();
                    let at_pos = raw_line
                        .find("@param")
                        .unwrap_or_else(|| effective.find("@param").unwrap_or(0));
                    text_offset = line_start + at_pos + "@param".len();
                } else {
                    in_param = false;
                }
            } else if in_param {
                param_text.push(' ');
                param_text.push_str(trimmed);
            }

            line_start += chunk.len();
        }
        if in_param
            && let Some((expr, local_offset)) =
                Self::extract_jsdoc_param_type_expr_from_param_tag(&param_text, param_name)
        {
            return Some((expr, text_offset + local_offset));
        }
        None
    }

    /// Extract a @param type expression (inside {}) matching a parameter name,
    /// returning the expression and its byte offset within the JSDoc tag body.
    fn extract_jsdoc_param_type_expr_from_param_tag(
        text: &str,
        param_name: &str,
    ) -> Option<(String, usize)> {
        let rest = text.trim();
        if rest.is_empty() {
            return None;
        }
        let text_ptr = text.as_ptr() as usize;
        let rest_ptr = rest.as_ptr() as usize;
        let rest_offset = rest_ptr.saturating_sub(text_ptr);

        // Handle alternate syntax: @param `name` {type} or @param name {type}
        if !rest.starts_with('{') {
            let name_part = rest.split_whitespace().next().unwrap_or("");
            let name_part_stripped = name_part.trim_matches('`');
            let decoded = Self::decode_unicode_escapes(name_part_stripped);
            if decoded == param_name {
                let after_name = rest[name_part.len()..].trim();
                if let Some((type_expr, _)) = Self::parse_jsdoc_curly_type_expr(after_name) {
                    let type_expr = type_expr.trim();
                    let type_expr_start_offset = type_expr.len() - type_expr.trim_start().len();
                    let type_expr_ptr = type_expr.as_ptr() as usize;
                    let offset = if type_expr.is_empty() {
                        0
                    } else {
                        let raw_offset = type_expr_ptr.saturating_sub(rest_ptr);
                        raw_offset + type_expr_start_offset + rest_offset
                    };
                    return Some((type_expr.to_string(), offset));
                }
            }
            return None;
        }

        // Standard syntax: @param {type} name
        if let Some((type_expr, after_type)) = Self::parse_jsdoc_curly_type_expr(rest) {
            let name = after_type.split_whitespace().next().unwrap_or("");
            let name = name.trim_start_matches('[');
            let name = name.split('=').next().unwrap_or(name);
            let name = name.trim_end_matches(']');
            let name = name.trim_matches('`');
            let decoded = Self::decode_unicode_escapes(name);
            if decoded == param_name {
                let type_expr = type_expr.trim();
                let type_expr_start_offset = type_expr.len() - type_expr.trim_start().len();
                let type_expr_ptr = type_expr.as_ptr() as usize;
                let offset = if type_expr.is_empty() {
                    0
                } else {
                    let raw_offset = type_expr_ptr.saturating_sub(rest_ptr);
                    raw_offset + type_expr_start_offset + rest_offset
                };
                return Some((type_expr.to_string(), offset));
            }
        }
        None
    }

    /// Decode unicode escapes (`\uXXXX` and `\u{XXXX}`) in a string.
    fn decode_unicode_escapes(s: &str) -> String {
        if !s.contains("\\u") {
            return s.to_string();
        }
        let mut result = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\\' && chars.peek() == Some(&'u') {
                chars.next(); // consume 'u'
                if chars.peek() == Some(&'{') {
                    // \u{XXXX} form
                    chars.next(); // consume '{'
                    let mut hex = String::new();
                    while let Some(&c) = chars.peek() {
                        if c == '}' {
                            chars.next();
                            break;
                        }
                        hex.push(c);
                        chars.next();
                    }
                    if let Ok(code) = u32::from_str_radix(&hex, 16)
                        && let Some(decoded) = char::from_u32(code)
                    {
                        result.push(decoded);
                        continue;
                    }
                    // Fallback: push original
                    result.push_str("\\u{");
                    result.push_str(&hex);
                    result.push('}');
                } else {
                    // \uXXXX form (exactly 4 hex digits)
                    let mut hex = String::new();
                    for _ in 0..4 {
                        if let Some(&c) = chars.peek() {
                            if c.is_ascii_hexdigit() {
                                hex.push(c);
                                chars.next();
                            } else {
                                break;
                            }
                        }
                    }
                    if hex.len() == 4
                        && let Ok(code) = u32::from_str_radix(&hex, 16)
                        && let Some(decoded) = char::from_u32(code)
                    {
                        result.push(decoded);
                        continue;
                    }
                    // Fallback: push original
                    result.push_str("\\u");
                    result.push_str(&hex);
                }
            } else {
                result.push(ch);
            }
        }
        result
    }

    /// Check if a `JSDoc` comment has any type annotations (`@param {type}`, `@returns {type}`,
    /// `@type {type}`, or `@template`).
    ///
    /// In tsc, when a function has `JSDoc` type annotations, implicit any errors (TS7010/TS7011)
    /// are suppressed even without explicit `@returns`, because the developer is providing
    /// type information through `JSDoc`.
    pub(crate) fn jsdoc_has_type_annotations(jsdoc: &str) -> bool {
        for line in jsdoc.lines() {
            let trimmed = line.trim();
            // @param {type} name
            if let Some(rest) = trimmed.strip_prefix("@param")
                && rest.trim().starts_with('{')
            {
                return true;
            }
            // @returns {type} or @return {type}
            if let Some(rest) = trimmed
                .strip_prefix("@returns")
                .or_else(|| trimmed.strip_prefix("@return"))
                && rest.trim().starts_with('{')
            {
                return true;
            }
            // @type {type}
            if let Some(rest) = trimmed.strip_prefix("@type")
                && rest.trim().starts_with('{')
            {
                return true;
            }
            // @template T
            if trimmed.starts_with("@template") {
                return true;
            }
        }
        false
    }

    pub(crate) fn jsdoc_type_tag_declares_callable(jsdoc: &str) -> bool {
        let Some(expr) = Self::jsdoc_extract_type_tag_expr_braceless(jsdoc) else {
            return false;
        };
        let expr = expr.trim();
        if expr.eq_ignore_ascii_case("function") || expr.eq_ignore_ascii_case("Function") {
            return false;
        }
        expr.contains("=>")
            || expr
                .strip_prefix("function")
                .is_some_and(|rest| rest.trim_start().starts_with('('))
    }

    pub(crate) fn jsdoc_type_tag_is_broad_function(jsdoc: &str) -> bool {
        let Some(expr) = Self::jsdoc_extract_type_tag_expr_braceless(jsdoc) else {
            return false;
        };
        let expr = expr.trim();
        expr.eq_ignore_ascii_case("function") || expr.eq_ignore_ascii_case("Function")
    }

    pub(crate) fn jsdoc_type_tag_function_missing_return(jsdoc: &str) -> bool {
        let Some(expr) = Self::jsdoc_extract_type_tag_expr_braceless(jsdoc) else {
            return false;
        };
        let expr = expr.trim();
        let Some(rest) = expr.strip_prefix("function") else {
            return false;
        };
        let rest = rest.trim_start();
        if !rest.starts_with('(') {
            return false;
        }
        let rest = &rest[1..];
        // Closure-style constructor types like `function(new: object, ...)` have
        // an implied return type (the type after `new:`).  They never need a
        // separate `:returnType` suffix, so they should not trigger TS7014.
        if rest.trim_start().starts_with("new:") || rest.trim_start().starts_with("new :") {
            return false;
        }
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
        let Some(close_idx) = close_idx else {
            return false;
        };
        !rest[close_idx + 1..].trim_start().starts_with(':')
    }

    pub(crate) fn jsdoc_type_tag_function_keyword_pos_in_source(
        source_text: &str,
        comment_pos: u32,
    ) -> Option<u32> {
        let comment_start = comment_pos as usize;
        let comment_text = &source_text[comment_start..];
        let comment_end = comment_text.find("*/")?;
        let comment_text = &comment_text[..comment_end];
        let tag_pos = comment_text.find("@type")?;
        let rest = &comment_text[tag_pos + "@type".len()..];
        let fn_rel = rest.find("function")?;
        Some(comment_pos + (tag_pos + "@type".len() + fn_rel) as u32)
    }

    pub(crate) fn jsdoc_extract_type_tag_expr_braceless(jsdoc: &str) -> Option<String> {
        for raw_line in jsdoc.lines() {
            let trimmed = raw_line.trim().trim_start_matches('*').trim();
            if let Some(rest) = trimmed.strip_prefix("@type") {
                let rest = rest.trim();
                if rest.starts_with('{')
                    && let Some(end) = rest[1..].find('}')
                {
                    return Some(rest[1..1 + end].trim().to_string());
                }
                if !rest.is_empty() && !rest.starts_with('@') {
                    return Some(rest.to_string());
                }
            }
        }
        None
    }

    /// Extract the type expression from a `@type {X}` JSDoc tag.
    /// Returns the inner type expression string (e.g., "Cb" from `@type {Cb}`).
    pub(crate) fn jsdoc_extract_type_tag_expr(jsdoc: &str) -> Option<String> {
        for raw_line in jsdoc.lines() {
            let trimmed = raw_line.trim().trim_start_matches('*').trim();
            if let Some(rest) = trimmed.strip_prefix("@type") {
                let rest = rest.trim();
                if rest.starts_with('{')
                    && let Some(end) = rest[1..].find('}')
                {
                    return Some(rest[1..1 + end].trim().to_string());
                }
            }
        }
        None
    }

    pub(crate) fn jsdoc_type_tag_expr_span_for_node_direct(
        &self,
        idx: NodeIndex,
    ) -> Option<(u32, u32)> {
        let sf = self.source_file_data_for_node(idx)?;
        let source_text = sf.text.to_string();
        let comments = sf.comments.clone();
        let pos = self.effective_jsdoc_pos_for_node(idx, &comments, &source_text)?;
        let (jsdoc, comment_pos) = self.try_leading_jsdoc_with_pos(&comments, pos, &source_text)?;
        let type_expr = Self::extract_jsdoc_type_expression(&jsdoc)?;
        let comment = comments.iter().find(|comment| comment.pos == comment_pos)?;
        let raw_comment = comment.get_text(&source_text);
        let type_tag_offset = raw_comment.find("@type")?;
        let after_tag = &raw_comment[type_tag_offset + "@type".len()..];
        let open_brace_offset = after_tag.find('{')?;
        let after_open_brace = &after_tag[open_brace_offset + 1..];
        let trimmed = after_open_brace.trim_start();
        let leading_ws = after_open_brace.len().saturating_sub(trimmed.len());
        let expr_start = comment_pos
            + (type_tag_offset + "@type".len() + open_brace_offset + 1 + leading_ws) as u32;
        Some((expr_start, type_expr.len() as u32))
    }

    /// Check if a JSDoc type expression is syntactically a callable/function type.
    /// Returns true for arrow types (`(x: T) => R`), function types (`function(x): R`),
    /// and generic signatures (`<T>(x: T) => R`).
    pub(crate) fn is_syntactically_callable_type(type_expr: &str) -> bool {
        let trimmed = type_expr.trim();
        // Arrow function type: contains `=>`
        if trimmed.contains("=>") {
            return true;
        }
        // function(...): ... type
        if trimmed.starts_with("function") {
            return true;
        }
        // Generic signature: <T>(...) => ...
        if trimmed.starts_with('<') {
            return true;
        }
        // Parenthesized callable: (x: number) => void
        if trimmed.starts_with('(') {
            return true;
        }
        false
    }

    /// Extract a type predicate from a `@type {CallbackType}` JSDoc annotation.
    /// Resolves the referenced type and checks both Function and Callable shapes.
    pub(crate) fn extract_type_predicate_from_jsdoc_type_tag(
        &mut self,
        jsdoc: &str,
    ) -> Option<tsz_solver::TypePredicate> {
        let type_expr = Self::jsdoc_extract_type_tag_expr(jsdoc)?;
        let resolved = self.resolve_jsdoc_type_str(&type_expr)?;
        if let Some(shape) = tsz_solver::type_queries::get_function_shape(self.ctx.types, resolved)
        {
            return shape.type_predicate;
        }
        if let Some(sigs) = tsz_solver::type_queries::get_call_signatures(self.ctx.types, resolved)
            && let Some(sig) = sigs.first()
        {
            return sig.type_predicate;
        }
        None
    }

    /// Extract `@template` type parameter names from a `JSDoc` comment.
    ///
    /// Supports simple forms like:
    /// - `@template T`
    /// - `@template T,U`
    /// - `@template T U`
    pub(crate) fn jsdoc_template_type_params(jsdoc: &str) -> Vec<String> {
        let mut out = Vec::new();
        for line in jsdoc.lines() {
            let trimmed = line.trim().trim_start_matches('*').trim();
            let Some(rest) = trimmed.strip_prefix("@template") else {
                continue;
            };
            for token in rest.split([',', ' ', '\t']) {
                let name = token.trim();
                if name.is_empty() {
                    continue;
                }
                if name
                    .chars()
                    .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
                    && !out.iter().any(|existing| existing == name)
                {
                    out.push(name.to_string());
                }
            }
        }
        out
    }

    /// Emit JSDoc `@template` syntax diagnostics for invalid brace forms like
    /// `@template {T}`. tsc reports both TS1069 at `{` and TS2304 at `T`.
    pub(crate) fn validate_jsdoc_template_tag_syntax_at_decl(&mut self, decl_idx: NodeIndex) {
        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return;
        };
        let Some((_, comment_pos)) =
            self.try_leading_jsdoc_with_pos(comments, node.pos, source_text)
        else {
            return;
        };
        let comment_end = node.pos.min(source_text.len() as u32);
        let comment_range = &source_text[comment_pos as usize..comment_end as usize];

        let mut scan_start = 0usize;
        while let Some(template_offset) = comment_range[scan_start..].find("@template") {
            let template_start = scan_start + template_offset;
            let rest = &comment_range[template_start + "@template".len()..];
            let trimmed = rest.trim_start();
            if !trimmed.starts_with('{') {
                scan_start = template_start + "@template".len();
                continue;
            }

            let leading_ws = rest.len() - trimmed.len();
            let brace_rel = template_start + "@template".len() + leading_ws;
            let after_brace = &trimmed[1..];
            let name_len = after_brace
                .chars()
                .take_while(|ch| *ch == '_' || *ch == '$' || ch.is_ascii_alphanumeric())
                .count();
            let error_rel = brace_rel
                + 1
                + name_len
                + usize::from(
                    after_brace
                        .get(name_len..)
                        .is_some_and(|rest| rest.starts_with('}')),
                );
            let brace_pos = comment_pos + error_rel as u32;
            self.ctx.error(
                brace_pos,
                1,
                crate::diagnostics::diagnostic_messages::UNEXPECTED_TOKEN_A_TYPE_PARAMETER_NAME_WAS_EXPECTED_WITHOUT_CURLY_BRACES.to_string(),
                crate::diagnostics::diagnostic_codes::UNEXPECTED_TOKEN_A_TYPE_PARAMETER_NAME_WAS_EXPECTED_WITHOUT_CURLY_BRACES,
            );

            if name_len > 0 {
                let name = &after_brace[..name_len];
                self.emit_jsdoc_cannot_find_name(name, comment_pos, comment_end, source_text);
            }

            scan_start = brace_rel + 1;
        }
    }

    /// Extract a simple identifier from `@returns {T}` / `@return {T}`.
    ///
    /// Returns `None` for complex type expressions.
    pub(crate) fn jsdoc_returns_type_name(jsdoc: &str) -> Option<String> {
        for line in jsdoc.lines() {
            let trimmed = line.trim().trim_start_matches('*').trim();
            let Some(rest) = trimmed
                .strip_prefix("@returns")
                .or_else(|| trimmed.strip_prefix("@return"))
            else {
                continue;
            };
            let rest = rest.trim_start();
            if !rest.starts_with('{') {
                continue;
            }
            let after_open = &rest[1..];
            let end = after_open.find('}')?;
            let type_expr = after_open[..end].trim();
            if !type_expr.is_empty()
                && type_expr
                    .chars()
                    .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
            {
                return Some(type_expr.to_string());
            }
        }
        None
    }

    /// Extract a type predicate from `@returns {x is Type}` / `@return {this is Entry}`.
    ///
    /// Returns `Some((is_asserts, param_name, type_str))` if the `@returns` tag
    /// contains a type predicate pattern like `{x is string}` or `{this is Entry}`.
    /// Also handles `{asserts x is Type}` and `{asserts x}` patterns.
    pub(crate) fn jsdoc_returns_type_predicate(
        jsdoc: &str,
    ) -> Option<(bool, String, Option<String>)> {
        for line in jsdoc.lines() {
            let trimmed = line.trim().trim_start_matches('*').trim();
            let Some(rest) = trimmed
                .strip_prefix("@returns")
                .or_else(|| trimmed.strip_prefix("@return"))
            else {
                continue;
            };
            let rest = rest.trim_start();
            if !rest.starts_with('{') {
                continue;
            }
            let after_open = &rest[1..];
            let end = after_open.find('}')?;
            let type_expr = after_open[..end].trim();

            // Check for "asserts" prefix
            let (is_asserts, remainder) =
                if let Some(after_asserts) = type_expr.strip_prefix("asserts ") {
                    (true, after_asserts.trim())
                } else {
                    (false, type_expr)
                };

            // Look for " is " separator (the type predicate pattern)
            if let Some(is_pos) = remainder.find(" is ") {
                let param_name = remainder[..is_pos].trim();
                let type_str = remainder[is_pos + 4..].trim();
                // Validate param_name is a simple identifier or "this"
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
                // "asserts x" without " is Type" — assertion without narrowing type
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
        }
        None
    }
}

#[cfg(test)]
#[path = "../types/utilities/tests/jsdoc_params_tests.rs"]
mod tests;
