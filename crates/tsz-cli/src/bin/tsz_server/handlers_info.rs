//! Hover, definition, navigation, and reference handlers for tsz-server.

use super::{Server, TsServerRequest, TsServerResponse};
use tsz::binder::SymbolId;
use tsz::lsp::definition::GoToDefinition;
use tsz::lsp::highlighting::DocumentHighlightProvider;
use tsz::lsp::hover::{HoverInfo, HoverProvider};
use tsz::lsp::implementation::GoToImplementationProvider;
use tsz::lsp::jsdoc::{jsdoc_for_node, parse_jsdoc};
use tsz::lsp::position::LineMap;
use tsz::lsp::references::FindReferences;
use tsz::lsp::rename::RenameProvider;
use tsz::lsp::signature_help::SignatureHelpProvider;
use tsz::lsp::symbols::document_symbols::DocumentSymbolProvider;
use tsz::parser::node::NodeAccess;
use tsz::parser::syntax_kind_ext;
use tsz_solver::TypeInterner;

/// Bundled context for a parsed file, reducing parameter count in helpers.
struct ParsedFileContext<'a> {
    arena: &'a tsz::parser::node::NodeArena,
    binder: &'a tsz::binder::BinderState,
    line_map: &'a LineMap,
    root: tsz::parser::NodeIndex,
    source_text: &'a str,
    file: &'a str,
}

impl Server {
    fn checker_options_for_source(source_text: &str) -> tsz::checker::context::CheckerOptions {
        let strict = source_text
            .lines()
            .take(64)
            .map(str::trim)
            .any(|line| line.contains("@strict:true") || line.contains("@strict: true"));
        tsz::checker::context::CheckerOptions {
            strict,
            no_implicit_any: strict,
            no_implicit_returns: false,
            no_implicit_this: strict,
            strict_null_checks: strict,
            strict_function_types: strict,
            strict_property_initialization: strict,
            use_unknown_in_catch_variables: strict,
            isolated_modules: false,
            ..Default::default()
        }
    }

    fn is_js_identifier_char(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
    }

    fn extract_trailing_type_name(display: &str) -> Option<String> {
        let (_, ty) = display.rsplit_once(": ")?;
        let ty = ty.trim();
        if ty.is_empty() {
            return None;
        }
        if ty
            .bytes()
            .all(|b| Self::is_js_identifier_char(b) || b == b'.')
        {
            Some(ty.to_string())
        } else {
            None
        }
    }

    fn identifier_at(source_text: &str, offset: u32) -> Option<String> {
        let bytes = source_text.as_bytes();
        let len = bytes.len() as u32;
        if offset >= len {
            return None;
        }
        let mut start = offset;
        while start > 0 && Self::is_js_identifier_char(bytes[(start - 1) as usize]) {
            start -= 1;
        }
        let mut end = start;
        while end < len && Self::is_js_identifier_char(bytes[end as usize]) {
            end += 1;
        }
        (end > start).then(|| source_text[start as usize..end as usize].to_string())
    }

    fn clean_jsdoc_comment(raw: &str) -> String {
        let inner = raw
            .trim()
            .trim_start_matches("/**")
            .trim_end_matches("*/")
            .trim();
        inner
            .lines()
            .map(|line| line.trim().trim_start_matches('*').trim())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    }

    fn find_interface_member_signature(
        source_text: &str,
        interface_name: &str,
        member_name: &str,
    ) -> Option<(String, String)> {
        let iface_pattern = format!("interface {interface_name}");
        let iface_start = source_text.find(&iface_pattern)?;
        let after_iface = &source_text[iface_start..];
        let body_start_rel = after_iface.find('{')?;
        let body_start = iface_start + body_start_rel + 1;
        let body_end = source_text[body_start..].find('}')? + body_start;
        let body = &source_text[body_start..body_end];

        let member_idx = body.find(member_name)?;
        let after_member = &body[member_idx + member_name.len()..];
        let colon_rel = after_member.find(':')?;
        let type_start = member_idx + member_name.len() + colon_rel + 1;
        let type_end = body[type_start..].find(';')? + type_start;
        let member_type = body[type_start..type_end].trim().to_string();

        let prefix = &body[..member_idx];
        let documentation = if let Some(doc_start) = prefix.rfind("/**") {
            if let Some(doc_end_rel) = prefix[doc_start..].find("*/") {
                let doc_end = doc_start + doc_end_rel + 2;
                Self::clean_jsdoc_comment(&prefix[doc_start..doc_end])
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        Some((member_type, documentation))
    }

    fn class_keyword_quickinfo_from_source(
        source_text: &str,
        line_map: &LineMap,
        position: tsz_common::position::Position,
    ) -> Option<(String, tsz_common::position::Range)> {
        let bytes = source_text.as_bytes();
        let len = bytes.len() as u32;
        let offset = line_map.position_to_offset(position, source_text)?;
        if offset >= len {
            return None;
        }
        let mut start = offset;
        while start > 0 && bytes[(start - 1) as usize].is_ascii_alphabetic() {
            start -= 1;
        }
        let mut end = offset;
        while end < len && bytes[end as usize].is_ascii_alphabetic() {
            end += 1;
        }
        if end <= start {
            return None;
        }
        let token = &source_text[start as usize..end as usize];
        if token != "class" {
            return None;
        }
        if start > 0 && Self::is_js_identifier_char(bytes[(start - 1) as usize]) {
            return None;
        }
        if end < len && Self::is_js_identifier_char(bytes[end as usize]) {
            return None;
        }

        let mut probe = end;
        while probe < len && bytes[probe as usize].is_ascii_whitespace() {
            probe += 1;
        }
        let name = if probe < len
            && ((bytes[probe as usize].is_ascii_alphabetic())
                || bytes[probe as usize] == b'_'
                || bytes[probe as usize] == b'$')
        {
            let name_start = probe;
            probe += 1;
            while probe < len && Self::is_js_identifier_char(bytes[probe as usize]) {
                probe += 1;
            }
            source_text[name_start as usize..probe as usize].to_string()
        } else {
            "(Anonymous class)".to_string()
        };

        let range = tsz_common::position::Range::new(
            line_map.offset_to_position(start, source_text),
            line_map.offset_to_position(end, source_text),
        );
        Some((format!("(local class) {name}"), range))
    }

    fn is_offset_inside_comment(source_text: &str, offset: u32) -> bool {
        let idx = offset as usize;
        if idx > source_text.len() {
            return false;
        }

        // Line comments.
        let line_start = source_text[..idx].rfind('\n').map_or(0, |i| i + 1);
        if let Some(line_comment) = source_text[line_start..idx].find("//") {
            let comment_start = line_start + line_comment;
            if comment_start <= idx {
                return true;
            }
        }

        // Block comments (including JSDoc).
        let last_open = source_text[..idx].rfind("/*");
        let last_close = source_text[..idx].rfind("*/");
        matches!((last_open, last_close), (Some(open), Some(close)) if open > close)
            || matches!((last_open, last_close), (Some(_), None))
    }

    fn member_name_node_if_matches(
        arena: &tsz::parser::node::NodeArena,
        member_idx: tsz::parser::NodeIndex,
        member_name: &str,
    ) -> Option<tsz::parser::NodeIndex> {
        let member_node = arena.get(member_idx)?;

        if let Some(sig) = arena.get_signature(member_node)
            && arena.get_identifier_text(sig.name) == Some(member_name)
        {
            return Some(sig.name);
        }

        if let Some(prop) = arena.get_property_decl(member_node)
            && arena.get_identifier_text(prop.name) == Some(member_name)
        {
            return Some(prop.name);
        }

        if let Some(method) = arena.get_method_decl(member_node)
            && arena.get_identifier_text(method.name) == Some(member_name)
        {
            return Some(method.name);
        }

        if let Some(accessor) = arena.get_accessor(member_node)
            && arena.get_identifier_text(accessor.name) == Some(member_name)
        {
            return Some(accessor.name);
        }

        None
    }

    fn find_named_member_declaration(
        arena: &tsz::parser::node::NodeArena,
        binder: &tsz::binder::BinderState,
        container_type_name: &str,
        member_name: &str,
    ) -> Option<(tsz::parser::NodeIndex, tsz::parser::NodeIndex)> {
        let mut candidate_decls = Vec::new();
        if let Some(sym_id) = binder.file_locals.get(container_type_name)
            && let Some(symbol) = binder.symbols.get(sym_id)
        {
            candidate_decls.extend(symbol.declarations.iter().copied());
        }

        if candidate_decls.is_empty() {
            for (idx, node) in arena.nodes.iter().enumerate() {
                if let Some(iface) = arena.get_interface(node)
                    && arena.get_identifier_text(iface.name) == Some(container_type_name)
                {
                    candidate_decls.push(tsz::parser::NodeIndex(idx as u32));
                    continue;
                }
                if let Some(class) = arena.get_class(node)
                    && arena.get_identifier_text(class.name) == Some(container_type_name)
                {
                    candidate_decls.push(tsz::parser::NodeIndex(idx as u32));
                }
            }
        }

        for decl_idx in candidate_decls {
            let Some(decl_node) = arena.get(decl_idx) else {
                continue;
            };

            if let Some(iface) = arena.get_interface(decl_node) {
                for &member_idx in &iface.members.nodes {
                    if let Some(name_node) =
                        Self::member_name_node_if_matches(arena, member_idx, member_name)
                    {
                        return Some((member_idx, name_node));
                    }
                }
            }

            if let Some(class) = arena.get_class(decl_node) {
                for &member_idx in &class.members.nodes {
                    if let Some(name_node) =
                        Self::member_name_node_if_matches(arena, member_idx, member_name)
                    {
                        return Some((member_idx, name_node));
                    }
                }
            }
        }

        None
    }

    fn quickinfo_member_access_declaration_hover(
        arena: &tsz::parser::node::NodeArena,
        binder: &tsz::binder::BinderState,
        line_map: &LineMap,
        source_text: &str,
        root: tsz::parser::NodeIndex,
        provider: &HoverProvider<'_>,
        type_cache: &mut Option<tsz::checker::TypeCache>,
        interner: &TypeInterner,
        file: &str,
        probe_offset: u32,
        container_type_hint: Option<&str>,
    ) -> Option<tsz::lsp::hover::HoverInfo> {
        let mut candidates = Vec::with_capacity(4);
        candidates.push(tsz::lsp::utils::find_node_at_or_before_offset(
            arena,
            probe_offset,
            source_text,
        ));
        if probe_offset > 0 {
            candidates.push(tsz::lsp::utils::find_node_at_or_before_offset(
                arena,
                probe_offset - 1,
                source_text,
            ));
        }
        if let Some(sym) =
            tsz::lsp::utils::find_symbol_query_node_at_or_before(arena, source_text, probe_offset)
        {
            candidates.push(sym);
        }
        if probe_offset > 0
            && let Some(sym) = tsz::lsp::utils::find_symbol_query_node_at_or_before(
                arena,
                source_text,
                probe_offset - 1,
            )
        {
            candidates.push(sym);
        }

        let mut selected = None;
        for node_idx in candidates {
            if node_idx.is_none() {
                continue;
            }
            let Some(node) = arena.get(node_idx) else {
                continue;
            };
            if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
                continue;
            }
            let Some(parent_idx) = arena.get_extended(node_idx).map(|e| e.parent) else {
                continue;
            };
            let Some(parent_node) = arena.get(parent_idx) else {
                continue;
            };
            if parent_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                continue;
            }
            let Some(access) = arena.get_access_expr(parent_node) else {
                continue;
            };
            if access.name_or_argument != node_idx {
                continue;
            }
            selected = Some((node_idx, access.expression));
            break;
        }
        let mut receiver_expr_idx = None;
        let member_name = if let Some((member_node_idx, receiver_idx)) = selected {
            receiver_expr_idx = Some(receiver_idx);
            arena.get_identifier_text(member_node_idx)?.to_string()
        } else {
            let hint = container_type_hint?;
            let bytes = source_text.as_bytes();
            let len = bytes.len() as u32;
            if probe_offset >= len {
                return None;
            }
            let mut start = probe_offset;
            while start > 0 && Self::is_js_identifier_char(bytes[(start - 1) as usize]) {
                start -= 1;
            }
            let mut end = start;
            while end < len && Self::is_js_identifier_char(bytes[end as usize]) {
                end += 1;
            }
            if end <= start {
                return None;
            }
            if !hint.is_empty() {
                source_text[start as usize..end as usize].to_string()
            } else {
                return None;
            }
        };

        let mut container_type_name = None;
        if let Some(receiver_expr_idx) = receiver_expr_idx
            && let Some(receiver_sym) = binder.resolve_identifier(arena, receiver_expr_idx)
            && let Some(receiver_symbol) = binder.symbols.get(receiver_sym)
        {
            let receiver_decl = if receiver_symbol.value_declaration.is_some() {
                Some(receiver_symbol.value_declaration)
            } else {
                receiver_symbol.declarations.first().copied()
            };
            if let Some(receiver_decl_idx) = receiver_decl
                && let Some(receiver_decl_node) = arena.get(receiver_decl_idx)
                && let Some(param) = arena.get_parameter(receiver_decl_node)
                && param.type_annotation.is_some()
            {
                container_type_name = arena
                    .get_identifier_text(param.type_annotation)
                    .map(std::string::ToString::to_string);

                if container_type_name.is_none()
                    && let Some(type_node) = arena.get(param.type_annotation)
                {
                    let text = source_text
                        .get(type_node.pos as usize..type_node.end as usize)
                        .map(str::trim)
                        .filter(|s| !s.is_empty());
                    container_type_name = text.map(std::string::ToString::to_string);
                }
            }
        }

        let container_type_name = if let Some(name) = container_type_name {
            name
        } else if let Some(hint) = container_type_hint {
            hint.to_string()
        } else {
            let compiler_options = tsz::checker::context::CheckerOptions::default();
            let mut checker = if let Some(cache) = type_cache.take() {
                tsz::checker::state::CheckerState::with_cache(
                    arena,
                    binder,
                    interner,
                    file.to_string(),
                    cache,
                    compiler_options,
                )
            } else {
                tsz::checker::state::CheckerState::new(
                    arena,
                    binder,
                    interner,
                    file.to_string(),
                    compiler_options,
                )
            };
            let receiver_expr_idx = receiver_expr_idx?;
            let container_ty = checker.get_type_of_node(receiver_expr_idx);
            let name = checker.format_type(container_ty);
            *type_cache = Some(checker.extract_cache());
            name
        };

        let (member_decl_idx, member_decl_name_idx) =
            Self::find_named_member_declaration(arena, binder, &container_type_name, &member_name)?;
        let member_name_node = arena.get(member_decl_name_idx)?;
        let decl_pos = line_map.offset_to_position(member_name_node.pos, source_text);
        if let Some(hover) = provider.get_hover(root, decl_pos, type_cache)
            && !hover.display_string.is_empty()
        {
            return Some(hover);
        }

        let compiler_options = Self::checker_options_for_source(source_text);
        let mut checker = if let Some(cache) = type_cache.take() {
            tsz::checker::state::CheckerState::with_cache(
                arena,
                binder,
                interner,
                file.to_string(),
                cache,
                compiler_options,
            )
        } else {
            tsz::checker::state::CheckerState::new(
                arena,
                binder,
                interner,
                file.to_string(),
                compiler_options,
            )
        };
        let member_type_id = checker.get_type_of_node(member_decl_idx);
        let member_type = checker.format_type(member_type_id);
        *type_cache = Some(checker.extract_cache());

        let display_string = format!(
            "(property) {}.{}: {}",
            container_type_name, member_name, member_type
        );
        let raw_doc = jsdoc_for_node(arena, root, member_decl_idx, source_text);
        let parsed_doc = parse_jsdoc(&raw_doc);
        let documentation = parsed_doc.summary.unwrap_or_default();
        let start = line_map.offset_to_position(member_name_node.pos, source_text);
        let end = line_map.offset_to_position(member_name_node.end, source_text);

        Some(HoverInfo {
            contents: vec![format!("```typescript\n{display_string}\n```")],
            range: Some(tsz::lsp::position::Range::new(start, end)),
            display_string,
            kind: "property".to_string(),
            kind_modifiers: String::new(),
            documentation,
            tags: Vec::new(),
        })
    }

