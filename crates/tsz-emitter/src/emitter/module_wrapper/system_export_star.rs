use super::super::Printer;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> Printer<'a> {
    pub(super) fn collect_system_export_star_excluded_names(
        &self,
        source: &tsz_parser::parser::node::SourceFileData,
    ) -> (Vec<String>, bool) {
        let type_only_nodes = rustc_hash::FxHashSet::default();
        let mut names = crate::transforms::module_commonjs::collect_export_names_with_options(
            self.arena,
            &source.statements.nodes,
            self.ctx.options.preserve_const_enums,
            &type_only_nodes,
        );
        let has_explicit_export_name = !names.is_empty();
        names.retain(|name| name != "default");
        let needs_empty_map = has_explicit_export_name
            || self.source_has_system_hoisted_default_function_export(source);
        (names, needs_empty_map)
    }

    fn source_has_system_hoisted_default_function_export(
        &self,
        source: &tsz_parser::parser::node::SourceFileData,
    ) -> bool {
        source.statements.nodes.iter().any(|&stmt_idx| {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                return false;
            };
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                return false;
            };
            let Some(export_decl) = self.arena.get_export_decl(stmt_node) else {
                return false;
            };
            if !export_decl.is_default_export {
                return false;
            }
            self.arena
                .get(export_decl.export_clause)
                .is_some_and(|clause_node| {
                    clause_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                })
        })
    }
}
