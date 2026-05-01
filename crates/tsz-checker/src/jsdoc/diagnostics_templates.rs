//! JSDoc `@template` diagnostic helpers for `CheckerState`.

use crate::state::CheckerState;

impl<'a> CheckerState<'a> {
    /// TS1274: In JSDoc, `@template in/out` is invalid on function declarations.
    ///
    /// TypeScript allows variance modifiers on class/interface/type-alias type
    /// parameters, but not on function type parameters. In JS sources this
    /// shows up through JSDoc `@template` tags, so we validate function-hosted
    /// JSDoc here and emit TS1274 at the modifier token.
    pub(crate) fn check_jsdoc_function_template_variance_modifiers(
        &mut self,
        func_idx: tsz_parser::parser::NodeIndex,
    ) {
        use crate::diagnostics::{diagnostic_codes, format_message};
        use tsz_common::diagnostics::get_message_template;

        if !self.is_js_file() {
            return;
        }

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let Some(func_node) = self.ctx.arena.get(func_idx) else {
            return;
        };

        let Some((_jsdoc, comment_pos)) =
            self.try_leading_jsdoc_with_pos(comments, func_node.pos, source_text)
        else {
            return;
        };

        let comment_end = func_node.pos.min(source_text.len() as u32) as usize;
        let raw_comment = &source_text[comment_pos as usize..comment_end];
        let mut cursor = 0usize;

        while let Some(rel) = raw_comment[cursor..].find("@template") {
            let template_start = cursor + rel;
            let mut idx = template_start + "@template".len();

            while let Some(ch) = raw_comment[idx..].chars().next() {
                if ch == ' ' || ch == '\t' || ch == '*' {
                    idx += ch.len_utf8();
                } else {
                    break;
                }
            }

            // Skip a leading JSDoc constraint shape: @template {Constraint} T
            if raw_comment[idx..].starts_with('{') {
                let mut depth = 0i32;
                let mut close_rel = None;
                for (off, ch) in raw_comment[idx..].char_indices() {
                    match ch {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                close_rel = Some(off);
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(close_rel) = close_rel {
                    idx += close_rel + 1;
                    while let Some(ch) = raw_comment[idx..].chars().next() {
                        if ch.is_ascii_whitespace() || ch == '*' {
                            idx += ch.len_utf8();
                        } else {
                            break;
                        }
                    }
                }
            }

            let mut scan = idx;
            while let Some(ch) = raw_comment[scan..].chars().next() {
                if ch == '\n' || ch == '\r' || ch == '@' {
                    break;
                }
                if ch == ',' || ch.is_ascii_whitespace() || ch == '*' {
                    scan += ch.len_utf8();
                    continue;
                }

                let token_start = scan;
                while let Some(tok_ch) = raw_comment[scan..].chars().next() {
                    if tok_ch == '_' || tok_ch == '$' || tok_ch.is_ascii_alphanumeric() {
                        scan += tok_ch.len_utf8();
                    } else {
                        break;
                    }
                }
                if token_start == scan {
                    break;
                }

                let token = &raw_comment[token_start..scan];
                if token == "const" {
                    continue;
                }
                if token == "in" || token == "out" {
                    let template = get_message_template(
                        diagnostic_codes::MODIFIER_CAN_ONLY_APPEAR_ON_A_TYPE_PARAMETER_OF_A_CLASS_INTERFACE_OR_TYPE_ALIAS,
                    )
                    .unwrap_or("'{0}' modifier can only appear on a type parameter of a class, interface or type alias");
                    let message = format_message(template, &[token]);
                    self.error_at_position(
                        comment_pos + token_start as u32,
                        token.len() as u32,
                        &message,
                        diagnostic_codes::MODIFIER_CAN_ONLY_APPEAR_ON_A_TYPE_PARAMETER_OF_A_CLASS_INTERFACE_OR_TYPE_ALIAS,
                    );
                    break;
                }
                // First non-modifier token is the type parameter name.
                break;
            }

            cursor = scan.max(template_start + "@template".len());
        }
    }

    /// TS8039: Check for `@template` tags that follow a `@typedef`, `@callback`,
    /// or `@overload` tag within the same JSDoc comment.
    ///
    /// In tsc, `@template` tags must appear BEFORE `@typedef`/`@callback`/`@overload`.
    /// When `@template` appears after, it's scoped to the preceding tag and is invalid.
    pub(crate) fn check_template_after_typedef_callback(&mut self) {
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

            let comment_text =
                &source_text[comment.pos as usize..(comment.end as usize).min(source_text.len())];
            let mut saw_typedef = false;
            let mut template_is_invalid_here = false;
            let mut emitted_template_error = false;

            for raw_line in comment_text.lines() {
                let line_start = raw_line.as_ptr() as usize - comment_text.as_ptr() as usize;
                let trimmed_start = raw_line
                    .find(|ch: char| !ch.is_whitespace() && ch != '*')
                    .unwrap_or(raw_line.len());
                let mut content = raw_line[trimmed_start..]
                    .trim_start_matches("/**")
                    .trim_start_matches("/*")
                    .trim();
                content = content.trim_end_matches("*/").trim();

                if content.starts_with("@typedef") {
                    saw_typedef = true;
                    continue;
                }
                if content.starts_with("@callback") || content.starts_with("@overload") {
                    template_is_invalid_here = true;
                    continue;
                }
                if (content.starts_with("@property")
                    || content.starts_with("@prop ")
                    || content.starts_with("@prop{")
                    || content.starts_with("@member")
                    || content.starts_with("@param"))
                    && saw_typedef
                {
                    template_is_invalid_here = true;
                }

                if !content.starts_with("@template") {
                    continue;
                }
                if !template_is_invalid_here && !saw_typedef {
                    break;
                }

                let prefix_len = raw_line[trimmed_start..].find(content).unwrap_or(0);
                if template_is_invalid_here && !emitted_template_error {
                    let pos = comment.pos + (line_start + trimmed_start + prefix_len + 1) as u32;
                    self.error_at_position(
                        pos,
                        "template".len() as u32,
                        diagnostic_messages::A_JSDOC_TEMPLATE_TAG_MAY_NOT_FOLLOW_A_TYPEDEF_CALLBACK_OR_OVERLOAD_TAG,
                        diagnostic_codes::A_JSDOC_TEMPLATE_TAG_MAY_NOT_FOLLOW_A_TYPEDEF_CALLBACK_OR_OVERLOAD_TAG,
                    );
                    emitted_template_error = true;
                }
                let invalid_template_name = content
                    .strip_prefix("@template")
                    .and_then(|rest| rest.split_whitespace().next())
                    .map(|name| name.trim_matches(',').to_string())
                    .filter(|name| !name.is_empty());
                if let Some(name) = invalid_template_name.as_deref() {
                    let mut later_base = line_start + raw_line.len();
                    for later_line in comment_text[later_base..].lines() {
                        let later_trimmed_start = later_line
                            .find(|ch: char| !ch.is_whitespace() && ch != '*')
                            .unwrap_or(later_line.len());
                        let later_content = later_line[later_trimmed_start..]
                            .trim_start_matches("/**")
                            .trim_start_matches("/*")
                            .trim()
                            .trim_end_matches("*/")
                            .trim();
                        if later_content.starts_with("@template") {
                            later_base += later_line.len() + 1;
                            continue;
                        }
                        if later_content.starts_with("@returns")
                            || later_content.starts_with("@return")
                        {
                            break;
                        }
                        if (later_content.starts_with("@param")
                            || later_content.starts_with("@property"))
                            && let Some(open) = later_content.find('{')
                            && let Some(close_rel) = later_content[open + 1..].find('}')
                        {
                            let type_expr = &later_content[open + 1..open + 1 + close_rel];
                            if let Some(name_offset) = type_expr.find(name) {
                                let content_offset = later_line[later_trimmed_start..]
                                    .find(later_content)
                                    .unwrap_or(0);
                                let type_start = content_offset + open + 1 + name_offset;
                                let pos = comment.pos
                                    + (later_base + later_trimmed_start + type_start) as u32;
                                self.error_at_position(
                                    pos,
                                    name.len() as u32,
                                    &crate::diagnostics::format_message(
                                        diagnostic_messages::CANNOT_FIND_NAME,
                                        &[name],
                                    ),
                                    diagnostic_codes::CANNOT_FIND_NAME,
                                );
                            }
                        }
                        later_base += later_line.len() + 1;
                    }
                }
            }
        }
    }

    /// TS1273/TS1277: Check for invalid modifiers on JSDoc `@template` type parameters.
    ///
    /// In tsc, certain modifier keywords before a `@template` type parameter name
    /// are always invalid (e.g. `private`, `public`, `protected`, `static` -> TS1273),
    /// while others like `const` are only valid on function/method/class type params
    /// (TS1277 when used on a `@typedef`/`@callback`).
    pub(crate) fn check_jsdoc_template_modifiers(&mut self) {
        use crate::diagnostics::diagnostic_codes;
        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: String = sf.text.to_string();
        let comments = sf.comments.clone();

        const NEVER_VALID_MODIFIERS: &[&str] = &[
            "private",
            "public",
            "protected",
            "static",
            "override",
            "abstract",
            "readonly",
            "async",
            "declare",
            "default",
            "export",
        ];
        const CONST_MODIFIER: &str = "const";

        for comment in &comments {
            if !is_jsdoc_comment(comment, &source_text) {
                continue;
            }
            let comment_text =
                &source_text[comment.pos as usize..(comment.end as usize).min(source_text.len())];
            let content = get_jsdoc_content(comment, &source_text);
            let has_typedef = content.contains("@typedef") || content.contains("@callback");

            for raw_line in content.lines() {
                let trimmed = raw_line.trim().trim_start_matches('*').trim();
                let Some(rest) = trimmed.strip_prefix("@template") else {
                    continue;
                };
                let rest = rest.trim();
                if rest.is_empty() {
                    continue;
                }

                let after_constraint = if let Some(inner) = rest.strip_prefix('{') {
                    let mut depth = 1usize;
                    let mut close_idx = None;
                    for (idx, ch) in inner.char_indices() {
                        match ch {
                            '{' => depth += 1,
                            '}' => {
                                depth = depth.saturating_sub(1);
                                if depth == 0 {
                                    close_idx = Some(idx);
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    if let Some(ci) = close_idx {
                        inner[ci + 1..].trim()
                    } else {
                        continue;
                    }
                } else {
                    rest
                };

                let first_word_end = after_constraint
                    .find(|c: char| c.is_ascii_whitespace() || c == ',')
                    .unwrap_or(after_constraint.len());
                let first_word = &after_constraint[..first_word_end];
                if first_word.is_empty() {
                    continue;
                }

                let after_first = after_constraint[first_word_end..].trim_start();
                let has_following_name = !after_first.is_empty()
                    && after_first
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$');
                if !has_following_name {
                    continue;
                }

                let find_modifier_pos = |modifier: &str| -> (u32, u32) {
                    if let Some(template_offset) = comment_text.find("@template") {
                        let after_template = &comment_text[template_offset + "@template".len()..];
                        if let Some(mod_offset) = after_template.find(modifier) {
                            let abs_pos = comment.pos
                                + template_offset as u32
                                + "@template".len() as u32
                                + mod_offset as u32;
                            return (abs_pos, modifier.len() as u32);
                        }
                    }
                    (comment.pos, 0)
                };

                if NEVER_VALID_MODIFIERS.contains(&first_word) {
                    let (pos, len) = find_modifier_pos(first_word);
                    let message =
                        format!("'{first_word}' modifier cannot appear on a type parameter");
                    self.error_at_position(
                        pos,
                        len,
                        &message,
                        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_TYPE_PARAMETER,
                    );
                    continue;
                }

                if first_word == CONST_MODIFIER {
                    if has_typedef {
                        let (pos, len) = find_modifier_pos(CONST_MODIFIER);
                        let message =
                            "'const' modifier can only appear on a type parameter of a function, method or class".to_string();
                        self.error_at_position(
                            pos,
                            len,
                            &message,
                            diagnostic_codes::MODIFIER_CAN_ONLY_APPEAR_ON_A_TYPE_PARAMETER_OF_A_FUNCTION_METHOD_OR_CLASS,
                        );
                    }
                    continue;
                }
            }
        }
    }

    /// Return `true` if `name` matches an `@template` declaration whose
    /// scope contains the reference at `ref_pos`.
    pub(crate) fn source_file_declares_jsdoc_template_at(&self, name: &str, ref_pos: u32) -> bool {
        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};
        use tsz_parser::parser::syntax_kind_ext;

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return false;
        };
        let source_text: &str = &sf.text;

        for &stmt_idx in &sf.statements.nodes {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::CLASS_DECLARATION
                && stmt_node.kind != syntax_kind_ext::CLASS_EXPRESSION
            {
                continue;
            }
            if !(ref_pos >= stmt_node.pos && ref_pos < stmt_node.end) {
                continue;
            }
            for comment in &sf.comments {
                if !is_jsdoc_comment(comment, source_text) {
                    continue;
                }
                if comment.end > stmt_node.pos {
                    continue;
                }
                let content = get_jsdoc_content(comment, source_text);
                if Self::jsdoc_template_type_params(&content)
                    .into_iter()
                    .any(|(decl_name, _)| decl_name == name)
                {
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn source_file_declares_jsdoc_template(&self, name: &str) -> bool {
        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};
        let Some(sf) = self.ctx.arena.source_files.first() else {
            return false;
        };
        let source_text: &str = &sf.text;
        for comment in &sf.comments {
            if !is_jsdoc_comment(comment, source_text) {
                continue;
            }
            let content = get_jsdoc_content(comment, source_text);
            for (decl_name, _is_const) in Self::jsdoc_template_type_params(&content) {
                if decl_name == name {
                    return true;
                }
            }
        }
        false
    }
}
