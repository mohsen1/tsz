//! Enum, namespace, and global-this property access fast paths.

use crate::state::CheckerState;
use crate::types_domain::queries::core::GlobalReceiver;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Fast path for enum/namespace member value access (`E.Member` or `Ns.Member`).
    /// Returns Some(type) if this is an enum/namespace member access that can be resolved
    /// directly, None otherwise (fall through to general property-access pipeline).
    pub(super) fn try_resolve_enum_namespace_member_access(
        &mut self,
        idx: NodeIndex,
        expression: NodeIndex,
        name_or_argument: NodeIndex,
        name_node: &tsz_parser::parser::node::Node,
        skip_flow_narrowing: bool,
    ) -> Option<TypeId> {
        let name_ident = self.ctx.arena.get_identifier(name_node)?;
        let property_name = &name_ident.escaped_text;

        let is_identifier_base = self
            .ctx
            .arena
            .get(expression)
            .is_some_and(|expr_node| expr_node.kind == SyntaxKind::Identifier as u16);

        if !is_identifier_base {
            return None;
        }

        let base_sym_id = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, expression)?;
        let base_symbol = self.ctx.binder.get_symbol(base_sym_id)?;

        // When the binder resolves an import to an intermediate alias (e.g.,
        // re-exported enums: `export { E } from './source'`), follow the
        // alias chain to find the actual enum/namespace symbol.
        //
        // For merged alias + namespace (`import { E } from './e'; namespace E { ... }`),
        // the base symbol carries both ALIAS and VALUE_MODULE flags. Prefer the
        // base symbol's own exports first, then fall back to the alias target's
        // exports so that enum members from the aliased source remain reachable.
        let (resolved_sym_id, resolved_flags) = if base_symbol.has_any_flags(symbol_flags::ALIAS)
            && !base_symbol.has_any_flags(symbol_flags::ENUM | symbol_flags::VALUE_MODULE)
        {
            let mut visited = crate::symbols_domain::alias_cycle::AliasCycleTracker::new();
            if let Some(target_id) = self.resolve_alias_symbol(base_sym_id, &mut visited) {
                let target_flags = self
                    .get_cross_file_symbol(target_id)
                    .or_else(|| self.ctx.binder.get_symbol(target_id))
                    .map_or(0, |s| s.flags);
                (target_id, target_flags)
            } else {
                (base_sym_id, base_symbol.flags)
            }
        } else {
            (base_sym_id, base_symbol.flags)
        };

        if resolved_flags & (symbol_flags::ENUM | symbol_flags::VALUE_MODULE) == 0 {
            return None;
        }

        // Extract data from resolved symbol before taking mutable borrows below.
        // If the member is missing from the resolved symbol's own exports and the
        // original base symbol is a merged alias + namespace, follow the alias to
        // consult the aliased target's exports (const-enum members accessible via
        // a re-exported + locally-merged namespace).
        let base_has_alias = base_symbol.has_any_flags(symbol_flags::ALIAS);
        let (member_sym_id, resolved_value_decl, resolved_first_decl, resolved_is_ambient) = {
            let resolved_symbol = self
                .get_cross_file_symbol(resolved_sym_id)
                .or_else(|| self.ctx.binder.get_symbol(resolved_sym_id))?;
            let own_member = resolved_symbol
                .exports
                .as_ref()
                .and_then(|e| e.get(property_name));
            let value_decl = resolved_symbol.value_declaration;
            let first_decl = resolved_symbol.declarations.first().copied();
            let is_ambient = self.is_const_enum_ambient(resolved_sym_id, resolved_symbol);
            (own_member, value_decl, first_decl, is_ambient)
        };
        let (member_sym_id, resolved_flags, resolved_is_ambient) = if let Some(id) = member_sym_id {
            (id, resolved_flags, resolved_is_ambient)
        } else if base_has_alias && resolved_sym_id == base_sym_id {
            // Merged alias + namespace: the namespace's own exports don't have
            // this member. Follow the alias to the aliased target.
            let mut visited = crate::symbols_domain::alias_cycle::AliasCycleTracker::new();
            let alias_target = self.resolve_alias_symbol(base_sym_id, &mut visited)?;
            let (alias_member, alias_flags, alias_is_ambient) = {
                let alias_sym = self
                    .get_cross_file_symbol(alias_target)
                    .or_else(|| self.ctx.binder.get_symbol(alias_target))?;
                let id = alias_sym.exports.as_ref()?.get(property_name)?;
                (
                    id,
                    alias_sym.flags,
                    self.is_const_enum_ambient(alias_target, alias_sym),
                )
            };
            if alias_flags & (symbol_flags::ENUM | symbol_flags::VALUE_MODULE) == 0 {
                return None;
            }
            (alias_member, alias_flags, alias_is_ambient)
        } else {
            return None;
        };

        // For namespace members, only use the fast path when the export has
        // value semantics (VARIABLE, CLASS, FUNCTION, etc.) or is an alias
        // (export import). Type-only exports (interfaces, type aliases) must go
        // through the general property-access path so that TS2708/TS2693
        // diagnostics are properly emitted.
        let member_has_value_semantics = self
            .ctx
            .binder
            .get_symbol(member_sym_id)
            .is_some_and(|s| s.flags & (symbol_flags::VALUE | symbol_flags::ALIAS) != 0);
        if !member_has_value_semantics {
            return None;
        }
        if resolved_flags & symbol_flags::CONST_ENUM != 0
            && resolved_flags & symbol_flags::VALUE_MODULE != 0
            && !self
                .ctx
                .binder
                .get_symbol(member_sym_id)
                .is_some_and(|s| s.has_any_flags(symbol_flags::ENUM_MEMBER))
        {
            let display_type = self
                .ctx
                .enum_namespace_types
                .get(&resolved_sym_id)
                .copied()
                .unwrap_or(TypeId::ANY);
            self.error_property_not_exist_at(property_name, display_type, name_or_argument);
            return Some(TypeId::ERROR);
        }

        // For merged symbols (e.g., namespace + interface), verify that the VALUE
        // part is actually exported. If only the TYPE part is exported, the value
        // is not accessible and we should fall through to emit TS2339.
        if !self.symbol_has_exported_value_declaration(member_sym_id) {
            return None;
        }

        let is_enum = resolved_flags & symbol_flags::ENUM != 0;

        // TS1361/TS1362: Check if the base identifier is a type-only import.
        if let Some(local_sym_id) = self.resolve_identifier_symbol(expression)
            && self.alias_resolves_to_type_only(local_sym_id)
            && let Some(base_node) = self.ctx.arena.get(expression)
            && let Some(base_ident) = self.ctx.arena.get_identifier(base_node)
            && !self
                .source_file_has_value_import_binding_named(expression, &base_ident.escaped_text)
        {
            self.report_wrong_meaning_diagnostic(
                &base_ident.escaped_text,
                expression,
                crate::query_boundaries::name_resolution::NameLookupKind::Type,
            );
            return Some(TypeId::ERROR);
        }

        if is_enum {
            // TS2450: Check if enum is used before its declaration (TDZ violation).
            if let Some(base_node) = self.ctx.arena.get(expression)
                && let Some(base_ident) = self.ctx.arena.get_identifier(base_node)
            {
                let base_name = &base_ident.escaped_text;
                if self.check_tdz_violation(base_sym_id, expression, base_name, true) {
                    return Some(TypeId::ERROR);
                }
            }

            // TS2748: Cannot access ambient const enums when isolatedModules /
            // verbatimModuleSyntax is enabled.
            //
            // tsc gates the *access-site* diagnostic on
            //   rawIsolatedModules || (verbatimModuleSyntax && firstId is not an alias)
            // (`checkConstEnumAccess`). Under verbatimModuleSyntax alone an
            // *imported* const enum is reported once at the import statement
            // instead, so the access site stays silent when the base identifier
            // resolves to an import alias. A locally-declared const enum (no
            // alias) is still reported at the access site under
            // verbatimModuleSyntax. Raw `isolatedModules` always reports here.
            //
            // An imported const enum can only be named through an import alias, so
            // "first identifier is an alias" reduces to "the const enum is declared
            // in another file". `base_sym_id` is already alias-resolved (the binder
            // follows imports), so compare the resolved const enum's declaring file
            // against the current file. The const-enum/ambient guards come first so
            // the cross-file declaring-file lookup only runs for the rare
            // verbatim-only ambient const enum access.
            if resolved_flags & symbol_flags::CONST_ENUM != 0
                && resolved_is_ambient
                && !self.is_in_type_only_position(idx)
                && (self.ctx.raw_isolated_modules()
                    || (self.ctx.compiler_options.verbatim_module_syntax
                        && !self.symbol_is_imported(resolved_sym_id)))
            {
                let option_name = if self.ctx.compiler_options.verbatim_module_syntax {
                    "verbatimModuleSyntax"
                } else {
                    "isolatedModules"
                };
                let msg = crate::diagnostics::format_message(
                    crate::diagnostics::diagnostic_messages::CANNOT_ACCESS_AMBIENT_CONST_ENUMS_WHEN_IS_ENABLED,
                    &[option_name],
                );
                self.error_at_node(
                    idx,
                    &msg,
                    crate::diagnostics::diagnostic_codes::CANNOT_ACCESS_AMBIENT_CONST_ENUMS_WHEN_IS_ENABLED,
                );
            }
        }

        // TS2729 for namespace member access in static property initializers.
        // Methods are hoisted and don't need initialization, so skip them.
        let member_is_method = self
            .get_cross_file_symbol(member_sym_id)
            .or_else(|| self.ctx.binder.get_symbol(member_sym_id))
            .is_some_and(|s| s.has_any_flags(symbol_flags::METHOD));
        if resolved_flags & symbol_flags::VALUE_MODULE != 0
            && !member_is_method
            && self.is_in_static_property_initializer_ast_context(expression)
            && self.find_enclosing_computed_property(expression).is_none()
        {
            let decl_idx = if resolved_value_decl.is_some() {
                resolved_value_decl
            } else if let Some(first_decl) = resolved_first_decl {
                first_decl
            } else {
                NodeIndex::NONE
            };
            if decl_idx.is_some()
                && let Some(usage_node) = self.ctx.arena.get(expression)
                && let Some(decl_node) = self.ctx.arena.get(decl_idx)
                && usage_node.pos < decl_node.pos
            {
                self.error_at_node(
                    name_or_argument,
                    &format!(
                        "Property '{}' is used before its initialization.",
                        name_ident.escaped_text
                    ),
                    tsz_common::diagnostics::diagnostic_codes::PROPERTY_IS_USED_BEFORE_ITS_INITIALIZATION,
                );
            }
        }

        // Resolve the member type.
        let member_sym = self
            .get_cross_file_symbol(member_sym_id)
            .or_else(|| self.ctx.binder.get_symbol(member_sym_id));
        let member_is_enum_object = member_sym.is_some_and(|s| {
            s.has_any_flags(symbol_flags::ENUM) && !s.has_any_flags(symbol_flags::ENUM_MEMBER)
        });
        let member_type = if member_is_enum_object {
            // Value-position access of a namespace's nested enum
            // (`Ns.SomeEnum`) must yield the enum's VALUE meaning — the
            // `typeof SomeEnum` object carrying the static member keys — not the
            // enum instance type (the union of member literals). `get_type_of_symbol`
            // can return either depending on which position resolved the enum
            // first; through a re-export hop the type-position resolves first and
            // caches the instance type, dropping the static keys (TS2339 on
            // `Ns.SomeEnum.Member`). Mirror the identifier value-position path and
            // `build_namespace_object_type`, which both convert the enum member to
            // its enum object type via `get_enum_namespace_type_for_value`.
            let base = self.get_type_of_symbol(member_sym_id);
            self.get_enum_namespace_type_for_value(base)
        } else if let Some(member_sym) = member_sym
            && member_sym.has_any_flags(symbol_flags::INTERFACE)
            && member_sym.has_any_flags(symbol_flags::VARIABLE)
            && member_sym.value_declaration.is_some()
        {
            self.type_of_value_declaration_for_symbol(member_sym_id, member_sym.value_declaration)
        } else {
            self.get_type_of_symbol(member_sym_id)
        };

        Some(self.finalize_property_access_result(idx, member_type, skip_flow_narrowing, false))
    }

    /// Handles property access on globalThis or Window-like expressions.
    /// Returns Some(type) if this is a globalThis/Window access, None otherwise.
    pub(super) fn try_resolve_global_this_property_access(
        &mut self,
        idx: NodeIndex,
        expression: NodeIndex,
        name_or_argument: NodeIndex,
        property_name: &str,
        skip_flow_narrowing: bool,
    ) -> Option<TypeId> {
        let is_this_global = self.is_this_resolving_to_global(expression);
        let is_global_this = self.is_global_this_expression(expression);
        let is_global_this_like = is_global_this || self.is_global_this_like_expression(expression);
        let is_declared_window_global_this =
            self.is_window_and_global_this_declared_expression(expression);
        if !(is_global_this_like || is_this_global || is_declared_window_global_this) {
            return None;
        }
        let targets_global_this = is_global_this || is_this_global;
        let receiver = GlobalReceiver::from_targets_global_this(targets_global_this);
        let allow_unknown_property_fallback =
            targets_global_this && !is_declared_window_global_this;
        let property_type = self.resolve_global_this_property_type(
            property_name,
            name_or_argument,
            allow_unknown_property_fallback,
            receiver,
        );
        if property_type == TypeId::ERROR {
            return Some(TypeId::ERROR);
        }
        // TS7017 for missing `typeof globalThis` member access under noImplicitAny.
        let access_targets_global_this =
            is_this_global || self.is_global_this_expression(expression);
        if access_targets_global_this && property_type == TypeId::ANY && self.ctx.no_implicit_any()
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            self.error_at_node(
                name_or_argument,
                &format_message(
                    diagnostic_messages::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_TYPE_HAS_NO_INDEX_SIGNATURE,
                    &["typeof globalThis"],
                ),
                diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_TYPE_HAS_NO_INDEX_SIGNATURE,
            );
        }

        Some(self.finalize_property_access_result(idx, property_type, skip_flow_narrowing, false))
    }
}
