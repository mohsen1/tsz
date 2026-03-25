//! JSDoc diagnostic validation helpers for `CheckerState`.
//!
//! This module owns all JSDoc-specific diagnostic emission:
//! - TS8033 duplicate `@type` in `@typedef` checking
//! - TS8021 missing type annotation in `@typedef` checking
//! - TS2304 base type validation for `@typedef` declarations
//! - TS2300 duplicate `@import` tag detection
//! - TS1109 malformed `@import` tag detection
//! - `@satisfies` malformed/duplicate tag detection

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

    /// Check for JSDoc `@property`/`@prop`/`@member` tags that use private
    /// names like `#id`. TypeScript reports TS1003 at the `#` token because
    /// JSDoc property names must be identifiers, dotted names, or quoted names.
    pub(crate) fn check_jsdoc_property_private_names(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_common::comments::is_jsdoc_comment;

        fn jsdoc_tag_len(line: &str) -> Option<usize> {
            for tag in ["@property", "@prop", "@member"] {
                if let Some(after_tag) = line.strip_prefix(tag) {
                    let next = after_tag.chars().next().unwrap_or('\0');
                    if next == '\0' || next.is_whitespace() || next == '{' {
                        return Some(tag.len());
                    }
                }
            }
            None
        }

        fn skip_curly_type_expr(text: &str) -> Option<usize> {
            if !text.starts_with('{') {
                return None;
            }
            let mut depth = 0usize;
            for (idx, ch) in text.char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            return Some(idx + 1);
                        }
                    }
                    _ => {}
                }
            }
            None
        }

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: &str = &sf.text;

        for comment in &sf.comments {
            if !is_jsdoc_comment(comment, source_text) {
                continue;
            }

            let comment_text = comment.get_text(source_text);
            let mut comment_offset = 0usize;

            for segment in comment_text.split_inclusive('\n') {
                let raw_line = segment.trim_end_matches(['\r', '\n']);
                let mut content_start = raw_line.len() - raw_line.trim_start().len();
                let mut content = &raw_line[content_start..];

                if let Some(after_open) = content.strip_prefix("/**") {
                    content_start += 3;
                    let ws_after_open = after_open.len() - after_open.trim_start().len();
                    content_start += ws_after_open;
                    content = &raw_line[content_start..];
                } else if let Some(after_open) = content.strip_prefix("/*") {
                    content_start += 2;
                    let ws_after_open = after_open.len() - after_open.trim_start().len();
                    content_start += ws_after_open;
                    content = &raw_line[content_start..];
                }

                if let Some(after_star) = content.strip_prefix('*') {
                    content_start += 1;
                    let ws_after_star = after_star.len() - after_star.trim_start().len();
                    content_start += ws_after_star;
                    content = &raw_line[content_start..];
                }

                let Some(tag_len) = jsdoc_tag_len(content) else {
                    comment_offset += segment.len();
                    continue;
                };

                let after_tag = &content[tag_len..];
                let ws_after_tag = after_tag.len() - after_tag.trim_start().len();
                let rest = after_tag.trim_start();
                let rest_offset = content_start + tag_len + ws_after_tag;

                let private_name_offset = if rest.starts_with('{') {
                    skip_curly_type_expr(rest).and_then(|type_end| {
                        let after_type = &rest[type_end..];
                        let ws_after_type = after_type.len() - after_type.trim_start().len();
                        after_type
                            .trim_start()
                            .starts_with('#')
                            .then_some(type_end + ws_after_type)
                    })
                } else {
                    rest.starts_with('#').then_some(0)
                };

                if let Some(private_name_offset) = private_name_offset {
                    self.ctx.error(
                        comment.pos + (comment_offset + rest_offset + private_name_offset) as u32,
                        1,
                        diagnostic_messages::IDENTIFIER_EXPECTED.to_string(),
                        diagnostic_codes::IDENTIFIER_EXPECTED,
                    );
                }

                comment_offset += segment.len();
            }
        }
    }

    /// Check for malformed JSDoc function types like `function(@foo)`.
    ///
    /// TypeScript reports:
    /// - TS7014 on the whole function type when it lacks a return annotation
    /// - TS1110 at the `@`
    /// - TS2304 at the following identifier
    pub(crate) fn check_malformed_jsdoc_function_type_params(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_common::comments::is_jsdoc_comment;

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: &str = &sf.text;

        for comment in &sf.comments {
            if !is_jsdoc_comment(comment, source_text) {
                continue;
            }

            let comment_text = comment.get_text(source_text);

            for (function_offset, _) in comment_text.match_indices("function(") {
                let after_function = &comment_text[function_offset + "function(".len()..];
                let Some(close_paren_offset) = after_function.find(')') else {
                    continue;
                };

                let params_text = &after_function[..close_paren_offset];
                let is_constructor_type = params_text.trim_start().starts_with("new:");
                let has_return_annotation =
                    after_function[close_paren_offset + 1..].trim_start().starts_with(':');
                let function_len = "function(".len() + close_paren_offset + 1;
                let function_pos = comment.pos + function_offset as u32;
                let mut reported_missing_return = false;
                let mut search_offset = 0usize;

                while let Some(at_offset) = params_text[search_offset..].find('@') {
                    let at_offset = search_offset + at_offset;
                    let ident_start = at_offset + 1;
                    let ident = params_text[ident_start..]
                        .chars()
                        .take_while(|ch| *ch == '_' || *ch == '$' || ch.is_ascii_alphanumeric())
                        .collect::<String>();

                    if ident.is_empty() {
                        search_offset = ident_start;
                        continue;
                    }

                    if !reported_missing_return
                        && !is_constructor_type
                        && !has_return_annotation
                        && self.ctx.no_implicit_any()
                    {
                        self.ctx.error(
                            function_pos,
                            function_len as u32,
                            diagnostic_messages::FUNCTION_TYPE_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN_TYPE
                                .replace("{0}", "any"),
                            diagnostic_codes::FUNCTION_TYPE_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN_TYPE,
                        );
                        reported_missing_return = true;
                    }

                    let at_pos =
                        function_pos + "function(".len() as u32 + at_offset as u32;
                    self.ctx.error(
                        at_pos,
                        1,
                        diagnostic_messages::TYPE_EXPECTED.to_string(),
                        diagnostic_codes::TYPE_EXPECTED,
                    );
                    self.ctx.error(
                        at_pos + 1,
                        ident.len() as u32,
                        format!("Cannot find name '{ident}'."),
                        diagnostic_codes::CANNOT_FIND_NAME,
                    );

                    search_offset = ident_start + ident.len();
                }
            }
        }
    }

    /// TS8039: Check for `@template` tags that follow a `@typedef`, `@callback`,
    /// or `@overload` tag within the same JSDoc comment.
    ///
    /// In tsc, `@template` tags must appear BEFORE `@typedef`/`@callback`/`@overload`.
    /// When `@template` appears after, it's scoped to the preceding tag and is invalid.
    pub(crate) fn check_template_after_typedef_callback(&mut self) {
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

            let _comment_text =
                &source_text[comment.pos as usize..(comment.end as usize).min(source_text.len())];

            // tsc 6.0: @template after @typedef/@callback/@overload in the same
            // comment is valid — it defines the type parameters for the typedef.
            // The previous check emitted TS8039 here but tsc 6.0 accepts this pattern.
        }
    }

    /// TS2300: Check for duplicate `@import` names across JSDoc comments.
    ///
    /// When the same name is imported via `@import` in multiple JSDoc comments,
    /// tsc emits TS2300 "Duplicate identifier 'X'" at each occurrence.
    pub(crate) fn check_jsdoc_duplicate_imports(&mut self) {
        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: String = sf.text.to_string();
        let comments = sf.comments.clone();

        // Collect all @import names with their source positions.
        // Each entry is (name, absolute_position_of_name, name_length).
        let mut import_names: Vec<(String, u32, u32)> = Vec::new();

        for comment in &comments {
            if !is_jsdoc_comment(comment, &source_text) {
                continue;
            }
            let comment_text =
                &source_text[comment.pos as usize..(comment.end as usize).min(source_text.len())];
            let content = get_jsdoc_content(comment, &source_text);

            // Scan for @import tags in this comment
            for line in content.lines() {
                let trimmed = line.trim_start_matches('*').trim();
                if let Some(rest) = trimmed.strip_prefix("@import") {
                    let imports = Self::parse_jsdoc_import_tag(rest);
                    for (local_name, _specifier, _import_name) in imports {
                        // Find the position of the local name in the comment text.
                        // For `@import { Foo } from "..."`, `Foo` appears after `{`.
                        // We search for the name in the comment text to get its absolute position.
                        if let Some(name_offset) =
                            Self::find_import_name_in_comment(comment_text, &local_name)
                        {
                            let abs_pos = comment.pos + name_offset as u32;
                            import_names.push((
                                local_name, abs_pos, 0, // placeholder, will use name length
                            ));
                        }
                    }
                }
            }
        }

        // Check for duplicates
        let mut seen: std::collections::HashMap<String, Vec<(u32, u32)>> =
            std::collections::HashMap::new();
        for (name, pos, _) in &import_names {
            seen.entry(name.clone())
                .or_default()
                .push((*pos, name.len() as u32));
        }

        for (name, positions) in &seen {
            if positions.len() > 1 {
                use crate::diagnostics::{diagnostic_codes, format_message};
                let message = format_message("Duplicate identifier '{0}'.", &[name]);
                for &(pos, len) in positions {
                    self.error_at_position(
                        pos,
                        len,
                        &message,
                        diagnostic_codes::DUPLICATE_IDENTIFIER,
                    );
                }
            }
        }
    }

    /// Find the position of an import name within a JSDoc comment text.
    /// Returns the byte offset from the start of the comment.
    fn find_import_name_in_comment(comment_text: &str, name: &str) -> Option<usize> {
        // Look for the name after @import
        let import_idx = comment_text.find("@import")?;
        let after_import = import_idx + "@import".len();
        let rest = &comment_text[after_import..];

        // For `@import { Foo } from "..."`, find `Foo` after `{`
        if let Some(brace_pos) = rest.find('{') {
            let after_brace = &rest[brace_pos + 1..];
            if let Some(name_offset) = after_brace.find(name) {
                // Verify it's a word boundary (not part of a longer name)
                let before_ok = name_offset == 0
                    || !after_brace.as_bytes()[name_offset - 1].is_ascii_alphanumeric();
                let after_ok = name_offset + name.len() >= after_brace.len()
                    || !after_brace.as_bytes()[name_offset + name.len()].is_ascii_alphanumeric();
                if before_ok && after_ok {
                    return Some(after_import + brace_pos + 1 + name_offset);
                }
            }
        }

        // For `@import * as Name from "..."` or `@import Name from "..."`
        if let Some(name_offset) = rest.find(name) {
            return Some(after_import + name_offset);
        }

        None
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
                    } else if !joined.is_empty() && !joined.contains("from") {
                        // TS1005: @import clause without 'from' keyword, e.g.:
                        //   @import x = require("types")  — should be: @import { x } from "types"
                        //   @import Foo                    — missing 'from "module"'
                        // Find the position after the import clause (first identifier)
                        // where 'from' is expected.
                        let rest_trimmed = rest_full.trim_start();
                        let skip_ws = rest_full.len() - rest_trimmed.len();
                        // Skip past the first identifier-like characters
                        let clause_end = rest_trimmed
                            .find(|c: char| {
                                !c.is_alphanumeric()
                                    && c != '_'
                                    && c != '{'
                                    && c != '}'
                                    && c != '*'
                                    && c != ' '
                                    && c != ','
                            })
                            .unwrap_or(rest_trimmed.len());
                        let error_pos =
                            comment.pos + after_import as u32 + skip_ws as u32 + clause_end as u32;
                        self.error_at_position(
                            error_pos,
                            1,
                            "'from' expected.",
                            crate::diagnostics::diagnostic_codes::EXPECTED,
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
                for prop in &typedef_info.properties {
                    let expr = prop.type_expr.trim().trim_end_matches('=').trim();
                    if expr.is_empty() || expr == "Object" || expr == "object" {
                        continue;
                    }
                    if !Self::is_simple_type_name(expr) {
                        continue;
                    }
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

// =============================================================================
// @satisfies tag validation
// =============================================================================

use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    pub(crate) fn report_malformed_jsdoc_satisfies_tags(&mut self, idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_common::comments::is_jsdoc_comment;

        if !self.ctx.should_resolve_jsdoc() {
            return;
        }

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;

        if let Some((_jsdoc, jsdoc_start)) =
            self.try_jsdoc_with_ancestor_walk_and_pos(idx, comments, source_text)
            && let Some(comment) = comments.iter().find(|c| c.pos == jsdoc_start)
        {
            for (open_pos, close_pos) in
                Self::malformed_jsdoc_satisfies_positions(source_text, comment.pos, comment.end)
            {
                self.ctx.error(
                    open_pos,
                    0,
                    format_message(diagnostic_messages::EXPECTED, &["{"]),
                    diagnostic_codes::EXPECTED,
                );
                self.ctx.error(
                    close_pos,
                    0,
                    format_message(diagnostic_messages::EXPECTED, &["}"]),
                    diagnostic_codes::EXPECTED,
                );
            }
        }

        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) else {
            return;
        };
        if var_decl.initializer.is_none() {
            return;
        }

        let Some(init_node) = self.ctx.arena.get(var_decl.initializer) else {
            return;
        };
        if let Some((_, pos)) =
            self.try_leading_jsdoc_with_pos(comments, init_node.pos, source_text)
            && let Some(comment) = comments
                .iter()
                .find(|c| c.pos == pos)
                .filter(|c| is_jsdoc_comment(c, source_text))
        {
            for (open_pos, close_pos) in
                Self::malformed_jsdoc_satisfies_positions(source_text, comment.pos, comment.end)
            {
                self.ctx.error(
                    open_pos,
                    0,
                    format_message(diagnostic_messages::EXPECTED, &["{"]),
                    diagnostic_codes::EXPECTED,
                );
                self.ctx.error(
                    close_pos,
                    0,
                    format_message(diagnostic_messages::EXPECTED, &["}"]),
                    diagnostic_codes::EXPECTED,
                );
            }
        }
    }

    pub(crate) fn report_duplicate_jsdoc_satisfies_tags(&mut self, idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_common::comments::is_jsdoc_comment;

        if !self.ctx.should_resolve_jsdoc() {
            return;
        }

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;

        let mut attached_positions: Vec<u32> = Vec::new();
        let mut attached_comment_pos = None;
        if let Some((_jsdoc, jsdoc_start)) =
            self.try_jsdoc_with_ancestor_walk_and_pos(idx, comments, source_text)
        {
            if let Some(comment) = comments.iter().find(|c| c.pos == jsdoc_start) {
                let raw = &source_text[comment.pos as usize..comment.end as usize];
                attached_positions = Self::jsdoc_satisfies_keyword_positions(raw, jsdoc_start);
            }
            attached_comment_pos = Some(jsdoc_start);
            self.emit_duplicate_jsdoc_satisfies_positions(&attached_positions);
        }

        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) else {
            return;
        };
        if var_decl.initializer.is_none() {
            return;
        }

        let Some(init_node) = self.ctx.arena.get(var_decl.initializer) else {
            return;
        };
        let Some(comment) = self
            .try_leading_jsdoc_with_pos(comments, init_node.pos, source_text)
            .and_then(|(_, pos)| comments.iter().find(|c| c.pos == pos))
            .filter(|c| is_jsdoc_comment(c, source_text))
        else {
            return;
        };

        let inline_positions = Self::jsdoc_satisfies_keyword_positions(
            &source_text[comment.pos as usize..comment.end as usize],
            comment.pos,
        );
        self.emit_duplicate_jsdoc_satisfies_positions(&inline_positions);

        if !attached_positions.is_empty()
            && !inline_positions.is_empty()
            && attached_comment_pos != Some(comment.pos)
        {
            let message =
                format_message(diagnostic_messages::TAG_ALREADY_SPECIFIED, &["satisfies"]);
            self.ctx.error(
                attached_positions[0],
                "satisfies".len() as u32,
                message,
                diagnostic_codes::TAG_ALREADY_SPECIFIED,
            );
        }
    }

    fn jsdoc_satisfies_keyword_positions(jsdoc: &str, jsdoc_start: u32) -> Vec<u32> {
        let mut positions = Vec::new();
        let mut search_from = 0usize;
        while let Some(rel) = jsdoc[search_from..].find("@satisfies") {
            let absolute = search_from + rel;
            positions.push(jsdoc_start + absolute as u32 + 1);
            search_from = absolute + "@satisfies".len();
        }
        positions
    }

    fn emit_duplicate_jsdoc_satisfies_positions(&mut self, positions: &[u32]) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        if positions.len() < 2 {
            return;
        }
        let message = format_message(diagnostic_messages::TAG_ALREADY_SPECIFIED, &["satisfies"]);
        for &pos in &positions[1..] {
            self.ctx.error(
                pos,
                "satisfies".len() as u32,
                message.clone(),
                diagnostic_codes::TAG_ALREADY_SPECIFIED,
            );
        }
    }

    fn malformed_jsdoc_satisfies_positions(
        source_text: &str,
        comment_pos: u32,
        comment_end: u32,
    ) -> Vec<(u32, u32)> {
        let raw = &source_text[comment_pos as usize..comment_end as usize];
        let mut result = Vec::new();
        let mut search_from = 0usize;
        while let Some(rel) = raw[search_from..].find("@satisfies") {
            let tag_start = search_from + rel;
            let after_tag = tag_start + "@satisfies".len();
            let ws_trimmed = raw[after_tag..].trim_start_matches(char::is_whitespace);
            let skipped = raw[after_tag..].len() - ws_trimmed.len();
            if !ws_trimmed.starts_with('{') {
                let open_pos = comment_pos + (after_tag + skipped) as u32;
                let close_pos = comment_end.saturating_sub(2);
                result.push((open_pos, close_pos));
            }
            search_from = after_tag;
        }
        result
    }
}
