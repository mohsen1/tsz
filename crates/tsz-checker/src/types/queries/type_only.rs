//! Type-only symbol and namespace detection utilities.
//!
//! Determines whether symbols, imports, and namespace members are "type-only"
//! (exist only at the type level with no runtime value). Used by the checker
//! to decide when to emit TS2708 and related diagnostics.

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
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

        let normalized = module_name.trim_matches('"').trim_matches('\'');
        let quoted = format!("\"{normalized}\"");
        let single_quoted = format!("'{normalized}'");

        let export_equals_sym = self
            .ctx
            .binder
            .module_exports
            .get(module_name)
            .and_then(|exports| exports.get("export="))
            .or_else(|| {
                self.ctx
                    .binder
                    .module_exports
                    .get(normalized)
                    .and_then(|exports| exports.get("export="))
            })
            .or_else(|| {
                self.ctx
                    .binder
                    .module_exports
                    .get(&quoted)
                    .and_then(|exports| exports.get("export="))
            })
            .or_else(|| {
                self.ctx
                    .binder
                    .module_exports
                    .get(&single_quoted)
                    .and_then(|exports| exports.get("export="))
            });

        let Some(export_equals_sym) = export_equals_sym else {
            return false;
        };

        let resolved_export_equals = if let Some(export_sym) = self
            .ctx
            .binder
            .get_symbol_with_libs(export_equals_sym, &lib_binders)
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
        if let Some(resolved_sym) = self
            .ctx
            .binder
            .get_symbol_with_libs(resolved_export_equals, &lib_binders)
            && resolved_sym.flags == symbol_flags::ALIAS
        {
            return false;
        }

        if let Some(export_symbol) = self
            .ctx
            .binder
            .get_symbol_with_libs(resolved_export_equals, &lib_binders)
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
                        if let Some(member_symbol) = self
                            .ctx
                            .binder
                            .get_symbol_with_libs(*member_id, &lib_binders)
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
                        if let Some(member_symbol) = self
                            .ctx
                            .binder
                            .get_symbol_with_libs(*member_id, &lib_binders)
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

            NamespaceMemberKind::ModuleNamespace(sym_ref) => {
                let sym_id = SymbolId(sym_ref.0);
                let Some(symbol) = self.get_cross_file_symbol(sym_id) else {
                    return false;
                };

                if symbol.flags & symbol_flags::MODULE == 0 {
                    return false;
                }

                let member_id = match symbol
                    .exports
                    .as_ref()
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

                let resolved_member_id = if let Some(member_symbol) =
                    self.get_cross_file_symbol(member_id)
                    && member_symbol.flags & symbol_flags::ALIAS != 0
                {
                    let mut visited_aliases = Vec::new();
                    self.resolve_alias_symbol(member_id, &mut visited_aliases)
                        .unwrap_or(member_id)
                } else {
                    member_id
                };

                if self.symbol_member_is_type_only(resolved_member_id, Some(property_name)) {
                    return true;
                }

                let Some(member_symbol) = self.get_cross_file_symbol(resolved_member_id) else {
                    return false;
                };

                let has_value =
                    (member_symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS)) != 0;
                let has_type = (member_symbol.flags & symbol_flags::TYPE) != 0;
                has_type && !has_value
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
        if symbol.is_type_only {
            return true;
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

        if (symbol.flags & symbol_flags::METHOD) != 0
            && (symbol.flags & symbol_flags::FUNCTION) == 0
        {
            return true;
        }

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
                    || k == syntax_kind_ext::INDEX_SIGNATURE =>
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

        match classify_namespace_member(self.ctx.types, object_type) {
            NamespaceMemberKind::Lazy(def_id) => {
                let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) else {
                    return false;
                };
                let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                    return false;
                };
                (symbol.flags & (symbol_flags::MODULE | symbol_flags::ENUM)) != 0
            }
            NamespaceMemberKind::ModuleNamespace(sym_ref) => {
                let sym_id = SymbolId(sym_ref.0);
                let Some(symbol) = self.get_cross_file_symbol(sym_id) else {
                    return false;
                };
                (symbol.flags & (symbol_flags::MODULE | symbol_flags::ENUM)) != 0
            }
            NamespaceMemberKind::Enum(_) => true,
            NamespaceMemberKind::Callable(_) | NamespaceMemberKind::Other => false,
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
}
