//! JSDoc `@typedef` validation helpers for `CheckerState`.
//!
//! Extracted from `jsdoc_params.rs` — contains:
//! - TS8033 duplicate `@type` in `@typedef` checking
//! - TS8021 missing type annotation in `@typedef` checking
//! - TS2304 base type validation for `@typedef` declarations
//! - TS1109 malformed `@import` tag detection

use crate::state::CheckerState;

// =============================================================================
// TS8033: Duplicate @type in @typedef
// =============================================================================

impl<'a> CheckerState<'a> {
    /// TS8033: Check all JSDoc comments for `@typedef` with multiple `@type` tags.
    ///
    /// A `@typedef` JSDoc comment should have at most one `@type` tag.
    /// If multiple `@type` tags are found, emit TS8033 at the second occurrence.
    pub(crate) fn check_typedef_duplicate_type_tags(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_common::comments::is_jsdoc_comment;

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;

        for comment in comments {
            if !is_jsdoc_comment(comment, source_text) {
                continue;
            }

            let comment_text = comment.get_text(source_text);

            // Check if this comment contains @typedef
            if !comment_text.contains("@typedef") {
                continue;
            }

            // Count @type tags (not @typedef or @typeParam etc.)
            let mut type_tag_count = 0u32;
            for (match_pos, _) in comment_text.match_indices("@type") {
                let after = match_pos + "@type".len();
                // Ensure @type is not a prefix of @typedef, @typeParam, etc.
                if after < comment_text.len() {
                    let next_ch = comment_text[after..].chars().next().unwrap_or('\0');
                    if next_ch.is_ascii_alphanumeric() || next_ch == 'P' {
                        // Likely @typedef or @typeParam — skip
                        continue;
                    }
                }
                type_tag_count += 1;
                if type_tag_count >= 2 {
                    // Emit TS8033 at this @type tag position
                    let error_pos = comment.pos + match_pos as u32;
                    let error_len = "@type".len() as u32;
                    self.ctx.error(
                        error_pos,
                        error_len,
                        diagnostic_messages::A_JSDOC_TYPEDEF_COMMENT_MAY_NOT_CONTAIN_MULTIPLE_TYPE_TAGS
                            .to_string(),
                        diagnostic_codes::A_JSDOC_TYPEDEF_COMMENT_MAY_NOT_CONTAIN_MULTIPLE_TYPE_TAGS,
                    );
                }
            }
        }
    }

    /// Check for `@typedef` tags that have neither a type annotation nor
    /// `@property`/`@member` tags. Emits TS8021.
    ///
    /// Valid: `/** @typedef {Object} Foo */` or `/** @typedef Foo \n @property {string} name */`
    /// Invalid: `/** @typedef T */` (no type, no properties)
    pub(crate) fn check_typedef_missing_type(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_common::comments::is_jsdoc_comment;

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;

        for comment in comments {
            if !is_jsdoc_comment(comment, source_text) {
                continue;
            }

            let comment_text = comment.get_text(source_text);

            // Find @typedef tag
            let Some(typedef_pos) = comment_text.find("@typedef") else {
                continue;
            };

            let after_typedef = typedef_pos + "@typedef".len();
            let rest = &comment_text[after_typedef..];

            // Check if there's a type annotation: @typedef {SomeType} Name
            let trimmed = rest.trim_start();
            let has_type = trimmed.starts_with('{');

            // Check if there are @property, @member, or @type tags
            // Note: "@typedef" itself contains "@type" as substring, so we
            // check for "@type " or "@type{" (with space or brace following).
            let has_type_tag = comment_text.contains("@type ") || comment_text.contains("@type{");
            let has_property = comment_text.contains("@property")
                || comment_text.contains("@prop ")
                || comment_text.contains("@prop{")
                || has_type_tag
                || comment_text.contains("@member")
                    && !comment_text.contains("@memberOf")
                    && !comment_text.contains("@memberof");

            if !has_type && !has_property {
                // Emit TS8021 at the typedef name position (TSC points at the name, not @typedef)
                let name_start =
                    after_typedef + rest.find(|c: char| !c.is_whitespace()).unwrap_or(0);
                let name_end = name_start
                    + comment_text[name_start..]
                        .find(|c: char| c.is_whitespace() || c == '*' || c == '/')
                        .unwrap_or(comment_text.len() - name_start);
                let name_len = name_end - name_start;
                let error_pos = comment.pos + name_start as u32;
                let error_len = if name_len > 0 {
                    name_len as u32
                } else {
                    "@typedef".len() as u32
                };
                self.ctx.error(
                    error_pos,
                    error_len,
                    diagnostic_messages::JSDOC_TYPEDEF_TAG_SHOULD_EITHER_HAVE_A_TYPE_ANNOTATION_OR_BE_FOLLOWED_BY_PROPERT
                        .to_string(),
                    diagnostic_codes::JSDOC_TYPEDEF_TAG_SHOULD_EITHER_HAVE_A_TYPE_ANNOTATION_OR_BE_FOLLOWED_BY_PROPERT,
                );
            }
        }
    }

