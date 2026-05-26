use super::*;

impl<'a> DocumentSymbolProvider<'a> {
    /// Build an `alias` entry for a single import/export binding. The `name`
    /// is the local identifier the binding introduces into scope (e.g. `B`
    /// for `import { x as B }` or `export { a as B }`). `decl_idx` is the
    /// enclosing statement used for the range span — tsc anchors specifier
    /// spans to the whole statement, not the specifier token.
    fn alias_symbol(
        &self,
        name: String,
        name_node: NodeIndex,
        decl_idx: NodeIndex,
        container_name: Option<&str>,
        modifiers: String,
    ) -> DocumentSymbolEntry {
        let range = node_range(self.arena, self.line_map, self.source_text, decl_idx);
        let selection_range = if name_node.is_some() {
            node_range(self.arena, self.line_map, self.source_text, name_node)
        } else {
            self.get_range_keyword(decl_idx, 6)
        };
        DocumentSymbolEntry {
            name,
            detail: None,
            kind: SymbolKind::Alias,
            kind_modifiers: modifiers,
            range,
            selection_range,
            container_name: container_name.map(std::string::ToString::to_string),
            children: vec![],
        }
    }

    /// Collect specifiers from a `NAMED_EXPORTS` / `NAMED_IMPORTS` clause.
    /// Each specifier's local name becomes an alias. When `treat_as_export`
    /// is true, the `export` modifier is applied.
    pub(super) fn collect_import_export_specifiers(
        &self,
        clause_idx: NodeIndex,
        container_name: Option<&str>,
        treat_as_export: bool,
    ) -> Vec<DocumentSymbolEntry> {
        let Some(clause_node) = self.arena.get(clause_idx) else {
            return Vec::new();
        };
        let Some(named) = self.arena.get_named_imports(clause_node) else {
            return Vec::new();
        };
        let mut symbols = Vec::new();
        for &spec_idx in &named.elements.nodes {
            let Some(spec_node) = self.arena.get(spec_idx) else {
                continue;
            };
            let Some(spec) = self.arena.get_specifier(spec_node) else {
                continue;
            };
            let name = self
                .get_name(spec.name)
                .unwrap_or_else(|| "<unknown>".to_string());
            let mods = if treat_as_export {
                String::from("export")
            } else {
                String::new()
            };
            symbols.push(self.alias_symbol(name, spec.name, spec_idx, container_name, mods));
        }
        symbols
    }

    /// Collect aliases from an `import ...` declaration.
    pub(super) fn collect_import_decl(
        &self,
        node: &Node,
        node_idx: NodeIndex,
        container_name: Option<&str>,
    ) -> Vec<DocumentSymbolEntry> {
        let Some(import) = self.arena.get_import_decl(node) else {
            return Vec::new();
        };
        let clause_idx = import.import_clause;
        if clause_idx.is_none() {
            return Vec::new();
        }
        let Some(clause_node) = self.arena.get(clause_idx) else {
            return Vec::new();
        };
        let Some(clause) = self.arena.get_import_clause(clause_node) else {
            return Vec::new();
        };
        let mut symbols = Vec::new();

        if clause.name.is_some()
            && let Some(name) = self.get_name(clause.name)
        {
            symbols.push(self.alias_symbol(
                name,
                clause.name,
                node_idx,
                container_name,
                String::new(),
            ));
        }

        let named_idx = clause.named_bindings;
        if named_idx.is_some()
            && let Some(named_node) = self.arena.get(named_idx)
        {
            if named_node.kind == syntax_kind_ext::NAMESPACE_IMPORT {
                if let Some(named) = self.arena.get_named_imports(named_node) {
                    let name = if named.name.is_some() {
                        self.get_name(named.name)
                            .unwrap_or_else(|| "<unknown>".to_string())
                    } else {
                        "<unknown>".to_string()
                    };
                    symbols.push(self.alias_symbol(
                        name,
                        named.name,
                        node_idx,
                        container_name,
                        String::new(),
                    ));
                }
            } else if named_node.kind == syntax_kind_ext::NAMED_IMPORTS {
                symbols.extend(self.collect_import_export_specifiers(
                    named_idx,
                    container_name,
                    false,
                ));
            }
        }

        symbols
    }

    /// Collect an alias from an `import e = require("...")` / `import e = x.y`
    /// declaration. When the statement has an `export` modifier, it is
    /// surfaced as a `kindModifier` on the alias.
    pub(super) fn collect_import_equals(
        &self,
        node: &Node,
        node_idx: NodeIndex,
        container_name: Option<&str>,
    ) -> Vec<DocumentSymbolEntry> {
        let Some(import) = self.arena.get_import_decl(node) else {
            return Vec::new();
        };
        let name_idx = import.import_clause;
        let Some(name) = self.get_name(name_idx) else {
            return Vec::new();
        };
        let modifiers = self.get_kind_modifiers_from_list(&import.modifiers);
        vec![self.alias_symbol(name, name_idx, node_idx, container_name, modifiers)]
    }
}
