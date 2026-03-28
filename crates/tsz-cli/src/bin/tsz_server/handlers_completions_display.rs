//! Display parts rendering and signature help for completion entries.
//!
//! Extracted from `handlers_completions.rs` to keep individual files under 2000 LOC.
//! Contains:
//! - `build_completion_display_parts` — rich displayParts builder for completion entries
//! - `handle_signature_help` — signatureHelp protocol handler
//! - `tokenize_*` / `type_display_kind` — structured display-parts tokenizers

use super::{Server, TsServerRequest, TsServerResponse};
use tsz::lsp::position::LineMap;
use tsz::lsp::signature_help::SignatureHelpProvider;
use tsz::parser::node::NodeAccess;
use tsz_solver::TypeInterner;

impl Server {
    /// Build rich displayParts for a completion entry, matching TypeScript's format.
    /// Generates structured parts like: class `ClassName`, var name: Type, function name(...), etc.
    pub(crate) fn build_completion_display_parts(
        item: Option<&tsz::lsp::completions::CompletionItem>,
        name: &str,
        member_parent: Option<&str>,
        arena: &tsz::parser::node::NodeArena,
        binder: &tsz::binder::BinderState,
        source_text: &str,
    ) -> serde_json::Value {
        use tsz::lsp::completions::CompletionItemKind;

        let Some(item) = item else {
            return serde_json::json!([{"text": name, "kind": "text"}]);
        };

        let mut parts: Vec<serde_json::Value> = Vec::new();

        match item.kind {
            CompletionItemKind::Class => {
                parts.push(serde_json::json!({"text": "class", "kind": "keyword"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                parts.push(serde_json::json!({"text": name, "kind": "className"}));
                if Self::is_merged_namespace_symbol(name, binder) {
                    parts.push(serde_json::json!({"text": "\n", "kind": "lineBreak"}));
                    parts.push(serde_json::json!({"text": "namespace", "kind": "keyword"}));
                    parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                    parts.push(serde_json::json!({"text": name, "kind": "moduleName"}));
                }
            }
            CompletionItemKind::Interface => {
                parts.push(serde_json::json!({"text": "interface", "kind": "keyword"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                parts.push(serde_json::json!({"text": name, "kind": "interfaceName"}));
            }
            CompletionItemKind::Enum => {
                parts.push(serde_json::json!({"text": "enum", "kind": "keyword"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                parts.push(serde_json::json!({"text": name, "kind": "enumName"}));
            }
            CompletionItemKind::Module => {
                parts.push(serde_json::json!({"text": "namespace", "kind": "keyword"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                parts.push(serde_json::json!({"text": name, "kind": "moduleName"}));
            }
            CompletionItemKind::TypeAlias => {
                parts.push(serde_json::json!({"text": "type", "kind": "keyword"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                parts.push(serde_json::json!({"text": name, "kind": "aliasName"}));
            }
            CompletionItemKind::TypeParameter => {
                parts.push(serde_json::json!({"text": "(", "kind": "punctuation"}));
                parts.push(serde_json::json!({"text": "type parameter", "kind": "text"}));
                parts.push(serde_json::json!({"text": ")", "kind": "punctuation"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                parts.push(serde_json::json!({"text": name, "kind": "typeParameterName"}));
                if let Some(suffix) =
                    Self::type_parameter_context_suffix(name, binder, arena, source_text)
                {
                    parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                    parts.push(serde_json::json!({"text": "in", "kind": "keyword"}));
                    parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                    parts.push(serde_json::json!({"text": suffix, "kind": "text"}));
                }
            }
            CompletionItemKind::Function => {
                parts.push(serde_json::json!({"text": "function", "kind": "keyword"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                parts.push(serde_json::json!({"text": name, "kind": "functionName"}));
                Self::append_function_signature_from_source(
                    &mut parts,
                    name,
                    binder,
                    arena,
                    source_text,
                );
                if Self::is_merged_namespace_symbol(name, binder) {
                    parts.push(serde_json::json!({"text": "\n", "kind": "lineBreak"}));
                    parts.push(serde_json::json!({"text": "namespace", "kind": "keyword"}));
                    parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                    parts.push(serde_json::json!({"text": name, "kind": "moduleName"}));
                }
            }
            CompletionItemKind::Method => {
                parts.push(serde_json::json!({"text": "(", "kind": "punctuation"}));
                parts.push(serde_json::json!({"text": "method", "kind": "text"}));
                parts.push(serde_json::json!({"text": ")", "kind": "punctuation"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                let qualified_name = member_parent
                    .map(|parent| format!("{parent}.{name}"))
                    .unwrap_or_else(|| name.to_string());
                parts.push(serde_json::json!({"text": qualified_name, "kind": "methodName"}));
                if let Some(sig) = item
                    .detail
                    .as_deref()
                    .and_then(Self::method_signature_from_detail)
                {
                    parts.push(serde_json::json!({"text": sig, "kind": "text"}));
                }
            }
            CompletionItemKind::Property => {
                parts.push(serde_json::json!({"text": "(", "kind": "punctuation"}));
                parts.push(serde_json::json!({"text": "property", "kind": "text"}));
                parts.push(serde_json::json!({"text": ")", "kind": "punctuation"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                let is_identifier = {
                    let mut chars = name.chars();
                    match chars.next() {
                        Some(first) => {
                            (first.is_ascii_alphabetic() || first == '_' || first == '$')
                                && chars
                                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
                        }
                        None => false,
                    }
                };
                let display_name = if is_identifier {
                    name.to_string()
                } else {
                    format!("\"{name}\"")
                };
                let qualified_name = member_parent
                    .map(|parent| format!("{parent}.{display_name}"))
                    .unwrap_or(display_name);
                parts.push(serde_json::json!({"text": qualified_name, "kind": "propertyName"}));
                let has_annotation = Self::append_type_annotation_from_source(
                    &mut parts,
                    name,
                    binder,
                    arena,
                    source_text,
                );
                if !has_annotation
                    && let Some(detail) = item.detail.as_deref()
                    && !detail.is_empty()
                {
                    parts.push(serde_json::json!({"text": ":", "kind": "punctuation"}));
                    parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                    parts.push(serde_json::json!({"text": detail, "kind": "keyword"}));
                }
            }
            CompletionItemKind::Alias => {
                // For import aliases, show as "import <name>"
                parts.push(serde_json::json!({"text": "(", "kind": "punctuation"}));
                parts.push(serde_json::json!({"text": "alias", "kind": "text"}));
                parts.push(serde_json::json!({"text": ")", "kind": "punctuation"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                parts.push(serde_json::json!({"text": name, "kind": "aliasName"}));
            }
            CompletionItemKind::Variable
            | CompletionItemKind::Const
            | CompletionItemKind::Let
            | CompletionItemKind::Parameter => {
                if item.kind == CompletionItemKind::Parameter {
                    parts.push(serde_json::json!({"text": "(", "kind": "punctuation"}));
                    parts.push(serde_json::json!({"text": "parameter", "kind": "text"}));
                    parts.push(serde_json::json!({"text": ")", "kind": "punctuation"}));
                    parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                    parts.push(serde_json::json!({"text": name, "kind": "parameterName"}));
                } else {
                    let keyword = match item.kind {
                        CompletionItemKind::Const => "const",
                        CompletionItemKind::Let => "let",
                        _ => Self::get_var_keyword_from_source(name, binder, arena, source_text)
                            .unwrap_or({
                                if let Some(ref detail) = item.detail {
                                    match detail.as_str() {
                                        "var" => "var",
                                        _ => "let",
                                    }
                                } else {
                                    "var"
                                }
                            }),
                    };
                    parts.push(serde_json::json!({"text": keyword, "kind": "keyword"}));
                    parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                    parts.push(serde_json::json!({"text": name, "kind": "localName"}));
                }
                let has_annotation = Self::append_type_annotation_from_source(
                    &mut parts,
                    name,
                    binder,
                    arena,
                    source_text,
                );
                if !has_annotation
                    && item.kind == CompletionItemKind::Parameter
                    && let Some(detail) = item.detail.as_deref()
                    && !detail.is_empty()
                {
                    parts.push(serde_json::json!({"text": ":", "kind": "punctuation"}));
                    parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                    parts.push(serde_json::json!({"text": detail, "kind": "keyword"}));
                } else if !has_annotation
                    && item.kind != CompletionItemKind::Parameter
                    && let Some(detail) = item.detail.as_deref()
                    && !detail.is_empty()
                    && detail != "var"
                    && detail != "let"
                    && detail != "const"
                {
                    parts.push(serde_json::json!({"text": ":", "kind": "punctuation"}));
                    parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                    parts.push(serde_json::json!({
                        "text": detail,
                        "kind": Self::type_display_kind(detail)
                    }));
                }
            }
            CompletionItemKind::Keyword => {
                parts.push(serde_json::json!({"text": name, "kind": "keyword"}));
            }
            CompletionItemKind::Constructor => {
                parts.push(serde_json::json!({"text": "constructor", "kind": "keyword"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                parts.push(serde_json::json!({"text": name, "kind": "className"}));
            }
        }

        serde_json::json!(parts)
    }

    fn node_text_slice<'a>(
        node: tsz::parser::NodeIndex,
        arena: &tsz::parser::node::NodeArena,
        source_text: &'a str,
    ) -> Option<&'a str> {
        let n = arena.get(node)?;
        let start = n.pos as usize;
        let end = n.end.min(source_text.len() as u32) as usize;
        (start < end).then(|| source_text[start..end].trim())
    }

    fn type_parameter_context_suffix(
        name: &str,
        binder: &tsz::binder::BinderState,
        arena: &tsz::parser::node::NodeArena,
        source_text: &str,
    ) -> Option<String> {
        use tsz::parser::syntax_kind_ext;

        let mut type_param_decl = None;
        for symbol in binder.symbols.iter() {
            if symbol.escaped_name != name {
                continue;
            }
            for &decl in &symbol.declarations {
                if arena
                    .get(decl)
                    .is_some_and(|node| node.kind == syntax_kind_ext::TYPE_PARAMETER)
                {
                    type_param_decl = Some(decl);
                    break;
                }
            }
            if type_param_decl.is_some() {
                break;
            }
        }
        let mut current = type_param_decl?;
        for _ in 0..24 {
            let ext = arena.get_extended(current)?;
            if ext.parent == current {
                break;
            }
            current = ext.parent;
            let node = arena.get(current)?;
            if node.kind == syntax_kind_ext::ARROW_FUNCTION
                || node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            {
                let function = arena.get_function(node)?;
                let type_params = function
                    .type_parameters
                    .as_ref()
                    .map(|list| {
                        list.nodes
                            .iter()
                            .filter_map(|&idx| {
                                Self::node_text_slice(idx, arena, source_text).map(|text| {
                                    text.trim()
                                        .trim_start_matches('<')
                                        .trim_end_matches('>')
                                        .trim()
                                        .to_string()
                                })
                            })
                            .filter(|s| !s.is_empty())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                let params = function
                    .parameters
                    .nodes
                    .iter()
                    .filter_map(|&param_idx| {
                        let param_node = arena.get(param_idx)?;
                        let param = arena.get_parameter(param_node)?;
                        let pname = arena.get_identifier_text(param.name)?;
                        let ptype = if param.type_annotation.is_some() {
                            let raw =
                                Self::node_text_slice(param.type_annotation, arena, source_text)?;
                            raw.trim()
                                .trim_end_matches(',')
                                .trim_end_matches(';')
                                .trim_end_matches(')')
                                .trim()
                        } else {
                            "any"
                        };
                        Some(format!("{pname}: {ptype}"))
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let return_type = if function.type_annotation.is_some() {
                    Self::node_text_slice(function.type_annotation, arena, source_text)
                        .unwrap_or("any")
                } else if node.kind == syntax_kind_ext::ARROW_FUNCTION {
                    let body_node = arena.get(function.body)?;
                    if body_node.kind == syntax_kind_ext::BLOCK {
                        "void"
                    } else {
                        "any"
                    }
                } else {
                    "any"
                };
                let signature = if type_params.is_empty() {
                    format!("({params}): {return_type}")
                } else {
                    format!("<{type_params}>({params}): {return_type}")
                };
                return Some(signature);
            }
        }
        None
    }

    fn is_merged_namespace_symbol(name: &str, binder: &tsz::binder::BinderState) -> bool {
        use tsz::binder::symbol_flags;

        binder
            .file_locals
            .get(name)
            .and_then(|sym_id| binder.symbols.get(sym_id))
            .is_some_and(|symbol| {
                (symbol.flags & (symbol_flags::FUNCTION | symbol_flags::CLASS)) != 0
                    && (symbol.flags
                        & (symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE))
                        != 0
            })
    }

    fn method_signature_from_detail(detail: &str) -> Option<String> {
        if !detail.starts_with('(') {
            return None;
        }
        Some(Self::arrow_to_colon(detail))
    }

    fn arrow_to_colon(type_string: &str) -> String {
        let bytes = type_string.as_bytes();
        let mut depth = 0i32;
        let mut last_close = None;
        for (i, &b) in bytes.iter().enumerate() {
            match b {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        last_close = Some(i);
                    }
                }
                _ => {}
            }
        }
        if let Some(close_idx) = last_close {
            let after = &type_string[close_idx + 1..];
            if let Some(arrow_pos) = after.find(" => ") {
                let before = &type_string[..close_idx + 1];
                let ret = &after[arrow_pos + 4..];
                return format!("{before}: {ret}");
            }
        }
        type_string.to_string()
    }

    /// Determine var/let/const from the declaration source text.
    pub(crate) fn get_var_keyword_from_source(
        name: &str,
        binder: &tsz::binder::BinderState,
        arena: &tsz::parser::node::NodeArena,
        source_text: &str,
    ) -> Option<&'static str> {
        use tsz::parser::syntax_kind_ext;

        let symbol_id = binder.file_locals.get(name)?;
        let sym = binder.symbols.get(symbol_id)?;
        let decl = if sym.value_declaration.is_some() {
            sym.value_declaration
        } else {
            *sym.declarations.first()?
        };
        let node = arena.get(decl)?;
        if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        // Walk up to VariableStatement to find the keyword
        let ext = arena.get_extended(decl)?;
        let parent = ext.parent;
        let parent_node = arena.get(parent)?;
        if parent_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            return None;
        }
        let gp_ext = arena.get_extended(parent)?;
        let gp = gp_ext.parent;
        let gp_node = arena.get(gp)?;
        if gp_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
            return None;
        }
        // Read the first keyword from the statement text
        let start = gp_node.pos as usize;
        let end = gp_node.end.min(source_text.len() as u32) as usize;
        if start >= end {
            return None;
        }
        let stmt_text = source_text[start..end].trim_start();
        if stmt_text.starts_with("const ") || stmt_text.starts_with("const\t") {
            Some("const")
        } else if stmt_text.starts_with("let ") || stmt_text.starts_with("let\t") {
            Some("let")
        } else if stmt_text.starts_with("var ") || stmt_text.starts_with("var\t") {
            Some("var")
        } else {
            None
        }
    }

    /// Extract function signature from source text and append as displayParts.
    pub(crate) fn append_function_signature_from_source(
        parts: &mut Vec<serde_json::Value>,
        name: &str,
        binder: &tsz::binder::BinderState,
        arena: &tsz::parser::node::NodeArena,
        source_text: &str,
    ) {
        let decl_text = binder.file_locals.get(name).and_then(|sid| {
            let sym = binder.symbols.get(sid)?;
            let decl = if sym.value_declaration.is_some() {
                sym.value_declaration
            } else {
                *sym.declarations.first()?
            };
            let node = arena.get(decl)?;
            let start = node.pos as usize;
            let end = node.end.min(source_text.len() as u32) as usize;
            (start < end).then(|| &source_text[start..end])
        });

        if let Some(text) = decl_text
            && let Some(open) = text.find('(')
        {
            let mut depth = 0;
            let mut close = None;
            for (i, ch) in text[open..].char_indices() {
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
            if let Some(close_pos) = close {
                let params_text = &text[open + 1..close_pos];
                parts.push(serde_json::json!({"text": "(", "kind": "punctuation"}));
                let params: Vec<&str> = if params_text.trim().is_empty() {
                    vec![]
                } else {
                    params_text.split(',').collect()
                };
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        parts.push(serde_json::json!({"text": ",", "kind": "punctuation"}));
                        parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                    }
                    let param = param.trim();
                    if let Some(colon_pos) = param.find(':') {
                        let pname = param[..colon_pos].trim();
                        let ptype = param[colon_pos + 1..].trim();
                        parts.push(serde_json::json!({"text": pname, "kind": "parameterName"}));
                        parts.push(serde_json::json!({"text": ":", "kind": "punctuation"}));
                        parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                        parts.push(serde_json::json!({"text": ptype, "kind": "keyword"}));
                    } else {
                        parts.push(serde_json::json!({"text": param, "kind": "parameterName"}));
                    }
                }
                parts.push(serde_json::json!({"text": ")", "kind": "punctuation"}));

                let after_close = text[close_pos + 1..].trim_start();
                if let Some(rest) = after_close.strip_prefix(':') {
                    let ret_type = rest.trim_start();
                    let ret_type = ret_type.split(['{', '\n']).next().unwrap_or("").trim();
                    if !ret_type.is_empty() {
                        parts.push(serde_json::json!({"text": ":", "kind": "punctuation"}));
                        parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                        parts.push(serde_json::json!({"text": ret_type, "kind": "keyword"}));
                    }
                } else {
                    parts.push(serde_json::json!({"text": ":", "kind": "punctuation"}));
                    parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                    parts.push(serde_json::json!({"text": "void", "kind": "keyword"}));
                }
                return;
            }
        }

        // Fallback: empty parens
        parts.push(serde_json::json!({"text": "(", "kind": "punctuation"}));
        parts.push(serde_json::json!({"text": ")", "kind": "punctuation"}));
        parts.push(serde_json::json!({"text": ":", "kind": "punctuation"}));
        parts.push(serde_json::json!({"text": " ", "kind": "space"}));
        parts.push(serde_json::json!({"text": "void", "kind": "keyword"}));
    }

    /// Extract type annotation from source text and append as displayParts.
    pub(crate) fn append_type_annotation_from_source(
        parts: &mut Vec<serde_json::Value>,
        name: &str,
        binder: &tsz::binder::BinderState,
        arena: &tsz::parser::node::NodeArena,
        source_text: &str,
    ) -> bool {
        let decl_text = binder.file_locals.get(name).and_then(|sid| {
            let sym = binder.symbols.get(sid)?;
            let decl = if sym.value_declaration.is_some() {
                sym.value_declaration
            } else {
                *sym.declarations.first()?
            };
            let node = arena.get(decl)?;
            let start = node.pos as usize;
            let end = node.end.min(source_text.len() as u32) as usize;
            (start < end).then(|| &source_text[start..end])
        });

        if let Some(text) = decl_text {
            // Find the name, then look for : after it
            if let Some(name_pos) = text.find(name) {
                let after_name = &text[name_pos + name.len()..];
                let after_name = after_name.trim_start();
                if let Some(rest) = after_name.strip_prefix(':') {
                    let type_text = rest.trim_start();
                    let type_text = type_text
                        .split(['=', ';', '\n'])
                        .next()
                        .unwrap_or("")
                        .trim();
                    if !type_text.is_empty() {
                        parts.push(serde_json::json!({"text": ":", "kind": "punctuation"}));
                        parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                        parts.push(serde_json::json!({"text": type_text, "kind": "keyword"}));
                        return true;
                    }
                }
            }

            if Self::is_const_variable_symbol(name, binder, arena)
                && let Some(name_pos) = text.find(name)
            {
                let after_name = &text[name_pos + name.len()..];
                if let Some(eq_pos) = after_name.find('=') {
                    let init_text = after_name[eq_pos + 1..]
                        .split([';', '\n'])
                        .next()
                        .unwrap_or("")
                        .trim();
                    if let Some(literal_type) = Self::const_literal_type_text(init_text) {
                        parts.push(serde_json::json!({"text": ":", "kind": "punctuation"}));
                        parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                        parts.push(serde_json::json!({
                            "text": literal_type,
                            "kind": Self::type_display_kind(literal_type)
                        }));
                        return true;
                    }
                }
            }
        }
        false
    }

    fn is_const_variable_symbol(
        name: &str,
        binder: &tsz::binder::BinderState,
        arena: &tsz::parser::node::NodeArena,
    ) -> bool {
        use tsz_parser::parser::flags::node_flags;
        use tsz_parser::syntax_kind_ext;

        let Some(symbol_id) = binder.file_locals.get(name) else {
            return false;
        };
        let Some(sym) = binder.symbols.get(symbol_id) else {
            return false;
        };
        let decl = if sym.value_declaration.is_some() {
            sym.value_declaration
        } else if let Some(&first) = sym.declarations.first() {
            first
        } else {
            return false;
        };

        let mut current = decl;
        for _ in 0..3 {
            let Some(ext) = arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
            let Some(node) = arena.get(current) else {
                break;
            };
            if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                return (node.flags as u32) & node_flags::CONST != 0;
            }
        }
        false
    }

    fn const_literal_type_text(init_text: &str) -> Option<&str> {
        let trimmed = init_text.trim();
        if trimmed.is_empty() {
            return None;
        }
        let bytes = trimmed.as_bytes();
        if bytes.len() >= 2
            && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
                || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
        {
            return Some(trimmed);
        }
        if trimmed == "true" || trimmed == "false" {
            return Some(trimmed);
        }
        let mut chars = trimmed.chars();
        let first = chars.next()?;
        if first == '-' || first == '+' {
            chars.clone().next()?;
        }
        if trimmed
            .chars()
            .all(|ch| ch.is_ascii_digit() || ch == '_' || ch == '.' || ch == '-' || ch == '+')
            && trimmed.chars().any(|ch| ch.is_ascii_digit())
        {
            return Some(trimmed);
        }
        None
    }

    pub(crate) fn handle_signature_help(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);

            // When trigger reason is "characterTyped", suppress signature help
            // if cursor is inside a string, comment, or template literal,
            // or if the typed character is not a direct syntactic part of a call.
            let trigger_reason = request
                .arguments
                .get("triggerReason")
                .and_then(|v| v.as_object());
            let is_character_typed = trigger_reason
                .as_ref()
                .and_then(|r| r.get("kind"))
                .and_then(|v| v.as_str())
                == Some("characterTyped");
            if is_character_typed {
                // Compute byte offset from line/character position
                let byte_offset = {
                    let mut off = 0usize;
                    let mut current_line = 0u32;
                    for (i, ch) in source_text.char_indices() {
                        if current_line == position.line {
                            off = i + position.character as usize;
                            break;
                        }
                        if ch == '\n' {
                            current_line += 1;
                        }
                    }
                    off
                };
                // Check the position of the just-typed character (offset - 1),
                // since the cursor is positioned after the typed character.
                if byte_offset > 0 && Self::is_in_string_or_comment(&source_text, byte_offset - 1) {
                    return None;
                }
            }

            let interner = TypeInterner::new();
            let provider = SignatureHelpProvider::new(
                &arena,
                &binder,
                &line_map,
                &interner,
                &source_text,
                file,
            );
            let mut type_cache = None;
            let sig_help = provider.get_signature_help(root, position, &mut type_cache)?;

            // For "characterTyped", verify the typed trigger character is a direct
            // syntactic part of the call expression (syntactic owner check).
            // This suppresses signature help when e.g. `(` creates a nested call,
            // or `,` is inside an object/array literal argument.
            if is_character_typed {
                let trigger_char = trigger_reason
                    .as_ref()
                    .and_then(|r| r.get("triggerCharacter"))
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.chars().next());
                if let Some(tc) = trigger_char {
                    let byte_offset = {
                        let mut off = 0usize;
                        let mut current_line = 0u32;
                        for (i, ch) in source_text.char_indices() {
                            if current_line == position.line {
                                off = i + position.character as usize;
                                break;
                            }
                            if ch == '\n' {
                                current_line += 1;
                            }
                        }
                        off
                    };
                    let typed_pos = byte_offset.saturating_sub(1);
                    let span_start = sig_help.applicable_span_start as usize;
                    if !Self::is_trigger_syntactic_owner(&source_text, tc, typed_pos, span_start) {
                        return None;
                    }
                }
            }

            let items: Vec<serde_json::Value> = sig_help
                .signatures
                .iter()
                .map(|sig| {
                    let params: Vec<serde_json::Value> = sig
                        .parameters
                        .iter()
                        .map(|p| {
                            let display_parts = Self::tokenize_param_label(&p.label);
                            // Build param JSON with correct field order:
                            // name, documentation, displayParts, isOptional, isRest
                            let mut map = serde_json::Map::new();
                            map.insert("name".to_string(), serde_json::json!(p.name));
                            if let Some(ref doc) = p.documentation {
                                map.insert(
                                    "documentation".to_string(),
                                    serde_json::json!([{"text": doc, "kind": "text"}]),
                                );
                            } else {
                                map.insert("documentation".to_string(), serde_json::json!([]));
                            }
                            map.insert(
                                "displayParts".to_string(),
                                serde_json::json!(display_parts),
                            );
                            map.insert("isOptional".to_string(), serde_json::json!(p.is_optional));
                            map.insert("isRest".to_string(), serde_json::json!(p.is_rest));
                            serde_json::Value::Object(map)
                        })
                        .collect();
                    let name_kind = if sig.is_constructor {
                        "className"
                    } else {
                        "functionName"
                    };
                    let prefix_parts = Self::tokenize_sig_prefix(&sig.prefix, name_kind);
                    let suffix_parts = Self::tokenize_sig_suffix(&sig.suffix, name_kind);
                    let mut item = serde_json::json!({
                        "isVariadic": sig.is_variadic,
                        "prefixDisplayParts": prefix_parts,
                        "suffixDisplayParts": suffix_parts,
                        "separatorDisplayParts": [
                            {"text": ",", "kind": "punctuation"},
                            {"text": " ", "kind": "space"}
                        ],
                        "parameters": params,
                    });
                    if let Some(ref doc) = sig.documentation {
                        item["documentation"] = serde_json::json!([{"text": doc, "kind": "text"}]);
                    }
                    // Omit "documentation" when empty (TypeScript omits it)
                    // Build tags: param tags from parameter documentation + non-param tags
                    let mut tags: Vec<serde_json::Value> = Vec::new();
                    // Add @param tags from parameter documentation
                    for p in &sig.parameters {
                        if let Some(ref doc) = p.documentation
                            && !doc.is_empty()
                        {
                            tags.push(serde_json::json!({
                                "name": "param",
                                "text": [
                                    {"text": &p.name, "kind": "parameterName"},
                                    {"text": " ", "kind": "space"},
                                    {"text": doc, "kind": "text"}
                                ]
                            }));
                        }
                    }
                    // Add non-param tags (e.g. @returns, @mytag)
                    for tag in &sig.tags {
                        if tag.text.is_empty() {
                            tags.push(serde_json::json!({
                                "name": &tag.name,
                                "text": []
                            }));
                        } else {
                            tags.push(serde_json::json!({
                                "name": &tag.name,
                                "text": [{"text": &tag.text, "kind": "text"}]
                            }));
                        }
                    }
                    item["tags"] = serde_json::json!(tags);
                    item
                })
                .collect();
            Some(serde_json::json!({
                "items": items,
                "applicableSpan": {
                    "start": sig_help.applicable_span_start,
                    "length": sig_help.applicable_span_length,
                },
                "selectedItemIndex": sig_help.active_signature,
                "argumentIndex": sig_help.active_parameter,
                "argumentCount": sig_help.argument_count,
            }))
        })();
        // Always return a body - processResponse asserts !!response.body.
        // When no signature help is found, return empty items array.
        // The test-worker converts empty items to undefined.
        let body = result.unwrap_or_else(|| {
            serde_json::json!({
                "items": [],
                "applicableSpan": { "start": 0, "length": 0 },
                "selectedItemIndex": 0,
                "argumentIndex": 0,
                "argumentCount": 0,
            })
        });
        self.stub_response(seq, request, Some(body))
    }

    /// Determine the display part kind for a type string.
    pub(crate) fn type_display_kind(type_str: &str) -> &'static str {
        match type_str {
            "void" | "number" | "string" | "boolean" | "any" | "never" | "undefined" | "null"
            | "unknown" | "object" | "symbol" | "bigint" | "true" | "false" => "keyword",
            _ => "text",
        }
    }

    /// Tokenize a signature prefix like "foo(" or "foo<T>(" into display parts.
    pub(crate) fn tokenize_sig_prefix(prefix: &str, name_kind: &str) -> Vec<serde_json::Value> {
        let mut parts = Vec::new();
        // The prefix ends with '('
        if let Some(stripped) = prefix.strip_suffix('(') {
            // Check for type params like "foo<T>"
            if let Some(angle_pos) = stripped.find('<') {
                let name = &stripped[..angle_pos];
                if !name.is_empty() {
                    parts.push(serde_json::json!({"text": name, "kind": name_kind}));
                }
                parts.push(serde_json::json!({"text": "<", "kind": "punctuation"}));
                let type_params_inner = &stripped[angle_pos + 1..];
                let type_params_inner = type_params_inner
                    .strip_suffix('>')
                    .unwrap_or(type_params_inner);
                // Tokenize type parameters
                Self::tokenize_type_params(type_params_inner, &mut parts);
                parts.push(serde_json::json!({"text": ">", "kind": "punctuation"}));
            } else if !stripped.is_empty() {
                parts.push(serde_json::json!({"text": stripped, "kind": name_kind}));
            }
            parts.push(serde_json::json!({"text": "(", "kind": "punctuation"}));
        } else {
            // Fallback
            parts.push(serde_json::json!({"text": prefix, "kind": "text"}));
        }
        parts
    }

    /// Tokenize type parameters like "T, U extends string" into display parts.
    pub(crate) fn tokenize_type_params(input: &str, parts: &mut Vec<serde_json::Value>) {
        let params: Vec<&str> = input.split(',').collect();
        for (i, param) in params.iter().enumerate() {
            if i > 0 {
                parts.push(serde_json::json!({"text": ",", "kind": "punctuation"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
            }
            let trimmed = param.trim();
            if let Some(ext_pos) = trimmed.find(" extends ") {
                let name = &trimmed[..ext_pos];
                parts.push(serde_json::json!({"text": name, "kind": "typeParameterName"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                parts.push(serde_json::json!({"text": "extends", "kind": "keyword"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                let constraint = &trimmed[ext_pos + 9..];
                let kind = Self::type_display_kind(constraint);
                parts.push(serde_json::json!({"text": constraint, "kind": kind}));
            } else {
                parts.push(serde_json::json!({"text": trimmed, "kind": "typeParameterName"}));
            }
        }
    }

    /// Tokenize a signature suffix like "): void" into display parts.
    pub(crate) fn tokenize_sig_suffix(suffix: &str, name_kind: &str) -> Vec<serde_json::Value> {
        let mut parts = Vec::new();
        // Suffix is typically "): returnType"
        if let Some(rest) = suffix.strip_prefix(')') {
            parts.push(serde_json::json!({"text": ")", "kind": "punctuation"}));
            if let Some(rest) = rest.strip_prefix(':') {
                parts.push(serde_json::json!({"text": ":", "kind": "punctuation"}));
                if let Some(rest) = rest.strip_prefix(' ') {
                    parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                    // For constructors, use className kind for return type
                    if name_kind == "className" {
                        parts.push(serde_json::json!({"text": rest, "kind": "className"}));
                    } else {
                        Self::tokenize_type_expr(rest, &mut parts);
                    }
                } else if !rest.is_empty() {
                    if name_kind == "className" {
                        parts.push(serde_json::json!({"text": rest, "kind": "className"}));
                    } else {
                        Self::tokenize_type_expr(rest, &mut parts);
                    }
                }
            } else if !rest.is_empty() {
                parts.push(serde_json::json!({"text": rest, "kind": "text"}));
            }
        } else {
            parts.push(serde_json::json!({"text": suffix, "kind": "text"}));
        }
        parts
    }

    /// Tokenize a type expression into display parts.
    pub(crate) fn tokenize_type_expr(type_str: &str, parts: &mut Vec<serde_json::Value>) {
        // Handle type predicates: "x is Type"
        if let Some(is_pos) = type_str.find(" is ") {
            let before = &type_str[..is_pos];
            // Check for "asserts x is Type"
            if let Some(param_name) = before.strip_prefix("asserts ") {
                parts.push(serde_json::json!({"text": "asserts", "kind": "keyword"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                parts.push(serde_json::json!({"text": param_name, "kind": "parameterName"}));
            } else {
                parts.push(serde_json::json!({"text": before, "kind": "parameterName"}));
            }
            parts.push(serde_json::json!({"text": " ", "kind": "space"}));
            parts.push(serde_json::json!({"text": "is", "kind": "keyword"}));
            parts.push(serde_json::json!({"text": " ", "kind": "space"}));
            let after = &type_str[is_pos + 4..];
            let kind = Self::type_display_kind(after);
            parts.push(serde_json::json!({"text": after, "kind": kind}));
            return;
        }
        let kind = Self::type_display_kind(type_str);
        parts.push(serde_json::json!({"text": type_str, "kind": kind}));
    }

    /// Tokenize a parameter label like "x: number" or "...args: string[]" into display parts.
    pub(crate) fn tokenize_param_label(label: &str) -> Vec<serde_json::Value> {
        let mut parts = Vec::new();
        let remaining = label;

        // Handle rest parameter prefix
        let remaining = if let Some(rest) = remaining.strip_prefix("...") {
            parts.push(serde_json::json!({"text": "...", "kind": "punctuation"}));
            rest
        } else {
            remaining
        };

        // Split at ": " for name and type
        if let Some(colon_pos) = remaining.find(": ") {
            let name_part = &remaining[..colon_pos];
            // Handle optional marker
            let (name, has_question) = if let Some(n) = name_part.strip_suffix('?') {
                (n, true)
            } else {
                (name_part, false)
            };
            parts.push(serde_json::json!({"text": name, "kind": "parameterName"}));
            if has_question {
                parts.push(serde_json::json!({"text": "?", "kind": "punctuation"}));
            }
            parts.push(serde_json::json!({"text": ":", "kind": "punctuation"}));
            parts.push(serde_json::json!({"text": " ", "kind": "space"}));
            let type_str = &remaining[colon_pos + 2..];
            Self::tokenize_type_expr(type_str, &mut parts);
        } else {
            // No colon - just a parameter name
            parts.push(serde_json::json!({"text": remaining, "kind": "parameterName"}));
        }

        parts
    }

    /// Check if a byte offset is inside a string literal, template literal, or comment.
    /// Used to suppress signature help when trigger character is typed in non-code context.
    fn is_in_string_or_comment(source: &str, offset: usize) -> bool {
        let bytes = source.as_bytes();
        let mut i = 0;
        while i < bytes.len() && i < offset {
            match bytes[i] {
                b'/' if i + 1 < bytes.len() => {
                    if bytes[i + 1] == b'/' {
                        // Line comment — skip to end of line
                        i += 2;
                        while i < bytes.len() && bytes[i] != b'\n' {
                            if i == offset {
                                return true;
                            }
                            i += 1;
                        }
                        continue;
                    } else if bytes[i + 1] == b'*' {
                        // Block comment — skip to */
                        i += 2;
                        while i + 1 < bytes.len() {
                            if i == offset || i + 1 == offset {
                                return true;
                            }
                            if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                                i += 2;
                                break;
                            }
                            i += 1;
                        }
                        continue;
                    }
                    i += 1;
                }
                b'\'' | b'"' => {
                    let quote = bytes[i];
                    i += 1;
                    while i < bytes.len() && bytes[i] != quote {
                        if i == offset {
                            return true;
                        }
                        if bytes[i] == b'\\' {
                            i += 1; // skip escaped char
                        }
                        i += 1;
                    }
                    if i < bytes.len() {
                        i += 1; // skip closing quote
                    }
                }
                b'`' => {
                    // Template literal
                    i += 1;
                    let mut depth = 0u32;
                    while i < bytes.len() {
                        if i == offset && depth == 0 {
                            return true;
                        }
                        if bytes[i] == b'\\' {
                            i += 2;
                            continue;
                        }
                        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                            depth += 1;
                            i += 2;
                            continue;
                        }
                        if bytes[i] == b'}' && depth > 0 {
                            depth -= 1;
                            i += 1;
                            continue;
                        }
                        if bytes[i] == b'`' && depth == 0 {
                            i += 1;
                            break;
                        }
                        i += 1;
                    }
                }
                _ => {
                    i += 1;
                }
            }
        }
        false
    }

    /// Check whether a typed trigger character is a "syntactic owner" of the
    /// call expression whose applicable span starts at `span_start`.
    ///
    /// For `(`: the typed `(` must be the opening delimiter of the call (right
    ///   before `span_start`).
    /// For `,`: the typed `,` must be at bracket-nesting depth 0 between
    ///   `span_start` and the cursor, meaning it separates top-level arguments.
    /// For `<`: the typed `<` must immediately precede the call's type argument
    ///   span (similar to `(`).
    ///
    /// This mirrors TypeScript's `isSyntacticOwner` check which prevents
    /// `characterTyped` from triggering signature help for nested expressions
    /// (e.g., `(` inside an object literal argument, `,` inside array literals).
    fn is_trigger_syntactic_owner(
        source: &str,
        trigger_char: char,
        typed_pos: usize,
        span_start: usize,
    ) -> bool {
        let bytes = source.as_bytes();
        match trigger_char {
            '(' => {
                // The typed `(` should be the opening paren of the call.
                // The applicable span starts right after the `(`, so the
                // `(` should be at span_start - 1 (or typed_pos == span_start - 1).
                // Allow a small tolerance for whitespace differences.
                span_start > 0 && typed_pos == span_start - 1
            }
            '<' => {
                // Similar to `(`: the `<` should be right before the span.
                span_start > 0 && typed_pos == span_start - 1
            }
            ',' => {
                // The `,` must be at nesting depth 0 within the call's argument
                // list (between span_start and typed_pos).
                if typed_pos < span_start || typed_pos >= bytes.len() {
                    return false;
                }
                let mut depth_paren = 0i32;
                let mut depth_bracket = 0i32;
                let mut depth_brace = 0i32;
                let mut i = span_start;
                while i < typed_pos && i < bytes.len() {
                    match bytes[i] {
                        b'(' => depth_paren += 1,
                        b')' => depth_paren -= 1,
                        b'[' => depth_bracket += 1,
                        b']' => depth_bracket -= 1,
                        b'{' => depth_brace += 1,
                        b'}' => depth_brace -= 1,
                        b'\'' | b'"' => {
                            // Skip string literals
                            let quote = bytes[i];
                            i += 1;
                            while i < bytes.len() && bytes[i] != quote {
                                if bytes[i] == b'\\' {
                                    i += 1;
                                }
                                i += 1;
                            }
                        }
                        b'`' => {
                            // Template literal: if typed_pos falls inside it
                            // (including expression parts), the trigger is nested.
                            i += 1;
                            let mut tdepth = 0u32;
                            while i < bytes.len() {
                                if i == typed_pos {
                                    // The trigger character is inside this template literal
                                    return false;
                                }
                                if bytes[i] == b'\\' {
                                    i += 2;
                                    continue;
                                }
                                if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                                    tdepth += 1;
                                    i += 2;
                                    continue;
                                }
                                if bytes[i] == b'}' && tdepth > 0 {
                                    tdepth -= 1;
                                    i += 1;
                                    continue;
                                }
                                if bytes[i] == b'`' && tdepth == 0 {
                                    break;
                                }
                                i += 1;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
                depth_paren == 0 && depth_bracket == 0 && depth_brace == 0
            }
            _ => true,
        }
    }
}
