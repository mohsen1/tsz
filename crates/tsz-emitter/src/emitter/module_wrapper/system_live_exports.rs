use super::super::{ModuleKind, Printer};
use rustc_hash::FxHashMap;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

fn push_system_reexport_name(
    reexported_name_lists: &mut FxHashMap<String, Vec<String>>,
    local_name: String,
    export_name: String,
) {
    let names = reexported_name_lists.entry(local_name).or_default();
    if !names.contains(&export_name) {
        names.push(export_name);
    }
}

impl<'a> Printer<'a> {
    pub(super) fn install_system_local_export_bindings(
        &mut self,
        source: &tsz_parser::parser::node::SourceFileData,
    ) {
        let (reexported_names, reexported_name_lists) =
            self.collect_system_local_export_bindings(source);
        self.system_reexported_names = reexported_names;
        self.system_reexported_name_lists = reexported_name_lists;
    }

    fn collect_system_local_export_bindings(
        &self,
        source: &tsz_parser::parser::node::SourceFileData,
    ) -> (FxHashMap<String, String>, FxHashMap<String, Vec<String>>) {
        let mut reexported_names: FxHashMap<String, String> = FxHashMap::default();
        let mut reexported_name_lists: FxHashMap<String, Vec<String>> = FxHashMap::default();
        for &stmt_idx in &source.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                && let Some(var_stmt) = self.arena.get_variable(stmt_node)
                && self
                    .arena
                    .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword)
            {
                for name in self.collect_variable_names(&var_stmt.declarations) {
                    if !name.is_empty() {
                        reexported_names
                            .entry(name.clone())
                            .or_insert_with(|| name.clone());
                        push_system_reexport_name(&mut reexported_name_lists, name.clone(), name);
                    }
                }
                continue;
            }
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export_decl) = self.arena.get_export_decl(stmt_node) else {
                continue;
            };
            if export_decl.module_specifier.is_some() || export_decl.is_default_export {
                continue;
            }
            let Some(clause_node) = self.arena.get(export_decl.export_clause) else {
                continue;
            };
            if clause_node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                && let Some(var_stmt) = self.arena.get_variable(clause_node)
            {
                for name in self.collect_variable_names(&var_stmt.declarations) {
                    if !name.is_empty() {
                        reexported_names
                            .entry(name.clone())
                            .or_insert_with(|| name.clone());
                        push_system_reexport_name(&mut reexported_name_lists, name.clone(), name);
                    }
                }
                continue;
            }
            if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
                continue;
            }
            let Some(named_exports) = self.arena.get_named_imports(clause_node) else {
                continue;
            };
            for &spec_idx in &named_exports.elements.nodes {
                if let Some(spec) = self.arena.get_specifier_at(spec_idx) {
                    let local_name = if spec.property_name.is_some() {
                        self.get_specifier_name_text(spec.property_name)
                    } else {
                        self.get_specifier_name_text(spec.name)
                    }
                    .unwrap_or_default();
                    let export_name = self.get_specifier_name_text(spec.name).unwrap_or_default();
                    if !local_name.is_empty() && !export_name.is_empty() {
                        reexported_names.insert(local_name.clone(), export_name.clone());
                        push_system_reexport_name(
                            &mut reexported_name_lists,
                            local_name,
                            export_name,
                        );
                    }
                }
            }
        }
        (reexported_names, reexported_name_lists)
    }

    pub(in crate::emitter) const fn is_system_live_export_context(&self) -> bool {
        self.in_system_execute_body
            || matches!(self.ctx.original_module_kind, Some(ModuleKind::System))
    }

    pub(in crate::emitter) fn system_export_names_for_local(
        &self,
        local_name: &str,
    ) -> Vec<String> {
        if let Some(export_names) = self.system_reexported_name_lists.get(local_name)
            && !export_names.is_empty()
        {
            return export_names.clone();
        }
        self.system_reexported_names
            .get(local_name)
            .cloned()
            .map(|export_name| vec![export_name])
            .unwrap_or_default()
    }

    pub(in crate::emitter) fn write_system_export_call_chain_start(&mut self, names: &[String]) {
        for export_name in names.iter().rev() {
            self.write("exports_1(\"");
            self.write(export_name);
            self.write("\", ");
        }
    }

    pub(in crate::emitter) fn write_system_export_call_chain_end(&mut self, names: &[String]) {
        for _ in names {
            self.write(")");
        }
    }
}
