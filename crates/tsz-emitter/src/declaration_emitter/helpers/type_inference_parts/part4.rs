use super::*;

impl<'a> DeclarationEmitter<'a> {
    /// Removes redundant parentheses that wrap an *entire* reused
    /// type-annotation text. `tsc`'s declaration printer never parenthesizes
    /// the outermost type, so a callee return annotation written as `(T)`,
    /// `((T))`, or `(  T  )` must surface as `T` when reused as an inferred
    /// declaration type.
    ///
    /// Only fully-enclosing outer parentheses are removed (via the shared,
    /// quote-/escape-aware [`Self::strip_balanced_outer_parens`] scanner).
    /// Parentheses that are merely an operand of a larger type (`(A | B)[]`,
    /// `Array<(A | B)>`) do not wrap the whole text and are left untouched, and
    /// disjoint groups such as `(A) | (B)` are preserved because the leading `(`
    /// does not pair with the trailing `)`.
    pub(in crate::declaration_emitter) fn strip_redundant_outer_type_parentheses(
        type_text: &str,
    ) -> String {
        let mut current = type_text.trim();
        while let Some(inner) = Self::strip_balanced_outer_parens(current) {
            current = inner.trim();
        }
        current.to_string()
    }

    pub(in crate::declaration_emitter) fn format_reused_call_structural_return_type_text(
        &self,
        type_text: &str,
    ) -> String {
        if !type_text.contains(" & ") || !type_text.contains("=> {") {
            return type_text.to_string();
        }

        let mut out = String::with_capacity(type_text.len() + 16);
        let mut rest = type_text;
        let member_indent = "    ".repeat((self.indent_level + 1) as usize);
        let closing_indent = "    ".repeat(self.indent_level as usize);

        while let Some(start) = rest.find("=> {") {
            let (before, after_marker) = rest.split_at(start + 4);
            out.push_str(before);
            let Some(end) = after_marker.find('}') else {
                out.push_str(after_marker);
                return out;
            };
            let body = after_marker[..end].trim();
            if body.is_empty()
                || body.contains('\n')
                || body.contains(';')
                || body.contains(',')
                || !body.contains(':')
            {
                out.push_str(&after_marker[..=end]);
                rest = &after_marker[end + 1..];
                continue;
            }

            let member = body.trim_end_matches(';').trim();
            out.push('\n');
            out.push_str(&member_indent);
            out.push_str(member);
            out.push(';');
            out.push('\n');
            out.push_str(&closing_indent);
            out.push('}');
            rest = &after_marker[end + 1..];
        }

        out.push_str(rest);
        out
    }

    pub(in crate::declaration_emitter) fn preserve_literal_mapped_return_type_substitutions(
        &self,
        source_arena: &NodeArena,
        parameters: &NodeList,
        call: &tsz_parser::parser::node::CallExprData,
        type_param_names: &[String],
        substitutions: &mut Vec<(String, String)>,
    ) {
        let Some(args) = call.arguments.as_ref() else {
            return;
        };

        for (&param_idx, &arg_idx) in parameters.nodes.iter().zip(args.nodes.iter()) {
            let Some(param_node) = source_arena.get(param_idx) else {
                continue;
            };
            let Some(param) = source_arena.get_parameter(param_node) else {
                continue;
            };
            let Some(param_type_text) = self
                .emit_type_node_text_from_arena(source_arena, param.type_annotation)
                .or_else(|| self.source_slice_from_arena(source_arena, param.type_annotation))
            else {
                continue;
            };
            let param_type_text = param_type_text.trim();
            if !type_param_names
                .iter()
                .any(|name| name.as_str() == param_type_text)
            {
                continue;
            }
            let Some(substitution_text) = self
                .enclosing_parameter_type_annotation_text_for_identifier(arg_idx)
                .or_else(|| self.reference_declared_type_annotation_text(arg_idx))
                .filter(|text| Self::simple_type_reference_name(text).is_some())
                .or_else(|| self.const_literal_initializer_text(arg_idx))
            else {
                continue;
            };
            if let Some((_, existing)) = substitutions
                .iter_mut()
                .find(|(name, _)| name.as_str() == param_type_text)
            {
                *existing = substitution_text;
            } else {
                substitutions.push((param_type_text.to_string(), substitution_text));
            }
        }
    }

