//! Helpers for computing type aliases in `compute_type_of_symbol`.

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_lowering::TypeLowering;
use tsz_parser::parser::node::{NodeAccess, NodeArena, TypeAliasData};
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;
use tsz_solver::is_compiler_managed_type;

impl<'a> CheckerState<'a> {
    pub(crate) fn resolve_cross_arena_type_alias_body_with_checker(
        &mut self,
        decl_arena: &NodeArena,
        sym_id: SymbolId,
        type_alias: &TypeAliasData,
    ) -> Option<TypeId> {
        let delegate_binder = self
            .ctx
            .get_binder_for_arena(decl_arena)
            .unwrap_or(self.ctx.binder);
        let delegate_file_idx = if std::ptr::eq(decl_arena, self.ctx.arena) {
            Some(self.ctx.current_file_idx)
        } else {
            self.ctx.get_file_idx_for_arena(decl_arena)
        };
        let delegate_file_name = decl_arena
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
            .unwrap_or_else(|| self.ctx.file_name.clone());

        let mut checker = Box::new(CheckerState::with_parent_cache(
            decl_arena,
            delegate_binder,
            self.ctx.types,
            delegate_file_name,
            self.ctx.compiler_options.clone(),
            self,
        ));
        checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
        checker.ctx.copy_cross_file_state_from(&self.ctx);
        self.ctx.copy_symbol_file_targets_to(&mut checker.ctx);
        checker.ctx.current_file_idx = delegate_file_idx.unwrap_or(self.ctx.current_file_idx);
        for &id in &self.ctx.symbol_resolution_set {
            if id != sym_id {
                checker.ctx.symbol_resolution_set.insert(id);
            }
        }

        let (_, tp_updates) = checker.push_type_parameters(&type_alias.type_parameters);
        let alias_type = checker.get_type_from_type_node(type_alias.type_node);
        checker.pop_type_parameters(tp_updates);

        self.ctx.merge_symbol_file_targets_from(&checker.ctx);
        Some(alias_type)
    }

