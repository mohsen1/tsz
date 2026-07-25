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

        while let Some(rel) = Self::jsdoc_tag_offset(&raw_comment[cursor..], "template") {
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
                    continue;
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

                if Self::jsdoc_line_starts_with_tag(content, "typedef") {
                    saw_typedef = true;
                    continue;
                }
                if Self::jsdoc_line_starts_with_tag(content, "callback")
                    || Self::jsdoc_line_starts_with_tag(content, "overload")
                {
                    template_is_invalid_here = true;
                    continue;
                }
                if (Self::jsdoc_line_starts_with_tag(content, "property")
                    || Self::jsdoc_line_starts_with_tag(content, "prop")
                    || Self::jsdoc_line_starts_with_tag(content, "member")
                    || Self::jsdoc_line_starts_with_tag(content, "param"))
                    && saw_typedef
                {
                    template_is_invalid_here = true;
                }

                let Some(template_rest) = Self::strip_jsdoc_tag_prefix(content, "template") else {
                    continue;
                };
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
                let invalid_template_name = template_rest
                    .split_whitespace()
                    .next()
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
                        if Self::jsdoc_line_starts_with_tag(later_content, "template") {
                            later_base += later_line.len() + 1;
                            continue;
                        }
                        if Self::strip_jsdoc_return_tag_prefix(later_content).is_some() {
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
            let has_typedef = Self::jsdoc_contains_tag(&content, "typedef")
                || Self::jsdoc_contains_tag(&content, "callback");

            for raw_line in content.lines() {
                let trimmed = raw_line.trim().trim_start_matches('*').trim();
                let Some(rest) = Self::strip_jsdoc_tag_prefix(trimmed, "template") else {
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
                    if let Some(template_offset) = Self::jsdoc_tag_offset(comment_text, "template")
                    {
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
        use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return false;
        };
        let source_text: &str = &sf.text;

        // Locate the innermost class enclosing the reference. Scanning the arena
        // rather than the top-level statement list is what makes `export class
        // Foo` work: the parser wraps that class in an `EXPORT_DECLARATION`, so
        // the class is not itself a top-level statement and a statement-list
        // scan would miss its `@template`.
        let mut host_pos: Option<u32> = None;
        for raw_idx in 0..self.ctx.arena.len() {
            let idx = NodeIndex(raw_idx as u32);
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::CLASS_DECLARATION
                && node.kind != syntax_kind_ext::CLASS_EXPRESSION
            {
                continue;
            }
            if !(ref_pos >= node.pos && ref_pos < node.end) {
                continue;
            }
            // The JSDoc sits before the `export` keyword, so anchor the comment
            // search at the wrapping export when there is one.
            let mut anchor = node.pos;
            if let Some(ext) = self.ctx.arena.get_extended(idx)
                && ext.parent.is_some()
                && let Some(parent) = self.ctx.arena.get(ext.parent)
                && parent.kind == syntax_kind_ext::EXPORT_DECLARATION
            {
                anchor = parent.pos;
            }
            // Innermost wins: a later, tighter class shadows an outer one.
            if host_pos.is_none_or(|prev| anchor >= prev) {
                host_pos = Some(anchor);
            }
        }
        let Some(host_pos) = host_pos else {
            return false;
        };

        for comment in &sf.comments {
            if !is_jsdoc_comment(comment, source_text) {
                continue;
            }
            if comment.end > host_pos {
                continue;
            }
            let content = get_jsdoc_content(comment, source_text);
            if Self::jsdoc_template_type_params(&content)
                .into_iter()
                .any(|(decl_name, _, _)| decl_name == name)
            {
                return true;
            }
        }
        false
    }

    /// True when `name` is declared by an `@template` tag that is actually in
    /// scope for a JSDoc type reference at `ref_pos`.
    ///
    /// `tsc` scopes `@template` to the declaration its comment is attached to,
    /// plus — for a class — that class's members. A `@template` written on some
    /// other declaration in the same file is not in scope, so a reference to it
    /// is a genuine `TS2304`. The case that matters most in practice is a JS
    /// constructor function: `@template K` on `function M() {}` does not reach a
    /// separate JSDoc comment on `M.prototype.get`, and `tsc` reports
    /// "Cannot find name 'K'" there.
    pub(crate) fn jsdoc_template_in_scope_for_reference(
        &self,
        name: &str,
        own_comment_content: &str,
        ref_pos: u32,
    ) -> bool {
        // The reference's own comment: `@template T` alongside `@param {T}`.
        if Self::jsdoc_template_type_params(own_comment_content)
            .into_iter()
            .any(|(decl_name, _is_const, _default)| decl_name == name)
        {
            return true;
        }
        // An enclosing class declaration's `@template`, in scope for members.
        self.source_file_declares_jsdoc_template_at(name, ref_pos)
    }

    /// TS1069: `@template {Constraint}` with no type-parameter name following
    /// the constraint braces (e.g. `@template {T}`). TypeScript's JSDoc parser
    /// reports "Unexpected token. A type parameter name was expected without
    /// curly braces." at the position where the name was expected. This is a
    /// purely syntactic property of the JSDoc comment — tsc emits it during
    /// parsing regardless of what the tag decorates — so it runs once per JS
    /// source file over every JSDoc comment (class, function, and variable
    /// hosts alike). No TS2304 accompanies it: tsc does not treat the braced
    /// text as a `Cannot find name` reference.
    pub(crate) fn check_jsdoc_template_brace_syntax(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_common::comments::is_jsdoc_comment;

        if !self.is_js_file() {
            return;
        }
        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: &str = &sf.text;

        // Collect anchors before emitting so the `self.ctx.error` loop does not
        // conflict with the immutable borrow of `sf.comments`.
        let mut anchors: Vec<u32> = Vec::new();
        for comment in &sf.comments {
            if !is_jsdoc_comment(comment, source_text) {
                continue;
            }
            let text = comment.get_text(source_text);
            let base = comment.pos;
            let mut scan = 0usize;
            while let Some(rel) = Self::jsdoc_tag_offset(&text[scan..], "template") {
                let tag_off = scan + rel;
                let after_tag = tag_off + "@template".len();
                let rest = &text[after_tag..];
                let trimmed = rest.trim_start();
                if !trimmed.starts_with('{') {
                    scan = after_tag;
                    continue;
                }
                let leading_ws = rest.len() - trimmed.len();
                let brace_off = after_tag + leading_ws;
                let Some(close_rel) =
                    Self::jsdoc_balanced_close_brace_offset(&text[brace_off + 1..])
                else {
                    scan = brace_off + 1;
                    continue;
                };
                let close_off = brace_off + 1 + close_rel;

                // `@template {Constraint} Name` — a following identifier (past
                // whitespace and continuation asterisks) is the valid
                // constrained form; nothing to report.
                if Self::jsdoc_next_is_type_param_name(&text[close_off + 1..]) {
                    scan = close_off + 1;
                    continue;
                }

                anchors
                    .push(base + Self::jsdoc_template_missing_name_anchor(text, close_off) as u32);
                scan = close_off + 1;
            }
        }

        for anchor in anchors {
            self.ctx.error(
                anchor,
                1,
                diagnostic_messages::UNEXPECTED_TOKEN_A_TYPE_PARAMETER_NAME_WAS_EXPECTED_WITHOUT_CURLY_BRACES.to_string(),
                diagnostic_codes::UNEXPECTED_TOKEN_A_TYPE_PARAMETER_NAME_WAS_EXPECTED_WITHOUT_CURLY_BRACES,
            );
        }
    }

    /// Byte offset of the `}` that balances the leading `{` of `s` (which is
    /// the text immediately after an opening `{`), or `None` if unbalanced.
    fn jsdoc_balanced_close_brace_offset(s: &str) -> Option<usize> {
        let mut depth: i32 = 1;
        for (i, ch) in s.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Whether the first non-trivia glyph in `after_close` starts a JSDoc type
    /// parameter name. Continuation whitespace and leading `*` line markers are
    /// skipped, mirroring tsc's `skipWhitespaceOrAsterisk` before it parses the
    /// name that a constraint (`@template {C} Name`) introduces.
    fn jsdoc_next_is_type_param_name(after_close: &str) -> bool {
        for ch in after_close.chars() {
            if ch == ' ' || ch == '\t' || ch == '\r' || ch == '\n' || ch == '*' {
                continue;
            }
            // `[` opens the bracketed `[Name=default]` form (tsc parses it via
            // `parseOptionalJsdoc(OpenBracketToken)`), so it also introduces a
            // valid type-parameter name.
            return ch == '_' || ch == '$' || ch == '[' || ch.is_ascii_alphabetic();
        }
        false
    }

    /// Byte offset (within the comment text) where TS1069 anchors when a
    /// constrained `@template` tag has no name. tsc reports at the token where
    /// the name was expected: right after the closing brace when more of the
    /// same line follows, or the first non-space glyph of the next continuation
    /// line (the leading `*`) when the constraint ends its line.
    fn jsdoc_template_missing_name_anchor(text: &str, close_off: usize) -> usize {
        let bytes = text.as_bytes();
        let mut i = close_off + 1;
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r') {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'\n' {
            i += 1;
            while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r') {
                i += 1;
            }
            return i;
        }
        close_off + 1
    }
}
