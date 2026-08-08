//! Lib symbol lookup, global value resolution, heritage symbol resolution,
//! test option parsing, and access class resolution.

use crate::state::{CheckerState, MAX_TREE_WALK_ITERATIONS};
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tracing::trace;
use tsz_binder::symbol_flags::CLASS;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{Node, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

/// Whether `node` is a string-literal ("external") ambient module declaration
/// such as `declare module "foo"` (or, defensively, a template-literal name).
///
/// String-literal ambient modules are not part of the global scope: `tsc`
/// records them in a separate ambient-module table, so they never seed
/// `program.globals` and never become properties of `typeof globalThis`.
/// Identifier-named namespaces (`declare module M {}`, `namespace M {}`) are
/// genuine globals and must be retained, so the test keys on the declaration's
/// *name* node being a string literal rather than on any module symbol flag,
/// which cannot distinguish the two forms.
fn declaration_is_string_literal_module(arena: &NodeArena, node: &Node) -> bool {
    if node.kind != syntax_kind_ext::MODULE_DECLARATION {
        return false;
    }
    let Some(module) = arena.get_module(node) else {
        return false;
    };
    arena.get(module.name).is_some_and(|name_node| {
        name_node.kind == SyntaxKind::StringLiteral as u16
            || name_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
    })
}

/// Whether every (resolvable) declaration of `symbol` is a string-literal
/// ("external") ambient module. Such a symbol contributes no global value, so
/// it must not appear on the `typeof globalThis` surface and indexing the
/// surface by its name is a `TS2339`. A symbol that also carries a genuine
/// value declaration is not module-only and stays on the surface.
///
/// `arena` must be the arena owning `symbol`'s declarations. For symbols whose
/// declarations span lib arenas, prefer the per-declaration
/// [`CheckerState::symbol_has_globalable_declaration`] gate, which resolves the
/// arena for each declaration.
pub(crate) fn symbol_is_string_literal_module_only(
    arena: &NodeArena,
    symbol: &tsz_binder::Symbol,
) -> bool {
    let mut saw_decl = false;
    for &decl_idx in &symbol.declarations {
        let Some(node) = arena.get(decl_idx) else {
            continue;
        };
        saw_decl = true;
        if !declaration_is_string_literal_module(arena, node) {
            return false;
        }
    }
    saw_decl
}

impl<'a> CheckerState<'a> {
    /// Find a VALUE symbol for a name across all lib binders.
    ///
    /// This handles declaration merging across lib files: `interface Promise<T>` may be
    /// in one lib file (TYPE-only) while `declare var Promise: PromiseConstructor` is
    /// in another (VALUE). When the initial resolution finds only the TYPE symbol,
    /// this method searches all lib binders for the VALUE declaration.
    ///
    /// Returns the `SymbolId` of the VALUE symbol if found.
    pub(crate) fn find_value_symbol_in_libs(&self, name: &str) -> Option<SymbolId> {
        let lib_binders = self.get_lib_binders();
        trace!(
            name = name,
            "find_value_symbol_in_libs: searching for VALUE symbol"
        );
        // Check file_locals first (may have merged value from lib)
        if let Some(val_sym_id) = self.ctx.binder.file_locals.get(name) {
            trace!(
                name = name,
                val_sym_id = ?val_sym_id,
                "find_value_symbol_in_libs: found in file_locals"
            );
            if let Some(val_symbol) = self
                .ctx
                .binder
                .get_symbol_with_libs(val_sym_id, &lib_binders)
            {
                trace!(
                    name = name,
                    val_sym_id = ?val_sym_id,
                    has_value = val_symbol.has_any_flags(symbol_flags::VALUE),
                    is_type_only = val_symbol.is_type_only,
                    flags = val_symbol.flags,
                    "find_value_symbol_in_libs: symbol details"
                );
                let value_flags_except_module = symbol_flags::VALUE & !symbol_flags::VALUE_MODULE;
                if val_symbol.has_any_flags(value_flags_except_module) && !val_symbol.is_type_only {
                    trace!(
                        name = name,
                        returned_sym_id = ?val_sym_id,
                        "find_value_symbol_in_libs: returning from file_locals"
                    );
                    return Some(val_sym_id);
                }
            }
        }
        // Search lib binders directly
        for (lib_idx, lib_binder) in lib_binders.iter().enumerate() {
            if let Some(val_sym_id) = lib_binder.file_locals.get(name) {
                trace!(
                    name = name,
                    lib_idx = lib_idx,
                    val_sym_id = ?val_sym_id,
                    "find_value_symbol_in_libs: found in lib_binder"
                );
                if let Some(val_symbol) = lib_binder.get_symbol(val_sym_id) {
                    trace!(
                        name = name,
                        lib_idx = lib_idx,
                        val_sym_id = ?val_sym_id,
                        has_value = val_symbol.has_any_flags(symbol_flags::VALUE),
                        is_type_only = val_symbol.is_type_only,
                        flags = val_symbol.flags,
                        "find_value_symbol_in_libs: lib symbol details"
                    );
                    if val_symbol.has_any_flags(symbol_flags::VALUE) && !val_symbol.is_type_only {
                        trace!(
                            name = name,
                            lib_idx = lib_idx,
                            returned_sym_id = ?val_sym_id,
                            "find_value_symbol_in_libs: returning from lib_binder"
                        );
                        return Some(val_sym_id);
                    }
                }
            }
        }
        trace!(
            name = name,
            "find_value_symbol_in_libs: no VALUE symbol found"
        );
        None
    }

    /// Find a VALUE declaration node for a name across current + lib binders.
    ///
    /// Returning the declaration node avoids relying on cross-binder `SymbolId`
    /// identity, which can collide and lead to incorrect value/type selection.
    pub(crate) fn find_value_declaration_in_libs(
        &self,
        name: &str,
    ) -> Option<(SymbolId, NodeIndex)> {
        let lib_binders = self.get_lib_binders();

        // Check merged/local symbols first.
        if let Some(val_sym_id) = self.ctx.binder.file_locals.get(name)
            && let Some(val_symbol) = self
                .ctx
                .binder
                .get_symbol_with_libs(val_sym_id, &lib_binders)
            && val_symbol.has_any_flags(symbol_flags::VALUE & !symbol_flags::VALUE_MODULE)
            && !val_symbol.is_type_only
            && val_symbol.value_declaration.is_some()
        {
            return Some((val_sym_id, val_symbol.value_declaration));
        }

        // Then scan lib binders directly.
        for lib_binder in lib_binders.iter() {
            if let Some(val_sym_id) = lib_binder.file_locals.get(name)
                && let Some(val_symbol) = lib_binder.get_symbol(val_sym_id)
                && val_symbol.has_any_flags(symbol_flags::VALUE)
                && !val_symbol.is_type_only
                && val_symbol.value_declaration.is_some()
            {
                return Some((val_sym_id, val_symbol.value_declaration));
            }
        }

        None
    }

    // =========================================================================
    // Global Symbol Resolution
    // =========================================================================

    /// Resolve a global value symbol by name from `file_locals` and lib binders.
    ///
    /// This is used for looking up global values like `console`, `Math`, `globalThis`, etc.
    /// It checks:
    /// 1. Local `file_locals` (for user-defined globals and merged lib symbols)
    /// 2. Lib binders' `file_locals` (only when `lib_symbols_merged` is false)
    pub(crate) fn resolve_global_value_symbol(&self, name: &str) -> Option<SymbolId> {
        // First check local file_locals
        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            return Some(sym_id);
        }

        // Skip lib binder scan if lib symbols are merged - they're all in file_locals already
        if self.ctx.binder.lib_symbols_are_merged() {
            return None;
        }

        // Legacy path: check lib binders for global symbols
        let lib_binders = self.get_lib_binders();
        for lib_binder in lib_binders.iter() {
            if let Some(sym_id) = lib_binder.file_locals.get(name) {
                return Some(sym_id);
            }
        }

        None
    }

    /// Search lib symbols for a function-scoped (var) value symbol with the given name.
    /// Handles block-scoped local shadowing a lib global var (e.g. `const Symbol = globalThis.Symbol`).
    ///
    /// Symbols whose only declaration is a parameter (kind `PARAMETER`) are rejected:
    /// `lib_symbol_ids` can include parameter symbols leaked from lib binding (e.g. the `y`
    /// parameter of `Math.atan2`), which would otherwise spoof a "lib var `y`" and
    /// suppress legitimate TS2339 reports for `globalThis.y` against a user `const y`.
    pub(crate) fn resolve_lib_global_var_symbol(&self, name: &str) -> Option<SymbolId> {
        // A real lib global (`declare var X`, `declare function X`, `declare class X`)
        // is at file scope: its parent symbol is either NONE or a non-callable
        // container (SourceFile / namespace / module). Parameters of callables share
        // the FUNCTION_SCOPED_VARIABLE flag, so the flag check alone admits cases
        // like `moveTo(x, y)`'s `y` — which then suppresses the legitimate TS2339
        // for `globalThis.y` when the user has `const y = 2`. Reject any lib
        // candidate whose parent is a callable.
        const NESTED_CALLABLE_PARENT_MASK: u32 = symbol_flags::FUNCTION
            | symbol_flags::METHOD
            | symbol_flags::CONSTRUCTOR
            | symbol_flags::GET_ACCESSOR
            | symbol_flags::SET_ACCESSOR
            | symbol_flags::SIGNATURE;
        let parent_is_callable = |parent: SymbolId| -> bool {
            if parent.is_none() {
                return false;
            }
            self.ctx
                .binder
                .get_symbol(parent)
                .is_some_and(|p| p.has_any_flags(NESTED_CALLABLE_PARENT_MASK))
        };
        if self.ctx.binder.lib_symbols_are_merged() {
            for &lib_id in self.ctx.binder.lib_symbol_ids.iter() {
                if let Some(lib_sym) = self.ctx.binder.get_symbol(lib_id)
                    && lib_sym.escaped_name == name
                    && lib_sym.has_any_flags(symbol_flags::VALUE)
                    && (!lib_sym.has_any_flags(symbol_flags::BLOCK_SCOPED_VARIABLE)
                        || lib_sym.has_any_flags(symbol_flags::FUNCTION_SCOPED_VARIABLE))
                    && self.symbol_has_globalable_declaration(lib_id, lib_sym, None)
                    && !parent_is_callable(lib_sym.parent)
                {
                    return Some(lib_id);
                }
            }
        } else {
            let lib_binders = self.get_lib_binders();
            for lib_binder in lib_binders.iter() {
                if let Some(sym_id) = lib_binder.file_locals.get(name)
                    && let Some(lib_sym) = lib_binder.get_symbol(sym_id)
                    && lib_sym.has_any_flags(symbol_flags::VALUE)
                    && (!lib_sym.has_any_flags(symbol_flags::BLOCK_SCOPED_VARIABLE)
                        || lib_sym.has_any_flags(symbol_flags::FUNCTION_SCOPED_VARIABLE))
                    && self.symbol_has_globalable_declaration(sym_id, lib_sym, Some(lib_binder))
                {
                    let parent = lib_sym.parent;
                    let parent_is_callable_legacy = parent.is_some()
                        && lib_binder
                            .get_symbol(parent)
                            .is_some_and(|p| p.has_any_flags(NESTED_CALLABLE_PARENT_MASK));
                    if !parent_is_callable_legacy {
                        return Some(sym_id);
                    }
                }
            }
        }
        None
    }

    /// True iff the symbol has at least one declaration that can plausibly be
    /// a global value (i.e. not a `Parameter` node). Used by
    /// `resolve_lib_global_var_symbol` to filter out parameter symbols that
    /// leak into `lib_symbol_ids`.
    pub(crate) fn symbol_has_globalable_declaration(
        &self,
        sym_id: SymbolId,
        sym: &tsz_binder::Symbol,
        lib_binder_override: Option<&tsz_binder::BinderState>,
    ) -> bool {
        let main_arena = self.ctx.arena;
        for &decl_idx in &sym.declarations {
            if decl_idx.is_none() {
                continue;
            }
            let arena = lib_binder_override
                .map(|b| b.arena_for_declaration_or(sym_id, decl_idx, main_arena))
                .unwrap_or_else(|| {
                    self.ctx
                        .binder
                        .arena_for_declaration_or(sym_id, decl_idx, main_arena)
                });
            if let Some(node) = arena.get(decl_idx)
                && node.kind != syntax_kind_ext::PARAMETER
            {
                return true;
            }
        }
        false
    }

    // =========================================================================
    // Heritage Symbol Resolution
    // =========================================================================

    /// Resolve a heritage clause expression to its symbol.
    ///
    /// Heritage clauses appear in `extends` and `implements` clauses of classes and interfaces.
    /// This function handles:
    /// - Simple identifiers (e.g., `class B extends A`)
    /// - Qualified names (e.g., `class B extends Namespace.A`)
    /// - Property access expressions (e.g., `class B extends module.A`)
    pub(crate) fn resolve_heritage_symbol(&self, idx: NodeIndex) -> Option<SymbolId> {
        if let Some(cached) = self.ctx.heritage_symbol_cache.borrow().get(&idx).copied() {
            return cached;
        }

        let node = self.ctx.arena.get(idx)?;

        let resolved = if node.kind == SyntaxKind::Identifier as u16 {
            self.resolve_identifier_symbol(idx)
        } else if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            self.resolve_qualified_symbol(idx)
        } else if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let Some(access) = self.ctx.arena.get_access_expr(node) else {
                self.ctx
                    .heritage_symbol_cache
                    .borrow_mut()
                    .insert(idx, None);
                return None;
            };
            let Some(left_sym_raw) = self.resolve_heritage_symbol(access.expression) else {
                self.ctx
                    .heritage_symbol_cache
                    .borrow_mut()
                    .insert(idx, None);
                return None;
            };
            let Some(name) = self
                .ctx
                .arena
                .get_identifier_at(access.name_or_argument)
                .map(|ident| ident.escaped_text.clone())
            else {
                self.ctx
                    .heritage_symbol_cache
                    .borrow_mut()
                    .insert(idx, None);
                return None;
            };

            // First, check the raw symbol's direct exports (for namespace symbols)
            if let Some(left_symbol) = self.ctx.binder.get_symbol(left_sym_raw) {
                if let Some(exports) = left_symbol.exports.as_ref()
                    && let Some(member_sym) = exports.get(&name)
                {
                    self.ctx
                        .heritage_symbol_cache
                        .borrow_mut()
                        .insert(idx, Some(member_sym));
                    return Some(member_sym);
                }

                // For import aliases (import X = require("./module")), X represents
                // the entire module namespace. Look up the member in module_exports.
                if let Some(module_specifier) = left_symbol.import_module() {
                    if left_symbol.has_any_flags(symbol_flags::ALIAS)
                        && self
                            .ctx
                            .module_resolves_to_non_module_entity(module_specifier)
                    {
                        self.ctx
                            .heritage_symbol_cache
                            .borrow_mut()
                            .insert(idx, None);
                        return None;
                    }
                    let mut visited_aliases = AliasCycleTracker::new();
                    if let Some(member_sym) = self.resolve_reexported_member_symbol(
                        module_specifier,
                        &name,
                        &mut visited_aliases,
                    ) {
                        self.ctx
                            .heritage_symbol_cache
                            .borrow_mut()
                            .insert(idx, Some(member_sym));
                        return Some(member_sym);
                    }
                }
            }

            // Try resolving the alias to get the actual symbol (for non-require aliases
            // like `import X = SomeNamespace`)
            let mut visited_aliases = AliasCycleTracker::new();
            if let Some(resolved_sym) =
                self.resolve_alias_symbol(left_sym_raw, &mut visited_aliases)
                && resolved_sym != left_sym_raw
                && let Some(resolved_symbol) = self.ctx.binder.get_symbol(resolved_sym)
            {
                if let Some(exports) = resolved_symbol.exports.as_ref()
                    && let Some(member_sym) = exports.get(&name)
                {
                    self.ctx
                        .heritage_symbol_cache
                        .borrow_mut()
                        .insert(idx, Some(member_sym));
                    return Some(member_sym);
                }
                // Also check module_exports on the resolved symbol
                if let Some(module_specifier) = resolved_symbol.import_module()
                    && let Some(member_sym) = self.resolve_reexported_member_symbol(
                        module_specifier,
                        &name,
                        &mut visited_aliases,
                    )
                {
                    self.ctx
                        .heritage_symbol_cache
                        .borrow_mut()
                        .insert(idx, Some(member_sym));
                    return Some(member_sym);
                }
            }

            None
        } else {
            None
        };

        self.ctx
            .heritage_symbol_cache
            .borrow_mut()
            .insert(idx, resolved);
        resolved
    }

    /// Follow a heritage symbol that is itself a *named* import/re-export alias to
    /// the original declaration symbol, chasing multi-hop named (type-only or
    /// value) re-export chains (`export type { X } from './x'`).
    ///
    /// The arity / "is generic" checks in the heritage path read type parameters
    /// off the symbol's own declarations. When the heritage clause names a symbol
    /// imported through a barrel of named re-exports, that local alias's
    /// declaration is the import specifier — it carries no type-parameter list —
    /// so the checks would conclude the base is non-generic and falsely emit
    /// TS2315. This mirrors the reference-type-parameter path, which already
    /// resolves the alias chain via `reference_import_alias_export_target` before
    /// reading type parameters.
    ///
    /// The re-point is deliberately narrow: it only applies when the chased target
    /// resolves to a *generic* declaration. Non-generic targets keep the original
    /// alias symbol so that the namespace / `import =` / `export =` value-vs-type
    /// heritage diagnostics (TS2507, TS2708, the genuine-non-generic TS2315) keep
    /// reading the alias surface exactly as before. Returns `Some(declaration_symbol)`
    /// only when the resolved target differs from the alias and is generic.
    pub(crate) fn resolve_heritage_alias_to_declaration_symbol(
        &mut self,
        heritage_sym: SymbolId,
        expr_idx: NodeIndex,
    ) -> Option<SymbolId> {
        let expected_name = self.heritage_name_text(expr_idx)?;
        let leaf_name = expected_name
            .rsplit('.')
            .next()
            .unwrap_or(&expected_name)
            .to_owned();
        let alias_symbol = self.ctx.binder.get_symbol(heritage_sym)?;
        if !self.reference_symbol_is_import_alias(alias_symbol) {
            return None;
        }
        // Only chase named member aliases (`import { X }` / `export { X } from`).
        // Namespace (`import * as`/`export * as`) and `import =`/`export =`
        // aliases name a module/namespace value, not a single declaration, and
        // re-pointing them would suppress namespace-vs-value heritage diagnostics.
        let import_name = alias_symbol.import_name()?;
        if import_name == "*" || import_name == "export=" || import_name == "default" {
            return None;
        }
        let (target_sym_id, target_file_idx) =
            self.reference_import_alias_export_target(alias_symbol, &leaf_name)?;
        if target_sym_id == heritage_sym {
            return None;
        }
        if let Some(file_idx) = target_file_idx {
            self.ctx
                .register_symbol_file_target(target_sym_id, file_idx);
        }
        // Keep the re-point confined to the generic-detection bug: only forward
        // when the chased declaration actually carries type parameters. Reading
        // the type-parameter list off the resolved target (which honors the
        // freshly-registered cross-file target) leaves genuinely non-generic
        // re-exported bases on the original alias surface, preserving the
        // TS2315 / namespace-vs-value heritage diagnostics.
        if self.get_type_params_for_symbol(target_sym_id).is_empty() {
            return None;
        }
        Some(target_sym_id)
    }

    /// Check if an expression is a property access on an unresolved import.
    ///
    /// Used to suppress TS2304 errors when TS2307 was already emitted for the module.
    pub(crate) fn is_property_access_on_unresolved_import(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        // Handle property access expressions (e.g., B.B in extends B.B)
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let Some(access) = self.ctx.arena.get_access_expr(node) else {
                return false;
            };
            // Check if the left side is an unresolved import or a property access on one
            return self.is_unresolved_import_symbol(access.expression)
                || self.is_property_access_on_unresolved_import(access.expression);
        }

        // Handle qualified names (e.g., A.B in type position)
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let Some(qn) = self.ctx.arena.get_qualified_name(node) else {
                return false;
            };
            return self.is_unresolved_import_symbol(qn.left)
                || self.is_property_access_on_unresolved_import(qn.left);
        }

        // Direct identifier - check if it's an unresolved import
        if node.kind == SyntaxKind::Identifier as u16 {
            return self.is_unresolved_import_symbol(idx);
        }

        false
    }

    /// Check if an identifier refers to an unresolved import symbol.
    ///
    /// Returns true if:
    /// - The symbol is an ALIAS (import)
    /// - The imported module cannot be resolved through any of:
    ///   - `module_exports`
    ///   - `shorthand_ambient_modules`
    ///   - `declared_modules`
    ///   - CLI-resolved modules
    pub(crate) fn is_unresolved_import_symbol(&self, idx: NodeIndex) -> bool {
        let Some(sym_id) = self.resolve_identifier_symbol(idx) else {
            return false;
        };

        self.is_unresolved_import_symbol_id(sym_id)
    }

    pub(crate) fn is_unresolved_import_symbol_id(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // Check if this is an ALIAS symbol (import)
        if !symbol.has_any_flags(symbol_flags::ALIAS) {
            return false;
        }

        // Check if it has an import_module - if so, check if that module is resolved
        if let Some(module_name) = symbol.import_module() {
            // Check various ways a module can be resolved
            if self
                .ctx
                .module_exports_contains_module(self.ctx.binder, module_name)
            {
                return false; // Module is resolved (has exports)
            }
            // Check if this is a shorthand ambient module (no body/exports)
            // These should be treated as unresolved imports (any type)
            if self
                .ctx
                .binder
                .shorthand_ambient_modules
                .contains(module_name)
            {
                return true; // Shorthand ambient module - treat as unresolved/any
            }
            if self.is_ambient_module_match(module_name) {
                return false; // Ambient module pattern matches (with body/exports)
            }
            if let Some(ref resolved) = self.ctx.resolved_modules
                && resolved.contains(module_name)
            {
                return false; // CLI resolved module
            }
            // Module is not resolved - this is an unresolved import
            return true;
        }

        // For import equals declarations without import_module set,
        // check if the value_declaration is an import equals with a require
        if symbol.value_declaration.is_some() {
            let Some(decl_node) = self.ctx.arena.get(symbol.value_declaration) else {
                return false;
            };
            if decl_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                && let Some(import) = self.ctx.arena.get_import_decl(decl_node)
            {
                if let Some(ref_node) = self.ctx.arena.get(import.module_specifier)
                    && ref_node.kind == SyntaxKind::StringLiteral as u16
                    && let Some(lit) = self.ctx.arena.get_literal(ref_node)
                {
                    let module_name = &lit.text;
                    if !self
                        .ctx
                        .module_exports_contains_module(self.ctx.binder, module_name)
                        && !self
                            .ctx
                            .binder
                            .shorthand_ambient_modules
                            .contains(module_name)
                        && !self
                            .ctx
                            .declared_modules_contains(self.ctx.binder, module_name)
                        && !self
                            .ctx
                            .resolved_modules
                            .as_ref()
                            .is_some_and(|r| r.contains(module_name))
                        && self.ctx.resolve_import_target(module_name).is_none()
                    {
                        return true;
                    }
                }

                // `import X = E.Member` (entity-name import-equals). The RHS is a
                // qualified/property-access name rather than a `require("…")`
                // string. When the entity name's leftmost root is itself an
                // unresolved module import (its TS2307 was already emitted), the
                // alias inherits the error/any poison: `E.Member` is a member of
                // an `any` namespace, so `X` is `any` and downstream member
                // access / type-argument instantiation must not cascade. This
                // mirrors tsc, which types members reached through an unresolved
                // import as `any`.
                if self.import_equals_entity_name_root_is_unresolved(import.module_specifier) {
                    return true;
                }
            }
        }

        false
    }

    /// For an `import X = <entity-name>` declaration, return `true` when the
    /// entity name (`E`, `E.M`, `E.M.N`, …) is rooted in an unresolved module
    /// import — i.e. the leftmost identifier resolves to an alias whose module
    /// could not be resolved (TS2307 already emitted). String-literal
    /// `require("…")` forms are handled separately by the caller.
    fn import_equals_entity_name_root_is_unresolved(&self, entity_name_idx: NodeIndex) -> bool {
        let mut current = entity_name_idx;
        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return false;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };
            if node.kind == syntax_kind_ext::QUALIFIED_NAME {
                let Some(qn) = self.ctx.arena.get_qualified_name(node) else {
                    return false;
                };
                current = qn.left;
                continue;
            }
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let Some(access) = self.ctx.arena.get_access_expr(node) else {
                    return false;
                };
                current = access.expression;
                continue;
            }
            if node.kind == SyntaxKind::Identifier as u16 {
                let Some(root_sym_id) = self.resolve_identifier_symbol(current) else {
                    return false;
                };
                // The root must be a direct module import (`import * as E from
                // "mod"` / `import E from "mod"` / `import E = require("mod")`)
                // whose module is unresolved. Restricting to aliases that carry
                // a module specifier keeps this a single, non-recursive check:
                // `is_unresolved_import_symbol_id` resolves the module without
                // re-entering the entity-name walk, so chained or cyclic
                // entity-name import-equals aliases cannot loop.
                let Some(root_symbol) = self.ctx.binder.get_symbol(root_sym_id) else {
                    return false;
                };
                if !root_symbol.has_any_flags(symbol_flags::ALIAS)
                    || root_symbol.import_module().is_none()
                {
                    return false;
                }
                return self.is_unresolved_import_symbol_id(root_sym_id);
            }
            return false;
        }
    }

    /// For an indexed-access type annotation `T[K]`, return `true` when the
    /// object operand `T` is a type reference rooted in an unresolved module
    /// import (TS2307 already emitted).
    ///
    /// `tsc` poisons a reference to a generic interface from an unresolved
    /// module to `any`, so the indexed access `T[K]` collapses to `any[K] =
    /// any` rather than a deferred `T[K]`. Without this signal the lowering
    /// keeps the access stuck on `Application(UnresolvedTypeName, …)`, which
    /// (1) false-fails assignability (`TS2322`) against an arrow initializer
    /// and (2) is treated as a valid contextual type that wrongly suppresses
    /// the implicit-`any` parameter diagnostics (`TS7006`) `tsc` reports.
    ///
    /// The root identifier is resolved structurally via
    /// `is_unresolved_import_symbol_id` (the same helper #13755/#13780 use);
    /// no identifier/file-name string is inspected. Qualified object names
    /// (`ns.Iface[K]`) walk to the leftmost identifier before the check.
    pub(crate) fn indexed_access_object_is_unresolved_import(
        &self,
        indexed_access_node: &Node,
    ) -> bool {
        let Some(indexed) = self.ctx.arena.get_indexed_access_type(indexed_access_node) else {
            return false;
        };
        let Some(object_node) = self.ctx.arena.get(indexed.object_type) else {
            return false;
        };
        if object_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = self.ctx.arena.get_type_ref(object_node) else {
            return false;
        };
        self.type_reference_root_is_unresolved_import(type_ref.type_name)
    }

    /// Walk an entity name (`E`, `E.M`, `E.M.N`, …) to its leftmost identifier
    /// and report whether that root resolves to an unresolved module import.
    /// Shared root-walk for the indexed-access poison check above.
    fn type_reference_root_is_unresolved_import(&self, entity_name_idx: NodeIndex) -> bool {
        let mut current = entity_name_idx;
        for _ in 0..MAX_TREE_WALK_ITERATIONS {
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };
            if node.kind == syntax_kind_ext::QUALIFIED_NAME {
                let Some(qn) = self.ctx.arena.get_qualified_name(node) else {
                    return false;
                };
                current = qn.left;
                continue;
            }
            if node.kind == SyntaxKind::Identifier as u16 {
                return matches!(
                    self.resolve_identifier_symbol_in_type_position_without_tracking(current),
                    crate::symbol_resolver::TypeSymbolResolution::Type(sym_id)
                        if self.is_unresolved_import_symbol_id(sym_id)
                );
            }
            return false;
        }
        false
    }

    /// Check if a module specifier matches a declared or shorthand ambient module pattern.
    ///
    /// Supports simple wildcard patterns using `*` (e.g., "foo*baz", "*!text").
    pub(crate) fn is_ambient_module_match(&self, module_name: &str) -> bool {
        // Check current binder first (always available)
        if self.binder_has_ambient_module(self.ctx.binder, module_name) {
            return true;
        }

        // Use the pre-built global index for O(1) exact + small pattern scan
        if let Some(declared) = &self.ctx.global_declared_modules {
            let normalized = module_name.trim().trim_matches('"').trim_matches('\'');
            if declared.exact.contains(normalized) {
                return true;
            }
            return declared.matches_wildcard(module_name);
        }

        // Fallback: scan all binders
        if let Some(binders) = &self.ctx.all_binders {
            for binder in binders.iter() {
                if self.binder_has_ambient_module(binder, module_name) {
                    return true;
                }
            }
        }

        false
    }

    fn binder_has_ambient_module(
        &self,
        binder: &tsz_binder::BinderState,
        module_name: &str,
    ) -> bool {
        if self.matches_module_pattern(&binder.declared_modules, module_name)
            || self.matches_module_pattern(&binder.shorthand_ambient_modules, module_name)
        {
            return true;
        }

        false
    }

    fn matches_module_pattern(
        &self,
        patterns: &rustc_hash::FxHashSet<String>,
        module_name: &str,
    ) -> bool {
        // The shared matcher applies tsc's ambient-module semantics (exactly one
        // `*` is a wildcard with literal prefix/suffix; everything else is an
        // exact name) without compiling a glob regex.
        patterns
            .iter()
            .any(|pattern| crate::context::ambient_pattern_matches(pattern, module_name))
    }

    // =========================================================================
    // Require/Import Resolution
    // =========================================================================

    /// Extract the module specifier from a `require()` call expression or
    /// a string literal (for import equals declarations where the parser
    /// stores only the string literal, not the full `require()` call).
    ///
    /// Returns the module path string (e.g., `'./util'` from `require('./util')`).
    pub(crate) fn get_require_module_specifier(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;

        // For import equals declarations, the parser stores just the string literal
        // e.g., `import x = require('./util')` has module_specifier = StringLiteral('./util')
        if node.kind == SyntaxKind::StringLiteral as u16 {
            let literal = self.ctx.arena.get_literal(node)?;
            // Strip surrounding quotes if present (parser stores raw text with quotes)
            let text = literal.text.trim_matches(|c| c == '"' || c == '\'');
            return Some(text.to_string());
        }

        // Handle full require() call expression (for other contexts)
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.ctx.arena.get_call_expr(node)?;
        let callee_ident = self.ctx.arena.get_identifier_at(call.expression)?;
        if callee_ident.escaped_text != "require" {
            return None;
        }

        let args = call.arguments.as_ref()?;
        let first_arg = args.nodes.first().copied()?;
        let literal = self.ctx.arena.get_literal_at(first_arg)?;
        Some(literal.text.clone())
    }

    /// Resolve a `require()` call to its symbol.
    ///
    /// For `require()` calls, we don't resolve to a single symbol.
    /// Instead, `compute_type_of_symbol` handles this by creating a module namespace type.
    pub(crate) fn resolve_require_call_symbol(
        &self,
        idx: NodeIndex,
        _visited_aliases: Option<&mut AliasCycleTracker>,
    ) -> Option<SymbolId> {
        // For require() calls, we don't resolve to a single symbol.
        // Instead, compute_type_of_symbol handles this by creating a module namespace type.
        // This function now just returns None to indicate no single symbol resolution.
        let _ = self.get_require_module_specifier(idx)?;
        // Module resolution for require() is handled in compute_type_of_symbol
        // by creating an object type from module_exports.
        None
    }

    // =========================================================================
    // Type Query Resolution
    // =========================================================================

    /// Find the missing left-most identifier in a type query expression.
    ///
    /// For `typeof A.B.C`, if `A` is unresolved, this returns the node for `A`.
    pub(crate) fn missing_type_query_left(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return None;
            }
            let node = self.ctx.arena.get(current)?;
            if node.kind == SyntaxKind::Identifier as u16 {
                if self.resolve_identifier_symbol(current).is_none() {
                    // globalThis is a synthetic global in tsc with no binder symbol.
                    // Don't report it as missing in typeof qualified expressions
                    // (e.g., `typeof globalThis.isNaN`).
                    if let Some(ident) = self.ctx.arena.get_identifier(node)
                        && ident.escaped_text == "globalThis"
                    {
                        return None;
                    }
                    return Some(current);
                }
                return None;
            }
            if node.kind != syntax_kind_ext::QUALIFIED_NAME {
                return None;
            }
            let qn = self.ctx.arena.get_qualified_name(node)?;
            current = qn.left;
        }
    }

    /// Check if a qualified name's left part resolves to a pure interface that
    /// shadows an outer namespace which has the right member.
    /// Used to suppress false TS2694 when import-equals resolution finds a local
    /// interface instead of the outer namespace.
    pub(crate) fn check_import_qualified_shadows_namespace(&self, idx: NodeIndex) -> bool {
        let node = match self.ctx.arena.get(idx) {
            Some(node) => node,
            None => return false,
        };
        if node.kind != syntax_kind_ext::QUALIFIED_NAME {
            return false;
        }
        let qn = match self.ctx.arena.get_qualified_name(node) {
            Some(qn) => qn,
            None => return false,
        };
        let left_sym = match self.resolve_qualified_symbol(qn.left) {
            Some(sym) => sym,
            None => return false,
        };
        let lib_binders = self.get_lib_binders();
        let left_symbol = match self.ctx.binder.get_symbol_with_libs(left_sym, &lib_binders) {
            Some(symbol) => symbol,
            None => return false,
        };
        // Only suppress when the left is a pure interface (no namespace meaning)
        let is_pure_interface = left_symbol.has_any_flags(symbol_flags::INTERFACE)
            && !left_symbol.has_any_flags(symbol_flags::MODULE)
            && !left_symbol.has_any_flags(CLASS)
            && !left_symbol.has_any_flags(symbol_flags::REGULAR_ENUM)
            && !left_symbol.has_any_flags(symbol_flags::CONST_ENUM);
        if !is_pure_interface {
            return false;
        }
        let right_name = match self
            .ctx
            .arena
            .get(qn.right)
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .map(|ident| ident.escaped_text.as_str())
        {
            Some(name) => name,
            None => return false,
        };
        let left_name = left_symbol.escaped_name.clone();
        // Check if an outer namespace has the member
        let Some(scope_id) = self
            .ctx
            .binder
            .find_enclosing_scope(self.ctx.arena, qn.left)
        else {
            return false;
        };
        let Some(current_scope) = self.ctx.binder.scopes.get(scope_id.0 as usize) else {
            return false;
        };
        let mut walk_id = current_scope.parent;
        while let Some(scope) = self.ctx.binder.scopes.get(walk_id.0 as usize) {
            if let Some(sym_id) = scope.table.get(&left_name)
                && let Some(sym) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
                && sym.has_any_flags(symbol_flags::NAMESPACE)
                && let Some(exports) = sym.exports.as_ref()
                && exports.has(right_name)
            {
                return true;
            }
            if walk_id == scope.parent {
                break;
            }
            walk_id = scope.parent;
        }
        false
    }

    /// Report a qualified namespace/member lookup error.
    ///
    /// For `typeof A.B` where `B` is not found in a local value namespace/object,
    /// emits TS2339. Import-query paths keep the existing exported-member
    /// diagnostic, since those are module export lookups. Other type-space
    /// qualified names keep TS2694/TS2724.
    /// Returns true if an error was reported.
    pub(crate) fn report_type_query_missing_member(&mut self, idx: NodeIndex) -> bool {
        self.report_qualified_member_missing(idx, true)
    }

    /// Report a qualified-name missing member error for non-`typeof` callers
    /// (import-equals, type-alias resolution). Always emits TS2694/TS2724 on
    /// failure; never the value-space TS2339.
    pub(crate) fn report_qualified_alias_missing_member(&mut self, idx: NodeIndex) -> bool {
        self.report_qualified_member_missing(idx, false)
    }

    fn report_qualified_member_missing(&mut self, idx: NodeIndex, from_typeof: bool) -> bool {
        let node = match self.ctx.arena.get(idx) {
            Some(node) => node,
            None => return false,
        };
        if node.kind != syntax_kind_ext::QUALIFIED_NAME {
            return false;
        }
        let qn = match self.ctx.arena.get_qualified_name(node) {
            Some(qn) => qn,
            None => return false,
        };

        let left_sym = match self.resolve_qualified_symbol(qn.left) {
            Some(sym) => sym,
            None => {
                // The left side couldn't be fully resolved. For nested qualified names
                // like X.Y.Z where Y doesn't exist in X, the resolution of X.Y fails.
                // Recursively check if we can report TS2694 on the left sub-expression.
                return self.report_qualified_member_missing(qn.left, from_typeof);
            }
        };
        let lib_binders = self.get_lib_binders();
        let left_symbol = match self.ctx.binder.get_symbol_with_libs(left_sym, &lib_binders) {
            Some(symbol) => symbol,
            None => return false,
        };

        // Only report TS2694 for namespace/module/enum/class symbols.
        // For regular variables (e.g., `typeof x.p` where x is a local variable),
        // the qualified name refers to a property access, not a namespace member.
        let is_namespace_like = left_symbol.flags
            & (symbol_flags::MODULE
                | CLASS
                | symbol_flags::REGULAR_ENUM
                | symbol_flags::CONST_ENUM
                | symbol_flags::INTERFACE)
            != 0;
        if !is_namespace_like {
            return false;
        }

        let right_name = match self
            .ctx
            .arena
            .get(qn.right)
            .and_then(|node| self.ctx.arena.get_identifier(node))
            .map(|ident| ident.escaped_text.as_str())
        {
            Some(name) => name,
            None => return false,
        };

        // Check direct exports first
        if let Some(exports) = left_symbol.exports.as_ref()
            && exports.has(right_name)
        {
            return false;
        }

        // For classes, check if the member exists in the class's members (static members)
        // This handles `typeof C.staticMember` where C is a class
        if left_symbol.has_any_flags(CLASS)
            && let Some(members) = left_symbol.members.as_ref()
            && members.has(right_name)
        {
            return false;
        }

        // Check for re-exports from other modules
        // This handles cases like: export { foo } from './bar'
        if let Some(module_specifier) = left_symbol.import_module() {
            if left_symbol.has_any_flags(symbol_flags::ALIAS)
                && self
                    .ctx
                    .module_resolves_to_non_module_entity(module_specifier)
            {
                let export_names: Vec<String> = left_symbol
                    .exports
                    .as_ref()
                    .map(|e| e.iter().map(|(name, _)| name.clone()).collect())
                    .unwrap_or_default();
                let req = crate::query_boundaries::name_resolution::NameResolutionRequest::exported_member(
                    right_name,
                    qn.right,
                    left_sym,
                    export_names,
                );
                let failure = match self.resolve_name_structured(&req) {
                    Err(f) => f,
                    Ok(_) => return true,
                };
                self.report_name_resolution_failure(&req, &failure);
                return true;
            }
            let mut visited_aliases = AliasCycleTracker::new();
            if self
                .resolve_reexported_member_symbol(
                    module_specifier,
                    right_name,
                    &mut visited_aliases,
                )
                .is_some()
            {
                return false;
            }
        }

        if from_typeof && left_symbol.import_module().is_none() {
            // `tsc` displays the resolved value symbol here, not the source
            // spelling of its qualification. Thus `typeof Outer.Inner.Missing`
            // and an import-equals alias to `Outer.Inner` both name `Inner`.
            let left_text = left_symbol.escaped_name.clone();
            let Some(right_node) = self.ctx.arena.get(qn.right) else {
                return false;
            };
            self.ctx.error(
                right_node.pos,
                right_node.end - right_node.pos,
                format!("Property '{right_name}' does not exist on type 'typeof {left_text}'."),
                crate::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
            );
            return true;
        }

        let export_names: Vec<String> = left_symbol
            .exports
            .as_ref()
            .map(|e| e.iter().map(|(name, _)| name.clone()).collect())
            .unwrap_or_default();
        let req = crate::query_boundaries::name_resolution::NameResolutionRequest::exported_member(
            right_name,
            qn.right,
            left_sym,
            export_names,
        );
        let failure = match self.resolve_name_structured(&req) {
            Err(f) => f,
            Ok(_) => return true,
        };
        self.report_name_resolution_failure(&req, &failure);
        true
    }

    fn is_inside_type_query(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        for _ in 0..MAX_TREE_WALK_ITERATIONS {
            let Some(parent) = self.ctx.arena.get_extended(current).map(|ext| ext.parent) else {
                return false;
            };
            if parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };
            if parent_node.kind == syntax_kind_ext::TYPE_QUERY {
                return true;
            }
            current = parent;
        }
        false
    }

    // =========================================================================
    // Test Option Resolution
    // =========================================================================

    /// Parse a boolean option from test file comments.
    ///
    /// Looks for `// @key: true` / `// @key: false` directives (canonical
    /// grammar from `tsz_common::test_directives`) in the leading comment
    /// block, capped at the first 32 lines. `key` is the pragma name with
    /// its leading `@` (e.g. `"@strict"`); the directive key must match it
    /// exactly (case-insensitive) — `@strict` does not match
    /// `@strictNullChecks` and vice versa, mirroring the conformance and
    /// emit harness recognizers.
    pub(crate) fn parse_test_option_bool(text: &str, key: &str) -> Option<bool> {
        Self::bool_pragma(&Self::leading_directives(text), key)
    }

    /// Key/value directives in the leading comment block, in source order.
    /// Collected once per file so the ~20 option lookups in
    /// `resolve_compiler_options_from_source` do not each re-scan the text.
    fn leading_directives(text: &str) -> Vec<tsz_common::test_directives::DirectiveLine<'_>> {
        Self::leading_comment_lines(text)
            .filter_map(tsz_common::test_directives::parse_directive_line)
            .collect()
    }

    fn bool_pragma(
        directives: &[tsz_common::test_directives::DirectiveLine<'_>],
        key: &str,
    ) -> Option<bool> {
        let name = key.strip_prefix('@').unwrap_or(key);
        directives.iter().find_map(|directive| {
            // Multi-variant values like "true, false" take the first
            // variant; non-boolean values keep scanning.
            directive
                .key_is(name)
                .then(|| tsz_common::test_directives::parse_bool_value(directive.value))
                .flatten()
        })
    }

    /// The value of a key/value directive, with any surrounding quotes
    /// stripped so `@opt: 6.0` and `@opt: "6.0"` read alike.
    ///
    /// Directives whose value is a version string rather than a boolean
    /// (`@ignoreDeprecations`) need the text, which `bool_pragma` discards.
    fn value_pragma<'d>(
        directives: &[tsz_common::test_directives::DirectiveLine<'d>],
        key: &str,
    ) -> Option<&'d str> {
        let name = key.strip_prefix('@').unwrap_or(key);
        directives.iter().find_map(|directive| {
            directive
                .key_is(name)
                .then(|| directive.value.trim_matches(['"', '\'']))
        })
    }

    /// The leading comment block of a source file: non-empty lines from the
    /// top of the file up to the first non-comment line, capped at 32 lines.
    /// Test pragmas are only honored in this region so a directive-shaped
    /// comment deep inside real code cannot mutate compiler options.
    fn leading_comment_lines(text: &str) -> impl Iterator<Item = &str> {
        text.lines()
            .take(32)
            .filter(|line| !line.trim().is_empty())
            .take_while(|line| {
                let trimmed = line.trim();
                trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*')
            })
    }

    /// Resolve a boolean compiler option from the file's leading directives.
    /// Checks for the option-specific pragma first, then optionally checks `@strict`,
    /// and falls back to the provided default.
    fn resolve_bool_option(
        directives: &[tsz_common::test_directives::DirectiveLine<'_>],
        pragma: &str,
        strict_fallback: bool,
        default: bool,
    ) -> bool {
        if let Some(value) = Self::bool_pragma(directives, pragma) {
            return value;
        }
        if strict_fallback && let Some(strict) = Self::bool_pragma(directives, "@strict") {
            return strict;
        }
        default
    }

    /// Resolve all compiler options from source file comment pragmas.
    /// Called once per file to override compiler options with test pragmas.
    pub(crate) fn resolve_compiler_options_from_source(&mut self, text: &str) {
        // One scan of the leading comment block serves every option lookup.
        let pragmas = Self::leading_directives(text);
        // Snapshot current defaults before mutation to avoid aliased borrows.
        let defaults = self.ctx.compiler_options.clone();
        let opts = &mut self.ctx.compiler_options;
        opts.strict = Self::resolve_bool_option(&pragmas, "@strict", false, defaults.strict);
        // Options that fall back to @strict
        opts.no_implicit_any =
            Self::resolve_bool_option(&pragmas, "@noimplicitany", true, defaults.no_implicit_any);
        opts.use_unknown_in_catch_variables = Self::resolve_bool_option(
            &pragmas,
            "@useunknownincatchvariables",
            true,
            defaults.use_unknown_in_catch_variables,
        );
        opts.no_implicit_this =
            Self::resolve_bool_option(&pragmas, "@noimplicitthis", true, defaults.no_implicit_this);
        opts.strict_property_initialization = Self::resolve_bool_option(
            &pragmas,
            "@strictpropertyinitialization",
            true,
            defaults.strict_property_initialization,
        );
        opts.strict_null_checks = Self::resolve_bool_option(
            &pragmas,
            "@strictnullchecks",
            true,
            defaults.strict_null_checks,
        );
        opts.strict_function_types = Self::resolve_bool_option(
            &pragmas,
            "@strictfunctiontypes",
            true,
            defaults.strict_function_types,
        );
        // Options without @strict fallback
        opts.no_implicit_returns = Self::resolve_bool_option(
            &pragmas,
            "@noimplicitreturns",
            false,
            defaults.no_implicit_returns,
        );
        opts.no_implicit_override = Self::resolve_bool_option(
            &pragmas,
            "@noimplicitoverride",
            false,
            defaults.no_implicit_override,
        );
        opts.no_property_access_from_index_signature = Self::resolve_bool_option(
            &pragmas,
            "@nopropertyaccessfromindexsignature",
            false,
            defaults.no_property_access_from_index_signature,
        );
        opts.no_unused_locals = Self::resolve_bool_option(
            &pragmas,
            "@nounusedlocals",
            false,
            defaults.no_unused_locals,
        );
        opts.no_unused_parameters = Self::resolve_bool_option(
            &pragmas,
            "@nounusedparameters",
            false,
            defaults.no_unused_parameters,
        );
        opts.always_strict =
            Self::resolve_bool_option(&pragmas, "@alwaysstrict", true, defaults.always_strict);
        opts.no_implicit_use_strict = Self::resolve_bool_option(
            &pragmas,
            "@noimplicitusestrict",
            false,
            defaults.no_implicit_use_strict,
        );
        // Option<bool> variant
        opts.allow_unreachable_code = Self::bool_pragma(&pragmas, "@allowunreachablecode")
            .map(Some)
            .unwrap_or(defaults.allow_unreachable_code);
        opts.allow_unused_labels = Self::bool_pragma(&pragmas, "@allowunusedlabels")
            .map(Some)
            .unwrap_or(defaults.allow_unused_labels);

        // @ignoreDeprecations: "5.0"/"6.0"/"7.0" carries a version string, and
        // the flag form also counts as present, so presence is a scan over
        // the leading comment block rather than a `pragmas` lookup (the
        // pre-parsed slice only holds key/value directives).
        if Self::source_has_pragma(text, "@ignoredeprecations") {
            opts.ignore_deprecations = true;
            // `ignore_deprecations_6_0` no longer gates TS2880 (#16217); kept
            // for any deprecation still gated on the "6.0" value specifically.
            opts.ignore_deprecations_6_0 =
                Self::value_pragma(&pragmas, "@ignoredeprecations") == Some("6.0");
        }
    }

    /// Case-insensitive presence check for a `@pragma` directive in the
    /// leading comment block of a source file. Mirrors
    /// `parse_test_option_bool`'s scope (first 32 lines, comment lines
    /// only). Recognizes both the key/value form (`// @pragma: value`) and
    /// the flag form (`// @pragma`), via the canonical directive grammar.
    fn source_has_pragma(text: &str, lower_pragma: &str) -> bool {
        let name = lower_pragma.strip_prefix('@').unwrap_or(lower_pragma);
        Self::leading_comment_lines(text).any(|line| {
            // The two forms are mutually exclusive: a key/value match means
            // the flag parse cannot succeed, so skip it.
            if let Some(directive) = tsz_common::test_directives::parse_directive_line(line) {
                return directive.key_is(name);
            }
            tsz_common::test_directives::parse_flag_directive_line(line)
                .is_some_and(|flag| flag.eq_ignore_ascii_case(name))
        })
    }

    // =========================================================================
    // Duplicate Declaration Resolution
    // =========================================================================

    /// Resolve the declaration node for duplicate identifier checking.
    ///
    /// For some nodes (like short-hand properties), we need to walk up to find
    /// the actual declaration node to report the error on.
    pub(crate) fn resolve_duplicate_decl_node(
        &self,
        arena: &tsz_parser::parser::NodeArena,
        decl_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = decl_idx;
        for _ in 0..8 {
            let node = arena.get(current)?;
            match node.kind {
                syntax_kind_ext::VARIABLE_DECLARATION
                | syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::INTERFACE_DECLARATION
                | syntax_kind_ext::TYPE_ALIAS_DECLARATION
                | syntax_kind_ext::ENUM_DECLARATION
                | syntax_kind_ext::MODULE_DECLARATION
                | syntax_kind_ext::GET_ACCESSOR
                | syntax_kind_ext::SET_ACCESSOR
                | syntax_kind_ext::IMPORT_DECLARATION
                | syntax_kind_ext::IMPORT_CLAUSE
                | syntax_kind_ext::NAMESPACE_IMPORT
                | syntax_kind_ext::IMPORT_SPECIFIER
                | syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                | syntax_kind_ext::EXPORT_DECLARATION
                | syntax_kind_ext::EXPORT_SPECIFIER
                | syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
                | syntax_kind_ext::CONSTRUCTOR
                | syntax_kind_ext::TYPE_PARAMETER
                | syntax_kind_ext::PARAMETER => {
                    return Some(current);
                }
                _ => {
                    let ext = arena.get_extended(current)?;
                    current = ext.parent;
                }
            }
        }
        None
    }

    // =========================================================================
    // Class Access Resolution
    // =========================================================================

    /// Resolve the class for a member access expression.
    ///
    /// Returns the class declaration node and whether the access is on the constructor type.
    /// Used for checking private/protected member accessibility.
    pub(crate) fn resolve_class_for_access(
        &mut self,
        expr_idx: NodeIndex,
        object_type: TypeId,
    ) -> Option<(NodeIndex, bool)> {
        let is_static_member_context = self.find_enclosing_static_block(expr_idx).is_some()
            || self.is_in_static_class_member_context(expr_idx);

        if self.is_this_expression(expr_idx)
            && let Some(ref class_info) = self.ctx.enclosing_class
        {
            return Some((
                class_info.class_idx,
                is_static_member_context || self.is_constructor_type(object_type),
            ));
        }

        if self.is_super_expression(expr_idx)
            && let Some(ref class_info) = self.ctx.enclosing_class
            && let Some(base_idx) = self.get_base_class_idx(class_info.class_idx)
        {
            return Some((
                base_idx,
                is_static_member_context || self.is_constructor_type(object_type),
            ));
        }

        if self
            .ctx
            .arena
            .get(expr_idx)
            .is_some_and(|node| node.kind == SyntaxKind::Identifier as u16)
            && let Some(sym_id) = self.ctx.binder.resolve_identifier(self.ctx.arena, expr_idx)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.has_any_flags(symbol_flags::CLASS)
            && let Some(class_idx) = self.get_class_declaration_from_symbol(sym_id)
        {
            return Some((class_idx, true));
        }

        if object_type != TypeId::ANY && object_type != TypeId::ERROR {
            if let Some(class_idx) = self.get_class_decl_from_type(object_type) {
                return Some((class_idx, false));
            }
            if let Some(sym_id) = self.property_access_receiver_symbol(object_type)
                && let Some(class_idx) = self.get_class_declaration_from_symbol(sym_id)
            {
                return Some((class_idx, false));
            }
        }

        // `this: T extends Foo` in a free function: object_type is the type
        // parameter `T`. Walk through its constraint to find the underlying
        // class declaration so private/protected accessibility is enforced.
        // Without this, `function ext<T extends Foo>(this: T) { this.priv }`
        // silently accepts the private access. Try the constraint via
        // `get_class_decl_from_type` first (covers brand-bearing instance
        // types), then fall back to the constraint's lazy-symbol class
        // declaration (covers merged interface+class where the constraint
        // resolves to the interface lazy without brand props).
        if object_type != TypeId::ANY
            && object_type != TypeId::ERROR
            && let Some(constraint) = crate::query_boundaries::common::type_parameter_constraint(
                self.ctx.types,
                object_type,
            )
            && constraint != object_type
        {
            if let Some(class_idx) = self.get_class_decl_from_type(constraint) {
                return Some((class_idx, false));
            }
            if let Some(sym_id) = self.property_access_receiver_symbol(constraint)
                && let Some(class_idx) = self.get_class_declaration_from_symbol(sym_id)
            {
                return Some((class_idx, false));
            }
        }

        None
    }

    /// Resolve the class established by the nearest enclosing non-arrow
    /// function's explicit `this: T` parameter, independent of any specific
    /// receiver expression.
    ///
    /// A free function's declared `this` parameter type is the accessibility
    /// context for *every* receiver checked inside its body, not only a bare
    /// `this.x` access: `function f<T extends B>(this: T, arg: B) { arg.b() }`
    /// must see `B` as the current class the same way a method body would.
    /// Mirrors `resolve_class_for_access`'s constraint walk so a generic
    /// `this: T extends Foo` resolves through `T`'s constraint to `Foo`.
    pub(crate) fn resolve_this_parameter_class_context(
        &mut self,
        idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let enclosing_fn = self.find_enclosing_non_arrow_function(idx)?;
        let fn_node = self.ctx.arena.get(enclosing_fn)?;
        let params: &[NodeIndex] = if let Some(f) = self.ctx.arena.get_function(fn_node) {
            &f.parameters.nodes
        } else if let Some(m) = self.ctx.arena.get_method_decl(fn_node) {
            &m.parameters.nodes
        } else if let Some(c) = self.ctx.arena.get_constructor(fn_node) {
            &c.parameters.nodes
        } else if let Some(a) = self.ctx.arena.get_accessor(fn_node) {
            &a.parameters.nodes
        } else {
            return None;
        };
        let annotation = self.get_explicit_this_type_annotation(params)?;
        let this_type = self.get_type_from_type_node(annotation);
        if let Some(class_idx) = self.get_class_decl_from_type(this_type) {
            return Some(class_idx);
        }
        let constraint =
            crate::query_boundaries::common::type_parameter_constraint(self.ctx.types, this_type)?;
        if constraint == this_type {
            return None;
        }
        self.get_class_decl_from_type(constraint)
    }

    /// Resolve the receiver class for a member access expression.
    ///
    /// Similar to `resolve_class_for_access`, but returns only the class node.
    /// Used for determining what class the receiver belongs to.
    pub(crate) fn resolve_receiver_class_for_access(
        &mut self,
        expr_idx: NodeIndex,
        object_type: TypeId,
    ) -> Option<NodeIndex> {
        if self.is_this_expression(expr_idx) || self.is_super_expression(expr_idx) {
            if let Some(class_idx) = self.ctx.enclosing_class.as_ref().map(|info| info.class_idx) {
                return Some(class_idx);
            }
            // No literal enclosing class: fall back to the nearest enclosing
            // free function's own `this` parameter type, which is what the
            // receiver's runtime type is bound by in that case.
            return self.resolve_this_parameter_class_context(expr_idx);
        }

        if self
            .ctx
            .arena
            .get(expr_idx)
            .is_some_and(|node| node.kind == SyntaxKind::Identifier as u16)
            && let Some(sym_id) = self.resolve_identifier_symbol(expr_idx)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.has_any_flags(symbol_flags::CLASS)
        {
            return self.get_class_declaration_from_symbol(sym_id);
        }

        if object_type != TypeId::ANY
            && object_type != TypeId::ERROR
            && let Some(class_idx) = self.get_class_decl_from_type(object_type)
        {
            return Some(class_idx);
        }

        None
    }

    /// Resolves a string identifier relative to the scope of a given node.
    pub(crate) fn resolve_name_at_node(&self, name: &str, node_idx: NodeIndex) -> Option<SymbolId> {
        let ignore_libs = !self.ctx.has_lib_loaded();
        let empty_binders: std::sync::Arc<Vec<std::sync::Arc<tsz_binder::BinderState>>> =
            std::sync::Arc::new(Vec::new());
        let lib_binders = if ignore_libs {
            empty_binders
        } else {
            self.get_lib_binders()
        };
        let is_from_lib = |sym_id: SymbolId| self.ctx.symbol_is_from_lib(sym_id);
        let should_skip_lib_symbol = |sym_id: SymbolId| ignore_libs && is_from_lib(sym_id);

        let result = self.ctx.binder.resolve_name_with_filter(
            name,
            self.ctx.arena,
            node_idx,
            &lib_binders,
            |sym_id| {
                if should_skip_lib_symbol(sym_id) {
                    return false;
                }
                if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) {
                    let is_class_member = Self::is_class_member_symbol(symbol.flags);
                    if is_class_member {
                        return is_from_lib(sym_id)
                            && symbol.has_any_flags(tsz_binder::symbol_flags::EXPORT_VALUE);
                    }
                }
                true
            },
        );

        if result.is_none() && !ignore_libs {
            for lib_ctx in self.ctx.lib_contexts.iter() {
                if let Some(lib_sym_id) = lib_ctx.binder.file_locals.get(name)
                    && !should_skip_lib_symbol(lib_sym_id)
                {
                    let Some(file_sym_id) = self.ctx.binder.file_locals.get(name) else {
                        continue;
                    };
                    return Some(file_sym_id);
                }
            }
        }

        result
    }

    /// Resolve a `DefId` from a node index for type lowering.
    ///
    /// This is the canonical stable-identity helper for `def_id_resolver` closures.
    /// It encapsulates the common pattern:
    ///   `resolve_type_symbol_for_lowering(node_idx) -> SymbolId -> get_or_create_def_id`
    ///
    /// Use this instead of inlining the SymbolId wrapping + DefId creation at each
    /// lowering call site.
    pub(crate) fn resolve_def_id_for_lowering(
        &self,
        node_idx: NodeIndex,
    ) -> Option<tsz_solver::def::DefId> {
        self.resolve_type_symbol_for_lowering(node_idx)
            .map(|sym_id| {
                let sym_id = tsz_binder::SymbolId(sym_id);
                if let Some(node) = self.ctx.arena.get(node_idx)
                    && let Some(ident) = self.ctx.arena.get_identifier(node)
                {
                    let authoritative_symbol_exists = self
                        .ctx
                        .resolve_symbol_file_index(sym_id)
                        .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
                        .and_then(|binder| binder.get_symbol(sym_id))
                        .is_some_and(|symbol| symbol.escaped_name == ident.escaped_text);
                    // A same-arena NodeIndex may resolve to a namespace-local type
                    // whose bare name collides with a lib global (`Promise`, etc.).
                    // Only canonicalize to the lib DefId when the resolved symbol
                    // itself is from a lib context.
                    if !self
                        .ctx
                        .file_local_type_shadow_for_lib_name(&ident.escaped_text)
                        && !authoritative_symbol_exists
                        && (self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
                            || self.ctx.symbol_is_from_lib(sym_id))
                        && let Some(def_id) =
                            self.resolve_actual_lib_name_to_def_id_for_lowering(&ident.escaped_text)
                    {
                        return def_id;
                    }
                    let expected_name = if let Some(symbol) = self.get_cross_file_symbol(sym_id) {
                        symbol.escaped_name.clone()
                    } else {
                        let lib_binders = self.get_lib_binders();
                        self.ctx
                            .binder
                            .get_symbol_with_libs(sym_id, &lib_binders)
                            .map_or_else(
                                || ident.escaped_text.to_string(),
                                |symbol| symbol.escaped_name.clone(),
                            )
                    };
                    return self
                        .ctx
                        .get_or_create_def_id_for_symbol_name(sym_id, expected_name.as_str());
                }
                self.ctx.get_or_create_def_id(sym_id)
            })
    }
}
