use super::DeclarationEmitter;

use super::helpers::{escape_string_for_double_quote, escape_string_for_single_quote};

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::NodeAccess;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

/// Re-escape a cooked template literal string so it can be placed back
/// between backticks.  The parser stores the *cooked* (processed) value in
/// `LiteralData::text`, so characters like `\n` have already been converted
/// to a real newline.  This function converts them back to escape sequences.
fn escape_template_literal_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '`' => out.push_str("\\`"),
            '$' if chars.peek() == Some(&'{') => out.push_str("\\$"),
            '$' => out.push('$'),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c => out.push(c),
        }
    }
    out
}

include!("type_emission_parts/part1.rs");
include!("type_emission_parts/part2.rs");

#[cfg(test)]
mod tests {
    use super::DeclarationEmitter;

    #[test]
    fn mapped_type_source_text_splits_compact_as_clause_after_indexed_access() {
        assert_eq!(
            DeclarationEmitter::mapped_type_constraint_source_text("T[number]as Item[Attr]"),
            "T[number]"
        );
        assert_eq!(
            DeclarationEmitter::mapped_type_constraint_source_text("T[number] as"),
            "T[number]"
        );
        assert_eq!(
            DeclarationEmitter::mapped_type_constraint_source_text("keyof T as"),
            "keyof T"
        );
        assert_eq!(
            DeclarationEmitter::mapped_type_name_source_text("T[number]as Item[Attr]"),
            "Item[Attr]"
        );
        assert_eq!(
            DeclarationEmitter::mapped_type_name_source_text("as `get${Capitalize<string & K>}`"),
            "`get${Capitalize<string & K>}`"
        );
        assert_eq!(
            DeclarationEmitter::mapped_type_name_source_text(
                "as as `get${Capitalize<string & K>}`"
            ),
            "`get${Capitalize<string & K>}`"
        );
        assert_eq!(
            DeclarationEmitter::mapped_type_name_source_text("asserts T"),
            "asserts T"
        );
        assert_eq!(
            DeclarationEmitter::mapped_type_constraint_source_text("keyof T]"),
            "keyof T"
        );
    }
}