    pub(crate) fn lower_cross_arena_type_alias_declaration(
        &mut self,
        _sym_id: SymbolId,
        decl_idx: NodeIndex,
        decl_arena: &NodeArena,
        type_alias: &TypeAliasData,
    ) -> (TypeId, Vec<tsz_solver::TypeParamInfo>) {
        // Prime lib type params for type references without explicit type args
        // in the alias body. Without this, generic lib types like Uint8Array
        // (which has a default type parameter) produce bare Lazy(DefId) instead
        // of Application(Lazy(DefId), [defaults]), causing false TS2345/TS2322.
        self.prime_cross_arena_type_reference_params(decl_arena, type_alias.type_node);
        let binder = &self.ctx.binder;
        let lib_binders = self.get_lib_binders();
        let decl_binder = self
            .ctx
            .get_binder_for_arena(decl_arena)
            .unwrap_or(self.ctx.binder);
        let namespace_prefix = self.type_alias_namespace_prefix(decl_arena, decl_idx);
        let resolve_symbol_in_decl_binder = |name: &str| -> Option<SymbolId> {
            let mut segments = name.split('.');
            let root_name = segments.next()?;
            let mut current_sym = decl_binder.file_locals.get(root_name)?;

            for segment in segments {
                let symbol = decl_binder
                    .get_symbol(current_sym)
                    .or_else(|| self.get_cross_file_symbol(current_sym))
                    .or_else(|| binder.get_symbol_with_libs(current_sym, &lib_binders))
                    .or_else(|| {
                        let resolved = decl_binder.resolve_import_symbol(current_sym)?;
                        decl_binder
                            .get_symbol(resolved)
                            .or_else(|| self.get_cross_file_symbol(resolved))
                            .or_else(|| binder.get_symbol_with_libs(resolved, &lib_binders))
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
        };
        let resolve_type_name = |name: &str| -> Option<SymbolId> {
            namespace_prefix
                .as_ref()
                .and_then(|prefix| {
                    let mut scoped = String::with_capacity(prefix.len() + 1 + name.len());
                    scoped.push_str(prefix);
                    scoped.push('.');
                    scoped.push_str(name);
                    resolve_symbol_in_decl_binder(&scoped).or_else(|| {
                        self.resolve_entity_name_text_to_def_id_for_lowering(&scoped)
                            .and_then(|def_id| self.ctx.def_to_symbol_id_with_fallback(def_id))
                    })
                })
                .or_else(|| {
                    resolve_symbol_in_decl_binder(name).or_else(|| {
                        self.resolve_entity_name_text_to_def_id_for_lowering(name)
                            .and_then(|def_id| self.ctx.def_to_symbol_id_with_fallback(def_id))
                    })
                })
        };
        let type_resolver = |node_idx: NodeIndex| -> Option<u32> {
            let ident_name = decl_arena.get_identifier_text(node_idx)?;
            if is_compiler_managed_type(ident_name) {
                return None;
            }
            let referenced_sym_id = resolve_type_name(ident_name)?;
            let symbol = binder.get_symbol_with_libs(referenced_sym_id, &lib_binders)?;
            ((symbol.flags & symbol_flags::TYPE) != 0).then_some(referenced_sym_id.0)
        };
        let value_resolver = |node_idx: NodeIndex| -> Option<u32> {
            let ident_name = decl_arena.get_identifier_text(node_idx)?;
            if is_compiler_managed_type(ident_name) {
                return None;
            }
            let referenced_sym_id = resolve_type_name(ident_name)?;
            let symbol = binder.get_symbol_with_libs(referenced_sym_id, &lib_binders)?;
            ((symbol.flags
                & (symbol_flags::VALUE
                    | symbol_flags::ALIAS
                    | symbol_flags::REGULAR_ENUM
                    | symbol_flags::CONST_ENUM))
                != 0)
                .then_some(referenced_sym_id.0)
        };
        let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> {
            let ident_name = decl_arena.get_identifier_text(node_idx)?;
            if is_compiler_managed_type(ident_name) {
                return None;
            }
            let referenced_sym_id = resolve_type_name(ident_name)?;
            let symbol = binder.get_symbol_with_libs(referenced_sym_id, &lib_binders)?;
            ((symbol.flags & symbol_flags::TYPE) != 0)
                .then(|| self.ctx.get_or_create_def_id(referenced_sym_id))
        };
        let name_resolver = |type_name: &str| -> Option<tsz_solver::def::DefId> {
            namespace_prefix
                .as_ref()
                .and_then(|prefix| {
                    let mut scoped = String::with_capacity(prefix.len() + 1 + type_name.len());
                    scoped.push_str(prefix);
                    scoped.push('.');
                    scoped.push_str(type_name);
                    self.resolve_entity_name_text_to_def_id_for_lowering(&scoped)
                })
                .or_else(|| self.resolve_entity_name_text_to_def_id_for_lowering(type_name))
        };
        let computed_names = self.precompute_cross_arena_computed_property_names(
            decl_arena,
            type_alias.type_node,
            &resolve_type_name,
        );
        let computed_name_resolver = |expr_idx: NodeIndex| computed_names.get(&expr_idx).copied();
        let bindings = self.get_type_param_bindings();
        let lazy_type_params_resolver =
            |def_id: tsz_solver::def::DefId| self.ctx.get_def_type_params(def_id);
        let lowering = TypeLowering::with_hybrid_resolver(
            decl_arena,
            self.ctx.types,
            &type_resolver,
            &def_id_resolver,
            &value_resolver,
        )
        .with_type_param_bindings(bindings)
        .with_computed_name_resolver(&computed_name_resolver)
        .with_lazy_type_params_resolver(&lazy_type_params_resolver)
        .with_name_def_id_resolver(&name_resolver);

        lowering.lower_type_alias_declaration(type_alias)
    }

    fn precompute_cross_arena_computed_property_names<F>(
        &self,
        arena: &NodeArena,
        root: NodeIndex,
        resolve_symbol: &F,
    ) -> rustc_hash::FxHashMap<NodeIndex, tsz_common::Atom>
    where
        F: Fn(&str) -> Option<SymbolId>,
    {
        let mut map = rustc_hash::FxHashMap::default();
        let mut stack = vec![root];

        while let Some(node_idx) = stack.pop() {
            let Some(node) = arena.get(node_idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                && let Some(name) =
                    self.resolve_cross_arena_computed_property_name(arena, node_idx, resolve_symbol)
                && let Some(computed) = arena.get_computed_property(node)
            {
                map.insert(computed.expression, self.ctx.types.intern_string(&name));
            }

            stack.extend(arena.get_children(node_idx));
        }

        map
    }

    fn resolve_cross_arena_computed_property_name<F>(
        &self,
        arena: &NodeArena,
        name_idx: NodeIndex,
        resolve_symbol: &F,
    ) -> Option<String>
    where
        F: Fn(&str) -> Option<SymbolId>,
    {
        if let Some(name) =
            crate::types_domain::queries::core::get_literal_property_name(arena, name_idx)
        {
            return Some(name);
        }

        let name_node = arena.get(name_idx)?;
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }

        let computed = arena.get_computed_property(name_node)?;
        if let Some(name) =
            self.well_known_symbol_property_name_in_cross_arena(arena, computed.expression)
        {
            return Some(name);
        }

        let text = self.cross_arena_expression_name_text(arena, computed.expression)?;
        let sym_id = resolve_symbol(&text)?;
        self.cross_arena_symbol_is_unique(sym_id)
            .then(|| format!("__unique_{}", sym_id.0))
    }

    fn well_known_symbol_property_name_in_cross_arena(
        &self,
        arena: &NodeArena,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let node = arena.get(expr_idx)?;

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            let paren = arena.get_parenthesized(node)?;
            return self.well_known_symbol_property_name_in_cross_arena(arena, paren.expression);
        }

        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
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
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        ) && let Some(lit) = arena.get_literal(name_node)
            && !lit.text.is_empty()
        {
            return Some(format!("[Symbol.{}]", lit.text));
        }

        None
    }

    fn cross_arena_expression_name_text(
        &self,
        arena: &NodeArena,
        idx: NodeIndex,
    ) -> Option<String> {
        let node = arena.get(idx)?;

        if node.kind == SyntaxKind::Identifier as u16 {
            return arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.clone());
        }

        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = arena.get_qualified_name(node)?;
            let left = self.cross_arena_expression_name_text(arena, qn.left)?;
            let right = self.cross_arena_expression_name_text(arena, qn.right)?;
            return Some(format!("{left}.{right}"));
        }

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            let paren = arena.get_parenthesized(node)?;
            return self.cross_arena_expression_name_text(arena, paren.expression);
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = arena.get_access_expr(node)
        {
            let left = self.cross_arena_expression_name_text(arena, access.expression)?;
            let right_node = arena.get(access.name_or_argument)?;
            let right = arena.get_identifier(right_node)?;
            return Some(format!("{left}.{}", right.escaped_text));
        }

