//! Node text utilities for TC39 decorator emission.

use super::TC39DecoratorEmitter;
#[allow(unused_imports)]
use super::helpers::*;
#[allow(unused_imports)]
use super::{
    AutoAccessorClassCtx, AutoAccessorMemberEmitCtx, ClassBodyCtx, ClassBodyFlags,
    ClassDecoratorInstancePrivateFieldInfo, ClassDecoratorVars, CtorInitFlags, CtorMembersCtx,
    CtorOutputCtx, DecoratorApplicationCtx, DecoratorReceiverState, EsDecorateMemberCtx,
    EsDecorateVars, PlainComputedInstanceFieldInfo,
};
#[allow(unused_imports)]
use crate::transforms::emit_utils::hygienic_temp_name;
#[allow(unused_imports)]
use rustc_hash::FxHashMap;
#[allow(unused_imports)]
use tsz_parser::parser::node::{NodeAccess, NodeArena};
#[allow(unused_imports)]
use tsz_parser::parser::syntax_kind_ext;
#[allow(unused_imports)]
use tsz_parser::parser::{NodeIndex, NodeList};
#[allow(unused_imports)]
use tsz_scanner::SyntaxKind;

impl<'a> TC39DecoratorEmitter<'a> {
    pub(super) fn has_user_static_members(&self, members: &NodeList) -> bool {
        for &idx in &members.nodes {
            let Some(node) = self.arena.get(idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                return true;
            }
            if node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                && let Some(prop) = self.arena.get_property_decl(node)
                && self.arena.is_static(&prop.modifiers)
            {
                return true;
            }
        }
        false
    }

    pub(super) fn plain_static_field_assignment(
        &self,
        member_node: &tsz_parser::parser::node::Node,
        class_ref: &str,
        indent: &str,
    ) -> Option<String> {
        if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
            return None;
        }
        let prop = self.arena.get_property_decl(member_node)?;
        if !self.arena.is_static(&prop.modifiers)
            || self
                .arena
                .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
            || self
                .arena
                .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
            || self
                .arena
                .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
        {
            return None;
        }

        let (property_access, property_key) = self.static_field_assignment_name(prop.name)?;
        let value = if prop.initializer.is_some() {
            if class_ref == "_classThis" && self.node_is_this_keyword(prop.initializer) {
                class_ref.to_string()
            } else {
                self.node_text(prop.initializer)
            }
        } else {
            "void 0".to_string()
        };

