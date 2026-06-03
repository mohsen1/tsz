use crate::emitter::Printer;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn is_runtime_module_indicating_statement(
        &self,
        stmt_node: &Node,
        is_erased: bool,
    ) -> bool {
        if is_erased {
            return false;
        }

        let kind = stmt_node.kind;
        if matches!(
            kind,
            syntax_kind_ext::IMPORT_DECLARATION
                | syntax_kind_ext::EXPORT_DECLARATION
                | syntax_kind_ext::EXPORT_ASSIGNMENT
        ) {
            return true;
        }

        if kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            return false;
        }

        // External module imports (`import x = require("mod")`) and exported
        // aliases count as runtime module syntax. Plain namespace aliases
        // (`import x = M.A`) are erased and should not suppress deferred
        // `export {};` emission.
        self.arena
            .get_import_decl(stmt_node)
            .is_some_and(|import_data| {
                self.arena
                    .has_modifier(&import_data.modifiers, SyntaxKind::ExportKeyword)
                    || self
                        .arena
                        .get(import_data.module_specifier)
                        .is_some_and(|spec_node| {
                            spec_node.is_string_literal()
                                || spec_node.kind == syntax_kind_ext::EXTERNAL_MODULE_REFERENCE
                        })
            })
    }
}