        None
    }

    fn cross_arena_symbol_is_unique(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) =
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
                    self.ctx.all_binders.as_ref().and_then(|binders| {
                        binders.iter().find_map(|binder| binder.get_symbol(sym_id))
                    })
                })
                .or_else(|| {
                    self.ctx
                        .lib_contexts
                        .iter()
                        .find_map(|ctx| ctx.binder.get_symbol(sym_id))
                })
        else {
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

            candidate_arenas.into_iter().any(|arena| {
                let Some(node) = arena.get(decl_idx) else {
                    return false;
                };
                if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                    return false;
                }
                let Some(var_decl) = arena.get_variable_declaration(node) else {
                    return false;
                };

                (var_decl.type_annotation.is_some()
                    && self
                        .cross_arena_unique_symbol_type_annotation(arena, var_decl.type_annotation))
                    || self.cross_arena_symbol_call_initializer(arena, var_decl.initializer)
            })
        })
    }

    fn cross_arena_unique_symbol_type_annotation(
        &self,
        arena: &NodeArena,
        type_annotation: NodeIndex,
    ) -> bool {
        let Some(type_node) = arena.get(type_annotation) else {
            return false;
        };
        match type_node.kind {
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                arena.get_type_operator(type_node).is_some_and(|op| {
                    op.operator == SyntaxKind::UniqueKeyword as u16
                        && self.cross_arena_symbol_type_reference(arena, op.type_node)
                })
            }
            _ => false,
        }
    }

    fn cross_arena_symbol_type_reference(
        &self,
        arena: &NodeArena,
        type_annotation: NodeIndex,
    ) -> bool {
        let Some(type_node) = arena.get(type_annotation) else {
            return false;
        };
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
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

    fn cross_arena_symbol_call_initializer(&self, arena: &NodeArena, init_idx: NodeIndex) -> bool {
        let Some(node) = arena.get(init_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
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

    fn type_alias_namespace_prefix(
        &self,
        decl_arena: &NodeArena,
        decl_idx: NodeIndex,
    ) -> Option<String> {
        let mut parent = decl_arena
            .get_extended(decl_idx)
            .map_or(NodeIndex::NONE, |info| info.parent);
        let mut prefixes = Vec::new();

        while !parent.is_none() {
            let parent_node = decl_arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module) = decl_arena.get_module(parent_node)
                && let Some(name_node) = decl_arena.get(module.name)
                && name_node.kind == SyntaxKind::Identifier as u16
                && let Some(name_ident) = decl_arena.get_identifier(name_node)
            {
                prefixes.push(name_ident.escaped_text.clone());
            }

            parent = decl_arena
                .get_extended(parent)
                .map_or(NodeIndex::NONE, |info| info.parent);
        }

        (!prefixes.is_empty()).then(|| prefixes.into_iter().rev().collect::<Vec<_>>().join("."))
    }

    /// Walk the type alias body in a cross-arena and prime lib type params
    /// for any `TYPE_REFERENCE` nodes that lack explicit type arguments.
    /// This ensures that generic lib types with all-default type params
    /// (e.g., `Uint8Array<TArrayBuffer = ArrayBuffer>`) get their defaults
    /// applied during lowering.
    fn prime_cross_arena_type_reference_params(&mut self, decl_arena: &NodeArena, root: NodeIndex) {
        let mut stack = vec![root];
        let mut names_to_prime = Vec::new();

        while let Some(node_idx) = stack.pop() {
            let Some(node) = decl_arena.get(node_idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::TYPE_REFERENCE
                && let Some(type_ref) = decl_arena.get_type_ref(node)
            {
                let has_type_args = type_ref
                    .type_arguments
                    .as_ref()
                    .is_some_and(|args| !args.nodes.is_empty());
                if !has_type_args
                    && let Some(name_node) = decl_arena.get(type_ref.type_name)
                    && let Some(ident) = decl_arena.get_identifier(name_node)
                {
                    names_to_prime.push(ident.escaped_text.clone());
                }
            }

            stack.extend(decl_arena.get_children(node_idx));
        }

        for name in names_to_prime {
            self.prime_lib_type_params(&name);
        }
    }
}
