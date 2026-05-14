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
        Self::normalize_jsdoc_type_atom(trimmed)
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
        if let Some((base, args)) = Self::split_jsdoc_generic_atom(s) {
            let args = Self::split_jsdoc_params(args)
                .into_iter()
                .map(Self::normalize_jsdoc_type_expr)
                .collect::<Vec<_>>();
            if args.is_empty() {
                return match base {
                    "Array" => "any[]".to_string(),
                    "Promise" => "Promise<any>".to_string(),
                    _ => format!("{base}<>"),
                };
            }
            return format!("{base}<{}>", args.join(", "));
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
            let type_text = if matches!(prop_decl.type_text.as_str(), "object" | "Object") {
                self.jsdoc_object_param_nested_type_literal(&params, &qualified_name, 2)
                    .unwrap_or_else(|| prop_decl.type_text.clone())
            } else {
                prop_decl.type_text.clone()
            };
            member.push_str(&type_text);
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

    fn jsdoc_object_param_nested_type_literal(
        &self,
        params: &[JsdocParamDecl],
        object_name: &str,
        depth: u32,
    ) -> Option<String> {
        let prefix = format!("{object_name}.");
        let mut members = Vec::new();
        for prop_decl in params.iter().filter(|decl| decl.name.starts_with(&prefix)) {
            let rest = &prop_decl.name[prefix.len()..];
            if rest.is_empty() || rest.contains('.') {
                continue;
            }

            let mut member = String::new();
            member.push_str(rest);
            if prop_decl.optional {
                member.push('?');
            }
            member.push_str(": ");
            let type_text = if matches!(prop_decl.type_text.as_str(), "object" | "Object") {
                self.jsdoc_object_param_nested_type_literal(params, &prop_decl.name, depth + 1)
                    .unwrap_or_else(|| prop_decl.type_text.clone())
            } else {
                prop_decl.type_text.clone()
            };
            member.push_str(&type_text);
            if prop_decl.optional && !Self::type_text_has_undefined_branch(&prop_decl.type_text) {
                member.push_str(" | undefined");
            }
            member.push(';');
            members.push(member);
        }
        if members.is_empty() {
            return None;
        }

        let member_indent = "    ".repeat((self.indent_level + depth) as usize);
        let closing_indent = "    ".repeat((self.indent_level + depth - 1) as usize);
        let lines: Vec<String> = members
            .into_iter()
            .map(|member| format!("{member_indent}{member}"))
            .collect();
        Some(format!("{{\n{}\n{closing_indent}}}", lines.join("\n")))
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

    fn jsdoc_contains_type_tag(jsdoc: &str) -> bool {
        jsdoc.lines().any(|raw_line| {
            let line = raw_line.trim_start_matches('*').trim();
            line.strip_prefix("@type")
                .is_some_and(|rest| !rest.trim_start().starts_with("def"))
        })
    }

    pub(in crate::declaration_emitter) fn jsdoc_contains_type_alias_tag(jsdoc: &str) -> bool {
        Self::jsdoc_has_property_tags(jsdoc) || Self::parse_jsdoc_typedef_alias(jsdoc).is_some()
    }

    pub(in crate::declaration_emitter) fn jsdoc_chain_without_type_tags(
        chain: &[String],
    ) -> Vec<String> {
        chain
            .iter()
            .filter(|jsdoc| !Self::jsdoc_contains_type_tag(jsdoc))
            .cloned()
            .collect()
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
        if !has_jsdoc_tags && !is_export_equals_root && !has_any_jsdoc {
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
            if let Some((constraint, name_rest)) = Self::parse_jsdoc_braced_type_and_name(rest)
                && let Some((name, remaining)) = Self::take_jsdoc_template_name(name_rest)
            {
                let constraint = Self::normalize_jsdoc_type_text(constraint, false);
                let name_key = name.to_string();
                if seen.insert(name_key) {
                    params.push(format!("{name} extends {constraint}"));
                }
                rest = remaining;
            }

            for name in rest
                .split([',', ' ', '\t'])
                .map(str::trim)
                .filter(|name| !name.is_empty())
            {
                // Bracket-default form `@template [T=string]` declares type
                // parameter `T` with default `string`. Without unwrapping the
                // brackets, the verbatim segment `[T=string]` would be
                // emitted between `<` and `>` and produce invalid `.d.ts`
                // output (issue #4005).
                let normalized = Self::normalize_jsdoc_template_bracket_default(name);
                let name_str = normalized.into_owned();
                let key = Self::jsdoc_template_param_name_key(&name_str).to_string();
                if seen.insert(key) {
                    params.push(name_str);
                }
            }
        }

        params
    }

    fn trim_jsdoc_same_line_following_tags(text: &str) -> &str {
        text.find(" @")
            .map(|idx| text[..idx].trim_end())
            .unwrap_or(text)
    }

    /// Strip `[…]` from a `@template` segment and rewrite `T=default` as
    /// `T = default` so the result is valid TypeScript type-parameter
    /// syntax. Non-bracket segments are returned unchanged.
    fn normalize_jsdoc_template_bracket_default(segment: &str) -> std::borrow::Cow<'_, str> {
        let trimmed = segment.trim();
        if !(trimmed.starts_with('[') && trimmed.ends_with(']')) {
            return std::borrow::Cow::Borrowed(segment);
        }
        let inner = &trimmed[1..trimmed.len() - 1];
        if let Some((name, default)) = inner.split_once('=') {
            std::borrow::Cow::Owned(format!("{} = {}", name.trim(), default.trim()))
        } else {
            std::borrow::Cow::Owned(inner.trim().to_string())
        }
    }

    fn jsdoc_template_param_name_key(text: &str) -> &str {
        let trimmed = text.trim();
        let end = trimmed
            .find(|c: char| c == '=' || c.is_whitespace())
            .unwrap_or(trimmed.len());
        trimmed[..end].trim()
    }

    fn take_jsdoc_template_name(text: &str) -> Option<(&str, &str)> {
        let text = text.trim_start_matches([',', ' ', '\t']);
        if text.is_empty() {
            return None;
        }

        let end = text.find([',', ' ', '\t']).unwrap_or(text.len());
        let name = text[..end].trim();
        if name.is_empty() {
            return None;
        }
        Some((name, &text[end..]))
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
        let name = name
            .find('<')
            .and_then(|generic_start| name[..generic_start].split_whitespace().next())
            .filter(|base| !base.is_empty())
            .unwrap_or(name);
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
        source.push_str(&Self::jsdoc_type_alias_parser_type_text(&decl.type_text));
        source.push_str(";\n");

        if decl.render_verbatim {
            return Some(source);
        }

        let mut parser = ParserState::new("jsdoc-alias.ts".to_string(), source);
        let root = parser.parse_source_file();
        let mut emitter = DeclarationEmitter::new(&parser.arena);
        let mut rendered = emitter.emit(root);
        rendered = Self::compact_rendered_jsdoc_type_alias(&rendered);
        if !decl.type_params.is_empty() && decl.type_text.contains('\n') {
            let type_params = decl.type_params.join(", ");
            rendered = format!("/**\n * <{type_params}>\n */\n{rendered}");
        }
        if rendered.trim().is_empty() {
            None
        } else {
            Some(rendered)
        }
    }

    fn compact_rendered_jsdoc_type_alias(rendered: &str) -> String {
        let lines = rendered.lines().collect::<Vec<_>>();
        let mut output = String::new();
        let mut i = 0usize;
        while i < lines.len() {
            let line = lines[i];
            if line.trim_end().ends_with(": {")
                && i + 2 < lines.len()
                && lines[i + 1].trim_start().starts_with("[")
                && lines[i + 2].trim() == "};"
            {
                let prefix = line.trim_end().trim_end_matches('{').trim_end();
                output.push_str(prefix);
                output.push_str(" { ");
                output.push_str(lines[i + 1].trim());
                output.push_str(" };\n");
                i += 3;
                continue;
            }

            if line.trim() == "} & {"
                && i + 2 < lines.len()
                && lines[i + 1].trim_start().starts_with("[")
                && lines[i + 2].trim() == "};"
            {
                output.push_str("} & { ");
                output.push_str(lines[i + 1].trim());
                output.push_str(" };\n");
                i += 3;
                continue;
            }

            output.push_str(line);
            output.push('\n');
            i += 1;
        }
        output
    }

    pub(crate) fn format_jsdoc_type_text_for_declaration(type_text: &str) -> String {
        let Some(open) = type_text.find("<{") else {
            return type_text.to_string();
        };
        if !type_text.ends_with("}>") {
            return type_text.to_string();
        }
        let prefix = &type_text[..open + 1];
        let inner = &type_text[open + 2..type_text.len() - 2];
        if inner.contains('{') || inner.contains('}') || inner.contains('\n') {
            return type_text.to_string();
        }

        let mut fields = Vec::new();
        for field in inner.split(',') {
            let Some((name, ty)) = field.split_once(':') else {
                return type_text.to_string();
            };
            let name = name.trim();
            let ty = ty.trim();
            if name.is_empty() || ty.is_empty() {
                return type_text.to_string();
            }
            fields.push(format!("    {name}: {ty};"));
        }

        format!("{prefix}{{\n{}\n}}>", fields.join("\n"))
    }

    fn jsdoc_type_alias_parser_type_text(type_text: &str) -> String {
        if !type_text.contains('\n') {
            return type_text.to_string();
        }

        let mut normalized = String::new();
        for raw_line in type_text.lines() {
            let line = raw_line.trim_end();
            let trimmed = line.trim();
            normalized.push_str(line);
            if Self::jsdoc_multiline_type_line_needs_separator(trimmed) {
                normalized.push(';');
            }
            normalized.push('\n');
        }
        normalized.trim_end().to_string()
    }

    fn jsdoc_multiline_type_line_needs_separator(line: &str) -> bool {
        if line.is_empty()
            || line.starts_with(':')
            || line.ends_with(';')
            || line.ends_with(',')
            || line.ends_with('{')
            || line.ends_with('(')
            || line.ends_with('&')
            || line.ends_with('|')
        {
            return false;
        }

        line.contains("?:") || line.ends_with(')')
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
