//! Type-only symbol and namespace detection utilities.
//!
//! Determines whether symbols, imports, and namespace members are "type-only"
//! (exist only at the type level with no runtime value). Used by the checker
//! to decide when to emit TS2708 and related diagnostics.

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::flags::node_flags;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn file_has_jsdoc_typedef_named(&self, file_idx: usize, export_name: &str) -> bool {
        let arena = self.ctx.get_arena_for_file(file_idx as u32);
        arena.source_files.iter().any(|source_file| {
            source_file.comments.iter().any(|comment| {
                let content = source_file
                    .text
                    .get(comment.pos as usize..comment.end as usize)
                    .unwrap_or("");
                Self::parse_jsdoc_typedefs(content)
                    .iter()
                    .any(|(name, _)| name == export_name)
            })
        })
    }

    pub(crate) fn report_namespace_value_access_for_type_only_import_equals_expr(
        &mut self,
        expr_idx: NodeIndex,
    ) -> bool {
        if !self.is_type_only_import_equals_namespace_expr(expr_idx) {
            return false;
        }

        let current_alias = self.resolve_identifier_symbol(expr_idx);
        let has_scoped_value_or_alias = self
            .entity_name_text(expr_idx)
            .map(|entity_name| {
                let lib_binders = self.get_lib_binders();
                self.ctx
                    .binder
                    .resolve_identifier_with_filter(self.ctx.arena, expr_idx, &lib_binders, |sid| {
                        self.ctx
                            .binder
                            .get_symbol_with_libs(sid, &lib_binders)
                            .is_some_and(|s| {
                                Some(sid) != current_alias
                                    && ((s.flags & symbol_flags::VALUE) != 0
                                        || ((s.flags & symbol_flags::ALIAS) != 0
                                            && !s.is_type_only
                                            && s.escaped_name == entity_name))
                            })
                    })
                    .is_some()
            })
            .unwrap_or(false);

        if has_scoped_value_or_alias {
            return false;
        }

        if let Some(ns_name) = self.entity_name_text(expr_idx) {
            self.report_wrong_meaning_diagnostic(
                &ns_name,
                expr_idx,
                crate::query_boundaries::name_resolution::NameLookupKind::Namespace,
            );
            if let Some(sym_id) = self.resolve_identifier_symbol(expr_idx)
                && self.alias_resolves_to_type_only(sym_id)
            {
                self.report_wrong_meaning_diagnostic(
                    &ns_name,
                    expr_idx,
                    crate::query_boundaries::name_resolution::NameLookupKind::Type,
                );
            }
        }

        true
    }

    fn find_import_equals_export_equals_symbol(
        &self,
        module_name: &str,
    ) -> Option<(&tsz_binder::BinderState, SymbolId)> {
        let normalized = module_name.trim_matches('"').trim_matches('\'');
        let quoted = format!("\"{normalized}\"");
        let single_quoted = format!("'{normalized}'");
        let candidates = [
            module_name,
            normalized,
            quoted.as_str(),
            single_quoted.as_str(),
        ];

        for candidate in candidates {
            if let Some(exports) = self.ctx.binder.module_exports.get(candidate)
                && let Some(sym_id) = exports.get("export=")
            {
                return Some((self.ctx.binder, sym_id));
            }
        }

        // Use global_module_exports_index for O(1) lookup instead of O(N) binder scan
        if let Some(index) = self.ctx.global_module_exports_index.as_ref() {
            let all_binders = self.ctx.all_binders.as_ref()?;
            for candidate in candidates {
                if let Some(entries) = index.get(candidate).and_then(|inner| inner.get("export=")) {
                    for &(file_idx, sym_id) in entries {
                        if let Some(binder) = all_binders.get(file_idx) {
                            return Some((binder, sym_id));
                        }
                    }
                }
            }
        } else if let Some(all_binders) = self.ctx.all_binders.as_ref() {
            for binder in all_binders.iter() {
                for candidate in candidates {
                    if let Some(exports) = binder.module_exports.get(candidate)
                        && let Some(sym_id) = exports.get("export=")
                    {
                        return Some((binder, sym_id));
                    }
                }
            }
        }

        None
    }

    /// Returns true when an expression is an `import x = require("...")` alias
    /// whose target module has `export =` bound to a pure type (interface or
    /// type alias) — i.e., NOT a namespace/module.
    ///
    /// When this returns true, the caller should emit TS2693 (type used as
    /// value) rather than TS2708 (namespace used as value).
    pub(crate) fn import_equals_export_is_pure_type(&self, expr_idx: NodeIndex) -> bool {
        let Some(sym_id) = self.resolve_identifier_symbol(expr_idx) else {
            return false;
        };

        let lib_binders = self.get_lib_binders();
        let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
            return false;
        };

        if (symbol.flags & symbol_flags::ALIAS) == 0 {
            return false;
        }

        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else if let Some(&first_decl) = symbol.declarations.first() {
            first_decl
        } else {
            return false;
        };

        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };

        if decl_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            return false;
        }

        let Some(import_decl) = self.ctx.arena.get_import_decl(decl_node) else {
            return false;
        };

        let module_name_owned;
        let module_name = if let Some(module_node) =
            self.ctx.arena.get(import_decl.module_specifier)
            && module_node.kind == SyntaxKind::StringLiteral as u16
            && let Some(literal) = self.ctx.arena.get_literal(module_node)
        {
            literal.text.as_str()
        } else if let Some(specifier) =
            self.get_require_module_specifier(import_decl.module_specifier)
        {
            module_name_owned = specifier;
            module_name_owned.as_str()
        } else {
            return false;
        };

        let Some((export_equals_binder, export_equals_sym)) =
            self.find_import_equals_export_equals_symbol(module_name)
        else {
            return false;
        };

        let resolved_export_equals = if let Some(export_sym) =
            export_equals_binder.get_symbol_with_libs(export_equals_sym, &lib_binders)
            && (export_sym.flags & symbol_flags::ALIAS) != 0
        {
            let mut visited_aliases = Vec::new();
            match self.resolve_alias_symbol(export_equals_sym, &mut visited_aliases) {
                Some(resolved) => resolved,
                None => return false,
            }
        } else {
            export_equals_sym
        };

        if let Some(export_symbol) =
            export_equals_binder.get_symbol_with_libs(resolved_export_equals, &lib_binders)
        {
            let has_namespace_exports = export_symbol
                .exports
                .as_ref()
                .is_some_and(|exports| !exports.is_empty());
            // Pure type: has INTERFACE or TYPE_ALIAS flags but no VALUE or NAMESPACE flags
            let is_pure_type = (export_symbol.flags
                & (symbol_flags::INTERFACE
                    | symbol_flags::TYPE_ALIAS
                    | symbol_flags::TYPE_PARAMETER))
                != 0;
            let is_namespace_or_module = (export_symbol.flags
                & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE))
                != 0;
            let has_value = (export_symbol.flags & symbol_flags::VALUE) != 0;
            return is_pure_type && !is_namespace_or_module && !has_namespace_exports && !has_value;
        }

        false
    }

    /// Returns true when an expression is an `import x = require("...")` alias
    /// whose target module has `export =` bound to a type-only symbol.
    ///
    /// In value position, member access on such aliases should emit TS2708
    /// (Cannot use namespace as a value).
    pub(crate) fn is_type_only_import_equals_namespace_expr(&self, expr_idx: NodeIndex) -> bool {
        let Some(sym_id) = self.resolve_identifier_symbol(expr_idx) else {
            return false;
        };

        let lib_binders = self.get_lib_binders();
        let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
            return false;
        };

        if (symbol.flags & symbol_flags::ALIAS) == 0 {
            return false;
        }

        // If the symbol also has non-alias VALUE flags (e.g., from a local
        // `const I = {}` merged with `import type I = require(...)`), the
        // identifier has value semantics and should NOT be treated as a
        // type-only namespace expression.
        let value_flags = symbol_flags::VARIABLE
            | symbol_flags::FUNCTION
            | symbol_flags::CLASS
            | symbol_flags::ENUM;
        if (symbol.flags & value_flags) != 0 {
            return false;
        }

        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else if let Some(&first_decl) = symbol.declarations.first() {
            first_decl
        } else {
            return false;
        };

        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };

        if decl_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            return false;
        }

        let Some(import_decl) = self.ctx.arena.get_import_decl(decl_node) else {
            return false;
        };

        let module_name_owned;
        let module_name = if let Some(module_node) =
            self.ctx.arena.get(import_decl.module_specifier)
            && module_node.kind == SyntaxKind::StringLiteral as u16
            && let Some(literal) = self.ctx.arena.get_literal(module_node)
        {
            literal.text.as_str()
        } else if let Some(specifier) =
            self.get_require_module_specifier(import_decl.module_specifier)
        {
            module_name_owned = specifier;
            module_name_owned.as_str()
        } else {
            return false;
        };

        let Some((export_equals_binder, export_equals_sym)) =
            self.find_import_equals_export_equals_symbol(module_name)
        else {
            return false;
        };

        let resolved_export_equals = if let Some(export_sym) =
            export_equals_binder.get_symbol_with_libs(export_equals_sym, &lib_binders)
            && (export_sym.flags & symbol_flags::ALIAS) != 0
        {
            let mut visited_aliases = Vec::new();
            match self.resolve_alias_symbol(export_equals_sym, &mut visited_aliases) {
                Some(resolved) => resolved,
                // If we can't resolve the alias (e.g., cross-binder `import X = C`
                // inside an ambient module), don't assume type-only.
                None => return false,
            }
        } else {
            export_equals_sym
        };

        // If alias resolution didn't fully resolve (symbol still only has ALIAS flag),
        // we can't determine if it's type-only. Conservatively assume it's NOT type-only
        // to avoid false TS2708 errors. This handles cases like:
        //   declare module 'M' { import X = C; export = X; }
        // where the export= -> X -> C chain can't be resolved across module boundaries.
        if let Some(resolved_sym) =
            export_equals_binder.get_symbol_with_libs(resolved_export_equals, &lib_binders)
            && resolved_sym.flags == symbol_flags::ALIAS
        {
            return false;
        }

        if let Some(export_symbol) =
            export_equals_binder.get_symbol_with_libs(resolved_export_equals, &lib_binders)
        {
            if (export_symbol.flags & symbol_flags::VALUE) == 0 {
                return true;
            }

            if (export_symbol.flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE))
                != 0
            {
                let mut has_runtime_value_member = false;

                // If the symbol also has non-namespace VALUE flags (CLASS, FUNCTION, etc.),
                // it's clearly a value and we don't need to check namespace members
                let non_namespace_value_flags = symbol_flags::VALUE & !(symbol_flags::VALUE_MODULE);
                if (export_symbol.flags & non_namespace_value_flags) != 0 {
                    has_runtime_value_member = true;
                }

                if !has_runtime_value_member && let Some(exports) = export_symbol.exports.as_ref() {
                    for (_, member_id) in exports.iter() {
                        if let Some(member_symbol) =
                            export_equals_binder.get_symbol_with_libs(*member_id, &lib_binders)
                            && (member_symbol.flags & symbol_flags::VALUE) != 0
                            && !self.symbol_member_is_type_only(*member_id, None)
                        {
                            has_runtime_value_member = true;
                            break;
                        }
                    }
                }

                if !has_runtime_value_member && let Some(members) = export_symbol.members.as_ref() {
                    for (_, member_id) in members.iter() {
                        if let Some(member_symbol) =
                            export_equals_binder.get_symbol_with_libs(*member_id, &lib_binders)
                            && (member_symbol.flags & symbol_flags::VALUE) != 0
                            && !self.symbol_member_is_type_only(*member_id, None)
                        {
                            has_runtime_value_member = true;
                            break;
                        }
                    }
                }

                if !has_runtime_value_member {
                    return true;
                }
            }
        }

        self.symbol_member_is_type_only(resolved_export_equals, Some("export="))
    }

    /// Check if a namespace member is transitively type-only through import chains.
    ///
    /// Resolves the namespace expression's alias chain to find the underlying
    /// namespace import, then checks if the member is type-only in the target
    /// module using cross-file type-only resolution.
    /// This catches cases like:
    ///   b.ts: `import A from './a'; export { A };`
    ///   a.ts: `export type { A as default };`
    /// where `A` is not explicitly type-only in b.ts but is transitively
    /// type-only through the `export type` in a.ts.
    pub(crate) fn is_namespace_member_transitively_type_only(
        &self,
        ns_expr: NodeIndex,
        member_name: &str,
    ) -> bool {
        let Some(ns_sym_id) = self.resolve_identifier_symbol(ns_expr) else {
            return false;
        };

        // Follow the alias chain to find the actual namespace import symbol.
        // For `import types from './c'` → `export { types as default }` →
        // `import * as types from './b'`, we need the final namespace import.
        let mut ns_visited = Vec::new();
        let resolved_ns = self
            .resolve_alias_symbol(ns_sym_id, &mut ns_visited)
            .unwrap_or(ns_sym_id);

        let lib_binders = self.get_lib_binders();

        // Check all symbols in the alias chain (including the resolved one)
        // to find one with an import_module pointing to the target module.
        let candidates = ns_visited
            .iter()
            .copied()
            .chain(std::iter::once(resolved_ns));
        for sym_id in candidates {
            let Some(sym) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
                continue;
            };
            let Some(ref import_module) = sym.import_module else {
                continue;
            };
            // Check if the member is type-only in the target module
            if self.is_export_type_only_across_binders(import_module, member_name) {
                return true;
            }
        }

        false
    }

    /// Returns true when this symbol is an import alias declared inside a module
    /// or global augmentation where imports are forbidden.
    pub(crate) fn symbol_is_import_alias_in_forbidden_augmentation(
        &self,
        sym_id: SymbolId,
    ) -> bool {
        let lib_binders = self.get_lib_binders();
        let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
            return false;
        };

        let symbol_arena: &NodeArena = self
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(|arena| arena.as_ref())
            .unwrap_or(self.ctx.arena);

        let is_alias_decl_in_forbidden_augmentation = |decl_idx: NodeIndex, arena: &NodeArena| {
            let Some(decl_node) = arena.get(decl_idx) else {
                return false;
            };
            if decl_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                return false;
            }

            let mut current = decl_idx;
            for _ in 0..16 {
                let Some(ext) = arena.get_extended(current) else {
                    return false;
                };
                let parent_idx = ext.parent;
                if parent_idx.is_none() {
                    return false;
                }
                let Some(parent_node) = arena.get(parent_idx) else {
                    return false;
                };
                if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                    if u32::from(parent_node.flags) & node_flags::GLOBAL_AUGMENTATION != 0 {
                        return true;
                    }
                    if let Some(module_decl) = arena.get_module(parent_node)
                        && let Some(name_node) = arena.get(module_decl.name)
                        && name_node.kind == SyntaxKind::StringLiteral as u16
                    {
                        return true;
                    }
                }
                current = parent_idx;
            }

            false
        };

        if symbol.value_declaration.is_some()
            && is_alias_decl_in_forbidden_augmentation(symbol.value_declaration, symbol_arena)
        {
            return true;
        }

        for &decl_idx in &symbol.declarations {
            if is_alias_decl_in_forbidden_augmentation(decl_idx, symbol_arena) {
                return true;
            }
        }

        false
    }

    /// Check if a namespace has a type-only member.
    ///
    /// This function determines if a specific property of a namespace
    /// is type-only (has TYPE flag but not VALUE flag).
    pub(crate) fn namespace_has_type_only_member(
        &self,
        object_type: TypeId,
        property_name: &str,
    ) -> bool {
        use tsz_solver::type_queries::{NamespaceMemberKind, classify_namespace_member};

        match classify_namespace_member(self.ctx.types, object_type) {
            // Handle Lazy types (direct namespace/module references)
            NamespaceMemberKind::Lazy(def_id) => {
                let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) else {
                    return false;
                };
                let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                    return false;
                };

                if symbol.flags & symbol_flags::MODULE == 0 {
                    return false;
                }

                let exports = symbol.exports.as_ref();

                let member_id = match exports
                    .and_then(|exports| exports.get(property_name))
                    .or_else(|| {
                        symbol
                            .members
                            .as_ref()
                            .and_then(|members| members.get(property_name))
                    }) {
                    Some(member_id) => member_id,
                    None => return false,
                };

                // Follow alias chains to determine if the ultimate target is type-only
                let resolved_member_id = if let Some(member_symbol) =
                    self.ctx.binder.get_symbol(member_id)
                    && member_symbol.flags & symbol_flags::ALIAS != 0
                {
                    let mut visited_aliases = Vec::new();
                    self.resolve_alias_symbol(member_id, &mut visited_aliases)
                        .unwrap_or(member_id)
                } else {
                    member_id
                };

                let member_symbol = match self.ctx.binder.get_symbol(resolved_member_id) {
                    Some(member_symbol) => member_symbol,
                    None => return false,
                };

                if self.symbol_member_is_type_only(resolved_member_id, Some(property_name)) {
                    return true;
                }

                let has_value =
                    (member_symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS)) != 0;
                let has_type = (member_symbol.flags & symbol_flags::TYPE) != 0;
                has_type && !has_value
            }

            NamespaceMemberKind::ModuleNamespace(_sym_ref) => {
                // For module namespace imports (`import * as ns from './mod'`),
                // type-only exports are completely absent from the value namespace.
                // tsc emits TS2339 ("property doesn't exist"), not TS2693
                // ("only refers to a type"). Return false here so the caller
                // falls through to the normal TS2339 path.
                false
            }

            // Handle Callable types from merged class+namespace or function+namespace symbols
            // For merged symbols, the namespace exports are stored as properties on the Callable
            NamespaceMemberKind::Callable(_) => {
                // Check if the property exists in the callable's properties
                if let Some(prop) = tsz_solver::type_queries::find_property_in_type_by_str(
                    self.ctx.types,
                    object_type,
                    property_name,
                ) {
                    return self.is_type_only_type(prop.type_id);
                }
                false
            }

            // TSZ-4: Handle Enum types - enum members are value members, not type-only
            NamespaceMemberKind::Enum(_def_id) => {
                // Enum members are always value members, never type-only
                false
            }

            // TypeQuery (typeof M): resolve to the underlying symbol type and re-check
            NamespaceMemberKind::TypeQuery(sym_ref) => {
                let sym_id = SymbolId(sym_ref.0);
                let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                    return false;
                };
                if symbol.flags & symbol_flags::MODULE == 0 {
                    return false;
                }
                let member_id = symbol
                    .exports
                    .as_ref()
                    .and_then(|exports| exports.get(property_name))
                    .or_else(|| {
                        symbol
                            .members
                            .as_ref()
                            .and_then(|members| members.get(property_name))
                    });
                let Some(member_id) = member_id else {
                    return false;
                };
                let Some(member_symbol) = self.ctx.binder.get_symbol(member_id) else {
                    return false;
                };
                if self.symbol_member_is_type_only(member_id, Some(property_name)) {
                    return true;
                }
                let has_value =
                    (member_symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS)) != 0;
                let has_type = (member_symbol.flags & symbol_flags::TYPE) != 0;
                has_type && !has_value
            }

            NamespaceMemberKind::Other => false,
        }
    }

    /// Check if an alias symbol resolves to a type-only symbol.
    ///
    /// Follows alias chains to determine if the ultimate target is type-only
    /// (has TYPE flag but not VALUE flag).
    pub(crate) fn alias_resolves_to_type_only(&self, sym_id: SymbolId) -> bool {
        let lib_binders = self.get_lib_binders();
        let symbol = match self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) {
            Some(symbol) => symbol,
            None => return false,
        };

        if symbol.flags & symbol_flags::ALIAS == 0 {
            return false;
        }
        // If the symbol has a VALUE binding (e.g., `import { X }` merged with
        // `const X = 42`), the value binding provides a runtime value and the
        // identifier should not be treated as type-only — regardless of whether
        // the import target is type-only.
        if (symbol.flags & symbol_flags::VALUE) != 0 {
            return false;
        }
        if symbol.is_type_only {
            return true;
        }
        if let Some(module_specifier) = symbol.import_module.as_deref() {
            // Namespace imports (import * as ns) and namespace re-exports
            // (export * as ns from) create value bindings — the namespace object.
            // They should not be treated as type-only even if the target module
            // only has type-only exports. Individual members surface as TS2339.
            let is_namespace_binding =
                symbol.import_name.is_none() || symbol.import_name.as_deref() == Some("*");
            let export_name = symbol
                .import_name
                .as_deref()
                .unwrap_or(&symbol.escaped_name);
            // Check across all binders for transitive type-only export chains
            if !is_namespace_binding
                && self.is_export_type_only_across_binders(module_specifier, export_name)
            {
                return true;
            }
            // Check if the target module has `export = X` where X is a type-only
            // import. This propagates type-only status through `export =` chains:
            //   // b.ts: import type * as ns from './a'; export = ns;
            //   // d.ts: import types from './b';         → type-only
            //   // e.ts: import types = require('./b');    → type-only
            //   // f.ts: import * as types from './b';    → type-only
            if self.is_module_export_equals_type_only(module_specifier) {
                return true;
            }
            // Namespace imports always create a value (the module namespace object),
            // regardless of whether internal exports are type-only. Don't walk the
            // alias chain for namespace bindings — individual type-only members
            // should surface as TS2339 at the property access site.
            if is_namespace_binding {
                return false;
            }
        }

        let mut visited = Vec::new();
        let target = match self.resolve_alias_symbol(sym_id, &mut visited) {
            Some(target) => target,
            None => return false,
        };

        // If any intermediate alias in the chain was marked type-only
        // (e.g. `export type { A }`), then the resolved symbol is type-only.
        for &alias_sym_id in &visited {
            if let Some(alias_sym) = self
                .ctx
                .binder
                .get_symbol_with_libs(alias_sym_id, &lib_binders)
                && alias_sym.is_type_only
            {
                return true;
            }
        }

        let target_symbol = match self.ctx.binder.get_symbol_with_libs(target, &lib_binders) {
            Some(target_symbol) => target_symbol,
            None => return false,
        };

        if target_symbol.is_type_only {
            return true;
        }

        let has_value = (target_symbol.flags & symbol_flags::VALUE) != 0;
        let has_type = (target_symbol.flags & symbol_flags::TYPE) != 0;
        has_type && !has_value
    }

    pub(crate) fn symbol_member_is_type_only(
        &self,
        sym_id: SymbolId,
        name_hint: Option<&str>,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let (symbol, arena) = if let Some(found) = self.lookup_symbol_with_name(sym_id, name_hint) {
            found
        } else if name_hint.is_some() {
            match self.lookup_symbol_with_name(sym_id, None) {
                Some(found) => found,
                None => return false,
            }
        } else {
            return false;
        };

        if symbol.is_type_only {
            return true;
        }

        // Note: We intentionally do NOT early-return for METHOD-flagged symbols
        // here. The METHOD flag is set for both interface method signatures
        // (type-only) and class method declarations (not type-only). The
        // declaration-based check below correctly distinguishes between these
        // cases by examining the parent node kind.

        let mut saw_declaration = false;
        let mut all_type_only = true;

        for &decl in &symbol.declarations {
            if decl.is_none() {
                continue;
            }
            let Some(node) = arena.get(decl) else {
                continue;
            };

            saw_declaration = true;

            let decl_is_type_only = match node.kind {
                k if k == syntax_kind_ext::METHOD_SIGNATURE
                    || k == syntax_kind_ext::PROPERTY_SIGNATURE
                    || k == syntax_kind_ext::CALL_SIGNATURE
                    || k == syntax_kind_ext::CONSTRUCT_SIGNATURE
                    || k == syntax_kind_ext::INDEX_SIGNATURE
                    || k == syntax_kind_ext::INTERFACE_DECLARATION
                    || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION =>
                {
                    true
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::PROPERTY_DECLARATION
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR =>
                {
                    if let Some(ext) = arena.get_extended(decl)
                        && let Some(parent) = arena.get(ext.parent)
                    {
                        parent.kind == syntax_kind_ext::INTERFACE_DECLARATION
                            || parent.kind == syntax_kind_ext::TYPE_LITERAL
                    } else {
                        false
                    }
                }
                _ => false,
            };

            if !decl_is_type_only {
                all_type_only = false;
                break;
            }
        }

        saw_declaration && all_type_only
    }

    /// Check if a type is type-only (has no runtime value).
    ///
    /// This is used for merged class+namespace symbols where namespace exports
    /// are stored as properties on the Callable type.
    fn is_type_only_type(&self, type_id: TypeId) -> bool {
        // Use resolve_type_to_symbol_id instead of get_ref_symbol
        if let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(type_id)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            let has_value = (symbol.flags & symbol_flags::VALUE) != 0;
            let has_type = (symbol.flags & symbol_flags::TYPE) != 0;
            return has_type && !has_value;
        }

        false
    }

    pub(crate) fn is_namespace_value_type(&self, object_type: TypeId) -> bool {
        use tsz_solver::type_queries::{NamespaceMemberKind, classify_namespace_member};

        // Check if a symbol is a pure namespace/enum (not merged with a class).
        // Class+namespace merges should be treated as class constructors, not namespaces,
        // so that property access (e.g., `.prototype`) goes through the solver path.
        const fn is_pure_namespace_or_enum(symbol: &tsz_binder::Symbol) -> bool {
            let is_namespace = (symbol.flags & symbol_flags::NAMESPACE) != 0;
            let is_class = (symbol.flags & symbol_flags::CLASS) != 0;
            is_namespace && !is_class
        }

        let kind = classify_namespace_member(self.ctx.types, object_type);
        match kind {
            NamespaceMemberKind::Lazy(def_id) => {
                let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) else {
                    return false;
                };
                // Resolve through alias chain to get the original symbol's flags.
                // Import aliases (e.g., `import { Foo }`) carry MODULE flags but not CLASS,
                // even when the original symbol is a class+namespace merge.
                let mut visited = Vec::new();
                let resolved_id = self
                    .resolve_alias_symbol(sym_id, &mut visited)
                    .unwrap_or(sym_id);
                let Some(symbol) = self
                    .ctx
                    .binder
                    .get_symbol(resolved_id)
                    .or_else(|| self.get_cross_file_symbol(resolved_id))
                else {
                    return false;
                };
                is_pure_namespace_or_enum(symbol)
            }
            NamespaceMemberKind::ModuleNamespace(sym_ref) => {
                let sym_id = SymbolId(sym_ref.0);
                let Some(symbol) = self.get_cross_file_symbol(sym_id) else {
                    return false;
                };
                is_pure_namespace_or_enum(symbol)
            }
            NamespaceMemberKind::Enum(_) => true,
            NamespaceMemberKind::TypeQuery(sym_ref) => {
                // TypeQuery (typeof M): check if the underlying symbol is a namespace
                let sym_id = SymbolId(sym_ref.0);
                let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                    return false;
                };
                is_pure_namespace_or_enum(symbol)
            }
            NamespaceMemberKind::Callable(_) | NamespaceMemberKind::Other => false,
        }
    }

    /// Resolve a property access on a `typeof Namespace` type by looking up the
    /// namespace symbol's exports directly.
    ///
    /// When a variable is typed as `typeof M` (where M is a namespace), the general
    /// property access pipeline may not find the property because it doesn't resolve
    /// the TypeQuery/Lazy type to an object with the namespace's exports. This helper
    /// extracts the namespace symbol from the TypeQuery or Lazy type and checks its
    /// exports table directly.
    pub(crate) fn resolve_namespace_typeof_member(
        &mut self,
        object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        use tsz_solver::type_queries::{NamespaceMemberKind, classify_namespace_member};

        let sym_id = match classify_namespace_member(self.ctx.types, object_type) {
            NamespaceMemberKind::TypeQuery(sym_ref) => SymbolId(sym_ref.0),
            NamespaceMemberKind::Lazy(def_id) => self.ctx.def_to_symbol_id(def_id)?,
            _ => return None,
        };

        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let exports = symbol.exports.as_ref()?;
        let member_sym_id = exports.get(property_name)?;
        let member_sym = self.ctx.binder.get_symbol(member_sym_id)?;
        let concrete_value = symbol_flags::VARIABLE
            | symbol_flags::FUNCTION
            | symbol_flags::CLASS
            | symbol_flags::ENUM
            | symbol_flags::VALUE_MODULE
            | symbol_flags::NAMESPACE_MODULE;

        if (member_sym.flags & concrete_value) == 0
            && self
                .ctx
                .resolve_symbol_file_index(member_sym_id)
                .is_some_and(|file_idx| self.file_has_jsdoc_typedef_named(file_idx, property_name))
        {
            return None;
        }

        // Only resolve value-side members. Type-only members (interfaces,
        // type aliases) should fall through to TS2693/TS2339 handling.
        if member_sym.flags & (symbol_flags::VALUE | symbol_flags::ALIAS) == 0 {
            return None;
        }

        // For enum members, return the runtime enum object type so that
        // property access on enum members (e.g., `m.Color.Blue`) works correctly.
        if member_sym.flags & symbol_flags::ENUM != 0
            && (member_sym.flags & symbol_flags::ENUM_MEMBER) == 0
        {
            let member_type = self
                .enum_object_type(member_sym_id)
                .unwrap_or_else(|| self.get_type_of_symbol(member_sym_id));
            return if member_type != TypeId::ERROR && member_type != TypeId::UNKNOWN {
                Some(member_type)
            } else {
                None
            };
        }

        // For merged interface+variable symbols, prefer the value declaration's type.
        let member_type = if member_sym.flags & symbol_flags::INTERFACE != 0
            && member_sym.flags & symbol_flags::VARIABLE != 0
            && member_sym.value_declaration.is_some()
        {
            self.type_of_value_declaration_for_symbol(member_sym_id, member_sym.value_declaration)
        } else {
            self.get_type_of_symbol(member_sym_id)
        };

        if member_type != TypeId::ERROR && member_type != TypeId::UNKNOWN {
            Some(member_type)
        } else {
            None
        }
    }

    /// Returns the name of an uninstantiated namespace if `expr_idx` resolves to one.
    ///
    /// Used to emit TS2708 instead of TS2339 when a property access targets an
    /// empty / type-only namespace (e.g., `namespace Basil { }` → `Basil.Pepper`).
    pub(crate) fn uninstantiated_namespace_name(&mut self, expr_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(expr_idx)?;
        let ident = self.ctx.arena.get_identifier(node)?;
        let name = &ident.escaped_text;

        let sym_id = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, expr_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;

        let is_namespace = (symbol.flags & symbol_flags::NAMESPACE_MODULE) != 0;
        let value_flags_except_module = symbol_flags::VALUE & !symbol_flags::VALUE_MODULE;
        let has_other_value = (symbol.flags & value_flags_except_module) != 0;

        if !is_namespace || has_other_value {
            return None;
        }

        let value_type = self.type_of_value_symbol_by_name(name);
        if !matches!(value_type, TypeId::UNKNOWN | TypeId::ERROR) {
            return None;
        }

        // A shadowing namespace should not trigger TS2708 when some other
        // project/lib binder still provides a real runtime value with this name.
        if self.has_non_umd_global_value(name) {
            return None;
        }

        // Check whether any declaration is instantiated (has runtime code).
        let is_instantiated = symbol
            .declarations
            .iter()
            .any(|&decl_idx| self.is_namespace_declaration_instantiated(decl_idx));

        if is_instantiated {
            None
        } else {
            Some(name.to_string())
        }
    }

    /// Check if a property access is on an enum instance value (not the enum object).
    ///
    /// Returns `true` when the object type is an enum type AND the expression
    /// is NOT a direct reference to the enum declaration. This distinguishes:
    /// - `x.toString()` where `x: Foo` → true (enum instance, should resolve apparent type)
    /// - `Foo.nonExistent` → false (direct enum reference, should error)
    pub(crate) fn is_enum_instance_property_access(
        &self,
        object_type: TypeId,
        expression: NodeIndex,
    ) -> bool {
        use tsz_solver::type_queries::{NamespaceMemberKind, classify_namespace_member};

        // Only applies to enum types
        if !matches!(
            classify_namespace_member(self.ctx.types, object_type),
            NamespaceMemberKind::Enum(_)
        ) {
            return false;
        }

        // Check if the expression is a direct reference to an enum declaration
        if let Some(sym_id) = self.resolve_identifier_symbol(expression)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && (symbol.flags & symbol_flags::ENUM) != 0
        {
            // Direct enum reference (e.g., `Foo.toString()`) - NOT an instance access
            return false;
        }

        // The expression is a variable/parameter/property with an enum type
        // (e.g., `x.toString()` where `let x: Foo`)
        true
    }

    /// Check if a symbol is type-only (from `import type`).
    ///
    /// This is used to allow type-only imports in type positions while
    /// preventing their use in value positions.
    pub(crate) fn symbol_is_type_only(&self, sym_id: SymbolId, name_hint: Option<&str>) -> bool {
        self.lookup_symbol_with_name(sym_id, name_hint)
            .is_some_and(|(symbol, _arena)| symbol.is_type_only)
    }

    /// Check if an export is type-only by resolving across file boundaries.
    ///
    /// Uses the checker's module resolution (`resolve_import_target` → `get_binder_for_file`)
    /// to follow transitive type-only chains: if module A does `export type { X }`,
    /// module B imports X and re-exports with `export { X }`, X is still type-only.
    pub(crate) fn is_export_type_only_across_binders(
        &self,
        module_specifier: &str,
        export_name: &str,
    ) -> bool {
        let mut visited = rustc_hash::FxHashSet::default();
        self.is_export_type_only_in_file(
            self.ctx.current_file_idx,
            module_specifier,
            export_name,
            &mut visited,
        )
    }

    /// Like `is_export_type_only_across_binders` but resolves the module specifier
    /// from a specific source file index. This is needed for cross-file namespace
    /// type construction where the module specifier is relative to the declaring
    /// file, not the current file being checked.
    pub(crate) fn is_export_type_only_from_file(
        &self,
        module_specifier: &str,
        export_name: &str,
        source_file_idx: Option<usize>,
    ) -> bool {
        let source = source_file_idx.unwrap_or(self.ctx.current_file_idx);
        let mut visited = rustc_hash::FxHashSet::default();
        self.is_export_type_only_in_file(source, module_specifier, export_name, &mut visited)
    }

    /// Check if a module has `export = X` where X is a type-only import or a pure type.
    /// This propagates type-only status through `export =` chains, e.g.:
    ///   `import type * as ns from './a'; export = ns;`
    ///   `declare type T = number; export = T;`
    /// Any import from such a module inherits the type-only status.
    pub(crate) fn is_module_export_equals_type_only(&self, module_specifier: &str) -> bool {
        let Some(target_idx) = self.ctx.resolve_import_target(module_specifier) else {
            return false;
        };
        let Some(target_binder) = self.ctx.get_binder_for_file(target_idx) else {
            return false;
        };
        let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
        let Some(file_name) = target_arena.source_files.first().map(|f| &f.file_name) else {
            return false;
        };
        let Some(exports) = target_binder.module_exports.get(file_name) else {
            return false;
        };
        let Some(export_eq_sym_id) = exports.get("export=") else {
            return false;
        };

        const PURE_TYPE: u32 = symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS;
        const VALUE: u32 = symbol_flags::VARIABLE
            | symbol_flags::FUNCTION
            | symbol_flags::CLASS
            | symbol_flags::ENUM
            | symbol_flags::ENUM_MEMBER
            | symbol_flags::VALUE_MODULE;

        let lib_binders = self.get_lib_binders();

        // Check the export= symbol in the main binder (merged arena)
        if let Some(eq_sym) = self
            .ctx
            .binder
            .get_symbol_with_libs(export_eq_sym_id, &lib_binders)
        {
            if eq_sym.is_type_only {
                return true;
            }
            // Check if the export= target is a pure type (type alias / interface)
            if (eq_sym.flags & PURE_TYPE) != 0 && (eq_sym.flags & VALUE) == 0 {
                return true;
            }
            // Follow the alias: if export= points to a type-only import, propagate
            if eq_sym.flags & symbol_flags::ALIAS != 0 {
                let mut visited = Vec::new();
                if let Some(resolved) = self.resolve_alias_symbol(export_eq_sym_id, &mut visited) {
                    // Check any intermediate alias in the chain
                    for &alias_id in &visited {
                        if let Some(alias_sym) =
                            self.ctx.binder.get_symbol_with_libs(alias_id, &lib_binders)
                            && alias_sym.is_type_only
                        {
                            return true;
                        }
                    }
                    // Check the final target
                    if let Some(target_sym) =
                        self.ctx.binder.get_symbol_with_libs(resolved, &lib_binders)
                    {
                        if target_sym.is_type_only {
                            return true;
                        }
                        if (target_sym.flags & PURE_TYPE) != 0 && (target_sym.flags & VALUE) == 0 {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    /// Resolve a module specifier from a given source file, then check if
    /// `export_name` in that target module is type-only.
    fn is_export_type_only_in_file(
        &self,
        source_file_idx: usize,
        module_specifier: &str,
        export_name: &str,
        visited: &mut rustc_hash::FxHashSet<(usize, String)>,
    ) -> bool {
        // Resolve the specifier to a target file index
        let Some(target_file_idx) = self
            .ctx
            .resolve_import_target_from_file(source_file_idx, module_specifier)
        else {
            return false;
        };

        let key = (target_file_idx, export_name.to_string());
        if !visited.insert(key) {
            return false; // cycle
        }

        let Some(target_binder) = self.ctx.get_binder_for_file(target_file_idx) else {
            return false;
        };

        // Get the target file's canonical name (module_exports key)
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let Some(target_file_name) = target_arena
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
        else {
            return false;
        };

        // Check direct exports in target binder
        if let Some(exports_table) = target_binder.module_exports.get(&target_file_name)
            && let Some(sym_id) = exports_table.get(export_name)
        {
            // Look up the symbol using the target binder first (which owns the export),
            // then fall back to the main binder (for merged/remapped symbol arenas in
            // the full pipeline). In per-file binder setups, SymbolIds are local to each
            // file, so `self.ctx.binder.get_symbol(sym_id)` may return a wrong symbol
            // from the current file at the same index.
            let sym_opt = target_binder
                .get_symbol(sym_id)
                .or_else(|| self.ctx.binder.get_symbol(sym_id));
            if let Some(sym) = sym_opt {
                if sym.is_type_only {
                    // A merged symbol like `import type { A }` + `const A = 0`
                    // has both ALIAS and VALUE flags. The value binding overrides
                    // type-only status. But cloned `export type { A as default }`
                    // symbols copy the source's value flags (e.g., CLASS) without
                    // ALIAS, so we only skip when ALIAS+VALUE are both present.
                    if sym.flags & symbol_flags::ALIAS == 0 || sym.flags & symbol_flags::VALUE == 0
                    {
                        return true;
                    }
                }
                let concrete_value = symbol_flags::VARIABLE
                    | symbol_flags::FUNCTION
                    | symbol_flags::CLASS
                    | symbol_flags::ENUM
                    | symbol_flags::VALUE_MODULE
                    | symbol_flags::NAMESPACE_MODULE;
                if (sym.flags & concrete_value) == 0
                    && self.file_has_jsdoc_typedef_named(target_file_idx, export_name)
                {
                    return true;
                }
                // Follow import alias chains transitively, but only if the
                // symbol doesn't have a concrete runtime value binding.
                // A merged symbol like `import { A }` + `const A = 0` (VARIABLE)
                // provides a real value and overrides type-only from the import.
                // But `namespace A {}` (VALUE_MODULE) alone doesn't override.
                let concrete_value = symbol_flags::VARIABLE
                    | symbol_flags::FUNCTION
                    | symbol_flags::CLASS
                    | symbol_flags::ENUM;
                if sym.flags & symbol_flags::ALIAS != 0
                    && sym.flags & concrete_value == 0
                    && let Some(ref import_module) = sym.import_module
                {
                    let import_name = sym.import_name.as_deref().unwrap_or(&sym.escaped_name);
                    if self.is_export_type_only_in_file(
                        target_file_idx,
                        import_module,
                        import_name,
                        visited,
                    ) {
                        return true;
                    }
                }
                // Direct export exists and is not type-only — don't check wildcard re-exports.
                return false;
            }
        }

        // Check named re-exports
        if let Some(file_reexports) = target_binder.reexports.get(&target_file_name)
            && let Some((source_module, original_name)) = file_reexports.get(export_name)
        {
            let name_to_lookup = original_name.as_deref().unwrap_or(export_name);
            return self.is_export_type_only_in_file(
                target_file_idx,
                source_module,
                name_to_lookup,
                visited,
            );
        }

        // Check wildcard re-exports (only if no direct export was found).
        // Two-pass approach: first check if any non-type-only wildcard provides
        // a value binding for the name (which overrides type-only from other
        // wildcards), then check type-only wildcards.
        if let Some(source_modules) = target_binder.wildcard_reexports.get(&target_file_name) {
            let source_type_only_flags = target_binder
                .wildcard_reexports_type_only
                .get(&target_file_name);

            // Pass 1: Check non-type-only wildcards for value exports.
            // If a non-type-only `export *` re-exports the name AND the name is
            // not type-only in the source module, the value binding takes precedence
            // over any type-only wildcard (even if a `export type *` also has it).
            // Note: `name_exists_in_module_exports` only checks existence,
            // `is_export_type_only_in_file` checks the full type-only chain.
            for (i, source_module) in source_modules.iter().enumerate() {
                let source_is_type_only = source_type_only_flags
                    .and_then(|flags| flags.get(i).map(|(_, is_to)| *is_to))
                    .unwrap_or(false);
                if source_is_type_only {
                    continue; // Skip type-only wildcards in pass 1
                }
                // Non-type-only wildcard: check if name exists as a value in source.
                // Use a separate visited set for the existence + type-only check
                // to avoid polluting the main cycle detection.
                let mut exists_visited = visited.clone();
                let exists_in_source = self.name_exists_in_module_exports(
                    target_file_idx,
                    source_module,
                    export_name,
                    &mut exists_visited,
                );
                if exists_in_source {
                    let mut type_only_visited = visited.clone();
                    let is_type_only_in_source = self.is_export_type_only_in_file(
                        target_file_idx,
                        source_module,
                        export_name,
                        &mut type_only_visited,
                    );
                    if !is_type_only_in_source {
                        // Value export found — name is NOT type-only
                        return false;
                    }
                }
            }

            // In JS files, `export type *` is a syntax error (TS8006), not a
            // semantic type-only marker. Skip type-only wildcard semantics for JS files.
            let target_is_js = target_file_name.ends_with(".js")
                || target_file_name.ends_with(".jsx")
                || target_file_name.ends_with(".mjs")
                || target_file_name.ends_with(".cjs");

            // Pass 2: Check type-only wildcards and transitive chains
            for (i, source_module) in source_modules.iter().enumerate() {
                let source_is_type_only = source_type_only_flags
                    .and_then(|flags| flags.get(i).map(|(_, is_to)| *is_to))
                    .unwrap_or(false);
                if source_is_type_only {
                    // In JS files, `export type` is invalid syntax — don't treat as type-only
                    if target_is_js {
                        continue;
                    }
                    // Type-only wildcard: verify the name actually exists in the source
                    if self.name_exists_in_module_exports(
                        target_file_idx,
                        source_module,
                        export_name,
                        visited,
                    ) {
                        return true;
                    }
                    continue;
                }
                // Non-type-only wildcard: check for transitive type-only chains
                if self.is_export_type_only_in_file(
                    target_file_idx,
                    source_module,
                    export_name,
                    visited,
                ) {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a name exists as an export in a module (regardless of type-only status).
    ///
    /// Used to verify that a specific name is actually re-exported through a
    /// wildcard `export type *` before marking it as type-only.
    fn name_exists_in_module_exports(
        &self,
        source_file_idx: usize,
        module_specifier: &str,
        export_name: &str,
        visited: &mut rustc_hash::FxHashSet<(usize, String)>,
    ) -> bool {
        let Some(target_file_idx) = self
            .ctx
            .resolve_import_target_from_file(source_file_idx, module_specifier)
        else {
            return false;
        };

        let key = (target_file_idx, format!("exists:{export_name}"));
        if !visited.insert(key) {
            return false; // cycle
        }

        let Some(target_binder) = self.ctx.get_binder_for_file(target_file_idx) else {
            return false;
        };

        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let Some(target_file_name) = target_arena
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
        else {
            return false;
        };

        // Check direct exports
        if let Some(exports_table) = target_binder.module_exports.get(&target_file_name)
            && exports_table.get(export_name).is_some()
        {
            return true;
        }

        // Check named re-exports
        if let Some(file_reexports) = target_binder.reexports.get(&target_file_name)
            && file_reexports.get(export_name).is_some()
        {
            return true;
        }

        // Check wildcard re-exports recursively
        if let Some(source_modules) = target_binder.wildcard_reexports.get(&target_file_name) {
            for source_module in source_modules.iter() {
                if self.name_exists_in_module_exports(
                    target_file_idx,
                    source_module,
                    export_name,
                    visited,
                ) {
                    return true;
                }
            }
        }

        false
    }
}