    pub(in crate::declaration_emitter) fn enclosing_parameter_type_annotation_text_for_identifier(
        &self,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        let arg_name = self.get_identifier_text(arg_idx)?;
        let mut current = arg_idx;
        for _ in 0..32 {
            let parent_idx = self.arena.parent_of(current)?;
            let parent_node = self.arena.get(parent_idx)?;
            if let Some(func) = self.arena.get_function(parent_node) {
                for &param_idx in &func.parameters.nodes {
                    let param_node = self.arena.get(param_idx)?;
                    let param = self.arena.get_parameter(param_node)?;
                    if self.get_identifier_text(param.name).as_deref() == Some(arg_name.as_str()) {
                        return self
                            .type_annotation_text_from_arena_node(self.arena, param.type_annotation)
                            .or_else(|| {
                                self.source_slice_from_arena(self.arena, param.type_annotation)
                            })
                            .map(|text| text.trim().to_string());
                    }
                }
                return None;
            }
            current = parent_idx;
        }
        None
    }

    pub(in crate::declaration_emitter) fn ensure_single_line_type_literal_member_semicolon(
        type_text: &str,
    ) -> String {
        let trimmed = type_text.trim();
        if trimmed.contains('\n') {
            return type_text.to_string();
        }
        let Some(inner) = trimmed
            .strip_prefix('{')
            .and_then(|text| text.strip_suffix('}'))
            .map(str::trim)
        else {
            return type_text.to_string();
        };
        if inner.is_empty() || inner.ends_with(';') || inner.contains(';') || !inner.contains(':') {
            type_text.to_string()
        } else {
            format!("{{ {inner}; }}")
        }
    }
}

#[cfg(test)]
mod strip_outer_parentheses_tests {
    use crate::declaration_emitter::DeclarationEmitter;

    fn strip(text: &str) -> String {
        DeclarationEmitter::strip_redundant_outer_type_parentheses(text)
    }

    #[test]
    fn removes_fully_enclosing_outer_parentheses() {
        assert_eq!(strip("(Cond<string>)"), "Cond<string>");
        assert_eq!(strip("((Cond<string>))"), "Cond<string>");
        assert_eq!(strip("(  Cond<string>  )"), "Cond<string>");
        assert_eq!(strip("(() => void)"), "() => void");
        assert_eq!(strip("(A | B)"), "A | B");
    }

    #[test]
    fn leaves_unparenthesized_text_unchanged() {
        assert_eq!(strip("Cond<string>"), "Cond<string>");
        assert_eq!(strip("() => void"), "() => void");
        assert_eq!(strip("A | B"), "A | B");
    }

    #[test]
    fn preserves_parentheses_that_do_not_wrap_the_whole_type() {
        // Operand parentheses inside a larger type are not the outermost wrap.
        assert_eq!(strip("(() => void)[]"), "(() => void)[]");
        assert_eq!(strip("(A | B)[]"), "(A | B)[]");
        assert_eq!(strip("Array<(A | B)>"), "Array<(A | B)>");
        assert_eq!(strip("(A & B) & C"), "(A & B) & C");
        // Disjoint groups: the leading `(` does not pair with the trailing `)`.
        assert_eq!(strip("(A) | (B)"), "(A) | (B)");
    }

    #[test]
    fn ignores_parentheses_inside_string_and_import_segments() {
        // Literal-type strings and import specifiers may contain parens that must
        // not skew the enclosing-pair detection.
        assert_eq!(strip("(import(\"./m\").Foo)"), "import(\"./m\").Foo");
        assert_eq!(strip("\"(\" | \")\""), "\"(\" | \")\"");
        assert_eq!(strip("(\"(\" | \")\")"), "\"(\" | \")\"");
    }
}