    /// Eagerly validate base types of all `@typedef` declarations in the file.
    /// Emits TS2304 "Cannot find name" for unresolvable simple-name base types.
    pub(crate) fn check_jsdoc_typedef_base_types(&mut self) {
        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        if sf.comments.is_empty() {
            return;
        }
        let source_text: String = sf.text.to_string();
        let comments = sf.comments.clone();

        for comment in &comments {
            if !is_jsdoc_comment(comment, &source_text) {
                continue;
            }
            let content = get_jsdoc_content(comment, &source_text);

            // TS1109: Check for malformed @import tags (bare @import or missing module specifier)
            {
                let comment_text = &source_text
                    [comment.pos as usize..(comment.end as usize).min(source_text.len())];
                let mut search_from = 0;
                while let Some(idx) = comment_text[search_from..].find("@import") {
                    let abs_idx = search_from + idx;
                    let after_import = abs_idx + "@import".len();
                    if after_import < comment_text.len() {
                        let next = comment_text.as_bytes()[after_import];
                        if next.is_ascii_alphanumeric() || next == b'_' {
                            search_from = after_import;
                            continue;
                        }
                    }
                    let rest_full = &comment_text[after_import..];
                    let next_tag = rest_full
                        .lines()
                        .skip(1)
                        .enumerate()
                        .find_map(|(i, line)| {
                            let trimmed = line.trim_start().trim_start_matches('*').trim();
                            if trimmed.starts_with('@') {
                                let offset: usize =
                                    rest_full.lines().take(i + 1).map(|l| l.len() + 1).sum();
                                Some(offset)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(rest_full.len());
                    let raw_slice = rest_full[..next_tag]
                        .trim_end()
                        .trim_end_matches("*/")
                        .trim_end();
                    let joined: String = raw_slice
                        .lines()
                        .map(|l| l.trim().trim_start_matches('*').trim())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let joined = joined.trim();

                    if joined.is_empty() {
                        self.error_expression_expected_at_position(
                            comment.pos + after_import as u32,
                            1,
                        );
                    } else if joined.contains("from")
                        && !joined.contains('"')
                        && !joined.contains('\'')
                        && let Some(from_off) = rest_full[..next_tag].rfind("from")
                    {
                        self.error_expression_expected_at_position(
                            comment.pos + after_import as u32 + from_off as u32 + 4,
                            1,
                        );
                    }
                    search_from = after_import;
                }
            }

            // Collect @template names defined in this comment so we can skip them
            // when checking callback param types.
            let template_names: Vec<String> = Self::jsdoc_template_constraints(&content)
                .into_iter()
                .map(|(name, _)| name)
                .collect();

            for (_name, typedef_info) in Self::parse_jsdoc_typedefs(&content) {
                if let Some(ref cb) = typedef_info.callback {
                    // Check callback param types for unresolvable references (TS2304)
                    for param in &cb.params {
                        let Some(type_expr) = param.type_expr.as_deref() else {
                            continue;
                        };
                        let expr = type_expr.trim();
                        let expr = expr.strip_prefix("...").unwrap_or(expr);
                        if expr.is_empty() {
                            continue;
                        }
                        if !Self::is_simple_type_name(expr) {
                            continue;
                        }
                        // Skip template params defined in this same comment
                        if template_names.iter().any(|t| t == expr) {
                            continue;
                        }
                        if self.resolve_jsdoc_type_str(expr).is_none() {
                            self.emit_jsdoc_cannot_find_name(
                                expr,
                                comment.pos,
                                comment.end,
                                &source_text,
                            );
                        }
                    }
                    continue;
                }
                if let Some(ref base_type) = typedef_info.base_type {
                    let expr = base_type.trim();
                    if expr == "Object" || expr == "object" || expr.is_empty() {
                        continue;
                    }
                    // Only validate simple identifier names — complex type expressions
                    // like `function(string): boolean` or `{num: number}` will naturally
                    // fail resolution and produce false TS2304 errors.
                    if !Self::is_simple_type_name(expr) {
                        continue;
                    }
                    if self.resolve_jsdoc_type_str(expr).is_none() {
                        self.emit_jsdoc_cannot_find_name(
                            expr,
                            comment.pos,
                            comment.end,
                            &source_text,
                        );
                    }
                }
            }
        }

        // Also check @type tag references for unresolvable simple names (TS2304).
        // Only for @type tags on file-level statements (not inside functions),
        // since function-level @type tags may reference function-scoped typedefs.
        for comment in &comments {
            if !is_jsdoc_comment(comment, &source_text) {
                continue;
            }
            // Skip comments inside function bodies - only check file-level @type tags.
            // Use find_function_body_end to get the real body end, since function
            // declaration nodes may include trailing trivia past the closing `}`.
            let mut in_function = false;
            for node in &self.ctx.arena.nodes {
                if !node.is_function_like() {
                    continue;
                }
                let body_end = Self::find_function_body_end(node.pos, node.end, &source_text);
                if comment.pos >= node.pos && comment.pos < body_end {
                    in_function = true;
                    break;
                }
            }
            if in_function {
                continue;
            }
            let content = get_jsdoc_content(comment, &source_text);
            // Check for @type {Name} where Name is a simple identifier
            if let Some(type_expr) = Self::jsdoc_extract_type_tag_expr(&content) {
                let expr = type_expr.trim();
                if !expr.is_empty()
                    && Self::is_simple_type_name(expr)
                    && !expr.contains('<')
                    && !expr.contains('.')
                {
                    // Set anchor to the comment position to respect typedef scoping
                    let prev_anchor = self.ctx.jsdoc_typedef_anchor_pos.get();
                    self.ctx.jsdoc_typedef_anchor_pos.set(comment.pos);
                    let resolved = self.resolve_jsdoc_type_str(expr);
                    self.ctx.jsdoc_typedef_anchor_pos.set(prev_anchor);
                    if resolved.is_none() {
                        // Also check if it's a typedef (globally) that's just out of scope
                        let typedef_exists = self
                            .resolve_jsdoc_typedef_type(expr, u32::MAX, &comments, &source_text)
                            .is_some();
                        if typedef_exists {
                            self.emit_jsdoc_cannot_find_name(
                                expr,
                                comment.pos,
                                comment.end,
                                &source_text,
                            );
                        }
                    }
                }
            }
        }
    }

    /// Emit TS2304 "Cannot find name 'X'" for an unresolvable JSDoc type reference.
    /// Locates the name within the comment text range for precise error positioning.
    pub(crate) fn emit_jsdoc_cannot_find_name(
        &mut self,
        name: &str,
        comment_pos: u32,
        comment_end: u32,
        source_text: &str,
    ) {
        let end = (comment_end as usize).min(source_text.len());
        let comment_range = &source_text[comment_pos as usize..end];
        let (start, length) = if let Some(offset) = comment_range.find(name) {
            (comment_pos + offset as u32, name.len() as u32)
        } else {
            (comment_pos, 0)
        };
        self.error_cannot_find_name_at_position(name, start, length);
    }

    /// Check whether a JSDoc type expression is a simple identifier name
    /// (possibly with dots and angle brackets for generics).
    /// Returns false for complex expressions like function types, object literals, unions.
    fn is_simple_type_name(expr: &str) -> bool {
        if expr.is_empty() {
            return false;
        }
        let first = expr.chars().next().unwrap_or('\0');
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
}
