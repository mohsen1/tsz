//! Property-name quoting helpers for type and diagnostic display.

use std::borrow::Cow;

/// Format the property-name slot used by excess-property diagnostics.
///
/// The full diagnostic already wraps this value in single quotes. For property
/// names that cannot be displayed as identifier-like names, `tsc` preserves an
/// inner double-quoted property name, e.g. `'"data-id"'`.
pub fn format_excess_property_name(name: &str) -> Cow<'_, str> {
    if !needs_property_name_quotes(name) {
        return Cow::Borrowed(name);
    }
    let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
    Cow::Owned(format!("\"{escaped}\""))
}

/// Returns `true` if a property name needs to be quoted in type display
/// (i.e. it is not a valid JS identifier or numeric literal).
pub(crate) fn needs_property_name_quotes(name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    // Computed property names wrapped in brackets (e.g. [Symbol.asyncIterator])
    // are displayed as-is without quotes, matching tsc behavior.
    if name.starts_with('[') && name.ends_with(']') {
        return false;
    }
    // Numeric property names don't need quotes. This includes integer-only
    // forms (`19230`) as well as canonical JS-numeric forms with decimals
    // (`3.14`), exponents (`5.462437423415177e+244`), or signs (`-1`), all
    // of which match `Number.prototype.toString()` round-trip and are
    // displayed unquoted by tsc in object type literals.
    if crate::utils::is_numeric_literal_name(name) {
        return false;
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' || first == '$' => {
            !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
        }
        _ => true,
    }
}
