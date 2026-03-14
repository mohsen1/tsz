//! Library type resolution: resolving built-in types from `.d.ts` lib files,
//! merging interface heritage from lib arenas, and handling global augmentations.

use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::sync::Arc;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;
use tsz_solver::is_compiler_managed_type;

impl<'a> CheckerState<'a> {
    // Section 45: Symbol Resolution Utilities
    // ----------------------------------------

    /// Resolve a library type by name from lib.d.ts and other library contexts.
    ///
    /// This function resolves types from library definition files like lib.d.ts,
    /// es2015.d.ts, etc., which provide built-in JavaScript types and DOM APIs.
    ///
    /// ## Library Contexts:
    /// - Searches through loaded library contexts (lib.d.ts, es2015.d.ts, etc.)
    /// - Each lib context has its own binder and arena
    /// - Types are "lowered" from lib arena to main arena
    ///
    /// ## Declaration Merging:
    /// - Interfaces can have multiple declarations that are merged
    /// - All declarations are lowered together to create merged type
    /// - Essential for types like `Array` which have multiple lib declarations
    ///
    /// ## Global Augmentations:
    /// - User's `declare global` blocks are merged with lib types
    /// - Allows extending built-in types like `Window`, `String`, etc.
    ///
    /// ## Examples:
    /// ```typescript
    /// // Built-in types from lib.d.ts
    /// let arr: Array<number>;  // resolve_lib_type_by_name("Array")
    /// let obj: Object;         // resolve_lib_type_by_name("Object")
    /// let prom: Promise<string>; // resolve_lib_type_by_name("Promise")
    ///
    /// // Global augmentation
    /// declare global {
    ///   interface Window {
    ///     myCustomProperty: string;
    ///   }
    /// }
    /// // lib Window type is merged with augmentation
    /// ```
    /// Merge base interface members into a lib interface type by walking
    /// heritage (`extends`) clauses in declaration-specific arenas.
    ///
    /// This is needed because `merge_interface_heritage_types` uses `self.ctx.arena`
    /// (the user file arena) and cannot read lib declarations that live in lib arenas.
    /// Takes the interface name and looks up declarations from the binder.
    pub(crate) fn merge_lib_interface_heritage(
        &mut self,
        mut derived_type: TypeId,
        name: &str,
    ) -> TypeId {
        // Guard against infinite recursion in recursive generic hierarchies
        // (e.g., interface B<T extends B<T,S>> extends A<B<T,S>, B<T,S>>)
        if !self.ctx.enter_recursion() {
            return derived_type;
        }

        // Name-based cycle guard: prevent re-entrant heritage merging for the same
        // interface name. This breaks the resolve_lib_type_by_name ↔ merge_lib_interface_heritage
        // mutual recursion that occurs through deep heritage chains
        // (e.g., Array → ReadonlyArray → Iterable → ...), especially when child
        // CheckerStates are created for cross-arena type param resolution.
        if !self.ctx.lib_heritage_in_progress.insert(name.to_string()) {
            self.ctx.leave_recursion();
            return derived_type;
        }

        let lib_contexts = self.ctx.lib_contexts.clone();

        // Look up the symbol and its declarations
        let Some(sym_id) = self.ctx.binder.file_locals.get(name) else {
            self.ctx.lib_heritage_in_progress.remove(name);
            self.ctx.leave_recursion();
            return derived_type;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            self.ctx.lib_heritage_in_progress.remove(name);
            self.ctx.leave_recursion();
            return derived_type;
        };

        let fallback_arena: &NodeArena = self
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(std::convert::AsRef::as_ref)
            .or_else(|| lib_contexts.first().map(|ctx| ctx.arena.as_ref()))
            .unwrap_or(self.ctx.arena);

        let user_arena: &NodeArena = self.ctx.arena;
        let decls_with_arenas: Vec<(NodeIndex, &NodeArena)> = symbol
            .declarations
            .iter()
            .flat_map(|&decl_idx| {
                if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                    arenas
                        .iter()
                        .map(|arc| (decl_idx, arc.as_ref()))
                        .collect::<Vec<_>>()
                } else if user_arena.get(decl_idx).is_some() {
                    // User augmentations (e.g., `interface Array<T> extends IFoo<T>`)
                    // are not in declaration_arenas (which only tracks lib-merged
                    // declarations). Check the user arena before falling back.
                    vec![(decl_idx, user_arena)]
                } else {
                    vec![(decl_idx, fallback_arena)]
                }
            })
            .collect();

