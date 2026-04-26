//! Array/object literal and property assignment emission.
//!
//! Handles array literals, object literals (including single-line detection,
//! method bodies, accessor emission), property assignments, and shorthand properties.

use super::super::*;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_array_literal(&mut self, node: &Node) {
        let Some(array) = self.arena.get_literal_expr(node) else {
            return;
        };

        if array.elements.nodes.is_empty() {
            // Emit any comments inside the brackets (e.g., `[ /* comment */]`).
            let bracket_pos = self.skip_trivia_forward(node.pos, node.end);
            self.write("[");
            if let Some(text) = self.source_text {
                while self.comment_emit_idx < self.all_comments.len() {
                    let c_pos = self.all_comments[self.comment_emit_idx].pos;
                    let c_end = self.all_comments[self.comment_emit_idx].end;
                    if c_pos > bracket_pos && c_end < node.end {
                        self.write_space();
                        if let Ok(comment_text) =
                            crate::safe_slice::slice(text, c_pos as usize, c_end as usize)
                        {
                            self.write_comment_with_reindent(comment_text, Some(c_pos));
                        }
                        self.comment_emit_idx += 1;
                    } else {
                        break;
                    }
                }
            }
            self.write("]");
            return;
        }

        // Preserve multi-line formatting from source.
        // Check for newlines BETWEEN consecutive elements, not within the overall expression.
        // This avoids treating `[, [\n...\n]]` as multi-line when only the nested array
        // is multi-line, not the outer array's element separation.
        let is_multiline = self.source_text.is_some_and(|text| {
            // Check between consecutive elements for newlines
            for i in 0..array.elements.nodes.len().saturating_sub(1) {
                let curr = array.elements.nodes[i];
                let next = array.elements.nodes[i + 1];
                if let (Some(curr_node), Some(next_node)) =
                    (self.arena.get(curr), self.arena.get(next))
                {
                    let curr_end = std::cmp::min(curr_node.end as usize, text.len());
                    let next_start = std::cmp::min(next_node.pos as usize, text.len());
                    if curr_end <= next_start && text[curr_end..next_start].contains('\n') {
                        return true;
                    }
                }
            }
            // Also check between '[' and first element
            let bracket_pos = self.skip_trivia_forward(node.pos, node.end) as usize;
            if let Some(first_node) = array
                .elements
                .nodes
                .first()
                .and_then(|&n| self.arena.get(n))
            {
                let first_pos = std::cmp::min(first_node.pos as usize, text.len());
                let start = std::cmp::min(bracket_pos, first_pos);
                if start < first_pos && text[start..first_pos].contains('\n') {
                    return true;
                }
            } else if !array.elements.nodes.is_empty() {
                // All elements are NONE (elisions); check for newlines in the array body.
                let end = std::cmp::min(node.end as usize, text.len());
                if bracket_pos + 1 < end && text[bracket_pos + 1..end].contains('\n') {
                    return true;
                }
            }
            false
        });
        let has_trailing_comma = self.has_trailing_comma_in_source(node, &array.elements.nodes);

        if !is_multiline {
            // Emit any inline leading comment before the first element.
            // e.g., `[/* comment */ 1]` or `[/* c */ a, b]`
            // Skip for NONE-first (elision) arrays; those comments are trailing, handled below.
            let bracket_pos = self.skip_trivia_forward(node.pos, node.end);
            let first_elem_is_none = array
                .elements
                .nodes
                .first()
                .is_some_and(|&idx| idx.is_none());
            let first_elem_pos = if first_elem_is_none {
                bracket_pos + 1 // empty range → emit nothing as leading
            } else {
                array
                    .elements
                    .nodes
                    .first()
                    .and_then(|&idx| self.arena.get(idx))
                    .map(|n| n.pos)
                    .unwrap_or(node.end)
            };
            self.write("[");
            self.increase_indent();
            self.emit_unemitted_comments_between(bracket_pos + 1, first_elem_pos);
            self.emit_comma_separated(&array.elements.nodes);
            // Preserve trailing comma for elisions: [,,] must keep both commas
            // Elided elements are represented as NodeIndex::NONE, not OMITTED_EXPRESSION nodes
            if has_trailing_comma || array.elements.nodes.last().is_some_and(|idx| idx.is_none()) {
                self.write(",");
            }
            // Emit any trailing inline comments between last element and ']'.
            // e.g., `[1 /* comment */]` or `[1, /* comment */]`
            if let Some(text) = self.source_text {
                while self.comment_emit_idx < self.all_comments.len() {
                    let c_pos = self.all_comments[self.comment_emit_idx].pos;
                    let c_end = self.all_comments[self.comment_emit_idx].end;
                    if c_end < node.end {
                        self.write_space();
                        if let Ok(comment_text) =
                            crate::safe_slice::slice(text, c_pos as usize, c_end as usize)
                        {
                            self.write_comment_with_reindent(comment_text, Some(c_pos));
                        }
                        self.comment_emit_idx += 1;
                    } else {
                        break;
                    }
                }
            }
            self.decrease_indent();
            self.write("]");
        } else {
            // Check if the first element is on a new line after '[' in the source.
            // TypeScript preserves the source formatting:
            // - `[elem1,\n  elem2]` -> first element on same line
            // - `[\n  elem1,\n  elem2\n]` -> first element on new line
            let first_elem_on_new_line = self.source_text.is_some_and(|text| {
                if let Some(first_elem) = array.elements.nodes.first() {
                    if let Some(first_node) = self.arena.get(*first_elem) {
                        let bracket_pos = self.skip_trivia_forward(node.pos, node.end) as usize;
                        let first_pos = first_node.pos as usize;
                        let end = std::cmp::min(first_pos, text.len());
                        let start = std::cmp::min(bracket_pos, end);
                        text[start..end].contains('\n')
                    } else {
                        // NONE (elision) first element: check for newline in array body after '['
                        let bracket_pos = self.skip_trivia_forward(node.pos, node.end) as usize;
                        let end = std::cmp::min(node.end as usize, text.len());
                        bracket_pos + 1 < end && text[bracket_pos + 1..end].contains('\n')
                    }
                } else {
                    false
                }
            });

            if first_elem_on_new_line {
                // Format: [\n  elem1,\n  elem2\n]
                //
                // Key invariant: the comma separator for element i is written AFTER
                // element i's content (and any "pre-separator" comments that precede the
                // comma in the source).  This mirrors TypeScript's emitter which treats
                // the separator comma as a pseudo-token with its own leading trivia.
                self.write("[");
                self.increase_indent();
                let elems: Vec<_> = array.elements.nodes.to_vec();
                let last_idx = elems.len().saturating_sub(1);
                for (i, &elem) in elems.iter().enumerate() {
                    let is_elision = elem.is_none();
                    self.write_line();

                    // --- Step A: emit leading comments before this element ---
                    // Only real elements have source positions; elisions don't.
                    if !is_elision {
                        let actual_start = self
                            .arena
                            .get(elem)
                            .map(|n| self.skip_trivia_forward(n.pos, n.end))
                            .unwrap_or(0);
                        if let Some(text) = self.source_text {
                            while self.comment_emit_idx < self.all_comments.len() {
                                let c_end = self.all_comments[self.comment_emit_idx].end;
                                if c_end <= actual_start {
                                    let c_pos = self.all_comments[self.comment_emit_idx].pos;
                                    if let Ok(comment_text) = crate::safe_slice::slice(
                                        text,
                                        c_pos as usize,
                                        c_end as usize,
                                    ) {
                                        self.write_comment_with_reindent(comment_text, Some(c_pos));
                                    }
                                    // Determine separation from what follows (next comment or element):
                                    // if there's a newline between this comment's end and
                                    // actual_start, put on a new line; otherwise add a space.
                                    let c_end_u = c_end as usize;
                                    let gap_has_newline = c_end_u < actual_start as usize
                                        && text[c_end_u..actual_start as usize].contains('\n');
                                    if gap_has_newline {
                                        self.write_line();
                                    } else {
                                        self.write_space();
                                    }
                                    self.comment_emit_idx += 1;
                                } else {
                                    break;
                                }
                            }
                        }
                    }

                    // --- Step B: emit the element ---
                    self.emit(elem);

                    // --- Step C: emit pre-separator comments then write comma ---
                    // Only needed for non-last elements.
                    if i < last_idx {
                        if is_elision {
                            // Elisions have no content; write comma directly.
                            self.write(",");
                        } else {
                            // Find the separator comma in the source that follows this element.
                            let elem_end = self.arena.end_at(elem).unwrap_or(0);

                            // Some element nodes (e.g. function expressions) include the
                            // trailing comma and whitespace in their `end` span.  In that
                            // case, `find_comma_pos_after(elem_end, ...)` would skip the
                            // real separator and find the NEXT comma.  Detect this by
                            // scanning backward from `elem_end` through trivia for a comma.
                            let comma_already_past = self.source_text.is_some_and(|text| {
                                let bytes = text.as_bytes();
                                let mut j = (elem_end as usize).min(bytes.len());
                                while j > 0 {
                                    j -= 1;
                                    match bytes[j] {
                                        b',' => return true,
                                        b' ' | b'\t' | b'\n' | b'\r' => continue,
                                        _ => return false,
                                    }
                                }
                                false
                            });

                            if comma_already_past {
                                // Comma is within the element's span – just write it.
                                self.write(",");
                            } else {
                                let comma_pos = self.find_comma_pos_after(elem_end, node.end);
                                // Emit any comments between the element's end and the comma.
                                // A comment on its own line → write_line() before it, then ` ,`.
                                // A same-line comment (e.g. `1 /* c */,`) → write_space(), then `,`.
                                let mut wrote_pre_sep = false;
                                let mut last_was_newline_comment = false;
                                if let (Some(sep), Some(text)) = (comma_pos, self.source_text) {
                                    while self.comment_emit_idx < self.all_comments.len() {
                                        let c_pos = self.all_comments[self.comment_emit_idx].pos;
                                        let c_end = self.all_comments[self.comment_emit_idx].end;
                                        if c_pos >= elem_end && c_end <= sep {
                                            let preceded_by_newline =
                                                self.comment_preceded_by_newline(c_pos);
                                            if preceded_by_newline {
                                                self.write_line();
                                            } else {
                                                self.write_space();
                                            }
                                            if let Ok(comment_text) = crate::safe_slice::slice(
                                                text,
                                                c_pos as usize,
                                                c_end as usize,
                                            ) {
                                                self.write_comment_with_reindent(
                                                    comment_text,
                                                    Some(c_pos),
                                                );
                                            }
                                            wrote_pre_sep = true;
                                            last_was_newline_comment = preceded_by_newline;
                                            self.comment_emit_idx += 1;
                                        } else {
                                            break;
                                        }
                                    }
                                }
                                if wrote_pre_sep && last_was_newline_comment {
                                    self.write(" ,");
                                } else {
                                    self.write(",");
                                }
                            }

                            // --- Step D: emit trailing same-line comments after the comma ---
                            // In source like `elem, // comment\n`, the comment follows the
                            // comma on the same line.  Emit it inline so it stays on the
                            // same output line; otherwise Step A of the next element would
                            // place it on its own line.
                            if let Some(text) = self.source_text {
                                let comma_source_pos = self
                                    .find_comma_pos_after(elem_end, node.end)
                                    .unwrap_or(elem_end)
                                    as usize;
                                while self.comment_emit_idx < self.all_comments.len() {
                                    let c_pos =
                                        self.all_comments[self.comment_emit_idx].pos as usize;
                                    let c_end =
                                        self.all_comments[self.comment_emit_idx].end as usize;
                                    // Only consider comments after the comma and on the same
                                    // source line (no newline between comma and comment start).
                                    if c_pos >= comma_source_pos
                                        && c_end <= node.end as usize
                                        && !text[comma_source_pos.min(text.len())
                                            ..c_pos.min(text.len())]
                                            .contains('\n')
                                    {
                                        self.write_space();
                                        if let Ok(comment_text) =
                                            crate::safe_slice::slice(text, c_pos, c_end)
                                        {
                                            self.write_comment_with_reindent(
                                                comment_text,
                                                Some(c_pos as u32),
                                            );
                                        }
                                        self.comment_emit_idx += 1;
                                    } else {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }

                // Trailing comma for elisions (last element is None) or explicit trailing comma.
                if has_trailing_comma
                    || array.elements.nodes.last().is_some_and(|idx| idx.is_none())
                {
                    self.write(",");
                }

                // Emit any comments that appear between the last element and ']'.
                // Same-line comments (e.g. `, /* comment */`) are written inline with a space;
                // comments on their own line are written with write_line().
                if let Some(text) = self.source_text {
                    while self.comment_emit_idx < self.all_comments.len() {
                        let c_pos = self.all_comments[self.comment_emit_idx].pos;
                        let c_end = self.all_comments[self.comment_emit_idx].end;
                        if c_end <= node.end {
                            if self.comment_preceded_by_newline(c_pos) {
                                self.write_line();
                            } else {
                                self.write_space();
                            }
                            if let Ok(comment_text) =
                                crate::safe_slice::slice(text, c_pos as usize, c_end as usize)
                            {
                                self.write_comment_with_reindent(comment_text, Some(c_pos));
                            }
                            self.comment_emit_idx += 1;
                        } else {
                            break;
                        }
                    }
                }

                self.write_line();
                self.decrease_indent();
                self.write("]");
            } else {
                // Format: [elem1,\n  elem2,\n  elem3]
                self.write("[");
                self.emit(array.elements.nodes[0]);
                self.increase_indent();
                for &elem in &array.elements.nodes[1..] {
                    self.write(",");
                    self.write_line();
                    self.emit(elem);
                }
                // Trailing comma for elisions
                if has_trailing_comma
                    || array.elements.nodes.last().is_some_and(|idx| idx.is_none())
                {
                    self.write(",");
                }
                self.decrease_indent();
                self.write("]");
            }
        }
    }

    pub(in crate::emitter) fn emit_object_literal(&mut self, node: &Node) {
        let Some(obj) = self.arena.get_literal_expr(node) else {
            return;
        };

        if obj.elements.nodes.is_empty() {
            self.write("{}");
            return;
        }

        // ES5 computed/spread lowering is handled via TransformDirective::ES5ObjectLiteral.
        // For ES2015-ES2017 targets, object spread must be lowered to Object.assign().
        // (ES2018+ supports native object spread syntax.)
        {
            use super::super::ScriptTarget;
            let has_spread = obj.elements.nodes.iter().any(|&idx| {
                self.arena
                    .get(idx)
                    .is_some_and(|n| n.kind == syntax_kind_ext::SPREAD_ASSIGNMENT)
            });
            let target_num = self.ctx.options.target as u32;
            let es2018_num = ScriptTarget::ES2018 as u32;
            if has_spread && target_num < es2018_num {
                // Target is ES2015/ES2016/ES2017: lower to Object.assign()
                let elems: Vec<NodeIndex> = obj.elements.nodes.to_vec();
                self.emit_object_literal_with_object_assign(&elems);
                return;
            }
        }

        // Check if source had a trailing comma after the last element
        let has_trailing_comma = self.has_trailing_comma_in_source(node, &obj.elements.nodes);

        // Preserve single-line formatting from source by looking only at separators
        // between properties (not inside member bodies).
        let source_single_line = self.source_text.is_some_and(|text| {
            let start = std::cmp::min(node.pos as usize, text.len());
            let end = std::cmp::min(node.end as usize, text.len());
            if start >= end || obj.elements.nodes.is_empty() {
                return false;
            }

            let Some(first_node) = self.arena.get(obj.elements.nodes[0]) else {
                return false;
            };
            let first_pos = std::cmp::min(first_node.pos as usize, text.len());
            if start < first_pos && text[start..first_pos].contains('\n') {
                return false;
            }

            for pair in obj.elements.nodes.windows(2) {
                let Some(curr) = self.arena.get(pair[0]) else {
                    continue;
                };
                let Some(next) = self.arena.get(pair[1]) else {
                    continue;
                };
                let curr_end = std::cmp::min(curr.end as usize, text.len());
                let next_pos = std::cmp::min(next.pos as usize, text.len());
                if curr_end < next_pos && text[curr_end..next_pos].contains('\n') {
                    return false;
                }
            }

            let Some(last_node) = obj
                .elements
                .nodes
                .last()
                .and_then(|&idx| self.arena.get(idx))
            else {
                return false;
            };
            let last_end = std::cmp::min(last_node.end as usize, text.len());
            if last_end < end && text[last_end..end].contains('\n') {
                return false;
            }

            true
        });
        let has_multiline_object_member = if obj.elements.nodes.len() == 1 {
            false
        } else {
            obj.elements.nodes.iter().any(|&prop| {
                let Some(prop_node) = self.arena.get(prop) else {
                    return false;
                };

                match prop_node.kind {
                    k if k == syntax_kind_ext::METHOD_DECLARATION => {
                        let Some(method) = self.arena.get_method_decl(prop_node) else {
                            return false;
                        };
                        if method.body.is_none() {
                            return false;
                        }
                        self.node_text_contains_node(method.body)
                    }
                    k if k == syntax_kind_ext::GET_ACCESSOR => {
                        let Some(accessor) = self.arena.get_accessor(prop_node) else {
                            return false;
                        };
                        if accessor.body.is_none() {
                            return false;
                        }
                        self.node_text_contains_node(accessor.body)
                    }
                    k if k == syntax_kind_ext::SET_ACCESSOR => {
                        let Some(accessor) = self.arena.get_accessor(prop_node) else {
                            return false;
                        };
                        if accessor.body.is_none() {
                            return false;
                        }
                        self.node_text_contains_node(accessor.body)
                    }
                    _ => false,
                }
            })
        };

        if obj.elements.nodes.len() == 1 {
            let prop = obj.elements.nodes[0];
            let Some(prop_node) = self.arena.get(prop) else {
                return;
            };
            let is_callable_member = prop_node.kind == syntax_kind_ext::METHOD_DECLARATION
                || prop_node.kind == syntax_kind_ext::GET_ACCESSOR
                || prop_node.kind == syntax_kind_ext::SET_ACCESSOR;
            if !is_callable_member {
                // Fall through to the regular object-literal formatter so comments/trailing
                // commas on property assignments are preserved.
            } else {
                let newline_before_prop = self.source_text.is_some_and(|text| {
                    let start = std::cmp::min(node.pos as usize, text.len());
                    let prop_start = std::cmp::min(prop_node.pos as usize, text.len());
                    start < prop_start && text[start..prop_start].contains('\n')
                });
                let mut newline_before_close = self.source_text.is_some_and(|text| {
                    let bytes = text.as_bytes();
                    let mut close = std::cmp::min(node.end as usize, text.len());
                    while close > 0 {
                        close -= 1;
                        if bytes[close] == b'}' {
                            break;
                        }
                    }
                    let prop_end = std::cmp::min(prop_node.end as usize, close);
                    prop_end < close && text[prop_end..close].contains('\n')
                });
                if !newline_before_close {
                    newline_before_close = self.source_text.is_some_and(|text| {
                        let start = std::cmp::min(node.pos as usize, text.len());
                        let mut close = std::cmp::min(node.end as usize, text.len());
                        let bytes = text.as_bytes();
                        while close > 0 {
                            close -= 1;
                            if bytes[close] == b'}' {
                                break;
                            }
                        }
                        if close <= start {
                            return false;
                        }
                        text[start..close].contains('\n')
                    });
                }

                self.write("{");
                if newline_before_prop {
                    self.write_line();
                    self.increase_indent();
                } else {
                    self.write(" ");
                    self.increase_indent();
                }

                self.emit_object_property(prop);
                if has_trailing_comma {
                    self.write(",");
                }

                // Emit trailing same-line comments before the closing brace,
                // e.g. `[this.bar()]() { } // needs capture`
                let token_end = self.find_token_end_before_trivia(prop_node.pos, prop_node.end);
                // Bound to node.end to avoid consuming comments outside the
                // object literal (e.g. `{ foo() { } }); // comment`).
                self.emit_trailing_comments_before(token_end, node.end);

                if newline_before_prop || newline_before_close {
                    let wrote_newline = self.emit_unemitted_comments_between(token_end, node.end);
                    if !wrote_newline {
                        self.write_line();
                    }
                    self.decrease_indent();
                    self.write("}");
                } else {
                    self.decrease_indent();
                    self.write(" }");
                }
                return;
            }
        }

        let should_emit_single_line = source_single_line && !has_multiline_object_member;
        if should_emit_single_line {
            self.write("{ ");
            for (i, &prop) in obj.elements.nodes.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.emit_object_property(prop);
            }
            if has_trailing_comma {
                self.write(",");
            }
            self.write(" }");
        } else {
            // Multi-line format: preserve original line layout from source
            // TSC keeps properties that are on the same line together
            self.write("{");
            self.write_line();
            self.increase_indent();
            // Find the opening `{` position for leading comment scanning
            let open_brace_end = self.source_text.map_or(node.pos + 1, |text| {
                let bytes = text.as_bytes();
                let start = node.pos as usize;
                let end = std::cmp::min(node.end as usize, bytes.len());
                bytes[start..end]
                    .iter()
                    .position(|&b| b == b'{')
                    .map(|off| (start + off + 1) as u32)
                    .unwrap_or(node.pos + 1)
            });
            for (i, &prop) in obj.elements.nodes.iter().enumerate() {
                let Some(prop_node) = self.arena.get(prop) else {
                    continue;
                };
                // Skip error-recovery shorthand placeholders synthesized when the parser
                // encounters an unexpected non-name token (e.g. extra commas: `{ x: 0,, }`).
                // The synthesized Identifier name has zero width (pos == end) and an empty
                // text — emitting it would produce stray commas in the output.
                if prop_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
                    && let Some(shorthand) = self.arena.get_shorthand_property(prop_node)
                    && let Some(name_node) = self.arena.get(shorthand.name)
                    && name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                    && name_node.pos == name_node.end
                {
                    continue;
                }
                // Emit leading comments before the first property (e.g. /** own x*/)
                if i == 0 {
                    self.emit_unemitted_comments_between(open_brace_end, prop_node.pos);
                }
                self.emit_object_property(prop);

                let is_last = i == obj.elements.nodes.len() - 1;

                // Use token_end (before trivia) for comment scanning.
                // The parser's node.end extends past trailing trivia (comments,
                // whitespace) into the next token's position, so using node.end
                // directly would miss trailing same-line comments.
                let token_end = self.find_token_end_before_trivia(prop_node.pos, prop_node.end);

                // For the last property, has_trailing_comma_in_source may miss
                // commas followed by inline comments (e.g., `x: 1, // comment`)
                // because its backward scan doesn't skip inline comments.
                // As a fallback, check if find_token_end_before_trivia landed
                // right after a comma (it treats commas as non-trivia tokens).
                let needs_comma = !is_last
                    || has_trailing_comma
                    || self.source_text.is_some_and(|text| {
                        let bytes = text.as_bytes();
                        let te = token_end as usize;
                        te > 0 && te <= bytes.len() && bytes[te - 1] == b','
                    });
                if needs_comma {
                    self.write(",");
                }

                // Check if next property is on the same line in source
                if !is_last {
                    let next_prop = obj.elements.nodes[i + 1];
                    let next_pos = self.arena.get(next_prop).map_or(prop_node.end, |n| n.pos);
                    // Check if there's a trailing comment on the same line after the comma
                    // If so, add a space between the comma and the comment
                    let has_same_line_comment = self.source_text.is_some_and(|text| {
                        let from = token_end as usize;
                        let to = std::cmp::min(next_pos as usize, text.len());
                        if from >= to {
                            return false;
                        }
                        let gap = &text[from..to];
                        // Check for comment on same line (no newline before comment start)
                        // Handles both // line comments and /* block comments
                        if let Some(slash_pos) = gap.find("//") {
                            !gap[..slash_pos].contains('\n')
                        } else if let Some(block_pos) = gap.find("/*") {
                            !gap[..block_pos].contains('\n')
                        } else {
                            false
                        }
                    });
                    let same_line = self.are_on_same_line_in_source(prop, next_prop);
                    if has_same_line_comment {
                        // Same-line trailing comment after comma: space before comment
                        self.write(" ");
                    } else if !same_line {
                        // Properties are on different lines and any comment is on
                        // a subsequent line — write a newline first so the comment
                        // appears on its own line (matching tsc).
                        self.write_line();
                    }
                    let wrote_newline = self.emit_unemitted_comments_between(token_end, next_pos);
                    if wrote_newline {
                        // Comment emission already wrote the trailing newline
                    } else if same_line {
                        // Keep on same line
                        self.write(" ");
                    } else if !has_same_line_comment {
                        // We already wrote a newline above; don't double up
                    } else {
                        // Same-line comment that didn't end with a newline,
                        // but properties are on different lines
                        self.write_line();
                    }
                } else {
                    // Last property: emit same-line trailing comments first,
                    // then any remaining comments before closing brace
                    self.emit_trailing_comments(token_end);
                    let wrote_newline = self.emit_unemitted_comments_between(token_end, node.end);
                    if !wrote_newline {
                        self.write_line();
                    }
                }
            }
            self.decrease_indent();
            self.write("}");
        }
    }

    pub(in crate::emitter) fn emit_property_assignment(&mut self, node: &Node) {
        let Some(prop) = self.arena.get_property_assignment(node) else {
            return;
        };

        // Shorthand property: parser creates PROPERTY_ASSIGNMENT with name == initializer
        // (same NodeIndex) for { name } instead of SHORTHAND_PROPERTY_ASSIGNMENT
        let is_shorthand = prop.name == prop.initializer;

        // For ES5 target, expand shorthand properties to full form: { x } → { x: x }
        // ES5 doesn't support shorthand property syntax (ES6 feature)
        if is_shorthand && self.ctx.target_es5 {
            self.emit_decl_name(prop.name);
            self.write(": ");
            self.emit_expression(prop.initializer);
            return;
        }

        // For ES6+ target, preserve shorthand as-is — UNLESS the identifier
        // will be import/export-substituted (e.g., `foo` → `foo_1.foo` or
        // `test` → `exports.test`), which breaks shorthand syntax. In that
        // case, expand to `name: substituted_value`.
        if is_shorthand {
            if let Some(ident) = self.arena.get_identifier(
                self.arena
                    .get(prop.name)
                    .expect("property name NodeIndex must be valid in arena"),
            ) {
                let has_import_subst = !self.suppress_commonjs_named_import_substitution
                    && self
                        .commonjs_named_import_substitutions
                        .contains_key(&ident.escaped_text);
                let has_export_var = !self.suppress_ns_qualification
                    && self
                        .commonjs_exported_var_names
                        .contains(ident.escaped_text.as_str());
                if has_import_subst || has_export_var {
                    // Emit name without substitution, then `: substituted_value`
                    self.write_identifier(&ident.escaped_text);
                    self.write(": ");
                    self.emit(prop.initializer);
                    return;
                }
            }
            self.emit(prop.name);
            return;
        }

        // Regular property: name: value
        // Use emit_decl_name for simple identifier property names to suppress
        // namespace qualification — property names are declarations, not references.
        // Computed property names (e.g., [SYMBOL]) are expressions and should
        // preserve namespace/export qualification.
        let name_node = self.arena.get(prop.name);
        let is_computed = name_node
            .is_some_and(|n| n.kind == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME);
        if is_computed {
            self.emit(prop.name);
        } else {
            self.emit_decl_name(prop.name);
        }
        self.write(": ");
        self.emit_expression(prop.initializer);
    }

    pub(in crate::emitter) fn emit_shorthand_property(&mut self, node: &Node) {
        let Some(shorthand) = self.arena.get_shorthand_property(node) else {
            // Fallback: try to get identifier data directly
            if let Some(ident) = self.arena.get_identifier(node) {
                self.write(&ident.escaped_text);
            }
            return;
        };

        // For ES5 target, expand shorthand properties to full form: { x } → { x: x }
        // ES5 doesn't support shorthand property syntax (ES6 feature)
        if self.ctx.target_es5 {
            self.emit_decl_name(shorthand.name);
            self.write(": ");
            self.emit(shorthand.name);
            if shorthand.equals_token {
                self.write(" = ");
                self.emit(shorthand.object_assignment_initializer);
            }
            return;
        }

        // For ES6+ target, emit shorthand as-is — UNLESS import/export substitution
        // would produce invalid shorthand syntax (e.g., `{ foo_1.foo }` or `{ exports.test }`).
        // Check both import substitutions and exported variable names.
        // Note: don't check is_commonjs() — module kind is temporarily None inside export bodies.
        let name_node = self.arena.get(shorthand.name);
        if let Some(ident) = name_node.and_then(|n| self.arena.get_identifier(n)) {
            let has_import_subst = !self.suppress_commonjs_named_import_substitution
                && self
                    .commonjs_named_import_substitutions
                    .contains_key(&ident.escaped_text);
            let has_export_var = !self.suppress_ns_qualification
                && self
                    .commonjs_exported_var_names
                    .contains(ident.escaped_text.as_str());
            let has_ns_qualification = self.in_namespace_iife
                && !self.suppress_ns_qualification
                && self
                    .namespace_exported_names
                    .contains(ident.escaped_text.as_str());
            if has_import_subst || has_export_var || has_ns_qualification {
                self.write_identifier(&ident.escaped_text);
                self.write(": ");
                self.emit(shorthand.name);
                return;
            }
        }

        // tsc emits `keyword: ` (non-shorthand, empty value) when the name is a reserved
        // keyword, since shorthand property syntax like `{ return }` or `{ class }` is
        // invalid JS. Also handles non-Identifier names (string/number literals).
        let is_non_shorthand_name = name_node.is_some_and(|n| {
            // Not an Identifier node kind (e.g. NumericLiteral, StringLiteral)
            if n.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
                return true;
            }
            // Identifier whose text is a JS reserved keyword
            if let Some(ident) = self.arena.get_identifier(n)
                && let Some(kw) = tsz_scanner::text_to_keyword(&ident.escaped_text)
            {
                return tsz_scanner::token_is_reserved_word(kw);
            }
            false
        });
        if is_non_shorthand_name {
            self.emit(shorthand.name);
            self.write(": ");
            return;
        }

        self.emit(shorthand.name);
        if shorthand.equals_token {
            self.write(" = ");
            self.emit(shorthand.object_assignment_initializer);
        }
    }

    fn node_text_contains_node(&self, node_idx: tsz_parser::parser::NodeIndex) -> bool {
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };
        self.node_text_contains_newline(node.pos as usize, node.end as usize)
    }

    fn node_text_contains_newline(&self, start: usize, end: usize) -> bool {
        self.source_text
            .is_some_and(|text| start < end && end <= text.len() && text[start..end].contains('\n'))
    }

    /// Emit object literal with spread elements as `Object.assign()` for pre-ES2018 targets.
    ///
    /// TypeScript's object spread lowering for ES2015-ES2017:
    /// - `{ ...a }` → `Object.assign({}, a)`
    /// - `{ x: 1, ...a }` → `Object.assign({ x: 1 }, a)`
    /// - `{ ...a, x: 1 }` → `Object.assign(Object.assign({}, a), { x: 1 })`
    /// - `{ ...a, x: 1, ...b }` → `Object.assign(Object.assign(Object.assign({}, a), { x: 1 }), b)`
    ///
    /// The pattern left-folds: each spread/segment adds one more `Object.assign` wrapping.
    fn emit_object_literal_with_object_assign(&mut self, elements: &[NodeIndex]) {
        // Segment elements into alternating spans of regular props and spread elements.
        // Each segment is either a slice of regular properties or a single spread node.
        #[derive(Clone)]
        enum Seg<'a> {
            Props(&'a [NodeIndex]),
            Spread(NodeIndex),
        }

        let mut segs: Vec<Seg<'_>> = Vec::new();
        let mut seg_start = 0usize;
        for (i, &idx) in elements.iter().enumerate() {
            let is_spread = self
                .arena
                .get(idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::SPREAD_ASSIGNMENT);
            if is_spread {
                if seg_start < i {
                    segs.push(Seg::Props(&elements[seg_start..i]));
                }
                segs.push(Seg::Spread(idx));
                seg_start = i + 1;
            }
        }
        if seg_start < elements.len() {
            segs.push(Seg::Props(&elements[seg_start..]));
        }

        // Count how many Object.assign calls we need:
        // one for each spread + one if the first segment is a spread (needs empty {} seed).
        let num_assign = segs.len();
        // Opening parens for left-folding: (num_assign - 1) calls wrapping the first.
        // Write the opening Object.assign( calls.
        for _ in 0..num_assign.saturating_sub(1) {
            self.write("Object.assign(");
        }

        // Emit the first segment (the "seed" accumulator).
        let first_seg = segs.first().cloned();
        match &first_seg {
            Some(Seg::Props(props)) => {
                self.emit_inline_object_props(props);
            }
            Some(Seg::Spread(spread_idx)) => {
                // When the spread expression is a *simple* object literal (no nested
                // spreads), tsc optimizes away the empty `{}` seed:
                //   `{ ...{x: 0} }` → `Object.assign({x: 0})`
                // But if the literal itself contains spreads, it will be lowered to
                // an Object.assign() chain, and using that as the seed would mutate
                // the intermediate result. In that case, wrap with `{}`:
                //   `{ ...{a: 3, ...b}, c: 1 }` → `Object.assign(Object.assign({}, Object.assign({a: 3}, b)), {c: 1})`
                let spread_is_simple_literal = self.arena.get(*spread_idx).is_some_and(|n| {
                    self.arena
                        .get_spread(n)
                        .and_then(|s| self.arena.get(s.expression))
                        .is_some_and(|e| {
                            if e.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                                return false;
                            }
                            // Check that the inner literal has no nested spreads
                            let Some(inner_obj) = self.arena.get_literal_expr(e) else {
                                return false;
                            };
                            !inner_obj.elements.nodes.iter().any(|&idx| {
                                self.arena
                                    .get(idx)
                                    .is_some_and(|n| n.kind == syntax_kind_ext::SPREAD_ASSIGNMENT)
                            })
                        })
                });
                if spread_is_simple_literal {
                    if segs.len() == 1 {
                        // Single spread of simple literal: Object.assign(expr)
                        self.write("Object.assign(");
                        self.emit_spread_expression_node(*spread_idx);
                        self.write(")");
                    } else {
                        // Multiple segments, simple literal first: use expr as seed
                        self.emit_spread_expression_node(*spread_idx);
                    }
                } else {
                    // Non-literal spread: seed is {}
                    self.write("Object.assign({}, ");
                    self.emit_spread_expression_node(*spread_idx);
                    self.write(")");
                }
            }
            None => {
                self.write("{}");
                return;
            }
        }

        // Emit remaining segments, each adding `, seg)` to close one Object.assign.
        for seg in segs.iter().skip(1) {
            self.write(", ");
            match seg {
                Seg::Props(props) => {
                    self.emit_inline_object_props(props);
                }
                Seg::Spread(spread_idx) => {
                    self.emit_spread_expression_node(*spread_idx);
                }
            }
            self.write(")");
        }
    }

    /// Emit `{ prop, prop, ... }` as an inline object literal (no lowering).
    fn emit_inline_object_props(&mut self, props: &[NodeIndex]) {
        self.write("{ ");
        for (i, &prop) in props.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.emit_object_property(prop);
        }
        self.write(" }");
    }

    /// Emit the expression part of a `SPREAD_ASSIGNMENT` node (the `x` in `...x`).
    fn emit_spread_expression_node(&mut self, spread_idx: NodeIndex) {
        if let Some(spread_node) = self.arena.get(spread_idx)
            && let Some(spread) = self.arena.get_spread(spread_node)
        {
            self.emit_expression(spread.expression);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::emitter::Printer;
    use tsz_parser::ParserState;

    /// tsc preserves trailing commas in single-line object literals.
    /// `{ a: 1, b: 2, }` must stay as `{ a: 1, b: 2, }`, not `{ a: 1, b: 2 }`.
    #[test]
    fn trailing_comma_preserved_in_single_line_object_literal() {
        let source = "var o = { a: 1, b: 2, };\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("{ a: 1, b: 2, }"),
            "Trailing comma should be preserved in single-line object literal.\nOutput:\n{output}"
        );
    }

    /// Without a trailing comma in source, no trailing comma should be emitted.
    #[test]
    fn no_trailing_comma_when_source_has_none() {
        let source = "var o = { a: 1, b: 2 };\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("{ a: 1, b: 2 }"),
            "No trailing comma should be added when source has none.\nOutput:\n{output}"
        );
    }

    /// Trailing comma in object binding pattern: `{ b1, } = expr`.
    #[test]
    fn trailing_comma_preserved_in_object_binding_pattern() {
        let source = "var { b1, } = { b1: 1, };\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("{ b1, }"),
            "Trailing comma should be preserved in object binding pattern.\nOutput:\n{output}"
        );
        assert!(
            output.contains("{ b1: 1, }"),
            "Trailing comma should be preserved in object literal initializer.\nOutput:\n{output}"
        );
    }

    /// Trailing comma + inline comment detection: `x: 1, // comment` preserves comma.
    /// `find_token_end_before_trivia` treats `,` as non-trivia, so `token_end` is
    /// past the comma. The fallback comma detection must find it.
    #[test]
    fn trailing_comma_with_inline_comment_detected() {
        let source = "var b = {\n    x: 1, // comment\n};\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        // The trailing comma must be preserved even when followed by an inline comment
        assert!(
            output.contains("x: 1,"),
            "Trailing comma should be preserved.\nOutput:\n{output}"
        );
    }

    /// Block comment between properties on same line should be preserved.
    #[test]
    fn block_comment_between_properties_preserved() {
        let source = "var o = {\n    a: 1, /* trailing */\n    b: 2\n};\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("1, /* trailing */"),
            "Block comment should stay on same line after comma.\nOutput:\n{output}"
        );
    }
}
