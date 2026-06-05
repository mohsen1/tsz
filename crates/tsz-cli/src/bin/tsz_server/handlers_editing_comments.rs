//! Comment editing handlers for tsz-server.

use super::{Server, TsServerRequest, TsServerResponse};

impl Server {
    pub(crate) fn handle_toggle_line_comment(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let start_line = request.arguments.get("startLine")?.as_u64()? as usize;
            let end_line = request.arguments.get("endLine")?.as_u64()? as usize;
            let end_offset = request
                .arguments
                .get("endOffset")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(1) as usize;
            let source_text = self.open_files.get(file)?.clone();

            let all_lines: Vec<&str> = source_text.lines().collect();
            // Convert 1-based to 0-based
            let first = start_line.saturating_sub(1);
            let mut last = end_line
                .saturating_sub(1)
                .min(all_lines.len().saturating_sub(1));

            // When the selection ends at the beginning of a line (offset 1),
            // exclude that line from commenting (TypeScript behavior)
            if first != last && end_offset == 1 && last > 0 {
                last -= 1;
            }

            // Collect the lines in range, skipping empty lines for analysis
            let non_empty_lines: Vec<(usize, &str)> = (first..=last)
                .filter_map(|i| {
                    let line = all_lines.get(i)?;
                    if line.trim().is_empty() {
                        None
                    } else {
                        Some((i, *line))
                    }
                })
                .collect();

            if non_empty_lines.is_empty() {
                return Some(serde_json::json!([]));
            }

            // Check if ALL non-empty lines are commented (start with //)
            let all_commented = non_empty_lines
                .iter()
                .all(|(_, line)| line.trim_start().starts_with("//"));

            let mut edits = Vec::new();

            if all_commented {
                // Uncomment: remove the // and one preceding space if present
                for &(line_idx, line) in &non_empty_lines {
                    let ws_len = line.len() - line.trim_start().len();
                    let rest = &line[ws_len..];
                    if rest.starts_with("//") {
                        let one_line = line_idx + 1; // 1-based
                        let start_col = ws_len;
                        let end_col = ws_len + 2; // past the //
                        edits.push(serde_json::json!({
                            "start": {"line": one_line, "offset": start_col + 1},
                            "end": {"line": one_line, "offset": end_col + 1},
                            "newText": ""
                        }));
                    }
                }
            } else {
                // Comment: insert // replacing one space at min_indent position
                let min_indent = non_empty_lines
                    .iter()
                    .map(|(_, line)| line.len() - line.trim_start().len())
                    .min()
                    .unwrap_or(0);

                for &(line_idx, _) in &non_empty_lines {
                    let one_line = line_idx + 1; // 1-based
                    // Insert // at min_indent position (zero-length insertion)
                    let insert_col = min_indent + 1; // 1-based offset
                    edits.push(serde_json::json!({
                        "start": {"line": one_line, "offset": insert_col},
                        "end": {"line": one_line, "offset": insert_col},
                        "newText": "//"
                    }));
                }
            }

            Some(Self::comment_edits_for_protocol(
                request,
                &source_text,
                edits,
            ))
        })();
        self.success_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_toggle_multiline_comment(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let start_line = request.arguments.get("startLine")?.as_u64()? as usize;
            let start_offset = request.arguments.get("startOffset")?.as_u64()? as usize;
            let end_line = request.arguments.get("endLine")?.as_u64()? as usize;
            let end_offset = request.arguments.get("endOffset")?.as_u64()? as usize;
            let source_text = self.open_files.get(file)?.clone();

            // Compute byte offsets from 1-based line/offset
            let sel_start =
                Self::line_offset_to_byte(&source_text, start_line as u32, start_offset as u32);
            let sel_end =
                Self::line_offset_to_byte(&source_text, end_line as u32, end_offset as u32);
            let lines: Vec<&str> = source_text.lines().collect();

            // Find all /* */ comment ranges in the source
            let comment_ranges = Self::find_multiline_comments(&source_text);

            // Check if selection is fully inside an existing comment
            let enclosing = comment_ranges
                .iter()
                .find(|(cs, ce)| *cs <= sel_start && sel_end <= *ce);

            // Find comments that overlap with the selection
            let overlapping: Vec<(usize, usize)> = comment_ranges
                .iter()
                .filter(|(cs, ce)| *cs < sel_end && *ce > sel_start)
                .map(|&(cs, ce)| (cs, ce))
                .collect();

            // Check if selection contains only comments and whitespace
            let only_comments_and_ws = if !overlapping.is_empty() && sel_start != sel_end {
                let mut all_covered = true;
                let mut pos = sel_start;
                for &(cs, ce) in &overlapping {
                    // Check non-comment text before this comment
                    let gap_start = pos.max(sel_start);
                    let gap_end = cs.max(sel_start).min(sel_end);
                    if gap_start < gap_end {
                        let gap = &source_text[gap_start..gap_end];
                        if gap.chars().any(|c| !c.is_whitespace()) {
                            all_covered = false;
                            break;
                        }
                    }
                    pos = ce;
                }
                if all_covered && pos < sel_end {
                    let gap = &source_text[pos..sel_end];
                    if gap.chars().any(|c| !c.is_whitespace()) {
                        all_covered = false;
                    }
                }
                all_covered
            } else {
                false
            };

            let mut edits = Vec::new();

            if let Some(&(comment_start, comment_end)) = enclosing {
                // Selection is inside an existing comment → remove the comment
                // Remove /* at comment_start
                let (sl, so) = Self::byte_to_line_offset(&lines, comment_start)?;
                edits.push(serde_json::json!({
                    "start": {"line": sl, "offset": so},
                    "end": {"line": sl, "offset": so + 2},
                    "newText": ""
                }));
                // Remove */ at comment_end - 2
                let close_pos = comment_end - 2;
                let (el, eo) = Self::byte_to_line_offset(&lines, close_pos)?;
                edits.push(serde_json::json!({
                    "start": {"line": el, "offset": eo},
                    "end": {"line": el, "offset": eo + 2},
                    "newText": ""
                }));
            } else if only_comments_and_ws {
                // Selection only contains comments and whitespace → remove all comments
                // Process in reverse order to preserve positions
                for &(cs, ce) in overlapping.iter().rev() {
                    let close_pos = ce - 2;
                    let (el, eo) = Self::byte_to_line_offset(&lines, close_pos)?;
                    edits.push(serde_json::json!({
                        "start": {"line": el, "offset": eo},
                        "end": {"line": el, "offset": eo + 2},
                        "newText": ""
                    }));
                    let (sl, so) = Self::byte_to_line_offset(&lines, cs)?;
                    edits.push(serde_json::json!({
                        "start": {"line": sl, "offset": so},
                        "end": {"line": sl, "offset": so + 2},
                        "newText": ""
                    }));
                }
            } else if sel_start == sel_end {
                // Empty selection, not inside a comment → insert /**/
                let (sl, so) = Self::byte_to_line_offset(&lines, sel_start)?;
                edits.push(serde_json::json!({
                    "start": {"line": sl, "offset": so},
                    "end": {"line": sl, "offset": so},
                    "newText": "/**/"
                }));
            } else {
                // Selection not inside a comment → wrap with /* */
                // Handle any existing /* or */ inside the selection by
                // closing and reopening comments around them
                if overlapping.is_empty() {
                    // Simple case: no existing comments in selection
                    let (sl, so) = Self::byte_to_line_offset(&lines, sel_start)?;
                    let (el, eo) = Self::byte_to_line_offset(&lines, sel_end)?;
                    edits.push(serde_json::json!({
                        "start": {"line": sl, "offset": so},
                        "end": {"line": sl, "offset": so},
                        "newText": "/*"
                    }));
                    edits.push(serde_json::json!({
                        "start": {"line": el, "offset": eo},
                        "end": {"line": el, "offset": eo},
                        "newText": "*/"
                    }));
                } else {
                    // Complex case: close and reopen around existing comment boundaries
                    let (sl, so) = Self::byte_to_line_offset(&lines, sel_start)?;
                    edits.push(serde_json::json!({
                        "start": {"line": sl, "offset": so},
                        "end": {"line": sl, "offset": so},
                        "newText": "/*"
                    }));

                    for &(cs, ce) in &overlapping {
                        if cs > sel_start && cs < sel_end {
                            // Close our comment before the existing /*
                            let (cl, co) = Self::byte_to_line_offset(&lines, cs)?;
                            edits.push(serde_json::json!({
                                "start": {"line": cl, "offset": co},
                                "end": {"line": cl, "offset": co},
                                "newText": "*/"
                            }));
                        }
                        if ce > sel_start && ce < sel_end {
                            // Reopen our comment after the existing */
                            let (cl, co) = Self::byte_to_line_offset(&lines, ce)?;
                            edits.push(serde_json::json!({
                                "start": {"line": cl, "offset": co},
                                "end": {"line": cl, "offset": co},
                                "newText": "/*"
                            }));
                        }
                    }

                    let (el, eo) = Self::byte_to_line_offset(&lines, sel_end)?;
                    edits.push(serde_json::json!({
                        "start": {"line": el, "offset": eo},
                        "end": {"line": el, "offset": eo},
                        "newText": "*/"
                    }));
                }
            }

            Some(Self::comment_edits_for_protocol(
                request,
                &source_text,
                edits,
            ))
        })();
        self.success_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    fn comment_edits_for_protocol(
        request: &TsServerRequest,
        source_text: &str,
        edits: Vec<serde_json::Value>,
    ) -> serde_json::Value {
        if !request.command.ends_with("-full") {
            return serde_json::json!(edits);
        }

        let text_changes: Vec<serde_json::Value> = edits
            .into_iter()
            .filter_map(|edit| {
                let start = edit.get("start")?;
                let end = edit.get("end")?;
                let start_line = start.get("line")?.as_u64()? as u32;
                let start_offset = start.get("offset")?.as_u64()? as u32;
                let end_line = end.get("line")?.as_u64()? as u32;
                let end_offset = end.get("offset")?.as_u64()? as u32;
                let new_text = edit
                    .get("newText")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!(""));
                let start_byte = Self::line_offset_to_byte(source_text, start_line, start_offset);
                let end_byte = Self::line_offset_to_byte(source_text, end_line, end_offset);

                Some(serde_json::json!({
                    "newText": new_text,
                    "span": {
                        "start": start_byte,
                        "length": end_byte.saturating_sub(start_byte)
                    }
                }))
            })
            .collect();

        serde_json::json!(text_changes)
    }

    /// Convert byte position to 1-based line/offset for multiline comment handler
    fn byte_to_line_offset(lines: &[&str], byte_pos: usize) -> Option<(usize, usize)> {
        let mut pos = 0usize;
        for (i, l) in lines.iter().enumerate() {
            let line_end = pos + l.len();
            if byte_pos <= line_end {
                return Some((i + 1, byte_pos - pos + 1)); // 1-based
            }
            pos = line_end + 1; // +1 for \n
        }
        None
    }

    /// Find all /* */ comment ranges as (start, end) byte positions
    fn find_multiline_comments(text: &str) -> Vec<(usize, usize)> {
        let bytes = text.as_bytes();
        let mut ranges = Vec::new();
        let mut i = 0;
        while i < bytes.len().saturating_sub(1) {
            if bytes[i] == b'/' && bytes[i + 1] == b'*' {
                let start = i;
                i += 2;
                // Find matching */
                while i < bytes.len().saturating_sub(1) {
                    if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        i += 2;
                        ranges.push((start, i));
                        break;
                    }
                    i += 1;
                }
            } else if bytes[i] == b'/' && bytes[i + 1] == b'/' {
                // Skip single-line comments to avoid matching /* inside them
                i += 2;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            } else if bytes[i] == b'"' || bytes[i] == b'\'' || bytes[i] == b'`' {
                // Skip string literals to avoid matching /* inside strings
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2; // skip escaped char
                        continue;
                    }
                    if bytes[i] == quote {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            } else {
                i += 1;
            }
        }
        ranges
    }

    pub(crate) fn handle_comment_selection(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let start_line = request.arguments.get("startLine")?.as_u64()? as usize;
            let start_offset = request.arguments.get("startOffset")?.as_u64()? as usize;
            let end_line = request.arguments.get("endLine")?.as_u64()? as usize;
            let end_offset = request.arguments.get("endOffset")?.as_u64()? as usize;
            let source_text = self.open_files.get(file)?.clone();

            let all_lines: Vec<&str> = source_text.lines().collect();
            let first = start_line.saturating_sub(1);
            let last = end_line
                .saturating_sub(1)
                .min(all_lines.len().saturating_sub(1));

            let mut edits = Vec::new();

            if first == last && start_offset != end_offset {
                // Single-line partial selection: use block comment /* ... */
                let line = all_lines.get(first)?;
                let sel_start = start_offset.saturating_sub(1);
                let sel_end = end_offset.saturating_sub(1).min(line.len());
                if sel_start < sel_end && sel_start < line.len() {
                    // Wrap selection in /* */
                    edits.push(serde_json::json!({
                        "start": {"line": start_line, "offset": start_offset},
                        "end": {"line": start_line, "offset": start_offset},
                        "newText": "/*"
                    }));
                    // After inserting /*, the end offset shifts by 2
                    edits.push(serde_json::json!({
                        "start": {"line": end_line, "offset": end_offset},
                        "end": {"line": end_line, "offset": end_offset},
                        "newText": "*/"
                    }));
                }
            } else {
                // Multi-line or cursor: add // to each non-empty line
                let non_empty_lines: Vec<(usize, &str)> = (first..=last)
                    .filter_map(|i| {
                        let line = all_lines.get(i)?;
                        if line.trim().is_empty() {
                            None
                        } else {
                            Some((i, *line))
                        }
                    })
                    .collect();

                if non_empty_lines.is_empty() {
                    return Some(serde_json::json!([]));
                }

                let min_indent = non_empty_lines
                    .iter()
                    .map(|(_, line)| line.len() - line.trim_start().len())
                    .min()
                    .unwrap_or(0);

                for &(line_idx, _) in &non_empty_lines {
                    let one_line = line_idx + 1;
                    if min_indent > 0 {
                        // Replace the space at min_indent-1 with //
                        edits.push(serde_json::json!({
                            "start": {"line": one_line, "offset": min_indent},
                            "end": {"line": one_line, "offset": min_indent + 1},
                            "newText": "//"
                        }));
                    } else {
                        edits.push(serde_json::json!({
                            "start": {"line": one_line, "offset": 1},
                            "end": {"line": one_line, "offset": 1},
                            "newText": "//"
                        }));
                    }
                }
            }

            Some(Self::comment_edits_for_protocol(
                request,
                &source_text,
                edits,
            ))
        })();
        self.success_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_uncomment_selection(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let start_line = request.arguments.get("startLine")?.as_u64()? as usize;
            let end_line = request.arguments.get("endLine")?.as_u64()? as usize;
            let source_text = self.open_files.get(file)?.clone();

            let all_lines: Vec<&str> = source_text.lines().collect();
            let first = start_line.saturating_sub(1);
            let last = end_line
                .saturating_sub(1)
                .min(all_lines.len().saturating_sub(1));

            let mut edits = Vec::new();

            // Check for block comments /* */ in the range and remove them
            // Also check for line comments //
            for line_idx in first..=last {
                let line = match all_lines.get(line_idx) {
                    Some(l) => *l,
                    None => continue,
                };

                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                let one_line = line_idx + 1; // 1-based

                // Check for line comment: remove leading //
                let ws_len = line.len() - trimmed.len();
                if trimmed.starts_with("//") {
                    let remove_len = if trimmed.starts_with("// ") { 3 } else { 2 };
                    let start_off = ws_len + 1;
                    edits.push(serde_json::json!({
                        "start": {"line": one_line, "offset": start_off},
                        "end": {"line": one_line, "offset": start_off + remove_len},
                        "newText": ""
                    }));
                    continue;
                }

                // Check for block comments {/* ... */} or /* ... */
                // Find and remove /* and */ pairs
                let mut col = 0;
                let chars: Vec<char> = line.chars().collect();
                while col < chars.len() {
                    if col + 1 < chars.len() && chars[col] == '/' && chars[col + 1] == '*' {
                        // Remove /*
                        edits.push(serde_json::json!({
                            "start": {"line": one_line, "offset": col + 1},
                            "end": {"line": one_line, "offset": col + 3},
                            "newText": ""
                        }));
                        col += 2;
                    } else if col + 1 < chars.len() && chars[col] == '*' && chars[col + 1] == '/' {
                        // Remove */
                        edits.push(serde_json::json!({
                            "start": {"line": one_line, "offset": col + 1},
                            "end": {"line": one_line, "offset": col + 3},
                            "newText": ""
                        }));
                        col += 2;
                    } else {
                        col += 1;
                    }
                }
            }

            Some(Self::comment_edits_for_protocol(
                request,
                &source_text,
                edits,
            ))
        })();
        self.success_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }
}