    fn constructor_quickinfo_from_new_expression(
        arena: &tsz::parser::node::NodeArena,
        binder: &tsz::binder::BinderState,
        line_map: &LineMap,
        source_text: &str,
        root: tsz::parser::NodeIndex,
        type_cache: &mut Option<tsz::checker::TypeCache>,
        interner: &TypeInterner,
        file: &str,
        probe_offset: u32,
    ) -> Option<HoverInfo> {
        let mut current =
            tsz::lsp::utils::find_node_at_or_before_offset(arena, probe_offset, source_text);
        if !current.is_some() {
            return None;
        }
        let new_expr = loop {
            let node = arena.get(current)?;
            if node.kind == syntax_kind_ext::NEW_EXPRESSION {
                break current;
            }
            let parent = arena.get_extended(current)?.parent;
            if !parent.is_some() {
                return None;
            }
            current = parent;
        };

        let new_node = arena.get(new_expr)?;
        let call_expr = arena.get_call_expr(new_node)?;
        let callee_node = arena.get(call_expr.expression)?;
        if probe_offset < new_node.pos || probe_offset > callee_node.end {
            return None;
        }

        let call_start = new_node.pos as usize;
        let call_end = (new_node.end as usize).min(source_text.len());
        let call_text = &source_text[call_start..call_end];
        let delimiter = call_text.find('(').or_else(|| {
            if call_expr.type_arguments.is_some() {
                call_text.find('<')
            } else {
                None
            }
        })?;
        let signature_probe = (call_start + delimiter + 1) as u32;
        if signature_probe >= source_text.len() as u32 {
            return None;
        }

        let sig_provider = SignatureHelpProvider::new(
            arena,
            binder,
            line_map,
            interner,
            source_text,
            file.to_string(),
        );
        let sig_help = sig_provider.get_signature_help(
            root,
            line_map.offset_to_position(signature_probe, source_text),
            type_cache,
        )?;
        let signature = sig_help
            .signatures
            .get(sig_help.active_signature as usize)
            .or_else(|| sig_help.signatures.first())?;
        let signature_name = signature
            .label
            .split('(')
            .next()
            .map(str::trim)
            .unwrap_or_default();
        let base_name = signature_name
            .find('<')
            .map(|idx| signature_name[..idx].trim())
            .unwrap_or(signature_name);
        let generic_params: Vec<String> = signature_name
            .find('<')
            .and_then(|start| signature_name.rfind('>').map(|end| (start, end)))
            .and_then(|(start, end)| {
                (end > start + 1).then(|| {
                    signature_name[start + 1..end]
                        .split(',')
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
            })
            .unwrap_or_default();
        let mut resolved_type_args = Vec::new();
        if let Some(type_args) = &call_expr.type_arguments {
            let arg_texts: Vec<String> = type_args
                .nodes
                .iter()
                .filter_map(|&arg_idx| {
                    arena
                        .get(arg_idx)
                        .and_then(|arg| source_text.get(arg.pos as usize..arg.end as usize))
                        .map(str::trim)
                        .map(|text| text.trim_end_matches('>').trim())
                        .filter(|text| !text.is_empty())
                        .map(str::to_string)
                })
                .collect();
            if !arg_texts.is_empty() {
                resolved_type_args = arg_texts;
            }
        }

        let can_infer_from_args = generic_params
            .iter()
            .any(|param| signature.label.contains(&format!(": {param}")));
        if resolved_type_args.is_empty() && !generic_params.is_empty() && can_infer_from_args {
            let compiler_options = tsz::checker::context::CheckerOptions::default();
            let mut checker = if let Some(cache) = type_cache.take() {
                tsz::checker::state::CheckerState::with_cache(
                    arena,
                    binder,
                    interner,
                    file.to_string(),
                    cache,
                    compiler_options,
                )
            } else {
                tsz::checker::state::CheckerState::new(
                    arena,
                    binder,
                    interner,
                    file.to_string(),
                    compiler_options,
                )
            };
            if let Some(arguments) = &call_expr.arguments {
                for arg_idx in arguments.nodes.iter().take(generic_params.len()) {
                    let arg_type_id = checker.get_type_of_node(*arg_idx);
                    resolved_type_args.push(checker.format_type(arg_type_id));
                }
            }
            *type_cache = Some(checker.extract_cache());
            while resolved_type_args.len() < generic_params.len() {
                resolved_type_args.push("unknown".to_string());
            }
        }

        let instance_type = if !generic_params.is_empty() {
            let mut args = resolved_type_args.clone();
            while args.len() < generic_params.len() {
                args.push("unknown".to_string());
            }
            format!("{base_name}<{}>", args.join(", "))
        } else {
            let compiler_options = tsz::checker::context::CheckerOptions::default();
            let mut checker = if let Some(cache) = type_cache.take() {
                tsz::checker::state::CheckerState::with_cache(
                    arena,
                    binder,
                    interner,
                    file.to_string(),
                    cache,
                    compiler_options,
                )
            } else {
                tsz::checker::state::CheckerState::new(
                    arena,
                    binder,
                    interner,
                    file.to_string(),
                    compiler_options,
                )
            };
            let instance_type_id = checker.get_type_of_node(new_expr);
            let instance_type = checker.format_type(instance_type_id);
            *type_cache = Some(checker.extract_cache());
            instance_type
        };
        let mut params_segment = signature
            .label
            .find('(')
            .and_then(|open| {
                signature.label.rfind("):").map(|end| {
                    let close = end;
                    signature.label[open..=close].to_string()
                })
            })
            .unwrap_or_else(|| "()".to_string());
        for (idx, param_name) in generic_params.iter().enumerate() {
            let replacement = resolved_type_args
                .get(idx)
                .map(String::as_str)
                .unwrap_or("unknown");
            params_segment =
                params_segment.replace(&format!(": {param_name}"), &format!(": {replacement}"));
        }
        let display_string = format!(
            "constructor {}{}: {}",
            instance_type, params_segment, instance_type
        );

        let start = line_map.offset_to_position(callee_node.pos, source_text);
        let end = line_map.offset_to_position(callee_node.end, source_text);
        Some(HoverInfo {
            contents: vec![format!("```typescript\n{display_string}\n```")],
            range: Some(tsz::lsp::position::Range::new(start, end)),
            display_string,
            kind: "constructor".to_string(),
            kind_modifiers: String::new(),
            documentation: String::new(),
            tags: Vec::new(),
        })
    }

    fn split_top_level_arrow_signature(sig: &str) -> Option<(&str, &str)> {
        let bytes = sig.as_bytes();
        let mut depth = 0i32;
        let mut i = 0usize;
        let mut arrow_pos = None;
        while i + 1 < bytes.len() {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => depth = depth.saturating_sub(1),
                b'=' if bytes[i + 1] == b'>' && depth == 0 => {
                    arrow_pos = Some(i);
                }
                _ => {}
            }
            i += 1;
        }
        let i = arrow_pos?;
        let params = sig[..i].trim();
        let ret = sig[i + 2..].trim();
        Some((params, ret))
    }

    fn arrow_function_display_string(type_text: &str) -> Option<String> {
        let trimmed = type_text.trim();
        let (params, ret) = Self::split_top_level_arrow_signature(trimmed)?;
        if !(params.starts_with('(') && params.ends_with(')')) || ret.is_empty() {
            return None;
        }
        Some(format!("function{params}: {ret}"))
    }

    fn quickinfo_from_arrow_token(
        arena: &tsz::parser::node::NodeArena,
        binder: &tsz::binder::BinderState,
        line_map: &LineMap,
        source_text: &str,
        root: tsz::parser::NodeIndex,
        provider: &HoverProvider<'_>,
        type_cache: &mut Option<tsz::checker::TypeCache>,
        interner: &TypeInterner,
        file: &str,
        probe_offset: u32,
    ) -> Option<HoverInfo> {
        let bytes = source_text.as_bytes();
        let len = bytes.len() as u32;
        if len < 2 || probe_offset >= len {
            return None;
        }

        let search_start = probe_offset.saturating_sub(2);
        let search_end = (probe_offset + 2).min(len.saturating_sub(1));
        let mut arrow_start = None;
        let mut cursor = search_start;
        while cursor < search_end {
            if bytes[cursor as usize] == b'=' && bytes[(cursor + 1) as usize] == b'>' {
                arrow_start = Some(cursor);
                break;
            }
            cursor += 1;
        }
        let Some(arrow_start) = arrow_start else {
            return None;
        };

        let mut current =
            tsz::lsp::utils::find_node_at_or_before_offset(arena, arrow_start + 1, source_text);
        if !current.is_some() {
            return None;
        }

        let arrow_fn = loop {
            let node = arena.get(current)?;
            if node.kind == syntax_kind_ext::ARROW_FUNCTION {
                break current;
            }
            let parent = arena.get_extended(current)?.parent;
            if !parent.is_some() {
                return None;
            }
            current = parent;
        };
        let arrow_fn_node = arena.get(arrow_fn)?;
        if arrow_start < arrow_fn_node.pos || arrow_start + 1 > arrow_fn_node.end {
            return None;
        }

        let compiler_options = tsz::checker::context::CheckerOptions::default();
        let mut checker = if let Some(cache) = type_cache.take() {
            tsz::checker::state::CheckerState::with_cache(
                arena,
                binder,
                interner,
                file.to_string(),
                cache,
                compiler_options,
            )
        } else {
            tsz::checker::state::CheckerState::new(
                arena,
                binder,
                interner,
                file.to_string(),
                compiler_options,
            )
        };
        let arrow_type = checker.get_type_of_node(arrow_fn);
        let type_text = checker.format_type(arrow_type);
        *type_cache = Some(checker.extract_cache());

        let return_type = Self::arrow_return_type_from_type_text(&type_text)?;
        let display_string = Self::contextual_arrow_display_string(
            arena,
            line_map,
            source_text,
            root,
            provider,
            type_cache,
            arrow_fn,
            arrow_start,
            &return_type,
        )
        .or_else(|| Self::arrow_function_display_string(&type_text))?;
        let start = line_map.offset_to_position(arrow_start, source_text);
        let end = line_map.offset_to_position((arrow_start + 2).min(len), source_text);
        Some(HoverInfo {
            contents: vec![format!("```typescript\n{display_string}\n```")],
            range: Some(tsz::lsp::position::Range::new(start, end)),
            display_string,
            kind: "function".to_string(),
            kind_modifiers: String::new(),
            documentation: String::new(),
            tags: Vec::new(),
        })
    }

    fn parse_hover_parameter_type(display: &str, param_name: &str) -> Option<String> {
        let prefix = format!("(parameter) {param_name}: ");
        display
            .strip_prefix(&prefix)
            .map(str::trim)
            .filter(|ty| !ty.is_empty())
            .map(str::to_string)
    }

    fn normalize_union_type_text(ty: &str) -> String {
        let mut parts: Vec<String> = ty
            .split('|')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect();
        if parts.len() <= 1 {
            return ty.trim().to_string();
        }
        parts.sort_by_key(|part| match part.as_str() {
            "string" => 0u8,
            "number" => 1u8,
            "boolean" => 2u8,
            "bigint" => 3u8,
            "symbol" => 4u8,
            "undefined" => 5u8,
            "null" => 6u8,
            _ => 7u8,
        });
        parts.dedup();
        parts.join(" | ")
    }

    fn normalize_parameter_type_text(ty: &str) -> String {
        let head = ty.split(") =>").next().unwrap_or(ty).trim();
        Self::normalize_union_type_text(head)
    }

    fn normalize_quickinfo_display_string(display: &str) -> String {
        let trimmed = display.trim();
        if !trimmed.starts_with("function(") {
            return trimmed.to_string();
        }
        let Some(ret_sep) = trimmed.rfind("): ") else {
            return trimmed.to_string();
        };
        let ret = trimmed[ret_sep + 3..].trim();
        let params_with_name = &trimmed["function(".len()..ret_sep];
        let params_clean = params_with_name
            .split(") =>")
            .next()
            .unwrap_or(params_with_name)
            .trim();
        let Some((name, ty)) = params_clean.split_once(':') else {
            return trimmed.to_string();
        };
        let name = name.trim();
        if name.is_empty() {
            return trimmed.to_string();
        }
        let ty = Self::normalize_parameter_type_text(ty);
        format!("function({name}: {ty}): {ret}")
    }

    fn strip_outer_parens(mut text: &str) -> &str {
        loop {
            let trimmed = text.trim();
            if !(trimmed.starts_with('(') && trimmed.ends_with(')')) {
                return trimmed;
            }
            let bytes = trimmed.as_bytes();
            let mut depth = 0i32;
            let mut balanced = true;
            for (i, b) in bytes.iter().enumerate() {
                match *b {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 && i + 1 < bytes.len() {
                            balanced = false;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if !balanced || depth != 0 {
                return trimmed;
            }
            text = &trimmed[1..trimmed.len() - 1];
        }
    }

    fn split_top_level_bytes(text: &str, sep: u8) -> Vec<String> {
        let mut parts = Vec::new();
        let bytes = text.as_bytes();
        let mut start = 0usize;
        let mut depth = 0i32;
        for (i, b) in bytes.iter().enumerate() {
            match *b {
                b'(' => depth += 1,
                b')' => depth = depth.saturating_sub(1),
                c if c == sep && depth == 0 => {
                    parts.push(text[start..i].trim().to_string());
                    start = i + 1;
                }
                _ => {}
            }
        }
        parts.push(text[start..].trim().to_string());
        parts
    }

    fn extract_first_param_type_from_fn_type(type_text: &str) -> Option<(String, bool)> {
        let trimmed = Self::strip_outer_parens(type_text);
        let open = trimmed.find('(')?;
        let mut depth = 0i32;
        let mut close = None;
        for (i, ch) in trimmed[open..].char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(open + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close = close?;
        let params = trimmed[open + 1..close].trim();
        if params.is_empty() {
            return None;
        }
        let first_param = Self::split_top_level_bytes(params, b',')
            .into_iter()
            .next()?;
        let (name_part, type_part) = first_param.split_once(':')?;
        let is_optional = name_part.trim().ends_with('?');
        let ty = type_part.trim();
        (!ty.is_empty()).then(|| (ty.to_string(), is_optional))
    }

    fn contextual_first_parameter_type_from_text(type_text: &str) -> Option<String> {
        let type_text = type_text.trim();
        if type_text.is_empty() {
            return None;
        }
        let mut union_parts = Vec::new();
        for part in Self::split_top_level_bytes(type_text, b'&') {
            let Some((ty, optional)) = Self::extract_first_param_type_from_fn_type(&part) else {
                continue;
            };
            if !union_parts.iter().any(|existing| existing == &ty) {
                union_parts.push(ty);
            }
            if optional && !union_parts.iter().any(|existing| existing == "undefined") {
                union_parts.push("undefined".to_string());
            }
        }
        if union_parts.is_empty() {
            return None;
        }
        Some(Self::normalize_union_type_text(&union_parts.join(" | ")))
    }

    fn contextual_first_parameter_type_from_assignment(
        source_text: &str,
        arrow_start: u32,
    ) -> Option<String> {
        let before = source_text.get(..arrow_start as usize)?;
        let bytes = before.as_bytes();
        let mut depth = 0i32;
        let mut top_level_eq = None;
        let mut top_level_colon = None;
        for (i, b) in bytes.iter().enumerate() {
            match *b {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth = depth.saturating_sub(1),
                b'=' if depth == 0 => {
                    let next = bytes.get(i + 1).copied();
                    if next != Some(b'>') {
                        top_level_eq = Some(i);
                    }
                }
                b':' if depth == 0 => top_level_colon = Some(i),
                _ => {}
            }
        }
        let eq = top_level_eq?;
        let before_eq = &before[..eq];
        let colon = top_level_colon?;
        let type_text = before_eq.get(colon + 1..)?.trim();
        Self::contextual_first_parameter_type_from_text(type_text)
    }

    fn contextual_first_parameter_type_from_var_annotation(
        arena: &tsz::parser::node::NodeArena,
        source_text: &str,
        mut node_idx: tsz::parser::NodeIndex,
    ) -> Option<String> {
        let var_decl_idx = loop {
            let parent = arena.get_extended(node_idx)?.parent;
            if !parent.is_some() {
                return None;
            }
            let parent_node = arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                break parent;
            }
            node_idx = parent;
        };
        let var_decl_node = arena.get(var_decl_idx)?;
        let var_decl = arena.get_variable_declaration(var_decl_node)?;
        if !var_decl.type_annotation.is_some() {
            return None;
        }
        let type_node = arena.get(var_decl.type_annotation)?;
        let type_text = source_text.get(type_node.pos as usize..type_node.end as usize)?;
        Self::contextual_first_parameter_type_from_text(type_text)
    }

    fn arrow_return_type_from_type_text(type_text: &str) -> Option<String> {
        let ret = type_text.rsplit("=>").next()?.trim();
        let ret = ret.trim_end_matches(')').trim();
        (!ret.is_empty()).then(|| ret.to_string())
    }

    fn contextual_arrow_display_string(
        arena: &tsz::parser::node::NodeArena,
        line_map: &LineMap,
        source_text: &str,
        root: tsz::parser::NodeIndex,
        provider: &HoverProvider<'_>,
        type_cache: &mut Option<tsz::checker::TypeCache>,
        arrow_fn: tsz::parser::NodeIndex,
        arrow_start: u32,
        return_type: &str,
    ) -> Option<String> {
        let arrow_node = arena.get(arrow_fn)?;
        let arrow = arena.get_function(arrow_node)?;
        let mut params = Vec::new();
        let contextual_first_param =
            Self::contextual_first_parameter_type_from_var_annotation(arena, source_text, arrow_fn)
                .or_else(|| {
                    Self::contextual_first_parameter_type_from_assignment(source_text, arrow_start)
                });

        if arrow.parameters.nodes.len() == 1
            && let Some(contextual) = contextual_first_param.as_ref()
            && let Some(param_idx) = arrow.parameters.nodes.first()
            && let Some(param_node) = arena.get(*param_idx)
            && let Some(param) = arena.get_parameter(param_node)
            && let Some(name) = arena.get_identifier_text(param.name)
        {
            return Some(format!("function({name}: {contextual}): {return_type}"));
        }

        for (param_position, param_idx) in arrow.parameters.nodes.iter().enumerate() {
            let param_node = arena.get(*param_idx)?;
            let param = arena.get_parameter(param_node)?;
            let name_node = arena.get(param.name)?;
            let name = arena.get_identifier_text(param.name)?.to_string();
            let pos = line_map.offset_to_position(name_node.pos, source_text);
            let hover = provider.get_hover(root, pos, type_cache)?;
            let mut ty = Self::normalize_parameter_type_text(&Self::parse_hover_parameter_type(
                &hover.display_string,
                &name,
            )?);
            if ty == "any"
                && param_position == 0
                && let Some(contextual) = contextual_first_param.as_ref()
            {
                ty = contextual.clone();
            }
            params.push(format!("{name}: {ty}"));
        }

        Some(format!("function({}): {}", params.join(", "), return_type))
    }

    fn extract_alias_module_name(display_string: &str) -> Option<String> {
        let prefix = "(alias) module \"";
        let rest = display_string.strip_prefix(prefix)?;
        let end = rest.find('"')?;
        Some(rest[..end].to_string())
    }

    fn extract_alias_name(display_string: &str) -> Option<String> {
        let import_line = display_string
            .lines()
            .find(|line| line.starts_with("import "))?;
        let rest = import_line.strip_prefix("import ")?;
        if let Some(eq_idx) = rest.find(" = ") {
            return Some(rest[..eq_idx].trim().to_string());
        }
        let end = rest
            .find(|c: char| c.is_whitespace() || c == ',' || c == '{' || c == ';')
            .unwrap_or(rest.len());
        if end == 0 {
            return None;
        }
        Some(rest[..end].trim().to_string())
    }

    fn extract_quoted_after(haystack: &str, token: &str) -> Option<String> {
        let idx = haystack.find(token)?;
        let after = &haystack[idx + token.len()..];
        for quote in ['"', '\''] {
            if let Some(start) = after.find(quote) {
                let rem = &after[start + 1..];
                if let Some(end) = rem.find(quote) {
                    return Some(rem[..end].to_string());
                }
            }
        }
        None
    }

    fn extract_module_name_from_source_for_alias(
        source_text: &str,
        alias_name: &str,
    ) -> Option<String> {
        for line in source_text.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with("import ") || !trimmed.contains(alias_name) {
                continue;
            }
            if let Some(module_name) = Self::extract_quoted_after(trimmed, "require(") {
                return Some(module_name);
            }
            if let Some(module_name) = Self::extract_quoted_after(trimmed, " from ") {
                return Some(module_name);
            }
        }
        None
    }

    fn find_namespace_alias_decl_offsets(
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

    fn find_ambient_module_definition_info(
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

    fn maybe_remap_alias_to_ambient_module(
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

    pub(crate) fn handle_quickinfo(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let interner = TypeInterner::new();
            let provider = HoverProvider::new(
                &arena,
                &binder,
                &line_map,
                &interner,
                &source_text,
                file.clone(),
            );
            let mut type_cache = None;
            let mut info = provider.get_hover(root, position, &mut type_cache);
            let bytes = source_text.as_bytes();
            if let Some(base_offset) = line_map.position_to_offset(position, &source_text) {
                let len = bytes.len() as u32;

                // Fourslash quickinfo markers are commonly comment-based (`/*1*/`).
                // Probe the identifier immediately after the marker so we don't keep
                // a weaker hover result (e.g. contextual parameter type falling back to `any`).
                if base_offset + 1 < len
                    && bytes[base_offset as usize] == b'/'
                    && bytes[(base_offset + 1) as usize] == b'*'
                {
                    let mut probe = base_offset + 2;
                    while probe + 1 < len {
                        if bytes[probe as usize] == b'*' && bytes[(probe + 1) as usize] == b'/' {
                            probe += 2;
                            break;
                        }
                        probe += 1;
                    }
                    while probe < len && bytes[probe as usize].is_ascii_whitespace() {
                        probe += 1;
                    }
                    if probe < len {
                        let probe_pos = line_map.offset_to_position(probe, &source_text);
                        if let Some(marker_hover) =
                            provider.get_hover(root, probe_pos, &mut type_cache)
                        {
                            let should_replace = info.as_ref().is_none_or(|existing| {
                                existing.display_string.contains(": any")
                                    && !marker_hover.display_string.contains(": any")
                            });
                            if should_replace {
                                info = Some(marker_hover);
                            }
                        }
                    }
                }

                let mut ctor_probe = base_offset;
                while ctor_probe < len && bytes[ctor_probe as usize].is_ascii_whitespace() {
                    ctor_probe += 1;
                }
                if let Some(ctor_hover) = Self::constructor_quickinfo_from_new_expression(
                    &arena,
                    &binder,
                    &line_map,
                    &source_text,
                    root,
                    &mut type_cache,
                    &interner,
                    &file,
                    ctor_probe,
                ) && info.as_ref().is_none_or(|hover| {
                    hover.kind == "class"
                        || hover.display_string.starts_with("(local class)")
                        || hover.display_string.starts_with("class ")
                }) {
                    info = Some(ctor_hover);
                }

                // On `.`/`?.` cursor positions, tsserver quickinfo resolves the RHS member.
                let mut rhs_member_probe = None;
                if base_offset < len {
                    let mut rhs_probe = None;
                    let current = bytes[base_offset as usize];
                    if current == b'.' {
                        rhs_probe = Some(base_offset + 1);
                    } else if current == b'?'
                        && base_offset + 1 < len
                        && bytes[(base_offset + 1) as usize] == b'.'
                    {
                        rhs_probe = Some(base_offset + 2);
                    }
                    if let Some(mut probe) = rhs_probe {
                        while probe < len && bytes[probe as usize].is_ascii_whitespace() {
                            probe += 1;
                        }
                        if probe < len {
                            rhs_member_probe = Some(probe);
                            let probe_pos = line_map.offset_to_position(probe, &source_text);
                            if let Some(member_hover) =
                                provider.get_hover(root, probe_pos, &mut type_cache)
                            {
                                info = Some(member_hover);
                            }
                        }
                    }
                }

                if let Some(member_probe) = rhs_member_probe
                    && info
                        .as_ref()
                        .is_none_or(|h| !h.display_string.starts_with("(property)"))
                    && let Some(existing) = info.as_ref()
                    && let Some(container_hint) =
                        Self::extract_trailing_type_name(&existing.display_string)
                    && let Some(member_name) = Self::identifier_at(&source_text, member_probe)
                    && let Some((member_type, member_doc)) = Self::find_interface_member_signature(
                        &source_text,
                        &container_hint,
                        &member_name,
                    )
                {
                    let display_string = format!(
                        "(property) {}.{}: {}",
                        container_hint, member_name, member_type
                    );
                    let start = line_map.offset_to_position(member_probe, &source_text);
                    let end = line_map
                        .offset_to_position(member_probe + member_name.len() as u32, &source_text);
                    info = Some(HoverInfo {
                        contents: vec![format!("```typescript\n{display_string}\n```")],
                        range: Some(tsz::lsp::position::Range::new(start, end)),
                        display_string,
                        kind: "property".to_string(),
                        kind_modifiers: String::new(),
                        documentation: member_doc,
                        tags: Vec::new(),
                    });
                }

                if let Some(member_probe) = rhs_member_probe
                    && info
                        .as_ref()
                        .is_none_or(|h| !h.display_string.starts_with("(property)"))
                    && let Some(container_hint) = info
                        .as_ref()
                        .and_then(|h| Self::extract_trailing_type_name(&h.display_string))
                    && let Some(member_hover) = Self::quickinfo_member_access_declaration_hover(
                        &arena,
                        &binder,
                        &line_map,
                        &source_text,
                        root,
                        &provider,
                        &mut type_cache,
                        &interner,
                        &file,
                        member_probe.saturating_add(1),
                        Some(container_hint.as_str()),
                    )
                {
                    info = Some(member_hover);
                }

                if info.is_none() {
                    // Fourslash markers can land at the first character of a member name
                    // (`x./**/m()`), where direct symbol lookup may miss the property.
                    // Probe one character forward so hover can backtrack from `(` to `m`.
                    if base_offset < len {
                        let current = bytes[base_offset as usize];
                        if (current.is_ascii_alphanumeric() || current == b'_' || current == b'$')
                            && base_offset > 0
                            && bytes[(base_offset - 1) as usize] == b'.'
                            && base_offset + 1 < len
                        {
                            let probe_pos =
                                line_map.offset_to_position(base_offset + 1, &source_text);
                            info = provider.get_hover(root, probe_pos, &mut type_cache);
                        }
                    }
                }

                if info.is_none() && base_offset < len {
                    let current = bytes[base_offset as usize];
                    if Self::is_js_identifier_char(current)
                        && base_offset > 0
                        && bytes[(base_offset - 1) as usize] == b'.'
                        && let Some(member_name) = Self::identifier_at(&source_text, base_offset)
                    {
                        let dot_pos = line_map.offset_to_position(base_offset - 1, &source_text);
                        if let Some(lhs_hover) = provider.get_hover(root, dot_pos, &mut type_cache)
                            && let Some(container_hint) =
                                Self::extract_trailing_type_name(&lhs_hover.display_string)
                            && let Some((member_type, member_doc)) =
                                Self::find_interface_member_signature(
                                    &source_text,
                                    &container_hint,
                                    &member_name,
                                )
                        {
                            let display_string = format!(
                                "(property) {}.{}: {}",
                                container_hint, member_name, member_type
                            );
                            let start = line_map.offset_to_position(base_offset, &source_text);
                            let end = line_map.offset_to_position(
                                base_offset + member_name.len() as u32,
                                &source_text,
                            );
                            info = Some(HoverInfo {
                                contents: vec![format!("```typescript\n{display_string}\n```")],
                                range: Some(tsz::lsp::position::Range::new(start, end)),
                                display_string,
                                kind: "property".to_string(),
                                kind_modifiers: String::new(),
                                documentation: member_doc,
                                tags: Vec::new(),
                            });
                        }
                    }
                }

                if info.is_none() {
                    // tsserver/fourslash markers may place cursor on punctuation directly
                    // adjacent to the symbol token (e.g. `x./**/m`). Probe nearby offsets
                    // to recover the expected symbol hover without broad behavior changes.
                    let mut probes = [base_offset; 3];
                    let mut probe_count = 0usize;
                    if base_offset < len {
                        let current = bytes[base_offset as usize];
                        if current == b'.'
                            || current == b'?'
                            || current == b':'
                            || current == b'('
                            || current == b')'
                            || current == b','
                            || current.is_ascii_whitespace()
                        {
                            if base_offset + 1 < len {
                                probes[probe_count] = base_offset + 1;
                                probe_count += 1;
                            }
                            if base_offset > 0 {
                                probes[probe_count] = base_offset - 1;
                                probe_count += 1;
                            }
                        }
                    } else if base_offset > 0 {
                        probes[probe_count] = base_offset - 1;
                        probe_count += 1;
                    }

                    for probe_offset in probes.into_iter().take(probe_count) {
                        let probe_pos = line_map.offset_to_position(probe_offset, &source_text);
                        info = provider.get_hover(root, probe_pos, &mut type_cache);
                        if info.is_some() {
                            break;
                        }
                    }
                }

                let mut arrow_probes = [base_offset; 7];
                let mut probe_count = 0usize;
                for delta in [0i32, -1, 1, -2, 2, -3, 3] {
                    let candidate = if delta < 0 {
                        base_offset.saturating_sub((-delta) as u32)
                    } else {
                        base_offset.saturating_add(delta as u32)
                    };
                    if candidate < len {
                        arrow_probes[probe_count] = candidate;
                        probe_count += 1;
                    }
                }
                for probe in arrow_probes.into_iter().take(probe_count) {
                    if let Some(arrow_hover) = Self::quickinfo_from_arrow_token(
                        &arena,
                        &binder,
                        &line_map,
                        &source_text,
                        root,
                        &provider,
                        &mut type_cache,
                        &interner,
                        &file,
                        probe,
                    ) {
                        info = Some(arrow_hover);
                        break;
                    }
                }

                if info.is_none()
                    && let Some(member_hover) = Self::quickinfo_member_access_declaration_hover(
                        &arena,
                        &binder,
                        &line_map,
                        &source_text,
                        root,
                        &provider,
                        &mut type_cache,
                        &interner,
                        &file,
                        base_offset.saturating_add(1),
                        None,
                    )
                {
                    info = Some(member_hover);
                }

                if info.is_none() {
                    // Fallback: when direct hover misses (common for some member-access
                    // usage positions), reuse definition resolution and hover the declaration.
                    let def_provider =
                        GoToDefinition::new(&arena, &binder, &line_map, file.clone(), &source_text);
                    if let Some(defs) = def_provider.get_definition_info(root, position)
                        && let Some(first_def) = defs.first()
                        && first_def.location.file_path == file
                    {
                        info = provider.get_hover(
                            root,
                            first_def.location.range.start,
                            &mut type_cache,
                        );
                    }
                }
            }
            let info = match info {
                Some(info) => info,
                None => {
                    if let Some((display_string, range)) =
                        Self::class_keyword_quickinfo_from_source(&source_text, &line_map, position)
                    {
                        return Some(serde_json::json!({
                            "displayString": display_string,
                            "documentation": serde_json::json!([]),
                            "kind": "class",
                            "kindModifiers": "",
                            "tags": [],
                            "start": Self::lsp_to_tsserver_position(range.start),
                            "end": Self::lsp_to_tsserver_position(range.end),
                        }));
                    }
                    return None;
                }
            };

            // Use structured fields from HoverInfo when available,
            // falling back to parsing from markdown contents
            let mut display_string = if !info.display_string.is_empty() {
                info.display_string.clone()
            } else {
                info.contents
                    .iter()
                    .find(|c| c.contains("```"))
                    .map(|c| {
                        c.replace("```typescript\n", "")
                            .replace("\n```", "")
                            .trim()
                            .to_string()
                    })
                    .unwrap_or_default()
            };
            display_string = Self::normalize_quickinfo_display_string(&display_string);

            let documentation = if !info.documentation.is_empty() {
                info.documentation.clone()
            } else {
                info.contents
                    .iter()
                    .find(|c| !c.contains("```"))
                    .cloned()
                    .unwrap_or_default()
            };

            let kind = if !info.kind.is_empty() {
                info.kind.clone()
            } else {
                "unknown".to_string()
            };

            let kind_modifiers = info.kind_modifiers.clone();

            let range = info
                .range
                .unwrap_or_else(|| tsz::lsp::position::Range::new(position, position));
            // Build tags array from JSDoc tags when available
            let tags: Vec<serde_json::Value> = info
                .tags
                .iter()
                .map(|tag| {
                    serde_json::json!({
                        "name": tag.name,
                        "text": tag.text,
                    })
                })
                .collect();

            // Return documentation as a structured display parts array when non-empty,
            // or empty string when there's no documentation. The SessionClient handles
            // string documentation by wrapping in [{kind:"text", text:doc}].
            // When doc is "", that creates [{kind:"text",text:""}] (length 1) which
            // causes an unwanted blank line in baseline output.
            // Return as empty array [] to avoid the blank line.
            let doc_value: serde_json::Value = if documentation.is_empty() {
                serde_json::json!([])
            } else {
                serde_json::json!([{"kind": "text", "text": documentation}])
            };

            Some(serde_json::json!({
                "displayString": display_string,
                "documentation": doc_value,
                "kind": kind,
                "kindModifiers": kind_modifiers,
                "tags": tags,
                "start": Self::lsp_to_tsserver_position(range.start),
                "end": Self::lsp_to_tsserver_position(range.end),
            }))
        })();

        // When quickinfo fails to resolve, return a response with valid start/end
        // spans. The harness accesses body.start.line and body.end.line, so an
        // empty object {} would cause "Cannot read properties of undefined".
        let fallback = (|| -> Option<serde_json::Value> {
            let (_, line, offset) = Self::extract_file_position(&request.arguments)?;
            let position = Self::tsserver_to_lsp_position(line, offset);
            Some(serde_json::json!({
                "displayString": "",
                "documentation": "",
                "kind": "",
                "kindModifiers": "",
                "tags": [],
                "start": Self::lsp_to_tsserver_position(position),
                "end": Self::lsp_to_tsserver_position(position),
            }))
        })();
        self.stub_response(
            seq,
            request,
            result.or(fallback).or(Some(serde_json::json!({
                "displayString": "",
                "documentation": "",
                "kind": "",
                "kindModifiers": "",
                "tags": [],
                "start": {"line": 1, "offset": 1},
                "end": {"line": 1, "offset": 1},
            }))),
        )
    }

    pub(crate) fn handle_definition(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let offset = line_map.position_to_offset(position, &source_text)?;
            if Self::is_offset_inside_comment(&source_text, offset) {
                return None;
            }
            let provider =
                GoToDefinition::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let mut infos = provider
                .get_definition_info(root, position)
                .unwrap_or_default();
            let file_ctx = ParsedFileContext {
                arena: &arena,
                binder: &binder,
                line_map: &line_map,
                root,
                source_text: &source_text,
                file: &file,
            };
            if let Some(remapped) =
                self.maybe_remap_alias_to_ambient_module(&file_ctx, position, &infos)
            {
                infos = remapped;
            }
            if infos.is_empty() {
                return None;
            }
            let body: Vec<serde_json::Value> = infos
                .iter()
                .map(|info| Self::definition_info_to_json(info, &file))
                .collect();
            Some(serde_json::json!(body))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_definition_and_bound_span(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let offset = line_map.position_to_offset(position, &source_text)?;
            if Self::is_offset_inside_comment(&source_text, offset) {
                return None;
            }
            let provider =
                GoToDefinition::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let mut infos = provider
                .get_definition_info(root, position)
                .unwrap_or_default();
            let file_ctx = ParsedFileContext {
                arena: &arena,
                binder: &binder,
                line_map: &line_map,
                root,
                source_text: &source_text,
                file: &file,
            };
            if let Some(remapped) =
                self.maybe_remap_alias_to_ambient_module(&file_ctx, position, &infos)
            {
                infos = remapped;
            }
            if infos.is_empty() {
                return None;
            }

            // Build definitions array with rich metadata
            let definitions: Vec<serde_json::Value> = infos
                .iter()
                .map(|info| Self::definition_info_to_json(info, &file))
                .collect();

            // Compute textSpan from hover range for symbol-accurate bound spans.
            let interner = TypeInterner::new();
            let hover_provider =
                HoverProvider::new(&arena, &binder, &line_map, &interner, &source_text, file);
            let mut type_cache = None;
            let hover_range = hover_provider
                .get_hover(root, position, &mut type_cache)
                .and_then(|info| info.range)
                .filter(|range| range.start != range.end);
            let symbol_range = hover_range.or_else(|| {
                let mut probe = line_map.position_to_offset(position, &source_text)?;
                let max = source_text.len() as u32;
                let mut remaining = 256u32;
                while probe < max && remaining > 0 {
                    let node_idx =
                        tsz::lsp::utils::find_node_at_or_before_offset(&arena, probe, &source_text);
                    if node_idx.is_some()
                        && tsz::lsp::utils::is_symbol_query_node(&arena, node_idx)
                        && let Some(node) = arena.get(node_idx)
                    {
                        let start = line_map.offset_to_position(node.pos, &source_text);
                        let end = line_map.offset_to_position(node.end, &source_text);
                        if start != end {
                            return Some(tsz::lsp::position::Range::new(start, end));
                        }
                    }

                    let ch = source_text.as_bytes()[probe as usize];
                    if ch == b'\n' || ch == b'\r' {
                        break;
                    }
                    probe += 1;
                    remaining -= 1;
                }
                None
            });
            let text_span = symbol_range
                .map(|range| {
                    serde_json::json!({
                        "start": Self::lsp_to_tsserver_position(range.start),
                        "end": Self::lsp_to_tsserver_position(range.end),
                    })
                })
                .unwrap_or_else(|| {
                    serde_json::json!({
                        "start": Self::lsp_to_tsserver_position(position),
                        "end": Self::lsp_to_tsserver_position(position),
                    })
                });

            Some(serde_json::json!({
                "definitions": definitions,
                "textSpan": text_span,
            }))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({
                "definitions": [],
                "textSpan": {"start": {"line": 1, "offset": 1}, "end": {"line": 1, "offset": 1}}
            }))),
        )
    }

    pub(crate) fn handle_references(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let provider = FindReferences::new(&arena, &binder, &line_map, file, &source_text);
            let (_symbol_id, ref_infos) = provider.find_references_with_symbol(root, position)?;

            // Try to get symbol name from the position
            let symbol_name = {
                let ref_offset = line_map.position_to_offset(position, &source_text)?;
                let node_idx = tsz::lsp::utils::find_node_at_offset(&arena, ref_offset);
                if node_idx.is_some() {
                    arena
                        .get_identifier_text(node_idx)
                        .map(std::string::ToString::to_string)
                } else {
                    None
                }
            }
            .unwrap_or_default();

            let refs: Vec<serde_json::Value> = ref_infos
                .iter()
                .map(|ref_info| {
                    serde_json::json!({
                        "file": ref_info.location.file_path,
                        "start": Self::lsp_to_tsserver_position(ref_info.location.range.start),
                        "end": Self::lsp_to_tsserver_position(ref_info.location.range.end),
                        "lineText": ref_info.line_text,
                        "isWriteAccess": ref_info.is_write_access,
                        "isDefinition": ref_info.is_definition,
                    })
                })
                .collect();
            Some(serde_json::json!({
                "refs": refs,
                "symbolName": symbol_name,
            }))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({"refs": [], "symbolName": ""}))),
        )
    }

    pub(crate) fn handle_document_highlights(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let provider = DocumentHighlightProvider::new(&arena, &binder, &line_map, &source_text);
            let highlights = provider.get_document_highlights(root, position)?;

            // Group highlights by file (tsserver groups by file, each with highlightSpans)
            let highlight_spans: Vec<serde_json::Value> = highlights
                .iter()
                .map(|hl| {
                    let kind_str = match hl.kind {
                        Some(tsz::lsp::highlighting::DocumentHighlightKind::Read) => "reference",
                        Some(tsz::lsp::highlighting::DocumentHighlightKind::Write) => {
                            "writtenReference"
                        }
                        Some(tsz::lsp::highlighting::DocumentHighlightKind::Text) | None => "none",
                    };
                    serde_json::json!({
                        "start": Self::lsp_to_tsserver_position(hl.range.start),
                        "end": Self::lsp_to_tsserver_position(hl.range.end),
                        "kind": kind_str,
                    })
                })
                .collect();
            // All highlights are in the same file for now
            Some(serde_json::json!([{
                "file": file,
                "highlightSpans": highlight_spans,
            }]))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_rename(
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
                RenameProvider::new(&arena, &binder, &line_map, file.clone(), &source_text);

            // Use the rich prepare_rename_info to get display name, kind, etc.
            let info = provider.prepare_rename_info(root, position);
            if !info.can_rename {
                return Some(serde_json::json!({
                    "info": {
                        "canRename": false,
                        "localizedErrorMessage": info.localized_error_message.unwrap_or_else(|| "You cannot rename this element.".to_string())
                    },
                    "locs": []
                }));
            }

            // Compute trigger span length from the range
            let start_offset = line_map
                .position_to_offset(info.trigger_span.start, &source_text)
                .unwrap_or(0) as usize;
            let end_offset = line_map
                .position_to_offset(info.trigger_span.end, &source_text)
                .unwrap_or(0) as usize;
            let trigger_length = end_offset.saturating_sub(start_offset);

            // Get rename locations from references with symbol info
            let find_refs =
                FindReferences::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let (symbol_id, ref_infos) = find_refs
                .find_references_with_symbol(root, position)
                .unwrap_or((SymbolId::NONE, Vec::new()));

            // Get definition info for context spans
            let def_provider =
                GoToDefinition::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let def_infos = if symbol_id.is_some() {
                def_provider.definition_infos_from_symbol(symbol_id)
            } else {
                None
            };

            let file_locs: Vec<serde_json::Value> = ref_infos
                .iter()
                .map(|ref_info| {
                    let mut loc = serde_json::json!({
                        "start": Self::lsp_to_tsserver_position(ref_info.location.range.start),
                        "end": Self::lsp_to_tsserver_position(ref_info.location.range.end),
                    });
                    // Add contextSpan for definition locations
                    if ref_info.is_definition
                        && let Some(ref defs) = def_infos
                    {
                        for def in defs {
                            if def.location.range == ref_info.location.range
                                && let Some(ref ctx) = def.context_span
                            {
                                loc["contextStart"] = Self::lsp_to_tsserver_position(ctx.start);
                                loc["contextEnd"] = Self::lsp_to_tsserver_position(ctx.end);
                                break;
                            }
                        }
                    }
                    loc
                })
                .collect();
            Some(serde_json::json!({
                "info": {
                    "canRename": true,
                    "displayName": info.display_name,
                    "fullDisplayName": info.full_display_name,
                    "kind": info.kind,
                    "kindModifiers": info.kind_modifiers,
                    "triggerSpan": {
                        "start": Self::lsp_to_tsserver_position(info.trigger_span.start),
                        "length": trigger_length
                    }
                },
                "locs": [{
                    "file": file,
                    "locs": file_locs,
                }]
            }))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({
                "info": {"canRename": false, "localizedErrorMessage": "Not yet implemented"},
                "locs": []
            }))),
        )
    }

    pub(crate) fn handle_references_full(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);

            // Get references with the resolved symbol
            let ref_provider =
                FindReferences::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let (symbol_id, ref_infos) =
                ref_provider.find_references_with_symbol(root, position)?;

            // Get definition metadata using GoToDefinition helpers
            let def_provider =
                GoToDefinition::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let def_infos = def_provider.definition_infos_from_symbol(symbol_id);

            // Get symbol info for display
            let symbol = binder.symbols.get(symbol_id)?;
            let kind_str = def_provider.symbol_flags_to_kind_string(symbol.flags);
            let symbol_name = symbol.escaped_name.clone();

            // Use HoverProvider to get the display string with type info
            let interner = TypeInterner::new();
            let hover_provider = HoverProvider::new(
                &arena,
                &binder,
                &line_map,
                &interner,
                &source_text,
                file.clone(),
            );
            let mut type_cache = None;
            let hover_info = hover_provider.get_hover(root, position, &mut type_cache);
            let display_string = hover_info
                .as_ref()
                .map(|h| h.display_string.clone())
                .unwrap_or_default();

            // Build definition object using first definition info
            let definition = if let Some(ref defs) = def_infos {
                if let Some(first_def) = defs.first() {
                    let def_start =
                        line_map.position_to_offset(first_def.location.range.start, &source_text);
                    let def_end =
                        line_map.position_to_offset(first_def.location.range.end, &source_text);

                    // Use display_string from HoverProvider if available for proper type info
                    let name = if !display_string.is_empty() {
                        display_string.clone()
                    } else {
                        format!("{} {}", first_def.kind, first_def.name)
                    };
                    let display_parts = if !display_string.is_empty() {
                        Self::parse_display_string_to_parts(
                            &display_string,
                            &first_def.kind,
                            &first_def.name,
                        )
                    } else {
                        Self::build_simple_display_parts(&first_def.kind, &first_def.name)
                    };

                    let mut def_json = serde_json::json!({
                        "containerKind": "",
                        "containerName": "",
                        "kind": first_def.kind,
                        "name": name,
                        "displayParts": display_parts,
                        "fileName": file,
                        "textSpan": {
                            "start": def_start.unwrap_or(0),
                            "length": def_end.unwrap_or(0).saturating_sub(def_start.unwrap_or(0)),
                        },
                    });
                    if let Some(ref ctx) = first_def.context_span {
                        let ctx_start = line_map.position_to_offset(ctx.start, &source_text);
                        let ctx_end = line_map.position_to_offset(ctx.end, &source_text);
                        let ctx_start_off = ctx_start.unwrap_or(0);
                        let ctx_end_off = ctx_end.unwrap_or(0);
                        let def_start_off = def_start.unwrap_or(0);
                        let def_end_off = def_end.unwrap_or(0);
                        // Skip contextSpan when it matches textSpan (e.g., catch clause vars)
                        if ctx_start_off != def_start_off || ctx_end_off != def_end_off {
                            def_json["contextSpan"] = serde_json::json!({
                                "start": ctx_start_off,
                                "length": ctx_end_off.saturating_sub(ctx_start_off),
                            });
                        }
                    }
                    if first_def.kind == "alias"
                        && let Some((ctx_start_off, ctx_end_off)) =
                            Self::import_statement_context_span(
                                &source_text,
                                def_start.unwrap_or(0),
                            )
                    {
                        def_json["contextSpan"] = serde_json::json!({
                            "start": ctx_start_off,
                            "length": ctx_end_off.saturating_sub(ctx_start_off),
                        });
                    }
                    def_json
                } else {
                    Self::build_fallback_definition(&file, &kind_str, &symbol_name)
                }
            } else {
                Self::build_fallback_definition(&file, &kind_str, &symbol_name)
            };

            // Build references array with byte-offset textSpans
            // Compute cursor offset for isDefinition check - TypeScript only sets
            // isDefinition=true when the cursor is ON the definition reference
            let cursor_offset = line_map
                .position_to_offset(position, &source_text)
                .unwrap_or(0);

            let references: Vec<serde_json::Value> = ref_infos
                .iter()
                .map(|ref_info| {
                    let start =
                        line_map.position_to_offset(ref_info.location.range.start, &source_text);
                    let end =
                        line_map.position_to_offset(ref_info.location.range.end, &source_text);
                    let start_off = start.unwrap_or(0);
                    let end_off = end.unwrap_or(0);

                    // isDefinition is only true when: (1) the reference IS a definition,
                    // AND (2) the cursor is at that reference's position
                    let is_definition = ref_info.is_definition
                        && cursor_offset >= start_off
                        && cursor_offset < end_off;

                    let mut ref_json = serde_json::json!({
                        "fileName": ref_info.location.file_path,
                        "textSpan": {
                            "start": start_off,
                            "length": end_off.saturating_sub(start_off),
                        },
                        "isWriteAccess": ref_info.is_write_access,
                        "isDefinition": is_definition,
                    });

                    // Add contextSpan for definition references
                    // Skip when contextSpan matches textSpan (e.g., catch clause variables)
                    if ref_info.is_definition
                        && let Some(ref defs) = def_infos
                    {
                        for def in defs {
                            if def.location.range == ref_info.location.range
                                && let Some(ref ctx) = def.context_span
                            {
                                let ctx_start =
                                    line_map.position_to_offset(ctx.start, &source_text);
                                let ctx_end = line_map.position_to_offset(ctx.end, &source_text);
                                let ctx_start_off = ctx_start.unwrap_or(0);
                                let ctx_end_off = ctx_end.unwrap_or(0);
                                // Only add contextSpan if it differs from the textSpan
                                if ctx_start_off != start_off || ctx_end_off != end_off {
                                    ref_json["contextSpan"] = serde_json::json!({
                                        "start": ctx_start_off,
                                        "length": ctx_end_off.saturating_sub(ctx_start_off),
                                    });
                                }
                                break;
                            }
                        }
                        if symbol.flags & tsz::binder::symbol_flags::ALIAS != 0
                            && let Some((ctx_start_off, ctx_end_off)) =
                                Self::import_statement_context_span(&source_text, start_off)
                        {
                            ref_json["contextSpan"] = serde_json::json!({
                                "start": ctx_start_off,
                                "length": ctx_end_off.saturating_sub(ctx_start_off),
                            });
                        }
                    }
                    ref_json
                })
                .collect();

            // Return as ReferencedSymbol array (single entry for single-file)
            Some(serde_json::json!([{
                "definition": definition,
                "references": references,
            }]))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn build_fallback_definition(
        file: &str,
        kind: &str,
        name: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "containerKind": "",
            "containerName": "",
            "kind": kind,
            "name": format!("{} {}", kind, name),
            "displayParts": Self::build_simple_display_parts(kind, name),
            "fileName": file,
            "textSpan": { "start": 0, "length": 0 },
        })
    }

    pub(crate) fn build_simple_display_parts(kind: &str, name: &str) -> Vec<serde_json::Value> {
        let mut parts = vec![];
        if !kind.is_empty() {
            parts.push(serde_json::json!({ "text": kind, "kind": "keyword" }));
            parts.push(serde_json::json!({ "text": " ", "kind": "space" }));
        }
        let name_kind = Self::symbol_kind_to_display_part_kind(kind);
        parts.push(serde_json::json!({ "text": name, "kind": name_kind }));
        parts
    }

    pub(crate) fn symbol_kind_to_display_part_kind(kind: &str) -> &'static str {
        match kind {
            "class" => "className",
            "function" => "functionName",
            "interface" => "interfaceName",
            "enum" => "enumName",
            "enum member" => "enumMemberName",
            "module" | "namespace" => "moduleName",
            "type" => "aliasName",
            "method" => "methodName",
            "property" => "propertyName",
            _ => "localName",
        }
    }

    fn import_statement_context_span(source_text: &str, anchor_offset: u32) -> Option<(u32, u32)> {
        if source_text.is_empty() {
            return None;
        }
        let idx = (anchor_offset as usize).min(source_text.len().saturating_sub(1));
        let line_start = source_text[..idx].rfind('\n').map_or(0, |i| i + 1);
        let line_end = source_text[idx..]
            .find('\n')
            .map_or(source_text.len(), |i| idx + i);
        let line_text = source_text[line_start..line_end].trim_start();
        if !line_text.starts_with("import ") {
            return None;
        }
        Some((line_start as u32, line_end as u32))
    }

    /// Parse a display string (e.g. "const x: number") into structured displayParts.
    /// This handles common patterns from the `HoverProvider`.
    pub(crate) fn parse_display_string_to_parts(
        display_string: &str,
        kind: &str,
        name: &str,
    ) -> Vec<serde_json::Value> {
        let name_kind = Self::symbol_kind_to_display_part_kind(kind);

        // Handle prefixed forms like "(local var) x: type" or "(parameter) x: type"
        let s = display_string;

        // Special-case alias module displays:
        // "(alias) module \"jquery\"\nimport x"
        if let Some(rest) = s.strip_prefix("(alias) module ") {
            let mut parts = vec![
                serde_json::json!({ "text": "(", "kind": "punctuation" }),
                serde_json::json!({ "text": "alias", "kind": "text" }),
                serde_json::json!({ "text": ")", "kind": "punctuation" }),
                serde_json::json!({ "text": " ", "kind": "space" }),
                serde_json::json!({ "text": "module", "kind": "keyword" }),
                serde_json::json!({ "text": " ", "kind": "space" }),
            ];

            if let Some(after_quote) = rest.strip_prefix('"')
                && let Some(end_quote_idx) = after_quote.find('"')
            {
                let quoted = &after_quote[..end_quote_idx];
                parts.push(
                    serde_json::json!({ "text": format!("\"{quoted}\""), "kind": "stringLiteral" }),
                );
                let after_module = &after_quote[end_quote_idx + 1..];
                if let Some(import_rest) = after_module.strip_prefix("\nimport ") {
                    parts.push(serde_json::json!({ "text": "\n", "kind": "lineBreak" }));
                    parts.push(serde_json::json!({ "text": "import", "kind": "keyword" }));
                    parts.push(serde_json::json!({ "text": " ", "kind": "space" }));
                    if let Some(eq_idx) = import_rest.find(" = ") {
                        let alias_name = import_rest[..eq_idx].trim();
                        parts.push(serde_json::json!({ "text": alias_name, "kind": "aliasName" }));
                        parts.push(serde_json::json!({ "text": import_rest[eq_idx..].to_string(), "kind": "text" }));
                    } else {
                        parts.push(
                            serde_json::json!({ "text": import_rest.trim(), "kind": "aliasName" }),
                        );
                    }
                    return parts;
                }
                return parts;
            }
        }

        // Check for parenthesized prefix like "(local var)" or "(parameter)"
        if let Some(rest) = s.strip_prefix('(')
            && let Some(paren_end) = rest.find(')')
        {
            let prefix = &rest[..paren_end];
            let after_paren = rest[paren_end + 1..].trim_start();

            let mut parts = vec![];
            parts.push(serde_json::json!({ "text": "(", "kind": "punctuation" }));

            // Split prefix words
            let prefix_words: Vec<&str> = prefix.split_whitespace().collect();
            for (i, word) in prefix_words.iter().enumerate() {
                if i > 0 {
                    parts.push(serde_json::json!({ "text": " ", "kind": "space" }));
                }
                parts.push(serde_json::json!({ "text": *word, "kind": "keyword" }));
            }
            parts.push(serde_json::json!({ "text": ")", "kind": "punctuation" }));
            parts.push(serde_json::json!({ "text": " ", "kind": "space" }));

            // Parse the rest: "name: type" or "name(sig): type"
            Self::parse_name_and_type(after_paren, name_kind, &mut parts);
            return parts;
        }

        // Handle "keyword name: type" or "keyword name" patterns
        let keywords = [
            "const",
            "let",
            "var",
            "function",
            "class",
            "interface",
            "enum",
            "type",
            "namespace",
        ];
        for kw in &keywords {
            if let Some(rest) = s.strip_prefix(kw)
                && rest.starts_with(' ')
            {
                let mut parts = vec![];
                parts.push(serde_json::json!({ "text": *kw, "kind": "keyword" }));
                parts.push(serde_json::json!({ "text": " ", "kind": "space" }));
                let rest = rest.trim_start();
                Self::parse_name_and_type(rest, name_kind, &mut parts);
                return parts;
            }
        }

        // Fallback: just use the display_string as-is
        Self::build_simple_display_parts(kind, name)
    }

    /// Parse "name: type" or "name(params): type" or just "name" from a string.
    pub(crate) fn parse_name_and_type(
        s: &str,
        name_kind: &str,
        parts: &mut Vec<serde_json::Value>,
    ) {
        // Find where the name ends - it could be followed by ':', '(', '<', '=', or end of string
        let name_end = s.find([':', '(', '<', '=']).unwrap_or(s.len());
        let name_part = s[..name_end].trim_end();

        if !name_part.is_empty() {
            // Check if name contains '.' for qualified names like "Foo.bar"
            if let Some(dot_pos) = name_part.rfind('.') {
                let container = &name_part[..dot_pos];
                let member = &name_part[dot_pos + 1..];
                parts.push(serde_json::json!({ "text": container, "kind": "className" }));
                parts.push(serde_json::json!({ "text": ".", "kind": "punctuation" }));
                parts.push(serde_json::json!({ "text": member, "kind": name_kind }));
            } else {
                parts.push(serde_json::json!({ "text": name_part, "kind": name_kind }));
            }
        }

        let remaining = &s[name_end..];
        if remaining.is_empty() {
            return;
        }

        // Handle signature parts like "(params): type" or "= type" or ": type"
        if remaining.starts_with('(') {
            // Function signature - add everything as-is for now with punctuation
            Self::parse_signature(remaining, parts);
        } else if let Some(rest) = remaining.strip_prefix(": ") {
            parts.push(serde_json::json!({ "text": ":", "kind": "punctuation" }));
            parts.push(serde_json::json!({ "text": " ", "kind": "space" }));
            Self::parse_type_string(rest, parts);
        } else if let Some(rest) = remaining.strip_prefix(":") {
            parts.push(serde_json::json!({ "text": ":", "kind": "punctuation" }));
            parts.push(serde_json::json!({ "text": " ", "kind": "space" }));
            Self::parse_type_string(rest.trim_start(), parts);
        } else if let Some(rest) = remaining.strip_prefix(" = ") {
            parts.push(serde_json::json!({ "text": " ", "kind": "space" }));
            parts.push(serde_json::json!({ "text": "=", "kind": "operator" }));
            parts.push(serde_json::json!({ "text": " ", "kind": "space" }));
            Self::parse_type_string(rest, parts);
        }
    }

    /// Parse a type string into display parts.
    pub(crate) fn parse_type_string(type_str: &str, parts: &mut Vec<serde_json::Value>) {
        let type_str = type_str.trim();
        if type_str.is_empty() {
            return;
        }

        // Check for TypeScript keyword types
        let keyword_types = [
            "any",
            "boolean",
            "bigint",
            "never",
            "null",
            "number",
            "object",
            "string",
            "symbol",
            "undefined",
            "unknown",
            "void",
            "true",
            "false",
        ];
        if keyword_types.contains(&type_str) {
            parts.push(serde_json::json!({ "text": type_str, "kind": "keyword" }));
            return;
        }

        // Check for numeric literal
        if type_str.parse::<f64>().is_ok() {
            parts.push(serde_json::json!({ "text": type_str, "kind": "stringLiteral" }));
            return;
        }

        // Check for string literal (starts and ends with quotes)
        if (type_str.starts_with('"') && type_str.ends_with('"'))
            || (type_str.starts_with('\'') && type_str.ends_with('\''))
        {
            parts.push(serde_json::json!({ "text": type_str, "kind": "stringLiteral" }));
            return;
        }

        // Default: treat as text (could be a complex type, interface name, etc.)
        parts.push(serde_json::json!({ "text": type_str, "kind": "text" }));
    }

    /// Parse a function signature like "(x: number): string" into parts.
    pub(crate) fn parse_signature(sig: &str, parts: &mut Vec<serde_json::Value>) {
        // For now, add the whole signature as text parts
        // This handles the common case of function signatures
        parts.push(serde_json::json!({ "text": sig, "kind": "text" }));
    }

    pub(crate) fn handle_navtree(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let (arena, _binder, root, source_text) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&source_text);
            let provider = DocumentSymbolProvider::new(&arena, &line_map, &source_text);
            let symbols = provider.get_document_symbols(root);

            fn symbol_to_navtree(
                sym: &tsz::lsp::symbols::document_symbols::DocumentSymbol,
            ) -> serde_json::Value {
                let kind = match sym.kind {
                    tsz::lsp::symbols::document_symbols::SymbolKind::File
                    | tsz::lsp::symbols::document_symbols::SymbolKind::Module
                    | tsz::lsp::symbols::document_symbols::SymbolKind::Namespace => "module",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Class => "class",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Method => "method",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Property
                    | tsz::lsp::symbols::document_symbols::SymbolKind::Field => "property",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Constructor => "constructor",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Enum => "enum",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Interface => "interface",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Function => "function",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Variable => "var",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Constant => "const",
                    tsz::lsp::symbols::document_symbols::SymbolKind::EnumMember => "enum member",
                    tsz::lsp::symbols::document_symbols::SymbolKind::TypeParameter => {
                        "type parameter"
                    }
                    tsz::lsp::symbols::document_symbols::SymbolKind::Struct => "type",
                    _ => "unknown",
                };
                let children: Vec<serde_json::Value> =
                    sym.children.iter().map(symbol_to_navtree).collect();
                serde_json::json!({
                    "text": sym.name,
                    "kind": kind,
                    "childItems": children,
                    "spans": [{
                        "start": {
                            "line": sym.range.start.line + 1,
                            "offset": sym.range.start.character + 1,
                        },
                        "end": {
                            "line": sym.range.end.line + 1,
                            "offset": sym.range.end.character + 1,
                        },
                    }],
                })
            }

            let child_items: Vec<serde_json::Value> =
                symbols.iter().map(symbol_to_navtree).collect();

            // Compute the end span based on source text length
            let total_lines = source_text.lines().count();
            let last_line_len = source_text.lines().last().map_or(0, str::len);
            Some(serde_json::json!({
                "text": "<global>",
                "kind": "script",
                "childItems": child_items,
                "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": total_lines, "offset": last_line_len + 1}}],
            }))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({
                "text": "<global>",
                "kind": "script",
                "childItems": [],
                "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": 1, "offset": 1}}],
            }))),
        )
    }

    pub(crate) fn handle_navbar(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let (arena, _binder, root, source_text) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&source_text);
            let provider = DocumentSymbolProvider::new(&arena, &line_map, &source_text);
            let symbols = provider.get_document_symbols(root);

            fn symbol_to_navbar_item(
                sym: &tsz::lsp::symbols::document_symbols::DocumentSymbol,
                indent: usize,
                items: &mut Vec<serde_json::Value>,
            ) {
                let kind = match sym.kind {
                    tsz::lsp::symbols::document_symbols::SymbolKind::File
                    | tsz::lsp::symbols::document_symbols::SymbolKind::Module
                    | tsz::lsp::symbols::document_symbols::SymbolKind::Namespace => "module",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Class => "class",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Method => "method",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Property
                    | tsz::lsp::symbols::document_symbols::SymbolKind::Field => "property",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Constructor => "constructor",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Enum => "enum",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Interface => "interface",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Function => "function",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Variable => "var",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Constant => "const",
                    tsz::lsp::symbols::document_symbols::SymbolKind::EnumMember => "enum member",
                    tsz::lsp::symbols::document_symbols::SymbolKind::TypeParameter => {
                        "type parameter"
                    }
                    tsz::lsp::symbols::document_symbols::SymbolKind::Struct => "type",
                    _ => "unknown",
                };
                let child_items: Vec<serde_json::Value> = sym
                    .children
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "text": c.name,
                            "kind": match c.kind {
                                tsz::lsp::symbols::document_symbols::SymbolKind::Function => "function",
                                tsz::lsp::symbols::document_symbols::SymbolKind::Class => "class",
                                tsz::lsp::symbols::document_symbols::SymbolKind::Method => "method",
                                tsz::lsp::symbols::document_symbols::SymbolKind::Property => "property",
                                tsz::lsp::symbols::document_symbols::SymbolKind::Variable => "var",
                                tsz::lsp::symbols::document_symbols::SymbolKind::Constant => "const",
                                tsz::lsp::symbols::document_symbols::SymbolKind::Enum => "enum",
                                tsz::lsp::symbols::document_symbols::SymbolKind::Interface => "interface",
                                tsz::lsp::symbols::document_symbols::SymbolKind::EnumMember => "enum member",
                                tsz::lsp::symbols::document_symbols::SymbolKind::Struct => "type",
                                _ => "unknown",
                            },
                        })
                    })
                    .collect();
                items.push(serde_json::json!({
                    "text": sym.name,
                    "kind": kind,
                    "childItems": child_items,
                    "indent": indent,
                    "spans": [{
                        "start": {
                            "line": sym.range.start.line + 1,
                            "offset": sym.range.start.character + 1,
                        },
                        "end": {
                            "line": sym.range.end.line + 1,
                            "offset": sym.range.end.character + 1,
                        },
                    }],
                }));
                for child in &sym.children {
                    symbol_to_navbar_item(child, indent + 1, items);
                }
            }

            let mut items = Vec::new();
            // Root item
            let total_lines = source_text.lines().count();
            let last_line_len = source_text.lines().last().map_or(0, str::len);
            let child_items: Vec<serde_json::Value> = symbols
                .iter()
                .map(|sym| {
                    serde_json::json!({
                        "text": sym.name,
                        "kind": match sym.kind {
                            tsz::lsp::symbols::document_symbols::SymbolKind::Function => "function",
                            tsz::lsp::symbols::document_symbols::SymbolKind::Class => "class",
                            tsz::lsp::symbols::document_symbols::SymbolKind::Method => "method",
                            tsz::lsp::symbols::document_symbols::SymbolKind::Property => "property",
                            tsz::lsp::symbols::document_symbols::SymbolKind::Variable => "var",
                            tsz::lsp::symbols::document_symbols::SymbolKind::Constant => "const",
                            tsz::lsp::symbols::document_symbols::SymbolKind::Enum => "enum",
                            tsz::lsp::symbols::document_symbols::SymbolKind::Interface => "interface",
                            tsz::lsp::symbols::document_symbols::SymbolKind::EnumMember => "enum member",
                            _ => "unknown",
                        },
                    })
                })
                .collect();
            items.push(serde_json::json!({
                "text": "<global>",
                "kind": "script",
                "childItems": child_items,
                "indent": 0,
                "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": total_lines, "offset": last_line_len + 1}}],
            }));
            // Flatten children
            for sym in &symbols {
                symbol_to_navbar_item(sym, 1, &mut items);
            }
            Some(serde_json::json!(items))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!([{
                "text": "<global>",
                "kind": "script",
                "childItems": [],
                "indent": 0,
                "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": 1, "offset": 1}}],
            }]))),
        )
    }

    pub(crate) fn handle_navto(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let search_value = request
                .arguments
                .get("searchValue")
                .and_then(|v| v.as_str())?;
            if search_value.is_empty() {
                return Some(serde_json::json!([]));
            }
            let search_lower = search_value.to_lowercase();
            let mut nav_items: Vec<serde_json::Value> = Vec::new();
            let file_paths: Vec<String> = self.open_files.keys().cloned().collect();
            for file_path in &file_paths {
                if let Some((arena, _binder, root, source_text)) =
                    self.parse_and_bind_file(file_path)
                {
                    let line_map = LineMap::build(&source_text);
                    let provider = DocumentSymbolProvider::new(&arena, &line_map, &source_text);
                    let symbols = provider.get_document_symbols(root);
                    Self::collect_navto_items(
                        &symbols,
                        search_value,
                        &search_lower,
                        file_path,
                        &mut nav_items,
                    );
                }
            }
            Some(serde_json::json!(nav_items))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn collect_navto_items(
        symbols: &[tsz::lsp::symbols::document_symbols::DocumentSymbol],
        search_value: &str,
        search_lower: &str,
        file_path: &str,
        result: &mut Vec<serde_json::Value>,
    ) {
        for sym in symbols {
            let name_lower = sym.name.to_lowercase();
            if name_lower.contains(search_lower) {
                let is_case_sensitive = sym.name.contains(search_value);
                let kind = match sym.kind {
                    tsz::lsp::symbols::document_symbols::SymbolKind::Module => "module",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Class => "class",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Method => "method",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Property
                    | tsz::lsp::symbols::document_symbols::SymbolKind::Field => "property",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Constructor => "constructor",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Enum => "enum",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Interface => "interface",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Function => "function",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Variable => "var",
                    tsz::lsp::symbols::document_symbols::SymbolKind::Constant => "const",
                    tsz::lsp::symbols::document_symbols::SymbolKind::EnumMember => "enum member",
                    tsz::lsp::symbols::document_symbols::SymbolKind::TypeParameter => {
                        "type parameter"
                    }
                    _ => "unknown",
                };
                let match_kind = if name_lower == *search_lower {
                    "exact"
                } else if name_lower.starts_with(search_lower) {
                    "prefix"
                } else {
                    "substring"
                };
                result.push(serde_json::json!({
                    "name": sym.name,
                    "kind": kind,
                    "kindModifiers": "",
                    "matchKind": match_kind,
                    "isCaseSensitive": is_case_sensitive,
                    "file": file_path,
                    "start": {
                        "line": sym.range.start.line + 1,
                        "offset": sym.range.start.character + 1,
                    },
                    "end": {
                        "line": sym.range.end.line + 1,
                        "offset": sym.range.end.character + 1,
                    },
                }));
            }
            Self::collect_navto_items(&sym.children, search_value, search_lower, file_path, result);
        }
    }

    pub(crate) fn handle_implementation(
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
                GoToImplementationProvider::new(&arena, &binder, &line_map, file, &source_text);
            let locations = provider.get_implementations(root, position)?;
            let body: Vec<serde_json::Value> = locations
                .iter()
                .map(|loc| {
                    serde_json::json!({
                        "file": loc.file_path,
                        "start": Self::lsp_to_tsserver_position(loc.range.start),
                        "end": Self::lsp_to_tsserver_position(loc.range.end),
                    })
                })
                .collect();
            Some(serde_json::json!(body))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_file_references(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(
            seq,
            request,
            Some(serde_json::json!({"refs": [], "symbolName": ""})),
        )
    }
}
