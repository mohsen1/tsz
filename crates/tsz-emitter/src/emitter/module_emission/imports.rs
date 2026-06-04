use super::super::{JsxEmit, ModuleKind, Printer};

use tsz_parser::parser::node::Node;

use tsz_parser::parser::syntax_kind_ext;

use tsz_parser::parser::{NodeIndex, NodeList};

use tsz_scanner::SyntaxKind;

include!("imports_parts/part1.rs");
include!("imports_parts/part2.rs");

#[cfg(test)]
mod tests {
    #[test]
    fn import_alias_redeclaration_requires_import_equals() {
        assert!(
            crate::import_usage::contains_identifier_occurrence_before_shadow(
                "import M = Z.I;\nM.bar();",
                "M",
            )
        );
        assert!(
            !crate::import_usage::contains_identifier_occurrence_before_shadow(
                "import M from \"pkg\";\nM.bar();",
                "M",
            )
        );
    }
}
