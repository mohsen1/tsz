//! JSDoc parsing, emission, and type alias handling

#[allow(unused_imports)]
use super::super::{DeclarationEmitter, ImportPlan, PlannedImportModule, PlannedImportSymbol};
#[allow(unused_imports)]
use crate::emitter::type_printer::TypePrinter;
#[allow(unused_imports)]
use crate::output::source_writer::{SourcePosition, SourceWriter, source_position_from_offset};
#[allow(unused_imports)]
use rustc_hash::{FxHashMap, FxHashSet};
#[allow(unused_imports)]
use std::sync::Arc;
#[allow(unused_imports)]
use tracing::debug;
#[allow(unused_imports)]
use tsz_binder::{BinderState, SymbolId, symbol_flags};
#[allow(unused_imports)]
use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};
#[allow(unused_imports)]
use tsz_parser::parser::ParserState;
#[allow(unused_imports)]
use tsz_parser::parser::node::{Node, NodeAccess, NodeArena};
#[allow(unused_imports)]
use tsz_parser::parser::syntax_kind_ext;
#[allow(unused_imports)]
use tsz_parser::parser::{NodeIndex, NodeList};
#[allow(unused_imports)]
use tsz_scanner::SyntaxKind;

use super::{JsdocParamDecl, JsdocTypeAliasDecl};

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn statement_has_attached_jsdoc(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
        stmt_node: &tsz_parser::parser::node::Node,
    ) -> bool {
        let text = source_file.text.as_ref();
        let bytes = text.as_bytes();
        let mut actual_start = stmt_node.pos as usize;
        while actual_start < bytes.len()
            && matches!(bytes[actual_start], b' ' | b'\t' | b'\r' | b'\n')
        {
            actual_start += 1;
        }

        let mut scan_start = actual_start;
        for comment in source_file.comments.iter().rev() {
            if comment.end as usize > scan_start {
                continue;
            }

            let between = &text[comment.end as usize..scan_start];
            if !between
                .bytes()
                .all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
            {
                break;
            }

            let comment_text = &text[comment.pos as usize..comment.end as usize];
            if comment_text.starts_with("/**") && comment_text != "/**/" {
                return true;
            }

            scan_start = comment.pos as usize;
        }

        false
    }

    pub(crate) fn leading_jsdoc_type_expr_for_pos(&self, pos: u32) -> Option<String> {
        let text = self.source_file_text.as_deref()?;
        let bytes = text.as_bytes();
        let mut actual_start = pos as usize;
        while actual_start < bytes.len()
            && matches!(bytes[actual_start], b' ' | b'\t' | b'\r' | b'\n')
        {
            actual_start += 1;
        }

        let nearest = self
            .all_comments
            .iter()
            .filter(|comment| comment.end as usize <= actual_start)
            .filter(|comment| is_jsdoc_comment(comment, text))
            .filter(|comment| {
                Self::jsdoc_attaches_through_var_prefix(&text[comment.end as usize..actual_start])
            })
            .max_by_key(|comment| comment.end)?;

        let jsdoc = get_jsdoc_content(nearest, text);
        if let Some(expr) = Self::extract_jsdoc_type_expression(&jsdoc) {
            return Some(expr.trim().to_string());
        }

        None
    }

    pub(in crate::declaration_emitter) fn jsdoc_attaches_through_var_prefix(between: &str) -> bool {
        let trimmed = between.trim();
        if trimmed.is_empty() {
            return true;
        }

        trimmed.split_whitespace().all(|word| {
            matches!(
                word,
                "export" | "declare" | "const" | "let" | "var" | "using" | "await"
            )
        })
    }

    pub(in crate::declaration_emitter) fn extract_jsdoc_type_expression(
        jsdoc: &str,
    ) -> Option<&str> {
        let typedef_pos = jsdoc.find("@typedef");
        let mut tag_pos = jsdoc.find("@type");

        while let Some(pos) = tag_pos {
            let next_char = jsdoc[pos + "@type".len()..].chars().next();
            if !next_char.is_some_and(|c| c.is_alphabetic()) {
                if let Some(td_pos) = typedef_pos
                    && td_pos < pos
                {
                    let typedef_rest = &jsdoc[td_pos + "@typedef".len()..pos];
                    let mut has_non_object_base = false;
                    if let Some(open) = typedef_rest.find('{')
                        && let Some(close) = typedef_rest[open..].find('}')
                    {
                        let base = typedef_rest[open + 1..open + close].trim();
                        if base != "Object" && base != "object" && !base.is_empty() {
                            has_non_object_base = true;
                        }
                    }
                    if !has_non_object_base {
                        return None;
                    }
                }
                break;
            }
            tag_pos = jsdoc[pos + 1..].find("@type").map(|p| p + pos + 1);
        }
        let tag_pos = tag_pos?;
        let rest = &jsdoc[tag_pos + "@type".len()..];

        if let Some(open) = rest.find('{') {
            let after_open = &rest[open + 1..];
            let mut depth = 1usize;
            let mut end_idx = None;
            for (i, ch) in after_open.char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end_idx = Some(i);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if let Some(end_idx) = end_idx {
                return Some(after_open[..end_idx].trim());
            }
        }

        let rest = rest.trim_start();
        if rest.is_empty() || rest.starts_with('@') || rest.starts_with('*') {
            return None;
        }
        let end = rest
            .find('\n')
            .or_else(|| rest.find("*/"))
            .unwrap_or(rest.len());
        let expr = rest[..end].trim().trim_end_matches('*').trim();
        if expr.is_empty() { None } else { Some(expr) }
    }

    pub(in crate::declaration_emitter) fn jsdoc_name_like_type_reference(expr: &str) -> bool {
        let expr = expr.trim();
        if expr.is_empty() {
            return false;
        }

        if expr
            .chars()
            .all(|ch| ch == '_' || ch == '$' || ch == '.' || ch.is_ascii_alphanumeric())
        {
            return true;
        }

        let Some(rest) = expr
            .strip_prefix("import(\"")
            .or_else(|| expr.strip_prefix("import('"))
        else {
            return false;
        };

        let quote = if expr.starts_with("import(\"") {
            '"'
        } else {
            '\''
        };
        let Some(close) = rest.find(&format!("{quote})")) else {
            return false;
        };
        let suffix = &rest[close + 2..];
        let Some(member_path) = suffix.strip_prefix('.') else {
            return false;
        };
        !member_path.is_empty()
            && member_path
                .chars()
                .all(|ch| ch == '_' || ch == '$' || ch == '.' || ch.is_ascii_alphanumeric())
    }

    pub(crate) fn jsdoc_name_like_type_expr_for_pos(&self, pos: u32) -> Option<String> {
        let expr = self.leading_jsdoc_type_expr_for_pos(pos)?;
        if Self::jsdoc_name_like_type_reference(&expr) {
            Some(expr)
        } else {
            None
        }
    }

    pub(crate) fn jsdoc_name_like_type_expr_for_node(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;
        self.jsdoc_name_like_type_expr_for_pos(node.pos)
    }

    pub(in crate::declaration_emitter) fn leading_jsdoc_comment_chain_for_pos(
        &self,
        pos: u32,
    ) -> Vec<String> {
        let Some(text) = self.source_file_text.as_deref() else {
            return Vec::new();
        };
        let bytes = text.as_bytes();
        let mut actual_start = pos as usize;
        while actual_start < bytes.len()
            && matches!(bytes[actual_start], b' ' | b'\t' | b'\r' | b'\n')
        {
            actual_start += 1;
        }

        let nearest = self
            .all_comments
            .iter()
            .filter(|comment| comment.end as usize <= actual_start)
            .filter(|comment| is_jsdoc_comment(comment, text))
            .filter(|comment| {
                Self::jsdoc_attaches_through_var_prefix(&text[comment.end as usize..actual_start])
            })
            .max_by_key(|comment| comment.end);

        let Some(nearest) = nearest else {
            return Vec::new();
        };

        let mut chain = vec![get_jsdoc_content(nearest, text)];
        let mut current_start = nearest.pos as usize;
        for comment in self
            .all_comments
            .iter()
            .filter(|comment| comment.end <= nearest.pos)
            .filter(|comment| is_jsdoc_comment(comment, text))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            let between = &text[comment.end as usize..current_start];
            if !between
                .bytes()
                .all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
            {
                break;
            }
            chain.push(get_jsdoc_content(comment, text));
            current_start = comment.pos as usize;
        }
        chain.reverse();
        chain
    }

    pub(in crate::declaration_emitter) fn leading_jsdoc_comment_chain_for_node_or_ancestors(
        &self,
        idx: NodeIndex,
    ) -> Vec<String> {
        let mut current = idx;
        for _ in 0..5 {
            let Some(node) = self.arena.get(current) else {
                break;
            };
            let chain = self.leading_jsdoc_comment_chain_for_pos(node.pos);
            if !chain.is_empty() {
                return chain;
            }
            let Some(ext) = self.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }
        Vec::new()
    }

    pub(crate) fn function_like_jsdoc_for_node(&self, idx: NodeIndex) -> Option<String> {
        let chain = self.leading_jsdoc_comment_chain_for_node_or_ancestors(idx);
        if chain.is_empty() {
            None
        } else {
            Some(chain.join("\n"))
        }
    }

    pub(in crate::declaration_emitter) fn normalize_jsdoc_block(jsdoc: &str) -> String {
        jsdoc
            .lines()
            .map(|line| line.trim_start_matches('*').trim())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub(in crate::declaration_emitter) fn normalize_jsdoc_param_name(text: &str) -> (String, bool) {
        let text = text.trim();
        if let Some(inner) = text
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            let name = inner.split('=').next().unwrap_or(inner).trim();
            return (name.to_string(), true);
        }
        (text.to_string(), false)
    }

    pub(in crate::declaration_emitter) fn normalize_jsdoc_type_text(
        type_expr: &str,
        rest: bool,
    ) -> String {
        let trimmed = type_expr.trim();
        let normalized = if trimmed == "*" { "any" } else { trimmed };
        if rest {
            format!("{normalized}[]")
        } else {
            normalized.to_string()
        }
    }

    pub(in crate::declaration_emitter) fn parse_jsdoc_param_decl(
        line: &str,
    ) -> Option<JsdocParamDecl> {
        let rest = line.strip_prefix("@param")?.trim();
        let (raw_type_expr, raw_name) = Self::parse_jsdoc_braced_type_and_name(rest)?;
        let raw_name = raw_name
            .split_whitespace()
            .next()
            .filter(|name| !name.is_empty())?;
        let (name, optional_name) = Self::normalize_jsdoc_param_name(raw_name);

        let mut type_expr = raw_type_expr.trim();
        let optional_type = type_expr.ends_with('=');
        if optional_type {
            type_expr = type_expr[..type_expr.len() - 1].trim();
        }

        let (rest_param, base_type) = if let Some(stripped) = type_expr.strip_prefix("...") {
            (true, stripped.trim())
        } else {
            (false, type_expr)
        };

        Some(JsdocParamDecl {
            name,
            type_text: Self::normalize_jsdoc_type_text(base_type, rest_param),
            optional: optional_name || optional_type,
            rest: rest_param,
        })
    }

    pub(in crate::declaration_emitter) fn parse_jsdoc_param_decls(
        jsdoc: &str,
    ) -> Vec<JsdocParamDecl> {
        jsdoc
            .lines()
            .map(|raw_line| raw_line.trim_start_matches('*').trim())
            .filter_map(Self::parse_jsdoc_param_decl)
            .collect()
    }

    pub(crate) fn jsdoc_param_decl_for_parameter(
        &self,
        param_idx: NodeIndex,
        position: usize,
    ) -> Option<JsdocParamDecl> {
        let jsdoc = self.function_like_jsdoc_for_node(param_idx)?;
        let params = Self::parse_jsdoc_param_decls(&jsdoc);
        if params.is_empty() {
            return None;
        }

        let param_node = self.arena.get(param_idx)?;
        let param = self.arena.get_parameter(param_node)?;
        if let Some(name) = self.get_identifier_text(param.name)
            && let Some(found) = params.iter().find(|decl| decl.name == name)
        {
            return Some(found.clone());
        }

        params.into_iter().nth(position)
    }

    pub(crate) fn parse_jsdoc_return_type_text(jsdoc: &str) -> Option<String> {
        for raw_line in jsdoc.lines() {
            let line = raw_line.trim_start_matches('*').trim();
            let Some(rest) = line
                .strip_prefix("@returns")
                .or_else(|| line.strip_prefix("@return"))
            else {
                continue;
            };
            let rest = rest.trim();
            let (type_expr, _) = Self::parse_jsdoc_braced_type_and_name(rest)?;
            return Some(Self::normalize_jsdoc_type_text(type_expr, false));
        }
        None
    }

    pub(crate) fn jsdoc_return_type_text_for_node(&self, idx: NodeIndex) -> Option<String> {
        let jsdoc = self.function_like_jsdoc_for_node(idx)?;
        Self::parse_jsdoc_return_type_text(&jsdoc)
    }

    pub(crate) fn jsdoc_template_params_for_node(&self, idx: NodeIndex) -> Vec<String> {
        self.function_like_jsdoc_for_node(idx)
            .map(|jsdoc| Self::parse_jsdoc_template_params(&jsdoc))
            .unwrap_or_default()
    }

    pub(crate) fn emit_jsdoc_template_parameters(&mut self, type_params: &[String]) {
        if type_params.is_empty() {
            return;
        }

        self.write("<");
        for (i, param) in type_params.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.write(param);
        }
        self.write(">");
    }

    pub(in crate::declaration_emitter) fn jsdoc_has_function_signature_tags(jsdoc: &str) -> bool {
        jsdoc.lines().any(|raw_line| {
            let line = raw_line.trim_start_matches('*').trim();
            line.starts_with("@param")
                || line.starts_with("@returns")
                || line.starts_with("@return")
                || line.starts_with("@template")
        })
    }

    pub(crate) fn emit_js_function_variable_declaration_if_possible(
        &mut self,
        decl_idx: NodeIndex,
        decl_name: NodeIndex,
        initializer: NodeIndex,
        is_exported: bool,
    ) -> bool {
        if !self.source_is_js_file || !initializer.is_some() {
            return false;
        }

        let Some(name_node) = self.arena.get(decl_name) else {
            return false;
        };
        if name_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        if self
            .leading_jsdoc_type_expr_for_pos(name_node.pos)
            .is_some()
        {
            return false;
        }
        let is_export_equals_root = self.is_js_export_equals_name(decl_name);

        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && init_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return false;
        }
        let Some(func) = self.arena.get_function(init_node) else {
            return false;
        };

        let jsdoc = self.function_like_jsdoc_for_node(initializer);
        if !jsdoc
            .as_deref()
            .is_some_and(Self::jsdoc_has_function_signature_tags)
            && !is_export_equals_root
        {
            return false;
        }

        self.emit_pending_js_export_equals_for_name(decl_name);
        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write("function ");
        self.emit_node(decl_name);

        let jsdoc_template_params = if func
            .type_parameters
            .as_ref()
            .is_none_or(|type_params| type_params.nodes.is_empty())
        {
            jsdoc
                .as_deref()
                .map(Self::parse_jsdoc_template_params)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        if let Some(ref type_params) = func.type_parameters {
            if !type_params.nodes.is_empty() {
                self.emit_type_parameters(type_params);
            } else if !jsdoc_template_params.is_empty() {
                self.emit_jsdoc_template_parameters(&jsdoc_template_params);
            }
        } else if !jsdoc_template_params.is_empty() {
            self.emit_jsdoc_template_parameters(&jsdoc_template_params);
        }

        self.write("(");
        self.emit_parameters_with_body(&func.parameters, func.body);
        self.write(")");

        if func.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(func.type_annotation);
        } else if let Some(return_type_text) = jsdoc
            .as_deref()
            .and_then(Self::parse_jsdoc_return_type_text)
        {
            self.write(": ");
            self.write(&return_type_text);
        } else if let (Some(interner), Some(cache)) = (&self.type_interner, &self.type_cache) {
            let func_type_id = cache
                .node_types
                .get(&initializer.0)
                .copied()
                .or_else(|| self.get_node_type_or_names(&[decl_idx, decl_name, initializer]));
            if let Some(func_type_id) = func_type_id
                && let Some(return_type_id) =
                    tsz_solver::type_queries::get_return_type(*interner, func_type_id)
            {
                if return_type_id == tsz_solver::types::TypeId::ANY
                    && func.body.is_some()
                    && self.body_returns_void(func.body)
                {
                    self.write(": void");
                } else {
                    self.write(": ");
                    self.write(&self.print_type_id(return_type_id));
                }
            } else if func.body.is_some() && self.body_returns_void(func.body) {
                self.write(": void");
            }
        } else if func.body.is_some() && self.body_returns_void(func.body) {
            self.write(": void");
        }

        self.write(";");
        self.write_line();
        self.emit_js_namespace_export_aliases_for_name(decl_name);
        true
    }

    pub(in crate::declaration_emitter) fn parse_jsdoc_callback_alias(
        jsdoc: &str,
    ) -> Option<(String, String)> {
        let mut name = None;
        let mut params = Vec::new();
        let mut return_type = None;

        for raw_line in jsdoc.lines() {
            let line = raw_line.trim_start_matches('*').trim();
            if line.is_empty() {
                continue;
            }

            if let Some(rest) = line.strip_prefix("@callback") {
                let callback_name = rest.trim();
                if !callback_name.is_empty() {
                    name = Some(callback_name.to_string());
                }
                continue;
            }

            if let Some(rest) = line.strip_prefix("@param") {
                let rest = rest.trim();
                if rest.starts_with('{')
                    && let Some(end) = rest[1..].find('}')
                {
                    let type_expr = rest[1..1 + end].trim();
                    let param_name = rest[2 + end..]
                        .split_whitespace()
                        .next()
                        .filter(|name| !name.is_empty())
                        .unwrap_or("arg");
                    let (rest_param, base_type) =
                        if let Some(stripped) = type_expr.strip_prefix("...") {
                            (true, stripped.trim())
                        } else {
                            (false, type_expr)
                        };
                    let ts_type = if base_type == "*" {
                        "any".to_string()
                    } else if rest_param {
                        format!("{base_type}[]")
                    } else {
                        base_type.to_string()
                    };
                    if rest_param {
                        params.push(format!("...{param_name}: {ts_type}"));
                    } else {
                        params.push(format!("{param_name}: {ts_type}"));
                    }
                }
                continue;
            }

            if let Some(rest) = line
                .strip_prefix("@returns")
                .or_else(|| line.strip_prefix("@return"))
            {
                let rest = rest.trim();
                if rest.starts_with('{')
                    && let Some(end) = rest[1..].find('}')
                {
                    let type_expr = rest[1..1 + end].trim();
                    return_type = Some(if type_expr == "*" {
                        "any".to_string()
                    } else {
                        type_expr.to_string()
                    });
                }
            }
        }

        let name = name?;
        let return_type = return_type.unwrap_or_else(|| "any".to_string());
        Some((name, format!("({}) => {return_type}", params.join(", "))))
    }

    pub(crate) fn parse_jsdoc_template_params(jsdoc: &str) -> Vec<String> {
        let mut params = Vec::new();
        let mut seen = FxHashSet::default();

        for raw_line in jsdoc.lines() {
            let line = raw_line.trim_start_matches('*').trim();
            let Some(rest) = line.strip_prefix("@template") else {
                continue;
            };

            for name in rest
                .split([',', ' ', '\t'])
                .map(str::trim)
                .filter(|name| !name.is_empty())
            {
                let name_str = name.to_string();
                if seen.insert(name_str.clone()) {
                    params.push(name_str);
                }
            }
        }

        params
    }

    pub(in crate::declaration_emitter) fn parse_jsdoc_typedef_alias(
        jsdoc: &str,
    ) -> Option<(String, String)> {
        let normalized = Self::normalize_jsdoc_block(jsdoc);
        let tag_pos = normalized.find("@typedef")?;
        let rest = normalized[tag_pos + "@typedef".len()..].trim();
        let (type_expr, name_rest) = Self::parse_jsdoc_braced_type_and_name(rest)?;
        let name = name_rest
            .split_whitespace()
            .next()
            .filter(|name| !name.is_empty())?;
        if type_expr.is_empty() {
            return None;
        }
        Some((name.to_string(), type_expr.to_string()))
    }

    pub(in crate::declaration_emitter) fn parse_jsdoc_braced_type_and_name(
        text: &str,
    ) -> Option<(&str, &str)> {
        let text = text.trim();
        if !text.starts_with('{') {
            return None;
        }

        let mut depth = 0usize;
        for (idx, ch) in text.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        let ty = text[1..idx].trim();
                        let rest = text[idx + 1..].trim();
                        return Some((ty, rest));
                    }
                }
                _ => {}
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn jsdoc_description_lines(jsdoc: &str) -> Vec<String> {
        let mut lines = Vec::new();
        for raw_line in jsdoc.lines() {
            let line = raw_line.trim_start_matches('*').trim();
            if line.starts_with('@') {
                break;
            }
            if !line.is_empty() {
                lines.push(line.to_string());
            }
        }
        lines
    }

    pub(in crate::declaration_emitter) fn jsdoc_has_property_tags(jsdoc: &str) -> bool {
        jsdoc.lines().any(|raw_line| {
            let line = raw_line.trim_start_matches('*').trim();
            line.starts_with("@property") || line.starts_with("@prop")
        })
    }

    pub(in crate::declaration_emitter) fn parse_jsdoc_property_type_alias(
        jsdoc: &str,
    ) -> Option<(String, String)> {
        let (name, base_type) = Self::parse_jsdoc_typedef_alias(jsdoc)?;
        if name == "default" || !matches!(base_type.as_str(), "Object" | "object") {
            return None;
        }

        let mut properties = Vec::new();
        let mut current_property: Option<(String, bool, String, Vec<String>)> = None;

        for raw_line in jsdoc.lines() {
            let line = raw_line.trim_start_matches('*').trim();
            if line.is_empty() {
                continue;
            }

            if let Some(rest) = line
                .strip_prefix("@property")
                .or_else(|| line.strip_prefix("@prop"))
            {
                if let Some(property) = current_property.take() {
                    properties.push(property);
                }

                let rest = rest.trim();
                let (type_expr, name_rest) = Self::parse_jsdoc_braced_type_and_name(rest)?;
                let mut parts = name_rest.split_whitespace();
                let property_name = parts.next()?.trim();
                if property_name.is_empty() {
                    return None;
                }

                let (property_name, optional) =
                    if property_name.starts_with('[') && property_name.ends_with(']') {
                        let trimmed = property_name
                            .trim_start_matches('[')
                            .trim_end_matches(']')
                            .trim_end_matches('=')
                            .to_string();
                        (trimmed, true)
                    } else {
                        (property_name.to_string(), false)
                    };

                let inline_description = parts.collect::<Vec<_>>().join(" ");
                let mut description_lines = Vec::new();
                if !inline_description.is_empty() {
                    description_lines.push(inline_description);
                }

                current_property = Some((
                    property_name,
                    optional,
                    Self::normalize_jsdoc_primitive_type_name(type_expr),
                    description_lines,
                ));
                continue;
            }

            if line.starts_with('@') {
                if let Some(property) = current_property.take() {
                    properties.push(property);
                }
                continue;
            }

            if let Some((_, _, _, description_lines)) = current_property.as_mut() {
                description_lines.push(line.to_string());
            }
        }

        if let Some(property) = current_property.take() {
            properties.push(property);
        }
        if properties.is_empty() {
            return None;
        }

        let mut type_text = String::from("{\n");
        for (property_name, optional, property_type, description_lines) in properties {
            if !description_lines.is_empty() {
                type_text.push_str("    /**\n");
                for line in description_lines {
                    type_text.push_str("     * ");
                    type_text.push_str(&line);
                    type_text.push('\n');
                }
                type_text.push_str("     */\n");
            }
            type_text.push_str("    ");
            type_text.push_str(&property_name);
            if optional {
                type_text.push('?');
            }
            type_text.push_str(": ");
            type_text.push_str(&property_type);
            type_text.push_str(";\n");
        }
        type_text.push('}');

        Some((name, type_text))
    }

    pub(in crate::declaration_emitter) fn normalize_jsdoc_primitive_type_name(
        type_name: &str,
    ) -> String {
        match type_name.trim() {
            "String" => "string".to_string(),
            "Number" => "number".to_string(),
            "Boolean" => "boolean".to_string(),
            "Symbol" => "symbol".to_string(),
            "BigInt" => "bigint".to_string(),
            "Undefined" => "undefined".to_string(),
            "Null" => "null".to_string(),
            "Object" => "object".to_string(),
            other => other.to_string(),
        }
    }

    pub(in crate::declaration_emitter) fn parse_jsdoc_type_alias_decl(
        jsdoc: &str,
    ) -> Option<JsdocTypeAliasDecl> {
        let type_params = Self::parse_jsdoc_template_params(jsdoc);
        let description_lines = Self::jsdoc_description_lines(jsdoc);

        if let Some((name, type_text)) = Self::parse_jsdoc_typedef_alias(jsdoc) {
            if name == "default" {
                return None;
            }
            if Self::jsdoc_has_property_tags(jsdoc) {
                let (name, type_text) = Self::parse_jsdoc_property_type_alias(jsdoc)?;
                return Some(JsdocTypeAliasDecl {
                    name,
                    type_params,
                    type_text,
                    description_lines: Vec::new(),
                    render_verbatim: true,
                });
            }
            return Some(JsdocTypeAliasDecl {
                name,
                type_params,
                type_text,
                description_lines: Vec::new(),
                render_verbatim: false,
            });
        }

        if let Some((name, type_text)) = Self::parse_jsdoc_callback_alias(jsdoc) {
            return Some(JsdocTypeAliasDecl {
                name,
                type_params,
                type_text,
                description_lines,
                render_verbatim: false,
            });
        }

        None
    }

    pub(in crate::declaration_emitter) fn render_jsdoc_type_alias_decl(
        decl: &JsdocTypeAliasDecl,
        exported: bool,
    ) -> Option<String> {
        let mut source = String::new();
        if !decl.description_lines.is_empty() {
            source.push_str("/**\n");
            for line in &decl.description_lines {
                source.push_str(" * ");
                source.push_str(line);
                source.push('\n');
            }
            source.push_str(" */\n");
        }
        source.push_str(if exported { "export type " } else { "type " });
        source.push_str(&decl.name);
        if !decl.type_params.is_empty() {
            source.push('<');
            source.push_str(&decl.type_params.join(", "));
            source.push('>');
        }
        source.push_str(" = ");
        source.push_str(&decl.type_text);
        source.push_str(";\n");

        if decl.render_verbatim {
            return Some(source);
        }

        let mut parser = ParserState::new("jsdoc-alias.ts".to_string(), source);
        let root = parser.parse_source_file();
        let mut emitter = DeclarationEmitter::new(&parser.arena);
        let rendered = emitter.emit(root);
        if rendered.trim().is_empty() {
            None
        } else {
            Some(rendered)
        }
    }

    pub(in crate::declaration_emitter) fn emit_rendered_jsdoc_type_alias(
        &mut self,
        decl: JsdocTypeAliasDecl,
        exported: bool,
    ) {
        if !self.emitted_jsdoc_type_aliases.insert(decl.name.clone()) {
            return;
        }
        let Some(rendered) = Self::render_jsdoc_type_alias_decl(&decl, exported) else {
            return;
        };
        self.write(&rendered);
        if exported {
            self.emitted_module_indicator = true;
        }
    }

    pub(crate) fn emit_leading_jsdoc_type_aliases_for_pos(&mut self, pos: u32) {
        if !self.source_is_js_file {
            return;
        }
        for jsdoc in self.leading_jsdoc_comment_chain_for_pos(pos) {
            if let Some(decl) = Self::parse_jsdoc_type_alias_decl(&jsdoc) {
                self.emit_rendered_jsdoc_type_alias(decl, true);
            }
        }
    }

    pub(crate) fn emit_jsdoc_callback_type_aliases_for_variable_statement(
        &mut self,
        stmt_idx: NodeIndex,
        force_exported: bool,
    ) {
        if !self.source_is_js_file {
            return;
        }
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
            return;
        };

        let callback_chain = self.leading_jsdoc_comment_chain_for_pos(stmt_node.pos);
        if callback_chain.is_empty() {
            return;
        }

        let callback_aliases = callback_chain
            .iter()
            .filter_map(|jsdoc| Self::parse_jsdoc_callback_alias(jsdoc))
            .collect::<FxHashMap<_, _>>();
        if callback_aliases.is_empty() {
            return;
        }

        let has_export_modifier = self
            .arena
            .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword);

        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            if decl_list_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                continue;
            }
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };

            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                let is_exported = force_exported
                    || has_export_modifier
                    || self.is_js_named_exported_name(decl.name);
                if !is_exported {
                    continue;
                }

                let Some(type_name) = self
                    .jsdoc_name_like_type_expr_for_pos(stmt_node.pos)
                    .or_else(|| self.jsdoc_name_like_type_expr_for_node(decl_idx))
                    .or_else(|| self.jsdoc_name_like_type_expr_for_node(decl.name))
                else {
                    continue;
                };

                let Some(type_text) = callback_aliases.get(&type_name) else {
                    continue;
                };
                if !self.emitted_jsdoc_type_aliases.insert(type_name.clone()) {
                    continue;
                }

                self.write_indent();
                self.write("export type ");
                self.write(&type_name);
                self.write(" = ");
                self.write(type_text);
                self.write(";");
                self.write_line();
            }
        }
    }

    pub(crate) fn emit_pending_jsdoc_callback_type_aliases(
        &mut self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) {
        if !self.source_is_js_file {
            return;
        }

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };

            match stmt_node.kind {
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                    self.emit_jsdoc_callback_type_aliases_for_variable_statement(stmt_idx, false);
                }
                k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                    let Some(export) = self.arena.get_export_decl(stmt_node) else {
                        continue;
                    };
                    let Some(clause_node) = self.arena.get(export.export_clause) else {
                        continue;
                    };
                    if clause_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                        self.emit_jsdoc_callback_type_aliases_for_variable_statement(
                            export.export_clause,
                            true,
                        );
                    }
                }
                _ => {}
            }
        }
    }

    pub(crate) fn emit_trailing_top_level_jsdoc_type_aliases(
        &mut self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) {
        if !self.source_is_js_file {
            return;
        }

        let Ok(eof_pos) = u32::try_from(source_file.text.len()) else {
            return;
        };

        for jsdoc in self.leading_jsdoc_comment_chain_for_pos(eof_pos) {
            if let Some(decl) = Self::parse_jsdoc_type_alias_decl(&jsdoc) {
                self.emit_rendered_jsdoc_type_alias(decl, true);
            }
        }
    }

    pub(crate) fn emit_pending_top_level_jsdoc_type_aliases(
        &mut self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) {
        if !self.source_is_js_file || self.source_file_has_module_syntax(source_file) {
            return;
        }

        let mut decls = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            for jsdoc in self.leading_jsdoc_comment_chain_for_pos(stmt_node.pos) {
                if let Some(decl) = Self::parse_jsdoc_type_alias_decl(&jsdoc) {
                    decls.push(decl);
                }
            }
        }

        let Ok(eof_pos) = u32::try_from(source_file.text.len()) else {
            return;
        };
        for jsdoc in self.leading_jsdoc_comment_chain_for_pos(eof_pos) {
            if let Some(decl) = Self::parse_jsdoc_type_alias_decl(&jsdoc) {
                decls.push(decl);
            }
        }

        for decl in decls {
            self.emit_rendered_jsdoc_type_alias(decl, false);
        }
    }
}
