//! Structural, format, and miscellaneous handlers for tsz-server.
//!
//! Handles formatting, inlay hints, selection ranges, call hierarchy,
//! outlining spans, brace matching, refactoring stubs, and related commands.

use super::{Server, TsServerRequest, TsServerResponse};
use tsz::lsp::call_hierarchy::CallHierarchyProvider;
use tsz::lsp::folding::FoldingRangeProvider;
use tsz::lsp::inlay_hints::InlayHintsProvider;
use tsz::lsp::position::{LineMap, Position, Range};
use tsz::lsp::selection_range::SelectionRangeProvider;
use tsz::lsp::semantic_tokens::SemanticTokensProvider;
use tsz_solver::TypeInterner;

impl Server {
    pub(crate) fn handle_get_supported_code_fixes(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let codes: Vec<String> = tsz::lsp::code_actions::CodeFixRegistry::supported_error_codes()
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        self.stub_response(seq, request, Some(serde_json::json!(codes)))
    }

    pub(crate) fn handle_encoded_semantic_classifications_full(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&source_text);
            let mut provider =
                SemanticTokensProvider::new(&arena, &binder, &line_map, &source_text);
            let tokens = provider.get_semantic_tokens(root);
            Some(serde_json::json!({
                "spans": tokens,
                "endOfLineState": 0,
            }))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({"spans": [], "endOfLineState": 0}))),
        )
    }

    pub(crate) fn handle_get_applicable_refactors(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    pub(crate) fn handle_get_edits_for_refactor(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!({"edits": []})))
    }

    pub(crate) fn handle_organize_imports(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    pub(crate) fn handle_get_edits_for_file_rename(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    pub(crate) fn handle_format(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let source_text = self.open_files.get(file)?.clone();

            let options = tsz::lsp::formatting::FormattingOptions {
                tab_size: request
                    .arguments
                    .get("options")
                    .and_then(|o| o.get("tabSize"))
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(4) as u32,
                insert_spaces: request
                    .arguments
                    .get("options")
                    .and_then(|o| o.get("insertSpaces"))
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true),
                ..Default::default()
            };

            match tsz::lsp::formatting::DocumentFormattingProvider::format_document(
                file,
                &source_text,
                &options,
            ) {
                Ok(edits) => {
                    let line_map = LineMap::build(&source_text);
                    let body: Vec<serde_json::Value> = edits
                        .iter()
                        .map(|edit| {
                            let (normalized_range, normalized_text) =
                                Self::narrow_to_indentation_only_edit_if_possible(
                                    &source_text,
                                    &line_map,
                                    edit,
                                );
                            serde_json::json!({
                                "start": Self::lsp_to_tsserver_position(normalized_range.start),
                                "end": Self::lsp_to_tsserver_position(normalized_range.end),
                                "newText": normalized_text,
                            })
                        })
                        .collect();
                    Some(serde_json::json!(body))
                }
                Err(_) => Some(serde_json::json!([])),
            }
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    fn narrow_to_indentation_only_edit_if_possible(
        source_text: &str,
        line_map: &LineMap,
        edit: &tsz::lsp::formatting::TextEdit,
    ) -> (Range, String) {
        let Some(start_off) = line_map.position_to_offset(edit.range.start, source_text) else {
            return (edit.range, edit.new_text.clone());
        };
        let Some(end_off) = line_map.position_to_offset(edit.range.end, source_text) else {
            return (edit.range, edit.new_text.clone());
        };
        if start_off >= end_off {
            return (edit.range, edit.new_text.clone());
        }

        let Some(old_text) = source_text.get(start_off as usize..end_off as usize) else {
            return (edit.range, edit.new_text.clone());
        };
        if old_text.contains('\n') || old_text.contains('\r') {
            return (edit.range, edit.new_text.clone());
        }
        if edit.new_text.contains('\n') || edit.new_text.contains('\r') {
            return (edit.range, edit.new_text.clone());
        }

        let old_trimmed = old_text.trim_start_matches([' ', '\t']);
        let new_trimmed = edit.new_text.trim_start_matches([' ', '\t']);
        if old_trimmed != new_trimmed {
            return (edit.range, edit.new_text.clone());
        }

        let old_indent_len = old_text.len().saturating_sub(old_trimmed.len());
        let new_indent_len = edit.new_text.len().saturating_sub(new_trimmed.len());
        if old_indent_len == 0 && new_indent_len == 0 {
            return (edit.range, edit.new_text.clone());
        }

        let indent_start = start_off;
        let indent_end = start_off + old_indent_len as u32;
        let start_pos = line_map.offset_to_position(indent_start, source_text);
        let end_pos = line_map.offset_to_position(indent_end, source_text);
        (
            Range::new(start_pos, end_pos),
            edit.new_text[..new_indent_len].to_string(),
        )
    }

    pub(crate) fn handle_format_on_key(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let source_text = self.open_files.get(file)?.clone();
            let line = request.arguments.get("line")?.as_u64()? as u32;
            let offset = request.arguments.get("offset")?.as_u64()? as u32;
            let key = request.arguments.get("key")?.as_str()?;

            let options = tsz::lsp::formatting::FormattingOptions {
                tab_size: request
                    .arguments
                    .get("options")
                    .and_then(|o| o.get("tabSize"))
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(4) as u32,
                insert_spaces: request
                    .arguments
                    .get("options")
                    .and_then(|o| o.get("insertSpaces"))
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true),
                ..Default::default()
            };

            // tsserver protocol uses 1-based line/offset, convert to 0-based
            let lsp_line = line.saturating_sub(1);
            let lsp_offset = offset.saturating_sub(1);

            match tsz::lsp::formatting::DocumentFormattingProvider::format_on_key(
                &source_text,
                lsp_line,
                lsp_offset,
                key,
                &options,
            ) {
                Ok(edits) => {
                    let body: Vec<serde_json::Value> = edits
                        .iter()
                        .map(|edit| {
                            serde_json::json!({
                                "start": Self::lsp_to_tsserver_position(edit.range.start),
                                "end": Self::lsp_to_tsserver_position(edit.range.end),
                                "newText": edit.new_text,
                            })
                        })
                        .collect();
                    Some(serde_json::json!(body))
                }
                Err(_) => Some(serde_json::json!([])),
            }
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_project_info(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(
            seq,
            request,
            Some(serde_json::json!({"configFileName": "", "fileNames": []})),
        )
    }

    pub(crate) fn handle_compiler_options_for_inferred(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, None)
    }

    pub(crate) fn handle_external_project(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, None)
    }

    pub(crate) fn handle_inlay_hints(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&source_text);
            let interner = TypeInterner::new();
            let provider = InlayHintsProvider::new(
                &arena,
                &binder,
                &line_map,
                &source_text,
                &interner,
                file.to_string(),
            );

            // Extract the range from arguments, default to entire file
            let start = match request
                .arguments
                .get("start")
                .and_then(serde_json::Value::as_u64)
            {
                Some(_) => {
                    let line = request
                        .arguments
                        .get("startLine")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(1) as u32;
                    let offset = request
                        .arguments
                        .get("startOffset")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(1) as u32;
                    Self::tsserver_to_lsp_position(line, offset)
                }
                None => Position::new(0, 0),
            };
            let end = match request
                .arguments
                .get("end")
                .and_then(serde_json::Value::as_u64)
            {
                Some(_) => {
                    let line = request
                        .arguments
                        .get("endLine")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(u32::MAX as u64) as u32;
                    let offset = request
                        .arguments
                        .get("endOffset")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(u32::MAX as u64) as u32;
                    Self::tsserver_to_lsp_position(line, offset)
                }
                None => Position::new(u32::MAX, u32::MAX),
            };
            let range = Range::new(start, end);

            let hints = provider.provide_inlay_hints(root, range);
            let body: Vec<serde_json::Value> = hints
                .iter()
                .map(|hint| {
                    let kind = match hint.kind {
                        tsz::lsp::inlay_hints::InlayHintKind::Parameter => "Parameter",
                        tsz::lsp::inlay_hints::InlayHintKind::Type => "Type",
                        tsz::lsp::inlay_hints::InlayHintKind::Generic => "Enum",
                    };
                    serde_json::json!({
                        "text": hint.label,
                        "position": Self::lsp_to_tsserver_position(hint.position),
                        "kind": kind,
                        "whitespaceBefore": false,
                        "whitespaceAfter": true,
                    })
                })
                .collect();
            Some(serde_json::json!(body))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_selection_range(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let (arena, _binder, _root, source_text) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&source_text);
            let provider = SelectionRangeProvider::new(&arena, &line_map, &source_text);

            let locations = request.arguments.get("locations")?.as_array()?;
            let positions: Vec<Position> = locations
                .iter()
                .filter_map(|loc| {
                    let line = loc.get("line")?.as_u64()? as u32;
                    let offset = loc.get("offset")?.as_u64()? as u32;
                    Some(Self::tsserver_to_lsp_position(line, offset))
                })
                .collect();

            let ranges = provider.get_selection_ranges(&positions);

            fn selection_range_to_json(
                sr: &tsz::lsp::selection_range::SelectionRange,
            ) -> serde_json::Value {
                let text_span = serde_json::json!({
                    "start": {
                        "line": sr.range.start.line + 1,
                        "offset": sr.range.start.character + 1,
                    },
                    "end": {
                        "line": sr.range.end.line + 1,
                        "offset": sr.range.end.character + 1,
                    },
                });
                if let Some(ref parent) = sr.parent {
                    serde_json::json!({
                        "textSpan": text_span,
                        "parent": selection_range_to_json(parent),
                    })
                } else {
                    serde_json::json!({
                        "textSpan": text_span,
                    })
                }
            }

            let body: Vec<serde_json::Value> = ranges
                .iter()
                .map(|opt_sr| {
                    opt_sr
                        .as_ref()
                        .map(selection_range_to_json)
                        .unwrap_or(serde_json::json!(null))
                })
                .collect();
            Some(serde_json::json!(body))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_linked_editing_range(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, None)
    }

    pub(crate) fn handle_prepare_call_hierarchy(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let provider =
                CallHierarchyProvider::new(&arena, &binder, &line_map, file, &source_text);
            let item = provider.prepare(root, position)?;
            Some(serde_json::json!([{
                "name": item.name,
                "kind": format!("{:?}", item.kind).to_lowercase(),
                "file": item.uri,
                "span": {
                    "start": Self::lsp_to_tsserver_position(item.range.start),
                    "end": Self::lsp_to_tsserver_position(item.range.end),
                },
                "selectionSpan": {
                    "start": Self::lsp_to_tsserver_position(item.selection_range.start),
                    "end": Self::lsp_to_tsserver_position(item.selection_range.end),
                },
            }]))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_call_hierarchy(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let provider =
                CallHierarchyProvider::new(&arena, &binder, &line_map, file, &source_text);

            let is_incoming = request.command == "provideCallHierarchyIncomingCalls";

            if is_incoming {
                let calls = provider.incoming_calls(root, position);
                let body: Vec<serde_json::Value> = calls
                    .iter()
                    .map(|call| {
                        let from_ranges: Vec<serde_json::Value> = call
                            .from_ranges
                            .iter()
                            .map(|r| {
                                serde_json::json!({
                                    "start": Self::lsp_to_tsserver_position(r.start),
                                    "end": Self::lsp_to_tsserver_position(r.end),
                                })
                            })
                            .collect();
                        serde_json::json!({
                            "from": {
                                "name": call.from.name,
                                "kind": format!("{:?}", call.from.kind).to_lowercase(),
                                "file": call.from.uri,
                                "span": {
                                    "start": Self::lsp_to_tsserver_position(call.from.range.start),
                                    "end": Self::lsp_to_tsserver_position(call.from.range.end),
                                },
                                "selectionSpan": {
                                    "start": Self::lsp_to_tsserver_position(call.from.selection_range.start),
                                    "end": Self::lsp_to_tsserver_position(call.from.selection_range.end),
                                },
                            },
                            "fromSpans": from_ranges,
                        })
                    })
                    .collect();
                Some(serde_json::json!(body))
            } else {
                let calls = provider.outgoing_calls(root, position);
                let body: Vec<serde_json::Value> = calls
                    .iter()
                    .map(|call| {
                        let from_ranges: Vec<serde_json::Value> = call
                            .from_ranges
                            .iter()
                            .map(|r| {
                                serde_json::json!({
                                    "start": Self::lsp_to_tsserver_position(r.start),
                                    "end": Self::lsp_to_tsserver_position(r.end),
                                })
                            })
                            .collect();
                        serde_json::json!({
                            "to": {
                                "name": call.to.name,
                                "kind": format!("{:?}", call.to.kind).to_lowercase(),
                                "file": call.to.uri,
                                "span": {
                                    "start": Self::lsp_to_tsserver_position(call.to.range.start),
                                    "end": Self::lsp_to_tsserver_position(call.to.range.end),
                                },
                                "selectionSpan": {
                                    "start": Self::lsp_to_tsserver_position(call.to.selection_range.start),
                                    "end": Self::lsp_to_tsserver_position(call.to.selection_range.end),
                                },
                            },
                            "fromSpans": from_ranges,
                        })
                    })
                    .collect();
                Some(serde_json::json!(body))
            }
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_map_code(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    pub(crate) fn handle_outlining_spans(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let (arena, _binder, root, source_text) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&source_text);
            let provider = FoldingRangeProvider::new(&arena, &line_map, &source_text);
            let ranges = provider.get_folding_ranges(root);

            let body: Vec<serde_json::Value> = ranges
                .iter()
                .map(|fr| {
                    // Convert byte offsets to precise line/offset positions
                    let start_pos = line_map.offset_to_position(fr.start_offset, &source_text);
                    let end_pos = line_map.offset_to_position(fr.end_offset, &source_text);
                    let hint_end_pos = line_map
                        .offset_to_position(fr.end_offset.min(fr.start_offset + 200), &source_text);

                    let mut span = serde_json::json!({
                        "textSpan": {
                            "start": Self::lsp_to_tsserver_position(start_pos),
                            "end": Self::lsp_to_tsserver_position(end_pos),
                        },
                        "hintSpan": {
                            "start": Self::lsp_to_tsserver_position(start_pos),
                            "end": Self::lsp_to_tsserver_position(
                                if hint_end_pos.line == start_pos.line {
                                    hint_end_pos
                                } else {
                                    end_pos
                                }
                            ),
                        },
                        "bannerText": "...",
                        "autoCollapse": false,
                    });
                    span["kind"] = serde_json::json!(fr.kind.as_deref().unwrap_or("code"));
                    span
                })
                .collect();
            Some(serde_json::json!(body))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

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
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }
}
