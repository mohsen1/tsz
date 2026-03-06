//! DefId Resolution Helpers for `TypeNodeChecker`
//!
//! Extracted from `type_node.rs` to keep that file under the LOC limit.
//! Contains methods for ensuring type alias bodies are registered in the
//! type environment and for resolving `DefIds` from qualified names.

use tsz_parser::parser::NodeIndex;

use super::type_node::TypeNodeChecker;

impl<'a, 'ctx> TypeNodeChecker<'a, 'ctx> {
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
                    if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
                        if type_params.is_empty() {
                            env.insert_def(def_id, type_id);
                        } else {
                            env.insert_def_with_params(def_id, type_id, type_params);
                        }
                        env.register_def_symbol_mapping(def_id, sym_id);
                    }
                }
            }
            return;
        }

        // Skip if type params already registered
        if self.ctx.get_def_type_params(def_id).is_some() {
            return;
        }

        // Only handle type aliases with type parameters
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return;
        };
        if symbol.flags & symbol_flags::TYPE_ALIAS == 0 {
            return;
        }

        // Find the type alias declaration
        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .first()
                .copied()
                .unwrap_or(NodeIndex::NONE)
        };
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return;
        };
        let Some(type_alias) = self.ctx.arena.get_type_alias(node) else {
            return;
        };
        let Some(ref type_param_list) = type_alias.type_parameters else {
            return;
        };
        if type_param_list.nodes.is_empty() {
            return;
        }

        // Extract type parameters from AST and create TypeParam TypeIds
        let factory = self.ctx.types.factory();
        let mut params = Vec::new();
        let mut bindings = Vec::new();

        for &param_idx in &type_param_list.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param_data) = self.ctx.arena.get_type_parameter(param_node) else {
                continue;
            };

            let name = self
                .ctx
                .arena
                .get(param_data.name)
                .and_then(|n| self.ctx.arena.get_identifier(n))
                .map_or_else(|| "T".to_string(), |id| id.escaped_text.clone());

            let atom = self.ctx.types.intern_string(&name);
            let info = tsz_solver::TypeParamInfo {
                name: atom,
                constraint: None,
                default: None,
                is_const: false,
            };
            let type_id = factory.type_param(info.clone());
            bindings.push((atom, type_id));
            params.push(info);
        }

        // Register type params for Application expansion
        self.ctx.insert_def_type_params(def_id, params.clone());

        // Lower the type alias body with the type params in scope
        if type_alias.type_node != NodeIndex::NONE {
            let type_resolver = |n: NodeIndex| -> Option<u32> { self.resolve_type_symbol(n) };
            let def_id_resolver = |n: NodeIndex| -> Option<tsz_solver::def::DefId> {
                let raw = self.resolve_type_symbol(n)?;
                let sym_id = tsz_binder::SymbolId(raw);
                let def_id = self.ctx.get_or_create_def_id(sym_id);
                // Recursively ensure referenced type aliases have their body
                // and params registered in TypeEnvironment. Without this,
                // type aliases only referenced inside other type alias bodies
                // (e.g., Func inside Spec) would have DefIds created but no
                // body registered, causing resolve_lazy to fail during evaluation.
                self.ensure_type_alias_resolved(sym_id, def_id);
                Some(def_id)
            };
            let value_resolver =
                |n: NodeIndex| -> Option<u32> { self.resolve_value_symbol_with_libs(n) };

            let lowering = tsz_lowering::TypeLowering::with_hybrid_resolver(
                self.ctx.arena,
                self.ctx.types,
                &type_resolver,
                &def_id_resolver,
                &value_resolver,
            )
            .with_type_param_bindings(bindings);

            let body = lowering.lower_type(type_alias.type_node);

            // Register body in type_env so resolve_lazy can find it
            if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
                env.insert_def_with_params(def_id, body, params);
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

        if let Some(sym_id) = self.resolve_type_symbol(node_idx) {
            return Some(self.ctx.get_or_create_def_id(tsz_binder::SymbolId(sym_id)));
        }

        let node = self.ctx.arena.get(node_idx)?;
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.ctx.arena.get_qualified_name(node)?;
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
            let resolved_symbol = self.ctx.binder.get_symbol(resolved_sym_id)?;

            let right_node = self.ctx.arena.get(qn.right)?;
            let right_ident = self.ctx.arena.get_identifier(right_node)?;
            let right_name = right_ident.escaped_text.as_str();

            // Look up the member in the resolved symbol's exports
            if let Some(member_sym_id) = resolved_symbol.exports.as_ref()?.get(right_name) {
                return Some(self.ctx.get_or_create_def_id(member_sym_id));
            }

            // Also check lib contexts for the member (e.g., global namespace types)
            for lib_ctx in &self.ctx.lib_contexts {
                if let Some(lib_resolved) = lib_ctx.binder.resolve_import_symbol(left_sym_id)
                    && let Some(lib_symbol) = lib_ctx.binder.get_symbol(lib_resolved)
                    && let Some(member_sym_id) = lib_symbol.exports.as_ref()?.get(right_name)
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

        for lib_ctx in &self.ctx.lib_contexts {
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
