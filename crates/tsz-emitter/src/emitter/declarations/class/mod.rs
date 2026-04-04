#![allow(clippy::nonminimal_bool, clippy::type_complexity)]

mod decorators;
mod emit_declaration;
mod emit_es6;
mod helpers;

use super::super::core::PropertyNameEmit;
use tsz_parser::parser::NodeIndex;

/// Entry for a static field initializer that will be emitted after the class body.
/// Fields: (name, initializer node, member pos, leading comments with source pos, trailing comments)
pub(in crate::emitter) type StaticFieldInit = (
    PropertyNameEmit,
    NodeIndex,
    u32,
    Vec<(String, u32)>,
    Vec<String>,
);
pub(in crate::emitter) type AutoAccessorInfo = (NodeIndex, String, Option<NodeIndex>, bool);

/// Replace all occurrences of an identifier with a replacement, respecting word boundaries.
pub(crate) fn replace_identifier(text: &str, name: &str, replacement: &str) -> String {
    let bytes = text.as_bytes();
    let name_bytes = name.as_bytes();
    let name_len = name_bytes.len();
    let mut result = String::with_capacity(text.len());
    let mut i = 0;
    while i + name_len <= bytes.len() {
        if let Some(pos) = text[i..].find(name) {
            let abs = i + pos;
            let before_ok = abs == 0
                || !(bytes[abs - 1].is_ascii_alphanumeric()
                    || bytes[abs - 1] == b'_'
                    || bytes[abs - 1] == b'$');
            let after_end = abs + name_len;
            let after_ok = after_end >= bytes.len()
                || !(bytes[after_end].is_ascii_alphanumeric()
                    || bytes[after_end] == b'_'
                    || bytes[after_end] == b'$');
            result.push_str(&text[i..abs]);
            if before_ok && after_ok {
                result.push_str(replacement);
            } else {
                result.push_str(name);
            }
            i = after_end;
        } else {
            result.push_str(&text[i..]);
            return result;
        }
    }
    result.push_str(&text[i..]);
    result
}

/// Check if a class body contains self-references to the class name.
/// This is needed for the `C_1` alias pattern in legacy decorator lowering.
/// When a decorated class references itself (e.g. `static x() { return C.y; }`),
/// tsc creates an alias `var C_1;` so the decorator can replace the class binding
/// without breaking internal references.
///
/// Uses source text scanning within member spans to detect references.
pub(super) fn class_has_self_references(
    arena: &tsz_parser::parser::node::NodeArena,
    source_text: Option<&str>,
    class_name: &str,
    members: &[NodeIndex],
) -> bool {
    if class_name.is_empty() {
        return false;
    }
    let Some(src) = source_text else {
        return false;
    };

    for &member_idx in members {
        let Some(member_node) = arena.get(member_idx) else {
            continue;
        };

        // Get the span of the member body/initializer
        let body_span = match member_node.kind {
            k if k == tsz_parser::parser::syntax_kind_ext::METHOD_DECLARATION => {
                let Some(method) = arena.get_method_decl(member_node) else {
                    continue;
                };
                arena
                    .get(method.body)
                    .map(|n| (n.pos as usize, n.end as usize))
            }
            k if k == tsz_parser::parser::syntax_kind_ext::PROPERTY_DECLARATION => {
                let Some(prop) = arena.get_property_decl(member_node) else {
                    continue;
                };
                arena
                    .get(prop.initializer)
                    .map(|n| (n.pos as usize, n.end as usize))
            }
            k if k == tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR
                || k == tsz_parser::parser::syntax_kind_ext::SET_ACCESSOR =>
            {
                let Some(acc) = arena.get_accessor(member_node) else {
                    continue;
                };
                arena
                    .get(acc.body)
                    .map(|n| (n.pos as usize, n.end as usize))
            }
            _ => continue,
        };

        if let Some((start, end)) = body_span
            && start < end
            && end <= src.len()
        {
            let body_text = &src[start..end];
            if text_contains_identifier(body_text, class_name) {
                return true;
            }
        }
    }
    false
}

/// Check if `text` contains `name` as an identifier (word boundary match).
fn text_contains_identifier(text: &str, name: &str) -> bool {
    let name_bytes = name.as_bytes();
    let text_bytes = text.as_bytes();
    let name_len = name_bytes.len();
    if name_len == 0 || text_bytes.len() < name_len {
        return false;
    }
    let mut i = 0;
    while i + name_len <= text_bytes.len() {
        if &text_bytes[i..i + name_len] == name_bytes {
            let before_ok = i == 0 || !is_ident_char(text_bytes[i - 1]);
            let after_ok =
                i + name_len == text_bytes.len() || !is_ident_char(text_bytes[i + name_len]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

pub(super) const fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

#[cfg(test)]
#[path = "../../../../tests/declarations_class.rs"]
mod tests;
