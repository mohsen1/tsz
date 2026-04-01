//! Type checking query helpers: library type resolution, namespace/alias
//! utilities, constructor accessibility, and symbol exclusion logic.
//!
//! Type-only symbol detection has been extracted to
//! `queries/type_only.rs`.

use super::lib_resolution::{
    lib_def_id_from_node_in_lib_contexts, no_value_resolver, resolve_lib_context_fallback_arena,
    resolve_lib_node_in_lib_contexts,
};
use crate::state::{CheckerState, MemberAccessLevel};
use tsz_binder::{SymbolId, symbol_flags};
use tsz_lowering::TypeLowering;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeParamInfo;
use tsz_solver::{TypeId, TypePredicateTarget};

impl<'a> CheckerState<'a> {
    /// Resolve a lib type by name and also return its type parameters.
    /// Used by `register_boxed_types` for generic types like Array<T> to extract
    /// the actual type parameters from the interface definition rather than
    /// synthesizing fresh ones.
    pub(crate) fn resolve_lib_type_with_params(
        &mut self,
        name: &str,
    ) -> (Option<TypeId>, Vec<TypeParamInfo>) {
        use crate::query_boundaries::common::TypeSubstitution;
        use tsz_solver::TypeInstantiator;

        let factory = self.ctx.types.factory();
        let lib_contexts = &*self.ctx.lib_contexts;

        let mut lib_types: Vec<TypeId> = Vec::new();
        let mut first_params: Option<Vec<TypeParamInfo>> = None;
        // Track canonical TypeIds for the first definition's type parameters.
        // Subsequent definitions will have their type params substituted with these.
        let mut canonical_param_type_ids: Vec<TypeId> = Vec::new();

        for lib_ctx in lib_contexts {
            if let Some(sym_id) = lib_ctx.binder.file_locals.get(name)
                && let Some(symbol) = lib_ctx.binder.get_symbol(sym_id)
            {
                // Multi-arena setup: Get the fallback arena
                let fallback_arena: &NodeArena = resolve_lib_context_fallback_arena(
                    &lib_ctx.binder,
                    sym_id,
                    lib_ctx.arena.as_ref(),
                );

                // Build declaration -> arena pairs using the shared helper.
                // No user_arena context here (per-lib-context iteration).
                let decls_with_arenas = super::lib_resolution::collect_lib_decls_with_arenas(
                    &lib_ctx.binder,
                    sym_id,
                    &symbol.declarations,
                    fallback_arena,
                    None,
                );

                // Resolver triplet: delegates to stable helpers. The `resolver`
                // closure extracts the raw `u32` at the TypeLowering boundary;
                // all internal resolution uses type-safe `SymbolId`.
                let resolver = |node_idx: NodeIndex| -> Option<u32> {
                    resolve_lib_node_in_lib_contexts(
                        node_idx,
                        &decls_with_arenas,
                        fallback_arena,
                        lib_contexts,
                    )
                    .map(|sym_id| sym_id.0)
                };
                let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::DefId> {
                    lib_def_id_from_node_in_lib_contexts(
                        &self.ctx,
                        node_idx,
                        &decls_with_arenas,
                        fallback_arena,
                        lib_contexts,
                    )
                };
                let name_resolver = |type_name: &str| -> Option<tsz_solver::DefId> {
                    self.resolve_entity_name_text_to_def_id_for_lowering(type_name)
                };

                let lazy_type_params_resolver =
                    |def_id: tsz_solver::def::DefId| self.ctx.get_def_type_params(def_id);

                let lowering = TypeLowering::with_hybrid_resolver(
                    fallback_arena,
                    self.ctx.types,
                    &resolver,
                    &def_id_resolver,
                    &no_value_resolver,
                )
                .with_lazy_type_params_resolver(&lazy_type_params_resolver)
                .with_name_def_id_resolver(&name_resolver);

                if !symbol.declarations.is_empty() {
                    // Use lower_merged_interface_declarations for proper multi-arena support
                    let (ty, params) =
                        lowering.lower_merged_interface_declarations(&decls_with_arenas);

                    // If interface lowering succeeded (not ERROR), use the result
                    if ty != TypeId::ERROR {
                        // For the first definition, record canonical type parameter TypeIds
                        if first_params.is_none() && !params.is_empty() {
                            first_params = Some(params.clone());
                            // Compute TypeIds for these canonical params (reuse outer factory)
                            canonical_param_type_ids =
                                params.iter().map(|p| factory.type_param(*p)).collect();

                            // Cache type parameters for Application expansion.
                            // Use the canonical (merged-binder) SymbolId so the DefId
                            // matches what type reference resolution produces.
                            self.ctx
                                .cache_canonical_lib_type_params(name, sym_id, params.clone());

                            lib_types.push(ty);
                        } else if !params.is_empty() && !canonical_param_type_ids.is_empty() {
                            // For subsequent definitions with type params, substitute them
                            // with the canonical TypeIds to ensure consistency.
                            // This fixes the Array<T1> & Array<T2> problem where T1 != T2.
                            let mut subst = TypeSubstitution::new();
                            for (i, p) in params.iter().enumerate() {
                                if i < canonical_param_type_ids.len() {
                                    subst.insert(p.name, canonical_param_type_ids[i]);
                                }
                            }
                            if !subst.is_empty() {
                                let mut instantiator =
                                    TypeInstantiator::new(self.ctx.types, &subst);
                                let substituted_ty = instantiator.instantiate(ty);
                                lib_types.push(substituted_ty);
                            } else {
                                lib_types.push(ty);
                            }
                        } else {
                            lib_types.push(ty);
                        }
                        continue;
                    }

                    // Interface lowering returned ERROR - try as type alias
                    for (decl_idx, decl_arena) in &decls_with_arenas {
                        if let Some(node) = decl_arena.get(*decl_idx)
                            && let Some(alias) = decl_arena.get_type_alias(node)
                        {
                            let alias_lowering = lowering.with_arena(decl_arena);
                            let (ty, params) = alias_lowering.lower_type_alias_declaration(alias);
                            if ty != TypeId::ERROR {
                                // Cache type parameters for Application expansion.
                                // Use the canonical (merged-binder) SymbolId to avoid
                                // collisions between per-lib-context and main binder identities.
                                self.ctx
                                    .cache_canonical_lib_type_params(name, sym_id, params);
                                lib_types.push(ty);
                                break;
                            }
                        }
                    }
                    if !lib_types.is_empty() {
                        continue;
                    }
                }

                let decl_idx = symbol.value_declaration;
                if decl_idx.0 != u32::MAX {
                    let value_arena = lib_ctx
                        .binder
                        .declaration_arenas
                        .get(&(sym_id, decl_idx))
                        .and_then(|v| v.first())
                        .map_or(fallback_arena, |arc| arc.as_ref());
                    let value_lowering = lowering.with_arena(value_arena);
                    lib_types.push(value_lowering.lower_type(decl_idx));
                    break;
                }
            }
        }

        let mut lib_type_id = match lib_types.len() {
            1 => Some(lib_types[0]),
            n if n > 1 => {
                let mut merged = lib_types[0];
                for &ty in &lib_types[1..] {
                    merged = factory.intersection2(merged, ty);
                }
                Some(merged)
            }
            _ => None,
        };

        // Merge global augmentations (declare global { interface X { ... } }).
        if let Some(merged) = self.merge_global_augmentations(name, lib_type_id, lib_contexts) {
            lib_type_id = Some(merged);
        }

        (lib_type_id, first_params.unwrap_or_default())
    }

    /// Resolve an alias symbol to its target symbol.
    ///
    /// This function follows alias chains to find the ultimate target symbol.
    /// Aliases are created by:
    /// - ES6 imports: `import { foo } from 'bar'`
    /// - Import equals: `import foo = require('bar')`
    /// - Re-exports: `export { foo } from 'bar'`
    ///
    /// ## Alias Resolution:
    /// - Follows re-export chains recursively
    /// - Uses binder's `resolve_import_symbol` for ES6 imports
    /// - Falls back to `module_exports` lookup
    /// - Handles circular references with `visited_aliases` tracking
    ///
    /// ## Re-export Chains:
    /// ```typescript
    /// // a.ts exports { x } from 'b.ts'
    /// // b.ts exports { x } from 'c.ts'
    /// // c.ts exports { x }
    /// // resolve_alias_symbol('x' in a.ts) → 'x' in c.ts
    /// ```
    ///
    /// ## Returns:
    /// - `Some(SymbolId)` - The resolved target symbol
    /// - `None` - If circular reference detected or resolution failed
    pub(crate) fn resolve_alias_symbol(
        &self,
        sym_id: tsz_binder::SymbolId,
        visited_aliases: &mut Vec<tsz_binder::SymbolId>,
    ) -> Option<tsz_binder::SymbolId> {
        // Prevent stack overflow from long alias chains
        const MAX_ALIAS_RESOLUTION_DEPTH: usize = 128;
        if visited_aliases.len() >= MAX_ALIAS_RESOLUTION_DEPTH {
            return None;
        }

        // Use get_symbol_with_libs to properly handle symbols from lib files
        let lib_binders = self.get_lib_binders();
        let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)?;
        // Defensive: Verify symbol is valid before accessing fields
        // This prevents crashes when symbol IDs reference non-existent symbols
        if symbol.flags & symbol_flags::ALIAS == 0 {
            return Some(sym_id);
        }
        if visited_aliases.contains(&sym_id) {
            return None;
        }
        visited_aliases.push(sym_id);

