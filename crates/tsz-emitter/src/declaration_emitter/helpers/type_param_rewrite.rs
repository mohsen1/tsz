use crate::declaration_emitter::DeclarationEmitter;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn rename_shadowed_infer_type_params_in_text(
        text: &str,
        outer_names: &[String],
    ) -> String {
        if outer_names.is_empty() || !text.contains("infer ") {
            return text.to_string();
        }
        Self::rename_shadowed_infer_type_params_in_conditional(text, outer_names)
    }

    fn rename_shadowed_infer_type_params_in_conditional(
        text: &str,
        outer_names: &[String],
    ) -> String {
        let Some((question, colon)) = Self::find_top_level_conditional_markers(text) else {
            return text.to_string();
        };

        let check_and_extends = text[..question].trim();
        let true_type = text[question + 1..colon].trim();
        let false_type = text[colon + 1..].trim();
        let renames = Self::infer_type_param_renames(check_and_extends, outer_names, text);
        let mut rewritten_check = check_and_extends.to_string();
        for (original, renamed) in &renames {
            rewritten_check = Self::replace_infer_binder(&rewritten_check, original, renamed);
        }

        let mut rewritten_true =
            Self::rename_shadowed_infer_type_params_in_conditional(true_type, outer_names);
        for (original, renamed) in &renames {
            rewritten_true = Self::replace_whole_word(&rewritten_true, original, renamed);
        }

        let rewritten_false =
            Self::rename_shadowed_infer_type_params_in_conditional(false_type, outer_names);
        format!("{rewritten_check} ? {rewritten_true} : {rewritten_false}")
    }

    fn find_top_level_conditional_markers(text: &str) -> Option<(usize, usize)> {
        let bytes = text.as_bytes();
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut brace_depth = 0usize;
        let mut angle_depth = 0usize;
        let mut question = None;
        let mut nested_conditional_depth = 0usize;
        let mut i = 0usize;
        while i < bytes.len() {
            match bytes[i] {
                b'(' => paren_depth += 1,
                b')' => paren_depth = paren_depth.saturating_sub(1),
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b'{' => brace_depth += 1,
                b'}' => brace_depth = brace_depth.saturating_sub(1),
                b'<' => angle_depth += 1,
                b'>' => angle_depth = angle_depth.saturating_sub(1),
                b'?' if paren_depth == 0
                    && bracket_depth == 0
                    && brace_depth == 0
                    && angle_depth == 0 =>
                {
                    if question.is_some() {
                        nested_conditional_depth += 1;
                    } else {
                        question = Some(i);
                    }
                }
                b':' if question.is_some()
                    && paren_depth == 0
                    && bracket_depth == 0
                    && brace_depth == 0
                    && angle_depth == 0 =>
                {
                    if nested_conditional_depth == 0 {
                        return question.map(|q| (q, i));
                    }
                    nested_conditional_depth -= 1;
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    fn infer_type_param_renames(
        text: &str,
        outer_names: &[String],
        full_text: &str,
    ) -> Vec<(String, String)> {
        let mut renames = Vec::new();
        let bytes = text.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if i + 5 <= bytes.len()
                && &bytes[i..i + 5] == b"infer"
                && (i == 0 || !Self::is_ident_char(bytes[i - 1]))
                && (i + 5 == bytes.len() || !Self::is_ident_char(bytes[i + 5]))
            {
                let mut name_start = i + 5;
                while name_start < bytes.len() && bytes[name_start].is_ascii_whitespace() {
                    name_start += 1;
                }
                let mut name_end = name_start;
                while name_end < bytes.len() && Self::is_ident_char(bytes[name_end]) {
                    name_end += 1;
                }
                if name_end > name_start {
                    let name = &text[name_start..name_end];
                    if outer_names.iter().any(|outer| outer == name)
                        && !renames.iter().any(|(original, _)| original == name)
                    {
                        renames.push((
                            name.to_string(),
                            Self::fresh_shadowed_type_param_name(
                                name,
                                outer_names,
                                &renames,
                                full_text,
                            ),
                        ));
                    }
                }
                i = name_end;
            } else {
                i += 1;
            }
        }
        renames
    }

    fn fresh_shadowed_type_param_name(
        name: &str,
        outer_names: &[String],
        renames: &[(String, String)],
        text: &str,
    ) -> String {
        let mut suffix = 1u32;
        loop {
            let candidate = format!("{name}_{suffix}");
            if !outer_names.contains(&candidate)
                && !renames.iter().any(|(_, renamed)| renamed == &candidate)
                && !Self::contains_whole_word(text, &candidate)
            {
                return candidate;
            }
            suffix += 1;
        }
    }

    fn replace_infer_binder(text: &str, original: &str, renamed: &str) -> String {
        let mut result = String::with_capacity(text.len() + renamed.len());
        let bytes = text.as_bytes();
        let original_bytes = original.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if i + 5 <= bytes.len()
                && &bytes[i..i + 5] == b"infer"
                && (i == 0 || !Self::is_ident_char(bytes[i - 1]))
                && (i + 5 == bytes.len() || !Self::is_ident_char(bytes[i + 5]))
            {
                let mut name_start = i + 5;
                while name_start < bytes.len() && bytes[name_start].is_ascii_whitespace() {
                    name_start += 1;
                }
                let name_end = name_start + original_bytes.len();
                if name_end <= bytes.len()
                    && &bytes[name_start..name_end] == original_bytes
                    && (name_end == bytes.len() || !Self::is_ident_char(bytes[name_end]))
                {
                    result.push_str(&text[i..name_start]);
                    result.push_str(renamed);
                    i = name_end;
                    continue;
                }
            }
            result.push(bytes[i] as char);
            i += 1;
        }
        result
    }

    fn contains_whole_word(text: &str, word: &str) -> bool {
        let bytes = text.as_bytes();
        let word_bytes = word.as_bytes();
        let word_len = word_bytes.len();
        let mut i = 0usize;
        while i + word_len <= bytes.len() {
            if &bytes[i..i + word_len] == word_bytes
                && (i == 0 || !Self::is_ident_char(bytes[i - 1]))
                && (i + word_len == bytes.len() || !Self::is_ident_char(bytes[i + word_len]))
            {
                return true;
            }
            i += 1;
        }
        false
    }
}
