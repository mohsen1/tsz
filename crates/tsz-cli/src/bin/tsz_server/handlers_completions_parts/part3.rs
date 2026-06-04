impl Server {
    pub(crate) fn handle_completion_details(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let entry_names = request.arguments.get("entryNames")?.as_array()?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&source_text);
            let preferences = request
                .arguments
                .get("preferences")
                .unwrap_or(&request.arguments);
            let interner = TypeInterner::new();
            let provider = Completions::new_with_types(
                &arena,
                &binder,
                &line_map,
                &interner,
                &source_text,
                file.to_string(),
            );
            let line = request.arguments.get("line")?.as_u64()? as u32;
            let offset = request.arguments.get("offset")?.as_u64()? as u32;
            let position = Self::tsserver_to_lsp_position(line, offset);
            let (completion_position, completion_result) =
                Self::completion_result_at_position(&provider, root, position);
            let provider_items = completion_result
                .as_ref()
                .map(|result| result.entries.clone())
                .unwrap_or_default();
            let project_completion_position = completion_position;
            let mut project_items =
                self.project_completion_items(file, project_completion_position, Some(preferences));
            let is_member_completion = completion_result
                .as_ref()
                .is_some_and(|result| result.is_member_completion);
            let allow_class_member_snippets = !is_member_completion
                && Self::is_class_member_snippet_context(
                    &source_text,
                    &line_map,
                    completion_position,
                );
            let include_class_member_snippets = Self::bool_pref_or_default(
                Some(preferences),
                "includeCompletionsWithClassMemberSnippets",
                self.include_completions_with_class_member_snippets,
            );
            let requested_class_member_snippet = entry_names.iter().any(|entry_name| {
                entry_name
                    .as_object()
                    .and_then(|obj| obj.get("source"))
                    .and_then(serde_json::Value::as_str)
                    == Some("ClassMemberSnippet/")
            });
            if allow_class_member_snippets
                && requested_class_member_snippet
                && project_items.is_empty()
            {
                let forced_auto_import_prefs =
                    serde_json::json!({ "includeCompletionsForModuleExports": true });
                project_items = self.project_completion_items(
                    file,
                    project_completion_position,
                    Some(&forced_auto_import_prefs),
                );
            }
            let snippet_items = if allow_class_member_snippets
                && (requested_class_member_snippet || include_class_member_snippets)
            {
                self.class_member_snippet_items(
                    &provider,
                    root,
                    completion_position,
                    file,
                    &source_text,
                    &project_items,
                )
            } else {
                Vec::new()
            };
            let items = if is_member_completion {
                provider_items
            } else {
                Self::merge_non_member_completion_items(provider_items, project_items.clone())
            };
            let mut items = items;
            if !snippet_items.is_empty() {
                items = Self::merge_non_member_completion_items(items, snippet_items.clone());
                items = Self::prioritize_class_member_snippet_items(items);
                items = Self::normalize_class_member_snippet_items(items);
            }
            items = self.maybe_add_verbatim_commonjs_auto_import_items(file, &source_text, items);
            if let Some(completion_offset) =
                line_map.position_to_offset(completion_position, &source_text)
            {
                items = Self::maybe_add_merged_class_function_members(
                    items,
                    &source_text,
                    completion_offset,
                    is_member_completion,
                );
            }
            if is_member_completion
                && items.is_empty()
                && let Some(completion_offset) =
                    line_map.position_to_offset(completion_position, &source_text)
            {
                let fallback = self.commonjs_require_member_completion_items(
                    file,
                    &source_text,
                    completion_offset,
                );
                if !fallback.is_empty() {
                    items = Self::merge_non_member_completion_items(items, fallback);
                }
            }
            Self::sort_tsserver_completion_items(&mut items);
            // Index merged items by label once so each entry_name lookup below
            // doesn't linear-scan the full list (can be ~100s of items when
            // the project has many exports). Lifetime-scoped to `items`, so
            // the closure's returned &CompletionItem stays valid.
            let mut items_by_label: FxHashMap<&str, Vec<&CompletionItem>> = FxHashMap::default();
            for i in &items {
                items_by_label.entry(i.label.as_str()).or_default().push(i);
            }
            let member_parent = completion_result
                .as_ref()
                .and_then(|result| {
                    result.is_member_completion.then(|| {
                        provider.get_member_completion_parent_type_name(root, completion_position)
                    })
                })
                .flatten();
            let details: Vec<serde_json::Value> = entry_names
                .iter()
                .map(|entry_name| {
                    let (name, requested_source, requested_export_name) =
                        if let Some(s) = entry_name.as_str() {
                            (s.to_string(), None, None)
                        } else if let Some(obj) = entry_name.as_object() {
                            let source_from_value = |value: Option<&serde_json::Value>| {
                                value.and_then(|v| {
                                    v.as_str()
                                        .map(|s| s.trim().to_string())
                                        .or_else(|| {
                                            v.as_object()
                                                .and_then(|obj| obj.get("text"))
                                                .and_then(serde_json::Value::as_str)
                                                .map(|s| s.trim().to_string())
                                        })
                                        .or_else(|| {
                                            v.as_array().and_then(|arr| {
                                                let mut text = String::new();
                                                for part in arr {
                                                    let part_text = part
                                                        .as_object()
                                                        .and_then(|obj| obj.get("text"))
                                                        .and_then(serde_json::Value::as_str)
                                                        .unwrap_or_default();
                                                    text.push_str(part_text);
                                                }
                                                let text = text.trim().to_string();
                                                (!text.is_empty()).then_some(text)
                                            })
                                        })
                                })
                            };
                            (
                                obj.get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                source_from_value(obj.get("source"))
                                    .or_else(|| source_from_value(obj.get("sourceDisplay"))),
                                obj.get("data")
                                    .and_then(|data| data.get("exportName"))
                                    .and_then(serde_json::Value::as_str)
                                    .map(std::string::ToString::to_string),
                            )
                        } else {
                            (String::new(), None, None)
                        };
                    // Try to find the matching completion item.
                    // When source metadata is missing for duplicate labels, prefer
                    // ClassMemberSnippet entries to keep tsserver details/code-action
                    // pairing stable for snippet-backed completions.
                    let export_name_matches =
                        |candidate: &CompletionItem, requested_export_name: &str| {
                            Self::auto_import_export_name(candidate)
                                .as_deref()
                                .is_some_and(|name| name == requested_export_name)
                        };
                    // Label-indexed candidates for this entry; avoids a fresh
                    // linear scan of the full merged-items list for every
                    // predicate branch below.
                    let empty_candidates: Vec<&CompletionItem> = Vec::new();
                    let candidates: &Vec<&CompletionItem> = items_by_label
                        .get(name.as_str())
                        .unwrap_or(&empty_candidates);
                    let find = |pred: &dyn Fn(&CompletionItem) -> bool| -> Option<&CompletionItem> {
                        candidates.iter().copied().find(|i| pred(i))
                    };
                    let mut item = if let Some(source) = requested_source.as_deref() {
                        find(&|i| {
                            i.has_action
                                && Self::completion_sources_match(i.source.as_deref(), source)
                                && requested_export_name
                                    .as_deref()
                                    .is_none_or(|requested| export_name_matches(i, requested))
                        })
                        .or_else(|| {
                            find(&|i| {
                                Self::completion_sources_match(i.source.as_deref(), source)
                                    && requested_export_name
                                        .as_deref()
                                        .is_none_or(|requested| export_name_matches(i, requested))
                            })
                        })
                        .or_else(|| {
                            find(&|i| Self::completion_sources_match(i.source.as_deref(), source))
                        })
                        .or_else(|| {
                            requested_export_name
                                .as_deref()
                                .and_then(|requested| find(&|i| export_name_matches(i, requested)))
                        })
                    } else if entry_name.as_object().is_some() {
                        find(&|i| i.source.as_deref() == Some("ClassMemberSnippet/")).or_else(
                            || {
                                find(&|i| {
                                    requested_export_name
                                        .as_deref()
                                        .is_none_or(|requested| export_name_matches(i, requested))
                                })
                            },
                        )
                    } else {
                        find(&|_| true)
                    };
                    if item.is_none() && requested_source.as_deref() == Some("ClassMemberSnippet/")
                    {
                        item = snippet_items.iter().find(|i| {
                            i.label == name && i.source.as_deref() == Some("ClassMemberSnippet/")
                        });
                    }
                    let auto_import_export_name = requested_export_name
                        .clone()
                        .or_else(|| item.and_then(Self::auto_import_export_name));
                    let is_default_auto_import_item =
                        auto_import_export_name.as_deref() == Some("default");
                    let mut display_item_owned = None;
                    if item.is_some_and(|i| i.has_action && i.source.is_some())
                        && let Some(found_item) = item
                    {
                        let mut adjusted_item = found_item.clone();
                        if let Some(export_name) = auto_import_export_name.as_deref() {
                            if export_name == "default" {
                                adjusted_item.kind = CompletionItemKind::Property;
                            }
                            if let Some((export_kind, export_type)) = self
                                .auto_import_export_literal_info(file, &adjusted_item, export_name)
                            {
                                adjusted_item.kind = export_kind;
                                adjusted_item.detail = Some(export_type);
                            } else if adjusted_item
                                .detail
                                .as_deref()
                                .is_some_and(|detail| detail.starts_with("auto-import"))
                            {
                                adjusted_item.detail = None;
                            }
                        }
                        display_item_owned = Some(adjusted_item);
                    }
                    let display_item = display_item_owned.as_ref().or(item);
                    let kind =
                        display_item.map_or("property", |i| Self::completion_kind_to_str(i.kind));
                    let kind_modifiers = display_item
                        .and_then(|i| i.kind_modifiers.as_deref())
                        .unwrap_or("");
                    let display_name = if is_default_auto_import_item {
                        "default"
                    } else {
                        &name
                    };
                    let display_parts = Self::build_completion_display_parts(
                        display_item,
                        display_name,
                        member_parent.as_deref(),
                        &arena,
                        &binder,
                        &source_text,
                    );
                    let (documentation, jsdoc_tags) = Self::completion_doc_and_tags(
                        display_item,
                        &name,
                        &arena,
                        &binder,
                        root,
                        &source_text,
                    );
                    let mut detail = serde_json::Map::new();
                    detail.insert("name".to_string(), serde_json::json!(name));
                    detail.insert("kind".to_string(), serde_json::json!(kind));
                    detail.insert(
                        "kindModifiers".to_string(),
                        serde_json::json!(kind_modifiers),
                    );
                    detail.insert("displayParts".to_string(), display_parts);
                    let is_auto_import_item =
                        item.is_some_and(|i| i.has_action && i.source.is_some());
                    if !is_auto_import_item && documentation != serde_json::json!([]) {
                        detail.insert("documentation".to_string(), documentation);
                    }
                    detail.insert(
                        "tags".to_string(),
                        if is_auto_import_item {
                            serde_json::json!([])
                        } else {
                            serde_json::json!(jsdoc_tags)
                        },
                    );
                    if let Some(source) = display_item.and_then(|i| i.source.as_ref()) {
                        let source_display =
                            serde_json::json!([{ "text": source, "kind": "text" }]);
                        detail.insert("source".to_string(), source_display.clone());
                        detail.insert("sourceDisplay".to_string(), source_display);
                    }
                    if let Some(item) = item
                        && item.has_action
                    {
                        let edits = item
                            .additional_text_edits
                            .as_ref()
                            .cloned()
                            .unwrap_or_default();
                        let mut text_changes: Vec<serde_json::Value> = edits
                            .iter()
                            .map(|edit| {
                                let start = line_map
                                    .position_to_offset(edit.range.start, &source_text)
                                    .unwrap_or(0);
                                let end = line_map
                                    .position_to_offset(edit.range.end, &source_text)
                                    .unwrap_or(start);
                                let new_text = Self::normalize_mts_auto_import_edit_text(
                                    file,
                                    item.kind,
                                    &source_text,
                                    &edit.new_text,
                                );
                                serde_json::json!({
                                    "span": {
                                        "start": start,
                                        "length": end.saturating_sub(start),
                                    },
                                    "newText": Self::normalize_tsserver_newlines_for_file(
                                        &new_text,
                                        file,
                                    ),
                                })
                            })
                            .collect();
                        if item.source.as_deref() == Some("ClassMemberSnippet/")
                            && let Some(insert_text) = item.insert_text.as_deref()
                        {
                            let mut synthesized =
                                Self::class_member_snippet_synthesized_text_changes(
                                    &source_text,
                                    insert_text,
                                    &item.label,
                                    &project_items,
                                );
                            if synthesized.is_empty() {
                                synthesized = self
                                    .class_member_snippet_transitive_default_import_text_changes(
                                        file,
                                        &source_text,
                                        insert_text,
                                        &item.label,
                                    );
                            }
                            if !synthesized.is_empty() {
                                text_changes = synthesized;
                            }
                        }
                        if !text_changes.is_empty() {
                            if file.ends_with(".mts") {
                                for change in &mut text_changes {
                                    let Some(new_text) =
                                        change.get("newText").and_then(serde_json::Value::as_str)
                                    else {
                                        continue;
                                    };
                                    let Some((module_specifier, _)) =
                                        Self::parse_named_import_clause(
                                            new_text, "import {", "} from ",
                                        )
                                    else {
                                        continue;
                                    };
                                    if Self::type_only_named_imports_for_module(
                                        &source_text,
                                        module_specifier,
                                    )
                                    .is_empty()
                                    {
                                        continue;
                                    }
                                    let Some((existing_start, existing_length)) =
                                        Self::find_type_only_named_import_span(
                                            &source_text,
                                            module_specifier,
                                        )
                                    else {
                                        continue;
                                    };

                                    let start = change
                                        .get("span")
                                        .and_then(|span| span.get("start"))
                                        .and_then(serde_json::Value::as_u64)
                                        .map(|n| n as u32)
                                        .unwrap_or(0);
                                    let length = change
                                        .get("span")
                                        .and_then(|span| span.get("length"))
                                        .and_then(serde_json::Value::as_u64)
                                        .map(|n| n as u32)
                                        .unwrap_or(0);
                                    if length != 0 || start != existing_start {
                                        continue;
                                    }

                                    if let Some(change_obj) = change.as_object_mut() {
                                        change_obj.insert(
                                            "span".to_string(),
                                            serde_json::json!({
                                                "start": existing_start,
                                                "length": existing_length,
                                            }),
                                        );
                                    }
                                    break;
                                }
                            }

                            let description = if item.source.as_deref()
                                == Some("ClassMemberSnippet/")
                            {
                                format!("Includes imports of types referenced by '{}'", item.label)
                            } else {
                                Self::auto_import_code_action_description(
                                    &source_text,
                                    file,
                                    item.source.as_deref(),
                                    &edits,
                                    &item.label,
                                )
                            };

                            detail.insert(
                                "codeActions".to_string(),
                                serde_json::json!([{
                                    "description": description,
                                    "changes": [{
                                        "fileName": file,
                                        "textChanges": text_changes,
                                    }],
                                }]),
                            );
                        }
                    }
                    serde_json::Value::Object(detail)
                })
                .collect();
            Some(serde_json::json!(details))
        })();
        self.success_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    // Display parts rendering, signature help handler, and tokenization utilities
    // are in handlers_completions_display.rs
}