        if self.use_define_for_class_fields {
            Some(format!(
                "Object.defineProperty({class_ref}, {property_key}, {{\n{indent}    enumerable: true,\n{indent}    configurable: true,\n{indent}    writable: true,\n{indent}    value: {value}\n{indent}}})"
            ))
        } else {
            Some(format!("{class_ref}{property_access} = {value}"))
        }
    }

    pub(super) fn static_field_assignment_name(
        &self,
        name_idx: NodeIndex,
    ) -> Option<(String, String)> {
        let name_node = self.arena.get(name_idx)?;
        match name_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                let name = self
                    .arena
                    .get_identifier(name_node)
                    .map(|id| id.escaped_text.clone())?;
                Some((format!(".{name}"), format!("\"{name}\"")))
            }
            k if k == SyntaxKind::PrivateIdentifier as u16 => None,
            k if k == SyntaxKind::StringLiteral as u16 => {
                let name_text = self.node_text(name_idx);
                if name_text.is_empty() {
                    None
                } else {
                    Some((format!("[{name_text}]"), name_text))
                }
            }
            k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                let computed = self.arena.get_computed_property(name_node)?;
                let key = self.node_text(computed.expression);
                if key.is_empty() {
                    None
                } else {
                    Some((format!("[{key}]"), key))
                }
            }
            _ => {
                let key = self.node_text(name_idx);
                if key.is_empty() {
                    None
                } else {
                    Some((format!("[{key}]"), key))
                }
            }
        }
    }

    pub(super) fn node_is_this_keyword(&self, idx: NodeIndex) -> bool {
        self.arena
            .get(idx)
            .is_some_and(|node| node.kind == SyntaxKind::ThisKeyword as u16)
    }

    pub(super) fn lower_static_block_text_to_iife(&self, text: &str) -> Option<String> {
        let trimmed = text.trim().trim_end_matches(';').trim();
        if !trimmed.starts_with("static") {
            return Some(trimmed.to_string());
        }
        let open = trimmed.find('{')?;
        let close = trimmed.rfind('}')?;
        if close <= open {
            return None;
        }
        let body = trimmed[open + 1..close].trim();
        let body = body
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n        ");
        if body.is_empty() {
            Some("(() => {\n    })()".to_string())
        } else {
            Some(format!("(() => {{\n        {body}\n    }})()"))
        }
    }

    /// Find the position of the class closing brace by scanning forward from the
    /// class body opening `{`, tracking brace depth.
    pub(super) fn find_class_close_brace(
        &self,
        class_node: &tsz_parser::parser::node::Node,
    ) -> usize {
        let Some(source) = self.source_text else {
            return class_node.end as usize;
        };
        let bytes = source.as_bytes();
        let start = class_node.pos as usize;
        let end = source.len().min(class_node.end as usize + 100); // generous bound

        // Find the opening `{` of the class body
        let mut pos = start;
        while pos < end && bytes[pos] != b'{' {
            pos += 1;
        }
        if pos >= end {
            return class_node.end as usize;
        }

        // Track brace depth from the opening `{`
        let mut depth: u32 = 0;
        let mut in_string = false;
        let mut string_char: u8 = 0;
        let mut in_template = false;
        let mut template_depth: u32 = 0;

        while pos < end {
            let ch = bytes[pos];
            if in_string {
                if ch == b'\\' {
                    pos += 1; // skip escape
                } else if ch == string_char {
                    in_string = false;
                }
            } else if in_template {
                if ch == b'\\' {
                    pos += 1;
                } else if ch == b'`' {
                    in_template = false;
                } else if ch == b'$' && pos + 1 < end && bytes[pos + 1] == b'{' {
                    template_depth += 1;
                    pos += 1;
                }
            } else {
                match ch {
                    b'\'' | b'"' => {
                        in_string = true;
                        string_char = ch;
                    }
                    b'`' => in_template = true,
                    b'{' => depth += 1,
                    b'}' => {
                        if template_depth > 0 {
                            template_depth -= 1;
                        } else {
                            depth -= 1;
                            if depth == 0 {
                                return pos; // position of the closing `}`
                            }
                        }
                    }
                    _ => {}
                }
            }
            pos += 1;
        }
        class_node.end as usize
    }

    /// Emit a single member with decorators stripped, bounded by the next member's start.
    /// Uses AST positions for the clean start and the next member's position as end boundary.
    pub(super) fn emit_member_bounded(
        &self,
        member_node: &tsz_parser::parser::node::Node,
        next_boundary: usize,
    ) -> String {
        let Some(source) = self.source_text else {
            return String::new();
        };

        let clean_start = self.find_member_clean_start(member_node);
        // Use member.end as the primary boundary, clamped by next_boundary
        let raw_end = std::cmp::min(member_node.end as usize, next_boundary);

        if clean_start < source.len() && raw_end <= source.len() && clean_start < raw_end {
            let mut text = source[clean_start..raw_end].trim();
            // Strip class closing brace that may leak into last member's text.
            // The parser sets member.end to include trailing trivia up to the class `}`.
            // Detect: a trailing `}` separated from member content by whitespace containing newline.
            if text.ends_with('}') {
                let before = &text[..text.len() - 1];
                let trimmed = before.trim_end();
                if trimmed.ends_with('}') && before.contains('\n') {
                    text = trimmed;
                }
            }
            // Strip TS type annotations from copied member syntax.
            let text = strip_member_type_annotations(text);
            let text = normalize_member_indentation(&text);
            let text = text.as_str();
            // Normalize empty method bodies: `{}` -> `{ }`
            if let Some(stripped) = text.strip_suffix("{}") {
                format!("{stripped}{{ }}")
            } else {
                text.to_string()
            }
        } else {
            String::new()
        }
    }

    /// Find the position in source text where the "clean" (non-decorator, non-TS-modifier)
    /// part of a class member begins.
    pub(super) fn find_member_clean_start(
        &self,
        member_node: &tsz_parser::parser::node::Node,
    ) -> usize {
        let (modifiers, name_idx) = match member_node.kind {
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let data = self.arena.get_method_decl(member_node);
                (
                    data.as_ref().and_then(|m| m.modifiers.clone()),
                    data.map(|m| m.name),
                )
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                let data = self.arena.get_property_decl(member_node);
                (
                    data.as_ref().and_then(|p| p.modifiers.clone()),
                    data.map(|p| p.name),
                )
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                let data = self.arena.get_accessor(member_node);
                (
                    data.as_ref().and_then(|a| a.modifiers.clone()),
                    data.map(|a| a.name),
                )
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                let data = self.arena.get_accessor(member_node);
                (
                    data.as_ref().and_then(|a| a.modifiers.clone()),
                    data.map(|a| a.name),
                )
            }
            _ => (None, None),
        };

        let Some(mods) = modifiers else {
            return member_node.pos as usize;
        };

        let ts_only_kinds: &[u16] = &[
            SyntaxKind::AbstractKeyword as u16,
            SyntaxKind::DeclareKeyword as u16,
            SyntaxKind::ReadonlyKeyword as u16,
            SyntaxKind::OverrideKeyword as u16,
            SyntaxKind::PublicKeyword as u16,
            SyntaxKind::PrivateKeyword as u16,
            SyntaxKind::ProtectedKeyword as u16,
            SyntaxKind::AccessorKeyword as u16,
        ];

        // Find the first JS-visible modifier (static, async, etc.)
        for &mod_idx in &mods.nodes {
            let Some(mod_node) = self.arena.get(mod_idx) else {
                continue;
            };
            if mod_node.kind != syntax_kind_ext::DECORATOR
                && !ts_only_kinds.contains(&mod_node.kind)
            {
                // JS-visible modifier - start from its position
                return mod_node.pos as usize;
            }
        }

        // All modifiers are decorators/TS-only.
        // Use the name node position as the reliable anchor, but for GET_ACCESSOR
        // and SET_ACCESSOR we must include the `get`/`set` keyword which precedes
        // the name in the source text and is NOT stored as a modifier.
        if let Some(idx) = name_idx
            && let Some(name_node) = self.arena.get(idx)
        {
            let name_pos = name_node.pos as usize;
            let is_accessor = member_node.kind == syntax_kind_ext::GET_ACCESSOR
                || member_node.kind == syntax_kind_ext::SET_ACCESSOR;
            if is_accessor && let Some(source) = self.source_text {
                // Scan backwards from name position to find 'get' or 'set' keyword
                let keyword = if member_node.kind == syntax_kind_ext::GET_ACCESSOR {
                    "get"
                } else {
                    "set"
                };
                // Allow generous whitespace between keyword and name
                let search_start = name_pos.saturating_sub(keyword.len() + 20);
                // Look for the keyword in the text before the name
                if let Some(kw_offset) = source[search_start..name_pos].rfind(keyword) {
                    return search_start + kw_offset;
                }
            }
            return name_pos;
        }

        member_node.pos as usize
    }

    /// Find the position of the matching `]` for a string starting after `[`.
    /// Returns the index of `]` within the input string, handling nested brackets.
    pub(super) fn find_matching_bracket(&self, s: &str) -> Option<usize> {
        let mut depth = 1;
        for (i, ch) in s.char_indices() {
            match ch {
                '[' => depth += 1,
                ']' => {
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

    pub(super) fn get_identifier_text(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            self.arena
                .get_identifier(node)
                .map(|id| id.escaped_text.clone())
        } else {
            None
        }
    }

    pub(super) fn node_text(&self, idx: NodeIndex) -> String {
        let Some(node) = self.arena.get(idx) else {
            return String::new();
        };
        let Some(source) = self.source_text else {
            return String::new();
        };
        let start = node.pos as usize;
        let end = node.end as usize;
        if start < source.len() && end <= source.len() && start < end {
            source[start..end].trim().to_string()
        } else {
            String::new()
        }
    }
}
