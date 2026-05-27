//! Type checking query helpers: library type resolution, namespace/alias
//! utilities, constructor accessibility, and symbol exclusion logic.
//!
//! Type-only symbol detection has been extracted to
//! `queries/type_only.rs`.

use super::lib_resolution::{
    lib_def_id_from_node_in_lib_contexts, no_value_resolver, resolve_lib_context_fallback_arena,
    resolve_lib_node_in_lib_contexts,
};
use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use crate::symbols_domain::name_text::{
    entity_name_text_in_arena, property_access_chain_text_in_arena,
};
use tsz_binder::{SymbolId, symbol_flags};
use tsz_lowering::TypeLowering;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;
use tsz_solver::TypeParamInfo;
use tsz_solver::computation::TypeResolver;

impl<'a> CheckerState<'a> {
    pub(crate) fn resolve_actual_lib_name_to_def_id_for_lowering(
        &self,
        type_name: &str,
    ) -> Option<tsz_solver::DefId> {
        self.ctx.actual_lib_def_id_for_bare_name(type_name)
    }

    pub(crate) fn lib_name_has_local_augmentation(&self, name: &str) -> bool {
        self.ctx
            .binder
            .global_augmentations
            .get(name)
            .is_some_and(|v| !v.is_empty())
    }

    fn lib_name_depends_on_builtin_iterator_return(name: &str) -> bool {
        matches!(
            name,
            "Array"
                | "ArrayIterator"
                | "Iterator"
                | "IteratorObject"
                | "Map"
                | "MapIterator"
                | "RegExpStringIterator"
                | "Set"
                | "SetIterator"
                | "StringIterator"
        )
    }

    /// True when callers must skip `shared_lib_type_cache` for `name`:
    /// either this checker locally augments `name`, or `name` is multi-lib
    /// merged where property-listing order in printed diagnostic messages
    /// is sensitive to who resolves first (e.g. `Array<T>`).
    pub(crate) fn lib_name_locally_augmented(&self, name: &str) -> bool {
        // Array is merged across lib.es5/lib.es2015.iterable/etc.; cross-checker
        // shared TypeIds expose property-order races to the type printer
        // (e.g. mappedTypeWithAsClauseAndLateBoundProperty).
        if name == "Array" || name.starts_with("Intl.") {
            return true;
        }
        if Self::lib_name_depends_on_builtin_iterator_return(name) {
            return true;
        }
        self.lib_name_has_local_augmentation(name)
    }