        // Early exit: skip expensive type parameter scope setup and heritage merge
        // if no declarations have extends clauses
        let has_any_heritage = decls_with_arenas.iter().any(|&(decl_idx, arena)| {
            let Some(node) = arena.get(decl_idx) else {
                return false;
            };
            let Some(interface) = arena.get_interface(node) else {
                return false;
            };
            interface
                .heritage_clauses
                .as_ref()
                .is_some_and(|hc| !hc.nodes.is_empty())
        });

        if !has_any_heritage {
            self.ctx.leave_recursion();
            return derived_type;
        }

        // Seed type-parameter scope with the derived interface's generic params so
        // heritage args like `extends IteratorObject<T, ...>` resolve `T` correctly.
        // Without this, lib heritage substitution falls back to `unknown` and loses
        // member types (e.g. `ArrayIterator<T>.next().value` becomes `unknown`).
        let mut scope_restore: Vec<(String, Option<TypeId>)> = Vec::new();
        for param in self.get_type_params_for_symbol(sym_id) {
            let name = self.ctx.types.resolve_atom(param.name).to_string();
            let param_ty = self.ctx.types.type_param(param.clone());
            let prev = self.ctx.type_parameter_scope.insert(name.clone(), param_ty);
            scope_restore.push((name, prev));
        }

