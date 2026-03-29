//! DefId Resolution Helpers for `TypeNodeChecker`
//!
//! Extracted from `type_node.rs` to keep that file under the LOC limit.
//! Contains methods for ensuring type alias bodies are registered in the
//! type environment and for resolving `DefIds` from qualified names.

use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_solver::TypeId;
use tsz_solver::is_compiler_managed_type;

use super::type_node::TypeNodeChecker;

impl<'a, 'ctx> TypeNodeChecker<'a, 'ctx> {
    pub(super) fn entity_name_text(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.clone());
        }

        if node.kind == tsz_parser::parser::syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.ctx.arena.get_qualified_name(node)?;
            let left = self.entity_name_text(qn.left)?;
            let right = self.entity_name_text(qn.right)?;
            let mut combined = String::with_capacity(left.len() + 1 + right.len());
            combined.push_str(&left);
            combined.push('.');
            combined.push_str(&right);
            return Some(combined);
        }

        None
    }

    pub(super) fn resolve_entity_name_text_symbol(
        &self,
        name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        let mut segments = name.split('.');
        let root_name = segments.next()?;
        let lib_binders: Vec<_> = self
            .ctx
            .lib_contexts
            .iter()
            .map(|ctx| std::sync::Arc::clone(&ctx.binder))
            .collect();
        let mut current_sym = self.ctx.binder.file_locals.get(root_name).or_else(|| {
            self.ctx
                .lib_contexts
                .iter()
                .find_map(|ctx| ctx.binder.file_locals.get(root_name))
        })?;

        for segment in segments {
            let symbol = self
                .ctx
                .binder
                .get_symbol_with_libs(current_sym, &lib_binders)?;
            current_sym = symbol
                .exports
                .as_ref()
                .and_then(|exports| exports.get(segment))
                .or_else(|| {
                    symbol
                        .members
                        .as_ref()
                        .and_then(|members| members.get(segment))
                })
                .or_else(|| {
                    // TYPE_ALIAS+ALIAS merge: follow alias_partner and resolve
                    // the member through the ALIAS symbol's import chain
                    let alias_id = self
                        .ctx
                        .binder
                        .alias_partners
                        .get(&current_sym)
                        .copied()
                        .or_else(|| {
                            let resolved = self.ctx.binder.resolve_import_symbol(current_sym)?;
                            self.ctx.binder.alias_partners.get(&resolved).copied()
                        })?;
                    let alias_sym = self
                        .ctx
                        .binder
                        .get_symbol_with_libs(alias_id, &lib_binders)?;
                    // Look up member in the ALIAS's exports/re-exports
                    alias_sym
                        .exports
                        .as_ref()
                        .and_then(|exports| exports.get(segment))
                        .or_else(|| {
                            // Follow the ALIAS's import_module, resolving from the
                            // ALIAS's source file perspective (cross-file), then
                            // falling back to the merged binder (same-file).
                            let module = alias_sym.import_module.as_ref()?;
                            self.ctx
                                .resolve_alias_import_member(alias_id, module, segment)
                                .or_else(|| {
                                    self.ctx
                                        .binder
                                        .resolve_import_with_reexports_type_only(module, segment)
                                        .map(|(sym_id, _)| sym_id)
                                })
                        })
                })?;
        }

        Some(current_sym)
    }

    fn resolve_entity_name_text_symbol_in_binder(
        &self,
        binder: &tsz_binder::BinderState,
        name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        let mut segments = name.split('.');
        let root_name = segments.next()?;
        let mut current_sym = binder
            .file_locals
            .get(root_name)
            .or_else(|| self.ctx.binder.file_locals.get(root_name))
            .or_else(|| {
                self.ctx
                    .lib_contexts
                    .iter()
                    .find_map(|ctx| ctx.binder.file_locals.get(root_name))
            })?;

        for segment in segments {
            let symbol = binder
                .get_symbol(current_sym)
                .or_else(|| self.ctx.binder.get_symbol(current_sym))
                .or_else(|| {
                    self.ctx
                        .lib_contexts
                        .iter()
                        .find_map(|ctx| ctx.binder.get_symbol(current_sym))
                })
                .or_else(|| {
                    let resolved = binder
                        .resolve_import_symbol(current_sym)
                        .or_else(|| self.ctx.binder.resolve_import_symbol(current_sym))?;
                    binder
                        .get_symbol(resolved)
                        .or_else(|| self.ctx.binder.get_symbol(resolved))
                        .or_else(|| {
                            self.ctx
                                .lib_contexts
                                .iter()
                                .find_map(|ctx| ctx.binder.get_symbol(resolved))
                        })
                })?;

            current_sym = symbol
                .exports
                .as_ref()
                .and_then(|exports| exports.get(segment))
                .or_else(|| {
                    symbol
                        .members
                        .as_ref()
                        .and_then(|members| members.get(segment))
                })?;
        }

        Some(current_sym)
    }

    fn resolve_entity_name_text_def_id(
        &self,
        current_sym_id: tsz_binder::SymbolId,
        current_def_id: tsz_solver::def::DefId,
        name: &str,
    ) -> Option<tsz_solver::def::DefId> {
        let sym_id = self.resolve_entity_name_text_symbol(name)?;
        let def_id = self.ctx.get_or_create_def_id(sym_id);
        if sym_id != current_sym_id && def_id != current_def_id {
            self.ensure_type_alias_resolved(sym_id, def_id);
        }
        Some(def_id)
    }

    fn find_type_alias_declaration(
        &self,
        sym_id: tsz_binder::SymbolId,
        symbol: &tsz_binder::Symbol,
    ) -> Option<(NodeIndex, &NodeArena)> {
        use tsz_parser::parser::syntax_kind_ext;

        let mut decl_candidates = symbol.declarations.clone();
        if symbol.value_declaration.is_some()
            && !decl_candidates.contains(&symbol.value_declaration)
        {
            decl_candidates.push(symbol.value_declaration);
        }

        for decl_idx in decl_candidates {
            if decl_idx.is_none() {
                continue;
            }

            let mut candidate_arenas: Vec<&NodeArena> = Vec::new();
            if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                candidate_arenas.extend(arenas.iter().map(std::convert::AsRef::as_ref));
            }
            if let Some(symbol_arena) = self.ctx.binder.symbol_arenas.get(&sym_id) {
                candidate_arenas.push(symbol_arena.as_ref());
            }
            candidate_arenas.push(self.ctx.arena);

            for arena in candidate_arenas {
                let Some(node) = arena.get(decl_idx) else {
                    continue;
                };
                if node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                    continue;
                }
                let Some(type_alias) = arena.get_type_alias(node) else {
                    continue;
                };
                let Some(name) = arena.get_identifier_text(type_alias.name) else {
                    continue;
                };
                if name == symbol.escaped_name.as_str() {
                    return Some((decl_idx, arena));
                }
            }
        }

        None
    }

    fn declaration_namespace_prefix(
        &self,
        arena: &NodeArena,
        node_idx: NodeIndex,
    ) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;

        let mut parent = arena
            .get_extended(node_idx)
            .map_or(NodeIndex::NONE, |info| info.parent);
        let mut prefixes = Vec::new();

        while !parent.is_none() {
            let parent_node = arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module) = arena.get_module(parent_node)
                && let Some(name_node) = arena.get(module.name)
                && name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                && let Some(name_ident) = arena.get_identifier(name_node)
            {
                prefixes.push(name_ident.escaped_text.clone());
            }

            parent = arena
                .get_extended(parent)
                .map_or(NodeIndex::NONE, |info| info.parent);
        }

        if prefixes.is_empty() {
            None
        } else {
            Some(prefixes.into_iter().rev().collect::<Vec<_>>().join("."))
        }
    }

    fn precompute_computed_property_names_in_arena<F>(
        &self,
        arena: &NodeArena,
        root: NodeIndex,
        resolve_text_symbol: &F,
    ) -> rustc_hash::FxHashMap<NodeIndex, tsz_common::Atom>
    where
        F: Fn(&str) -> Option<tsz_binder::SymbolId>,
    {
        let mut map = rustc_hash::FxHashMap::default();
        let mut stack = vec![root];

        while let Some(node_idx) = stack.pop() {
            let Some(node) = arena.get(node_idx) else {
                continue;
            };

            if node.kind == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME
                && let Some(name) = self.resolve_computed_property_name_in_arena(
                    arena,
                    node_idx,
                    resolve_text_symbol,
                )
                && let Some(computed) = arena.get_computed_property(node)
            {
                map.insert(computed.expression, self.ctx.types.intern_string(&name));
            }

            stack.extend(arena.get_children(node_idx));
        }

        map
    }

    fn resolve_computed_property_name_in_arena<F>(
        &self,
        arena: &NodeArena,
        name_idx: NodeIndex,
        resolve_text_symbol: &F,
    ) -> Option<String>
    where
        F: Fn(&str) -> Option<tsz_binder::SymbolId>,
    {
        if let Some(name) =
            crate::types_domain::queries::core::get_literal_property_name(arena, name_idx)
        {
            return Some(name);
        }

        let name_node = arena.get(name_idx)?;
        if name_node.kind != tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }

        let computed = arena.get_computed_property(name_node)?;
        if let Some(symbol_name) =
            self.well_known_symbol_property_name_in_arena(arena, computed.expression)
        {
            return Some(symbol_name);
        }

        let sym_id = self.resolve_computed_property_symbol_in_arena(
            arena,
            computed.expression,
            resolve_text_symbol,
        )?;
        self.symbol_refers_to_unique_symbol_anywhere(sym_id)
            .then(|| format!("__unique_{}", sym_id.0))
    }

    fn well_known_symbol_property_name_in_arena(
        &self,
        arena: &NodeArena,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let node = arena.get(expr_idx)?;

        if node.kind == tsz_parser::parser::syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            let paren = arena.get_parenthesized(node)?;
            return self.well_known_symbol_property_name_in_arena(arena, paren.expression);
        }

        if node.kind != tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && node.kind != tsz_parser::parser::syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return None;
        }

        let access = arena.get_access_expr(node)?;
        let base_node = arena.get(access.expression)?;
        let base_ident = arena.get_identifier(base_node)?;
        if base_ident.escaped_text != "Symbol" {
            return None;
        }

        let name_node = arena.get(access.name_or_argument)?;
        if let Some(ident) = arena.get_identifier(name_node) {
            return Some(format!("[Symbol.{}]", ident.escaped_text));
        }

        if matches!(
            name_node.kind,
            k if k == tsz_scanner::SyntaxKind::StringLiteral as u16
                || k == tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16
        ) && let Some(lit) = arena.get_literal(name_node)
            && !lit.text.is_empty()
        {
            return Some(format!("[Symbol.{}]", lit.text));
        }

        None
    }

    fn resolve_computed_property_symbol_in_arena<F>(
        &self,
        arena: &NodeArena,
        expr_idx: NodeIndex,
        resolve_text_symbol: &F,
    ) -> Option<tsz_binder::SymbolId>
    where
        F: Fn(&str) -> Option<tsz_binder::SymbolId>,
    {
        let node = arena.get(expr_idx)?;

        if node.kind == tsz_parser::parser::syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            let paren = arena.get_parenthesized(node)?;
            return self.resolve_computed_property_symbol_in_arena(
                arena,
                paren.expression,
                resolve_text_symbol,
            );
        }

        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let ident = arena.get_identifier(node)?;
            return resolve_text_symbol(&ident.escaped_text);
        }

        let qualified = self.expression_name_text_in_arena(arena, expr_idx)?;
        resolve_text_symbol(&qualified)
    }

    fn expression_name_text_in_arena(&self, arena: &NodeArena, idx: NodeIndex) -> Option<String> {
        let node = arena.get(idx)?;

        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            return arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.clone());
        }

        if node.kind == tsz_parser::parser::syntax_kind_ext::QUALIFIED_NAME {
            let qn = arena.get_qualified_name(node)?;
            let left = self.expression_name_text_in_arena(arena, qn.left)?;
            let right = self.expression_name_text_in_arena(arena, qn.right)?;
            return Some(format!("{left}.{right}"));
        }

        if node.kind == tsz_parser::parser::syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            let paren = arena.get_parenthesized(node)?;
            return self.expression_name_text_in_arena(arena, paren.expression);
        }

        if node.kind == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = arena.get_access_expr(node)
        {
            let left = self.expression_name_text_in_arena(arena, access.expression)?;
            let right_node = arena.get(access.name_or_argument)?;
            let right = arena.get_identifier(right_node)?;
            return Some(format!("{left}.{}", right.escaped_text));
        }

        None
    }

    fn symbol_refers_to_unique_symbol_anywhere(&self, sym_id: tsz_binder::SymbolId) -> bool {
        let Some(symbol) = self.get_symbol_from_any_context(sym_id) else {
            return false;
        };
        let file_idx = symbol.decl_file_idx;
        let owner_binder = self
            .ctx
            .get_binder_for_file(file_idx as usize)
            .unwrap_or(self.ctx.binder);

        let mut decl_candidates = symbol.declarations.clone();
        if symbol.value_declaration.is_some()
            && !decl_candidates.contains(&symbol.value_declaration)
        {
            decl_candidates.push(symbol.value_declaration);
        }

        decl_candidates.into_iter().any(|decl_idx| {
            let mut candidate_arenas: Vec<&NodeArena> = Vec::new();
            if let Some(arenas) = owner_binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                candidate_arenas.extend(arenas.iter().map(std::convert::AsRef::as_ref));
            }
            if let Some(symbol_arena) = owner_binder.symbol_arenas.get(&sym_id) {
                candidate_arenas.push(symbol_arena.as_ref());
            }
            if std::ptr::eq(owner_binder, self.ctx.binder) {
                candidate_arenas.push(self.ctx.arena);
            }

            candidate_arenas
                .into_iter()
                .any(|arena| self.declaration_is_unique_symbol_in_arena(arena, decl_idx))
        })
    }

    fn declaration_is_unique_symbol_in_arena(
        &self,
        arena: &NodeArena,
        decl_idx: NodeIndex,
    ) -> bool {
        let Some(node) = arena.get(decl_idx) else {
            return false;
        };
        if node.kind != tsz_parser::parser::syntax_kind_ext::VARIABLE_DECLARATION {
            return false;
        }

        let Some(var_decl) = arena.get_variable_declaration(node) else {
            return false;
        };

        (var_decl.type_annotation.is_some()
            && self.is_unique_symbol_type_annotation_in_resolution_arena(
                arena,
                var_decl.type_annotation,
            ))
            || self.is_symbol_call_initializer_in_resolution_arena(arena, var_decl.initializer)
    }

    fn is_unique_symbol_type_annotation_in_resolution_arena(
        &self,
        arena: &NodeArena,
        type_annotation: NodeIndex,
    ) -> bool {
        let Some(type_node) = arena.get(type_annotation) else {
            return false;
        };

        match type_node.kind {
            k if k == tsz_parser::parser::syntax_kind_ext::TYPE_OPERATOR => {
                arena.get_type_operator(type_node).is_some_and(|op| {
                    op.operator == tsz_scanner::SyntaxKind::UniqueKeyword as u16
                        && self.is_symbol_type_node_in_resolution_arena(arena, op.type_node)
                })
            }
            _ => false,
        }
    }

    fn is_symbol_type_node_in_resolution_arena(
        &self,
        arena: &NodeArena,
        type_annotation: NodeIndex,
    ) -> bool {
        let Some(type_node) = arena.get(type_annotation) else {
            return false;
        };
        if type_node.kind != tsz_parser::parser::syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }

        let Some(type_ref) = arena.get_type_ref(type_node) else {
            return false;
        };
        let Some(name_node) = arena.get(type_ref.type_name) else {
            return false;
        };

        arena
            .get_identifier(name_node)
            .is_some_and(|ident| ident.escaped_text == "symbol")
    }

    fn is_symbol_call_initializer_in_resolution_arena(
        &self,
        arena: &NodeArena,
        init_idx: NodeIndex,
    ) -> bool {
        let Some(node) = arena.get(init_idx) else {
            return false;
        };
        if node.kind != tsz_parser::parser::syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }

        let Some(call) = arena.get_call_expr(node) else {
            return false;
        };
        let Some(expr_node) = arena.get(call.expression) else {
            return false;
        };

        arena
            .get_identifier(expr_node)
            .is_some_and(|ident| ident.escaped_text == "Symbol")
    }

    fn get_symbol_from_any_context(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<&tsz_binder::Symbol> {
        self.ctx
            .binder
            .get_symbol(sym_id)
            .or_else(|| {
                // O(1) fast-path via resolve_symbol_file_index
                let file_idx = self.ctx.resolve_symbol_file_index(sym_id);
                if let Some(file_idx) = file_idx
                    && let Some(binder) = self.ctx.get_binder_for_file(file_idx)
                    && let Some(sym) = binder.get_symbol(sym_id)
                {
                    return Some(sym);
                }
                self.ctx
                    .all_binders
                    .as_ref()
                    .and_then(|binders| binders.iter().find_map(|binder| binder.get_symbol(sym_id)))
            })
            .or_else(|| {
                self.ctx
                    .lib_contexts
                    .iter()
                    .find_map(|ctx| ctx.binder.get_symbol(sym_id))
            })
    }

    /// Get or create a `DefId` for a symbol and ensure its type alias body
    /// is registered in the type environment.
    ///
    /// This is the canonical stable-identity helper that consolidates the
    /// repetitive two-step pattern:
    ///   1. `ctx.get_or_create_def_id(sym_id)` — mint/retrieve DefId
    ///   2. `ensure_type_alias_resolved(sym_id, def_id)` — register body+params
    ///
    /// Used in qualified name resolution paths where every member lookup
    /// needs stable identity with alias body registration.
    pub(crate) fn ensure_def_id_with_alias(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> tsz_solver::def::DefId {
        let def_id = self.ctx.get_or_create_def_id(sym_id);
        self.ensure_type_alias_resolved(sym_id, def_id);
        def_id
    }

    /// Ensure a type alias symbol has its type params and body registered
    /// so the solver can expand Application(Lazy(DefId), Args) later.
    ///
    /// This is needed because `TypeLowering` creates Application types without
    /// calling `get_type_of_symbol`, so type aliases referenced only inside
    /// lowered type expressions (mapped type templates, etc.) may not have
    /// their type params or body registered in the resolver caches.
    pub(crate) fn ensure_type_alias_resolved(
        &self,
        sym_id: tsz_binder::SymbolId,
        def_id: tsz_solver::def::DefId,
    ) {
        use tsz_binder::symbol_flags;

        if let Ok(env) = self.ctx.type_env.try_borrow()
            && env.get_def(def_id).is_some()
        {
            return;
        }

        // If already resolved via get_type_of_symbol, ensure the TypeEnvironment
        // has the DefId-keyed entry. This handles a timing issue: register_resolved_type
        // may have been called before the DefId was created (DefId is created during
        // type lowering of references, which happens after type alias resolution).
        if self.ctx.symbol_types.contains_key(&sym_id) {
            if let Ok(env) = self.ctx.type_env.try_borrow()
                && env.get_def(def_id).is_none()
            {
                drop(env);
                // Body not registered for this DefId — register it now
                if let Some(&type_id) = self.ctx.symbol_types.get(&sym_id) {
                    let type_params = self.ctx.get_def_type_params(def_id).unwrap_or_default();
                    if type_params.is_empty() {
                        self.ctx.register_def_in_envs(def_id, type_id);
                    } else {
                        self.ctx
                            .register_def_with_params_in_envs(def_id, type_id, type_params);
                    }
                    // Register symbol mapping in both envs
                    if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
                        env.register_def_symbol_mapping(def_id, sym_id);
                    }
                    if let Ok(mut env) = self.ctx.type_environment.try_borrow_mut() {
                        env.register_def_symbol_mapping(def_id, sym_id);
                    }
                }
            }
            return;
        }

        let symbol = self.get_symbol_from_any_context(sym_id);
        let Some(symbol) = symbol else {
            return;
        };
        if symbol.flags & symbol_flags::TYPE_ALIAS == 0 {
            return;
        }

        let Some((decl_idx, decl_arena)) = self.find_type_alias_declaration(sym_id, symbol) else {
            return;
        };
        let Some(node) = decl_arena.get(decl_idx) else {
            return;
        };
        let Some(type_alias) = decl_arena.get_type_alias(node) else {
            return;
        };

        // Extract type parameters from AST and create TypeParam TypeIds
        let factory = self.ctx.types.factory();
        let mut params = Vec::new();
        let mut bindings = Vec::new();

        if let Some(ref type_param_list) = type_alias.type_parameters {
            for &param_idx in &type_param_list.nodes {
                let Some(param_node) = decl_arena.get(param_idx) else {
                    continue;
                };
                let Some(param_data) = decl_arena.get_type_parameter(param_node) else {
                    continue;
                };

                let name = decl_arena
                    .get(param_data.name)
                    .and_then(|n| decl_arena.get_identifier(n))
                    .map_or_else(|| "T".to_string(), |id| id.escaped_text.clone());

                let atom = self.ctx.types.intern_string(&name);
                let info = tsz_solver::TypeParamInfo {
                    name: atom,
                    constraint: None,
                    default: None,
                    is_const: false,
                };
                let type_id = factory.type_param(info);
                bindings.push((atom, type_id));
                params.push(info);
            }
        }

        if !params.is_empty() {
            self.ctx.insert_def_type_params(def_id, params.clone());
        }

        // Lower the type alias body with the type params in scope
        if type_alias.type_node != NodeIndex::NONE {
            let namespace_prefix = self.declaration_namespace_prefix(decl_arena, decl_idx);
            let decl_binder = self
                .ctx
                .get_binder_for_arena(decl_arena)
                .unwrap_or(self.ctx.binder);
            let resolve_text_symbol = |name: &str| -> Option<tsz_binder::SymbolId> {
                namespace_prefix
                    .as_ref()
                    .and_then(|prefix| {
                        let mut scoped = String::with_capacity(prefix.len() + 1 + name.len());
                        scoped.push_str(prefix);
                        scoped.push('.');
                        scoped.push_str(name);
                        self.resolve_entity_name_text_symbol_in_binder(decl_binder, &scoped)
                    })
                    .or_else(|| self.resolve_entity_name_text_symbol_in_binder(decl_binder, name))
            };
            let type_resolver = |n: NodeIndex| -> Option<u32> {
                if std::ptr::eq(decl_arena, self.ctx.arena) {
                    return self.resolve_type_symbol(n);
                }
                let ident_name = decl_arena.get_identifier_text(n)?;
                if is_compiler_managed_type(ident_name) {
                    return None;
                }
                resolve_text_symbol(ident_name).map(|sym| sym.0)
            };
            let def_id_resolver = |n: NodeIndex| -> Option<tsz_solver::def::DefId> {
                let referenced_sym_id = if std::ptr::eq(decl_arena, self.ctx.arena) {
                    tsz_binder::SymbolId(self.resolve_type_symbol(n)?)
                } else {
                    let ident_name = decl_arena.get_identifier_text(n)?;
                    if is_compiler_managed_type(ident_name) {
                        return None;
                    }
                    resolve_text_symbol(ident_name)?
                };
                let resolved_def_id = self.ctx.get_or_create_def_id(referenced_sym_id);
                // Recursively ensure referenced type aliases have their body
                // and params registered in TypeEnvironment. Without this,
                // type aliases only referenced inside other type alias bodies
                // (e.g., Func inside Spec) would have DefIds created but no
                // body registered, causing resolve_lazy to fail during evaluation.
                if referenced_sym_id != sym_id && resolved_def_id != def_id {
                    self.ensure_type_alias_resolved(referenced_sym_id, resolved_def_id);
                }
                Some(resolved_def_id)
            };
            let value_resolver = |n: NodeIndex| -> Option<u32> {
                if std::ptr::eq(decl_arena, self.ctx.arena) {
                    return self.resolve_value_symbol_with_libs(n);
                }
                let ident_name = decl_arena.get_identifier_text(n)?;
                if is_compiler_managed_type(ident_name) {
                    return None;
                }
                let sym_id = resolve_text_symbol(ident_name)?;
                let symbol = self.ctx.binder.get_symbol(sym_id).or_else(|| {
                    self.ctx
                        .lib_contexts
                        .iter()
                        .find_map(|ctx| ctx.binder.get_symbol(sym_id))
                })?;
                ((symbol.flags
                    & (symbol_flags::VALUE
                        | symbol_flags::ALIAS
                        | symbol_flags::REGULAR_ENUM
                        | symbol_flags::CONST_ENUM))
                    != 0)
                    .then_some(sym_id.0)
            };
            let name_resolver = |name: &str| -> Option<tsz_solver::def::DefId> {
                if let Some(prefix) = namespace_prefix.as_ref() {
                    let mut scoped = String::with_capacity(prefix.len() + 1 + name.len());
                    scoped.push_str(prefix);
                    scoped.push('.');
                    scoped.push_str(name);
                    if let Some(resolved) =
                        self.resolve_entity_name_text_def_id(sym_id, def_id, &scoped)
                    {
                        return Some(resolved);
                    }
                }
                self.resolve_entity_name_text_def_id(sym_id, def_id, name)
            };
            let computed_names = self.precompute_computed_property_names_in_arena(
                decl_arena,
                type_alias.type_node,
                &resolve_text_symbol,
            );
            let computed_name_resolver =
                |expr_idx: NodeIndex| computed_names.get(&expr_idx).copied();

            // Provide flow-narrowed types for `typeof expr` in the type alias body.
            // These were pre-computed by `precompute_type_query_flow_types` during
            // `check_type_alias_declaration` and stored in `node_types`.
            let type_query_override = |expr_name_idx: NodeIndex| -> Option<TypeId> {
                self.ctx
                    .node_types
                    .get(&expr_name_idx.0)
                    .copied()
                    .filter(|&t| t != TypeId::ERROR)
            };
            let lowering = tsz_lowering::TypeLowering::with_hybrid_resolver(
                decl_arena,
                self.ctx.types,
                &type_resolver,
                &def_id_resolver,
                &value_resolver,
            )
            .with_type_param_bindings(bindings)
            .with_computed_name_resolver(&computed_name_resolver)
            .with_name_def_id_resolver(&name_resolver)
            .with_type_query_override(&type_query_override);

            let body = lowering.lower_type(type_alias.type_node);

            // Register body in both type environments so resolve_lazy
            // and flow-analysis narrowing can both find it
            if params.is_empty() {
                self.ctx.register_def_in_envs(def_id, body);
            } else {
                self.ctx
                    .register_def_with_params_in_envs(def_id, body, params);
            }
            if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
                env.register_def_symbol_mapping(def_id, sym_id);
            }
            if let Ok(mut env) = self.ctx.type_environment.try_borrow_mut() {
                env.register_def_symbol_mapping(def_id, sym_id);
            }
        }
    }

    /// Resolve a DefId with support for qualified names (e.g., `AnimalType.cat`).
    ///
    /// Used by the `compute_type` fallback path where template literal types may
    /// reference enum members via qualified names inside `${...}`.
    pub(crate) fn resolve_def_id_with_qualified_names(
        &self,
        node_idx: NodeIndex,
    ) -> Option<tsz_solver::def::DefId> {
        use tsz_parser::parser::syntax_kind_ext;

        if let Some(name) = self.entity_name_text(node_idx)
            && let Some(sym_id) = self.resolve_entity_name_text_symbol(&name)
        {
            return Some(self.ensure_def_id_with_alias(sym_id));
        }

        if let Some(sym_id) = self.resolve_type_symbol(node_idx) {
            let sym_id = tsz_binder::SymbolId(sym_id);
            return Some(self.ensure_def_id_with_alias(sym_id));
        }

        let node = self.ctx.arena.get(node_idx)?;
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.ctx.arena.get_qualified_name(node)?;
            let lib_binders: Vec<_> = self
                .ctx
                .lib_contexts
                .iter()
                .map(|ctx| std::sync::Arc::clone(&ctx.binder))
                .collect();
            // For the left part of a qualified name (e.g., `Lib` in `Lib.Base`),
            // we need to also consider ALIAS symbols because import declarations
            // like `import Lib = require('./helper')` create ALIAS-flagged symbols.
            // resolve_type_symbol only checks TYPE | ENUM flags, so try it first,
            // then fall back to resolve_type_or_alias_symbol for the namespace part.
            let left_sym_raw = self
                .resolve_type_symbol(qn.left)
                .or_else(|| self.resolve_type_or_alias_symbol(qn.left))?;
            let left_sym_id = tsz_binder::SymbolId(left_sym_raw);

            // If the left symbol is an import alias (e.g., `import Lib = require('./helper')`),
            // follow the import to the target module symbol which holds the actual exports.
            let resolved_sym_id = self
                .ctx
                .binder
                .resolve_import_symbol(left_sym_id)
                .unwrap_or(left_sym_id);
            let resolved_symbol = self
                .ctx
                .binder
                .get_symbol_with_libs(resolved_sym_id, &lib_binders)?;

            let right_node = self.ctx.arena.get(qn.right)?;
            let right_ident = self.ctx.arena.get_identifier(right_node)?;
            let right_name = right_ident.escaped_text.as_str();

            // Look up the member in the resolved symbol's exports
            if let Some(exports) = resolved_symbol.exports.as_ref()
                && let Some(member_sym_id) = exports.get(right_name)
            {
                return Some(self.ensure_def_id_with_alias(member_sym_id));
            }

            // TYPE_ALIAS+ALIAS merge: resolve member through ALIAS partner
            if let Some(&alias_id) = self.ctx.binder.alias_partners.get(&resolved_sym_id)
                && let Some(alias_sym) =
                    self.ctx.binder.get_symbol_with_libs(alias_id, &lib_binders)
            {
                // Check direct exports first
                if let Some(exports) = alias_sym.exports.as_ref()
                    && let Some(member_sym_id) = exports.get(right_name)
                {
                    return Some(self.ensure_def_id_with_alias(member_sym_id));
                }
                // Follow the ALIAS's import_module, resolving from the
                // ALIAS's source file perspective (cross-file), then
                // falling back to the merged binder (same-file).
                if let Some(module_name) = alias_sym.import_module.as_ref() {
                    let member = self
                        .ctx
                        .resolve_alias_import_member(alias_id, module_name, right_name)
                        .or_else(|| {
                            self.ctx
                                .binder
                                .resolve_import_with_reexports_type_only(module_name, right_name)
                                .map(|(sym_id, _)| sym_id)
                        });
                    if let Some(member_sym_id) = member {
                        return Some(self.ensure_def_id_with_alias(member_sym_id));
                    }
                }
            }

            // Also check lib contexts for the member (e.g., global namespace types)
            for lib_ctx in self.ctx.lib_contexts.iter() {
                if let Some(lib_resolved) = lib_ctx.binder.resolve_import_symbol(left_sym_id)
                    && let Some(lib_symbol) = lib_ctx.binder.get_symbol(lib_resolved)
                    && let Some(exports) = lib_symbol.exports.as_ref()
                    && let Some(member_sym_id) = exports.get(right_name)
                {
                    return Some(self.ctx.get_or_create_def_id(member_sym_id));
                }
            }
        }

        None
    }

    /// Resolve a type-or-alias-or-namespace symbol from a node index.
    ///
    /// Like `resolve_type_symbol` but also matches ALIAS and NAMESPACE-flagged
    /// symbols, needed for:
    /// - Import declarations used as namespace qualifiers
    ///   (e.g., `import Lib = require('./helper')` then `Lib.Type`)
    /// - Namespace declarations used as qualified name prefixes
    ///   (e.g., `declare namespace NS { class C {} }` then `NS.C`)
    fn resolve_type_or_alias_symbol(&self, node_idx: NodeIndex) -> Option<u32> {
        use tsz_binder::symbol_flags;

        let ident = self.ctx.arena.get_identifier_at(node_idx)?;
        let name = ident.escaped_text.as_str();

        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            if (symbol.flags
                & (symbol_flags::TYPE
                    | symbol_flags::ALIAS
                    | symbol_flags::REGULAR_ENUM
                    | symbol_flags::CONST_ENUM
                    | symbol_flags::VALUE_MODULE
                    | symbol_flags::NAMESPACE_MODULE))
                != 0
            {
                return Some(sym_id.0);
            }
        }

        for lib_ctx in self.ctx.lib_contexts.iter() {
            if let Some(lib_sym_id) = lib_ctx.binder.file_locals.get(name) {
                let symbol = lib_ctx.binder.get_symbol(lib_sym_id)?;
                if (symbol.flags
                    & (symbol_flags::TYPE
                        | symbol_flags::ALIAS
                        | symbol_flags::REGULAR_ENUM
                        | symbol_flags::CONST_ENUM
                        | symbol_flags::VALUE_MODULE
                        | symbol_flags::NAMESPACE_MODULE))
                    != 0
                {
                    let file_sym_id = self.ctx.binder.file_locals.get(name).unwrap_or(lib_sym_id);
                    return Some(file_sym_id.0);
                }
            }
        }

        None
    }
}
