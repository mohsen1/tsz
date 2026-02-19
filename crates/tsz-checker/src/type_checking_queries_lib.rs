//! Type checking query helpers: library type resolution, namespace/alias
//! utilities, and type-only symbol detection.

use crate::state::{CheckerState, MemberAccessLevel};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_lowering::TypeLowering;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeParamInfo;
use tsz_solver::is_compiler_managed_type;
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
        use tsz_parser::parser::node::NodeAccess;
        use tsz_solver::TypeInstantiator;
        use tsz_solver::TypeSubstitution;

        let factory = self.ctx.types.factory();
        let lib_contexts = self.ctx.lib_contexts.clone();
        let binder_for_arena = |arena_ref: &NodeArena| -> Option<&tsz_binder::BinderState> {
            let arenas = self.ctx.all_arenas.as_ref()?;
            let binders = self.ctx.all_binders.as_ref()?;
            let arena_ptr = arena_ref as *const NodeArena;
            for (idx, arena) in arenas.iter().enumerate() {
                if Arc::as_ptr(arena) == arena_ptr {
                    return binders.get(idx).map(Arc::as_ref);
                }
            }
            None
        };

        let mut lib_types: Vec<TypeId> = Vec::new();
        let mut first_params: Option<Vec<TypeParamInfo>> = None;
        // Track canonical TypeIds for the first definition's type parameters.
        // Subsequent definitions will have their type params substituted with these.
        let mut canonical_param_type_ids: Vec<TypeId> = Vec::new();

        for lib_ctx in &lib_contexts {
            if let Some(sym_id) = lib_ctx.binder.file_locals.get(name)
                && let Some(symbol) = lib_ctx.binder.get_symbol(sym_id)
            {
                // Multi-arena setup: Get the fallback arena
                let fallback_arena: &NodeArena = lib_ctx
                    .binder
                    .symbol_arenas
                    .get(&sym_id)
                    .map_or_else(|| lib_ctx.arena.as_ref(), |arc| arc.as_ref());

                // Build declaration -> arena pairs using declaration_arenas
                // This is critical for merged interfaces like Array<T> that span multiple lib files
                let decls_with_arenas: Vec<(NodeIndex, &NodeArena)> = symbol
                    .declarations
                    .iter()
                    .flat_map(|&decl_idx| {
                        if let Some(arenas) =
                            lib_ctx.binder.declaration_arenas.get(&(sym_id, decl_idx))
                        {
                            arenas
                                .iter()
                                .map(|arc| (decl_idx, arc.as_ref()))
                                .collect::<Vec<_>>()
                        } else {
                            vec![(decl_idx, fallback_arena)]
                        }
                    })
                    .collect();

                // Create resolver that can look up names across all lib contexts and arenas
                let resolver = |node_idx: NodeIndex| -> Option<u32> {
                    // Check specific arenas first
                    for (_, arena) in &decls_with_arenas {
                        if let Some(ident_name) = arena.get_identifier_text(node_idx) {
                            if is_compiler_managed_type(ident_name) {
                                return None;
                            }
                            for ctx in &lib_contexts {
                                if let Some(found_sym) = ctx.binder.file_locals.get(ident_name) {
                                    return Some(found_sym.0);
                                }
                            }
                            break;
                        }
                    }
                    // Fallback to default arena
                    let ident_name = fallback_arena.get_identifier_text(node_idx)?;
                    if is_compiler_managed_type(ident_name) {
                        return None;
                    }
                    for ctx in &lib_contexts {
                        if let Some(found_sym) = ctx.binder.file_locals.get(ident_name) {
                            return Some(found_sym.0);
                        }
                    }
                    None
                };

                // Create def_id_resolver that converts SymbolIds to DefIds
                let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::DefId> {
                    resolver(node_idx)
                        .map(|sym_id| self.ctx.get_or_create_def_id(tsz_binder::SymbolId(sym_id)))
                };

                let lowering = TypeLowering::with_hybrid_resolver(
                    fallback_arena,
                    self.ctx.types,
                    &resolver,
                    &def_id_resolver,
                    &|_| None,
                );

                if !symbol.declarations.is_empty() {
                    // Use lower_merged_interface_declarations for proper multi-arena support
                    let (ty, params) =
                        lowering.lower_merged_interface_declarations(&decls_with_arenas);

                    // If interface lowering succeeded (not ERROR), use the result
                    if ty != TypeId::ERROR {
                        // For the first definition, record canonical type parameter TypeIds
                        if first_params.is_none() && !params.is_empty() {
                            first_params = Some(params.clone());
                            // Compute TypeIds for these canonical params
                            let factory = self.ctx.types.factory();
                            canonical_param_type_ids = params
                                .iter()
                                .map(|p| factory.type_param(p.clone()))
                                .collect();

                            // Cache type parameters for Application expansion.
                            // Use file binder's sym_id (after lib merge) so the def_id
                            // matches what type reference resolution produces.
                            let file_sym_id =
                                self.ctx.binder.file_locals.get(name).unwrap_or(sym_id);
                            let def_id = self.ctx.get_or_create_def_id(file_sym_id);
                            self.ctx.insert_def_type_params(def_id, params.clone());

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
                                // Cache type parameters for Application expansion
                                let def_id = self.ctx.get_or_create_def_id(sym_id);
                                self.ctx.insert_def_type_params(def_id, params);
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
                    merged = factory.intersection(vec![merged, ty]);
                }
                Some(merged)
            }
            _ => None,
        };

        // Merge global augmentations (same as resolve_lib_type_by_name)
        if let Some(augmentation_decls) = self.ctx.binder.global_augmentations.get(name)
            && !augmentation_decls.is_empty()
        {
            let current_arena: &NodeArena = self.ctx.arena;
            let binder_ref = self.ctx.binder;

            // Group augmentation declarations by arena
            let mut current_file_decls: Vec<NodeIndex> = Vec::new();
            let mut cross_file_groups: FxHashMap<usize, (Arc<NodeArena>, Vec<NodeIndex>)> =
                FxHashMap::default();

            for aug in augmentation_decls {
                if let Some(ref arena) = aug.arena {
                    let key = Arc::as_ptr(arena) as usize;
                    cross_file_groups
                        .entry(key)
                        .or_insert_with(|| (Arc::clone(arena), Vec::new()))
                        .1
                        .push(aug.node);
                } else {
                    current_file_decls.push(aug.node);
                }
            }

            let resolve_in_scope = |binder: &tsz_binder::BinderState,
                                    arena_ref: &NodeArena,
                                    node_idx: NodeIndex|
             -> Option<u32> {
                let ident_name = arena_ref.get_identifier_text(node_idx)?;
                let mut scope_id = binder.find_enclosing_scope(arena_ref, node_idx)?;
                while scope_id != tsz_binder::ScopeId::NONE {
                    let scope = binder.scopes.get(scope_id.0 as usize)?;
                    if let Some(sym_id) = scope.table.get(ident_name) {
                        return Some(sym_id.0);
                    }
                    scope_id = scope.parent;
                }
                None
            };

            // Helper: lower augmentation declarations using a given arena
            let mut lower_with_arena = |arena_ref: &NodeArena, decls: &[NodeIndex]| {
                let decl_binder = binder_for_arena(arena_ref).unwrap_or(binder_ref);
                let resolver = |node_idx: NodeIndex| -> Option<u32> {
                    if let Some(sym_id) = decl_binder.get_node_symbol(node_idx) {
                        return Some(sym_id.0);
                    }
                    if let Some(sym_id) = resolve_in_scope(decl_binder, arena_ref, node_idx) {
                        return Some(sym_id);
                    }
                    let ident_name = arena_ref.get_identifier_text(node_idx)?;
                    if is_compiler_managed_type(ident_name) {
                        return None;
                    }
                    if let Some(found_sym) = decl_binder.file_locals.get(ident_name) {
                        return Some(found_sym.0);
                    }
                    for ctx in &lib_contexts {
                        if let Some(found_sym) = ctx.binder.file_locals.get(ident_name) {
                            return Some(found_sym.0);
                        }
                    }
                    None
                };
                let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::DefId> {
                    if let Some(sym_id) = decl_binder.get_node_symbol(node_idx) {
                        return Some(
                            self.ctx
                                .get_or_create_def_id(tsz_binder::SymbolId(sym_id.0)),
                        );
                    }
                    if let Some(sym_id) = resolve_in_scope(decl_binder, arena_ref, node_idx) {
                        return Some(self.ctx.get_or_create_def_id(tsz_binder::SymbolId(sym_id)));
                    }
                    let ident_name = arena_ref.get_identifier_text(node_idx)?;
                    if is_compiler_managed_type(ident_name) {
                        return None;
                    }
                    let sym_id = decl_binder.file_locals.get(ident_name).or_else(|| {
                        if let Some(all_binders) = self.ctx.all_binders.as_ref() {
                            for binder in all_binders.iter() {
                                if let Some(found_sym) = binder.file_locals.get(ident_name) {
                                    return Some(found_sym);
                                }
                            }
                        }
                        lib_contexts
                            .iter()
                            .find_map(|ctx| ctx.binder.file_locals.get(ident_name))
                    })?;
                    Some(
                        self.ctx
                            .get_or_create_def_id(tsz_binder::SymbolId(sym_id.0)),
                    )
                };
                let lowering = TypeLowering::with_hybrid_resolver(
                    arena_ref,
                    self.ctx.types,
                    &resolver,
                    &def_id_resolver,
                    &|_| None,
                );
                let aug_type = lowering.lower_interface_declarations(decls);
                lib_type_id = if let Some(lib_type) = lib_type_id {
                    Some(factory.intersection(vec![lib_type, aug_type]))
                } else {
                    Some(aug_type)
                };
            };

            // Lower current-file augmentations
            if !current_file_decls.is_empty() {
                lower_with_arena(current_arena, &current_file_decls);
            }

            // Lower cross-file augmentations (each group uses its own arena)
            for (arena, decls) in cross_file_groups.values() {
                lower_with_arena(arena.as_ref(), decls);
            }
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
            // Look up the exported symbol in module_exports
            if let Some(exports) = self.ctx.binder.module_exports.get(module_name)
                && let Some(target_sym_id) = exports.get(export_name)
            {
                // Recursively resolve if the target is also an alias
                return self.resolve_alias_symbol(target_sym_id, visited_aliases);
            }
            if let Some(binders) = &self.ctx.all_binders {
                for binder in binders.iter() {
                    if let Some(exports) = binder.module_exports.get(module_name)
                        && let Some(target_sym_id) = exports.get(export_name)
                    {
                        return self.resolve_alias_symbol(target_sym_id, visited_aliases);
                    }
                }
            }
            // For ES6 imports, if we can't find the export, return the alias symbol itself
            // This allows the type checker to use the symbol reference
            return Some(sym_id);
        }

        let decl_idx = if !symbol.value_declaration.is_none() {
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
        use tsz_solver::type_queries_extended::{NamespaceMemberKind, classify_namespace_member};

        match classify_namespace_member(self.ctx.types, object_type) {
            // Handle Lazy types (direct namespace/module references)
            NamespaceMemberKind::Lazy(def_id) => {
                let sym_id = self.ctx.def_to_symbol_id(def_id)?;
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

                if let Some(member_id) = direct_member_id {
                    // Record member as cross-file so get_type_of_symbol delegates correctly
                    let cross_file_idx = self
                        .ctx
                        .cross_file_symbol_targets
                        .borrow()
                        .get(&sym_id)
                        .copied();
                    if let Some(file_idx) = cross_file_idx {
                        self.ctx
                            .cross_file_symbol_targets
                            .borrow_mut()
                            .insert(member_id, file_idx);
                    }

                    // Follow re-export chains to get the actual symbol
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
                        return None;
                    }

                    if let Some(member_symbol) = self.get_cross_file_symbol(resolved_member_id)
                        // Namespace export tables may point at EXPORT_VALUE wrapper symbols
                        // (e.g. `export { x }`). Treat them as runtime-value members.
                        && member_symbol.flags & symbol_flags::VALUE == 0
                        && member_symbol.flags & symbol_flags::ALIAS == 0
                        && member_symbol.flags & symbol_flags::EXPORT_VALUE == 0
                    {
                        return None;
                    }
                    return Some(self.get_type_of_symbol(resolved_member_id));
                }

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

                if let Some(member_id) = module_export_member_id {
                    // Record member as cross-file so get_type_of_symbol delegates correctly
                    let cross_file_idx = self
                        .ctx
                        .cross_file_symbol_targets
                        .borrow()
                        .get(&sym_id)
                        .copied();
                    if let Some(file_idx) = cross_file_idx {
                        self.ctx
                            .cross_file_symbol_targets
                            .borrow_mut()
                            .insert(member_id, file_idx);
                    }

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
                        return None;
                    }

                    if let Some(member_symbol) = self.get_cross_file_symbol(resolved_member_id)
                        && member_symbol.flags & symbol_flags::VALUE == 0
                        && member_symbol.flags & symbol_flags::ALIAS == 0
                        && member_symbol.flags & symbol_flags::EXPORT_VALUE == 0
                    {
                        return None;
                    }

                    return Some(self.get_type_of_symbol(resolved_member_id));
                }

                // Check for re-exports from other modules
                // This handles cases like: export { foo } from './bar'
                if let Some(ref module_specifier) = symbol.import_module {
                    let mut visited_aliases = Vec::new();
                    if let Some(reexported_sym) = self.resolve_reexported_member_symbol(
                        module_specifier,
                        property_name,
                        &mut visited_aliases,
                    ) {
                        if self.symbol_member_is_type_only(reexported_sym, Some(property_name)) {
                            return None;
                        }

                        if let Some(member_symbol) = self.get_cross_file_symbol(reexported_sym)
                            && member_symbol.flags & symbol_flags::VALUE == 0
                            && member_symbol.flags & symbol_flags::ALIAS == 0
                            && member_symbol.flags & symbol_flags::EXPORT_VALUE == 0
                        {
                            return None;
                        }
                        return Some(self.get_type_of_symbol(reexported_sym));
                    }
                }

                if symbol.flags & symbol_flags::ENUM != 0
                    && let Some(member_type) = self.enum_member_type_for_name(sym_id, property_name)
                {
                    return Some(member_type);
                }

                None
            }

            // Handle ModuleNamespace types (import * as ns / namespace value bindings)
            NamespaceMemberKind::ModuleNamespace(sym_ref) => {
                let sym_id = SymbolId(sym_ref.0);
                let symbol = self.get_cross_file_symbol(sym_id)?;
                if symbol.flags & (symbol_flags::MODULE | symbol_flags::ENUM) == 0 {
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

                if let Some(member_id) = direct_member_id {
                    let cross_file_idx = self
                        .ctx
                        .cross_file_symbol_targets
                        .borrow()
                        .get(&sym_id)
                        .copied();
                    if let Some(file_idx) = cross_file_idx {
                        self.ctx
                            .cross_file_symbol_targets
                            .borrow_mut()
                            .insert(member_id, file_idx);
                    }

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
                        return None;
                    }

                    if let Some(member_symbol) = self.get_cross_file_symbol(resolved_member_id)
                        && member_symbol.flags & symbol_flags::VALUE == 0
                        && member_symbol.flags & symbol_flags::ALIAS == 0
                        && member_symbol.flags & symbol_flags::EXPORT_VALUE == 0
                    {
                        return None;
                    }
                    return Some(self.get_type_of_symbol(resolved_member_id));
                }

                // Fallback for namespace symbols whose export table is stored by module name.
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

                if let Some(member_id) = module_export_member_id {
                    let cross_file_idx = self
                        .ctx
                        .cross_file_symbol_targets
                        .borrow()
                        .get(&sym_id)
                        .copied();
                    if let Some(file_idx) = cross_file_idx {
                        self.ctx
                            .cross_file_symbol_targets
                            .borrow_mut()
                            .insert(member_id, file_idx);
                    }

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
                        return None;
                    }

                    if let Some(member_symbol) = self.get_cross_file_symbol(resolved_member_id)
                        && member_symbol.flags & symbol_flags::VALUE == 0
                        && member_symbol.flags & symbol_flags::ALIAS == 0
                        && member_symbol.flags & symbol_flags::EXPORT_VALUE == 0
                    {
                        return None;
                    }

                    return Some(self.get_type_of_symbol(resolved_member_id));
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
                let sym_id = self.ctx.def_to_symbol.borrow().get(&def_id).copied();
                let sym_id = sym_id?;

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
                    // Record member as cross-file so get_type_of_symbol delegates correctly
                    let cross_file_idx = self
                        .ctx
                        .cross_file_symbol_targets
                        .borrow()
                        .get(&sym_id)
                        .copied();
                    if let Some(file_idx) = cross_file_idx {
                        self.ctx
                            .cross_file_symbol_targets
                            .borrow_mut()
                            .insert(member_id, file_idx);
                    }
                    return Some(self.get_type_of_symbol(member_id));
                }

                // Fallback to enum_member_type_for_name
                self.enum_member_type_for_name(sym_id, property_name)
            }

            NamespaceMemberKind::Other => None,
        }
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

        let decl_idx = if !symbol.value_declaration.is_none() {
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
    ///
    /// ## Type-Only Members:
    /// - Interface declarations: `export interface Foo {}`
    /// - Type alias declarations: `export type Bar = number;`
    /// - Class declarations (when used as types): `export class Baz {}`
    ///
    /// ## Value Members:
    /// - Function declarations: `export function foo() {}`
    /// - Variable declarations: `export const x = 1;`
    /// - Enum declarations: `export enum E {}`
    ///
    /// ## Examples:
    /// ```typescript
    /// namespace Types {
    ///   export interface Foo {} // type-only
    ///   export type Bar = number; // type-only
    ///   export function helper() {} // value member
    /// }
    /// // namespace_has_type_only_member(Types, "Foo") → true
    /// // namespace_has_type_only_member(Types, "helper") → false
    /// ```
    pub(crate) fn namespace_has_type_only_member(
        &self,
        object_type: TypeId,
        property_name: &str,
    ) -> bool {
        use tsz_solver::type_queries_extended::{NamespaceMemberKind, classify_namespace_member};

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
    /// This function follows alias chains to determine if the ultimate
    /// target is type-only (has TYPE flag but not VALUE flag).
    ///
    /// ## Type-Only Imports:
    /// - `import type { Foo } from 'module'` - Foo is type-only
    /// - `import type { Bar } from './types'` - Bar is type-only
    ///
    /// ## Alias Resolution:
    /// - Follows re-export chains
    /// - Checks the ultimate target's flags
    /// - Respects `is_type_only` flag on alias symbols
    ///
    /// ## Examples:
    /// ```typescript
    /// // types.ts
    /// export interface Foo {}
    /// export const bar: number = 42;
    ///
    /// // main.ts
    /// import type { Foo } from './types'; // type-only import
    /// import { bar } from './types'; // value import
    ///
    /// // alias_resolves_to_type_only(Foo) → true
    /// // alias_resolves_to_type_only(bar) → false
    /// ```
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

        let target_symbol = match self.ctx.binder.get_symbol_with_libs(target, &lib_binders) {
            Some(target_symbol) => target_symbol,
            None => return false,
        };

        let has_value = (target_symbol.flags & symbol_flags::VALUE) != 0;
        let has_type = (target_symbol.flags & symbol_flags::TYPE) != 0;
        has_type && !has_value
    }

    fn symbol_member_is_type_only(&self, sym_id: SymbolId, name_hint: Option<&str>) -> bool {
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
        // Phase 4.2: Use resolve_type_to_symbol_id instead of get_ref_symbol
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
        use tsz_solver::type_queries_extended::{NamespaceMemberKind, classify_namespace_member};

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
        use tsz_solver::type_queries_extended::{NamespaceMemberKind, classify_namespace_member};

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
    ///
    /// ## Import Type Statement:
    /// - `import type { Foo } from 'module'` - `Foo.is_type_only` = true
    /// - Type-only imports can only be used in type annotations
    /// - Cannot be used as values (variables, function arguments, etc.)
    ///
    /// ## Examples:
    /// ```typescript
    /// import type { Foo } from './types'; // type-only import
    /// import { Bar } from './types'; // regular import
    ///
    /// const x: Foo = ...; // OK - Foo used in type position
    /// const y = Foo; // ERROR - Foo cannot be used as value
    ///
    /// const z: Bar = ...; // OK - Bar has both type and value
    /// const w = Bar; // OK - Bar can be used as value
    /// ```
    pub(crate) fn symbol_is_type_only(&self, sym_id: SymbolId, name_hint: Option<&str>) -> bool {
        self.lookup_symbol_with_name(sym_id, name_hint)
            .is_some_and(|(symbol, _arena)| symbol.is_type_only)
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
            _ => None,
        }
    }
}
