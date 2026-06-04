impl Server {
    pub(crate) fn handle_brace(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, _binder, _root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let byte_offset = line_map.position_to_offset(position, &source_text)? as usize;

            let bytes = source_text.as_bytes();
            if byte_offset >= bytes.len() {
                return Some(serde_json::json!([]));
            }

            let ch = bytes[byte_offset];

            // Build a map of which positions are "in code" (not inside strings/comments)
            let code_map = super::build_code_map(bytes);

            if !code_map[byte_offset] {
                return Some(serde_json::json!([]));
            }

            let match_pos = match ch {
                b'{' => super::scan_forward(bytes, &code_map, byte_offset, b'{', b'}'),
                b'(' => super::scan_forward(bytes, &code_map, byte_offset, b'(', b')'),
                b'[' => super::scan_forward(bytes, &code_map, byte_offset, b'[', b']'),
                b'}' => super::scan_backward(bytes, &code_map, byte_offset, b'}', b'{'),
                b')' => super::scan_backward(bytes, &code_map, byte_offset, b')', b'('),
                b']' => super::scan_backward(bytes, &code_map, byte_offset, b']', b'['),
                b'<' | b'>' => {
                    // For angle brackets, use AST-based matching (not text scanning)
                    // because < and > are also comparison operators
                    super::find_angle_bracket_match(&arena, &source_text, byte_offset)
                }
                _ => None,
            };

            if let Some(match_offset) = match_pos {
                let pos1 = line_map.offset_to_position(byte_offset as u32, &source_text);
                let pos1_end = line_map.offset_to_position((byte_offset + 1) as u32, &source_text);
                let pos2 = line_map.offset_to_position(match_offset as u32, &source_text);
                let pos2_end = line_map.offset_to_position((match_offset + 1) as u32, &source_text);

                let span1 = serde_json::json!({
                    "start": {"line": pos1.line + 1, "offset": pos1.character + 1},
                    "end": {"line": pos1_end.line + 1, "offset": pos1_end.character + 1}
                });
                let span2 = serde_json::json!({
                    "start": {"line": pos2.line + 1, "offset": pos2.character + 1},
                    "end": {"line": pos2_end.line + 1, "offset": pos2_end.character + 1}
                });

                // Return sorted by position
                if byte_offset < match_offset {
                    Some(serde_json::json!([span1, span2]))
                } else {
                    Some(serde_json::json!([span2, span1]))
                }
            } else {
                Some(serde_json::json!([]))
            }
        })();
        self.success_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }
}