        // Collect base type info: name and type argument node indices with their arena.
        // We collect these first to avoid borrow conflicts during resolution.
        struct HeritageBase<'a> {
            name: String,
            type_arg_indices: Vec<NodeIndex>,
            arena: &'a NodeArena,
        }
        let mut bases: Vec<HeritageBase<'_>> = Vec::new();

        for &(decl_idx, arena) in &decls_with_arenas {
            let Some(node) = arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = arena.get_interface(node) else {
                continue;
            };
            let Some(ref heritage_clauses) = interface.heritage_clauses else {
                continue;
            };

            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }

                for &type_idx in &heritage.types.nodes {
                    let Some(type_node) = arena.get(type_idx) else {
                        continue;
                    };

                    // Extract the base type name and type arguments
                    let (expr_idx, type_arguments) =
                        if let Some(eta) = arena.get_expr_type_args(type_node) {
                            (eta.expression, eta.type_arguments.as_ref())
                        } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                            if let Some(tr) = arena.get_type_ref(type_node) {
                                (tr.type_name, tr.type_arguments.as_ref())
                            } else {
                                (type_idx, None)
                            }
                        } else {
                            (type_idx, None)
                        };

                    if let Some(base_name) = arena.get_identifier_text(expr_idx) {
                        let type_arg_indices = type_arguments
                            .map(|args| args.nodes.clone())
                            .unwrap_or_default();
                        bases.push(HeritageBase {
                            name: base_name.to_string(),
                            type_arg_indices,
                            arena,
                        });
                    }
                }
            }
        }

        // Now resolve each base type and merge, applying type argument substitution
        for base in &bases {
            if let Some(mut base_type) = self.resolve_lib_type_by_name(&base.name) {
                // If there are type arguments, resolve them and substitute
                if !base.type_arg_indices.is_empty() {
                    let base_sym = self.ctx.binder.file_locals.get(&base.name);
                    if let Some(base_sym_id) = base_sym {
                        let base_params = self.get_type_params_for_symbol(base_sym_id);
                        if !base_params.is_empty() {
                            let mut type_args = Vec::new();
                            for &arg_idx in &base.type_arg_indices {
                                // Resolve type arguments from the lib arena.
                                // Heritage type args are typically simple type
                                // references (e.g., `string`, `number`).
                                let ty = self.resolve_lib_heritage_type_arg(arg_idx, base.arena);
                                type_args.push(ty);
                            }
                            // Pad/truncate args to match params
                            while type_args.len() < base_params.len() {
                                let param = &base_params[type_args.len()];
                                type_args.push(
                                    param
                                        .default
                                        .or(param.constraint)
                                        .unwrap_or(TypeId::UNKNOWN),
                                );
                            }
                            type_args.truncate(base_params.len());

                            let substitution = tsz_solver::TypeSubstitution::from_args(
                                self.ctx.types,
                                &base_params,
                                &type_args,
                            );
                            base_type = tsz_solver::instantiate_type(
                                self.ctx.types,
                                base_type,
                                &substitution,
                            );
                        }
                    }
                }
                derived_type = self.merge_interface_types(derived_type, base_type);
            }
        }

        for (name, prev) in scope_restore {
            if let Some(prev_ty) = prev {
                self.ctx.type_parameter_scope.insert(name, prev_ty);
            } else {
                self.ctx.type_parameter_scope.remove(&name);
            }
        }

        self.ctx.lib_heritage_in_progress.remove(name);
        self.ctx.leave_recursion();
        derived_type
    }

    /// Resolve a type argument node from a lib arena to a TypeId.
    /// Handles simple keyword types (string, number, etc.), type references
    /// to other lib types, and the derived interface's own type parameters.
    fn resolve_lib_heritage_type_arg(&mut self, node_idx: NodeIndex, arena: &NodeArena) -> TypeId {
        let Some(node) = arena.get(node_idx) else {
            return TypeId::UNKNOWN;
        };

        // Handle keyword types (string, number, boolean, etc.)
        match node.kind {
            k if k == SyntaxKind::StringKeyword as u16 => return TypeId::STRING,
            k if k == SyntaxKind::NumberKeyword as u16 => return TypeId::NUMBER,
            k if k == SyntaxKind::BooleanKeyword as u16 => return TypeId::BOOLEAN,
            k if k == SyntaxKind::VoidKeyword as u16 => return TypeId::VOID,
            k if k == SyntaxKind::UndefinedKeyword as u16 => return TypeId::UNDEFINED,
            k if k == SyntaxKind::NullKeyword as u16 => return TypeId::NULL,
            k if k == SyntaxKind::NeverKeyword as u16 => return TypeId::NEVER,
            k if k == SyntaxKind::UnknownKeyword as u16 => return TypeId::UNKNOWN,
            k if k == SyntaxKind::AnyKeyword as u16 => return TypeId::ANY,
            k if k == SyntaxKind::ObjectKeyword as u16 => return TypeId::OBJECT,
            k if k == SyntaxKind::SymbolKeyword as u16 => return TypeId::SYMBOL,
            k if k == SyntaxKind::BigIntKeyword as u16 => return TypeId::BIGINT,
            _ => {}
        }

        // Handle type references (e.g., other interface names or type params)
        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = arena.get_type_ref(node)
            && let Some(name) = arena.get_identifier_text(type_ref.type_name)
        {
            // Check primitive/keyword type names first
            match name {
                "string" => return TypeId::STRING,
                "number" => return TypeId::NUMBER,
                "boolean" => return TypeId::BOOLEAN,
                "void" => return TypeId::VOID,
                "undefined" => return TypeId::UNDEFINED,
                "null" => return TypeId::NULL,
                "never" => return TypeId::NEVER,
                "unknown" => return TypeId::UNKNOWN,
                "any" => return TypeId::ANY,
                "object" => return TypeId::OBJECT,
                "symbol" => return TypeId::SYMBOL,
                "bigint" => return TypeId::BIGINT,
                _ => {}
            }
            // Check type parameter scope
            if let Some(&type_id) = self.ctx.type_parameter_scope.get(name) {
                return type_id;
            }
            // Try to resolve as a lib type
            if let Some(ty) = self.resolve_lib_type_by_name(name) {
                return ty;
            }
            // Preserve unresolved lib heritage args as symbolic type params
            // (e.g. `T` in `extends IteratorObject<T, ...>`) instead of
            // collapsing to unknown.
            let atom = self.ctx.types.intern_string(name);
            return self.ctx.types.type_param(tsz_solver::TypeParamInfo {
                name: atom,
                constraint: None,
                default: None,
                is_const: false,
            });
        }

        // For identifiers, try resolving the name
        if let Some(name) = arena.get_identifier_text(node_idx) {
            if let Some(&type_id) = self.ctx.type_parameter_scope.get(name) {
                return type_id;
            }
            if let Some(ty) = self.resolve_lib_type_by_name(name) {
                return ty;
            }
            let atom = self.ctx.types.intern_string(name);
            return self.ctx.types.type_param(tsz_solver::TypeParamInfo {
                name: atom,
                constraint: None,
                default: None,
                is_const: false,
            });
        }

        TypeId::UNKNOWN
    }

    pub(crate) fn resolve_lib_type_by_name(&mut self, name: &str) -> Option<TypeId> {
        use tsz_lowering::TypeLowering;

        // When TS5107/TS5101 deprecation diagnostics are present, skip all lib type
        // resolution. tsc stops compilation at TS5107 and never resolves lib types.
        // We still walk the AST for grammar errors (17xxx), but short-circuit type
        // resolution to avoid the O(n²) memory explosion from multiple files
        // independently resolving deep es5 heritage chains.
        if self.ctx.skip_lib_type_resolution {
            return None;
        }

        // TS 6.0 lib intrinsic: defaults to `undefined` unless
        // `strictBuiltinIteratorReturn` is disabled.
        // We currently model default strict behavior.
        if name == "BuiltinIteratorReturn" {
            return Some(TypeId::UNDEFINED);
        }

        // Check shared cross-file lib cache first
        if let Some(ref shared_cache) = self.ctx.shared_lib_type_cache
            && let Some(entry) = shared_cache.get(name)
        {
            let result = *entry;
            self.ctx
                .lib_type_resolution_cache
                .insert(name.to_string(), result);
            return result;
        }

        if let Some(cached) = self.ctx.lib_type_resolution_cache.get(name) {
            return *cached;
        }

        tracing::trace!(name, "resolve_lib_type_by_name: called");
        let mut lib_type_id: Option<TypeId> = None;
        let factory = self.ctx.types.factory();

        let lib_contexts = self.ctx.lib_contexts.clone();
        // Collect lowered types from the symbol's declarations.
        // The main file's binder already has merged declarations from all lib files.
        let mut lib_types: Vec<TypeId> = Vec::new();

        // CRITICAL: Look up the symbol in the MAIN file's binder (self.ctx.binder),
        // not in lib_ctx.binder. The main file's binder has lib symbols merged with
        // unique SymbolIds via merge_lib_contexts_into_binder during binding.
        // lib_ctx.binder is a SEPARATE merged binder with DIFFERENT SymbolIds.
        // Using lib_ctx.binder's SymbolIds with self.ctx.get_or_create_def_id causes
        // SymbolId collisions and wrong type resolution.
        let lib_binders = self.get_lib_binders();
        let sym_id = self.ctx.binder.file_locals.get(name).or_else(|| {
            self.ctx
                .binder
                .get_global_type_with_libs(name, &lib_binders)
        });

        if let Some(sym_id) = sym_id {
            // Get the symbol's declaration(s) from the main file's binder
            if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) {
                // Get the fallback arena from lib_contexts if available, otherwise use main arena
                let fallback_arena: &NodeArena = self
                    .ctx
                    .binder
                    .symbol_arenas
                    .get(&sym_id)
                    .map(std::convert::AsRef::as_ref)
                    .or_else(|| lib_contexts.first().map(|ctx| ctx.arena.as_ref()))
                    .unwrap_or(self.ctx.arena);

                // Build declaration -> arena pairs using declaration_arenas
                // This is critical for merged interfaces like Array which have
                // declarations in es5.d.ts, es2015.d.ts, etc.
                // Use the MAIN file's binder's declaration_arenas, not lib_ctx.binder.
                let decls_with_arenas: Vec<(NodeIndex, &NodeArena)> = symbol
                    .declarations
                    .iter()
                    .flat_map(|&decl_idx| {
                        if let Some(arenas) =
                            self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx))
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

                // Create resolver that looks up names in the MAIN file's binder
                // CRITICAL: Use self.ctx.binder, not lib_contexts binders, to avoid SymbolId collisions
                let binder = &self.ctx.binder;
                // Resolver hot path: cache SymbolId -> DefId for this lowering run.
                // Many identifiers repeat across merged lib declarations.
                let def_id_cache =
                    RefCell::new(FxHashMap::<tsz_binder::SymbolId, tsz_solver::DefId>::default());
                let resolver_cache = RefCell::new(FxHashMap::<NodeIndex, Option<u32>>::default());
                let get_cached_def_id = |symbol_id: tsz_binder::SymbolId| -> tsz_solver::DefId {
                    if let Some(def_id) = def_id_cache.borrow().get(&symbol_id).copied() {
                        return def_id;
                    }
                    let def_id = self.ctx.get_or_create_def_id(symbol_id);
                    def_id_cache.borrow_mut().insert(symbol_id, def_id);
                    def_id
                };
                let resolver = |node_idx: NodeIndex| -> Option<u32> {
                    if let Some(cached) = resolver_cache.borrow().get(&node_idx) {
                        return *cached;
                    }

                    // For merged declarations, we need to check the arena for this specific node.
                    // IMPORTANT: NodeIndex values are arena-specific — the same index can refer
                    // to different nodes in different arenas. We must check ALL arenas and only
                    // return a match when the identifier is found in file_locals. Don't break
                    // early on a mismatch since another arena may have the correct identifier
                    // at the same NodeIndex.
                    for (_, arena) in &decls_with_arenas {
                        if let Some(ident_name) = arena.get_identifier_text(node_idx) {
                            if is_compiler_managed_type(ident_name) {
                                continue;
                            }
                            if let Some(found_sym) = binder.file_locals.get(ident_name) {
                                let resolved = Some(found_sym.0);
                                resolver_cache.borrow_mut().insert(node_idx, resolved);
                                return resolved;
                            }
                            // Don't break - another arena may have a different identifier
                            // at the same NodeIndex that resolves successfully
                        }
                    }
                    // Also try fallback arena
                    if let Some(ident_name) = fallback_arena.get_identifier_text(node_idx) {
                        if is_compiler_managed_type(ident_name) {
                            resolver_cache.borrow_mut().insert(node_idx, None);
                            return None;
                        }
                        if let Some(found_sym) = binder.file_locals.get(ident_name) {
                            let resolved = Some(found_sym.0);
                            resolver_cache.borrow_mut().insert(node_idx, resolved);
                            return resolved;
                        }
                    }
                    resolver_cache.borrow_mut().insert(node_idx, None);
                    None
                };

                // Create def_id_resolver that converts SymbolIds to DefIds
                // This is required for Phase 4.2 which uses TypeData::Lazy(DefId) everywhere
                let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::DefId> {
                    resolver(node_idx).map(|sym_id| get_cached_def_id(tsz_binder::SymbolId(sym_id)))
                };

                // Name-based resolver: resolves identifier text directly without NodeIndex.
                // This is the reliable fallback for cross-arena lowering where NodeIndex
                // values from the current arena don't match nodes in the declaration arenas.
                let name_resolver = |type_name: &str| -> Option<tsz_solver::DefId> {
                    self.resolve_entity_name_text_to_def_id_for_lowering(type_name)
                };

                // Create base lowering with the fallback arena and both resolvers
                let lowering = TypeLowering::with_hybrid_resolver(
                    fallback_arena,
                    self.ctx.types,
                    &resolver,
                    &def_id_resolver,
                    &|_| None,
                )
                .with_name_def_id_resolver(&name_resolver);

                // Try to lower as interface first (handles declaration merging)
                if !symbol.declarations.is_empty() {
                    // Check if any declaration is a type alias — if so, skip interface
                    // lowering. Type aliases like Record<K,T>, Partial<T>, Pick<T,K>
                    // would incorrectly succeed interface lowering with 0 type params,
                    // preventing the proper type alias path from running.
                    let is_type_alias = (symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS) != 0;

                    if !is_type_alias {
                        // Use lower_merged_interface_declarations for proper multi-arena support
                        let (ty, params) =
                            lowering.lower_merged_interface_declarations(&decls_with_arenas);

                        // If lowering succeeded (not ERROR), use the result
                        if ty != TypeId::ERROR {
                            // Record type parameters for generic interfaces
                            let file_sym_id =
                                self.ctx.binder.file_locals.get(name).unwrap_or(sym_id);
                            let def_id = get_cached_def_id(file_sym_id);
                            if !params.is_empty() {
                                // Cache type params for Application expansion
                                self.ctx.insert_def_type_params(def_id, params.clone());
                            }

                            // Register the interface body in TypeEnvironment so that
                            // resolve_lazy(def_id) can find it. Without this, Lazy(DefId)
                            // references to lib interfaces (e.g., ConcatArray in Array.concat)
                            // fall through to a SymbolId-based fallback that produces wrong types.
                            if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
                                if params.is_empty() {
                                    env.insert_def(def_id, ty);
                                } else {
                                    env.insert_def_with_params(def_id, ty, params.clone());
                                }
                            }
                            // Also register in type_environment (Rc-wrapped) for FlowAnalyzer.
                            // type_env and type_environment are separate TypeEnvironment instances
                            // that are only synchronized once at startup. Without this, narrowing
                            // contexts can't resolve Application types for cross-file lib interfaces
                            // (e.g., ArrayLike<any> in type predicate narrowing).
                            if let Ok(mut env) = self.ctx.type_environment.try_borrow_mut() {
                                if params.is_empty() {
                                    env.insert_def(def_id, ty);
                                } else {
                                    env.insert_def_with_params(def_id, ty, params);
                                }
                            }

                            lib_types.push(ty);
                        }
                    }

                    // Interface lowering skipped or returned ERROR - try as type alias
                    // Type aliases like Partial<T>, Pick<T,K>, Record<K,T> have their
                    // declaration in symbol.declarations but are not interface nodes
                    if lib_types.is_empty() {
                        for (decl_idx, decl_arena) in &decls_with_arenas {
                            if let Some(node) = decl_arena.get(*decl_idx)
                                && let Some(alias) = decl_arena.get_type_alias(node)
                            {
                                let alias_lowering = lowering.with_arena(decl_arena);
                                let (ty, params) =
                                    alias_lowering.lower_type_alias_declaration(alias);
                                if ty != TypeId::ERROR {
                                    // Cache type parameters for Application expansion
                                    let def_id = get_cached_def_id(sym_id);
                                    self.ctx.insert_def_type_params(def_id, params.clone());

                                    // CRITICAL: Register the type body in TypeEnvironment so that
                                    // evaluate_application can resolve it via resolve_lazy(def_id).
                                    // Without this, Partial<T>, Pick<T,K>, etc. resolve to unknown.
                                    if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
                                        env.insert_def_with_params(def_id, ty, params.clone());
                                    }
                                    // Also register in type_environment for FlowAnalyzer.
                                    if let Ok(mut env) = self.ctx.type_environment.try_borrow_mut()
                                    {
                                        env.insert_def_with_params(def_id, ty, params);
                                    }

                                    // CRITICAL: Return Lazy(DefId) instead of the structural body.
                                    // Application types only expand when the base is Lazy, not when
                                    // it's the actual MappedType/Object/etc. This allows evaluate_application
                                    // to trigger and substitute type parameters correctly.
                                    let lazy_type = self.ctx.types.factory().lazy(def_id);
                                    lib_types.push(lazy_type);

                                    // Type aliases don't merge across files, take the first one
                                    break;
                                }
                            }
                        }
                    }
                }

                // For value declarations (vars, consts, functions)
                let decl_idx = symbol.value_declaration;
                if decl_idx.0 != u32::MAX {
                    // Get the correct arena for the value declaration from main binder
                    let value_arena = self
                        .ctx
                        .binder
                        .declaration_arenas
                        .get(&(sym_id, decl_idx))
                        .and_then(|v| v.first())
                        .map_or(fallback_arena, |arc| arc.as_ref());
                    let value_lowering = if value_arena
                        .get(decl_idx)
                        .and_then(|node| value_arena.get_source_file(node))
                        .is_some_and(|source| {
                            source.is_declaration_file
                                && source.file_name.starts_with("lib.")
                                && source.file_name.ends_with(".d.ts")
                        }) {
                        lowering
                            .with_arena(value_arena)
                            .prefer_name_def_id_resolution()
                    } else {
                        lowering.with_arena(value_arena)
                    };
                    let val_type = value_lowering.lower_type(decl_idx);
                    // Only include non-ERROR types. Value declaration lowering can fail
                    // when type references (e.g., `PromiseConstructor`) can't be resolved
                    // during TypeLowering. Including ERROR in the lib_types vector would
                    // cause intersection2 to collapse a valid interface type to ERROR.
                    if val_type != TypeId::ERROR {
                        lib_types.push(val_type);
                    }
                }
            }
        }

        // Merge all found types from different lib files using intersection
        if lib_types.len() == 1 {
            lib_type_id = Some(lib_types[0]);
        } else if lib_types.len() > 1 {
            let mut merged = lib_types[0];
            for &ty in &lib_types[1..] {
                merged = factory.intersection2(merged, ty);
            }
            lib_type_id = Some(merged);
        }

        // Merge heritage (extends) from lib interface declarations.
        // This propagates base interface members (e.g., Iterator.next() into ArrayIterator).
        //
        // CRITICAL: Insert the pre-heritage type into the cache BEFORE merging heritage.
        // merge_lib_interface_heritage calls resolve_lib_type_by_name recursively for base
        // types. Without this early cache insertion, recursive calls redo all lowering work
        // for types already being resolved, causing O(n!) blowup on deep heritage chains
        // (e.g., es5.d.ts where Array extends ReadonlyArray, etc.).
        // The recursive call gets the un-merged type (missing inherited members), which is
        // still correct for breaking cycles. The final cache update below overwrites with
        // the fully-merged type.
        if let Some(ty) = lib_type_id {
            self.ctx
                .lib_type_resolution_cache
                .insert(name.to_string(), Some(ty));
            // Also insert pre-heritage type into shared cache so parallel threads
            // can break out of their own resolution early (they get the un-merged
            // type, which is correct for cycle breaking).
            if let Some(ref shared_cache) = self.ctx.shared_lib_type_cache {
                shared_cache.entry(name.to_string()).or_insert(Some(ty));
            }
            lib_type_id = Some(self.merge_lib_interface_heritage(ty, name));
        }

        // Check for global augmentations that should merge with this type.
        // Augmentations may come from the current file or other files (cross-file merge).
        if let Some(augmentation_decls) = self.ctx.binder.global_augmentations.get(name)
            && !augmentation_decls.is_empty()
        {
            // Group augmentation declarations by arena.
            // Declarations with arena=None use the current file's arena.
            let current_arena: &NodeArena = self.ctx.arena;
            let binder_ref = self.ctx.binder;

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

            // Collect declarations grouped by arena pointer identity
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
                let symbol_lookup_cache =
                    RefCell::new(FxHashMap::<String, Option<tsz_binder::SymbolId>>::default());
                let resolve_name_symbol = |ident_name: &str| -> Option<tsz_binder::SymbolId> {
                    if let Some(cached) = symbol_lookup_cache.borrow().get(ident_name) {
                        return *cached;
                    }

                    let found = decl_binder.file_locals.get(ident_name).or_else(|| {
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
                    });

                    symbol_lookup_cache
                        .borrow_mut()
                        .insert(ident_name.to_string(), found);
                    found
                };
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
                    resolve_name_symbol(ident_name).map(|sym| sym.0)
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
                    let sym_id = resolve_name_symbol(ident_name)?;
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
                    Some(factory.intersection2(lib_type, aug_type))
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

        // Process heritage clauses from global augmentations.
        // This is in a separate block because lower_with_arena borrows `self`
        // and we need `&mut self` for resolve_heritage_symbol/get_type_of_symbol.
        if let Some(augmentation_decls) = self.ctx.binder.global_augmentations.get(name)
            && !augmentation_decls.is_empty()
        {
            let current_arena: &NodeArena = self.ctx.arena;
            // Process heritage clauses from augmentation declarations that are in
            // the current file's arena. lower_interface_declarations only merges body
            // members, not extends clauses. User augmentations like
            // `interface Number extends ICloneable {}` need their heritage merged.
            //
            // Note: in parallel compilation, ALL augmentations get tagged with an
            // arena (even same-file ones), so we identify current-file augmentations
            // by checking if the arena pointer matches the current arena.
            //
            // We use a lightweight approach here (manual heritage walk + resolve_heritage_symbol)
            // instead of merge_interface_heritage_types, because that function triggers deep type
            // evaluation via resolve_type_for_interface_merge which can cause infinite loops
            // during lib type resolution.
            let current_arena_ptr = current_arena as *const NodeArena;
            let same_file_aug_nodes: Vec<NodeIndex> = augmentation_decls
                .iter()
                .filter(|aug| {
                    aug.arena
                        .as_ref()
                        .is_none_or(|a| Arc::as_ptr(a) == current_arena_ptr)
                })
                .map(|aug| aug.node)
                .collect();

            for &decl_idx in &same_file_aug_nodes {
                let Some(node) = current_arena.get(decl_idx) else {
                    continue;
                };
                let Some(interface) = current_arena.get_interface(node) else {
                    continue;
                };
                let Some(ref heritage_clauses) = interface.heritage_clauses else {
                    continue;
                };

                for &clause_idx in &heritage_clauses.nodes {
                    let Some(clause_node) = current_arena.get(clause_idx) else {
                        continue;
                    };
                    let Some(heritage) = current_arena.get_heritage_clause(clause_node) else {
                        continue;
                    };
                    if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                        continue;
                    }

                    for &type_idx in &heritage.types.nodes {
                        let Some(type_node) = current_arena.get(type_idx) else {
                            continue;
                        };
                        let expr_idx =
                            if let Some(eta) = current_arena.get_expr_type_args(type_node) {
                                eta.expression
                            } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                                if let Some(tr) = current_arena.get_type_ref(type_node) {
                                    tr.type_name
                                } else {
                                    type_idx
                                }
                            } else {
                                type_idx
                            };

                        // resolve_heritage_symbol handles simple identifiers, qualified
                        // names, and property access expressions (e.g., EndGate.ICloneable).
                        let Some(base_sym_id) = self.resolve_heritage_symbol(expr_idx) else {
                            continue;
                        };
                        let base_type = self.get_type_of_symbol(base_sym_id);
                        if base_type == TypeId::ERROR || base_type == TypeId::UNKNOWN {
                            continue;
                        }
                        if let Some(current_type) = lib_type_id {
                            let merged = self.merge_interface_types(current_type, base_type);
                            if merged != current_type {
                                lib_type_id = Some(merged);
                            }
                        }
                    }
                }
            }
        }

        // For generic lib interfaces, we already cached the type params in the
        // interface lowering code above. The type is already correctly lowered
        // and can be returned directly.
        self.ctx
            .lib_type_resolution_cache
            .insert(name.to_string(), lib_type_id);

        // Store in shared cross-file cache for other parallel file checks.
        let has_augmentations = self
            .ctx
            .binder
            .global_augmentations
            .get(name)
            .is_some_and(|v| !v.is_empty());
        if !has_augmentations && let Some(ref shared_cache) = self.ctx.shared_lib_type_cache {
            shared_cache.insert(name.to_string(), lib_type_id);
        }

        lib_type_id
    }
}