    /// Resolve a lib type by name and also return its type parameters.
    /// Used by `register_boxed_types` for generic types like Array<T> to extract
    /// the actual type parameters from the interface definition rather than
    /// synthesizing fresh ones.
    pub(crate) fn resolve_lib_type_with_params(
        &mut self,
        name: &str,
    ) -> (Option<TypeId>, Vec<TypeParamInfo>) {
        use crate::query_boundaries::common::{TypeSubstitution, instantiate_type};

        if name == "Array"
            && self.ctx.share_owner_symbol_type_results
            && !self.ctx.emit_declarations()
            && !self.lib_name_has_local_augmentation(name)
            && let Some(ty) = TypeResolver::get_array_base_type(&self.ctx.types)
        {
            let params = TypeResolver::get_array_base_type_params(&self.ctx.types).to_vec();
            if !params.is_empty() {
                return (Some(ty), params);
            }
        }

        // Short-circuit via shared cache; skip when this checker locally
        // augments `name` (its merged TypeId would differ from peers').
        let lib_name_locally_augmented = self.lib_name_locally_augmented(name);
        if !lib_name_locally_augmented
            && let Some(ref shared) = self.ctx.shared_lib_type_cache
            && let Some(entry) = shared.get(name)
            && let Some(ty) = *entry
        {
            for lib_ctx in self.ctx.lib_contexts.iter() {
                if let Some(per_lib_sym) = lib_ctx.binder.file_locals.get(name) {
                    let def_id = self.ctx.get_canonical_lib_def_id(name, per_lib_sym);
                    if let Some(params) = self.ctx.get_def_type_params(def_id) {
                        return (Some(ty), params);
                    }
                    break;
                }
            }
        }

        let factory = self.ctx.types.factory();
        let lib_contexts = self.ctx.lib_contexts.clone();

        let mut lib_types: Vec<TypeId> = Vec::new();
        let mut first_params: Option<Vec<TypeParamInfo>> = None;
        let mut symbol_has_interface = false;
        // Track canonical TypeIds for the first definition's type parameters.
        // Subsequent definitions will have their type params substituted with these.
        let mut canonical_param_type_ids: Vec<TypeId> = Vec::new();

        for lib_ctx in lib_contexts.iter() {
            if let Some(sym_id) = lib_ctx.binder.file_locals.get(name)
                && let Some(symbol) = lib_ctx.binder.get_symbol(sym_id)
            {
                symbol_has_interface |= symbol.has_any_flags(symbol_flags::INTERFACE);
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
                        &lib_contexts,
                    )
                    .map(|sym_id| sym_id.0)
                };
                let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::DefId> {
                    lib_def_id_from_node_in_lib_contexts(
                        &self.ctx,
                        node_idx,
                        &decls_with_arenas,
                        fallback_arena,
                        &lib_contexts,
                    )
                };
                let name_resolver = |type_name: &str| -> Option<tsz_solver::DefId> {
                    self.resolve_actual_lib_name_to_def_id_for_lowering(type_name)
                        .or_else(|| self.resolve_entity_name_text_to_def_id_for_lowering(type_name))
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
                .with_builtin_iterator_return_type(self.builtin_iterator_return_intrinsic_type())
                .with_lazy_type_params_resolver(&lazy_type_params_resolver)
                .with_name_def_id_resolver(&name_resolver);
                let lowering = if self.ctx.all_binders.is_some()
                    || self.ctx.global_file_locals_index.is_some()
                {
                    lowering.prefer_name_def_id_resolution()
                } else {
                    lowering
                };

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
                                let substituted_ty = instantiate_type(self.ctx.types, ty, &subst);
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
                    merged = if symbol_has_interface && self.ctx.emit_declarations() {
                        self.merge_interface_types(merged, ty)
                    } else {
                        factory.intersection2(merged, ty)
                    };
                }
                Some(merged)
            }
            _ => None,
        };

        if let Some(ty) = lib_type_id {
            lib_type_id = Some(self.merge_lib_interface_heritage(ty, name));
        }

        // Merge global augmentations (declare global { interface X { ... } }).
        if let Some(merged) = self.merge_global_augmentations(name, lib_type_id, &lib_contexts) {
            lib_type_id = Some(merged);
            self.register_augmented_lib_body(name, merged);
        }

        // Mirror into shared cache when safe (no local augmentations).
        if !lib_name_locally_augmented && let Some(ref shared) = self.ctx.shared_lib_type_cache {
            shared.insert(name.to_string(), lib_type_id);
        }

        (lib_type_id, first_params.unwrap_or_default())
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
            return entity_name_text_in_arena(self.ctx.arena, idx);
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return property_access_chain_text_in_arena(self.ctx.arena, idx);
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
        let parent_name = self
            .get_cross_file_symbol(parent_sym_id)
            .or_else(|| self.ctx.binder.get_symbol(parent_sym_id))
            .map(|symbol| symbol.escaped_name.clone());
        let mut member_id = member_id;
        self.propagate_cross_file_target(parent_sym_id, member_id);

        // Check is_type_only on the original export specifier BEFORE alias
        // resolution, since `export type { A }` sets is_type_only on the
        // export wrapper, not on the target class/function symbol.
        if let Some(member_symbol) = self
            .get_cross_file_symbol(member_id)
            .or_else(|| self.ctx.binder.get_symbol(member_id))
            && member_symbol.is_type_only
        {
            if let Some(parent_name) = parent_name.as_deref()
                && let Some(alt_member_id) =
                    self.resolve_namespace_member_from_all_binders(parent_name, property_name)
                && alt_member_id != member_id
            {
                member_id = alt_member_id;
                self.propagate_cross_file_target(parent_sym_id, member_id);
            } else {
                return None;
            }
        }

        if let Some(member_symbol) = self
            .get_cross_file_symbol(member_id)
            .or_else(|| self.ctx.binder.get_symbol(member_id))
            && member_symbol.has_any_flags(symbol_flags::ALIAS)
            && member_symbol.import_name.is_none()
            && let Some(module_specifier) = member_symbol.import_module.clone()
        {
            let source_file_idx = if member_symbol.decl_file_idx == u32::MAX {
                self.ctx.current_file_idx
            } else {
                member_symbol.decl_file_idx as usize
            };
            if let Some(module_type) =
                self.commonjs_module_value_type(&module_specifier, Some(source_file_idx))
            {
                return Some(module_type);
            }
        }

        if let Some(member_symbol) = self
            .get_cross_file_symbol(member_id)
            .or_else(|| self.ctx.binder.get_symbol(member_id))
            && member_symbol.has_any_flags(symbol_flags::ALIAS)
            && member_symbol.import_name.as_deref() == Some("default")
            && let Some(module_specifier) = member_symbol.import_module.clone()
            && let Some(namespace_type) =
                self.node_esm_cjs_default_import_namespace_type(&module_specifier)
        {
            return Some(namespace_type);
        }

        let resolved_member_id = if let Some(member_symbol) = self.get_cross_file_symbol(member_id)
            && member_symbol.has_any_flags(symbol_flags::ALIAS)
        {
            let mut visited_aliases = AliasCycleTracker::new();
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
            for alias_sym_id in &visited_aliases {
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

        self.get_validated_member_type(resolved_member_id, property_name)
            .or_else(|| {
                let parent_name = parent_name.as_deref()?;
                let alt_member_id =
                    self.resolve_namespace_member_from_all_binders(parent_name, property_name)?;
                if alt_member_id == resolved_member_id {
                    return None;
                }
                self.propagate_cross_file_target(parent_sym_id, alt_member_id);
                self.get_validated_member_type(alt_member_id, property_name)
            })
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
            && !member_symbol.has_any_flags(
                symbol_flags::VALUE | symbol_flags::ALIAS | symbol_flags::EXPORT_VALUE,
            )
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
        use crate::query_boundaries::common::{NamespaceMemberKind, classify_namespace_member};

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
                    let symbol = self
                        .get_cross_file_symbol(sym_id)
                        .or_else(|| self.ctx.binder.get_symbol(sym_id))?;
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

                if let Some(module_specifier) = import_module.as_deref()
                    && let Some(member_type) = self.namespace_default_reexport_property_type(
                        module_specifier,
                        self.ctx
                            .resolve_symbol_file_index(sym_id)
                            .or(Some(self.ctx.current_file_idx)),
                        property_name,
                    )
                {
                    return Some(member_type);
                }

                if let Some(member_id) = direct_member_id {
                    let member_type =
                        self.resolve_validated_namespace_member(sym_id, member_id, property_name)?;
                    return if let Some(module_specifier) = import_module.as_deref() {
                        Some(self.apply_module_augmentations(
                            module_specifier,
                            property_name,
                            member_type,
                        ))
                    } else {
                        Some(member_type)
                    };
                }

                if let Some(member_id) = module_export_member_id {
                    let member_type =
                        self.resolve_validated_namespace_member(sym_id, member_id, property_name)?;
                    return if let Some(module_specifier) = import_module.as_deref() {
                        Some(self.apply_module_augmentations(
                            module_specifier,
                            property_name,
                            member_type,
                        ))
                    } else {
                        Some(member_type)
                    };
                }

                // Check for re-exports from other modules
                // This handles cases like: export { foo } from './bar'
                if let Some(ref module_specifier) = import_module {
                    let mut visited_aliases = AliasCycleTracker::new();
                    if let Some(reexported_sym) = self.resolve_reexported_member_symbol(
                        module_specifier,
                        property_name,
                        &mut visited_aliases,
                    ) {
                        let member_type =
                            self.get_validated_member_type(reexported_sym, property_name)?;
                        return Some(self.apply_module_augmentations(
                            module_specifier,
                            property_name,
                            member_type,
                        ));
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
                    let member_type =
                        self.resolve_validated_namespace_member(sym_id, member_id, property_name)?;
                    return if let Some(module_specifier) = import_module.as_deref() {
                        Some(self.apply_module_augmentations(
                            module_specifier,
                            property_name,
                            member_type,
                        ))
                    } else {
                        Some(member_type)
                    };
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

                if let Some(module_specifier) = import_module.as_deref()
                    && let Some(member_type) = self.namespace_default_reexport_property_type(
                        module_specifier,
                        self.ctx
                            .resolve_symbol_file_index(sym_id)
                            .or(Some(self.ctx.current_file_idx)),
                        property_name,
                    )
                {
                    return Some(member_type);
                }

                if let Some(member_id) = direct_member_id {
                    // Keep the main-branch type-only wildcard export guard for namespace
                    // imports, then apply the PR's augmentation-aware direct member path.
                    if let Some(ref module_specifier) = import_module
                        && self.is_member_type_only_wildcard_export(module_specifier, property_name)
                    {
                        return None;
                    }
                    let member_type =
                        self.resolve_validated_namespace_member(sym_id, member_id, property_name)?;
                    return if let Some(module_specifier) = import_module.as_deref() {
                        Some(self.apply_module_augmentations(
                            module_specifier,
                            property_name,
                            member_type,
                        ))
                    } else {
                        Some(member_type)
                    };
                }

                if let Some(member_id) = module_export_member_id {
                    // Check type-only wildcard export guard for module exports path
                    if let Some(ref module_specifier) = import_module
                        && self.is_member_type_only_wildcard_export(module_specifier, property_name)
                    {
                        return None;
                    }
                    let member_type =
                        self.resolve_validated_namespace_member(sym_id, member_id, property_name)?;
                    return if let Some(module_specifier) = import_module.as_deref() {
                        Some(self.apply_module_augmentations(
                            module_specifier,
                            property_name,
                            member_type,
                        ))
                    } else {
                        Some(member_type)
                    };
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
                crate::query_boundaries::common::find_property_by_str(
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

                if !symbol.has_any_flags(symbol_flags::ENUM) {
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
                if let Some(members) = crate::query_boundaries::common::intersection_members(
                    self.ctx.types,
                    object_type,
                ) {
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

    /// Check if a member is from a type-only wildcard export from a specific module.
    ///
    /// This handles cases like:
    ///   // intermediate.ts: export type * from './ghost'
    ///   // main.ts: import * as intermediate from './intermediate'
    ///   // intermediate.Ghost should not be accessible as a value
    ///
    /// Returns true if the member was re-exported via `export type * from '...'`
    /// and is therefore type-only.
    fn is_member_type_only_wildcard_export(
        &mut self,
        module_specifier: &str,
        member_name: &str,
    ) -> bool {
        // Resolve the target module file
        let Some(target_file_idx) = self.ctx.resolve_import_target(module_specifier) else {
            return false;
        };
        let Some(target_binder) = self.ctx.get_binder_for_file(target_file_idx) else {
            return false;
        };

        // Get the file name for looking up wildcard re-exports
        let target_file_name = self
            .ctx
            .get_arena_for_file(target_file_idx as u32)
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
            .unwrap_or_default();

        // Check if there's a type-only wildcard re-export
        if let Some(source_modules) = self
            .ctx
            .wildcard_reexports_for_file(target_binder, &target_file_name)
        {
            let type_only_flags = self
                .ctx
                .wildcard_reexports_type_only_for_file(target_binder, &target_file_name);

            for (i, source_module) in source_modules.iter().enumerate() {
                let is_type_only = type_only_flags
                    .and_then(|flags| flags.get(i).map(|(_, is_to)| *is_to))
                    .unwrap_or(false);

                if !is_type_only {
                    continue;
                }

                // This is a type-only wildcard export - check if the member comes from here
                if let Some(source_idx) = self
                    .ctx
                    .resolve_import_target_from_file(target_file_idx, source_module)
                    && let Some(source_binder) = self.ctx.get_binder_for_file(source_idx)
                {
                    let source_file_name = self
                        .ctx
                        .get_arena_for_file(source_idx as u32)
                        .source_files
                        .first()
                        .map(|sf| sf.file_name.clone())
                        .unwrap_or_default();

                    // Check if the member exists in the source module's exports
                    if let Some(exports) = self
                        .ctx
                        .module_exports_for_module(source_binder, &source_file_name)
                        && exports.get(member_name).is_some()
                    {
                        return true;
                    }
                }
            }
        }

        false
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

                if !symbol.has_any_flags(shadowing_flags) {
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
                                let is_global_augmentation = parent_node.is_global_augmentation()
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
            let symbol = self
                .get_cross_file_symbol(sym_id)
                .or_else(|| self.ctx.binder.get_symbol(sym_id))?;
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

        // Node ESM -> CJS namespace interop: `import * as ns from "./m.cjs"`
        // should treat `ns.default` as the namespace object (module.exports),
        // not as a potentially unrelated named `default` export value.
        if property_name == "default"
            && let Some(module_specifier) = import_module.as_deref()
            && self.ctx.compiler_options.module.is_node_module()
            && self.ctx.file_is_esm == Some(true)
            && !self.module_is_esm(module_specifier)
            && self.module_can_use_synthetic_default_import(module_specifier)
        {
            let namespace_type = self.get_type_of_symbol(sym_id);
            if namespace_type != TypeId::ERROR && namespace_type != TypeId::UNKNOWN {
                return Some(namespace_type);
            }
        }

        if let Some(member_id) = direct_member_id {
            // Check type-only wildcard export guard for direct members
            if let Some(ref module_specifier) = import_module
                && self.is_member_type_only_wildcard_export(module_specifier, property_name)
            {
                return None;
            }
            let member_type =
                self.resolve_validated_namespace_member(sym_id, member_id, property_name)?;
            return if let Some(module_specifier) = import_module.as_deref() {
                Some(self.apply_module_augmentations(module_specifier, property_name, member_type))
            } else {
                Some(member_type)
            };
        }

        if let Some(member_id) = module_export_member_id {
            // Check type-only wildcard export guard for module exports path
            if let Some(ref module_specifier) = import_module
                && self.is_member_type_only_wildcard_export(module_specifier, property_name)
            {
                return None;
            }
            let member_type =
                self.resolve_validated_namespace_member(sym_id, member_id, property_name)?;
            return if let Some(module_specifier) = import_module.as_deref() {
                Some(self.apply_module_augmentations(module_specifier, property_name, member_type))
            } else {
                Some(member_type)
            };
        }

        if let Some(ref module_specifier) = import_module {
            // Check type-only wildcard export guard before resolving module member
            if self.is_member_type_only_wildcard_export(module_specifier, property_name) {
                return None;
            }
            if let Some(member_id) = self.resolve_module_member_from_specifier(
                module_specifier,
                property_name,
                decl_file_idx,
            ) {
                let member_type =
                    self.resolve_validated_namespace_member(sym_id, member_id, property_name)?;
                return Some(self.apply_module_augmentations(
                    module_specifier,
                    property_name,
                    member_type,
                ));
            }

            let mut visited_aliases = AliasCycleTracker::new();
            if let Some(reexported_sym) = self.resolve_reexported_member_symbol(
                module_specifier,
                property_name,
                &mut visited_aliases,
            ) {
                let member_type = self.get_validated_member_type(reexported_sym, property_name)?;
                return Some(self.apply_module_augmentations(
                    module_specifier,
                    property_name,
                    member_type,
                ));
            }

            if self.module_augmentation_introduces_member(module_specifier, property_name) {
                return Some(TypeId::ANY);
            }

            if let Some(umd_name) =
                self.resolve_umd_namespace_name_for_module(module_specifier, decl_file_idx)
                && let Some(member_id) =
                    self.resolve_namespace_member_across_binders(&umd_name, property_name)
            {
                let member_type =
                    self.resolve_validated_namespace_member(sym_id, member_id, property_name)?;
                return Some(member_type);
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
            let member_type =
                self.resolve_validated_namespace_member(sym_id, member_id, property_name)?;
            return if let Some(module_specifier) = import_module.as_deref() {
                Some(self.apply_module_augmentations(module_specifier, property_name, member_type))
            } else {
                Some(member_type)
            };
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

#[cfg(test)]
mod tests {
    use crate::state::CheckerState;
    use tsz_binder::BinderState;
    use tsz_parser::parser::NodeArena;
    use tsz_solver::TypeParamInfo;
    use tsz_solver::construction::QueryDatabase;
    use tsz_solver::construction::TypeInterner;

    #[test]
    fn shared_array_resolution_reuses_registered_base_and_params() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let array_base = types.factory().object(Vec::new());
        let array_param = TypeParamInfo {
            name: types.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
        };
        types.set_array_base_type(array_base, vec![array_param]);

        let mut checker = CheckerState::new(
            &arena,
            &binder,
            &types,
            "test.ts".to_string(),
            crate::context::CheckerOptions::default(),
        );
        checker.ctx.share_owner_symbol_type_results = true;

        let (resolved, params) = checker.resolve_lib_type_with_params("Array");

        assert_eq!(resolved, Some(array_base));
        assert_eq!(params, vec![array_param]);

        let (resolved_string, params_string) = checker.resolve_lib_type_with_params("String");
        assert_eq!(resolved_string, None);
        assert_eq!(params_string, Vec::<TypeParamInfo>::new());
    }
}
