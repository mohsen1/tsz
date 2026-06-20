//! Type-alias/value-alias resolution helpers.
//!
//! Split out of the parent module to satisfy the source-file line cap.

use super::*;

impl<'a> CheckerState<'a> {
    /// Resolve the type of a synthetic `export default interface A {}` /
    /// `export default type A = ...` alias.
    ///
    /// The binder models the inline default export of a pure type as an
    /// `ALIAS`-only `"default"` symbol whose declaration is the
    /// `INTERFACE_DECLARATION` / `TYPE_ALIAS_DECLARATION` node, but it carries no
    /// `value_declaration` (it is type-only). The same node also declares a local
    /// type symbol (`A`) in its file scope. We resolve the alias to that local
    /// symbol's type so a cross-file default import of the type
    /// (`import A from './a'; let x: A`) sees the interface/alias type rather than
    /// the generic alias-`any` fallback.
    ///
    /// The match is purely structural (binder symbol flags + declaration node
    /// kind); no identifier/file-name string is used as a decision predicate.
    pub(super) fn inline_default_export_type_only_target_type(
        &mut self,
        alias_sym_id: tsz_binder::SymbolId,
        declarations: &[NodeIndex],
    ) -> Option<TypeId> {
        // Resolve the local interface/type-alias symbol first, holding only
        // immutable borrows of the declaring file's arena/binder. Drop those
        // borrows before calling `get_type_of_symbol` (which needs `&mut self`).
        let local_sym_id = {
            // The alias's declarations live in its declaring file's arena/binder,
            // not necessarily the current one (cross-file default import).
            let file_idx = self
                .ctx
                .resolve_symbol_declaring_file_index(alias_sym_id)
                .or_else(|| self.ctx.resolve_symbol_file_index(alias_sym_id));
            let arena = match file_idx {
                Some(idx) => self.ctx.get_arena_for_file(idx as u32),
                None => self.ctx.arena,
            };
            let binder = file_idx
                .and_then(|idx| self.ctx.get_binder_for_file(idx))
                .unwrap_or(self.ctx.binder);

            let mut found = None;
            for &decl_idx in declarations {
                let Some(decl_node) = arena.get(decl_idx) else {
                    continue;
                };
                // Only the synthetic default export carries an inline
                // interface/type declaration node as one of its declarations.
                let name_idx = if decl_node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
                    arena.get_interface(decl_node).map(|iface| iface.name)
                } else if decl_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                    arena.get_type_alias(decl_node).map(|alias| alias.name)
                } else {
                    None
                };
                let Some(name_idx) = name_idx else {
                    continue;
                };
                let Some(name) = arena
                    .get(name_idx)
                    .and_then(|n| arena.get_identifier(n))
                    .map(|ident| ident.escaped_text.as_str())
                else {
                    continue;
                };

                // Find the local TYPE symbol the binder declared for this inline
                // declaration's name. It must be a real interface/type-alias
                // symbol, distinct from the `ALIAS`-only default-export symbol.
                let Some(candidate) = binder.file_locals.get(name) else {
                    continue;
                };
                if candidate == alias_sym_id {
                    continue;
                }
                if binder.get_symbol(candidate).is_some_and(|local| {
                    local.has_any_flags(symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS)
                }) {
                    found = Some(candidate);
                    break;
                }
            }
            found?
        };

        let target_type = self.get_type_of_symbol(local_sym_id);
        (target_type != TypeId::ERROR && target_type != TypeId::ANY).then_some(target_type)
    }

    pub(super) fn type_node_contains_kind(&self, root: NodeIndex, kind: u16) -> bool {
        let mut stack = vec![root];
        while let Some(idx) = stack.pop() {
            if self
                .ctx
                .arena
                .get(idx)
                .is_some_and(|node| node.kind == kind)
            {
                return true;
            }
            stack.extend(self.ctx.arena.get_children(idx));
        }
        false
    }

    pub(super) fn alias_has_local_non_import_declaration(
        &self,
        sym_id: tsz_binder::SymbolId,
        declarations: &[NodeIndex],
    ) -> bool {
        declarations.iter().copied().any(|decl_idx| {
            if self.ctx.binder.node_symbols.get(&decl_idx.0) != Some(&sym_id) {
                return false;
            }
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                return false;
            };
            if matches!(
                node.kind,
                syntax_kind_ext::IMPORT_SPECIFIER
                    | syntax_kind_ext::EXPORT_SPECIFIER
                    | syntax_kind_ext::IMPORT_CLAUSE
                    | syntax_kind_ext::NAMESPACE_IMPORT
                    | syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            ) {
                return false;
            }
            if node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                return true;
            }
            self.ctx
                .arena
                .get_export_decl(node)
                .is_some_and(|export_decl| {
                    export_decl.module_specifier.is_none() || export_decl.is_default_export
                })
        })
    }

    pub(super) fn alias_declarations_include_import_or_export_specifier(
        &self,
        declarations: &[NodeIndex],
    ) -> bool {
        declarations.iter().copied().any(|decl| {
            self.ctx.arena.get(decl).is_some_and(|node| {
                matches!(
                    node.kind,
                    syntax_kind_ext::IMPORT_SPECIFIER | syntax_kind_ext::EXPORT_SPECIFIER
                )
            })
        })
    }

    pub(super) fn resolve_default_import_namespace_fallback(
        &mut self,
        module_name: &str,
        is_node_esm_importing_cjs: bool,
    ) -> Option<TypeId> {
        if self
            .ctx
            .module_namespace_resolution_set
            .contains(module_name)
        {
            return Some(TypeId::ANY);
        }
        self.ctx
            .module_namespace_resolution_set
            .insert(module_name.to_string());

        let result = self
            .resolve_default_import_namespace_from_exports(module_name)
            .or_else(|| {
                is_node_esm_importing_cjs
                    .then(|| self.resolve_node_esm_cjs_default_import_namespace(module_name))
                    .flatten()
            });

        self.ctx.module_namespace_resolution_set.remove(module_name);
        result
    }

    fn resolve_default_import_namespace_from_exports(
        &mut self,
        module_name: &str,
    ) -> Option<TypeId> {
        let exports_table = self.resolve_effective_module_exports_from_file(
            module_name,
            Some(self.ctx.current_file_idx),
        )?;
        let ordered_exports = self.ordered_namespace_export_entries(&exports_table);
        if exports_table.has("export=")
            && let Some(export_eq_sym) = exports_table.get("export=")
        {
            let export_eq_type = self.get_type_of_symbol(export_eq_sym);
            let export_eq_type =
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, export_eq_type);
            let export_eq_type =
                self.widen_fresh_object_literal_properties_for_display(export_eq_type);
            return Some(export_eq_type);
        }

        use tsz_solver::PropertyInfo;
        let mut props: Vec<PropertyInfo> = Vec::new();
        for &(name, export_sym_id) in &ordered_exports {
            if self.should_skip_namespace_export_name(&exports_table, name, export_sym_id) {
                continue;
            }
            let declaration_order = if name == "default" {
                1
            } else {
                props.len() as u32 + 2
            };
            let prop_type = self.get_type_of_symbol(export_sym_id);
            let name_atom = self.ctx.types.intern_string(name);
            props.push(PropertyInfo {
                name: name_atom,
                type_id: prop_type,
                write_type: prop_type,
                optional: false,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order,
                is_string_named: false,
                is_symbol_named: false,
                single_quoted_name: false,
            });
        }
        Self::normalize_namespace_export_declaration_order(&mut props);
        let module_type = self.ctx.types.factory().object(props);
        self.ctx.namespace_module_names.insert(
            module_type,
            self.imported_namespace_display_module_name(module_name),
        );
        Some(module_type)
    }

    fn resolve_node_esm_cjs_default_import_namespace(
        &mut self,
        module_name: &str,
    ) -> Option<TypeId> {
        let default_sym_id = self.resolve_cross_file_export_from_file(
            module_name,
            "default",
            Some(self.ctx.current_file_idx),
        )?;
        use tsz_solver::PropertyInfo;
        let default_type = self.get_type_of_symbol(default_sym_id);
        let module_type = self.ctx.types.factory().object(vec![PropertyInfo {
            name: self.ctx.types.intern_string("default"),
            type_id: default_type,
            write_type: default_type,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 1,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        }]);
        self.ctx.namespace_module_names.insert(
            module_type,
            self.imported_namespace_display_module_name(module_name),
        );
        Some(module_type)
    }

    /// Resolve the `unique symbol` identity of a value+type-alias symbol that
    /// follows the self-`typeof` idiom: a `const X = Symbol()` /
    /// `const X = Symbol.for(...)` / `const X: unique symbol` value merged with a
    /// same-named `type X = typeof X` alias.
    ///
    /// Returns `Some` only when this symbol owns BOTH a self-referential
    /// `typeof X` type-alias declaration and a unique-symbol const value
    /// declaration, so non-idiomatic value+type-alias merges (e.g. `const X =
    /// 5; type X = string`) keep their generic alias-body resolution. The
    /// returned type is exactly what `typeof X` denotes, so it is correct for
    /// both the value and type meanings of the merged symbol.
    pub(super) fn merged_self_typeof_unique_symbol_type(
        &mut self,
        declarations: &[NodeIndex],
        escaped_name: &str,
    ) -> Option<TypeId> {
        let has_self_typeof_alias = declarations
            .iter()
            .copied()
            .any(|decl_idx| self.type_alias_body_is_self_typeof(decl_idx, escaped_name));
        if !has_self_typeof_alias {
            return None;
        }
        for decl_idx in declarations.iter().copied() {
            // Unannotated `const X = Symbol()` / `const X = Symbol.for(...)`.
            if let Some(unique) = self.const_symbol_factory_unique_value_type(decl_idx) {
                return Some(unique);
            }
            // Annotated `const X: unique symbol`.
            if let Some(node) = self.ctx.arena.get(decl_idx)
                && let Some(var_decl) = self.ctx.arena.get_variable_declaration(node)
                && var_decl.type_annotation.is_some()
                && self.is_const_variable_declaration(decl_idx)
            {
                let upgraded = self.const_unique_symbol_value_type(
                    decl_idx,
                    var_decl.type_annotation,
                    TypeId::SYMBOL,
                );
                if upgraded != TypeId::SYMBOL {
                    return Some(upgraded);
                }
            }
        }
        None
    }

    /// Whether `decl_idx` is a `type X = typeof X` declaration whose `typeof`
    /// operand names `escaped_name` — a self-referential typeof of the merged
    /// symbol's own value. Pure syntactic check (no symbol resolution) so it
    /// cannot re-enter symbol-type computation for the symbol being resolved.
    fn type_alias_body_is_self_typeof(&self, decl_idx: NodeIndex, escaped_name: &str) -> bool {
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
            return false;
        }
        let Some(type_alias) = self.ctx.arena.get_type_alias(node) else {
            return false;
        };
        let Some(body) = self.ctx.arena.get(type_alias.type_node) else {
            return false;
        };
        if body.kind != syntax_kind_ext::TYPE_QUERY {
            return false;
        }
        let Some(type_query) = self.ctx.arena.get_type_query(body) else {
            return false;
        };
        // A self-`typeof` references a bare identifier (not a qualified name)
        // whose text matches the merged symbol's name.
        self.ctx
            .arena
            .get_identifier_text(type_query.expr_name)
            .is_some_and(|name| name == escaped_name)
    }
}
