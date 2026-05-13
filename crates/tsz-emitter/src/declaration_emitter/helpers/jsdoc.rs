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
use super::{JsdocParamDecl, JsdocTypeAliasDecl, escape_string_for_double_quote};

#[derive(Clone)]
pub(crate) struct JsdocOverloadSignature {
    pub(crate) comment: String,
    pub(crate) type_params: Vec<String>,
    pub(crate) params: Vec<JsdocParamDecl>,
    pub(crate) return_type: Option<String>,
}

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
        let mut without_line_comments = String::new();
        for line in between.lines() {
            let line = line
                .split_once("//")
                .map(|(before, _)| before)
                .unwrap_or(line);
            without_line_comments.push_str(line);
            without_line_comments.push('\n');
        }
        let trimmed = without_line_comments.trim();
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

    pub(in crate::declaration_emitter) fn leading_jsdoc_comment_chain_for_pos(
        &self,
        pos: u32,
    ) -> Vec<String> {
        self.leading_jsdoc_comment_chain_for_pos_with_style(pos)
            .into_iter()
            .map(|(jsdoc, _)| jsdoc)
            .collect()
    }

    pub(in crate::declaration_emitter) fn leading_jsdoc_comment_chain_for_pos_with_style(
        &self,
        pos: u32,
    ) -> Vec<(String, bool)> {
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

        let styled_comment = |comment: &tsz_common::comments::CommentRange| {
            let raw = &text[comment.pos as usize..comment.end as usize];
            (get_jsdoc_content(comment, text), raw.contains('\n'))
        };

        let mut chain = vec![styled_comment(nearest)];
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
            chain.push(styled_comment(comment));
            current_start = comment.pos as usize;
        }
        chain.reverse();
        chain
    }

    pub(in crate::declaration_emitter) fn jsdoc_comment_chain_with_source_style_for_pos(
        &self,
        pos: u32,
        chain: &[String],
    ) -> Vec<(String, bool)> {
        let styled_chain = self.leading_jsdoc_comment_chain_for_pos_with_style(pos);
        chain
            .iter()
            .map(|jsdoc| {
                let force_multiline = styled_chain
                    .iter()
                    .find_map(|(styled_jsdoc, force_multiline)| {
                        (styled_jsdoc == jsdoc).then_some(*force_multiline)
                    })
                    .unwrap_or(false);
                (jsdoc.clone(), force_multiline)
            })
            .collect()
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

    pub(in crate::declaration_emitter) fn normalize_jsdoc_type_expr(type_expr: &str) -> String {
        let normalized_legacy_generics = type_expr.trim().replace(".<", "<");
        let trimmed = normalized_legacy_generics.as_str();
        if trimmed.is_empty() {
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
        if let Some(inner) = s.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
            return format!("\"{}\"", escape_string_for_double_quote(inner));
        }
        match s {
            "*" | "?" => "any".to_string(),
            // `Array<>` is the form after `normalize_jsdoc_type_expr` strips
            // the legacy `.<` → `<` so both `Array` and `Array.<>` reach this
            // arm. tsc treats empty-args generic JSDoc references as
            // implicit-any (`Array.<>` → `any[]`); without the `Array<>` arm
            // the DTS surfaces a literal `Array<>` token that is not valid
            // TypeScript.
            "Array" | "Array.<>" | "Array<>" => "any[]".to_string(),
            "Promise" | "Promise.<>" | "Promise<>" => "Promise<any>".to_string(),
            _ => s.to_string(),
        }
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

    pub(in crate::declaration_emitter) fn jsdoc_has_overload_tag(jsdoc: &str) -> bool {
        jsdoc.lines().any(|raw_line| {
            let line = raw_line.trim_start_matches('*').trim();
            let Some(rest) = line.strip_prefix("@overload") else {
                return false;
            };
            rest.chars()
                .next()
                .is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '$')
        })
    }

    pub(crate) fn jsdoc_overload_signatures_from_chain(
        chain: &[String],
    ) -> Vec<JsdocOverloadSignature> {
        let mut overloads = Vec::new();
        for jsdoc in chain {
            if !Self::jsdoc_has_overload_tag(jsdoc) {
                continue;
            }

            let lines = jsdoc
                .lines()
                .map(|line| line.trim_start_matches('*').trim().to_string())
                .collect::<Vec<_>>();
            let overload_starts = lines
                .iter()
                .enumerate()
                .filter_map(|(idx, line)| {
                    line.strip_prefix("@overload").and_then(|rest| {
                        rest.chars()
                            .next()
                            .is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '$')
                            .then_some(idx)
                    })
                })
                .collect::<Vec<_>>();
            if overload_starts.is_empty() {
                continue;
            }

            let type_params = Self::parse_jsdoc_template_params(jsdoc);
            for (idx, &start) in overload_starts.iter().enumerate() {
                let end = overload_starts
                    .get(idx + 1)
                    .copied()
                    .unwrap_or_else(|| Self::jsdoc_last_overload_segment_end(&lines, start));
                let segment = lines[start..end].join("\n");
                let params = Self::parse_jsdoc_param_decls(&segment);
                let return_type = Self::parse_jsdoc_return_type_text(&segment);
                if params.is_empty() && return_type.is_none() {
                    continue;
                }

                overloads.push(JsdocOverloadSignature {
                    comment: jsdoc.clone(),
                    type_params: type_params.clone(),
                    params,
                    return_type,
                });
            }
        }
        overloads
    }

    fn jsdoc_last_overload_segment_end(lines: &[String], start: usize) -> usize {
        for idx in start + 1..lines.len().saturating_sub(1) {
            if !lines[idx].trim().is_empty() {
                continue;
            }
            let next = lines[idx + 1].trim();
            if next.starts_with("@param")
                || next.starts_with("@return")
                || next.starts_with("@returns")
            {
                return idx;
            }
        }
        lines.len()
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

    pub(crate) fn jsdoc_object_binding_param_type_literal(
        &self,
        param_idx: NodeIndex,
        position: usize,
    ) -> Option<String> {
        let jsdoc = self.function_like_jsdoc_for_node(param_idx)?;
        let params = Self::parse_jsdoc_param_decls(&jsdoc);
        if params.is_empty() {
            return None;
        }

        let param_node = self.arena.get(param_idx)?;
        let param = self.arena.get_parameter(param_node)?;
        let pattern_node = self.arena.get(param.name)?;
        if pattern_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN {
            return None;
        }

        let root_decl = if let Some(name) = self.get_identifier_text(param.name) {
            params.iter().find(|decl| decl.name == name)
        } else {
            params.get(position)
        }?;
        if !matches!(root_decl.type_text.as_str(), "object" | "Object") {
            return None;
        }

        let pattern = self.arena.get_binding_pattern(pattern_node)?;
        let mut members = Vec::new();
        for &elem_idx in &pattern.elements.nodes {
            let elem_node = self.arena.get(elem_idx)?;
            let elem = self.arena.get_binding_element(elem_node)?;
            if elem.dot_dot_dot_token {
                return None;
            }

            let prop_name_idx = if elem.property_name.is_some() {
                elem.property_name
            } else {
                elem.name
            };
            let prop_node = self.arena.get(prop_name_idx)?;
            if prop_node.kind != SyntaxKind::Identifier as u16 {
                return None;
            }
            let prop_name = self.arena.get_identifier(prop_node)?.escaped_text.as_str();
            let qualified_name = format!("{}.{}", root_decl.name, prop_name);
            let prop_decl = params.iter().find(|decl| decl.name == qualified_name)?;

            let mut member = String::new();
            member.push_str(prop_name);
            if prop_decl.optional {
                member.push('?');
            }
            member.push_str(": ");
            if matches!(prop_decl.type_text.as_str(), "object" | "Object")
                && let Some(nested) = Self::jsdoc_nested_object_type_literal(
                    &params,
                    &qualified_name,
                    (self.indent_level + 1) as usize,
                )
            {
                member.push_str(&nested);
            } else {
                member.push_str(&prop_decl.type_text);
            }
            if prop_decl.optional && !Self::type_text_has_undefined_branch(&prop_decl.type_text) {
                member.push_str(" | undefined");
            }
            member.push(';');
            members.push(member);
        }

        let member_indent = "    ".repeat((self.indent_level + 1) as usize);
        let closing_indent = "    ".repeat(self.indent_level as usize);
        let lines: Vec<String> = members
            .into_iter()
            .map(|member| format!("{member_indent}{member}"))
            .collect();
        (!lines.is_empty()).then(|| format!("{{\n{}\n{closing_indent}}}", lines.join("\n")))
    }

    fn jsdoc_nested_object_type_literal(
        params: &[JsdocParamDecl],
        prefix: &str,
        type_indent_level: usize,
    ) -> Option<String> {
        let child_prefix = format!("{prefix}.");
        let mut members = Vec::new();
        for decl in params {
            let Some(rest) = decl.name.strip_prefix(&child_prefix) else {
                continue;
            };
            if rest.is_empty() || rest.contains('.') {
                continue;
            }

            let mut member = String::new();
            member.push_str(rest);
            if decl.optional {
                member.push('?');
            }
            member.push_str(": ");
            if matches!(decl.type_text.as_str(), "object" | "Object")
                && let Some(nested) = Self::jsdoc_nested_object_type_literal(
                    params,
                    &decl.name,
                    type_indent_level + 1,
                )
            {
                member.push_str(&nested);
            } else {
                member.push_str(&decl.type_text);
            }
            if decl.optional && !Self::type_text_has_undefined_branch(&decl.type_text) {
                member.push_str(" | undefined");
            }
            member.push(';');
            members.push(member);
        }

        if members.is_empty() {
            return None;
        }

        let member_indent = "    ".repeat(type_indent_level + 1);
        let closing_indent = "    ".repeat(type_indent_level);
        let lines = members
            .into_iter()
            .map(|member| format!("{member_indent}{member}"))
            .collect::<Vec<_>>()
            .join("\n");
        Some(format!("{{\n{lines}\n{closing_indent}}}"))
    }

    pub(crate) fn jsdoc_satisfies_param_decl_for_parameter(
        &self,
        param_idx: NodeIndex,
        position: usize,
    ) -> Option<JsdocParamDecl> {
        let jsdoc = self.function_like_jsdoc_for_node(param_idx)?;
        let params = Self::parse_jsdoc_satisfies_param_decls(&jsdoc);
        if params.is_empty() {
            return None;
        }

        let param_node = self.arena.get(param_idx)?;
        let param = self.arena.get_parameter(param_node)?;
        let source_is_rest = param.dot_dot_dot_token;

        if let Some(name) = self.get_identifier_text(param.name)
            && let Some(found) = params.iter().find(|decl| decl.name == name)
        {
            return Some(Self::adapt_jsdoc_satisfies_param_decl(
                found,
                source_is_rest,
            ));
        }

        let mut next_position = 0usize;
        let mut rest_decl = None;
        for decl in &params {
            if decl.rest {
                rest_decl = Some(decl);
                continue;
            }
            if next_position == position {
                return Some(Self::adapt_jsdoc_satisfies_param_decl(decl, source_is_rest));
            }
            next_position += 1;
        }

        rest_decl.map(|decl| Self::adapt_jsdoc_satisfies_param_decl(decl, source_is_rest))
    }

    fn adapt_jsdoc_satisfies_param_decl(
        decl: &JsdocParamDecl,
        source_is_rest: bool,
    ) -> JsdocParamDecl {
        let mut adapted = decl.clone();
        if source_is_rest {
            return adapted;
        }

        adapted.rest = false;
        if decl.rest {
            adapted.optional = false;
            if let Some(element_type) = adapted.type_text.strip_suffix("[]") {
                adapted.type_text = element_type.trim().to_string();
            }
        }
        adapted
    }

    pub(in crate::declaration_emitter) fn parse_jsdoc_satisfies_param_decls(
        jsdoc: &str,
    ) -> Vec<JsdocParamDecl> {
        let Some(type_expr) = Self::extract_jsdoc_satisfies_expression(jsdoc) else {
            return Vec::new();
        };
        Self::parse_function_type_param_decls(type_expr)
    }

    fn extract_jsdoc_satisfies_expression(jsdoc: &str) -> Option<&str> {
        let tag_pos = jsdoc.find("@satisfies")?;
        let rest = &jsdoc[tag_pos + "@satisfies".len()..];
        let open = rest.find('{')?;
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
        let end_idx = end_idx?;
        Some(after_open[..end_idx].trim())
    }

    fn parse_function_type_param_decls(type_expr: &str) -> Vec<JsdocParamDecl> {
        let type_expr = type_expr.trim();
        let Some(params_text) = Self::function_type_params_text(type_expr) else {
            return Vec::new();
        };
        Self::split_jsdoc_params(params_text)
            .into_iter()
            .enumerate()
            .filter_map(|(index, raw)| Self::parse_function_type_param_decl(raw, index))
            .filter(|decl| decl.name != "this")
            .collect()
    }

    fn function_type_params_text(type_expr: &str) -> Option<&str> {
        if let Some(rest) = type_expr.strip_prefix("function") {
            let rest = rest.trim_start();
            let rest = rest.strip_prefix('(')?;
            let close_idx = Self::find_matching_paren(rest)?;
            return Some(&rest[..close_idx]);
        }

        let rest = type_expr.strip_prefix('(')?;
        let close_idx = Self::find_matching_paren(rest)?;
        let after_close = rest[close_idx + 1..].trim_start();
        if !after_close.starts_with("=>") {
            return None;
        }
        Some(&rest[..close_idx])
    }

    fn find_matching_paren(text: &str) -> Option<usize> {
        let mut depth = 1usize;
        let mut quote: Option<char> = None;
        let mut escaped = false;
        for (i, ch) in text.char_indices() {
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
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn parse_function_type_param_decl(raw: &str, index: usize) -> Option<JsdocParamDecl> {
        let raw = raw.trim();
        if raw.is_empty() {
            return None;
        }

        let (rest_param, raw) = if let Some(stripped) = raw.strip_prefix("...") {
            (true, stripped.trim())
        } else {
            (false, raw)
        };

        let (name, optional, type_expr) = if let Some(colon_idx) = Self::find_top_level_colon(raw) {
            let raw_name = raw[..colon_idx].trim();
            let raw_type = raw[colon_idx + 1..].trim();
            let optional = raw_name.ends_with('?');
            let name = raw_name.trim_end_matches('?').trim();
            let name = if name.is_empty() {
                format!("arg{index}")
            } else {
                name.to_string()
            };
            (name, optional, raw_type)
        } else {
            (format!("arg{index}"), false, raw)
        };

        Some(JsdocParamDecl {
            name,
            type_text: Self::normalize_jsdoc_type_text(type_expr, rest_param),
            optional,
            rest: rest_param,
        })
    }

    fn find_top_level_colon(text: &str) -> Option<usize> {
        let mut depth = 0usize;
        let mut quote: Option<char> = None;
        let mut escaped = false;
        for (i, ch) in text.char_indices() {
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
                ')' | '>' | '}' | ']' => depth = depth.saturating_sub(1),
                ':' if depth == 0 => return Some(i),
                _ => {}
            }
        }
        None
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
            let text = Self::normalize_jsdoc_type_text(type_expr, false);
            if Self::jsdoc_type_needs_checker_resolution(&text) {
                return Self::convert_jsdoc_function_type(&text);
            }
            return Some(text);
        }
        None
    }

    pub(crate) fn parse_jsdoc_type_text(jsdoc: &str) -> Option<String> {
        for raw_line in jsdoc.lines() {
            let line = raw_line.trim_start_matches('*').trim();
            let Some(rest) = line.strip_prefix("@type") else {
                continue;
            };
            if rest.starts_with("def") {
                continue;
            }
            let rest = rest.trim();
            let (type_expr, _) = Self::parse_jsdoc_braced_type_and_name(rest)?;
            let text = if type_expr.trim() == "?" {
                "unknown".to_string()
            } else {
                Self::normalize_jsdoc_type_text(type_expr, false)
            };
            if Self::jsdoc_type_needs_checker_resolution(&text) {
                return Self::convert_jsdoc_function_type(&text);
            }
            return Some(text);
        }
        None
    }

    pub(crate) fn jsdoc_return_type_text_for_node(&self, idx: NodeIndex) -> Option<String> {
        let jsdoc = self.function_like_jsdoc_for_node(idx)?;
        Self::parse_jsdoc_return_type_text(&jsdoc)
    }

    pub(crate) fn jsdoc_type_text_for_node(&self, idx: NodeIndex) -> Option<String> {
        let jsdoc = self.function_like_jsdoc_for_node(idx)?;
        let type_text = Self::parse_jsdoc_type_text(&jsdoc)?;
        self.local_semicolon_class_member_typedef_type_text(idx, &type_text)
            .or(Some(type_text))
    }

    fn local_semicolon_class_member_typedef_type_text(
        &self,
        idx: NodeIndex,
        type_text: &str,
    ) -> Option<String> {
        let node = self.arena.get(idx)?;
        if self.arena.get_property_decl(node).is_none()
            || !Self::is_simple_jsdoc_type_name(type_text)
        {
            return None;
        }

        let text = self.source_file_text.as_deref()?;
        let mut cursor = node.pos as usize;
        while cursor < text.len() && matches!(text.as_bytes()[cursor], b' ' | b'\t' | b'\r' | b'\n')
        {
            cursor += 1;
        }

        for comment in self
            .all_comments
            .iter()
            .filter(|comment| comment.end as usize <= cursor)
            .filter(|comment| is_jsdoc_comment(comment, text))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            let between = &text[comment.end as usize..cursor];
            if !between
                .bytes()
                .all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b';'))
            {
                break;
            }

            let jsdoc = get_jsdoc_content(comment, text);
            if let Some((name, base_type)) = Self::parse_jsdoc_typedef_alias(&jsdoc)
                && name == type_text
            {
                return Some(Self::normalize_jsdoc_type_text(&base_type, false));
            }
            cursor = comment.pos as usize;
        }

        None
    }

    fn is_simple_jsdoc_type_name(type_text: &str) -> bool {
        let mut chars = type_text.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        (first == '_' || first == '$' || first.is_ascii_alphabetic())
            && chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
    }

    pub(crate) fn jsdoc_function_type_signature_for_node(
        &self,
        idx: NodeIndex,
    ) -> Option<JsdocFunctionTypeSignature> {
        let jsdoc = self.function_like_jsdoc_for_node(idx)?;
        let type_name = Self::parse_jsdoc_type_text(&jsdoc)?;
        if !Self::is_simple_jsdoc_type_name(&type_name) {
            return None;
        }

        for comment in self.leading_jsdoc_comment_chain_for_node_or_ancestors(idx) {
            let Some((name, type_text)) = Self::parse_jsdoc_typedef_alias(&comment) else {
                continue;
            };
            if name != type_name {
                continue;
            }
            if let Some(signature) = parse_jsdoc_function_type_signature(&type_text) {
                return Some(signature);
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn statement_jsdoc_type_function_signature_node(
        &self,
        stmt_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let stmt_node = self.arena.get(stmt_idx)?;
        let func_idx = if stmt_node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
            stmt_idx
        } else if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
            let export = self.arena.get_export_decl(stmt_node)?;
            let clause_node = self.arena.get(export.export_clause)?;
            (clause_node.kind == syntax_kind_ext::FUNCTION_DECLARATION)
                .then_some(export.export_clause)?
        } else {
            return None;
        };
        self.jsdoc_function_type_signature_for_node(func_idx)
            .map(|_| func_idx)
    }

    pub(crate) fn emit_jsdoc_function_type_signature(
        &mut self,
        type_params: &[String],
        params: &[(String, String)],
        return_type: &str,
    ) {
        self.emit_jsdoc_template_parameters(type_params);
        self.write("(");
        for (idx, (name, type_text)) in params.iter().enumerate() {
            if idx > 0 {
                self.write(", ");
            }
            self.write(name);
            self.write(": ");
            self.write(type_text);
        }
        self.write("): ");
        self.write(return_type);
    }

    pub(crate) fn jsdoc_template_params_for_node(&self, idx: NodeIndex) -> Vec<String> {
        self.function_like_jsdoc_for_node(idx)
            .map(|jsdoc| Self::parse_jsdoc_template_params(&jsdoc))
            .unwrap_or_default()
    }

    pub(crate) fn jsdoc_template_params_for_pos(&self, pos: u32) -> Vec<String> {
        for jsdoc in self.leading_jsdoc_comment_chain_for_pos(pos) {
            let params = Self::parse_jsdoc_template_params(&jsdoc);
            if !params.is_empty() {
                return params;
            }
        }
        Vec::new()
    }

    pub(crate) fn jsdoc_has_readonly_for_node(&self, idx: NodeIndex) -> bool {
        self.function_like_jsdoc_for_node(idx)
            .as_deref()
            .is_some_and(|jsdoc| {
                jsdoc.lines().any(|raw_line| {
                    let line = raw_line.trim_start_matches('*').trim();
                    line == "@readonly" || line.starts_with("@readonly ")
                })
            })
    }

    pub(crate) fn jsdoc_has_protected_for_node(&self, idx: NodeIndex) -> bool {
        self.function_like_jsdoc_for_node(idx)
            .as_deref()
            .is_some_and(|jsdoc| {
                jsdoc.lines().any(|raw_line| {
                    let line = raw_line.trim_start_matches('*').trim();
                    line == "@protected" || line.starts_with("@protected ")
                })
            })
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

    pub(in crate::declaration_emitter) fn jsdoc_has_satisfies_tag(jsdoc: &str) -> bool {
        jsdoc.lines().any(|raw_line| {
            let line = raw_line.trim_start_matches('*').trim();
            let Some(rest) = line.strip_prefix("@satisfies") else {
                return false;
            };
            rest.chars()
                .next()
                .is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '$')
        })
    }

    pub(in crate::declaration_emitter) fn jsdoc_contains_type_tag(jsdoc: &str) -> bool {
        jsdoc.lines().any(|raw_line| {
            let line = raw_line.trim_start_matches('*').trim();
            line.strip_prefix("@type")
                .is_some_and(|rest| !rest.trim_start().starts_with("def"))
        })
    }

    pub(in crate::declaration_emitter) fn jsdoc_contains_type_alias_tag(jsdoc: &str) -> bool {
        Self::jsdoc_has_property_tags(jsdoc) || Self::parse_jsdoc_typedef_alias(jsdoc).is_some()
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
        // In JS files, tsc always converts `const x = (arrow) => ...` or
        // `const x = function(...) {}` to `function x(...)` in declarations,
        // regardless of whether JSDoc @param/@returns tags are present.
        // Only bail out for non-export-equals when there are no JSDoc tags
        // AND no attached JSDoc comment at all (so we don't lose doc comments).
        let has_jsdoc_tags = jsdoc
            .as_deref()
            .is_some_and(Self::jsdoc_has_function_signature_tags);
        let has_any_jsdoc = jsdoc.is_some();
        if !has_jsdoc_tags && !is_export_equals_root && !has_any_jsdoc && !is_exported {
            return false;
        }

        if self
            .current_statement_jsdoc_chain
            .iter()
            .any(|jsdoc| Self::jsdoc_has_satisfies_tag(jsdoc))
        {
            self.suppress_current_statement_jsdoc_comments = true;
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
        self.use_jsdoc_satisfies_parameter_fallback = true;
        self.emit_parameters_with_body(&func.parameters, func.body);
        self.use_jsdoc_satisfies_parameter_fallback = false;
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
        } else if let Some(return_type_text) = self
            .js_function_body_preferred_return_text_for_declaration(
                func.body,
                decl_name,
                &func.parameters,
            )
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
        self.emit_js_computed_binding_name_dependencies_for_function(&func.parameters);
        self.emit_js_function_like_class_if_needed(
            decl_name,
            &func.parameters,
            func.body,
            is_exported,
            initializer,
        );
        self.emit_js_namespace_export_aliases_for_name(decl_name, is_exported);
        true
    }

    pub(in crate::declaration_emitter) fn js_const_used_as_computed_binding_name_in_exported_function(
        &self,
        name_idx: NodeIndex,
    ) -> bool {
        if !self.source_is_js_file {
            return false;
        }
        let Some(binder) = self.binder else {
            return false;
        };
        let target_symbol = binder.node_symbols.get(&name_idx.0).copied().or_else(|| {
            self.get_identifier_text(name_idx)
                .and_then(|name| binder.file_locals.get(&name))
        });
        let Some(target_symbol) = target_symbol else {
            return false;
        };

        self.arena.nodes.iter().enumerate().any(|(idx, node)| {
            let decl_idx = NodeIndex(idx as u32);
            let Some(decl) = self.arena.get_variable_declaration(node) else {
                return false;
            };
            if !self.variable_declaration_has_effective_export(decl_idx) {
                return false;
            }
            let Some(init_node) = self.arena.get(decl.initializer) else {
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
            let mut symbols = Vec::new();
            let mut seen = FxHashSet::default();
            self.collect_computed_binding_name_symbols(&func.parameters, &mut symbols, &mut seen);
            symbols.contains(&target_symbol)
        })
    }

    pub(in crate::declaration_emitter) fn emit_js_computed_binding_name_dependencies_for_function(
        &mut self,
        params: &NodeList,
    ) {
        if !self.source_is_js_file {
            return;
        }
        let mut symbols = Vec::new();
        let mut seen = FxHashSet::default();
        self.collect_computed_binding_name_symbols(params, &mut symbols, &mut seen);
        for symbol_id in symbols {
            self.emit_js_const_symbol_dependency(symbol_id);
        }
    }

    fn collect_computed_binding_name_symbols(
        &self,
        params: &NodeList,
        symbols: &mut Vec<SymbolId>,
        seen: &mut FxHashSet<SymbolId>,
    ) {
        for &param_idx in &params.nodes {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            self.collect_computed_binding_name_symbols_from_pattern(param.name, symbols, seen);
        }
    }

    fn collect_computed_binding_name_symbols_from_pattern(
        &self,
        pattern_idx: NodeIndex,
        symbols: &mut Vec<SymbolId>,
        seen: &mut FxHashSet<SymbolId>,
    ) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };
        if pattern_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN
            && pattern_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN
        {
            return;
        }
        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return;
        };
        for &elem_idx in &pattern.elements.nodes {
            let Some(elem_node) = self.arena.get(elem_idx) else {
                continue;
            };
            if elem_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                continue;
            }
            let Some(elem) = self.arena.get_binding_element(elem_node) else {
                continue;
            };
            if elem.property_name.is_some()
                && let Some(prop_node) = self.arena.get(elem.property_name)
                && prop_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                && let Some(computed) = self.arena.get_computed_property(prop_node)
            {
                let expr_idx = self
                    .arena
                    .skip_parenthesized_and_assertions_and_comma(computed.expression);
                if let Some(sym_id) = self.value_reference_symbol(expr_idx)
                    && seen.insert(sym_id)
                {
                    symbols.push(sym_id);
                }
            }
            self.collect_computed_binding_name_symbols_from_pattern(elem.name, symbols, seen);
        }
    }

    fn emit_js_const_symbol_dependency(&mut self, symbol_id: SymbolId) -> bool {
        if !self.emitted_synthetic_dependency_symbols.insert(symbol_id) {
            return false;
        }
        let Some(binder) = self.binder else {
            self.emitted_synthetic_dependency_symbols.remove(&symbol_id);
            return false;
        };
        let Some(symbol) = binder.symbols.get(symbol_id) else {
            self.emitted_synthetic_dependency_symbols.remove(&symbol_id);
            return false;
        };
        for decl_idx in symbol.all_declarations() {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                continue;
            };
            if !self.arena.is_const_variable_declaration(decl_idx) {
                continue;
            }
            let Some(name) = self.get_identifier_text(decl.name) else {
                continue;
            };
            let Some(type_text) = self.const_literal_initializer_text_deep(decl.initializer) else {
                continue;
            };
            self.write_indent();
            if self.should_emit_declare_keyword(false) {
                self.write("declare ");
            }
            self.write("const ");
            self.write(&name);
            self.write(": ");
            self.write(&type_text);
            self.write(";");
            self.write_line();
            self.emitted_non_exported_declaration = true;
            return true;
        }
        self.emitted_synthetic_dependency_symbols.remove(&symbol_id);
        false
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

            let mut rest = Self::trim_jsdoc_same_line_following_tags(rest.trim());
            let mut constraint = None;
            if let Some((raw_constraint, name_rest)) = Self::parse_jsdoc_braced_type_and_name(rest)
            {
                constraint = Some(Self::normalize_jsdoc_type_text(raw_constraint, false));
                rest = name_rest;
            }

            let mut applied_constraint = false;
            for (name, default) in Self::parse_jsdoc_template_param_parts(rest) {
                let constraint = if applied_constraint {
                    None
                } else {
                    constraint.as_deref()
                };
                let rendered =
                    Self::render_jsdoc_template_param(&name, default.as_deref(), constraint);
                let key = Self::jsdoc_template_param_name_key(&rendered).to_string();
                if seen.insert(key) {
                    params.push(rendered);
                }
                applied_constraint = true;
            }
        }

        params
    }

    fn parse_jsdoc_template_param_parts(text: &str) -> Vec<(String, Option<String>)> {
        let mut parts = Vec::new();
        let mut cursor = 0usize;
        let bytes = text.as_bytes();
        let mut parsed_any = false;

        while cursor < bytes.len() {
            let mut saw_comma = false;
            while cursor < bytes.len() {
                let ch = bytes[cursor] as char;
                if ch == ',' {
                    saw_comma = true;
                    cursor += 1;
                } else if ch.is_ascii_whitespace() {
                    cursor += 1;
                } else {
                    break;
                }
            }
            if cursor >= bytes.len() || (parsed_any && !saw_comma) {
                break;
            }

            let (name, default, end) = if bytes[cursor] == b'[' {
                let Some(close_offset) = text[cursor + 1..].find(']') else {
                    break;
                };
                let close = cursor + 1 + close_offset;
                let inner = text[cursor + 1..close].trim();
                let (name, default) = if let Some((name, default)) = inner.split_once('=') {
                    (name.trim(), Some(default.trim()))
                } else {
                    (inner, Some("any"))
                };
                if name.is_empty() {
                    break;
                }
                (name.to_string(), default.map(str::to_string), close + 1)
            } else {
                let start = cursor;
                while cursor < bytes.len() {
                    let ch = bytes[cursor] as char;
                    if ch == '_' || ch == '$' || ch.is_ascii_alphanumeric() {
                        cursor += 1;
                    } else {
                        break;
                    }
                }
                if start == cursor {
                    break;
                }

                let first = &text[start..cursor];
                if first == "const" {
                    let mut name_start = cursor;
                    while name_start < bytes.len()
                        && (bytes[name_start] as char).is_ascii_whitespace()
                    {
                        name_start += 1;
                    }
                    let mut name_end = name_start;
                    while name_end < bytes.len() {
                        let ch = bytes[name_end] as char;
                        if ch == '_' || ch == '$' || ch.is_ascii_alphanumeric() {
                            name_end += 1;
                        } else {
                            break;
                        }
                    }
                    if name_start == name_end {
                        break;
                    }
                    (
                        format!("const {}", &text[name_start..name_end]),
                        None,
                        name_end,
                    )
                } else {
                    (first.to_string(), None, cursor)
                }
            };

            parts.push((name, default));
            parsed_any = true;
            cursor = end;
        }

        parts
    }

    fn render_jsdoc_template_param(
        name: &str,
        default: Option<&str>,
        constraint: Option<&str>,
    ) -> String {
        let mut rendered = name.trim().to_string();
        if let Some(constraint) = constraint
            && !constraint.trim().is_empty()
        {
            rendered.push_str(" extends ");
            rendered.push_str(constraint.trim());
        }
        if let Some(default) = default {
            rendered.push_str(" = ");
            let default = default.trim();
            rendered.push_str(if default.is_empty() { "any" } else { default });
        }
        rendered
    }

    fn trim_jsdoc_same_line_following_tags(text: &str) -> &str {
        text.find(" @")
            .map(|idx| text[..idx].trim_end())
            .unwrap_or(text)
    }

    fn jsdoc_template_param_name_key(text: &str) -> &str {
        let trimmed = text.trim();
        let trimmed = trimmed.strip_prefix("const ").unwrap_or(trimmed);
        let end = trimmed
            .find(|c: char| c == '=' || c.is_whitespace())
            .unwrap_or(trimmed.len());
        trimmed[..end].trim()
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

    fn jsdoc_typedef_inline_description_lines(jsdoc: &str) -> Vec<String> {
        let normalized = Self::normalize_jsdoc_block(jsdoc);
        let Some(tag_pos) = normalized.find("@typedef") else {
            return Vec::new();
        };
        let rest = normalized[tag_pos + "@typedef".len()..].trim();
        let Some((_type_expr, name_rest)) = Self::parse_jsdoc_braced_type_and_name(rest) else {
            return Vec::new();
        };
        let after_name = name_rest
            .split_once(char::is_whitespace)
            .map(|(_, rest)| rest.trim())
            .unwrap_or_default();
        if after_name.is_empty() || after_name.starts_with('@') {
            return Vec::new();
        }

        after_name
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect()
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
        let (name, base_type) = Self::parse_jsdoc_typedef_alias(jsdoc)
            .or_else(|| Self::parse_jsdoc_name_only_typedef_alias(jsdoc))?;
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
            type_text.push_str(&Self::render_jsdoc_property_name(&property_name));
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

    fn parse_jsdoc_name_only_typedef_alias(jsdoc: &str) -> Option<(String, String)> {
        let normalized = Self::normalize_jsdoc_block(jsdoc);
        let tag_pos = normalized.find("@typedef")?;
        let rest = normalized[tag_pos + "@typedef".len()..].trim();
        if rest.starts_with('{') {
            return None;
        }
        let name = rest
            .split_whitespace()
            .next()
            .filter(|name| !name.is_empty())?;
        Some((name.to_string(), "Object".to_string()))
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

    fn render_jsdoc_property_name(name: &str) -> String {
        if Self::is_jsdoc_property_identifier_name(name) {
            return name.to_string();
        }
        if Self::is_quoted_jsdoc_property_name(name) {
            return name.to_string();
        }

        let mut quoted = String::from("\"");
        for ch in name.chars() {
            match ch {
                '"' => quoted.push_str("\\\""),
                '\\' => quoted.push_str("\\\\"),
                '\n' => quoted.push_str("\\n"),
                '\r' => quoted.push_str("\\r"),
                '\t' => quoted.push_str("\\t"),
                _ => quoted.push(ch),
            }
        }
        quoted.push('"');
        quoted
    }

    fn is_jsdoc_property_identifier_name(name: &str) -> bool {
        let mut chars = name.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        (first == '_' || first == '$' || first.is_ascii_alphabetic())
            && chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
    }

    fn is_quoted_jsdoc_property_name(name: &str) -> bool {
        let mut chars = name.chars();
        let Some(quote @ ('"' | '\'')) = chars.next() else {
            return false;
        };
        name.ends_with(quote) && name.len() > quote.len_utf8()
    }

    pub(in crate::declaration_emitter) fn parse_jsdoc_type_alias_decl(
        jsdoc: &str,
    ) -> Option<JsdocTypeAliasDecl> {
        let type_params = Self::parse_jsdoc_template_params(jsdoc);
        let description_lines = Self::jsdoc_description_lines(jsdoc);

        if Self::jsdoc_has_property_tags(jsdoc) {
            let (name, type_text) = Self::parse_jsdoc_property_type_alias(jsdoc)?;
            if name == "default" {
                return None;
            }
            return Some(JsdocTypeAliasDecl {
                name,
                type_params,
                type_text,
                description_lines,
                render_verbatim: true,
            });
        }

        if let Some((name, type_text)) = Self::parse_jsdoc_typedef_alias(jsdoc) {
            if name == "default" {
                return None;
            }
            let description_lines = if description_lines.is_empty() {
                Self::jsdoc_typedef_inline_description_lines(jsdoc)
            } else {
                description_lines
            };
            return Some(JsdocTypeAliasDecl {
                name,
                type_params,
                type_text,
                description_lines,
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

    fn parse_jsdoc_default_typedef_alias_decl(
        jsdoc: &str,
        alias_name: &str,
    ) -> Option<JsdocTypeAliasDecl> {
        let type_params = Self::parse_jsdoc_template_params(jsdoc);
        let (name, type_text) = if Self::jsdoc_has_property_tags(jsdoc) {
            Self::parse_jsdoc_property_type_alias(jsdoc)?
        } else {
            Self::parse_jsdoc_typedef_alias(jsdoc)?
        };
        if name != "default" {
            return None;
        }

        Some(JsdocTypeAliasDecl {
            name: alias_name.to_string(),
            type_params,
            type_text,
            description_lines: Vec::new(),
            render_verbatim: Self::jsdoc_has_property_tags(jsdoc),
        })
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
        let exported = self
            .current_source_file_idx
            .and_then(|source_file_idx| self.arena.get(source_file_idx))
            .and_then(|node| self.arena.get_source_file(node))
            .is_some_and(|source_file| self.source_file_has_module_syntax(source_file))
            && self.js_export_equals_names.is_empty();
        if !exported {
            return;
        }
        for jsdoc in self.leading_jsdoc_comment_chain_for_pos(pos) {
            if let Some(decl) = Self::parse_jsdoc_type_alias_decl(&jsdoc) {
                self.emit_rendered_jsdoc_type_alias(decl, exported);
            }
        }
    }

    pub(in crate::declaration_emitter) fn jsdoc_type_alias_decls_before_pos(
        &self,
        pos: u32,
    ) -> Vec<JsdocTypeAliasDecl> {
        if !self.source_is_js_file {
            return Vec::new();
        }
        let Some(text) = self.source_file_text.as_deref() else {
            return Vec::new();
        };
        self.all_comments
            .iter()
            .filter(|comment| comment.end <= pos)
            .filter(|comment| is_jsdoc_comment(comment, text))
            .map(|comment| get_jsdoc_content(comment, text))
            .filter_map(|jsdoc| Self::parse_jsdoc_type_alias_decl(&jsdoc))
            .collect()
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
        let (var_stmt, callback_pos) = if let Some(var_stmt) = self.arena.get_variable(stmt_node) {
            (var_stmt, stmt_node.pos)
        } else if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
            let Some(export) = self.arena.get_export_decl(stmt_node) else {
                return;
            };
            let Some(export_clause_node) = self.arena.get(export.export_clause) else {
                return;
            };
            let Some(var_stmt) = self.arena.get_variable(export_clause_node) else {
                return;
            };
            (var_stmt, stmt_node.pos)
        } else {
            return;
        };

        let callback_chain = self.leading_jsdoc_comment_chain_for_pos(callback_pos);
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
                    .jsdoc_name_like_type_expr_for_pos(callback_pos)
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
                            stmt_idx, true,
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
                self.emit_rendered_jsdoc_type_alias(decl, self.js_export_equals_names.is_empty());
            }
        }
    }

    pub(crate) fn emit_pending_top_level_jsdoc_type_aliases(
        &mut self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) {
        if !self.source_is_js_file {
            return;
        }
        let exported = self.source_file_has_module_syntax(source_file)
            && self.js_export_equals_names.is_empty();

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
            self.emit_rendered_jsdoc_type_alias(decl, exported);
        }
    }

    pub(crate) fn emit_jsdoc_default_typedef_aliases_for_hoisted_default_exports(
        &mut self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) {
        if !self.source_is_js_file || self.js_export_default_names.len() != 1 {
            return;
        }
        let Some(alias_name) = self.js_export_default_names.iter().next().cloned() else {
            return;
        };

        let exported = self.source_file_has_module_syntax(source_file)
            && self.js_export_equals_names.is_empty();

        let alias_can_share_declaration_name =
            self.js_default_typedef_alias_can_share_declaration_name(source_file, &alias_name);

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            for jsdoc in self.leading_jsdoc_comment_chain_for_pos(stmt_node.pos) {
                self.emit_jsdoc_default_typedef_alias_decl_for_comment(
                    &jsdoc,
                    &alias_name,
                    exported,
                    alias_can_share_declaration_name,
                );
            }
        }

        let Ok(eof_pos) = u32::try_from(source_file.text.len()) else {
            return;
        };
        for jsdoc in self.leading_jsdoc_comment_chain_for_pos(eof_pos) {
            self.emit_jsdoc_default_typedef_alias_decl_for_comment(
                &jsdoc,
                &alias_name,
                exported,
                alias_can_share_declaration_name,
            );
        }
    }

    fn emit_jsdoc_default_typedef_alias_decl_for_comment(
        &mut self,
        jsdoc: &str,
        alias_name: &str,
        exported: bool,
        alias_can_share_declaration_name: bool,
    ) {
        let Some(mut decl) = Self::parse_jsdoc_default_typedef_alias_decl(jsdoc, alias_name) else {
            return;
        };

        if self.reserved_names.contains(&decl.name) && !alias_can_share_declaration_name {
            decl.name = self.generate_unique_name(&decl.name);
        }
        self.reserved_names.insert(decl.name.clone());

        self.emit_rendered_jsdoc_type_alias(decl, exported);
    }

    fn js_default_typedef_alias_can_share_declaration_name(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
        alias_name: &str,
    ) -> bool {
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            match stmt_node.kind {
                k if k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_DECLARATION =>
                {
                    if self.extract_declaration_name(stmt_idx).as_deref() == Some(alias_name) {
                        return true;
                    }
                }
                _ => {}
            }
        }

        false
    }
}
