//! Optional and undefined semantic helpers for declaration type printing.

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

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn simplify_inexact_optional_mapped_intersection_text(
        type_text: &str,
    ) -> Option<String> {
        let start = type_text.find("{} & {")?;
        let first_start = start + "{} & ".len();
        let first_end = Self::balanced_brace_end(type_text, first_start)?;
        let mut next = Self::skip_ascii_whitespace(type_text, first_end)?;
        if !type_text.get(next..)?.starts_with('&') {
            return None;
        }
        next += 1;
        next = Self::skip_ascii_whitespace(type_text, next)?;
        if !type_text.get(next..)?.starts_with('{') {
            return None;
        }
        let second_end = Self::balanced_brace_end(type_text, next)?;
        let candidate = type_text.get(start..second_end)?;
        if !candidate.contains("as undefined extends")
            || !candidate.contains("[keyof unknown]")
            || !candidate.contains("? keyof unknown : never")
            || !candidate.contains("? never : keyof unknown")
        {
            return None;
        }

        let source_object = Self::inexact_optional_source_object_text(candidate)?;
        let simplified = Self::inexact_optional_object_intersection_text(&source_object)?;
        let mut output =
            String::with_capacity(type_text.len() - candidate.len() + simplified.len());
        output.push_str(type_text.get(..start)?);
        output.push_str(&simplified);
        output.push_str(type_text.get(second_end..)?);
        Some(output)
    }

    fn inexact_optional_source_object_text(candidate: &str) -> Option<String> {
        let marker = "undefined extends";
        let marker_start = candidate.find(marker)? + marker.len();
        let object_start = Self::skip_ascii_whitespace(candidate, marker_start)?;
        if !candidate.get(object_start..)?.starts_with('{') {
            return None;
        }
        let object_end = Self::balanced_brace_end(candidate, object_start)?;
        candidate.get(object_start..object_end).map(str::to_string)
    }

    pub(in crate::declaration_emitter) fn inexact_optional_object_intersection_text(
        source_object: &str,
    ) -> Option<String> {
        let inner = source_object.trim().strip_prefix('{')?.strip_suffix('}')?;
        let members = Self::split_object_members(inner);
        if members.is_empty() {
            return None;
        }

        let mut optional_members = Vec::new();
        let mut required_members = Vec::new();
        for member in members {
            let (name, explicit_optional, type_text) = Self::parse_object_property_member(&member)?;
            let type_includes_undefined = Self::type_text_contains_undefined(type_text);
            if explicit_optional || type_includes_undefined {
                let optional_name = name.strip_suffix('?').unwrap_or(name).trim();
                let optional_type = if type_includes_undefined {
                    type_text.to_string()
                } else {
                    format!("{type_text} | undefined")
                };
                optional_members.push(format!("    {optional_name}?: {optional_type};"));
            } else {
                required_members.push(format!("    {name}: {type_text};"));
            }
        }

        if optional_members.is_empty() || required_members.is_empty() {
            return None;
        }

        Some(format!(
            "{{\n{}\n}} & {{\n{}\n}}",
            optional_members.join("\n"),
            required_members.join("\n")
        ))
    }

    fn split_object_members(inner: &str) -> Vec<String> {
        let mut members = Vec::new();
        let mut start = 0usize;
        for idx in Self::top_level_byte_indices(inner, b';') {
            let member = inner.get(start..idx).map(str::trim).unwrap_or_default();
            if !member.is_empty() {
                members.push(member.to_string());
            }
            start = idx + 1;
        }
        let tail = inner.get(start..).map(str::trim).unwrap_or_default();
        if !tail.is_empty() {
            members.push(tail.to_string());
        }
        members
    }

    fn parse_object_property_member(member: &str) -> Option<(&str, bool, &str)> {
        let colon = Self::top_level_byte_indices(member, b':')
            .into_iter()
            .next()?;
        let name = member.get(..colon)?.trim();
        let type_text = member.get(colon + 1..)?.trim();
        let explicit_optional = name.ends_with('?');
        Some((name, explicit_optional, type_text))
    }

    fn type_text_contains_undefined(type_text: &str) -> bool {
        let bytes = type_text.as_bytes();
        let needle = b"undefined";
        let mut i = 0usize;
        while i + needle.len() <= bytes.len() {
            if &bytes[i..i + needle.len()] == needle {
                let before_ok = i == 0 || !Self::is_ident_char(bytes[i - 1]);
                let after = i + needle.len();
                let after_ok = after == bytes.len() || !Self::is_ident_char(bytes[after]);
                if before_ok && after_ok {
                    return true;
                }
                i += needle.len();
            } else {
                i += 1;
            }
        }
        false
    }

    pub(in crate::declaration_emitter) fn type_text_has_undefined_branch(type_text: &str) -> bool {
        let mut text = type_text.trim();
        while let Some(inner) = Self::strip_balanced_outer_parens(text) {
            text = inner.trim();
        }

        if text == "undefined" {
            return true;
        }

        let union_indices = Self::top_level_byte_indices(text, b'|');
        if union_indices.is_empty() {
            return false;
        }

        let mut start = 0usize;
        for index in union_indices {
            if Self::type_text_has_undefined_branch(&text[start..index]) {
                return true;
            }
            start = index + 1;
        }
        Self::type_text_has_undefined_branch(&text[start..])
    }

    pub(in crate::declaration_emitter) fn type_annotation_semantically_includes_undefined(
        &self,
        type_annotation: NodeIndex,
    ) -> bool {
        type_annotation.is_some()
            && (self
                .emit_type_node_text(type_annotation)
                .is_some_and(|type_text| self.type_text_or_alias_includes_undefined(&type_text, 0))
                || self.type_node_semantically_includes_undefined(type_annotation, 0))
    }

    pub(in crate::declaration_emitter) fn emitted_type_text_semantically_includes_undefined(
        &self,
        type_text: &str,
    ) -> bool {
        self.type_text_or_alias_includes_undefined(type_text, 0)
    }

    fn type_text_or_alias_includes_undefined(&self, type_text: &str, depth: usize) -> bool {
        if depth > 8 {
            return false;
        }
        let mut text = type_text.trim();
        while let Some(inner) = Self::strip_balanced_outer_parens(text) {
            text = inner.trim();
        }

        if text == "undefined" {
            return true;
        }

        let union_indices = Self::top_level_byte_indices(text, b'|');
        if !union_indices.is_empty() {
            let mut start = 0usize;
            for index in union_indices {
                if self.type_text_or_alias_includes_undefined(&text[start..index], depth + 1) {
                    return true;
                }
                start = index + 1;
            }
            return self.type_text_or_alias_includes_undefined(&text[start..], depth + 1);
        }

        if let Some((name, args)) = Self::parse_utility_type_text(text) {
            return match name {
                "Exclude" => {
                    let first_includes_undefined = args.first().is_some_and(|arg| {
                        self.type_text_or_alias_includes_undefined(arg, depth + 1)
                    });
                    let excluded_includes_undefined = args.get(1).is_some_and(|arg| {
                        self.type_text_or_alias_includes_undefined(arg, depth + 1)
                    });
                    first_includes_undefined && !excluded_includes_undefined
                }
                "Extract" => {
                    args.first().is_some_and(|arg| {
                        self.type_text_or_alias_includes_undefined(arg, depth + 1)
                    }) && args.get(1).is_some_and(|arg| {
                        self.type_text_or_alias_includes_undefined(arg, depth + 1)
                    })
                }
                _ => false,
            };
        }
        if Self::is_simple_identifier_text(text) {
            return self
                .find_local_type_alias_type_node(text)
                .or_else(|| self.current_file_type_alias_type_node_by_name(text))
                .and_then(|alias_type| self.emit_type_node_text(alias_type))
                .is_some_and(|alias_text| {
                    self.type_text_or_alias_includes_undefined(&alias_text, depth + 1)
                });
        }
        false
    }

    fn parse_utility_type_text(text: &str) -> Option<(&str, Vec<&str>)> {
        let lt = text.find('<')?;
        if !text.ends_with('>') {
            return None;
        }
        let name = text[..lt].trim();
        if !matches!(name, "Exclude" | "Extract") {
            return None;
        }
        let inner = &text[lt + 1..text.len() - 1];
        Some((name, Self::split_top_level_commas(inner)))
    }

    pub(in crate::declaration_emitter) fn type_node_semantically_includes_undefined(
        &self,
        type_idx: NodeIndex,
        depth: usize,
    ) -> bool {
        if depth > 8 {
            return false;
        }

        let Some(type_node) = self.arena.get(type_idx) else {
            return false;
        };
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = self.arena.get_type_ref(type_node) else {
            return false;
        };
        let Some(name) = self.identifier_text_from_arena(self.arena, type_ref.type_name) else {
            return false;
        };

        match name.as_str() {
            "NonNullable" => false,
            "Exclude" => {
                let Some(args) = type_ref.type_arguments.as_ref() else {
                    return false;
                };
                let first_includes_undefined =
                    args.nodes.first().copied().is_some_and(|arg| {
                        self.type_node_or_alias_includes_undefined(arg, depth + 1)
                    });
                let excluded_includes_undefined =
                    args.nodes.get(1).copied().is_some_and(|arg| {
                        self.type_node_or_alias_includes_undefined(arg, depth + 1)
                    });
                first_includes_undefined && !excluded_includes_undefined
            }
            "Extract" => {
                let Some(args) = type_ref.type_arguments.as_ref() else {
                    return false;
                };
                args.nodes
                    .first()
                    .copied()
                    .is_some_and(|arg| self.type_node_or_alias_includes_undefined(arg, depth + 1))
                    && args.nodes.get(1).copied().is_some_and(|arg| {
                        self.type_node_or_alias_includes_undefined(arg, depth + 1)
                    })
            }
            _ => self
                .find_local_type_alias_type_node(&name)
                .or_else(|| self.current_file_type_alias_type_node_by_name(&name))
                .is_some_and(|alias_type| {
                    self.type_node_or_alias_includes_undefined(alias_type, depth + 1)
                }),
        }
    }

    fn current_file_type_alias_type_node_by_name(&self, name: &str) -> Option<NodeIndex> {
        let source_file = self
            .current_source_file_idx
            .and_then(|idx| self.arena.get(idx))
            .and_then(|node| self.arena.get_source_file(node))?;
        for &stmt_idx in &source_file.statements.nodes {
            let stmt_node = self.arena.get(stmt_idx)?;
            let alias = self.arena.get_type_alias(stmt_node)?;
            if self.get_identifier_text(alias.name).as_deref() == Some(name) {
                return Some(alias.type_node);
            }
        }
        None
    }

    fn type_node_or_alias_includes_undefined(&self, type_idx: NodeIndex, depth: usize) -> bool {
        self.emit_type_node_text(type_idx)
            .is_some_and(|type_text| Self::type_text_has_undefined_branch(&type_text))
            || self.type_node_semantically_includes_undefined(type_idx, depth)
    }

    fn strip_balanced_outer_parens(text: &str) -> Option<&str> {
        let bytes = text.as_bytes();
        if bytes.first() != Some(&b'(') || bytes.last() != Some(&b')') {
            return None;
        }

        let mut depth = 0usize;
        let mut quote: Option<u8> = None;
        let mut escaped = false;

        for (i, &byte) in bytes.iter().enumerate() {
            if let Some(q) = quote {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == q {
                    quote = None;
                }
                continue;
            }

            match byte {
                b'\'' | b'"' | b'`' => quote = Some(byte),
                b'(' => depth += 1,
                b')' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 && i != bytes.len() - 1 {
                        return None;
                    }
                }
                _ => {}
            }
        }

        (depth == 0).then_some(&text[1..text.len() - 1])
    }

    fn skip_ascii_whitespace(text: &str, start: usize) -> Option<usize> {
        let bytes = text.as_bytes();
        let mut i = start;
        while i < bytes.len() && (bytes[i] as char).is_ascii_whitespace() {
            i += 1;
        }
        Some(i)
    }

    fn top_level_byte_indices(text: &str, target: u8) -> Vec<usize> {
        let bytes = text.as_bytes();
        let mut indices = Vec::new();
        let mut brace_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut paren_depth = 0usize;
        let mut angle_depth = 0usize;
        let mut quote: Option<u8> = None;
        let mut i = 0usize;
        while i < bytes.len() {
            let b = bytes[i];
            if let Some(q) = quote {
                if b == b'\\' {
                    i = (i + 2).min(bytes.len());
                    continue;
                }
                if b == q {
                    quote = None;
                }
                i += 1;
                continue;
            }

            match b {
                b'\'' | b'"' | b'`' => quote = Some(b),
                b'{' => brace_depth += 1,
                b'}' => brace_depth = brace_depth.saturating_sub(1),
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b'(' => paren_depth += 1,
                b')' => paren_depth = paren_depth.saturating_sub(1),
                b'<' => angle_depth += 1,
                b'>' => angle_depth = angle_depth.saturating_sub(1),
                _ if b == target
                    && brace_depth == 0
                    && bracket_depth == 0
                    && paren_depth == 0
                    && angle_depth == 0 =>
                {
                    indices.push(i);
                }
                _ => {}
            }
            i += 1;
        }
        indices
    }

    fn balanced_brace_end(text: &str, start: usize) -> Option<usize> {
        let bytes = text.as_bytes();
        if bytes.get(start).copied() != Some(b'{') {
            return None;
        }
        let mut depth = 0usize;
        let mut quote: Option<u8> = None;
        let mut i = start;
        while i < bytes.len() {
            let b = bytes[i];
            if let Some(q) = quote {
                if b == b'\\' {
                    i = (i + 2).min(bytes.len());
                    continue;
                }
                if b == q {
                    quote = None;
                }
                i += 1;
                continue;
            }

            match b {
                b'\'' | b'"' | b'`' => quote = Some(b),
                b'{' => depth += 1,
                b'}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(i + 1);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }
}
