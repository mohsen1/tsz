impl<'a> CheckerState<'a> {
    // Property initialization order checking (TS2729) is in
    // `type_checking/property_init.rs`.

    // 18. AST Context Checking (4 functions)

    /// Walk a namespace declaration's body and decide whether any statement
    /// produces runtime values, treating `export { ... }` as "needs runtime"
    /// only if at least one specifier resolves to a value-meaning symbol.
    ///
    /// Returns `Some(true)` if value-instantiated, `Some(false)` if all
    /// runtime statements are type-only, and `None` if the body is missing
    /// or the structure is unexpected (caller should fall back).
    fn namespace_body_value_instantiated(&self, namespace_idx: NodeIndex) -> Option<bool> {
        let node = self.ctx.arena.get(namespace_idx)?;

        // `export as namespace X` always creates a runtime global.
        if node.kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION {
            return Some(true);
        }
        if node.kind != syntax_kind_ext::MODULE_DECLARATION {
            return Some(false);
        }
        let module_decl = self.ctx.arena.get_module(node)?;
        let body_idx = module_decl.body;
        if body_idx.is_none() {
            return Some(false);
        }
        let body_node = self.ctx.arena.get(body_idx)?;

        // Dotted namespace: `namespace Foo.Bar { ... }` — recurse into inner.
        if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            return self.namespace_body_value_instantiated(body_idx);
        }
        if body_node.kind != syntax_kind_ext::MODULE_BLOCK {
            return Some(false);
        }
        let module_block = self.ctx.arena.get_module_block(body_node)?;
        let statements = module_block.statements.as_ref()?;

        for &stmt_idx in &statements.nodes {
            let stmt_node = match self.ctx.arena.get(stmt_idx) {
                Some(n) => n,
                None => continue,
            };
            if self.namespace_statement_creates_runtime_value(stmt_node, stmt_idx) {
                return Some(true);
            }
        }
        Some(false)
    }

    /// Mirror of `NodeArena::is_runtime_module_statement` that resolves the
    /// named-export case rather than treating it as conservatively runtime.
    fn namespace_statement_creates_runtime_value(
        &self,
        node: &tsz_parser::parser::node::Node,
        node_idx: NodeIndex,
    ) -> bool {
        use syntax_kind_ext::{
            CLASS_DECLARATION, ENUM_DECLARATION, EXPORT_DECLARATION, FUNCTION_DECLARATION,
            IMPORT_DECLARATION, IMPORT_EQUALS_DECLARATION, INTERFACE_DECLARATION,
            MODULE_DECLARATION, NAMED_EXPORTS, TYPE_ALIAS_DECLARATION, VARIABLE_STATEMENT,
        };

        match node.kind {
            // Type-only declarations
            k if k == INTERFACE_DECLARATION || k == TYPE_ALIAS_DECLARATION => false,
            // Imports do not produce runtime values for the enclosing namespace.
            k if k == IMPORT_DECLARATION || k == IMPORT_EQUALS_DECLARATION => false,
            k if k == EXPORT_DECLARATION => {
                let Some(export_decl) = self.ctx.arena.get_export_decl(node) else {
                    return false;
                };
                // `export type { ... }` is always type-only.
                if export_decl.is_type_only {
                    return false;
                }
                // `export { x } from "mod"` re-exports a binding from another
                // module. Without resolving the foreign module's symbols here
                // we keep the parser's conservative answer (treat as runtime).
                if export_decl.module_specifier.is_some() {
                    return true;
                }
                let Some(clause) = self.ctx.arena.get(export_decl.export_clause) else {
                    return false;
                };
                match clause.kind {
                    k if k == VARIABLE_STATEMENT
                        || k == FUNCTION_DECLARATION
                        || k == CLASS_DECLARATION
                        || k == ENUM_DECLARATION =>
                    {
                        true
                    }
                    k if k == MODULE_DECLARATION => self
                        .namespace_body_value_instantiated(export_decl.export_clause)
                        .unwrap_or(true),
                    k if k == NAMED_EXPORTS => self.named_exports_have_value_specifier(clause),
                    _ => false,
                }
            }
            // Nested namespace — recurse with the same precise check.
            k if k == MODULE_DECLARATION => self
                .namespace_body_value_instantiated(node_idx)
                .unwrap_or_else(|| self.ctx.arena.is_namespace_instantiated(node_idx)),
            // Variables/functions/classes/enums/expressions/etc. always runtime.
            _ => true,
        }
    }

    /// Resolve each specifier of a `NAMED_EXPORTS` clause and return true if
    /// any of them refers to a symbol with value meaning. Specifiers marked
    /// `export type {}` are skipped (always type-only).
    fn named_exports_have_value_specifier(
        &self,
        named_exports: &tsz_parser::parser::node::Node,
    ) -> bool {
        let Some(named) = self.ctx.arena.get_named_imports(named_exports) else {
            // Couldn't inspect — fall back to conservative (treat as runtime).
            return true;
        };
        for &spec_idx in &named.elements.nodes {
            let Some(spec_node) = self.ctx.arena.get(spec_idx) else {
                continue;
            };
            if spec_node.kind != syntax_kind_ext::EXPORT_SPECIFIER {
                continue;
            }
            let Some(spec) = self.ctx.arena.get_specifier(spec_node) else {
                continue;
            };
            if spec.is_type_only {
                continue;
            }
            // `property_name` is the source name (when aliased); fall back to
            // `name` when the export isn't renamed.
            let lookup_idx = if spec.property_name.is_none() {
                spec.name
            } else {
                spec.property_name
            };
            if lookup_idx.is_none() {
                // No identifier to resolve — conservative.
                return true;
            }
            if self.specifier_target_has_value_meaning(lookup_idx) {
                return true;
            }
        }
        false
    }

    /// Resolve the symbol targeted by an export specifier and return whether
    /// it has any value-meaning flags. Unresolved bindings fall back to true
    /// (conservative) so we never silently downgrade a namespace from
    /// instantiated to non-instantiated when symbol resolution is incomplete.
    fn specifier_target_has_value_meaning(&self, name_idx: NodeIndex) -> bool {
        let Some(sym_id) = self.ctx.binder.resolve_identifier(self.ctx.arena, name_idx) else {
            return true;
        };
        let lib_binders = self.get_lib_binders();
        let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
            return true;
        };
        let value_mask = tsz_binder::symbol_flags::VALUE & !tsz_binder::symbol_flags::VALUE_MODULE;
        if symbol.has_any_flags(value_mask) {
            return true;
        }
        // A namespace symbol has value meaning only when it is actually
        // instantiated — recurse into its declarations to check precisely.
        if (symbol.flags & tsz_binder::symbol_flags::VALUE_MODULE) != 0 {
            return symbol
                .declarations
                .iter()
                .any(|&d| self.is_namespace_declaration_value_instantiated(d));
        }
        // ALIAS symbols (import equals, default import, etc.) require resolving
        // the alias target. Conservatively treat as runtime if we can't tell.
        if (symbol.flags & tsz_binder::symbol_flags::ALIAS) != 0 {
            return true;
        }
        false
    }

    /// Check if a method declaration has a body (is an implementation, not just a signature).
    ///
    /// ## Parameters
    /// - `decl_idx`: The method declaration node index
    ///
    /// Returns true if the method has a body.
    pub(crate) fn method_has_body(&self, decl_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::METHOD_DECLARATION {
            return false;
        }
        let Some(method) = self.ctx.arena.get_method_decl(node) else {
            return false;
        };
        method.body.is_some()
    }

    /// Get the name node of a declaration for error reporting.
    ///
    /// ## Parameters
    /// - `decl_idx`: The declaration node index
    ///
    /// Returns the name node if the declaration has one.
    pub(crate) fn get_declaration_name_node(&self, decl_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(decl_idx)?;

        match node.kind {
            k if k == tsz_scanner::SyntaxKind::Identifier as u16 => Some(decl_idx),
            syntax_kind_ext::VARIABLE_DECLARATION => {
                let var_decl = self.ctx.arena.get_variable_declaration(node)?;
                Some(var_decl.name)
            }
            syntax_kind_ext::FUNCTION_DECLARATION => {
                let func = self.ctx.arena.get_function(node)?;
                Some(func.name)
            }
            syntax_kind_ext::PARAMETER => {
                let param = self.ctx.arena.get_parameter(node)?;
                Some(param.name)
            }
            syntax_kind_ext::CLASS_DECLARATION => {
                let class = self.ctx.arena.get_class(node)?;
                Some(class.name)
            }
            syntax_kind_ext::PROPERTY_DECLARATION => {
                let prop = self.ctx.arena.get_property_decl(node)?;
                Some(prop.name)
            }
            syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.ctx.arena.get_method_decl(node)?;
                Some(method.name)
            }
            syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                let accessor = self.ctx.arena.get_accessor(node)?;
                Some(accessor.name)
            }
            syntax_kind_ext::INTERFACE_DECLARATION => {
                let interface = self.ctx.arena.get_interface(node)?;
                Some(interface.name)
            }
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                let type_alias = self.ctx.arena.get_type_alias(node)?;
                Some(type_alias.name)
            }
            syntax_kind_ext::ENUM_DECLARATION => {
                let enum_decl = self.ctx.arena.get_enum(node)?;
                Some(enum_decl.name)
            }
            syntax_kind_ext::MODULE_DECLARATION => {
                let module_decl = self.ctx.arena.get_module(node)?;
                Some(module_decl.name)
            }
            syntax_kind_ext::IMPORT_CLAUSE => {
                let clause = self.ctx.arena.get_import_clause(node)?;
                Some(clause.name)
            }
            syntax_kind_ext::NAMESPACE_IMPORT => {
                let named = self.ctx.arena.get_named_imports(node)?;
                Some(named.name)
            }
            syntax_kind_ext::IMPORT_SPECIFIER | syntax_kind_ext::EXPORT_SPECIFIER => {
                let spec = self.ctx.arena.get_specifier(node)?;
                if spec.name.is_some() {
                    Some(spec.name)
                } else {
                    Some(spec.property_name)
                }
            }
            syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                let import = self.ctx.arena.get_import_decl(node)?;
                Some(import.import_clause)
            }
            syntax_kind_ext::EXPORT_ASSIGNMENT => {
                let export = self.ctx.arena.get_export_assignment(node)?;
                Some(export.expression)
            }
            _ => None,
        }
    }

    /// Get the declaration name as a string text.
    ///
    /// Combines `get_declaration_name_node` with identifier/literal text extraction.
    /// Handles both regular identifiers and string literal module names.
    pub(crate) fn get_declaration_name_text(&self, decl_idx: NodeIndex) -> Option<String> {
        let name_idx = self.get_declaration_name_node(decl_idx)?;
        let name_node = self.ctx.arena.get(name_idx)?;
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            return Some(ident.escaped_text.clone());
        }
        if let Some(lit) = self.ctx.arena.get_literal(name_node) {
            return Some(lit.text.clone());
        }
        None
    }

    // Interface merge compatibility, name matching, property name utilities,
    // and node containment are in `type_checking/declarations_utils.rs`.
}
