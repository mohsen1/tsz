//! Trigger-context and call-site detection for signature help.
//!
//! This module handles locating the containing call expression (or tagged
//! template), resolving the callee name, and determining the active parameter
//! index for the cursor position.

use super::{CallKind, CallSite, SignatureHelpProvider};
use tsz_parser::{NodeIndex, parser::node::NodeAccess, syntax_kind_ext};

impl<'a> SignatureHelpProvider<'a> {
    /// Resolve the name of the callee for display in signature help.
    /// For `foo(...)` returns "foo", for `obj.method(...)` returns "method",
    /// for `new Foo(...)` returns "Foo".
    pub(super) fn resolve_callee_name(&self, expr_idx: NodeIndex, _call_kind: CallKind) -> String {
        // Try to get identifier text directly (handles simple identifiers)
        if let Some(name) = self.arena.get_identifier_text(expr_idx)
            && !name.is_empty()
        {
            return name.to_string();
        }
        if let Some(node) = self.arena.get(expr_idx) {
            // Property access: obj.method(...)
            if let Some(access) = self.arena.get_access_expr(node) {
                if let Some(name) = self.arena.get_identifier_text(access.name_or_argument)
                    && !name.is_empty()
                {
                    return name.to_string();
                }
                // Source text fallback for property name
                if let Some(pn) = self.arena.get(access.name_or_argument) {
                    let s = pn.pos as usize;
                    let e = pn.end as usize;
                    if s < e && e <= self.source_text.len() {
                        let text = self.source_text[s..e].trim();
                        if !text.is_empty()
                            && text
                                .chars()
                                .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
                        {
                            return text.to_string();
                        }
                    }
                }
            }
        }
        // Fallback: try to extract name from source text
        if let Some(node) = self.arena.get(expr_idx) {
            let start = node.pos as usize;
            let end = node.end as usize;
            if start < end && end <= self.source_text.len() {
                let text = &self.source_text[start..end];
                // For dotted access, take the last segment
                if let Some(dot_pos) = text.rfind('.') {
                    let name = text[dot_pos + 1..].trim();
                    if !name.is_empty()
                        && name
                            .chars()
                            .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
                    {
                        return name.to_string();
                    }
                }
                // For simple identifier, use the whole text
                let trimmed = text.trim();
                if !trimmed.is_empty()
                    && trimmed
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
                {
                    return trimmed.to_string();
                }
            }
        }
        String::new()
    }

    /// Walk up the AST to find the call expression or tagged template containing the cursor.
    pub(super) fn find_containing_call(
        &self,
        start_node: NodeIndex,
        cursor_offset: u32,
    ) -> Option<(NodeIndex, CallSite<'a>, CallKind)> {
        let mut current = start_node;

        // Safety limit to prevent infinite loops
        let mut depth = 0;
        while current.is_some() && depth < 100 {
            if let Some(node) = self.arena.get(current) {
                if (node.kind == syntax_kind_ext::CALL_EXPRESSION
                    || node.kind == syntax_kind_ext::NEW_EXPRESSION)
                    && let Some(data) = self.arena.get_call_expr(node)
                {
                    // Only provide signature help if cursor is after the opening
                    // `(` or `<` of the call. We find the delimiter by scanning
                    // the source text within the call node range.
                    let call_start = node.pos as usize;
                    let call_end = (node.end as usize).min(self.source_text.len());
                    let call_text = &self.source_text[call_start..call_end];
                    let delimiter = if data.type_arguments.is_some() {
                        call_text.find('<').or_else(|| call_text.find('('))
                    } else {
                        call_text.find('(').or_else(|| call_text.find('<'))
                    };
                    if let Some(delim_offset) = delimiter {
                        let delim_pos = (call_start + delim_offset) as u32;
                        if cursor_offset > delim_pos
                            && !self.cursor_after_closed_call_delimiter(
                                call_start,
                                call_text,
                                delim_offset,
                                cursor_offset,
                            )
                        {
                            let kind = if node.kind == syntax_kind_ext::NEW_EXPRESSION {
                                CallKind::New
                            } else {
                                CallKind::Call
                            };
                            return Some((current, CallSite::Regular(data), kind));
                        }
                    }
                }

                // Check for tagged template expression
                if node.kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION
                    && let Some(data) = self.arena.get_tagged_template(node)
                {
                    // Cursor must be strictly inside the template backticks.
                    // tmpl_node.pos may include leading trivia, so find the
                    // actual opening backtick position in the source text.
                    if let Some(tmpl_node) = self.arena.get(data.template) {
                        let tmpl_start = tmpl_node.pos as usize;
                        let tmpl_end = (tmpl_node.end as usize).min(self.source_text.len());
                        let tmpl_text = &self.source_text[tmpl_start..tmpl_end];
                        if let Some(backtick_rel) = tmpl_text.find('`') {
                            let backtick_pos = (tmpl_start + backtick_rel) as u32;
                            // Cursor must be strictly after opening backtick
                            // and strictly before closing backtick.
                            // For incomplete templates (missing closing backtick),
                            // the parser sets tmpl_node.end before the cursor,
                            // so relax the upper bound check.
                            let template_incomplete = tmpl_end <= tmpl_start
                                || self.source_text.as_bytes()[tmpl_end - 1] != b'`';
                            if cursor_offset > backtick_pos
                                && (template_incomplete || cursor_offset < tmpl_node.end)
                            {
                                return Some((
                                    current,
                                    CallSite::TaggedTemplate(data),
                                    CallKind::TaggedTemplate,
                                ));
                            }
                        }
                    }
                }

                // Stop at function boundaries — if the cursor is inside a nested
                // function body (arrow, function expression, method), don't provide
                // signature help for the outer call expression.
                if node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                {
                    // Only stop if the cursor is inside a multi-line function BODY.
                    // For single-line bodies like `foo(() => {/**/})`, still show
                    // signature help since the user is effectively still at the argument.
                    if let Some(fn_data) = self.arena.get_function(node)
                        && let Some(body_node) = self.arena.get(fn_data.body)
                        && cursor_offset >= body_node.pos
                        && cursor_offset <= body_node.end
                    {
                        let body_text =
                            &self.source_text[body_node.pos as usize..body_node.end as usize];
                        if body_text.contains('\n') {
                            return None;
                        }
                    }
                }

                // Move up to parent
                if let Some(extended) = self.arena.get_extended(current) {
                    current = extended.parent;
                } else {
                    break;
                }
            } else {
                break;
            }
            depth += 1;
        }

        None
    }

