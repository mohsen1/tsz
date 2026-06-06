//! Display-payload construction for signature help.
//!
//! This module handles computing the applicable span for a call expression and
//! applying type-parameter substitutions to assembled `SignatureInformation`
//! values before they are returned to the LSP client.

use super::{SignatureHelpProvider, SignatureInformation};
use tsz_parser::{NodeIndex, parser::node::CallExprData};

impl<'a> SignatureHelpProvider<'a> {
    /// Compute the applicable span for a regular call expression.
    /// Returns (`start_offset`, length) as byte offsets in the source text.
    pub(super) fn compute_applicable_span(
        &self,
        call_idx: NodeIndex,
        data: &CallExprData,
    ) -> (u32, u32) {
        let call_node = match self.arena.get(call_idx) {
            Some(n) => n,
            None => return (0, 0),
        };
        let call_start = call_node.pos as usize;
        let call_end = (call_node.end as usize).min(self.source_text.len());
        let call_text = &self.source_text[call_start..call_end];

        // Find opening paren
        let paren_rel = match call_text.find('(') {
            Some(p) => p,
            None => return (call_node.pos, 0),
        };
        let after_paren = (call_start + paren_rel + 1) as u32;

        // If there are arguments, span from after '(' to before ')'
        if let Some(ref args) = data.arguments
            && !args.nodes.is_empty()
        {
            let first_start = args
                .nodes
                .first()
                .and_then(|&idx| self.arena.get(idx))
                .map_or(after_paren, |n| n.pos);
            let last_end = args
                .nodes
                .last()
                .and_then(|&idx| self.arena.get(idx))
                .map_or(after_paren, |n| n.end);
            return (first_start, last_end.saturating_sub(first_start));
        }

        // No arguments — zero-length span at after-paren position
        (after_paren, 0)
    }
}

/// Apply type parameter substitution to a `SignatureInformation`, replacing each
/// type parameter name with its resolved substitution (default type, constraint
/// type, or `unknown`) in parameter labels, prefix, suffix, and the full label.
pub(crate) fn apply_type_param_substitution(
    info: &mut SignatureInformation,
    type_param_substitutions: &[(String, String)],
) {
    // Substitute in each parameter label
    for param in &mut info.parameters {
        param.label = substitute_type_params(&param.label, type_param_substitutions);
    }
    // Substitute in suffix (contains return type)
    info.suffix = substitute_type_params(&info.suffix, type_param_substitutions);
    // Rebuild full label from prefix + substituted param labels + substituted suffix
    let param_labels: Vec<&str> = info.parameters.iter().map(|p| p.label.as_str()).collect();
    info.label = format!("{}{}{}", info.prefix, param_labels.join(", "), info.suffix);
}

/// Substitute occurrences of type parameter names with their resolved
/// substitution text in a formatted type string. Uses word-boundary-aware
/// replacement so that e.g. type param `T` does not replace the `T` inside
/// `Tuple`.
pub(crate) fn substitute_type_params(
    s: &str,
    type_param_substitutions: &[(String, String)],
) -> String {
    let mut result = s.to_string();
    for (name, substitution) in type_param_substitutions {
        // Replace whole-word occurrences of the type parameter name with its
        // substitution. A "word boundary" here means the character before/after
        // is not alphanumeric or underscore (matching TypeScript identifier
        // characters).
        let mut out = String::with_capacity(result.len());
        let name_len = name.len();
        let bytes = result.as_bytes();
        let len = bytes.len();
        let mut i = 0;
        while i < len {
            if i + name_len <= len && &result[i..i + name_len] == name.as_str() {
                let before_ok = i == 0 || !is_ident_char(bytes[i - 1]);
                let after_ok = i + name_len == len || !is_ident_char(bytes[i + name_len]);
                if before_ok && after_ok {
                    out.push_str(substitution);
                    i += name_len;
                    continue;
                }
            }
            out.push(bytes[i] as char);
            i += 1;
        }
        result = out;
    }
    result
}

#[inline]
pub(crate) const fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}