        // First, try using the binder's resolve_import_symbol which follows re-export chains
        // This handles both named re-exports (`export { foo } from 'bar'`) and wildcard
        // re-exports (`export * from 'bar'`), properly following chains like:
        // a.ts exports { x } from 'b.ts'
        // b.ts exports { x } from 'c.ts'
        // c.ts exports { x }
        if let Some(resolved_sym_id) = self.ctx.binder.resolve_import_symbol(sym_id) {
            // Prevent infinite loops in re-export chains
            if !visited_aliases.contains(&resolved_sym_id) {
                return self.resolve_alias_symbol(resolved_sym_id, visited_aliases);
            }
        }

        // Fallback to direct module_exports lookup for backward compatibility
        // Handle ES6 imports: import { X } from 'module' or import X from 'module'
        // The binder sets import_module and import_name for these
        if let Some(ref module_name) = symbol.import_module {
            let export_name = symbol
                .import_name
                .as_deref()
                .unwrap_or(&symbol.escaped_name);
            let source_file_idx = self
                .ctx
                .resolve_symbol_file_index(sym_id)
                .unwrap_or(self.ctx.current_file_idx);
            // Look up the exported symbol in module_exports.
            // Only do this for named/default imports (import_name is Some).
            // For namespace/require imports (`import X = require("m")`),
            // import_name is None and escaped_name could accidentally match
            // a specific export, resolving the alias to that export instead
            // of the module namespace.
            if symbol.import_name.is_some() {
                if let Some(target_sym_id) = self.resolve_cross_file_export_from_file(
                    module_name,
                    export_name,
                    Some(source_file_idx),
                ) {
                    return self.resolve_alias_symbol(target_sym_id, visited_aliases);
                }
                if let Some(exports) = self.ctx.binder.module_exports.get(module_name)
                    && let Some(target_sym_id) = exports.get(export_name)
                {
                    // Recursively resolve if the target is also an alias
                    return self.resolve_alias_symbol(target_sym_id, visited_aliases);
                }
                if let Some(all_binders) = &self.ctx.all_binders {
                    if let Some(file_indices) = self.ctx.files_for_module_specifier(module_name) {
                        for &file_idx in file_indices {
                            if let Some(binder) = all_binders.get(file_idx)
                                && let Some(exports) = binder.module_exports.get(module_name)
                                && let Some(target_sym_id) = exports.get(export_name)
                            {
                                return self.resolve_alias_symbol(target_sym_id, visited_aliases);
                            }
                        }
                    } else {
                        for binder in all_binders.iter() {
                            if let Some(exports) = binder.module_exports.get(module_name)
                                && let Some(target_sym_id) = exports.get(export_name)
                            {
                                return self.resolve_alias_symbol(target_sym_id, visited_aliases);
                            }
                        }
                    }
                }
            }

            if symbol.import_name.is_some()
                && let Some(target_sym_id) =
                    self.resolve_named_export_via_export_equals(module_name, export_name)
            {
                return self.resolve_alias_symbol(target_sym_id, visited_aliases);
            }

            // For namespace/require imports (`import X = require("m")`), import_name
            // is None and the symbol's escaped_name won't match any module export.
            // Try the module's `export =` value (stored under key "export=").
            // This handles `declare module "react" { export = __React; }`.
            if symbol.import_name.is_none() {
                if let Some(exports) = self.ctx.binder.module_exports.get(module_name)
                    && let Some(target_sym_id) = exports.get("export=")
                {
                    return self.resolve_alias_symbol(target_sym_id, visited_aliases);
                }
                if let Some(all_binders) = &self.ctx.all_binders {
                    if let Some(file_indices) = self.ctx.files_for_module_specifier(module_name) {
                        for &file_idx in file_indices {
                            if let Some(binder) = all_binders.get(file_idx)
                                && let Some(exports) = binder.module_exports.get(module_name)
                                && let Some(target_sym_id) = exports.get("export=")
                            {
                                return self.resolve_alias_symbol(target_sym_id, visited_aliases);
                            }
                        }
                    } else {
                        for binder in all_binders.iter() {
                            if let Some(exports) = binder.module_exports.get(module_name)
                                && let Some(target_sym_id) = exports.get("export=")
                            {
                                return self.resolve_alias_symbol(target_sym_id, visited_aliases);
                            }
                        }
                    }
                }
            }
            // Cross-file fallback: the module specifier is relative to the
            // declaring file, not the current file.  Use resolve_symbol_file_index
            // to find the source file and resolve_import_target_from_file to
            // convert the relative specifier into a target file index.
            let source_file_idx = self
                .ctx
                .resolve_symbol_file_index(sym_id)
                .unwrap_or(self.ctx.current_file_idx);
            if let Some(target_idx) = self
                .ctx
                .resolve_import_target_from_file(source_file_idx, module_name)
                && let Some(target_binder) = self.ctx.get_binder_for_file(target_idx)
            {
                let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
                if let Some(file_name) = target_arena.source_files.first().map(|f| &f.file_name) {
                    // Namespace imports (`import * as X` / `export * as X`)
                    // resolve to the module symbol rather than a specific export.
                    // The symbol itself acts as the namespace container; record
                    // all target exports for cross-file member resolution.
                    let is_namespace_import = export_name == "*"
                        || (symbol.import_name.is_none() && symbol.escaped_name != "default");
                    if is_namespace_import {
                        if let Some(exports) = target_binder.module_exports.get(file_name) {
                            for (_, &sid) in exports.iter() {
                                self.ctx.register_symbol_file_target(sid, target_idx);
                            }
                        }
                        // Return the alias symbol itself — the caller
                        // resolves members through resolve_symbol_export
                        // which follows import_module re-exports.
                        return Some(sym_id);
                    }
                    if let Some(exports) = target_binder.module_exports.get(file_name) {
                        if let Some(target_sym_id) = exports.get(export_name) {
                            self.ctx
                                .register_symbol_file_target(target_sym_id, target_idx);
                            return self.resolve_alias_symbol(target_sym_id, visited_aliases);
                        }
                        // For require imports, also try "export="
                        if symbol.import_name.is_none()
                            && let Some(target_sym_id) = exports.get("export=")
                        {
                            self.ctx
                                .register_symbol_file_target(target_sym_id, target_idx);
                            return self.resolve_alias_symbol(target_sym_id, visited_aliases);
                        }
                    }

                    // Follow named re-exports: `export { A } from './other'`
                    if let Some(file_reexports) = target_binder.reexports.get(file_name)
                        && let Some((source_module, original_name)) =
                            file_reexports.get(export_name)
                    {
                        let name_to_lookup = original_name.as_deref().unwrap_or(export_name);
                        if let Some(reexport_target_idx) = self
                            .ctx
                            .resolve_import_target_from_file(target_idx, source_module)
                            && let Some(reexport_binder) =
                                self.ctx.get_binder_for_file(reexport_target_idx)
                        {
                            let reexport_arena =
                                self.ctx.get_arena_for_file(reexport_target_idx as u32);
                            if let Some(reexport_file) =
                                reexport_arena.source_files.first().map(|f| &f.file_name)
                                && let Some(re_exports) =
                                    reexport_binder.module_exports.get(reexport_file)
                                && let Some(target_sym_id) = re_exports.get(name_to_lookup)
                            {
                                self.ctx.register_symbol_file_target(
                                    target_sym_id,
                                    reexport_target_idx,
                                );
                                return self.resolve_alias_symbol(target_sym_id, visited_aliases);
                            }
                        }
                    }

                    // Follow wildcard re-exports: `export * from './other'`
                    // This resolves imports through chains like:
                    //   g.ts: import { A } from './f'
                    //   f.ts: export * from './e'  (where e.ts exports A)
                    if let Some(source_modules) = target_binder.wildcard_reexports.get(file_name) {
                        let source_modules = source_modules.clone();
                        for source_module in &source_modules {
                            if let Some(wc_target_idx) = self
                                .ctx
                                .resolve_import_target_from_file(target_idx, source_module)
                                && let Some(wc_binder) = self.ctx.get_binder_for_file(wc_target_idx)
                            {
                                let wc_arena = self.ctx.get_arena_for_file(wc_target_idx as u32);
                                if let Some(wc_file) =
                                    wc_arena.source_files.first().map(|f| &f.file_name)
                                    && let Some(wc_exports) = wc_binder.module_exports.get(wc_file)
                                    && let Some(target_sym_id) = wc_exports.get(export_name)
                                {
                                    self.ctx
                                        .register_symbol_file_target(target_sym_id, wc_target_idx);
                                    return self
                                        .resolve_alias_symbol(target_sym_id, visited_aliases);
                                }
                            }
                        }
                    }
                }
            }
            // For ES6 imports, if we can't find the export, return the alias symbol itself
            // This allows the type checker to use the symbol reference
            return Some(sym_id);
        }

        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let decl_node = self.ctx.arena.get(decl_idx)?;
        if decl_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            let import = self.ctx.arena.get_import_decl(decl_node)?;
            // Track resolution depth to prevent stack overflow
            let depth = visited_aliases.len();
            if depth >= 128 {
                return None; // Prevent stack overflow
            }
            if let Some(target) =
                self.resolve_qualified_symbol_inner(import.module_specifier, visited_aliases, depth)
            {
                return Some(target);
            }
            return self
                .resolve_require_call_symbol(import.module_specifier, Some(visited_aliases));
        }
        if symbol.import_module.is_none() {
            return Some(sym_id);
        }
        // For other alias symbols (not ES6 imports or import equals), return None
        // to indicate we couldn't resolve the alias
        None
    }

    /// Get the text representation of a heritage clause name.
    ///
    /// Heritage clauses appear in class declarations as `extends` and `implements` clauses.
    /// This function extracts the name text from various heritage clause node types.
    ///
    /// ## Heritage Clause Types:
    /// - Simple identifier: `extends Foo` → "Foo"
    /// - Qualified name: `extends ns.Foo` → "ns.Foo"
    /// - Property access: `extends ns.Foo` → "ns.Foo"
    /// - Keyword literals: `extends null`, `extends true` → "null", "true"
    ///
    /// ## Examples:
    /// ```typescript
    /// class Foo extends Bar {} // "Bar"
    /// class Foo extends ns.Bar {} // "ns.Bar"
    /// class Foo implements IFoo {} // "IFoo"
    /// ```
    pub(crate) fn heritage_name_text(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;

        if node.kind == SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.clone());
        }

        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            return self.entity_name_text(idx);
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.ctx.arena.get_access_expr(node)?;
            let left = self.heritage_name_text(access.expression)?;
            let right = self
                .ctx
                .arena
                .get_identifier_at(access.name_or_argument)
                .map(|ident| ident.escaped_text.clone())?;
            let mut combined = String::with_capacity(left.len() + 1 + right.len());
            combined.push_str(&left);
            combined.push('.');
            combined.push_str(&right);
            return Some(combined);
        }

        // Handle keyword literals in heritage clauses (e.g., extends null, extends true)
        match node.kind {
            k if k == SyntaxKind::NullKeyword as u16 => return Some("null".to_string()),
            k if k == SyntaxKind::TrueKeyword as u16 => return Some("true".to_string()),
            k if k == SyntaxKind::FalseKeyword as u16 => return Some("false".to_string()),
            k if k == SyntaxKind::UndefinedKeyword as u16 => return Some("undefined".to_string()),
            k if k == SyntaxKind::NumericLiteral as u16 => return Some("0".to_string()),
            k if k == SyntaxKind::StringLiteral as u16 => return Some("0".to_string()),
            _ => {}
        }

        None
    }

    // Section 46: Namespace Type Utilities
    // -------------------------------------

    /// Propagate cross-file symbol tracking from a parent symbol to a member.
    ///
    /// When resolving members of cross-file namespace/module symbols, the member
    /// SymbolId must also be recorded as cross-file so `get_type_of_symbol`
    /// delegates to the correct file's binder.
    fn propagate_cross_file_target(&self, parent_sym_id: SymbolId, member_id: SymbolId) {
        if let Some(file_idx) = self.ctx.resolve_symbol_file_index(parent_sym_id) {
            self.ctx.register_symbol_file_target(member_id, file_idx);
        }
    }

    /// Resolve a namespace member symbol through alias chains, validate it is a
    /// runtime-value member, and return its type.
    ///
    /// Shared pipeline for namespace member resolution:
    /// 1. Propagate cross-file target tracking from parent to member
    /// 2. Follow alias chains to the actual symbol
    /// 3. Filter out type-only members
    /// 4. Filter out non-value symbols (types, interfaces, etc.)
    /// 5. Return the member's type
    fn resolve_validated_namespace_member(
        &mut self,
        parent_sym_id: SymbolId,
        member_id: SymbolId,
        property_name: &str,
    ) -> Option<TypeId> {
        self.propagate_cross_file_target(parent_sym_id, member_id);

        // Check is_type_only on the original export specifier BEFORE alias
        // resolution, since `export type { A }` sets is_type_only on the
        // export wrapper, not on the target class/function symbol.
        if let Some(member_symbol) = self
            .get_cross_file_symbol(member_id)
            .or_else(|| self.ctx.binder.get_symbol(member_id))
            && member_symbol.is_type_only
        {
            return None;
        }

        let resolved_member_id = if let Some(member_symbol) = self.get_cross_file_symbol(member_id)
            && member_symbol.flags & symbol_flags::ALIAS != 0
        {
            let mut visited_aliases = Vec::new();
            let resolved = self
                .resolve_alias_symbol(member_id, &mut visited_aliases)
                .unwrap_or(member_id);

            // Check if any intermediate alias in the chain is type-only.
            // This catches transitive type-only through import chains, e.g.:
            //   b.ts: import A from './a';  (not explicitly type-only)
            //   a.ts: export type { A as default };  (type-only export specifier)
            // The export specifier in a.ts has is_type_only = true, so A
            // should not be resolvable as a value member of b's namespace.
            let lib_binders = self.get_lib_binders();
            for &alias_sym_id in &visited_aliases {
                if let Some(alias_sym) = self
                    .ctx
                    .binder
                    .get_symbol_with_libs(alias_sym_id, &lib_binders)
                    && alias_sym.is_type_only
                {
                    return None;
                }
            }

            resolved
        } else {
            member_id
        };

        let parent_is_umd_export = self
            .get_cross_file_symbol(parent_sym_id)
            .or_else(|| self.ctx.binder.get_symbol(parent_sym_id))
            .is_some_and(|symbol| symbol.is_umd_export);
        if parent_is_umd_export
            && let Some(member_symbol) = self
                .get_cross_file_symbol(resolved_member_id)
                .or_else(|| self.ctx.binder.get_symbol(resolved_member_id))
            && (member_symbol.flags & symbol_flags::CLASS) != 0
            && member_symbol.value_declaration.is_some()
        {
            return Some(
                self.type_of_value_declaration_for_symbol_without_module_augmentations(
                    resolved_member_id,
                    member_symbol.value_declaration,
                ),
            );
        }

        self.get_validated_member_type(resolved_member_id, property_name)
    }

    fn namespace_has_umd_augmentation_member(
        &self,
        namespace_name: &str,
        property_name: &str,
    ) -> bool {
        let mut module_specs = Vec::new();
        let mut collect_from_binder = |binder: &tsz_binder::BinderState| {
            if let Some(sym_id) = binder.file_locals.get(namespace_name)
                && let Some(symbol) = binder.get_symbol(sym_id)
                && symbol.is_umd_export
                && let Some(module_spec) = symbol.import_module.as_ref()
                && !module_specs.iter().any(|existing| existing == module_spec)
            {
                module_specs.push(module_spec.clone());
            }
        };

        collect_from_binder(self.ctx.binder);
        if let Some(all_binders) = self.ctx.all_binders.as_ref() {
            for binder in all_binders.iter() {
                collect_from_binder(binder);
            }
        }

        module_specs.into_iter().any(|module_spec| {
            self.collect_module_augmentation_names(&module_spec)
                .iter()
                .any(|name| name == property_name)
        })
    }

    /// Check if a resolved member symbol is a runtime value and return its type.
    ///
    /// For already-resolved symbols (e.g., re-exported members that have already
    /// been followed through alias chains).
    fn get_validated_member_type(
        &mut self,
        resolved_member_id: SymbolId,
        property_name: &str,
    ) -> Option<TypeId> {
        if self.symbol_member_is_type_only(resolved_member_id, Some(property_name)) {
            return None;
        }
        // Namespace export tables may point at EXPORT_VALUE wrapper symbols
        // (e.g. `export { x }`). Treat them as runtime-value members.
        if let Some(member_symbol) = self.get_cross_file_symbol(resolved_member_id)
            && member_symbol.flags & symbol_flags::VALUE == 0
            && member_symbol.flags & symbol_flags::ALIAS == 0
            && member_symbol.flags & symbol_flags::EXPORT_VALUE == 0
        {
            return None;
        }

        // For merged interface+variable symbols (e.g., `export interface Point` +
        // `export var Point = 1`), `get_type_of_symbol` returns the interface type
        // because compute_type_of_symbol enters the INTERFACE branch. In namespace
        // member access (value position), we need the VALUE side type.
        // This mirrors the `is_merged_interface_value` path in `get_type_of_identifier`.
        //
        // Only apply to INTERFACE + VARIABLE merges, NOT CLASS+INTERFACE or
        // FUNCTION+INTERFACE merges, since get_type_of_symbol already handles
        // those correctly (CLASS/FUNCTION branches precede INTERFACE).
        let (flags, value_decl) = {
            let member_symbol = self
                .get_cross_file_symbol(resolved_member_id)
                .or_else(|| self.ctx.binder.get_symbol(resolved_member_id));
            match member_symbol {
                Some(sym) => (sym.flags, sym.value_declaration),
                None => (0, NodeIndex::default()),
            }
        };

        // Enum symbols accessed as namespace members (e.g., M3.Color) should
        // return the enum object type (with members as properties), not the
        // enum union type. This mirrors the pattern in identifier.rs for
        // direct enum references.
        if (flags & symbol_flags::ENUM) != 0
            && (flags & symbol_flags::ENUM_MEMBER) == 0
            && let Some(enum_obj) = self.enum_object_type(resolved_member_id)
        {
            return Some(enum_obj);
        }

        if flags != 0 {
            let is_merged_interface_variable = (flags & symbol_flags::INTERFACE) != 0
                && (flags & symbol_flags::VARIABLE) != 0
                && (flags & symbol_flags::CLASS) == 0
                && (flags & symbol_flags::FUNCTION) == 0;
            if is_merged_interface_variable {
                let value_type =
                    self.type_of_value_declaration_for_symbol(resolved_member_id, value_decl);
                if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                    return Some(value_type);
                }
            }
        }

        Some(self.get_type_of_symbol(resolved_member_id))
    }

    /// Resolve a namespace value member by name.
    ///
    /// This function resolves value members of namespace/enum types.
    /// It handles both namespace exports and enum members.
    ///
    /// ## Namespace Members:
    /// - Resolves exported members of namespace types
    /// - Filters out type-only members (no value flag)
    /// - Returns the type of the member symbol
    ///
    /// ## Enum Members:
    /// - Resolves enum members by name
    /// - Returns the member's literal type
    ///
    /// ## Examples:
    /// ```typescript
    /// namespace Utils {
    ///   export function helper(): void {}
    ///   export type Helper = number;
    /// }
    /// const x = Utils.helper; // resolve_namespace_value_member(Utils, "helper")
    /// // x has type () => void
    ///
    /// enum Color {
    ///   Red,
    ///   Green,
    /// }
    /// const c = Color.Red; // resolve_namespace_value_member(Color, "Red")
    /// // c has type Color.Red
    /// ```
    pub(crate) fn resolve_namespace_value_member(
        &mut self,
        object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        use tsz_solver::type_queries::{NamespaceMemberKind, classify_namespace_member};

        let classification = classify_namespace_member(self.ctx.types, object_type);

        // Handle TypeQuery types (typeof M) by resolving the symbol reference
        // to its underlying type (Lazy(DefId) for namespaces) and re-classifying.
        // This fixes property access on variables typed as `typeof Namespace`:
        //   var m: typeof M; m.Point  → should resolve namespace export "Point"
        if let NamespaceMemberKind::TypeQuery(sym_ref) = classification {
            let sym_id = SymbolId(sym_ref.0);
            if self
                .get_cross_file_symbol(sym_id)
                .is_some_and(|symbol| symbol.is_umd_export)
            {
                return self.resolve_namespace_value_member_from_symbol(sym_id, property_name);
            }
            let resolved_type = self.get_type_of_symbol(sym_id);
            if resolved_type != object_type
                && resolved_type != TypeId::ANY
                && resolved_type != TypeId::ERROR
            {
                return self.resolve_namespace_value_member(resolved_type, property_name);
            }
            return self.resolve_namespace_value_member_from_symbol(sym_id, property_name);
        }

        match classification {
            // Handle Lazy types (direct namespace/module references)
            NamespaceMemberKind::Lazy(def_id) => {
                let sym_id = self.ctx.def_to_symbol_id(def_id)?;

                // Extract needed data from symbol before mutable borrows below.
                let (sym_flags, sym_name, direct_member_id, module_export_member_id, import_module) = {
                    let symbol = self.get_cross_file_symbol(sym_id)?;
                    if symbol.flags & (symbol_flags::MODULE | symbol_flags::ENUM) == 0 {
                        return None;
                    }

                    tracing::trace!(
                        sym_id = sym_id.0,
                        symbol_name = symbol.escaped_name.as_str(),
                        property_name,
                        has_exports = symbol.exports.is_some(),
                        has_members = symbol.members.is_some(),
                        exports_len = symbol.exports.as_ref().map_or(0, |t| t.iter().count()),
                        members_len = symbol.members.as_ref().map_or(0, |t| t.iter().count()),
                        has_module_exports = self
                            .ctx
                            .binder
                            .module_exports
                            .contains_key(symbol.escaped_name.as_str()),
                        "resolve_namespace_value_member: lazy namespace lookup"
                    );

                    // Check direct exports first, then namespace members as fallback.
                    let direct_member_id = symbol
                        .exports
                        .as_ref()
                        .and_then(|exports| exports.get(property_name))
                        .or_else(|| {
                            symbol
                                .members
                                .as_ref()
                                .and_then(|members| members.get(property_name))
                        });

                    // Fallback: some ambient/module symbols keep exported members in
                    // binder.module_exports without populating symbol.exports/members.
                    let module_export_member_id = {
                        let module_name = symbol.escaped_name.as_str();
                        self.ctx
                            .binder
                            .module_exports
                            .get(module_name)
                            .and_then(|exports| exports.get(property_name))
                            .or_else(|| {
                                self.resolve_cross_file_namespace_exports(module_name)
                                    .and_then(|exports| exports.get(property_name))
                            })
                    };

                    (
                        symbol.flags,
                        symbol.escaped_name.clone(),
                        direct_member_id,
                        module_export_member_id,
                        symbol.import_module.clone(),
                    )
                };

                if (sym_flags & symbol_flags::MODULE) != 0 {
                    let module_name = import_module.as_deref().unwrap_or(sym_name.as_str());
                    if let Some(surface) = self.resolve_js_export_surface_for_module(
                        module_name,
                        Some(self.ctx.current_file_idx),
                    ) && surface.has_commonjs_exports
                    {
                        if let Some(prop) = surface
                            .named_exports
                            .iter()
                            .find(|prop| self.ctx.types.resolve_atom(prop.name) == property_name)
                        {
                            return Some(prop.type_id);
                        }
                        return None;
                    }
                }

                if let Some(member_id) = direct_member_id {
                    return self.resolve_validated_namespace_member(
                        sym_id,
                        member_id,
                        property_name,
                    );
                }

                if let Some(member_id) = module_export_member_id {
                    return self.resolve_validated_namespace_member(
                        sym_id,
                        member_id,
                        property_name,
                    );
                }

                // Check for re-exports from other modules
                // This handles cases like: export { foo } from './bar'
                if let Some(ref module_specifier) = import_module {
                    let mut visited_aliases = Vec::new();
                    if let Some(reexported_sym) = self.resolve_reexported_member_symbol(
                        module_specifier,
                        property_name,
                        &mut visited_aliases,
                    ) {
                        return self.get_validated_member_type(reexported_sym, property_name);
                    }

                    if self
                        .collect_module_augmentation_names(module_specifier)
                        .iter()
                        .any(|name| name == property_name)
                    {
                        return Some(TypeId::ANY);
                    }
                }

                if sym_flags & symbol_flags::ENUM != 0
                    && let Some(member_type) = self.enum_member_type_for_name(sym_id, property_name)
                {
                    return Some(member_type);
                }

                // Cross-file namespace merging: if the member wasn't found in this
                // symbol's exports, check other files for namespace declarations
                // with the same name that may export this member.
                if sym_flags & symbol_flags::MODULE != 0
                    && let Some(member_id) = self
                        .resolve_namespace_member_from_all_binders(sym_name.as_str(), property_name)
                {
                    return self.resolve_validated_namespace_member(
                        sym_id,
                        member_id,
                        property_name,
                    );
                }

                if sym_flags & symbol_flags::MODULE != 0
                    && self.namespace_has_umd_augmentation_member(sym_name.as_str(), property_name)
                {
                    return Some(TypeId::ANY);
                }

                None
            }

            // Handle ModuleNamespace types (import * as ns / namespace value bindings)
            NamespaceMemberKind::ModuleNamespace(sym_ref) => {
                let sym_id = SymbolId(sym_ref.0);
                let (
                    symbol_flags_value,
                    module_name,
                    direct_member_id,
                    module_export_member_id,
                    import_module,
                ) = {
                    let symbol = self.get_cross_file_symbol(sym_id)?;
                    if symbol.flags & (symbol_flags::MODULE | symbol_flags::ENUM) == 0 {
                        return None;
                    }

                    let module_name = symbol
                        .import_module
                        .as_deref()
                        .unwrap_or(symbol.escaped_name.as_str())
                        .to_string();

                    let import_module = symbol.import_module.clone();

                    let direct_member_id = symbol
                        .exports
                        .as_ref()
                        .and_then(|exports| exports.get(property_name))
                        .or_else(|| {
                            symbol
                                .members
                                .as_ref()
                                .and_then(|members| members.get(property_name))
                        });

                    let module_export_member_id = self
                        .ctx
                        .binder
                        .module_exports
                        .get(module_name.as_str())
                        .and_then(|exports| exports.get(property_name))
                        .or_else(|| {
                            self.resolve_cross_file_namespace_exports(module_name.as_str())
                                .and_then(|exports| exports.get(property_name))
                        });

                    (
                        symbol.flags,
                        module_name,
                        direct_member_id,
                        module_export_member_id,
                        import_module,
                    )
                };

                if symbol_flags_value & (symbol_flags::MODULE | symbol_flags::ENUM) == 0 {
                    return None;
                }

                if (symbol_flags_value & symbol_flags::MODULE) != 0
                    && let Some(surface) = self.resolve_js_export_surface_for_module(
                        module_name.as_str(),
                        Some(self.ctx.current_file_idx),
                    )
                    && surface.has_commonjs_exports
                {
                    return surface
                        .named_exports
                        .iter()
                        .find(|prop| self.ctx.types.resolve_atom(prop.name) == property_name)
                        .map(|prop| prop.type_id);
                }

                if let Some(member_id) = direct_member_id {
                    return self.resolve_validated_namespace_member(
                        sym_id,
                        member_id,
                        property_name,
                    );
                }

                if let Some(member_id) = module_export_member_id {
                    return self.resolve_validated_namespace_member(
                        sym_id,
                        member_id,
                        property_name,
                    );
                }

                if let Some(ref module_specifier) = import_module
                    && self
                        .collect_module_augmentation_names(module_specifier)
                        .iter()
                        .any(|name| name == property_name)
                {
                    return Some(TypeId::ANY);
                }

                None
            }

            // Handle Callable types from merged class+namespace or function+namespace symbols
            // When a class/function merges with a namespace, the type is a Callable with
            // properties containing the namespace exports
            NamespaceMemberKind::Callable(_) => {
                // Check if the callable has the property as a member (from namespace merge)
                tsz_solver::type_queries::find_property_in_type_by_str(
                    self.ctx.types,
                    object_type,
                    property_name,
                )
                .map(|prop| prop.type_id)
            }

            // TSZ-4: Handle Enum types for enum member property access (E.A)
            NamespaceMemberKind::Enum(def_id) => {
                // Resolve the DefId to a SymbolId and reuse the enum member lookup logic
                let sym_id = self.ctx.def_to_symbol_id(def_id)?;

                // Use cross-file-aware lookup: SymbolIds from cross-file enums
                // map to wrong symbols in the local binder (SymbolId collision).
                let symbol = self.get_cross_file_symbol(sym_id)?;

                if symbol.flags & symbol_flags::ENUM == 0 {
                    return None;
                }

                // Check direct exports first
                if let Some(exports) = symbol.exports.as_ref()
                    && let Some(member_id) = exports.get(property_name)
                {
                    self.propagate_cross_file_target(sym_id, member_id);
                    return Some(self.get_type_of_symbol(member_id));
                }

                // Fallback to enum_member_type_for_name
                self.enum_member_type_for_name(sym_id, property_name)
            }

            // TypeQuery is handled by the early return above; unreachable here
            NamespaceMemberKind::TypeQuery(_) => None,

            NamespaceMemberKind::Other => {
                // Handle intersection types: when a module/namespace value is an
                // intersection (e.g., `export = __React` produces an intersection of
                // the namespace's type-side and value-side), try each member.
                if let Some(members) =
                    tsz_solver::type_queries::get_intersection_members(self.ctx.types, object_type)
                {
                    for member in members {
                        if let Some(result) =
                            self.resolve_namespace_value_member(member, property_name)
                        {
                            return Some(result);
                        }
                    }
                }
                None
            }
        }
    }

    // Section 47: Node Predicate Utilities
    // ------------------------------------

    /// Check if a variable declaration is a catch clause variable.
    ///
    /// This function determines if a given variable declaration node is
    /// the variable declaration of a catch clause (try/catch statement).
    ///
    /// ## Catch Clause Variables:
    /// - Catch clause variables have special scoping rules
    /// - They are block-scoped to the catch block
    /// - They shadow variables with the same name in outer scopes
    /// - They cannot be accessed before declaration (TDZ applies)
    ///
    /// ## Examples:
    /// ```typescript
    /// try {
    ///   throw new Error("error");
    /// } catch (e) {
    ///   // e is a catch clause variable
    ///   console.log(e.message);
    /// }
    /// // is_catch_clause_variable_declaration(e_node) → true
    ///
    /// const x = 5;
    /// // is_catch_clause_variable_declaration(x_node) → false
    /// ```
    pub(crate) fn is_catch_clause_variable_declaration(&self, var_decl_idx: NodeIndex) -> bool {
        let Some(ext) = self.ctx.arena.get_extended(var_decl_idx) else {
            return false;
        };
        let parent_idx = ext.parent;
        if parent_idx.is_none() {
            return false;
        }
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        if parent_node.kind != syntax_kind_ext::CATCH_CLAUSE {
            return false;
        }
        let Some(catch) = self.ctx.arena.get_catch_clause(parent_node) else {
            return false;
        };
        catch.variable_declaration == var_decl_idx
    }

    /// Check if a variable declaration is in a `for...in` statement.
    /// For-in loop variables are always typed as `string`.
    ///
    /// AST walk: `VariableDeclaration` → `VariableDeclarationList` → `ForInStatement`
    pub(crate) fn is_for_in_variable_declaration(&self, var_decl_idx: NodeIndex) -> bool {
        // VariableDeclaration → parent (VariableDeclarationList)
        let Some(ext) = self.ctx.arena.get_extended(var_decl_idx) else {
            return false;
        };
        let vdl_idx = ext.parent;
        if vdl_idx.is_none() {
            return false;
        }
        // VariableDeclarationList → parent (ForInStatement?)
        let Some(vdl_ext) = self.ctx.arena.get_extended(vdl_idx) else {
            return false;
        };
        let for_in_idx = vdl_ext.parent;
        if for_in_idx.is_none() {
            return false;
        }
        let Some(parent_node) = self.ctx.arena.get(for_in_idx) else {
            return false;
        };
        parent_node.kind == syntax_kind_ext::FOR_IN_STATEMENT
    }

    /// Check if a variable declaration is in a `for...in` or `for...of` statement.
    /// These loop variables get their type from the iterable expression, not from
    /// the variable declaration itself.
    ///
    /// AST walk: `VariableDeclaration` → `VariableDeclarationList` → `ForInStatement`/`ForOfStatement`
    pub(crate) fn is_for_in_or_of_variable_declaration(&self, var_decl_idx: NodeIndex) -> bool {
        let Some(ext) = self.ctx.arena.get_extended(var_decl_idx) else {
            return false;
        };
        let vdl_idx = ext.parent;
        if vdl_idx.is_none() {
            return false;
        }
        let Some(vdl_ext) = self.ctx.arena.get_extended(vdl_idx) else {
            return false;
        };
        let parent_idx = vdl_ext.parent;
        if parent_idx.is_none() {
            return false;
        }
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        parent_node.kind == syntax_kind_ext::FOR_IN_STATEMENT
            || parent_node.kind == syntax_kind_ext::FOR_OF_STATEMENT
    }

    // Section 48: Type Predicate Utilities
    // -------------------------------------

    /// Get the target of a type predicate from a parameter name node.
    ///
    /// Type predicates are used in function signatures to narrow types
    /// based on runtime checks. The target can be either `this` or an
    /// identifier parameter name.
    ///
    /// ## Type Predicate Targets:
    /// - **This**: `asserts this is T` - Used in methods to narrow the receiver type
    /// - **Identifier**: `argName is T` - Used to narrow a parameter's type
    ///
    /// ## Examples:
    /// ```typescript
    /// // This type predicate
    /// function assertIsString(this: unknown): asserts this is string {
    ///   if (typeof this === 'string') {
    ///     return; // this is narrowed to string
    ///   }
    ///   throw new Error('Not a string');
    /// }
    /// // type_predicate_target(thisKeywordNode) → TypePredicateTarget::This
    ///
    /// // Identifier type predicate
    /// function isString(val: unknown): val is string {
    ///   return typeof val === 'string';
    /// }
    /// // type_predicate_target(valIdentifierNode) → TypePredicateTarget::Identifier("val")
    /// ```
    pub(crate) fn type_predicate_target(
        &self,
        param_name: NodeIndex,
    ) -> Option<TypePredicateTarget> {
        let node = self.ctx.arena.get(param_name)?;
        if node.kind == SyntaxKind::ThisKeyword as u16 || node.kind == syntax_kind_ext::THIS_TYPE {
            return Some(TypePredicateTarget::This);
        }

        self.ctx.arena.get_identifier(node).map(|ident| {
            TypePredicateTarget::Identifier(self.ctx.types.intern_string(&ident.escaped_text))
        })
    }

    // Section 49: Constructor Accessibility Utilities
    // -----------------------------------------------

    /// Convert a constructor access level to its string representation.
    ///
    /// This function is used for error messages to display the accessibility
    /// level of a constructor (private, protected, or public).
    ///
    /// ## Constructor Accessibility:
    /// - **Private**: `private constructor()` - Only accessible within the class
    /// - **Protected**: `protected constructor()` - Accessible within class and subclasses
    /// - **Public**: `constructor()` or `public constructor()` - Accessible everywhere
    ///
    /// ## Examples:
    /// ```typescript
    /// class Singleton {
    ///   private constructor() {} // Only accessible within Singleton
    /// }
    /// // constructor_access_name(Some(Private)) → "private"
    ///
    /// class Base {
    ///   protected constructor() {} // Accessible in Base and subclasses
    /// }
    /// // constructor_access_name(Some(Protected)) → "protected"
    ///
    /// class Public {
    ///   constructor() {} // Public by default
    /// }
    /// // constructor_access_name(None) → "public"
    /// ```
    pub(crate) const fn constructor_access_name(level: Option<MemberAccessLevel>) -> &'static str {
        match level {
            Some(MemberAccessLevel::Private) => "private",
            Some(MemberAccessLevel::Protected) => "protected",
            None => "public",
        }
    }

    /// Get the numeric rank of a constructor access level.
    ///
    /// This function assigns a numeric value to access levels for comparison:
    /// - Private (2) > Protected (1) > Public (0)
    ///
    /// Higher ranks indicate more restrictive access levels. This is used
    /// to determine if a constructor accessibility mismatch exists between
    /// source and target types.
    ///
    /// ## Rank Ordering:
    /// ```typescript
    /// Private (2)   - Most restrictive
    /// Protected (1) - Medium restrictiveness
    /// Public (0)    - Least restrictive
    /// ```
    ///
    /// ## Examples:
    /// ```typescript
    /// constructor_access_rank(Some(Private))    // → 2
    /// constructor_access_rank(Some(Protected)) // → 1
    /// constructor_access_rank(None)            // → 0 (Public)
    /// ```
    pub(crate) const fn constructor_access_rank(level: Option<MemberAccessLevel>) -> u8 {
        match level {
            Some(MemberAccessLevel::Private) => 2,
            Some(MemberAccessLevel::Protected) => 1,
            None => 0,
        }
    }

    /// Get the excluded symbol flags for a given symbol.
    ///
    /// Each symbol type (function, class, interface, etc.) has specific
    /// flags that represent incompatible symbols that cannot share the same name.
    /// This function returns those exclusion flags.
    ///
    /// ## Symbol Exclusion Rules:
    /// - Functions exclude other functions with the same name
    /// - Classes exclude interfaces with the same name (unless merging)
    /// - Variables exclude other variables with the same name in the same scope
    ///
    /// ## Examples:
    /// ```typescript
    /// // Function exclusions
    /// function foo() {}
    /// function foo() {} // ERROR: Duplicate function declaration
    ///
    /// // Class/Interface merging (allowed)
    /// interface Foo {}
    /// class Foo {} // Allowed: interface and class can merge
    ///
    /// // Variable exclusions
    /// let x = 1;
    /// let x = 2; // ERROR: Duplicate variable declaration
    /// ```
    const fn excluded_symbol_flags(flags: u32) -> u32 {
        if (flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0 {
            return symbol_flags::FUNCTION_SCOPED_VARIABLE_EXCLUDES;
        }
        if (flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0 {
            return symbol_flags::BLOCK_SCOPED_VARIABLE_EXCLUDES;
        }
        if (flags & symbol_flags::FUNCTION) != 0 {
            return symbol_flags::CLASS;
        }
        if (flags & symbol_flags::CLASS) != 0 {
            return symbol_flags::FUNCTION;
        }
        if (flags & symbol_flags::INTERFACE) != 0 {
            return symbol_flags::INTERFACE_EXCLUDES;
        }
        if (flags & symbol_flags::TYPE_ALIAS) != 0 {
            return symbol_flags::TYPE_ALIAS_EXCLUDES;
        }
        if (flags & symbol_flags::REGULAR_ENUM) != 0 {
            return symbol_flags::REGULAR_ENUM_EXCLUDES;
        }
        if (flags & symbol_flags::CONST_ENUM) != 0 {
            return symbol_flags::CONST_ENUM_EXCLUDES;
        }
        // Check NAMESPACE_MODULE before VALUE_MODULE since namespaces have both flags
        // and NAMESPACE_MODULE_EXCLUDES (NONE) allows more merging than VALUE_MODULE_EXCLUDES
        if (flags & symbol_flags::NAMESPACE_MODULE) != 0 {
            return symbol_flags::NAMESPACE_MODULE_EXCLUDES;
        }
        if (flags & symbol_flags::VALUE_MODULE) != 0 {
            return symbol_flags::VALUE_MODULE_EXCLUDES;
        }
        if (flags & symbol_flags::GET_ACCESSOR) != 0 {
            return symbol_flags::GET_ACCESSOR_EXCLUDES;
        }
        if (flags & symbol_flags::SET_ACCESSOR) != 0 {
            return symbol_flags::SET_ACCESSOR_EXCLUDES;
        }
        if (flags & symbol_flags::METHOD) != 0 {
            return symbol_flags::METHOD_EXCLUDES;
        }
        if (flags & symbol_flags::ALIAS) != 0 {
            return symbol_flags::ALIAS_EXCLUDES;
        }
        symbol_flags::NONE
    }

    /// Check if two declarations conflict based on their symbol flags.
    ///
    /// This function determines whether two symbols with the given flags
    /// can coexist in the same scope without conflict.
    ///
    /// ## Conflict Rules:
    /// - **Static vs Instance**: Static and instance members with the same name don't conflict
    /// - **Exclusion Flags**: If either declaration excludes the other's flags, they conflict
    ///
    /// ## Examples:
    /// ```typescript
    /// class Example {
    ///   static x = 1;  // Static member
    ///   x = 2;         // Instance member - no conflict
    /// }
    ///
    /// class Conflict {
    ///   foo() {}      // Method
    ///   foo: number;  // Property - CONFLICT!
    /// }
    ///
    /// interface Merge {
    ///   foo(): void;
    /// }
    /// interface Merge {
    ///   bar(): void;  // No conflict - different members
    /// }
    /// ```
    pub(crate) const fn declarations_conflict(flags_a: u32, flags_b: u32) -> bool {
        // Static and instance members with the same name don't conflict
        let a_is_static = (flags_a & symbol_flags::STATIC) != 0;
        let b_is_static = (flags_b & symbol_flags::STATIC) != 0;
        if a_is_static != b_is_static {
            return false;
        }

        let excludes_a = Self::excluded_symbol_flags(flags_a);
        let excludes_b = Self::excluded_symbol_flags(flags_b);
        (flags_a & excludes_b) != 0 || (flags_b & excludes_a) != 0
    }

    // Section 51: Literal Type Utilities
    // ----------------------------------

    /// Infer a literal type from an initializer expression.
    ///
    /// This function attempts to infer the most specific literal type from an
    /// expression, enabling const declarations to have literal types.
    ///
    /// **Literal Type Inference:**
    /// - **String literals**: `"hello"` → `"hello"` (string literal type)
    /// - **Numeric literals**: `42` → `42` (numeric literal type)
    /// - **Boolean literals**: `true` → `true`, `false` → `false`
    /// - **Null literal**: `null` → null type
    /// - **Unary expressions**: `-42` → `-42`, `+42` → `42`
    ///
    /// **Non-Literal Expressions:**
    /// - Complex expressions return None (not a literal)
    /// - Function calls, object literals, etc. return None
    ///
    /// **Const Declarations:**
    /// - `const x = "hello"` infers type `"hello"` (not `string`)
    /// - `let y = "hello"` infers type `string` (widened)
    /// - This function enables the const behavior
    ///
    /// ## Examples:
    /// ```typescript
    /// // String literal
    /// const greeting = "hello";  // Type: "hello"
    /// literal_type_from_initializer(greeting_node) → Some("hello")
    ///
    /// // Numeric literal
    /// const count = 42;  // Type: 42
    /// literal_type_from_initializer(count_node) → Some(42)
    ///
    /// // Negative number
    /// const temp = -42;  // Type: -42
    /// literal_type_from_initializer(temp_node) → Some(-42)
    ///
    /// // Boolean
    /// const flag = true;  // Type: true
    /// literal_type_from_initializer(flag_node) → Some(true)
    ///
    /// // Non-literal
    /// const arr = [1, 2, 3];  // Type: number[]
    /// literal_type_from_initializer(arr_node) → None
    /// ```
    pub(crate) fn literal_type_from_initializer(&self, idx: NodeIndex) -> Option<TypeId> {
        let node = self.ctx.arena.get(idx)?;

        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                let lit = self.ctx.arena.get_literal(node)?;
                Some(self.ctx.types.literal_string(&lit.text))
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.ctx.arena.get_literal(node)?;
                lit.value.map(|value| self.ctx.types.literal_number(value))
            }
            k if k == SyntaxKind::BigIntLiteral as u16 => {
                let lit = self.ctx.arena.get_literal(node)?;
                let text = lit.text.strip_suffix('n').unwrap_or(&lit.text);
                Some(self.ctx.types.literal_bigint(text))
            }
            k if k == SyntaxKind::TrueKeyword as u16 => Some(self.ctx.types.literal_boolean(true)),
            k if k == SyntaxKind::FalseKeyword as u16 => {
                Some(self.ctx.types.literal_boolean(false))
            }
            k if k == SyntaxKind::NullKeyword as u16 => Some(TypeId::NULL),
            // `undefined` in expression position is parsed as an Identifier with
            // text "undefined".  Treat it as a unit literal for discriminant narrowing.
            k if k == SyntaxKind::Identifier as u16 => {
                let ident = self.ctx.arena.get_identifier(node)?;
                if ident.escaped_text == "undefined" {
                    Some(TypeId::UNDEFINED)
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.ctx.arena.get_unary_expr(node)?;
                let op = unary.operator;
                if op != SyntaxKind::MinusToken as u16 && op != SyntaxKind::PlusToken as u16 {
                    return None;
                }
                let operand = unary.operand;
                let operand_node = self.ctx.arena.get(operand)?;
                if operand_node.kind == SyntaxKind::BigIntLiteral as u16 {
                    let lit = self.ctx.arena.get_literal(operand_node)?;
                    let text = lit.text.strip_suffix('n').unwrap_or(&lit.text);
                    let negative = op == SyntaxKind::MinusToken as u16;
                    return Some(self.ctx.types.literal_bigint_with_sign(negative, text));
                }
                if operand_node.kind != SyntaxKind::NumericLiteral as u16 {
                    return None;
                }
                let lit = self.ctx.arena.get_literal(operand_node)?;
                let value = lit.value?;
                let value = if op == SyntaxKind::MinusToken as u16 {
                    -value
                } else {
                    value
                };
                Some(self.ctx.types.literal_number(value))
            }
            k if k == tsz_parser::parser::syntax_kind_ext::BINARY_EXPRESSION => {
                let binary = self.ctx.arena.get_binary_expr(node)?;
                if binary.operator_token == tsz_scanner::SyntaxKind::CommaToken as u16 {
                    return self.literal_type_from_initializer(binary.right);
                }
                if binary.operator_token == tsz_scanner::SyntaxKind::AmpersandAmpersandToken as u16
                {
                    let left_ty = self.literal_type_from_initializer(binary.left);
                    let right_ty = self.literal_type_from_initializer(binary.right);
                    if let (Some(l), Some(r)) = (left_ty, right_ty) {
                        let evaluator = tsz_solver::BinaryOpEvaluator::new(self.ctx.types);
                        if let crate::query_boundaries::type_computation::core::BinaryOpResult::Success(res) =
                            evaluator.evaluate(l, r, "&&")
                        {
                            return Some(res);
                        }
                    }
                }
                if binary.operator_token == tsz_scanner::SyntaxKind::BarBarToken as u16 {
                    let left_ty = self.literal_type_from_initializer(binary.left);
                    let right_ty = self.literal_type_from_initializer(binary.right);
                    if let (Some(l), Some(r)) = (left_ty, right_ty) {
                        let evaluator = tsz_solver::BinaryOpEvaluator::new(self.ctx.types);
                        if let crate::query_boundaries::type_computation::core::BinaryOpResult::Success(res) =
                            evaluator.evaluate(l, r, "||")
                        {
                            return Some(res);
                        }
                    }
                }
                None
            }
            k if k == tsz_parser::parser::syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let paren = self.ctx.arena.get_parenthesized(node)?;
                self.literal_type_from_initializer(paren.expression)
            }
            k if k == tsz_parser::parser::syntax_kind_ext::TEMPLATE_EXPRESSION => {
                let template = self.ctx.arena.get_template_expr(node)?;
                // Get the head text (text before the first ${})
                let head_text = self
                    .ctx
                    .arena
                    .get(template.head)
                    .and_then(|n| self.ctx.arena.get_literal(n))
                    .map(|lit| lit.text.clone())
                    .unwrap_or_default();
                let mut result = head_text;
                // For each span, try to evaluate the expression to a string literal
                for &span_idx in &template.template_spans.nodes {
                    let span_node = self.ctx.arena.get(span_idx)?;
                    let span = self.ctx.arena.get_template_span(span_node)?;
                    // Recursively evaluate the expression inside ${}
                    let expr_type = self.literal_type_from_initializer(span.expression)?;
                    // Stringify the literal type (handles string, number, bigint,
                    // boolean, null, undefined — not just string literals)
                    let expr_str = tsz_solver::type_queries::stringify_literal_type(
                        self.ctx.types,
                        expr_type,
                    )?;
                    result.push_str(&expr_str);
                    // Get the text after this expression (middle or tail)
                    let tail_text = self
                        .ctx
                        .arena
                        .get(span.literal)
                        .and_then(|n| self.ctx.arena.get_literal(n))
                        .map(|lit| lit.text.clone())
                        .unwrap_or_default();
                    result.push_str(&tail_text);
                }
                Some(self.ctx.types.literal_string(&result))
            }
            _ => None,
        }
    }

    pub(crate) fn resolve_umd_namespace_name_for_module(
        &self,
        module_specifier: &str,
        source_file_idx: usize,
    ) -> Option<String> {
        let trimmed = module_specifier.trim().trim_matches('"').trim_matches('\'');
        let target_idx = self
            .ctx
            .resolve_import_target_from_file(source_file_idx, module_specifier)
            .or_else(|| self.ctx.resolve_import_target(module_specifier))
            .or_else(|| self.ctx.resolve_import_target(trimmed))
            .or_else(|| {
                self.ctx.all_arenas.as_ref().and_then(|arenas| {
                    arenas.iter().enumerate().find_map(|(idx, arena)| {
                        let file_name = arena.source_files.first()?.file_name.as_str();
                        (file_name == module_specifier || file_name == trimmed).then_some(idx)
                    })
                })
            })?;
        let target_binder = self.ctx.get_binder_for_file(target_idx)?;

        for (name, &sym_id) in target_binder.file_locals.iter() {
            if let Some(symbol) = target_binder.get_symbol(sym_id)
                && symbol.is_umd_export
            {
                return Some(name.clone());
            }
        }

        None
    }

    pub(crate) fn collect_namespace_exports_across_binders(
        &mut self,
        namespace_name: &str,
    ) -> Vec<(String, tsz_binder::SymbolId)> {
        let mut exports = Vec::new();
        let mut seen = rustc_hash::FxHashSet::default();

        let mut collect_from_binder =
            |binder: &tsz_binder::BinderState, file_idx: Option<usize>| {
                if let Some(ns_sym_id) = binder.file_locals.get(namespace_name)
                    && let Some(ns_symbol) = binder.get_symbol(ns_sym_id)
                    && ns_symbol.flags
                        & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)
                        != 0
                    && let Some(ns_exports) = ns_symbol.exports.as_ref()
                {
                    for (name, member_id) in ns_exports.iter() {
                        if seen.insert(name.clone()) {
                            if let Some(file_idx) = file_idx {
                                self.ctx.register_symbol_file_target(*member_id, file_idx);
                            }
                            exports.push((name.clone(), *member_id));
                        }
                    }
                }
            };

        collect_from_binder(self.ctx.binder, None);

        if let Some(all_binders) = self.ctx.all_binders.clone() {
            for (file_idx, binder) in all_binders.iter().enumerate() {
                collect_from_binder(binder, Some(file_idx));
            }
        }

        exports
    }

    pub(crate) fn resolve_umd_global_symbol_by_name(
        &mut self,
        namespace_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        if let Some(sym_id) = self.ctx.binder.file_locals.get(namespace_name) {
            let local_symbol = self
                .get_cross_file_symbol(sym_id)
                .or_else(|| self.ctx.binder.get_symbol(sym_id));
            if local_symbol.is_some_and(|symbol| symbol.is_umd_export) {
                return Some(sym_id);
            }
            let current_file_binding_shadows_umd = local_symbol.is_some_and(|symbol| {
                let shadowing_flags = symbol_flags::ALIAS
                    | symbol_flags::FUNCTION_SCOPED_VARIABLE
                    | symbol_flags::BLOCK_SCOPED_VARIABLE
                    | symbol_flags::FUNCTION
                    | symbol_flags::CLASS
                    | symbol_flags::ENUM;

                if (symbol.flags & shadowing_flags) == 0 {
                    return false;
                }

                symbol.declarations.iter().any(|&decl_idx| {
                    let mut saw_namespace_declaration = false;
                    let mut saw_instantiated_namespace = false;
                    let mut current = Some(decl_idx);
                    while let Some(node_idx) = current {
                        let Some(ext) = self.ctx.arena.get_extended(node_idx) else {
                            break;
                        };
                        if ext.parent.is_none() {
                            break;
                        }
                        let parent_idx = ext.parent;
                        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                            break;
                        };
                        if parent_node.kind
                            == tsz_parser::parser::syntax_kind_ext::MODULE_DECLARATION
                        {
                            saw_namespace_declaration = true;
                            if let Some(module) = self.ctx.arena.get_module(parent_node) {
                                let is_global_augmentation = (u32::from(parent_node.flags)
                                    & tsz_parser::parser::node_flags::GLOBAL_AUGMENTATION)
                                    != 0
                                    || self
                                        .ctx
                                        .arena
                                        .get(module.name)
                                        .and_then(|name_node| {
                                            if let Some(ident) =
                                                self.ctx.arena.get_identifier(name_node)
                                            {
                                                return Some(
                                                    ident.escaped_text.as_str() == "global",
                                                );
                                            }
                                            if name_node.kind
                                                == tsz_scanner::SyntaxKind::GlobalKeyword as u16
                                            {
                                                return Some(true);
                                            }
                                            None
                                        })
                                        .unwrap_or(false);
                                if is_global_augmentation {
                                    return false;
                                }
                            }
                            saw_instantiated_namespace |=
                                self.is_namespace_declaration_instantiated(parent_idx);
                        }
                        current = Some(parent_idx);
                    }
                    if saw_namespace_declaration {
                        saw_instantiated_namespace
                    } else {
                        true
                    }
                })
            });
            if current_file_binding_shadows_umd {
                return None;
            }
        }

        if let Some(all_binders) = self.ctx.all_binders.clone() {
            for (file_idx, binder) in all_binders.iter().enumerate() {
                if let Some(sym_id) = binder.file_locals.get(namespace_name) {
                    self.ctx.register_symbol_file_target(sym_id, file_idx);
                    let is_umd_export = self
                        .get_cross_file_symbol(sym_id)
                        .is_some_and(|symbol| symbol.is_umd_export);
                    if is_umd_export {
                        return Some(sym_id);
                    }
                }
            }
        }

        None
    }

    pub(crate) fn resolve_umd_global_member_by_name(
        &mut self,
        namespace_name: &str,
        property_name: &str,
    ) -> Option<TypeId> {
        let sym_id = self.resolve_umd_global_symbol_by_name(namespace_name)?;
        self.resolve_namespace_value_member_from_symbol(sym_id, property_name)
    }

    pub(crate) fn resolve_namespace_value_member_from_symbol(
        &mut self,
        sym_id: SymbolId,
        property_name: &str,
    ) -> Option<TypeId> {
        let (
            sym_flags,
            sym_name,
            direct_member_id,
            module_export_member_id,
            import_module,
            decl_file_idx,
        ) = {
            let symbol = self.get_cross_file_symbol(sym_id)?;
            if symbol.flags & (symbol_flags::MODULE | symbol_flags::ENUM | symbol_flags::ALIAS) == 0
            {
                return None;
            }

            let direct_member_id = symbol
                .exports
                .as_ref()
                .and_then(|exports| exports.get(property_name))
                .or_else(|| {
                    symbol
                        .members
                        .as_ref()
                        .and_then(|members| members.get(property_name))
                });

            let module_export_member_id = {
                let module_name = symbol.escaped_name.as_str();
                self.ctx
                    .binder
                    .module_exports
                    .get(module_name)
                    .and_then(|exports| exports.get(property_name))
                    .or_else(|| {
                        self.resolve_cross_file_namespace_exports(module_name)
                            .and_then(|exports| exports.get(property_name))
                    })
            };

            (
                symbol.flags,
                symbol.escaped_name.clone(),
                direct_member_id,
                module_export_member_id,
                symbol.import_module.clone(),
                symbol.decl_file_idx as usize,
            )
        };

        if let Some(member_id) = direct_member_id {
            return self.resolve_validated_namespace_member(sym_id, member_id, property_name);
        }

        if let Some(member_id) = module_export_member_id {
            return self.resolve_validated_namespace_member(sym_id, member_id, property_name);
        }

        if let Some(ref module_specifier) = import_module {
            if let Some(member_id) = self.resolve_module_member_from_specifier(
                module_specifier,
                property_name,
                decl_file_idx,
            ) {
                return self.resolve_validated_namespace_member(sym_id, member_id, property_name);
            }

            let mut visited_aliases = Vec::new();
            if let Some(reexported_sym) = self.resolve_reexported_member_symbol(
                module_specifier,
                property_name,
                &mut visited_aliases,
            ) {
                return self.get_validated_member_type(reexported_sym, property_name);
            }

            if self.module_augmentation_introduces_member(module_specifier, property_name) {
                return Some(TypeId::ANY);
            }

            if let Some(umd_name) =
                self.resolve_umd_namespace_name_for_module(module_specifier, decl_file_idx)
                && let Some(member_id) =
                    self.resolve_namespace_member_across_binders(&umd_name, property_name)
            {
                return self.resolve_validated_namespace_member(sym_id, member_id, property_name);
            }
        }

        if sym_flags & symbol_flags::ENUM != 0
            && let Some(member_type) = self.enum_member_type_for_name(sym_id, property_name)
        {
            return Some(member_type);
        }

        if sym_flags & symbol_flags::MODULE != 0
            && let Some(member_id) =
                self.resolve_namespace_member_across_binders(sym_name.as_str(), property_name)
        {
            return self.resolve_validated_namespace_member(sym_id, member_id, property_name);
        }

        None
    }

    fn resolve_module_member_from_specifier(
        &self,
        module_specifier: &str,
        property_name: &str,
        source_file_idx: usize,
    ) -> Option<tsz_binder::SymbolId> {
        self.resolve_effective_module_exports_from_file(module_specifier, Some(source_file_idx))
            .and_then(|exports| exports.get(property_name))
    }

    fn module_augmentation_introduces_member(
        &self,
        module_specifier: &str,
        property_name: &str,
    ) -> bool {
        !self
            .get_module_augmentation_declarations(module_specifier, property_name)
            .is_empty()
    }

    fn resolve_namespace_member_across_binders(
        &mut self,
        namespace_name: &str,
        property_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        let lookup_in_binder = |binder: &tsz_binder::BinderState,
                                file_idx: Option<usize>|
         -> Option<tsz_binder::SymbolId> {
            let ns_sym_id = binder.file_locals.get(namespace_name)?;
            let ns_symbol = binder.get_symbol(ns_sym_id)?;
            if ns_symbol.flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) == 0
            {
                return None;
            }
            let member_id = ns_symbol
                .exports
                .as_ref()
                .and_then(|exports| exports.get(property_name))
                .or_else(|| {
                    ns_symbol
                        .members
                        .as_ref()
                        .and_then(|members| members.get(property_name))
                })?;
            if let Some(file_idx) = file_idx {
                self.ctx.register_symbol_file_target(member_id, file_idx);
            }
            Some(member_id)
        };

        if let Some(member_id) = lookup_in_binder(self.ctx.binder, None) {
            return Some(member_id);
        }

        if let Some(all_binders) = self.ctx.all_binders.clone() {
            for (file_idx, binder) in all_binders.iter().enumerate() {
                if let Some(member_id) = lookup_in_binder(binder, Some(file_idx)) {
                    return Some(member_id);
                }
            }
        }

        None
    }
}
