//! File management handlers for tsz-server.
//!
//! Handles commands for opening, closing, changing, and updating open files.

use super::{Server, TsServerRequest, TsServerResponse};

impl Server {
    pub(crate) fn handle_open(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        let file = request.arguments.get("file").and_then(|v| v.as_str());
        let content = request
            .arguments
            .get("fileContent")
            .and_then(|v| v.as_str());

        if let Some(file_path) = file {
            let text = if let Some(c) = content {
                c.to_string()
            } else {
                std::fs::read_to_string(file_path).unwrap_or_default()
            };
            self.open_files.insert(file_path.to_string(), text);
        }

        TsServerResponse {
            seq,
            msg_type: "response".to_string(),
            command: "open".to_string(),
            request_seq: request.seq,
            success: true,
            message: None,
            body: None,
        }
    }

    pub(crate) fn handle_close(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        let file = request.arguments.get("file").and_then(|v| v.as_str());
        if let Some(file_path) = file {
            self.open_files.remove(file_path);
        }

        TsServerResponse {
            seq,
            msg_type: "response".to_string(),
            command: "close".to_string(),
            request_seq: request.seq,
            success: true,
            message: None,
            body: None,
        }
    }

    pub(crate) fn handle_change(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let args = &request.arguments;
        let file = args.get("file").and_then(|v| v.as_str());

        if let Some(file_path) = file {
            let line = args
                .get("line")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(1) as u32;
            let offset = args
                .get("offset")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(1) as u32;
            let end_line = args
                .get("endLine")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(line as u64) as u32;
            let end_offset = args
                .get("endOffset")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(offset as u64) as u32;
            let insert_string = args
                .get("insertString")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if let Some(content) = self.open_files.get(file_path).cloned() {
                let new_content =
                    Self::apply_change(&content, line, offset, end_line, end_offset, insert_string);
                self.open_files.insert(file_path.to_string(), new_content);
            }
        }

        TsServerResponse {
            seq,
            msg_type: "response".to_string(),
            command: "change".to_string(),
            request_seq: request.seq,
            success: true,
            message: None,
            body: None,
        }
    }

    /// Apply a text change to file content.
    pub(crate) fn apply_change(
        content: &str,
        line: u32,
        offset: u32,
        end_line: u32,
        end_offset: u32,
        insert_string: &str,
    ) -> String {
        let start_byte = Self::line_offset_to_byte(content, line, offset);
        let end_byte = Self::line_offset_to_byte(content, end_line, end_offset);
        let mut result = String::with_capacity(
            content
                .len()
                .saturating_sub(end_byte.saturating_sub(start_byte))
                .saturating_add(insert_string.len()),
        );
        result.push_str(&content[..start_byte]);
        result.push_str(insert_string);
        result.push_str(&content[end_byte..]);
        result
    }

    /// Convert 1-based line/offset to a byte offset in the content string.
    pub(crate) fn line_offset_to_byte(content: &str, line: u32, offset: u32) -> usize {
        let target_line = (line as usize).saturating_sub(1);
        let target_col = (offset as usize).saturating_sub(1);
        let mut current_line = 0usize;
        let mut line_start = 0usize;
        if target_line > 0 {
            for (i, ch) in content.char_indices() {
                if ch == '\n' {
                    current_line += 1;
                    if current_line == target_line {
                        line_start = i + 1;
                        break;
                    }
                }
            }
            if current_line < target_line {
                return content.len();
            }
        }
        let mut byte_pos = line_start;
        for _ in 0..target_col {
            match content[byte_pos..].chars().next() {
                Some(c) if c != '\n' => byte_pos += c.len_utf8(),
                _ => break,
            }
        }
        byte_pos.min(content.len())
    }

    pub(crate) fn handle_update_open(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        // Process opened, changed, and closed files
        if let Some(opened) = request
            .arguments
            .get("openFiles")
            .and_then(|v| v.as_array())
        {
            for entry in opened {
                if let (Some(file), Some(content)) = (
                    entry.get("file").and_then(|v| v.as_str()),
                    entry.get("fileContent").and_then(|v| v.as_str()),
                ) {
                    self.open_files
                        .insert(file.to_string(), content.to_string());
                }
            }
        }
        if let Some(closed) = request
            .arguments
            .get("closedFiles")
            .and_then(|v| v.as_array())
        {
            for entry in closed {
                if let Some(file) = entry.as_str() {
                    self.open_files.remove(file);
                }
            }
        }

        self.stub_response(seq, request, None)
    }
}
