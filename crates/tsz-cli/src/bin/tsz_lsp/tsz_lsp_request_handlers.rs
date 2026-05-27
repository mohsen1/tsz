use super::*;

impl LspServer {
    // ─── Code Actions ───────────────────────────────────────────────────

    pub(super) fn handle_code_action(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = Self::extract_uri(&params).ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(&uri);

        let range = Self::extract_range(&params, "range")
            .unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(0, 0)));

        // Extract diagnostics from context
        let diagnostics = params
            .as_ref()
            .and_then(|p| p.get("context"))
            .and_then(|ctx| ctx.get("diagnostics"))
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|d| {
                        Some(tsz::lsp::LspDiagnostic {
                            range: {
                                let r = d.get("range")?;
                                let s = r.get("start")?;
                                let e = r.get("end")?;
                                Range::new(
                                    Position::new(
                                        s.get("line")?.as_u64()? as u32,
                                        s.get("character")?.as_u64()? as u32,
                                    ),
                                    Position::new(
                                        e.get("line")?.as_u64()? as u32,
                                        e.get("character")?.as_u64()? as u32,
                                    ),
                                )
                            },
                            severity: d
                                .get("severity")
                                .and_then(|s| s.as_u64())
                                .and_then(|s| (s as u8).try_into().ok()),
                            code: d.get("code").and_then(|c| c.as_u64()).map(|c| c as u32),
                            source: d.get("source").and_then(|s| s.as_str()).map(String::from),
                            message: d
                                .get("message")
                                .and_then(|m| m.as_str())
                                .unwrap_or("")
                                .to_string(),
                            related_information: None,
                            reports_unnecessary: None,
                            reports_deprecated: None,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        match self
            .project
            .get_code_actions(&file_name, range, diagnostics, None)
        {
            Some(actions) => {
                let lsp_actions: Vec<Value> = actions
                    .iter()
                    .map(|action| {
                        let mut a = serde_json::json!({
                            "title": action.title,
                        });
                        // Serialize kind using its serde rename
                        if let Ok(kind_val) = serde_json::to_value(&action.kind) {
                            a["kind"] = kind_val;
                        }
                        if let Some(ref edit) = action.edit {
                            let mut changes: serde_json::Map<String, Value> =
                                serde_json::Map::new();
                            for (file, edits) in &edit.changes {
                                let lsp_edits: Vec<Value> = edits
                                    .iter()
                                    .map(|e| {
                                        serde_json::json!({
                                            "range": Self::range_to_json(&e.range),
                                            "newText": e.new_text,
                                        })
                                    })
                                    .collect();
                                changes
                                    .insert(Self::file_name_to_uri(file), Value::Array(lsp_edits));
                            }
                            a["edit"] = serde_json::json!({ "changes": changes });
                        }
                        if action.is_preferred {
                            a["isPreferred"] = Value::from(true);
                        }
                        a
                    })
                    .collect();
                Ok(Value::Array(lsp_actions))
            }
            None => Ok(Value::Array(vec![])),
        }
    }

    // ─── Code Lens ──────────────────────────────────────────────────────

    pub(super) fn handle_code_lens(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = Self::extract_uri(&params).ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(&uri);

        match self.project.get_code_lenses(&file_name) {
            Some(lenses) => {
                let lsp_lenses: Vec<Value> = lenses
                    .iter()
                    .map(|lens| {
                        let mut l = serde_json::json!({
                            "range": Self::range_to_json(&lens.range),
                        });
                        if let Some(ref cmd) = lens.command {
                            l["command"] = serde_json::json!({
                                "title": cmd.title,
                                "command": cmd.command,
                            });
                        }
                        if let Some(ref data) = lens.data {
                            l["data"] = serde_json::to_value(data).unwrap_or(Value::Null);
                        }
                        l
                    })
                    .collect();
                Ok(Value::Array(lsp_lenses))
            }
            None => Ok(Value::Array(vec![])),
        }
    }

    pub(super) fn handle_code_lens_resolve(&mut self, params: Option<Value>) -> Result<Value> {
        let p = params
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing params"))?;

        let range = Self::extract_range(&params, "range")
            .unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(0, 0)));

        // Deserialize the data field to reconstruct the CodeLens
        let data: Option<tsz::lsp::CodeLensData> = p
            .get("data")
            .and_then(|d| serde_json::from_value(d.clone()).ok());

        let lens = tsz::lsp::CodeLens {
            range,
            command: None,
            data,
        };

        if let Some(ref data) = lens.data {
            let file_name = Self::uri_to_file_name(&data.file_path);
            if let Some(resolved) = self.project.resolve_code_lens(&file_name, &lens) {
                let mut l = serde_json::json!({
                    "range": Self::range_to_json(&resolved.range),
                });
                if let Some(ref cmd) = resolved.command {
                    let mut cmd_json = serde_json::json!({
                        "title": cmd.title,
                        "command": cmd.command,
                    });
                    if let Some(ref args) = cmd.arguments {
                        cmd_json["arguments"] = Value::Array(args.clone());
                    }
                    l["command"] = cmd_json;
                }
                return Ok(l);
            }
        }

        // Fallback: return as-is
        Ok(params.unwrap_or(Value::Null))
    }

    // ─── Selection Range ────────────────────────────────────────────────

    pub(super) fn handle_selection_range(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = Self::extract_uri(&params).ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(&uri);

        let positions: Vec<Position> = params
            .as_ref()
            .and_then(|p| p.get("positions"))
            .and_then(|pos| pos.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|p| {
                        let line = p.get("line")?.as_u64()? as u32;
                        let character = p.get("character")?.as_u64()? as u32;
                        Some(Position::new(line, character))
                    })
                    .collect()
            })
            .unwrap_or_default();

        match self.project.get_selection_ranges(&file_name, &positions) {
            Some(ranges) => {
                let lsp_ranges: Vec<Value> = ranges
                    .iter()
                    .map(|r| match r {
                        Some(sr) => Self::selection_range_to_json(sr),
                        None => Value::Null,
                    })
                    .collect();
                Ok(Value::Array(lsp_ranges))
            }
            None => Ok(Value::Array(vec![])),
        }
    }

    fn selection_range_to_json(sr: &tsz::lsp::SelectionRange) -> Value {
        let mut result = serde_json::json!({
            "range": Self::range_to_json(&sr.range),
        });
        if let Some(ref parent) = sr.parent {
            result["parent"] = Self::selection_range_to_json(parent);
        }
        result
    }

    // ─── Folding Range ──────────────────────────────────────────────────

    pub(super) fn handle_folding_range(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = Self::extract_uri(&params).ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(&uri);

        match self.project.get_folding_ranges(&file_name) {
            Some(ranges) => {
                let lsp_ranges: Vec<Value> = ranges
                    .iter()
                    .map(|r| {
                        let mut fr = serde_json::json!({
                            "startLine": r.start_line,
                            "endLine": r.end_line,
                        });
                        if let Some(ref kind) = r.kind {
                            fr["kind"] = Value::from(kind.as_str());
                        }
                        fr
                    })
                    .collect();
                Ok(Value::Array(lsp_ranges))
            }
            None => Ok(Value::Array(vec![])),
        }
    }

    // ─── Signature Help ─────────────────────────────────────────────────

    pub(super) fn handle_signature_help(&mut self, params: Option<Value>) -> Result<Value> {
        let (uri, position) = Self::extract_position(&params)
            .ok_or_else(|| anyhow::anyhow!("Missing position params"))?;
        let file_name = Self::uri_to_file_name(&uri);

        match self.project.get_signature_help(&file_name, position) {
            Some(help) => {
                let signatures: Vec<Value> = help
                    .signatures
                    .iter()
                    .map(|sig| {
                        let params: Vec<Value> = sig
                            .parameters
                            .iter()
                            .map(|p| {
                                let mut param = serde_json::json!({
                                    "label": p.label.clone(),
                                });
                                if let Some(ref doc) = p.documentation {
                                    param["documentation"] = Value::from(doc.as_str());
                                }
                                param
                            })
                            .collect();
                        let mut s = serde_json::json!({
                            "label": sig.label,
                            "parameters": params,
                        });
                        if let Some(ref doc) = sig.documentation {
                            s["documentation"] = Value::from(doc.as_str());
                        }
                        s
                    })
                    .collect();

                Ok(serde_json::json!({
                    "signatures": signatures,
                    "activeSignature": help.active_signature,
                    "activeParameter": help.active_parameter,
                }))
            }
            None => Ok(Value::Null),
        }
    }

    // ─── Semantic Tokens ────────────────────────────────────────────────

    pub(super) fn handle_semantic_tokens_full(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = Self::extract_uri(&params).ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(&uri);

        match self.project.get_semantic_tokens_full(&file_name) {
            Some(data) => Ok(serde_json::json!({ "data": data })),
            None => Ok(serde_json::json!({ "data": [] })),
        }
    }

    pub(super) fn handle_semantic_tokens_range(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = Self::extract_uri(&params).ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(&uri);
        let range = Self::extract_range(&params, "range")
            .ok_or_else(|| anyhow::anyhow!("Missing range"))?;

        match self.project.get_semantic_tokens_range(&file_name, range) {
            Some(data) => Ok(serde_json::json!({ "data": data })),
            None => Ok(serde_json::json!({ "data": [] })),
        }
    }

    // ─── Document Highlight ─────────────────────────────────────────────

    pub(super) fn handle_document_highlight(&mut self, params: Option<Value>) -> Result<Value> {
        let (uri, position) = Self::extract_position(&params)
            .ok_or_else(|| anyhow::anyhow!("Missing position params"))?;
        let file_name = Self::uri_to_file_name(&uri);

        match self.project.get_document_highlighting(&file_name, position) {
            Some(highlights) => {
                let lsp_highlights: Vec<Value> = highlights
                    .iter()
                    .map(|h| {
                        let kind = match h.kind {
                            Some(tsz::lsp::DocumentHighlightKind::Text) | None => 1,
                            Some(tsz::lsp::DocumentHighlightKind::Read) => 2,
                            Some(tsz::lsp::DocumentHighlightKind::Write) => 3,
                        };
                        serde_json::json!({
                            "range": Self::range_to_json(&h.range),
                            "kind": kind,
                        })
                    })
                    .collect();
                Ok(Value::Array(lsp_highlights))
            }
            None => Ok(Value::Array(vec![])),
        }
    }

    // ─── Inlay Hints ────────────────────────────────────────────────────

    pub(super) fn handle_inlay_hint(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = Self::extract_uri(&params).ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(&uri);

        let range = Self::extract_range(&params, "range")
            .unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(u32::MAX, 0)));

        match self.project.get_inlay_hints(&file_name, range) {
            Some(hints) => {
                let lsp_hints: Vec<Value> = hints
                    .iter()
                    .map(|h| {
                        let kind = match h.kind {
                            tsz::lsp::InlayHintKind::Type | tsz::lsp::InlayHintKind::Generic => 1,
                            tsz::lsp::InlayHintKind::Parameter => 2,
                        };
                        let mut hint = serde_json::json!({
                            "position": Self::position_to_json(&h.position),
                            "label": h.label,
                            "kind": kind,
                        });
                        if let Some(ref tooltip) = h.tooltip {
                            hint["tooltip"] = Value::from(tooltip.as_str());
                        }
                        hint
                    })
                    .collect();
                Ok(Value::Array(lsp_hints))
            }
            None => Ok(Value::Array(vec![])),
        }
    }

    pub(super) fn handle_inlay_hint_resolve(&mut self, params: Option<Value>) -> Result<Value> {
        // The params IS the inlay hint itself - just add a tooltip if missing
        let mut hint = params.unwrap_or(Value::Null);

        // If no tooltip yet, add a description based on the kind
        if hint.get("tooltip").is_none() {
            let kind = hint.get("kind").and_then(|k| k.as_u64()).unwrap_or(0);
            let label = hint
                .get("label")
                .and_then(|l| l.as_str())
                .unwrap_or_default();

            let tooltip = match kind {
                1 => {
                    // Type hint
                    format!("Inferred type{label}")
                }
                2 => {
                    // Parameter hint
                    format!("Parameter name{label}")
                }
                _ => String::new(),
            };

            if !tooltip.is_empty() {
                hint["tooltip"] = serde_json::json!({
                    "kind": "markdown",
                    "value": format!("```typescript\n{tooltip}\n```"),
                });
            }
        }

        Ok(hint)
    }

    // ─── Document Colors ─────────────────────────────────────────────────

    pub(super) fn handle_document_color(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = Self::extract_uri(&params).ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(&uri);

        match self.project.get_document_colors(&file_name) {
            Some(colors) => {
                let lsp_colors: Vec<Value> = colors
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "range": Self::range_to_json(&c.range),
                            "color": {
                                "red": c.color.red,
                                "green": c.color.green,
                                "blue": c.color.blue,
                                "alpha": c.color.alpha,
                            },
                        })
                    })
                    .collect();
                Ok(Value::Array(lsp_colors))
            }
            None => Ok(Value::Array(vec![])),
        }
    }

    pub(super) fn handle_color_presentation(&mut self, params: Option<Value>) -> Result<Value> {
        // Convert a color back to a text representation
        let color = params
            .as_ref()
            .and_then(|p| p.get("color"))
            .ok_or_else(|| anyhow::anyhow!("Missing color"))?;

        let color = tsz_lsp::Color {
            red: color.get("red").and_then(|v| v.as_f64()).unwrap_or(0.0),
            green: color.get("green").and_then(|v| v.as_f64()).unwrap_or(0.0),
            blue: color.get("blue").and_then(|v| v.as_f64()).unwrap_or(0.0),
            alpha: color.get("alpha").and_then(|v| v.as_f64()).unwrap_or(1.0),
        };

        let presentations: Vec<Value> =
            tsz_lsp::DocumentColorProvider::provide_color_presentations(&color)
                .into_iter()
                .map(|presentation| {
                    serde_json::json!({
                        "label": presentation.label,
                    })
                })
                .collect();

        Ok(Value::Array(presentations))
    }

    // ─── Document Links ─────────────────────────────────────────────────

    pub(super) fn handle_document_link(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = Self::extract_uri(&params).ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(&uri);

        match self.project.get_document_links(&file_name) {
            Some(links) => {
                let lsp_links: Vec<Value> = links
                    .iter()
                    .map(|link| {
                        let mut l = serde_json::json!({
                            "range": Self::range_to_json(&link.range),
                        });
                        if let Some(ref target) = link.target {
                            l["target"] = Value::from(target.as_str());
                        }
                        if let Some(ref tooltip) = link.tooltip {
                            l["tooltip"] = Value::from(tooltip.as_str());
                        }
                        l
                    })
                    .collect();
                Ok(Value::Array(lsp_links))
            }
            None => Ok(Value::Array(vec![])),
        }
    }

    // ─── Linked Editing Range ───────────────────────────────────────────

    pub(super) fn handle_linked_editing_range(&mut self, params: Option<Value>) -> Result<Value> {
        let (uri, position) = Self::extract_position(&params)
            .ok_or_else(|| anyhow::anyhow!("Missing position params"))?;
        let file_name = Self::uri_to_file_name(&uri);

        match self.project.get_linked_editing_ranges(&file_name, position) {
            Some(result) => {
                let ranges: Vec<Value> = result.ranges.iter().map(Self::range_to_json).collect();
                let mut response = serde_json::json!({ "ranges": ranges });
                if let Some(ref pattern) = result.word_pattern {
                    response["wordPattern"] = Value::from(pattern.as_str());
                }
                Ok(response)
            }
            None => Ok(Value::Null),
        }
    }

    // ─── Call Hierarchy ─────────────────────────────────────────────────

    pub(super) fn handle_prepare_call_hierarchy(&mut self, params: Option<Value>) -> Result<Value> {
        let (uri, position) = Self::extract_position(&params)
            .ok_or_else(|| anyhow::anyhow!("Missing position params"))?;
        let file_name = Self::uri_to_file_name(&uri);

        match self.project.prepare_call_hierarchy(&file_name, position) {
            Some(item) => Ok(Value::Array(vec![Self::call_hierarchy_item_to_json(&item)])),
            None => Ok(Value::Array(vec![])),
        }
    }

    pub(super) fn handle_incoming_calls(&mut self, params: Option<Value>) -> Result<Value> {
        let item = params
            .as_ref()
            .and_then(|p| p.get("item"))
            .ok_or_else(|| anyhow::anyhow!("Missing item"))?;
        let uri = item
            .get("uri")
            .and_then(|u| u.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(uri);
        let position = Self::extract_range_start(item.get("selectionRange"));

        let calls = self.project.get_incoming_calls(&file_name, position);
        let lsp_calls: Vec<Value> = calls
            .iter()
            .map(|call| {
                let from_ranges: Vec<Value> =
                    call.from_ranges.iter().map(Self::range_to_json).collect();
                serde_json::json!({
                    "from": Self::call_hierarchy_item_to_json(&call.from),
                    "fromRanges": from_ranges,
                })
            })
            .collect();
        Ok(Value::Array(lsp_calls))
    }

    pub(super) fn handle_outgoing_calls(&mut self, params: Option<Value>) -> Result<Value> {
        let item = params
            .as_ref()
            .and_then(|p| p.get("item"))
            .ok_or_else(|| anyhow::anyhow!("Missing item"))?;
        let uri = item
            .get("uri")
            .and_then(|u| u.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(uri);
        let position = Self::extract_range_start(item.get("selectionRange"));

        let calls = self.project.get_outgoing_calls(&file_name, position);
        let lsp_calls: Vec<Value> = calls
            .iter()
            .map(|call| {
                let from_ranges: Vec<Value> =
                    call.from_ranges.iter().map(Self::range_to_json).collect();
                serde_json::json!({
                    "to": Self::call_hierarchy_item_to_json(&call.to),
                    "fromRanges": from_ranges,
                })
            })
            .collect();
        Ok(Value::Array(lsp_calls))
    }

    fn call_hierarchy_item_to_json(item: &tsz::lsp::CallHierarchyItem) -> Value {
        serde_json::json!({
            "name": item.name,
            "kind": Self::symbol_kind_to_lsp(item.kind),
            "uri": Self::file_name_to_uri(&item.uri),
            "range": Self::range_to_json(&item.range),
            "selectionRange": Self::range_to_json(&item.selection_range),
        })
    }

    fn extract_range_start(range: Option<&Value>) -> Position {
        range
            .and_then(|r| {
                let start = r.get("start")?;
                let line = start.get("line")?.as_u64()? as u32;
                let character = start.get("character")?.as_u64()? as u32;
                Some(Position::new(line, character))
            })
            .unwrap_or_else(|| Position::new(0, 0))
    }

    // ─── Type Hierarchy ─────────────────────────────────────────────────

    pub(super) fn handle_prepare_type_hierarchy(&mut self, params: Option<Value>) -> Result<Value> {
        let (uri, position) = Self::extract_position(&params)
            .ok_or_else(|| anyhow::anyhow!("Missing position params"))?;
        let file_name = Self::uri_to_file_name(&uri);

        match self.project.prepare_type_hierarchy(&file_name, position) {
            Some(item) => Ok(Value::Array(vec![Self::type_hierarchy_item_to_json(&item)])),
            None => Ok(Value::Array(vec![])),
        }
    }

    pub(super) fn handle_supertypes(&mut self, params: Option<Value>) -> Result<Value> {
        let item = params
            .as_ref()
            .and_then(|p| p.get("item"))
            .ok_or_else(|| anyhow::anyhow!("Missing item"))?;
        let uri = item
            .get("uri")
            .and_then(|u| u.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(uri);
        let position = Self::extract_range_start(item.get("selectionRange"));

        let items = self.project.supertypes(&file_name, position);
        let lsp_items: Vec<Value> = items
            .iter()
            .map(Self::type_hierarchy_item_to_json)
            .collect();
        Ok(Value::Array(lsp_items))
    }

    pub(super) fn handle_subtypes(&mut self, params: Option<Value>) -> Result<Value> {
        let item = params
            .as_ref()
            .and_then(|p| p.get("item"))
            .ok_or_else(|| anyhow::anyhow!("Missing item"))?;
        let uri = item
            .get("uri")
            .and_then(|u| u.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(uri);
        let position = Self::extract_range_start(item.get("selectionRange"));

        let items = self.project.subtypes(&file_name, position);
        let lsp_items: Vec<Value> = items
            .iter()
            .map(Self::type_hierarchy_item_to_json)
            .collect();
        Ok(Value::Array(lsp_items))
    }

    fn type_hierarchy_item_to_json(item: &tsz::lsp::TypeHierarchyItem) -> Value {
        serde_json::json!({
            "name": item.name,
            "kind": Self::symbol_kind_to_lsp(item.kind),
            "uri": Self::file_name_to_uri(&item.uri),
            "range": Self::range_to_json(&item.range),
            "selectionRange": Self::range_to_json(&item.selection_range),
        })
    }

    // ─── Workspace Symbols ──────────────────────────────────────────────

    pub(super) fn handle_workspace_symbol(&mut self, params: Option<Value>) -> Result<Value> {
        let query = params
            .as_ref()
            .and_then(|p| p.get("query"))
            .and_then(|q| q.as_str())
            .unwrap_or("");

        let symbols = self.project.get_workspace_symbols(query);
        let lsp_symbols: Vec<Value> = symbols
            .iter()
            .map(|sym| {
                serde_json::json!({
                    "name": sym.name,
                    "kind": Self::symbol_kind_to_lsp(sym.kind),
                    "location": Self::location_to_json(&sym.location),
                })
            })
            .collect();
        Ok(Value::Array(lsp_symbols))
    }

    // ─── Range Formatting ──────────────────────────────────────────────

    pub(super) fn handle_range_formatting(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = Self::extract_uri(&params).ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(&uri);

        let range = Self::extract_range(&params, "range")
            .ok_or_else(|| anyhow::anyhow!("Missing range"))?;

        let options = params
            .as_ref()
            .and_then(|p| p.get("options"))
            .map(|opts| FormattingOptions {
                tab_size: opts.get("tabSize").and_then(|v| v.as_u64()).unwrap_or(4) as u32,
                insert_spaces: opts
                    .get("insertSpaces")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
                trim_trailing_whitespace: opts
                    .get("trimTrailingWhitespace")
                    .and_then(|v| v.as_bool()),
                insert_final_newline: None,
                trim_final_newlines: None,
                semicolons: opts
                    .get("semicolons")
                    .and_then(|v| v.as_str())
                    .map(String::from),
            })
            .unwrap_or_default();

        // Format the whole document and filter edits to only those within the range
        match self.project.format_document(&file_name, &options) {
            Some(Ok(edits)) => {
                let lsp_edits: Vec<Value> = edits
                    .iter()
                    .filter(|edit| {
                        // Include edits that overlap with the requested range
                        edit.range.start.line <= range.end.line
                            && edit.range.end.line >= range.start.line
                    })
                    .map(|edit| {
                        serde_json::json!({
                            "range": Self::range_to_json(&edit.range),
                            "newText": edit.new_text,
                        })
                    })
                    .collect();
                Ok(Value::Array(lsp_edits))
            }
            Some(Err(e)) => {
                debug!("Range formatting error: {}", e);
                Ok(Value::Array(vec![]))
            }
            None => Ok(Value::Array(vec![])),
        }
    }

    // ─── On-Type Formatting ────────────────────────────────────────────

    pub(super) fn handle_on_type_formatting(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = Self::extract_uri(&params).ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(&uri);

        let position = params
            .as_ref()
            .and_then(|p| p.get("position"))
            .and_then(|pos| {
                let line = pos.get("line")?.as_u64()? as u32;
                let character = pos.get("character")?.as_u64()? as u32;
                Some(Position::new(line, character))
            })
            .ok_or_else(|| anyhow::anyhow!("Missing position"))?;

        let ch = params
            .as_ref()
            .and_then(|p| p.get("ch"))
            .and_then(|c| c.as_str())
            .unwrap_or(";");

        let options = params
            .as_ref()
            .and_then(|p| p.get("options"))
            .map(|opts| FormattingOptions {
                tab_size: opts.get("tabSize").and_then(|v| v.as_u64()).unwrap_or(4) as u32,
                insert_spaces: opts
                    .get("insertSpaces")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
                trim_trailing_whitespace: None,
                insert_final_newline: None,
                trim_final_newlines: None,
                semicolons: None,
            })
            .unwrap_or_default();

        // Use the format_on_key API from the formatting provider
        if let Some(file) = self.project.file(&file_name) {
            let source = file.source_text();
            let offset = file
                .line_map()
                .position_to_offset(position, source)
                .unwrap_or(0);

            match tsz::lsp::DocumentFormattingProvider::format_on_key(
                source,
                position.line,
                offset,
                ch,
                &options,
            ) {
                Ok(edits) => {
                    let lsp_edits: Vec<Value> = edits
                        .iter()
                        .map(|edit| {
                            serde_json::json!({
                                "range": Self::range_to_json(&edit.range),
                                "newText": edit.new_text,
                            })
                        })
                        .collect();
                    return Ok(Value::Array(lsp_edits));
                }
                Err(e) => {
                    debug!("On-type formatting error: {}", e);
                    return Ok(Value::Array(vec![]));
                }
            }
        }

        Ok(Value::Array(vec![]))
    }

    // ─── Workspace Configuration ───────────────────────────────────────

    pub(super) fn handle_did_change_configuration(&mut self, params: Option<Value>) {
        // Extract settings if provided
        if let Some(settings) = params
            .as_ref()
            .and_then(|p| p.get("settings"))
            .and_then(|s| s.get("tsz").or_else(|| s.get("typescript")))
        {
            // Apply strict mode setting if present
            if let Some(strict) = settings.get("strict").and_then(|v| v.as_bool()) {
                self.project.set_strict(strict);
            }

            debug!("Configuration updated: {:?}", settings);
        }
    }

    // ─── Watched File Changes ──────────────────────────────────────────

    pub(super) fn handle_did_change_watched_files(&mut self, params: Option<Value>) {
        let changes = match params
            .as_ref()
            .and_then(|p| p.get("changes"))
            .and_then(|c| c.as_array())
        {
            Some(c) => c,
            None => return,
        };

        for change in changes {
            let uri = match change.get("uri").and_then(|u| u.as_str()) {
                Some(u) => u,
                None => continue,
            };
            let change_type = change.get("type").and_then(|t| t.as_u64()).unwrap_or(0);
            let file_name = Self::uri_to_file_name(uri);

            match change_type {
                1 => {
                    // Created: read and add if it's a TS/JS file
                    if Self::is_ts_file(&file_name)
                        && let Ok(content) = std::fs::read_to_string(&file_name)
                    {
                        self.project.set_file(file_name, content);
                    }
                }
                2 => {
                    // Changed: update if we're tracking it
                    if self.project.file(&file_name).is_some()
                        && let Ok(content) = std::fs::read_to_string(&file_name)
                    {
                        self.project.set_file(file_name, content);
                    }
                }
                3 => {
                    // Deleted: remove from project
                    self.project.remove_file(&file_name);
                    // Clear diagnostics for deleted file
                    self.pending_notifications.push(JsonRpcNotification {
                        jsonrpc: "2.0".to_string(),
                        method: "textDocument/publishDiagnostics".to_string(),
                        params: serde_json::json!({
                            "uri": uri,
                            "diagnostics": [],
                        }),
                    });
                }
                _ => {}
            }
        }
    }

    pub(super) fn is_ts_file(path: &str) -> bool {
        let extensions = [".ts", ".tsx", ".js", ".jsx", ".mts", ".cts", ".mjs", ".cjs"];
        extensions.iter().any(|ext| path.ends_with(ext))
    }

    // ─── File Rename ───────────────────────────────────────────────────

    pub(super) fn handle_will_rename_files(&mut self, params: Option<Value>) -> Result<Value> {
        let files = params
            .as_ref()
            .and_then(|p| p.get("files"))
            .and_then(|f| f.as_array())
            .cloned()
            .unwrap_or_default();

        let mut all_changes = serde_json::Map::new();

        for file_entry in &files {
            let old_uri = match file_entry.get("oldUri").and_then(|u| u.as_str()) {
                Some(u) => u,
                None => continue,
            };
            let new_uri = match file_entry.get("newUri").and_then(|u| u.as_str()) {
                Some(u) => u,
                None => continue,
            };

            let old_path = Self::uri_to_file_name(old_uri);
            let new_path = Self::uri_to_file_name(new_uri);

            let edits = self.project.get_file_rename_edits(&old_path, &new_path);

            for (file_name, file_edits) in edits {
                let uri = Self::file_name_to_uri(&file_name);
                let lsp_edits: Vec<Value> = file_edits
                    .iter()
                    .map(|edit| {
                        serde_json::json!({
                            "range": Self::range_to_json(&edit.range),
                            "newText": edit.new_text,
                        })
                    })
                    .collect();

                // Merge with existing edits for this file
                all_changes
                    .entry(uri)
                    .or_insert_with(|| Value::Array(vec![]))
                    .as_array_mut()
                    .unwrap()
                    .extend(lsp_edits);
            }
        }

        if all_changes.is_empty() {
            Ok(Value::Null)
        } else {
            Ok(serde_json::json!({
                "changes": all_changes,
            }))
        }
    }

    pub(super) fn handle_did_rename_files(&mut self, params: Option<Value>) {
        let files = match params
            .as_ref()
            .and_then(|p| p.get("files"))
            .and_then(|f| f.as_array())
        {
            Some(f) => f.clone(),
            None => return,
        };

        for file_entry in &files {
            let old_uri = match file_entry.get("oldUri").and_then(|u| u.as_str()) {
                Some(u) => u,
                None => continue,
            };
            let new_uri = match file_entry.get("newUri").and_then(|u| u.as_str()) {
                Some(u) => u,
                None => continue,
            };

            let old_path = Self::uri_to_file_name(old_uri);
            let new_path = Self::uri_to_file_name(new_uri);

            // Compute workspace edits to update import paths
            let edits = self.project.get_file_rename_edits(&old_path, &new_path);

            // Apply edits via workspace/applyEdit request
            if !edits.is_empty() {
                let mut changes = serde_json::Map::new();
                for (file_name, file_edits) in edits {
                    let uri = Self::file_name_to_uri(&file_name);
                    let lsp_edits: Vec<Value> = file_edits
                        .iter()
                        .map(|edit| {
                            serde_json::json!({
                                "range": Self::range_to_json(&edit.range),
                                "newText": edit.new_text,
                            })
                        })
                        .collect();
                    changes.insert(uri, Value::Array(lsp_edits));
                }

                let req_id = self.next_server_request_id;
                self.next_server_request_id += 1;
                self.pending_server_requests.push(JsonRpcServerRequest {
                    jsonrpc: "2.0".to_string(),
                    id: serde_json::Value::Number(req_id.into()),
                    method: "workspace/applyEdit".to_string(),
                    params: serde_json::json!({
                        "label": "Update imports for renamed file",
                        "edit": {
                            "changes": changes,
                        },
                    }),
                });
            }

            // Update project state: remove old file, add new
            if let Some(file) = self.project.file(&old_path) {
                let source = file.source_text().to_string();
                self.project.remove_file(&old_path);
                self.project.set_file(new_path.clone(), source);
            }
        }
    }

    // ─── Execute Command ───────────────────────────────────────────────

    pub(super) fn handle_execute_command(&mut self, params: Option<Value>) -> Result<Value> {
        let command = params
            .as_ref()
            .and_then(|p| p.get("command"))
            .and_then(|c| c.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing command"))?;

        let arguments = params
            .as_ref()
            .and_then(|p| p.get("arguments"))
            .and_then(|a| a.as_array());

        match command {
            "tsz.organizeImports" => {
                // Extract file URI from arguments
                if let Some(args) = arguments
                    && let Some(uri) = args.first().and_then(|a| a.as_str())
                {
                    let file_name = Self::uri_to_file_name(uri);
                    let range = Range::new(Position::new(0, 0), Position::new(0, 0));
                    let context = tsz::lsp::CodeActionContext {
                        diagnostics: vec![],
                        only: Some(vec![tsz::lsp::CodeActionKind::SourceOrganizeImports]),
                        import_candidates: vec![],
                    };

                    if let Some(file) = self.project.file(&file_name) {
                        let provider = tsz::lsp::CodeActionProvider::new(
                            file.arena(),
                            file.binder(),
                            file.line_map(),
                            file_name,
                            file.source_text(),
                        );
                        let actions = provider.provide_code_actions(file.root(), range, context);
                        if let Some(action) = actions.first()
                            && let Some(ref edit) = action.edit
                        {
                            // Apply the workspace edit
                            return Ok(serde_json::to_value(edit).unwrap_or(Value::Null));
                        }
                    }
                }
                Ok(Value::Null)
            }
            "tsz.applyCodeAction" => {
                // Apply a code action edit that was deferred for server-side execution.
                // Arguments: [workspaceEdit]
                if let Some(args) = arguments
                    && let Some(edit_value) = args.first()
                {
                    // Issue #3545: workspace/applyEdit is a server-to-client
                    // request, not a notification. Allocate an id so the
                    // client can respond with `ApplyWorkspaceEditResponse`.
                    let req_id = self.next_server_request_id;
                    self.next_server_request_id += 1;
                    self.pending_server_requests.push(JsonRpcServerRequest {
                        jsonrpc: "2.0".to_string(),
                        id: serde_json::Value::Number(req_id.into()),
                        method: "workspace/applyEdit".to_string(),
                        params: serde_json::json!({
                            "label": "Apply code action",
                            "edit": edit_value,
                        }),
                    });
                    return Ok(Value::Bool(true));
                }
                Ok(Value::Null)
            }
            "tsz.reloadProject" => {
                // Reload tsconfig.json and re-discover workspace files
                let roots: Vec<String> = self.project.workspace_roots().to_vec();
                for root in &roots {
                    self.project.load_tsconfig(root);
                }
                let discovered = self.project.discover_files(&roots);
                self.show_message(
                    3, // Info
                    &format!("Reloaded project: {} files indexed", discovered.len()),
                );
                Ok(Value::Bool(true))
            }
            _ => {
                debug!("Unknown command: {command}");
                Ok(Value::Null)
            }
        }
    }
}
