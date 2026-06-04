impl Server {
    pub(super) fn find_namespace_alias_decl_offsets(
        source_text: &str,
        alias_name: &str,
    ) -> Option<(u32, u32, u32, u32)> {
        let needle = format!("import * as {alias_name}");
        let stmt_start = source_text.find(&needle)?;
        let alias_rel = needle.find(alias_name)?;
        let alias_start = stmt_start + alias_rel;
        let alias_end = alias_start + alias_name.len();
        let context_start = source_text[..stmt_start].rfind('\n').map_or(0, |i| i + 1);
        let context_end = source_text[stmt_start..]
            .find('\n')
            .map_or(source_text.len(), |i| stmt_start + i);
        Some((
            alias_start as u32,
            alias_end as u32,
            context_start as u32,
            context_end as u32,
        ))
    }

    fn find_ambient_module_offsets(
        source_text: &str,
        module_name: &str,
    ) -> Option<(u32, u32, u32, u32)> {
        for quote in ['"', '\''] {
            let needle = format!("declare module {quote}{module_name}{quote}");
            if let Some(stmt_start) = source_text.find(&needle) {
                let literal_start = stmt_start + "declare module ".len();
                let literal_end = literal_start + module_name.len() + 2;
                let context_start = source_text[..stmt_start].rfind('\n').map_or(0, |i| i + 1);
                let context_end = source_text[stmt_start..]
                    .find('\n')
                    .map_or(source_text.len(), |i| stmt_start + i);
                return Some((
                    literal_start as u32,
                    literal_end as u32,
                    context_start as u32,
                    context_end as u32,
                ));
            }
        }
        None
    }

    pub(super) fn find_ambient_module_definition_info(
        &self,
        module_name: &str,
    ) -> Option<tsz::lsp::definition::DefinitionInfo> {
        for (file_path, source_text) in &self.open_files {
            let Some((name_start, name_end, context_start, context_end)) =
                Self::find_ambient_module_offsets(source_text, module_name)
            else {
                continue;
            };
            let line_map = LineMap::build(source_text);
            let name_range = tsz::lsp::position::Range::new(
                line_map.offset_to_position(name_start, source_text),
                line_map.offset_to_position(name_end, source_text),
            );
            let context_range = tsz::lsp::position::Range::new(
                line_map.offset_to_position(context_start, source_text),
                line_map.offset_to_position(context_end, source_text),
            );
            return Some(tsz::lsp::definition::DefinitionInfo {
                location: tsz_common::position::Location {
                    file_path: file_path.clone(),
                    range: name_range,
                },
                context_span: Some(context_range),
                name: format!("\"{module_name}\""),
                kind: "module".to_string(),
                container_name: String::new(),
                container_kind: String::new(),
                is_local: false,
                is_ambient: true,
            });
        }
        None
    }

    pub(super) fn maybe_remap_alias_to_ambient_module(
        &self,
        ctx: &ParsedFileContext<'_>,
        position: tsz_common::position::Position,
        infos: &[tsz::lsp::definition::DefinitionInfo],
    ) -> Option<Vec<tsz::lsp::definition::DefinitionInfo>> {
        let interner = TypeInterner::new();
        let provider = HoverProvider::new(
            ctx.arena,
            ctx.binder,
            ctx.line_map,
            &interner,
            ctx.source_text,
            ctx.file.to_string(),
        );
        let mut type_cache = None;
        let hover = provider.get_hover(ctx.root, position, &mut type_cache);
        let mut alias_name = hover
            .as_ref()
            .and_then(|hover_info| Self::extract_alias_name(&hover_info.display_string));
        if alias_name.is_none() {
            alias_name = hover.as_ref().and_then(|hover_info| {
                let range = hover_info.range?;
                let start = ctx
                    .line_map
                    .position_to_offset(range.start, ctx.source_text)?;
                let end = ctx
                    .line_map
                    .position_to_offset(range.end, ctx.source_text)?;
                if start >= end || end as usize > ctx.source_text.len() {
                    return None;
                }
                Some(ctx.source_text[start as usize..end as usize].to_string())
            });
        }
        if alias_name.is_none() {
            alias_name = infos.first().map(|info| info.name.clone());
        }
        if alias_name
            .as_deref()
            .is_some_and(|name| name.chars().any(char::is_whitespace))
        {
            alias_name = None;
        }
        let alias_name = alias_name.or_else(|| infos.first().map(|info| info.name.clone()))?;
        let namespace_decl = Self::find_namespace_alias_decl_offsets(ctx.source_text, &alias_name);
        let offset = ctx.line_map.position_to_offset(position, ctx.source_text)?;
        let on_declaration = if let Some(first) = infos.first() {
            if first.kind != "alias" {
                return None;
            }
            match (
                ctx.line_map
                    .position_to_offset(first.location.range.start, ctx.source_text),
                ctx.line_map
                    .position_to_offset(first.location.range.end, ctx.source_text),
            ) {
                (Some(start), Some(end)) => offset >= start && offset <= end,
                _ => false,
            }
        } else if let Some((alias_start, alias_end, _, _)) = namespace_decl {
            offset >= alias_start && offset <= alias_end
        } else {
            false
        };

        // Namespace import usages should navigate to the namespace import declaration.
        if let Some((alias_start, alias_end, context_start, context_end)) = namespace_decl
            && !on_declaration
        {
            let alias_range = tsz::lsp::position::Range::new(
                ctx.line_map
                    .offset_to_position(alias_start, ctx.source_text),
                ctx.line_map.offset_to_position(alias_end, ctx.source_text),
            );
            let context_range = tsz::lsp::position::Range::new(
                ctx.line_map
                    .offset_to_position(context_start, ctx.source_text),
                ctx.line_map
                    .offset_to_position(context_end, ctx.source_text),
            );
            return Some(vec![tsz::lsp::definition::DefinitionInfo {
                location: tsz_common::position::Location {
                    file_path: ctx.file.to_string(),
                    range: alias_range,
                },
                context_span: Some(context_range),
                name: alias_name,
                kind: "alias".to_string(),
                container_name: String::new(),
                container_kind: String::new(),
                is_local: true,
                is_ambient: false,
            }]);
        }

        let module_name = hover
            .as_ref()
            .and_then(|hover_info| Self::extract_alias_module_name(&hover_info.display_string))
            .or_else(|| {
                Self::extract_module_name_from_source_for_alias(ctx.source_text, &alias_name)
            })?;

        self.find_ambient_module_definition_info(&module_name)
            .map(|info| vec![info])
    }

    pub(super) fn import_statement_context_span(
        source_text: &str,
        anchor_offset: u32,
    ) -> Option<(u32, u32)> {
        if source_text.is_empty() {
            return None;
        }
        let idx = (anchor_offset as usize).min(source_text.len().saturating_sub(1));
        let line_start = source_text[..idx].rfind('\n').map_or(0, |i| i + 1);
        let line_end = source_text[idx..]
            .find('\n')
            .map_or(source_text.len(), |i| idx + i);
        let line_text = source_text[line_start..line_end].trim_start();
        if !line_text.starts_with("import ") && !line_text.starts_with("export ") {
            return None;
        }
        Some((line_start as u32, line_end as u32))
    }
}
