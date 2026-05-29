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

use super::jsdoc_function_signature::{
    JsdocFunctionTypeSignature, parse_jsdoc_function_type_signature,
};
use super::{
    JsdocOverloadSignature, JsdocParamDecl, JsdocTypeAliasDecl, escape_string_for_double_quote,
};

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
        let Some(trimmed) = Self::trim_jsdoc_attach_trivia(between) else {
            return false;
        };
        if trimmed.is_empty() {
            return true;
        }

        trimmed.split_whitespace().all(|word| {
            matches!(
                word,
                "export"
                    | "declare"
                    | "default"
                    | "const"
                    | "let"
                    | "var"
                    | "using"
                    | "await"
                    | "class"
                    | "function"
                    | "interface"
                    | "enum"
                    | "namespace"
                    | "module"
                    | "abstract"
                    | "async"
            )
        })
    }

    fn trim_jsdoc_attach_trivia(mut text: &str) -> Option<&str> {
        loop {
            let trimmed = text.trim_start_matches(char::is_whitespace);
            if let Some(rest) = trimmed.strip_prefix("//") {
                let Some(line_end) = rest.find('\n') else {
                    return Some("");
                };
                text = &rest[line_end + 1..];
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("/*") {
                let comment_end = rest.find("*/")?;
                text = &rest[comment_end + 2..];
                continue;
            }
            return Some(trimmed.trim_end());
        }
    }

    pub(in crate::declaration_emitter) fn extract_jsdoc_type_expression(
        jsdoc: &str,
    ) -> Option<&str> {
        let typedef_pos = jsdoc.find("@typedef");
        let mut tag_pos = jsdoc.find("@type");

        while let Some(pos) = tag_pos {
            let next_char = jsdoc[pos + "@type".len()..].chars().next();
            if !next_char.is_some_and(Self::is_jsdoc_tag_name_continuation) {
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

        if Self::jsdoc_generic_name_like_type_reference(expr) {
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

    fn jsdoc_generic_name_like_type_reference(expr: &str) -> bool {
        let Some(open) = expr.find('<') else {
            return false;
        };
        if !expr.ends_with('>') {
            return false;
        }

        let base = expr[..open].trim();
        if base.is_empty()
            || base.ends_with('.')
            || !base
                .chars()
                .all(|ch| ch == '_' || ch == '$' || ch == '.' || ch.is_ascii_alphanumeric())
        {
            return false;
        }

        let args = expr[open + 1..expr.len() - 1].trim();
        !args.is_empty()
            && args.chars().all(|ch| {
                ch == '_'
                    || ch == '$'
                    || ch == '.'
                    || ch == ','
                    || ch == '<'
                    || ch == '>'
                    || ch == '['
                    || ch == ']'
                    || ch == ' '
                    || ch == '\t'
                    || ch == '\''
                    || ch == '"'
                    || ch.is_ascii_alphanumeric()
            })
            && Self::jsdoc_angle_brackets_are_balanced(args)
    }

    fn jsdoc_angle_brackets_are_balanced(text: &str) -> bool {
        let mut depth = 0usize;
        for ch in text.chars() {
            match ch {
                '<' => depth += 1,
                '>' => {
                    if depth == 0 {
                        return false;
                    }
                    depth -= 1;
                }
                _ => {}
            }
        }
        depth == 0
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

    pub(in crate::declaration_emitter) fn jsdoc_preserves_js_var_keyword<I>(
        &self,
        stmt_pos: u32,
        decls: I,
    ) -> bool
    where
        I: IntoIterator<Item = (NodeIndex, NodeIndex)>,
    {
        !self
            .leading_jsdoc_comment_chain_for_pos(stmt_pos)
            .is_empty()
            || self.jsdoc_name_like_type_expr_for_pos(stmt_pos).is_some()
            || decls.into_iter().any(|(decl_idx, decl_name)| {
                self.jsdoc_name_like_type_expr_for_node(decl_idx).is_some()
                    || self.jsdoc_name_like_type_expr_for_node(decl_name).is_some()
            })
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

    pub(crate) fn nearest_jsdoc_comment_for_pos_relaxed(&self, pos: u32) -> Option<String> {
        let text = self.source_file_text.as_deref()?;
        let bytes = text.as_bytes();
        let mut actual_start = pos as usize;
        while actual_start < bytes.len()
            && matches!(bytes[actual_start], b' ' | b'\t' | b'\r' | b'\n')
        {
            actual_start += 1;
        }

        self.all_comments
            .iter()
            .filter(|comment| comment.end as usize <= actual_start)
            .filter(|comment| is_jsdoc_comment(comment, text))
            .max_by_key(|comment| comment.end)
            .map(|comment| get_jsdoc_content(comment, text))
    }

    pub(crate) fn emittable_jsdoc_comment_chain_for_pos(&self, pos: u32) -> Vec<String> {
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

        let mut chain = Vec::new();
        let mut idx = self.comment_emit_idx;
        while idx < self.all_comments.len() {
            let comment = &self.all_comments[idx];
            if comment.end as usize > actual_start {
                break;
            }
            let raw = &text[comment.pos as usize..comment.end as usize];
            if raw.starts_with("/**") && raw != "/**/" {
                chain.push(get_jsdoc_content(comment, text));
            }
            idx += 1;
        }
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

    const fn is_jsdoc_tag_name_continuation(ch: char) -> bool {
        ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
    }

    pub(in crate::declaration_emitter) fn normalize_jsdoc_type_text(
        type_expr: &str,
        rest: bool,
    ) -> String {
        let normalized = Self::normalize_jsdoc_type_expr(type_expr);
        if rest {
            format!("{normalized}[]")
        } else {
            normalized
        }
    }

    pub(in crate::declaration_emitter) fn jsdoc_type_text_for_declaration_emit(
        &self,
        type_text: &str,
    ) -> String {
        let normalized = if Self::jsdoc_name_like_type_reference(type_text) {
            Self::normalize_jsdoc_type_expr(type_text)
        } else {
            type_text.to_string()
        };
        let portable = self.qualify_jsdoc_typeof_self_exports(&normalized);
        let portable = self.rewrite_jsdoc_typeof_module_exports_surfaces(&portable);
        let portable = self.rewrite_ambient_module_relative_import_type_text(&portable);
        let portable = self.rewrite_jsdoc_bare_module_import_type_text(&portable);
        Self::format_jsdoc_declaration_type_text(&portable)
    }

    fn jsdoc_type_alias_text_for_declaration_emit(&self, type_text: &str) -> String {
        let portable = self.rewrite_ambient_module_relative_import_type_text(type_text);
        let portable = self.rewrite_jsdoc_bare_module_import_type_text(&portable);
        Self::format_jsdoc_declaration_type_text(&portable)
    }

    fn rewrite_ambient_module_relative_import_type_text(&self, type_text: &str) -> String {
        let Some(current_module) = self.current_ambient_module_specifier.as_deref() else {
            return type_text.to_string();
        };
        if !type_text.contains("import(") {
            return type_text.to_string();
        }

        let mut output = String::new();
        let mut remaining = type_text;
        while let Some((start, module_specifier, tail)) = Self::next_import_type_text(remaining) {
            output.push_str(&remaining[..start]);
            let import_len = remaining.len() - tail.len() - start;
            if module_specifier.starts_with('.') {
                let rewritten = Self::resolve_ambient_module_relative_specifier(
                    current_module,
                    &module_specifier,
                );
                output.push_str("import(\"");
                output.push_str(&rewritten);
                output.push_str("\")");
            } else {
                output.push_str(&remaining[start..start + import_len]);
            }
            remaining = tail;
        }
        output.push_str(remaining);
        output
    }

    fn rewrite_jsdoc_typeof_module_exports_surfaces(&self, type_text: &str) -> String {
        const NEEDLE: &str = "typeof module.exports.";
        if !self.source_is_js_file || !type_text.contains(NEEDLE) {
            return type_text.to_string();
        }

        let mut output = String::new();
        let mut cursor = 0usize;
        while let Some(relative) = type_text[cursor..].find(NEEDLE) {
            let start = cursor + relative;
            let name_start = start + NEEDLE.len();
            let export_name: String = type_text[name_start..]
                .chars()
                .take_while(|ch| Self::is_jsdoc_identifier_part(*ch))
                .collect();
            if export_name.is_empty() {
                output.push_str(&type_text[cursor..name_start]);
                cursor = name_start;
                continue;
            }

            let end = name_start + export_name.len();
            output.push_str(&type_text[cursor..start]);
            if let Some(surface) = self.jsdoc_same_file_commonjs_named_export_surface(&export_name)
            {
                output.push_str(&surface);
            } else {
                output.push_str(&type_text[start..end]);
            }
            cursor = end;
        }
        output.push_str(&type_text[cursor..]);
        output
    }

    fn resolve_ambient_module_relative_specifier(
        current_module: &str,
        module_specifier: &str,
    ) -> String {
        let mut parts: Vec<&str> = current_module.split('/').collect();
        parts.pop();

        for part in module_specifier.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                _ => parts.push(part),
            }
        }

        parts.join("/")
    }

    fn rewrite_jsdoc_bare_module_import_type_text(&self, type_text: &str) -> String {
        if !type_text.contains("import(") {
            return type_text.to_string();
        }

        let mut output = String::new();
        let mut remaining = type_text;
        while let Some((start, module_specifier, tail)) = Self::next_import_type_text(remaining) {
            output.push_str(&remaining[..start]);
            let import_len = remaining.len() - tail.len() - start;
            let import_text = &remaining[start..start + import_len];
            if Self::jsdoc_import_type_tail_is_bare(tail) {
                if let Some(surface) =
                    self.jsdoc_bare_module_import_export_surface(&module_specifier)
                {
                    output.push_str(&surface);
                } else {
                    output.push_str(import_text);
                }
            } else {
                output.push_str(import_text);
            }
            remaining = tail;
        }
        output.push_str(remaining);
        output
    }

    fn jsdoc_import_type_tail_is_bare(tail: &str) -> bool {
        tail.trim_start()
            .chars()
            .next()
            .is_none_or(|ch| matches!(ch, ',' | ')' | '>' | '|' | '&' | ';' | ':' | '}'))
    }

    fn jsdoc_bare_module_import_export_surface(&self, module_specifier: &str) -> Option<String> {
        let binder = self.binder?;
        let current_path = self.current_file_path.as_deref()?;
        if let Some(surface) = self
            .matching_module_export_paths(binder, current_path, module_specifier)
            .into_iter()
            .find_map(|module_path| {
                let exports = binder.module_exports.get(module_path)?;
                let root_sym_id = exports.get("export=")?;
                self.jsdoc_export_equals_symbol_surface(root_sym_id, exports)
            })
        {
            return Some(surface);
        }

        self.jsdoc_bare_module_import_source_surface(current_path, module_specifier)
    }

    fn jsdoc_bare_module_import_source_surface(
        &self,
        current_path: &str,
        module_specifier: &str,
    ) -> Option<String> {
        self.arena_to_path
            .iter()
            .filter_map(|(&arena_addr, source_path)| {
                let relative = self
                    .strip_ts_extensions(&self.calculate_relative_path(current_path, source_path));
                if relative != module_specifier
                    && relative
                        .strip_suffix("/index")
                        .is_none_or(|without_index| without_index != module_specifier)
                {
                    return None;
                }
                let arena = self
                    .global_symbol_arenas
                    .values()
                    .find(|arena| Arc::as_ptr(arena) as usize == arena_addr)?;
                self.jsdoc_commonjs_source_export_surface(arena.as_ref())
            })
            .next()
    }

    fn jsdoc_commonjs_source_export_surface(&self, source_arena: &NodeArena) -> Option<String> {
        let source_file = self.arena_source_file(source_arena)?;
        let root_name = self.jsdoc_commonjs_source_export_equals_name(source_arena, source_file)?;
        let root_call = self.jsdoc_source_callable_symbol_surface(source_arena, &root_name)?;
        let mut members =
            self.jsdoc_commonjs_source_static_members(source_arena, source_file, &root_name)?;
        members.sort_by(|(left, _), (right, _)| left.cmp(right));

        let mut surface = String::from("{\n");
        surface.push_str("    ");
        surface.push_str(&root_call);
        surface.push('\n');
        for (name, type_text) in members {
            surface.push_str("    ");
            surface.push_str(&name);
            surface.push_str(": ");
            surface.push_str(&Self::indent_jsdoc_inline_member_type(&type_text));
            surface.push_str(";\n");
        }
        surface.push('}');
        Some(surface)
    }

    fn jsdoc_same_file_commonjs_named_export_surface(&self, export_name: &str) -> Option<String> {
        let source_file_idx = self.current_source_file_idx?;
        let source_file_node = self.arena.get(source_file_idx)?;
        let source_file = self.arena.get_source_file(source_file_node)?;

        let mut call_signature = None;
        for &stmt_idx in &source_file.statements.nodes {
            let Some((name_idx, initializer)) =
                self.js_commonjs_named_export_for_statement(stmt_idx)
            else {
                continue;
            };
            if self.js_commonjs_export_name_text(name_idx).as_deref() != Some(export_name) {
                continue;
            }
            let initializer = self
                .arena
                .skip_parenthesized_and_assertions_and_comma(initializer);
            let init_node = self.arena.get(initializer)?;
            if !self.is_js_function_initializer(initializer) {
                return None;
            }
            let func = self.arena.get_function(init_node)?;
            call_signature = self.jsdoc_same_file_function_call_signature_text(initializer, func);
            break;
        }
        let call_signature = call_signature?;

        let mut members = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            let Some((member_name, initializer)) =
                self.jsdoc_same_file_commonjs_named_export_member(stmt_idx, export_name)
            else {
                continue;
            };
            if !Self::is_unquoted_property_name(&member_name) {
                return None;
            }
            let type_text = self.jsdoc_same_file_commonjs_export_member_type_text(initializer)?;
            members.push((member_name, type_text));
        }

        let mut surface = String::from("{\n");
        surface.push_str("    ");
        surface.push_str(&call_signature);
        surface.push('\n');
        for (name, type_text) in members {
            surface.push_str("    ");
            surface.push_str(&name);
            surface.push_str(": ");
            surface.push_str(&Self::indent_jsdoc_inline_member_type(&type_text));
            if !type_text.trim_end().ends_with(';') {
                surface.push(';');
            }
            surface.push('\n');
        }
        surface.push('}');
        Some(surface)
    }

    fn jsdoc_same_file_function_call_signature_text(
        &self,
        initializer: NodeIndex,
        func: &tsz_parser::parser::node::FunctionData,
    ) -> Option<String> {
        let jsdoc = self.function_like_jsdoc_for_node(initializer);
        let jsdoc_params = jsdoc
            .as_deref()
            .map(Self::parse_jsdoc_param_decls)
            .unwrap_or_default();
        let mut params = Vec::with_capacity(func.parameters.nodes.len());
        for (idx, &param_idx) in func.parameters.nodes.iter().enumerate() {
            let param_node = self.arena.get(param_idx)?;
            let param = self.arena.get_parameter(param_node)?;
            let name = self
                .get_identifier_text(param.name)
                .unwrap_or_else(|| "arg".to_string());
            let rest =
                if param.dot_dot_dot_token || jsdoc_params.get(idx).is_some_and(|decl| decl.rest) {
                    "..."
                } else {
                    ""
                };
            let optional = if param.question_token
                || jsdoc_params
                    .get(idx)
                    .is_some_and(|decl| decl.optional && !decl.rest)
            {
                "?"
            } else {
                ""
            };
            let type_text = if param.type_annotation.is_some() {
                self.emit_type_node_text_from_arena(self.arena, param.type_annotation)
                    .or_else(|| self.source_slice_from_arena(self.arena, param.type_annotation))
                    .map(|text| text.trim().to_string())
                    .filter(|text| !text.is_empty())
                    .unwrap_or_else(|| "any".to_string())
            } else if let Some(jsdoc_param) = jsdoc_params.get(idx) {
                self.jsdoc_type_text_for_declaration_emit(&jsdoc_param.type_text)
            } else {
                "any".to_string()
            };
            params.push(format!("{rest}{name}{optional}: {type_text}"));
        }

        let return_type = if func.type_annotation.is_some() {
            self.emit_type_node_text_from_arena(self.arena, func.type_annotation)
                .or_else(|| self.source_slice_from_arena(self.arena, func.type_annotation))
                .map(|text| text.trim().to_string())
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| "any".to_string())
        } else if let Some(return_type) = jsdoc
            .as_deref()
            .and_then(Self::parse_jsdoc_return_type_text)
        {
            self.jsdoc_type_text_for_declaration_emit(&return_type)
        } else if let (Some(interner), Some(cache)) = (&self.type_interner, &self.type_cache)
            && let Some(func_type_id) = cache.node_types.get(&initializer.0).copied()
            && let Some(return_type_id) =
                tsz_solver::type_queries::get_return_type(*interner, func_type_id)
        {
            if return_type_id == tsz_solver::types::TypeId::ANY
                && func.body.is_some()
                && self.body_returns_void(func.body)
            {
                "void".to_string()
            } else {
                self.print_type_id(return_type_id)
            }
        } else if func.body.is_some() && self.body_returns_void(func.body) {
            "void".to_string()
        } else {
            "any".to_string()
        };

        Some(format!("({}): {return_type};", params.join(", ")))
    }

    fn jsdoc_same_file_commonjs_named_export_member(
        &self,
        stmt_idx: NodeIndex,
        export_name: &str,
    ) -> Option<(String, NodeIndex)> {
        let stmt_node = self.arena.get(stmt_idx)?;
        if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
            return None;
        }
        let expr_stmt = self.arena.get_expression_statement(stmt_node)?;
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_stmt.expression);
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let binary = self.arena.get_binary_expr(expr_node)?;
        if binary.operator_token != SyntaxKind::EqualsToken as u16 {
            return None;
        }
        let lhs = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(binary.left);
        let lhs_node = self.arena.get(lhs)?;
        if lhs_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let lhs_access = self.arena.get_access_expr(lhs_node)?;
        let member_name = self.get_identifier_text(lhs_access.name_or_argument)?;
        let receiver = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(lhs_access.expression);
        let receiver_node = self.arena.get(receiver)?;
        if receiver_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let receiver_access = self.arena.get_access_expr(receiver_node)?;
        if self
            .get_identifier_text(receiver_access.name_or_argument)
            .as_deref()
            != Some(export_name)
            || !self.is_module_exports_reference(receiver_access.expression)
        {
            return None;
        }
        let initializer = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(binary.right);
        Some((member_name, initializer))
    }

    fn jsdoc_same_file_commonjs_export_member_type_text(
        &self,
        initializer: NodeIndex,
    ) -> Option<String> {
        let init_node = self.arena.get(initializer)?;
        if self.is_js_function_initializer(initializer) {
            let func = self.arena.get_function(init_node)?;
            let signature = self.jsdoc_same_file_function_call_signature_text(initializer, func)?;
            let signature = signature.trim_end_matches(';');
            if let Some((params, return_type)) = signature.split_once(": ") {
                return Some(format!("{params} => {return_type}"));
            }
        }
        self.js_namespace_value_member_type_text(initializer)
            .or_else(|| self.js_synthetic_export_value_type_text(initializer))
    }

    fn jsdoc_commonjs_source_export_equals_name(
        &self,
        source_arena: &NodeArena,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> Option<String> {
        for &stmt_idx in &source_file.statements.nodes {
            let stmt_node = source_arena.get(stmt_idx)?;
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let expr_stmt = source_arena.get_expression_statement(stmt_node)?;
            let expr_idx =
                source_arena.skip_parenthesized_and_assertions_and_comma(expr_stmt.expression);
            let expr_node = source_arena.get(expr_idx)?;
            if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let binary = source_arena.get_binary_expr(expr_node)?;
            if binary.operator_token != SyntaxKind::EqualsToken as u16
                || !Self::jsdoc_source_is_module_exports_reference(source_arena, binary.left)
            {
                continue;
            }
            let rhs = source_arena.skip_parenthesized_and_assertions_and_comma(binary.right);
            if let Some(name) = self.identifier_text_from_arena(source_arena, rhs) {
                return Some(name);
            }
        }
        None
    }

    fn jsdoc_commonjs_source_static_members(
        &self,
        source_arena: &NodeArena,
        source_file: &tsz_parser::parser::node::SourceFileData,
        root_name: &str,
    ) -> Option<Vec<(String, String)>> {
        let mut members = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            let stmt_node = source_arena.get(stmt_idx)?;
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let expr_stmt = source_arena.get_expression_statement(stmt_node)?;
            let expr_idx =
                source_arena.skip_parenthesized_and_assertions_and_comma(expr_stmt.expression);
            let expr_node = source_arena.get(expr_idx)?;
            if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let binary = source_arena.get_binary_expr(expr_node)?;
            if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                continue;
            }
            let Some((receiver_name, member_name)) =
                self.jsdoc_source_property_assignment_name(source_arena, binary.left)
            else {
                continue;
            };
            if receiver_name != root_name {
                continue;
            }
            if !Self::is_unquoted_property_name(&member_name) {
                return None;
            }
            let rhs = source_arena.skip_parenthesized_and_assertions_and_comma(binary.right);
            let rhs_name = self.identifier_text_from_arena(source_arena, rhs)?;
            let type_text =
                self.jsdoc_construct_signature_surface_for_source_name(source_arena, &rhs_name)?;
            members.push((member_name, type_text));
        }
        Some(members)
    }

    fn jsdoc_source_property_assignment_name(
        &self,
        source_arena: &NodeArena,
        lhs: NodeIndex,
    ) -> Option<(String, String)> {
        let lhs = source_arena.skip_parenthesized_and_assertions_and_comma(lhs);
        let lhs_node = source_arena.get(lhs)?;
        if lhs_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = source_arena.get_access_expr(lhs_node)?;
        let receiver = source_arena.skip_parenthesized_and_assertions_and_comma(access.expression);
        let receiver_name = self.identifier_text_from_arena(source_arena, receiver)?;
        let member_name = self.identifier_text_from_arena(source_arena, access.name_or_argument)?;
        Some((receiver_name, member_name))
    }

    fn jsdoc_source_is_module_exports_reference(source_arena: &NodeArena, idx: NodeIndex) -> bool {
        let idx = source_arena.skip_parenthesized_and_assertions_and_comma(idx);
        let Some(node) = source_arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = source_arena.get_access_expr(node) else {
            return false;
        };
        let Some(receiver_node) = source_arena.get(access.expression) else {
            return false;
        };
        let Some(member_node) = source_arena.get(access.name_or_argument) else {
            return false;
        };
        source_arena
            .get_identifier(receiver_node)
            .is_some_and(|ident| ident.escaped_text == "module")
            && source_arena
                .get_identifier(member_node)
                .is_some_and(|ident| ident.escaped_text == "exports")
    }

    fn jsdoc_export_equals_symbol_surface(
        &self,
        root_sym_id: SymbolId,
        exports: &tsz_binder::SymbolTable,
    ) -> Option<String> {
        let root_call = self.jsdoc_export_equals_call_signature_text(root_sym_id)?;
        let mut members = Vec::new();
        for (name, &sym_id) in exports.iter() {
            if name == "export=" || name == "default" {
                continue;
            }
            if !Self::is_unquoted_property_name(name) {
                return None;
            }
            let type_text = self.jsdoc_exported_static_member_type_text(sym_id)?;
            members.push((name.clone(), type_text));
        }
        members.sort_by(|(left, _), (right, _)| left.cmp(right));

        let mut surface = String::from("{\n");
        surface.push_str("    ");
        surface.push_str(&root_call);
        surface.push('\n');
        for (name, type_text) in members {
            surface.push_str("    ");
            surface.push_str(&name);
            surface.push_str(": ");
            surface.push_str(&Self::indent_jsdoc_inline_member_type(&type_text));
            surface.push_str(";\n");
        }
        surface.push('}');
        Some(surface)
    }

    fn jsdoc_export_equals_call_signature_text(&self, sym_id: SymbolId) -> Option<String> {
        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let decl_node = source_arena.get(decl_idx)?;
            let callable = Self::callable_decl_parts_from_node(source_arena, decl_node)?;
            let params = self.jsdoc_parameter_signature_texts(source_arena, callable.parameters)?;
            let return_type = if callable.type_annotation.is_some() {
                self.emit_type_node_text_from_arena(source_arena, callable.type_annotation)
                    .or_else(|| {
                        self.source_slice_from_arena(source_arena, callable.type_annotation)
                    })
                    .map(|text| text.trim().to_string())
                    .filter(|text| !text.is_empty())
                    .unwrap_or_else(|| "any".to_string())
            } else {
                "{}".to_string()
            };
            Some(format!("({}): {return_type};", params.join(", ")))
        })
    }

    fn jsdoc_source_callable_symbol_surface(
        &self,
        source_arena: &NodeArena,
        name: &str,
    ) -> Option<String> {
        let source_file = self.arena_source_file(source_arena)?;
        source_file.statements.nodes.iter().find_map(|&stmt_idx| {
            let stmt_node = source_arena.get(stmt_idx)?;
            if stmt_node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
                let func = source_arena.get_function(stmt_node)?;
                if self
                    .identifier_text_from_arena(source_arena, func.name)
                    .as_deref()
                    == Some(name)
                {
                    return self.jsdoc_callable_parts_signature_text(
                        source_arena,
                        &func.parameters,
                        func.type_annotation,
                    );
                }
            }

            let var_stmt = source_arena.get_variable(stmt_node)?;
            for &decl_list_idx in &var_stmt.declarations.nodes {
                let decl_list_node = source_arena.get(decl_list_idx)?;
                let decl_list = source_arena.get_variable(decl_list_node)?;
                for &decl_idx in &decl_list.declarations.nodes {
                    let decl_node = source_arena.get(decl_idx)?;
                    let decl = source_arena.get_variable_declaration(decl_node)?;
                    if self
                        .identifier_text_from_arena(source_arena, decl.name)
                        .as_deref()
                        != Some(name)
                    {
                        continue;
                    }
                    let initializer =
                        source_arena.skip_parenthesized_and_assertions_and_comma(decl.initializer);
                    let init_node = source_arena.get(initializer)?;
                    if init_node.kind != syntax_kind_ext::ARROW_FUNCTION
                        && init_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
                    {
                        return None;
                    }
                    let func = source_arena.get_function(init_node)?;
                    return self.jsdoc_callable_parts_signature_text(
                        source_arena,
                        &func.parameters,
                        func.type_annotation,
                    );
                }
            }
            None
        })
    }

    fn jsdoc_callable_parts_signature_text(
        &self,
        source_arena: &NodeArena,
        parameters: &NodeList,
        type_annotation: NodeIndex,
    ) -> Option<String> {
        let params = self.jsdoc_parameter_signature_texts(source_arena, parameters)?;
        let return_type = if type_annotation.is_some() {
            self.emit_type_node_text_from_arena(source_arena, type_annotation)
                .or_else(|| self.source_slice_from_arena(source_arena, type_annotation))
                .map(|text| text.trim().to_string())
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| "any".to_string())
        } else {
            "{}".to_string()
        };
        Some(format!("({}): {return_type};", params.join(", ")))
    }

    fn jsdoc_parameter_signature_texts(
        &self,
        source_arena: &NodeArena,
        params: &NodeList,
    ) -> Option<Vec<String>> {
        let mut texts = Vec::with_capacity(params.nodes.len());
        for &param_idx in &params.nodes {
            let param_node = source_arena.get(param_idx)?;
            let param = source_arena.get_parameter(param_node)?;
            let name = self
                .identifier_text_from_arena(source_arena, param.name)
                .unwrap_or_else(|| "arg".to_string());
            let type_text = if param.type_annotation.is_some() {
                self.emit_type_node_text_from_arena(source_arena, param.type_annotation)
                    .or_else(|| self.source_slice_from_arena(source_arena, param.type_annotation))
                    .map(|text| text.trim().to_string())
                    .filter(|text| !text.is_empty())
                    .unwrap_or_else(|| "any".to_string())
            } else {
                "any".to_string()
            };
            let rest = if param.dot_dot_dot_token { "..." } else { "" };
            let optional = if param.question_token { "?" } else { "" };
            texts.push(format!("{rest}{name}{optional}: {type_text}"));
        }
        Some(texts)
    }

    fn jsdoc_exported_static_member_type_text(&self, sym_id: SymbolId) -> Option<String> {
        if let Some(type_text) = self.jsdoc_construct_signature_surface_for_symbol(sym_id) {
            return Some(type_text);
        }

        let cache = self.type_cache.as_ref()?;
        let type_id = cache.symbol_types.get(&sym_id).copied()?;
        Some(self.print_type_id(type_id))
    }

    fn jsdoc_construct_signature_surface_for_symbol(&self, sym_id: SymbolId) -> Option<String> {
        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let decl_node = source_arena.get(decl_idx)?;
            self.jsdoc_construct_signature_surface_from_class_node(source_arena, decl_node)
        })
    }

    fn jsdoc_construct_signature_surface_for_source_name(
        &self,
        source_arena: &NodeArena,
        name: &str,
    ) -> Option<String> {
        let source_file = self.arena_source_file(source_arena)?;
        source_file.statements.nodes.iter().find_map(|&stmt_idx| {
            let stmt_node = source_arena.get(stmt_idx)?;
            if stmt_node.kind != syntax_kind_ext::CLASS_DECLARATION {
                return None;
            }
            let class = source_arena.get_class(stmt_node)?;
            (self
                .identifier_text_from_arena(source_arena, class.name)
                .as_deref()
                == Some(name))
            .then(|| {
                self.jsdoc_construct_signature_surface_from_class_node(source_arena, stmt_node)
            })
            .flatten()
        })
    }

    fn jsdoc_construct_signature_surface_from_class_node(
        &self,
        source_arena: &NodeArena,
        class_node: &Node,
    ) -> Option<String> {
        let class = source_arena.get_class(class_node)?;
        let constructor_params = class
            .members
            .nodes
            .iter()
            .find_map(|&member_idx| {
                let member_node = source_arena.get(member_idx)?;
                (member_node.kind == syntax_kind_ext::CONSTRUCTOR).then_some(member_node)?;
                let ctor = source_arena.get_constructor(member_node)?;
                self.jsdoc_parameter_signature_texts(source_arena, &ctor.parameters)
            })
            .unwrap_or_default();
        Some(format!(
            "{{\n    new ({}): {{}};\n}}",
            constructor_params.join(", ")
        ))
    }

    fn indent_jsdoc_inline_member_type(type_text: &str) -> String {
        if !type_text.contains('\n') {
            return type_text.to_string();
        }
        let mut lines = type_text.lines();
        let Some(first) = lines.next() else {
            return type_text.to_string();
        };
        let mut output = first.to_string();
        for line in lines {
            output.push('\n');
            output.push_str("    ");
            output.push_str(line);
        }
        output
    }

    fn qualify_jsdoc_typeof_self_exports(&self, type_text: &str) -> String {
        if !self.source_is_js_file
            || !type_text.contains("typeof ")
            || type_text.contains("typeof import(")
        {
            return type_text.to_string();
        }

        let mut result = type_text.to_string();
        let mut export_names = self.top_level_self_exported_names();
        export_names.extend(self.js_named_export_names.iter().cloned());
        if export_names.is_empty() {
            return result;
        }

        let mut names = export_names.iter().collect::<Vec<_>>();
        names.sort_by_key(|name| std::cmp::Reverse(name.len()));
        for name in names {
            let needle = format!("typeof {name}");
            let replacement = format!("typeof import(\".\").{name}");
            result = Self::replace_jsdoc_typeof_identifier(&result, &needle, &replacement);
        }
        result
    }

    fn replace_jsdoc_typeof_identifier(text: &str, needle: &str, replacement: &str) -> String {
        let mut result = String::new();
        let mut cursor = 0usize;
        while let Some(relative) = text[cursor..].find(needle) {
            let start = cursor + relative;
            let end = start + needle.len();
            let before_ok = start == 0
                || !text[..start]
                    .chars()
                    .next_back()
                    .is_some_and(Self::is_jsdoc_identifier_part);
            let after_ok = end == text.len()
                || !text[end..]
                    .chars()
                    .next()
                    .is_some_and(Self::is_jsdoc_identifier_part);
            result.push_str(&text[cursor..start]);
            if before_ok && after_ok {
                result.push_str(replacement);
            } else {
                result.push_str(&text[start..end]);
            }
            cursor = end;
        }
        result.push_str(&text[cursor..]);
        result
    }

    const fn is_jsdoc_identifier_part(ch: char) -> bool {
        ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()
    }

    fn format_jsdoc_object_type_text(type_text: &str) -> Option<String> {
        let trimmed = type_text.trim();
        let inner = trimmed.strip_prefix('{')?.strip_suffix('}')?.trim();
        if inner.is_empty() {
            return None;
        }

        let members = Self::split_jsdoc_params(inner);
        if members.is_empty() {
            return None;
        }
        if members.len() == 1
            && let Some(formatted) = Self::format_jsdoc_mapped_object_member(members[0].trim())
        {
            return Some(formatted);
        }

        let mut formatted = String::from("{\n");
        for member in members {
            let member = member.trim().trim_end_matches(';').trim();
            if member.is_empty() || !member.contains(':') {
                return None;
            }
            let (name, member_type) = member.split_once(':')?;
            let name = name.trim();
            let member_type = member_type.trim();
            if name.is_empty() || member_type.is_empty() {
                return None;
            }
            formatted.push_str("    ");
            formatted.push_str(name);
            formatted.push_str(": ");
            formatted.push_str(&Self::indent_jsdoc_inline_member_type(member_type));
            if !member_type.trim_end().ends_with(';') {
                formatted.push(';');
            }
            formatted.push('\n');
        }
        formatted.push('}');
        Some(formatted)
    }

    fn format_jsdoc_mapped_object_member(member: &str) -> Option<String> {
        let member = member.trim().trim_end_matches(';').trim();
        let (key, value) = member.split_once(':')?;
        let key = key.trim();
        if !(key.starts_with('[') && key.ends_with(']')) {
            return None;
        }
        let value_inner = value.trim().strip_prefix('{')?.strip_suffix('}')?.trim();
        if value_inner.is_empty() || value_inner.contains(['{', '}']) {
            return None;
        }

        let fields = Self::split_jsdoc_params(value_inner);
        if fields.is_empty() {
            return None;
        }

        let mut formatted = format!("{{ {key}: {{\n");
        for field in fields {
            let field = field.trim().trim_end_matches(';').trim();
            let (name, ty) = field.split_once(':')?;
            let name = name.trim();
            let ty = ty.trim();
            if name.is_empty() || ty.is_empty() {
                return None;
            }
            formatted.push_str("    ");
            formatted.push_str(name);
            formatted.push_str(": ");
            formatted.push_str(ty);
            formatted.push_str(";\n");
        }
        formatted.push_str("}; }");
        Some(formatted)
    }

    fn format_jsdoc_declaration_type_text(type_text: &str) -> String {
        if Self::jsdoc_module_reference_type_falls_back_to_any(type_text) {
            return "any".to_string();
        }
        if let Some(formatted) = Self::format_jsdoc_generic_object_type_text(type_text) {
            return formatted;
        }
        Self::format_jsdoc_object_type_text(type_text).unwrap_or_else(|| type_text.to_string())
    }

    fn format_jsdoc_generic_object_type_text(type_text: &str) -> Option<String> {
        let open = type_text.find("<{")?;
        if !type_text.ends_with("}>") {
            return None;
        }
        let prefix = &type_text[..open + 1];
        let inner = &type_text[open + 2..type_text.len() - 2];
        if inner.contains('{') || inner.contains('}') || inner.contains('\n') {
            return None;
        }

        let mut fields = Vec::new();
        for field in inner.split(',') {
            let (name, ty) = field.split_once(':')?;
            let name = name.trim();
            let ty = ty.trim();
            if name.is_empty() || ty.is_empty() {
                return None;
            }
            fields.push(format!("    {name}: {ty};"));
        }

        Some(format!("{prefix}{{\n{}\n}}>", fields.join("\n")))
    }

    pub(in crate::declaration_emitter) fn normalize_jsdoc_type_expr(type_expr: &str) -> String {
        let normalized_legacy_generics = type_expr.trim().replace(".<", "<");
        let trimmed = normalized_legacy_generics.as_str();
        if trimmed.is_empty() {
            return "any".to_string();
        }
        if Self::jsdoc_module_reference_type_falls_back_to_any(trimmed) {
            return "any".to_string();
        }
        if let Some(index_signature) = Self::normalize_jsdoc_object_index_type(trimmed) {
            return index_signature;
        }
        if trimmed == "?" {
            return Self::normalize_jsdoc_type_atom(trimmed);
        }
        if let Some(inner) = trimmed.strip_prefix('?') {
            return format!("{} | null", Self::normalize_jsdoc_type_expr(inner));
        }
        if let Some(inner) = trimmed.strip_suffix('?') {
            return format!("{} | null", Self::normalize_jsdoc_type_expr(inner));
        }
        if let Some(inner) = trimmed.strip_suffix('=') {
            return format!("{} | undefined", Self::normalize_jsdoc_type_expr(inner));
        }
        if let Some(inner) = trimmed.strip_prefix('!') {
            return Self::normalize_jsdoc_type_expr(inner);
        }
        if let Some(inner) = trimmed.strip_suffix('!') {
            return Self::normalize_jsdoc_type_expr(inner);
        }
        if let Some(inner) = Self::strip_balanced_parens(trimmed) {
            let normalized = Self::normalize_jsdoc_type_expr(inner);
            return if normalized.contains('|') {
                format!("({normalized})")
            } else {
                normalized
            };
        }
        let union_parts = Self::split_jsdoc_params(trimmed);
        if union_parts.len() == 1 && trimmed.contains('|') {
            let parts = Self::split_top_level_jsdoc_union(trimmed);
            if parts.len() > 1 {
                return parts
                    .into_iter()
                    .map(Self::normalize_jsdoc_type_expr)
                    .collect::<Vec<_>>()
                    .join(" | ");
            }
        }
        if let Some(generic) = Self::normalize_jsdoc_generic_type_reference(trimmed) {
            return generic;
        }
        Self::normalize_jsdoc_type_atom(trimmed)
    }

    fn normalize_jsdoc_generic_type_reference(type_expr: &str) -> Option<String> {
        let open = type_expr.find('<')?;
        if !type_expr.ends_with('>') {
            return None;
        }

        let base = type_expr[..open].trim();
        if base.is_empty()
            || !base
                .chars()
                .all(|ch| ch == '_' || ch == '$' || ch == '.' || ch.is_ascii_alphanumeric())
        {
            return None;
        }

        let args = type_expr[open + 1..type_expr.len() - 1].trim();
        if args.is_empty() {
            return None;
        }

        let normalized_args = Self::split_jsdoc_params(args)
            .into_iter()
            .map(Self::normalize_jsdoc_type_expr)
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!("{base}<{normalized_args}>"))
    }

    fn normalize_jsdoc_object_index_type(type_expr: &str) -> Option<String> {
        let args = type_expr
            .strip_prefix("Object<")
            .or_else(|| type_expr.strip_prefix("Object.<"))?
            .strip_suffix('>')?;
        let parts = Self::split_jsdoc_params(args);
        if parts.len() != 2 {
            return None;
        }
        let key = Self::normalize_jsdoc_type_expr(parts[0]);
        let value = Self::normalize_jsdoc_type_expr(parts[1]);
        let key = match key.as_str() {
            "string" | "number" | "symbol" => key,
            _ => "string".to_string(),
        };
        Some(format!("{{\n    [x: {key}]: {value};\n}}"))
    }

    fn strip_balanced_parens(text: &str) -> Option<&str> {
        let inner = text.strip_prefix('(')?.strip_suffix(')')?;
        let mut depth = 0usize;
        for (index, ch) in text.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 && index != text.len() - 1 {
                        return None;
                    }
                }
                _ => {}
            }
        }
        Some(inner)
    }

    fn split_top_level_jsdoc_union(text: &str) -> Vec<&str> {
        let mut result = Vec::new();
        let mut depth = 0usize;
        let mut start = 0usize;
        for (index, ch) in text.char_indices() {
            match ch {
                '(' | '<' | '{' | '[' => depth += 1,
                ')' | '>' | '}' | ']' => depth = depth.saturating_sub(1),
                '|' if depth == 0 => {
                    result.push(text[start..index].trim());
                    start = index + 1;
                }
                _ => {}
            }
        }
        result.push(text[start..].trim());
        result
    }

    /// Returns true when a JSDoc type expression contains syntax that cannot
    /// be emitted verbatim as TypeScript and should instead be resolved through
    /// the checker/solver type cache.
    pub(crate) fn jsdoc_type_needs_checker_resolution(type_text: &str) -> bool {
        let t = type_text.trim();
        t.starts_with("function(") || t.starts_with("function (")
    }

    /// Convert a JSDoc `function(...)` type to TypeScript arrow function syntax.
    /// e.g. `function(this:Object, ...*):*` -> `(this: Object, ...args: any[]) => any`
    pub(crate) fn convert_jsdoc_function_type(type_text: &str) -> Option<String> {
        let t = type_text.trim();
        let rest = t.strip_prefix("function")?.trim();
        let rest = rest.strip_prefix('(')?;

        // Find matching closing paren (handling nested parens)
        let mut depth = 1usize;
        let mut close_idx = None;
        for (i, ch) in rest.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close_idx = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close_idx = close_idx?;
        let params_str = &rest[..close_idx];
        let after_close = rest[close_idx + 1..].trim();

        // Parse return type (after `:`)
        let return_type = if let Some(ret) = after_close.strip_prefix(':') {
            Self::normalize_jsdoc_type_atom(ret.trim())
        } else {
            "any".to_string()
        };

        // Parse parameters
        let mut ts_params = Vec::new();
        let mut construct_return_type = None;
        if !params_str.trim().is_empty() {
            let raw_params = Self::split_jsdoc_params(params_str);
            let mut unnamed_idx = 0u32;
            for raw in &raw_params {
                let raw = raw.trim();
                if raw.is_empty() {
                    continue;
                }
                let (is_rest, raw) = if let Some(s) = raw.strip_prefix("...") {
                    (true, s.trim())
                } else {
                    (false, raw)
                };

                // Check for `name:Type` or just `Type`
                if let Some(colon) = Self::find_param_colon(raw) {
                    let name = raw[..colon].trim();
                    let ptype = Self::normalize_jsdoc_type_atom(raw[colon + 1..].trim());
                    if name == "new" {
                        construct_return_type = Some(ptype);
                        unnamed_idx = unnamed_idx.max(1);
                        continue;
                    }
                    if is_rest {
                        ts_params.push(format!("...args: {ptype}[]"));
                    } else {
                        ts_params.push(format!("{name}: {ptype}"));
                    }
                } else {
                    let ptype = Self::normalize_jsdoc_type_atom(raw);
                    if is_rest {
                        ts_params.push(format!("...args: {ptype}[]"));
                    } else {
                        let name = format!("arg{unnamed_idx}");
                        unnamed_idx += 1;
                        ts_params.push(format!("{name}: {ptype}"));
                    }
                }
            }
        }

        if let Some(construct_return_type) = construct_return_type {
            Some(format!(
                "new ({}) => {}",
                ts_params.join(", "),
                construct_return_type
            ))
        } else {
            Some(format!("({}) => {}", ts_params.join(", "), return_type))
        }
    }

    /// Normalize a single JSDoc type atom: `*` -> `any`, otherwise pass through.
    fn normalize_jsdoc_type_atom(s: &str) -> String {
        let s = s.trim();
        if Self::jsdoc_module_reference_type_falls_back_to_any(s) {
            return "any".to_string();
        }
        if let Some((base, args)) = Self::split_jsdoc_generic_atom(s) {
            if args.trim().is_empty() {
                return match base {
                    "Array" => "any[]".to_string(),
                    "Promise" => "Promise<any>".to_string(),
                    _ => format!("{base}<>"),
                };
            }
            let args = Self::split_jsdoc_params(args)
                .into_iter()
                .map(Self::normalize_jsdoc_type_expr)
                .collect::<Vec<_>>();
            return format!("{base}<{}>", args.join(", "));
        }
        match s {
            "*" | "?" => "any".to_string(),
            "String" => "string".to_string(),
            "Number" => "number".to_string(),
            "Boolean" => "boolean".to_string(),
            "Void" => "void".to_string(),
            "Undefined" => "undefined".to_string(),
            "Null" => "null".to_string(),
            "function" => "Function".to_string(),
            "event" => "Event".to_string(),
            // `Array<>` is the form after `normalize_jsdoc_type_expr` strips
            // the legacy `.<` → `<` so both `Array` and `Array.<>` reach this
            // arm. tsc treats empty-args generic JSDoc references as
            // implicit-any (`Array.<>` → `any[]`); without the `Array<>` arm
            // the DTS surfaces a literal `Array<>` token that is not valid
            // TypeScript.
            "array" | "Array" | "Array.<>" | "Array<>" => "any[]".to_string(),
            "promise" | "Promise" | "Promise.<>" | "Promise<>" => "Promise<any>".to_string(),
            _ => s.to_string(),
        }
    }

    fn jsdoc_module_reference_type_falls_back_to_any(type_text: &str) -> bool {
        type_text
            .trim()
            .strip_prefix("module:")
            .is_some_and(|rest| !rest.trim().is_empty())
    }

    fn split_jsdoc_generic_atom(s: &str) -> Option<(&str, &str)> {
        let open = s.find('<')?;
        if !s.ends_with('>') {
            return None;
        }
        let base = s[..open].trim();
        if base.is_empty()
            || !base
                .chars()
                .all(|ch| ch == '_' || ch == '$' || ch == '.' || ch.is_ascii_alphanumeric())
        {
            return None;
        }

        let mut depth = 0usize;
        for (idx, ch) in s.char_indices().skip(open) {
            match ch {
                '<' => depth += 1,
                '>' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 && idx != s.len() - 1 {
                        return None;
                    }
                }
                _ => {}
            }
        }
        if depth != 0 {
            return None;
        }
        Some((base, &s[open + 1..s.len() - 1]))
    }

    /// Split JSDoc function parameters by commas, respecting nested parens.
    fn split_jsdoc_params(s: &str) -> Vec<&str> {
        let mut result = Vec::new();
        let mut depth = 0usize;
        let mut quote: Option<char> = None;
        let mut escaped = false;
        let mut start = 0;
        for (i, ch) in s.char_indices() {
            if let Some(q) = quote {
                if escaped {
                    escaped = false;
                    continue;
                }
                if ch == '\\' {
                    escaped = true;
                    continue;
                }
                if ch == q {
                    quote = None;
                }
                continue;
            }
            match ch {
                '\'' | '"' | '`' => quote = Some(ch),
                '(' | '<' | '{' | '[' => depth += 1,
                ')' | '>' | '}' | ']' => {
                    depth = depth.saturating_sub(1);
                }
                ',' if depth == 0 => {
                    result.push(&s[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
        result.push(&s[start..]);
        result
    }

    /// Find the colon separating name from type in a JSDoc param like `this:Object`.
    /// Returns None if no colon found (the whole thing is a type).
    fn find_param_colon(s: &str) -> Option<usize> {
        // The name part should be a simple identifier (letters, digits, _, $)
        // If the first `:` appears after such a name, it's a name:type separator.
        let s = s.trim();
        for (i, ch) in s.char_indices() {
            if ch == ':' {
                return Some(i);
            }
            if !ch.is_ascii_alphanumeric() && ch != '_' && ch != '$' {
                return None;
            }
        }
        None
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
}

#[path = "jsdoc/function_facts.rs"]
mod function_facts;
#[path = "jsdoc/type_aliases.rs"]
mod type_aliases;

#[cfg(test)]
#[path = "jsdoc_tests.rs"]
mod jsdoc_tests;
