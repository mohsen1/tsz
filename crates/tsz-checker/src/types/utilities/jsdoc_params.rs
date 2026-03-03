//! JSDoc param tag validation, JSDoc comment finding, and text parsing utilities
//! for `CheckerState`.
//!
//! Extracted from `jsdoc.rs` — contains:
//! - TS8024 `@param` tag name checking
//! - JSDoc comment position/content lookup
//! - Pure text-level JSDoc parsing helpers (param names, type expressions, etc.)

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    // =========================================================================
    // JSDoc Helpers for Implicit Any Suppression
    // =========================================================================

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
        for (param_name, _tag_offset) in &jsdoc_params {
            // Skip empty names (malformed @param tags)
            if param_name.is_empty() {
                continue;
            }
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
                    let mut search_from = 0;
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
                        self.ctx.error(
                            pos as u32,
                            param_name.len() as u32,
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
        // Check if the next word is the name
        if let Some(after_name) = rest.strip_prefix(name) {
            // Verify it's a complete word (followed by non-alphanumeric or end)
            if after_name.is_empty() || !after_name.chars().next().unwrap().is_alphanumeric() {
                return Some(offset);
            }
        }
        None
    }

    /// Get the byte position of the JSDoc comment for a function node.
    ///
    /// Returns `Some(pos)` where pos is the byte offset of `/**` in the source.
    fn get_jsdoc_comment_pos_for_function(&self, func_idx: NodeIndex) -> Option<u32> {
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

        // Try leading comments, then walk up the parent chain for
        // `const f = value => ...` where JSDoc is on the `const` line.
        self.try_jsdoc_with_ancestor_walk(func_idx, comments, source_text)
    }

    /// Try to find a leading JSDoc comment for a node, walking up to 4 ancestors.
    ///
    /// First checks `idx` itself, then walks the parent chain up to 4 levels.
    /// Returns the first JSDoc content found, or `None`.
    pub(crate) fn try_jsdoc_with_ancestor_walk(
        &self,
        idx: NodeIndex,
        comments: &[tsz_common::comments::CommentRange],
        source_text: &str,
    ) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        let jsdoc = self.try_leading_jsdoc(comments, node.pos, source_text);
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
            let parent_node = self.ctx.arena.get(parent)?;
            let jsdoc = self.try_leading_jsdoc(comments, parent_node.pos, source_text);
            if jsdoc.is_some() {
                return jsdoc;
            }
            current = parent;
        }
        None
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

    /// Extract the type expression string from a `@param {type} name` JSDoc tag.
    ///
    /// Returns the type expression (e.g., "Object.<string, boolean>") for the given
    /// parameter name, or None if no matching `@param` tag is found.
    pub(crate) fn extract_jsdoc_param_type_string(jsdoc: &str, param_name: &str) -> Option<String> {
        // JSDoc @param may span multiple lines. Collect all text after each @param
        // and process them. We also need to handle nested braces in types like
        // @param {{ x: T, y: T}} obj
        let mut in_param = false;
        let mut param_text = String::new();

        for line in jsdoc.lines() {
            let trimmed = line.trim().trim_start_matches('*').trim();

            // Skip backtick-quoted sections to find real @-tags.
            // Lines like `` `@param` @param {string} z `` have a real @param after backticks.
            let effective = Self::skip_backtick_quoted(trimmed);

            // Check if this line starts a new @tag
            if effective.starts_with('@') {
                // Process any accumulated @param text
                if in_param {
                    if let Some(type_expr) =
                        Self::extract_jsdoc_param_type_expr(&param_text, param_name)
                    {
                        return Some(type_expr.to_string());
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
        // Process the last @param if any
        if in_param
            && let Some(type_expr) = Self::extract_jsdoc_param_type_expr(&param_text, param_name)
        {
            return Some(type_expr.to_string());
        }
        None
    }

    /// Resolve the type from a JSDoc `@param {Type} name` annotation for a specific parameter.
    ///
    /// Extracts the type expression string from the `@param` tag matching `param_name`,
    /// then resolves it to a `TypeId` using the JSDoc type expression parser.
    ///
    /// Handles JSDoc optional parameter syntax:
    /// - `@param {number} [p]` → `number | undefined`
    /// - `@param {number} [p=0]` → `number | undefined`
    /// - `@param {number=} p` → `number | undefined` (= suffix in type)
    ///
    /// Handles JSDoc destructured parameter type literals:
    /// - `@param {Object} opts` + `@param {string} opts.x` → `{ x: string }`
    /// - `@param {object[]} arr` + `@param {string} arr[].x` → `{ x: string }[]`
    ///
    /// Returns `None` if no matching `@param` tag exists or the type can't be resolved.
    pub(crate) fn resolve_jsdoc_param_type(
        &mut self,
        jsdoc: &str,
        param_name: &str,
    ) -> Option<tsz_solver::TypeId> {
        let type_expr = Self::extract_jsdoc_param_type_string(jsdoc, param_name)?;
        // Handle {Type=} suffix which means optional (Type | undefined)
        let (effective_type_expr, is_optional_type) = if type_expr.ends_with('=') {
            (type_expr[..type_expr.len() - 1].to_string(), true)
        } else {
            (type_expr, false)
        };
        let mut base_type = self.jsdoc_type_from_expression(&effective_type_expr)?;

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

        if is_object_base || is_array_object_base {
            let nested = Self::extract_jsdoc_nested_param_properties(jsdoc, param_name);
            if !nested.is_empty() {
                let mut properties = Vec::new();
                for (prop_name, prop_type_expr, is_prop_optional) in &nested {
                    // Handle {Type=} suffix in nested property types
                    let (eff_type, opt_from_type) = if prop_type_expr.ends_with('=') {
                        (&prop_type_expr[..prop_type_expr.len() - 1], true)
                    } else {
                        (prop_type_expr.as_str(), false)
                    };
                    let prop_type = self.jsdoc_type_from_expression(eff_type);
                    if let Some(mut prop_type_id) = prop_type {
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
                                .union(vec![prop_type_id, tsz_solver::TypeId::UNDEFINED]);
                        }
                        let name_atom = self.ctx.types.intern_string(prop_name);
                        properties.push(tsz_solver::PropertyInfo {
                            name: name_atom,
                            type_id: prop_type_id,
                            write_type: prop_type_id,
                            optional: is_optional,
                            readonly: false,
                            is_method: false,
                            visibility: tsz_solver::Visibility::Public,
                            parent_id: None,
                            declaration_order: properties.len() as u32,
                        });
                    }
                }
                if !properties.is_empty() {
                    let obj_type = self.ctx.types.factory().object(properties);
                    if is_array_object_base {
                        base_type = self.ctx.types.factory().array(obj_type);
                    } else {
                        base_type = obj_type;
                    }
                }
            }
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
                    .union(vec![base_type, tsz_solver::TypeId::UNDEFINED]),
            )
        } else {
            Some(base_type)
        }
    }

    /// Extract nested `@param` properties for a destructured parameter.
    ///
    /// Given a parent parameter name like `opts`, extracts entries like:
    /// - `@param {string} opts.x` → ("x", "string", false)
    /// - `@param {string=} opts.y` → ("y", "string", true)  (= suffix)
    /// - `@param {string} [opts.z]` → ("z", "string", true)  (bracket syntax)
    /// - `@param {string} [opts.w="hi"]` → ("w", "string", true) (bracket + default)
    /// - `@param {string} opts[].x` → ("x", "string", false) (array element property)
    ///
    /// Only extracts immediate child properties (one level of nesting).
    fn extract_jsdoc_nested_param_properties(
        jsdoc: &str,
        parent_name: &str,
    ) -> Vec<(String, String, bool)> {
        let mut result = Vec::new();
        let dot_prefix = format!("{parent_name}.");
        let array_dot_prefix = format!("{parent_name}[].");

        for line in jsdoc.lines() {
            let trimmed = line.trim().trim_start_matches('*').trim();
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

            // Check if this is a direct child property of the parent
            // e.g., "opts.x" for parent "opts", or "opts[].x" for array parent
            let prop_name = if let Some(prop) = bare_name.strip_prefix(&dot_prefix) {
                // Skip deeper nesting like opts.what.bad (contains another dot)
                if prop.contains('.') || prop.contains("[]") {
                    continue;
                }
                prop
            } else if let Some(prop) = bare_name.strip_prefix(&array_dot_prefix) {
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
                type_expr.trim().to_string(),
                is_bracket_optional,
            ));
        }
        result
    }

    /// Check if a JSDoc `@param` uses bracket syntax indicating optionality.
    ///
    /// Returns `true` for `@param {Type} [name]` or `@param {Type} [name=default]`.
    fn is_jsdoc_param_optional_by_brackets(jsdoc: &str, param_name: &str) -> bool {
        for line in jsdoc.lines() {
            let trimmed = line.trim().trim_start_matches('*').trim();
            let effective = Self::skip_backtick_quoted(trimmed);
            if let Some(rest) = effective.strip_prefix("@param") {
                let rest = rest.trim();
                // Standard: @param {type} [name] or @param {type} [name=default]
                if rest.starts_with('{')
                    && let Some((_type_expr, after_type)) = Self::parse_jsdoc_curly_type_expr(rest)
                {
                    let name_part = after_type.split_whitespace().next().unwrap_or("");
                    if name_part.starts_with('[') {
                        // Extract the bare name from [name] or [name=default]
                        let inner = name_part.trim_start_matches('[');
                        let bare = inner.split('=').next().unwrap_or(inner);
                        let bare = bare.trim_end_matches(']');
                        if bare == param_name {
                            return true;
                        }
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
            let trimmed = line.trim().trim_start_matches('*').trim();
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
        let rest = tag_body.trim();
        if rest.is_empty() {
            return None;
        }

        let name_str = if rest.starts_with('{') {
            // Standard syntax: {type} name
            let (_, after_type) = Self::parse_jsdoc_curly_type_expr(rest)?;
            after_type.split_whitespace().next().unwrap_or("")
        } else {
            // Alternate syntax: name {type} or just name
            rest.split_whitespace().next().unwrap_or("")
        };

        // Clean up the name: remove [], =default, backticks
        let mut name = name_str.trim_start_matches('[');
        name = name.split('=').next().unwrap_or(name);
        name = name.trim_end_matches(']');
        name = name.trim_matches('`');

        // Skip dotted names like opts.x or opts[].x — these are nested property
        // docs for destructured parameters, not standalone params
        if name.contains('.') || name.contains("[]") {
            return None;
        }

        // Skip rest parameter prefix
        let name = name.trim_start_matches("...");

        let decoded = Self::decode_unicode_escapes(name);
        if decoded.is_empty() {
            return Some(String::new()); // Empty name — still a @param tag
        }
        Some(decoded)
    }

    /// Skip leading backtick-quoted sections in a `JSDoc` line.
    ///
    /// Lines like `` `@param` @param {string} z `` contain backtick-quoted text
    /// before the real `@param` tag. This function strips those leading quoted
    /// sections so the real tag can be detected.
    fn skip_backtick_quoted(s: &str) -> &str {
        let mut rest = s;
        loop {
            rest = rest.trim_start();
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

    /// Helper to extract a @param type expression (inside {}) if it matches a parameter name.
    /// Handles nested braces in type expressions like `{{ x: T, y: T}}`.
    /// Also handles alternate `@param name {type}` syntax (name before type).
    fn extract_jsdoc_param_type_expr<'b>(text: &'b str, param_name: &str) -> Option<&'b str> {
        let rest = text.trim();

        // Handle alternate syntax: @param `name` {type} or @param name {type}
        if !rest.starts_with('{') {
            let name_part = rest.split_whitespace().next().unwrap_or("");
            let name_part_stripped = name_part.trim_matches('`');
            let decoded = Self::decode_unicode_escapes(name_part_stripped);
            if decoded == param_name {
                let after_name = rest[name_part.len()..].trim();
                if let Some((type_expr, _)) = Self::parse_jsdoc_curly_type_expr(after_name) {
                    return Some(type_expr.trim());
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
                return Some(type_expr.trim());
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

    /// Check if a `JSDoc` comment has a `@type {expr}` tag.
    ///
    /// When `@type` declares a full function type (e.g., `@type {function((string)): string}`),
    /// all parameters are typed and TS7006 should be suppressed.
    pub(crate) fn jsdoc_has_type_tag(jsdoc: &str) -> bool {
        for line in jsdoc.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("@type") {
                let rest = rest.trim();
                // Accept both braced `@type {T}` and braceless `@type T` forms.
                // The braceless form is used in tsc for inline function types like
                // `@type (arg: string) => string`.
                if rest.starts_with('{') || (!rest.is_empty() && !rest.starts_with('@')) {
                    return true;
                }
            }
        }
        false
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
mod tests {
    use super::*;

    #[test]
    fn extract_nested_params_basic_object() {
        let jsdoc = r#"
 * @param {Object} opts doc
 * @param {string} opts.x doc2
 * @param {number} opts.y doc3
        "#;
        let result = CheckerState::extract_jsdoc_nested_param_properties(jsdoc, "opts");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], ("x".to_string(), "string".to_string(), false));
        assert_eq!(result[1], ("y".to_string(), "number".to_string(), false));
    }

    #[test]
    fn extract_nested_params_optional_bracket() {
        let jsdoc = r#"
 * @param {Object} opts
 * @param {string} opts.x
 * @param {string=} opts.y
 * @param {string} [opts.z]
 * @param {string} [opts.w="hi"]
        "#;
        let result = CheckerState::extract_jsdoc_nested_param_properties(jsdoc, "opts");
        assert_eq!(result.len(), 4);
        assert_eq!(result[0].0, "x");
        assert!(!result[0].2); // not optional
        assert_eq!(result[1].0, "y");
        assert_eq!(result[1].1, "string="); // = suffix preserved for caller to handle
        assert!(!result[1].2);
        assert_eq!(result[2].0, "z");
        assert!(result[2].2); // bracket optional
        assert_eq!(result[3].0, "w");
        assert!(result[3].2); // bracket + default optional
    }

    #[test]
    fn extract_nested_params_array_element() {
        let jsdoc = r#"
 * @param {Object[]} opts2
 * @param {string} opts2[].anotherX
 * @param {string=} opts2[].anotherY
        "#;
        let result = CheckerState::extract_jsdoc_nested_param_properties(jsdoc, "opts2");
        assert_eq!(result.len(), 2);
        assert_eq!(
            result[0],
            ("anotherX".to_string(), "string".to_string(), false)
        );
        assert_eq!(
            result[1],
            ("anotherY".to_string(), "string=".to_string(), false)
        );
    }

    #[test]
    fn extract_nested_params_skips_deep_nesting() {
        let jsdoc = r#"
 * @param {object[]} opts5
 * @param {string} opts5[].help
 * @param {object} opts5[].what
 * @param {string} opts5[].what.a
 * @param {Object[]} opts5[].what.bad
 * @param {string} opts5[].what.bad[].idea
        "#;
        let result = CheckerState::extract_jsdoc_nested_param_properties(jsdoc, "opts5");
        // Only immediate children: help, what
        // Deeper nesting (what.a, what.bad, what.bad[].idea) should be skipped
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "help");
        assert_eq!(result[1].0, "what");
    }

    #[test]
    fn extract_nested_params_no_children() {
        let jsdoc = r#"
 * @param {string} name
 * @param {number} age
        "#;
        let result = CheckerState::extract_jsdoc_nested_param_properties(jsdoc, "name");
        assert!(result.is_empty());
    }

    #[test]
    fn extract_nested_params_wrong_parent() {
        let jsdoc = r#"
 * @param {Object} opts1
 * @param {string} opts1.x
 * @param {Object} opts2
 * @param {number} opts2.y
        "#;
        let result = CheckerState::extract_jsdoc_nested_param_properties(jsdoc, "opts1");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "x");
    }
}