    /// Return `true` when the cursor lies strictly after the matching closing
    /// `)` of the call delimiter at `open_rel` within `call_text`.
    pub(super) fn cursor_after_closed_call_delimiter(
        &self,
        call_start: usize,
        call_text: &str,
        open_rel: usize,
        cursor_offset: u32,
    ) -> bool {
        let bytes = call_text.as_bytes();
        if open_rel >= bytes.len() || bytes[open_rel] != b'(' {
            return false;
        }

        let mut depth = 1i32;
        let mut cursor = open_rel + 1;
        while cursor < bytes.len() {
            match bytes[cursor] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        let close_pos = (call_start + cursor) as u32;
                        return cursor_offset > close_pos;
                    }
                }
                _ => {}
            }
            cursor += 1;
        }

        false
    }

    /// Determine the active parameter for a tagged template expression.
    ///
    /// For tagged templates like ``tag`text ${expr1} text ${expr2} text``:
    /// - Parameter 0 is always the templateStrings array
    /// - Parameter N (1-based) corresponds to the Nth ${} expression
    /// - Cursor in static template text maps to parameter 0
    /// - Cursor inside ${expr} maps to the corresponding parameter index
    pub(super) fn determine_tagged_template_active_param(
        &self,
        tagged: &tsz_parser::parser::node::TaggedTemplateData,
        cursor_offset: u32,
    ) -> u32 {
        let Some(tmpl_node) = self.arena.get(tagged.template) else {
            return 0;
        };

        // If the template is a NoSubstitutionTemplateLiteral, active param is always 0
        let Some(tmpl_expr) = self.arena.get_template_expr(tmpl_node) else {
            return 0;
        };

        // Use head/literal boundaries to determine active parameter.
        // The head token covers `text${` - cursor before head.end is in template text (param 0).
        // Each span's literal covers `}text${` or `}text` - cursor in literal is in template text (param 0).
        // Everything between head.end and span[i].literal.pos is the expression area (param i+1).
        // This avoids gaps caused by trivia between AST node boundaries.
        let Some(head_node) = self.arena.get(tmpl_expr.head) else {
            return 0;
        };

        // Cursor in head (before the first ${) → param 0 (templateStrings)
        if cursor_offset < head_node.end {
            return 0;
        }

        // Walk spans: region from head.end/prev-literal.end to this literal.pos is expression area
        for (i, &span_idx) in tmpl_expr.template_spans.nodes.iter().enumerate() {
            let Some(span_node) = self.arena.get(span_idx) else {
                continue;
            };
            if let Some(span_data) = self.arena.get_template_span(span_node)
                && let Some(lit_node) = self.arena.get(span_data.literal)
            {
                // Cursor at or before the literal's `}` → in expression area → param i+1
                // The literal starts with `}` which closes the expression; cursor there
                // is still conceptually "at the expression" (matches TypeScript behavior).
                if cursor_offset <= lit_node.pos {
                    return (i + 1) as u32;
                }
                // Cursor within the literal (template text after `}`) → param 0
                if cursor_offset < lit_node.end {
                    return 0;
                }
                // Cursor past this literal → continue to next span
            }
        }

        0
    }
}
