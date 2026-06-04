use super::super::DeclarationEmitter;

use tsz_parser::parser::node::{CallExprData, ClassData, MethodDeclData, NodeAccess, NodeArena};

use tsz_parser::parser::syntax_kind_ext;

use tsz_parser::parser::{NodeIndex, NodeList};

use tsz_scanner::SyntaxKind;

include!("type_inference_return_normalization_parts/part1.rs");
include!("type_inference_return_normalization_parts/part2.rs");

#[cfg(test)]
mod tests {
    use super::DeclarationEmitter;

    #[test]
    fn exact_return_member_rewrite_preserves_indentation() {
        let source = "{\n    value: unknown;\n    other: unknown;\n}";
        let rewrites = vec![("value: unknown;".to_string(), "value: string;".to_string())];

        let rewritten = DeclarationEmitter::rewrite_exact_return_member_lines(source, &rewrites);

        assert_eq!(rewritten, "{\n    value: string;\n    other: unknown;\n}");
    }

    #[test]
    fn exact_return_member_rewrite_does_not_touch_partial_matches() {
        let source = "{\n    value: unknown;\n    nested: { value: unknown; };\n}";
        let rewrites = vec![("value: unknown;".to_string(), "value: number;".to_string())];

        let rewritten = DeclarationEmitter::rewrite_exact_return_member_lines(source, &rewrites);

        assert_eq!(
            rewritten,
            "{\n    value: number;\n    nested: { value: unknown; };\n}"
        );
    }
}
