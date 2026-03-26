//! Symbol-level type resolution: resolving symbols to their type-reference types,
//! interface type construction, and type-reference-with-params computation.

use crate::query_boundaries::state::type_resolution as query;
use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;
use tsz_solver::is_compiler_managed_type;

impl<'a> CheckerState<'a> {
    pub(crate) fn type_reference_symbol_type(&mut self, sym_id: SymbolId) -> TypeId {
        let symbol_meta = self.get_cross_file_symbol(sym_id).map(|symbol| {
            (
                symbol.escaped_name.clone(),
                symbol.flags,
                symbol.declarations.clone(),
                symbol.value_declaration,
            )
        });

        if let Some((name, flags, _, _)) = symbol_meta.as_ref() {
            tracing::debug!(
                sym_id = sym_id.0,
                name = %name,
                flags = *flags,
                "type_reference_symbol_type: ENTRY"
            );
        }
        // Recursion depth check: prevents stack overflow from circular
        // interface/class type references (e.g. I<T extends I<T>>)
        if !self.ctx.enter_recursion() {
            return TypeId::ERROR;
        }

        if let Some((ref escaped_name, flags, ref declarations, value_declaration)) = symbol_meta {
            // For classes, return Lazy(DefId) to preserve class names in error messages
            // (e.g., "type MyClass" instead of expanded object shape)
            //
            // Special case: For merged class+namespace symbols, we still need the constructor type
            // to access namespace members via Foo.Bar. But we should still return Lazy for consistency.
            let prefer_interface_type_position =
                (flags & symbol_flags::CLASS) != 0 && (flags & symbol_flags::INTERFACE) != 0;
            if flags & symbol_flags::CLASS != 0 && !prefer_interface_type_position {
                // For classes in TYPE position, return the INSTANCE TYPE directly
                // This is critical for nominal type checking to work correctly
                let instance_type_opt = self.class_instance_type_with_params_from_symbol(sym_id);

                if let Some((instance_type, params)) = instance_type_opt {
                    // Register instance type → DefId so the TypeFormatter can display
                    // the class name (e.g., "A") even when the type was resolved via
                    // cross-file delegation and produced a different TypeId than the
                    // original get_class_instance_type_inner call.
                    let def_id = self.ctx.get_or_create_def_id(sym_id);
                    if !params.is_empty() && self.ctx.get_def_type_params(def_id).is_none() {
                        self.ctx.insert_def_type_params(def_id, params);
                    }
                    self.ctx
                        .definition_store
                        .register_type_to_def(instance_type, def_id);
                    self.ctx
                        .register_class_instance_in_envs(def_id, instance_type);

                    self.ctx.leave_recursion();
                    return instance_type;
                }

                // Fallback: if instance type couldn't be computed, return Lazy
                let lazy_type = self.ctx.create_lazy_type_ref(sym_id);
                self.ctx.leave_recursion();
                return lazy_type;
            }
            if flags & symbol_flags::INTERFACE != 0 {
                if !declarations.is_empty() {
                    // Return Lazy(DefId) for interface type references to preserve
                    // interface names in error messages. Compute and cache the structural
                    // type first so resolve_lazy() can return it for type checking.
                    // For merged interface+namespace symbols, get_type_of_symbol returns the
                    // namespace type (from compute_type_of_symbol's namespace branch). We need
                    // the interface type for type-position usage, so compute it directly from
                    // the interface declarations.
                    let is_merged_with_namespace =
                        flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0;

                    let mut structural_type = if is_merged_with_namespace {
                        // Compute the interface type directly, bypassing get_type_of_symbol
                        // which would return the namespace type for merged symbols.
                        self.compute_interface_type_from_declarations(sym_id)
                    } else {
                        self.get_type_of_symbol(sym_id)
                    };
                    // Cross-file fallback: if the structural type could not be
                    // computed locally, the declarations may be in a different
                    // arena/binder. Delegate to a child checker with the symbol's
                    // home arena instead of silently degrading imported types.
                    if (structural_type == TypeId::UNKNOWN || structural_type == TypeId::ERROR)
                        && let Some(delegate_type) =
                            self.delegate_cross_arena_interface_type(sym_id)
                    {
                        structural_type = delegate_type;
                    }

                    // Step 1.25: Apply module augmentations to the structural type.
                    // If this symbol was reached via an import alias, merge augmentation
                    // members into the base type. This ensures ALL access paths — type
                    // references, Application evaluation, and value-position prototype
                    // access — see the augmented members.
                    if structural_type != TypeId::ERROR
                        && structural_type != TypeId::UNKNOWN
                        && let Some(local_sym) = self.ctx.binder.get_symbol(sym_id)
                        && let Some(module_specifier) = local_sym.import_module.as_ref()
                    {
                        let aug_name = local_sym
                            .import_name
                            .as_deref()
                            .unwrap_or(&local_sym.escaped_name);
                        structural_type = self.apply_module_augmentations(
                            module_specifier,
                            aug_name,
                            structural_type,
                        );
                    }

                    // Step 1.5: Cache type parameters for generic interfaces (Promise<T>, Map<K,V>, etc.)
                    // This must use canonical symbol-based extraction, not raw NodeIndex lookups
                    // against the local arena. Lib and cross-file symbols can share NodeIndex values
                    // with unrelated local declarations, which corrupts cached generic metadata.
                    let def_id = self.ctx.get_or_create_def_id(sym_id);
                    if self.ctx.get_def_type_params(def_id).is_none() {
                        let params = self.get_type_params_for_symbol(sym_id);
                        if !params.is_empty() {
                            self.ctx.insert_def_type_params(def_id, params);
                        }
                    }

                    // Step 1.75: Ensure the DefId→TypeId mapping exists in the TypeEnvironment.
                    // When get_type_of_symbol hits the symbol_types cache (common for cross-file
                    // lib types like ArrayLike, Iterable, Promise), it returns early and skips
                    // the TypeEnvironment registration block. This leaves resolve_lazy(DefId)
                    // returning None, breaking Application type resolution in narrowing contexts
                    // (e.g., type predicate narrowing can't check if ArrayLike<any> is assignable
                    // to { length: unknown } because the Application can't be expanded).
                    if structural_type != TypeId::ERROR
                        && structural_type != TypeId::ANY
                        && structural_type != TypeId::UNKNOWN
                    {
                        // Only register if not already present in type_env
                        let needs_registration = self
                            .ctx
                            .type_env
                            .try_borrow()
                            .is_ok_and(|env| env.get_def(def_id).is_none());
                        if needs_registration {
                            let type_params =
                                self.ctx.get_def_type_params(def_id).unwrap_or_default();
                            if type_params.is_empty() {
                                self.ctx.register_def_in_envs(def_id, structural_type);
                            } else {
                                self.ctx.register_def_with_params_in_envs(
                                    def_id,
                                    structural_type,
                                    type_params,
                                );
                            }
                        }
                    }

                    // For merged interface+namespace symbols, return the structural type
                    // directly instead of Lazy wrapper. The Lazy wrapper causes property
                    // access to incorrectly classify the type as a namespace value,
                    // blocking interface member resolution.
                    //
                    // Also return structural type for interfaces with index signatures
                    // (ObjectWithIndex) — Lazy causes issues with flow analysis there.
                    //
                    // Also return Unknown directly when cross-file interface resolution
                    // fails — wrapping in Lazy(DefId) would create an unresolvable ref.
                    if is_merged_with_namespace
                        || query::is_object_with_index_type(self.ctx.types, structural_type)
                        || structural_type == TypeId::UNKNOWN
                    {
                        self.ctx.leave_recursion();
                        return structural_type;
                    }

                    // Return Lazy wrapper for regular interfaces
                    let lazy_type = self.ctx.create_lazy_type_ref(sym_id);
                    self.ctx.leave_recursion();
                    return lazy_type;
                }
                if value_declaration.is_some() {
                    let result = self.get_type_of_interface(value_declaration);
                    self.ctx.leave_recursion();
                    return result;
                }
            }

            // For type aliases, resolve the body type using the correct arena.
            // Search declarations[] for the actual type alias decl (merged symbols
            // may have value_declaration pointing to a var decl, not the type alias).
            if flags & symbol_flags::TYPE_ALIAS != 0 {
                let has_type_alias_decl = declarations.iter().any(|&d| {
                    self.ctx
                        .arena
                        .get(d)
                        .and_then(|n| {
                            if n.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                                // Verify name matches to prevent NodeIndex collisions
                                let type_alias = self.ctx.arena.get_type_alias(n)?;
                                let name = self.ctx.arena.get_identifier_text(type_alias.name)?;
                                Some(name == escaped_name.as_str())
                            } else {
                                Some(false)
                            }
                        })
                        .unwrap_or(false)
                }) || value_declaration.is_some()
                    || !declarations.is_empty();
                if has_type_alias_decl {
                    // Return structural type directly for type aliases (not Lazy) so
                    // conditional types are fully resolved during assignability checking.
                    let mut structural_type = self.get_type_of_symbol(sym_id);
                    if (structural_type == TypeId::ANY
                        || structural_type == TypeId::UNKNOWN
                        || structural_type == TypeId::ERROR)
                        && let Some((delegate_type, _)) =
                            self.delegate_cross_arena_symbol_resolution(sym_id)
                        && delegate_type != TypeId::UNKNOWN
                        && delegate_type != TypeId::ERROR
                    {
                        structural_type = delegate_type;
                    }
                    let preserve_deferred_keyof =
                        tsz_solver::type_queries::get_keyof_type(self.ctx.types, structural_type)
                            .is_some();
                    let structural_type = if structural_type != TypeId::ERROR
                        && structural_type != TypeId::UNKNOWN
                        && !preserve_deferred_keyof
                        && !tsz_solver::type_queries::contains_type_parameters_db(
                            self.ctx.types,
                            structural_type,
                        ) {
                        self.evaluate_type_with_resolution(structural_type)
                    } else {
                        structural_type
                    };
                    // Register for alias-name formatting in diagnostics
                    self.ctx
                        .register_resolved_type(sym_id, structural_type, Vec::new());
                    self.ctx.leave_recursion();
                    return structural_type;
                }
            }
        }
        // For ALIAS symbols (e.g., `import b = a.c`), resolve to the target
        // symbol and re-enter type_reference_symbol_type if the target is a class
        // or interface. This ensures we get the instance type, not the constructor
        // type, when the alias is used in a type position like `x: b`.
        if let Some((_, flags, _, _)) = symbol_meta.as_ref()
            && flags & symbol_flags::ALIAS != 0
        {
            let mut visited = Vec::new();
            let alias_result = self.resolve_alias_symbol(sym_id, &mut visited);
            let is_default_import_alias = self
                .get_cross_file_symbol(sym_id)
                .and_then(|symbol| symbol.import_name.as_deref())
                == Some("default");
            if let Some(target_sym_id) = alias_result
                && target_sym_id != sym_id
            {
                let target_flags = self
                    .get_cross_file_symbol(target_sym_id)
                    .map(|s| s.flags)
                    .unwrap_or(0);
                if target_flags & symbol_flags::CLASS != 0
                    || target_flags & symbol_flags::INTERFACE != 0
                    || target_flags & symbol_flags::TYPE_ALIAS != 0
                    || target_flags & symbol_flags::ENUM != 0
                    || target_flags & symbol_flags::TYPE_PARAMETER != 0
                {
                    self.ctx.leave_recursion();
                    return self.type_reference_symbol_type(target_sym_id);
                }

                // For synthetic default exports whose value_declaration is a property
                // access expression (e.g., `export default C.B` where B is both a
                // static property and an interface), resolve the type meaning of the
                // property access.
                if target_flags & symbol_flags::EXPORT_VALUE != 0
                    && let Some(type_id) =
                        self.resolve_default_export_property_type_meaning(target_sym_id)
                {
                    self.ctx.leave_recursion();
                    return type_id;
                }
            }

            let current_flags = self
                .get_cross_file_symbol(sym_id)
                .map(|s| s.flags)
                .unwrap_or(0);
            if current_flags & symbol_flags::EXPORT_VALUE != 0
                && let Some(type_id) = self.resolve_default_export_property_type_meaning(sym_id)
            {
                self.ctx.leave_recursion();
                return type_id;
            }

            // Fallback: resolve_alias_symbol may fail for cross-file default imports
            // when the relative module specifier doesn't match the binder's module_exports
            // keys. Also retry for default imports when local alias resolution lands
            // on a runtime-only target symbol so we can still inspect the synthetic
            // default export's type meaning (e.g., `export default C.B` where B is
            // both a static property and an interface).
            let should_try_cross_file_default = alias_result.is_none()
                || alias_result == Some(sym_id)
                || (is_default_import_alias
                    && alias_result.is_some_and(|target_sym_id| {
                        let target_flags = self
                            .get_cross_file_symbol(target_sym_id)
                            .map(|s| s.flags)
                            .unwrap_or(0);
                        target_flags
                            & (symbol_flags::CLASS
                                | symbol_flags::INTERFACE
                                | symbol_flags::EXPORT_VALUE)
                            == 0
                    }));
            if should_try_cross_file_default {
                let cross_file_result = self.resolve_import_alias_cross_file(sym_id);
                if let Some(target_sym_id) = cross_file_result {
                    let target_flags = self
                        .get_cross_file_symbol(target_sym_id)
                        .map(|s| s.flags)
                        .unwrap_or(0);
                    if target_flags & symbol_flags::CLASS != 0
                        || target_flags & symbol_flags::INTERFACE != 0
                        || target_flags & symbol_flags::TYPE_ALIAS != 0
                        || target_flags & symbol_flags::ENUM != 0
                        || target_flags & symbol_flags::TYPE_PARAMETER != 0
                    {
                        self.ctx.leave_recursion();
                        return self.type_reference_symbol_type(target_sym_id);
                    }
                    if target_flags & symbol_flags::EXPORT_VALUE != 0 {
                        let prop_result =
                            self.resolve_default_export_property_type_meaning(target_sym_id);
                        if let Some(type_id) = prop_result {
                            self.ctx.leave_recursion();
                            return type_id;
                        }
                    }
                }
            }
        }

        let result = self.get_type_of_symbol(sym_id);
        // TYPE_ALIAS + ALIAS merge: prefer the type alias body in type reference position
        let result = self
            .ctx
            .import_type_alias_types
            .get(&sym_id)
            .copied()
            .unwrap_or(result);
        self.ctx.leave_recursion();
        result
    }

    /// Resolve the type meaning of a synthetic default export whose `value_declaration`
    /// is a property access expression.
    ///
    /// For `export default C.B` where `C` is a class/namespace and `B` is both a
    /// static property and a type (interface/type alias), the default export carries
    /// the value meaning (`number`).  When the import is used as a type reference,
    /// we need the type meaning (the interface `C.B`).
    fn resolve_default_export_property_type_meaning(
        &mut self,
        target_sym_id: SymbolId,
    ) -> Option<TypeId> {
        let lib_binders: Vec<_> = self
            .ctx
            .lib_contexts
            .iter()
            .map(|lc| std::sync::Arc::clone(&lc.binder))
            .collect();

        let symbol = self.get_cross_file_symbol(target_sym_id)?;
        let value_decl = symbol.value_declaration;
        if value_decl.is_none() {
            return None;
        }

        // Find the arena containing the value declaration (may be cross-file).
        let file_idx = self.ctx.resolve_symbol_file_index(target_sym_id);
        let arena: &NodeArena = if let Some(file_idx) = file_idx {
            self.ctx.get_arena_for_file(file_idx as u32)
        } else {
            self.ctx.arena
        };

        let node = arena.get(value_decl)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = arena.get_access_expr(node)?;
        let name_node = arena.get(access.name_or_argument)?;
        let name_ident = arena.get_identifier(name_node)?;
        let member_name = &name_ident.escaped_text;

        // Resolve the base expression (e.g., `C`) to its symbol.
        let base_node = arena.get(access.expression)?;
        let base_ident = arena.get_identifier(base_node)?;
        let base_name = &base_ident.escaped_text;

        // Look up the base symbol in the source file's binder.
        let source_binder = if let Some(file_idx) = file_idx {
            self.ctx.get_binder_for_file(file_idx)?
        } else {
            self.ctx.binder
        };

        let base_sym_id = source_binder.file_locals.get(base_name)?;
        let base_symbol = source_binder.get_symbol_with_libs(base_sym_id, &lib_binders)?;

        let symbol_named_member =
            |symbol: &tsz_binder::Symbol, member_name: &str| -> Option<SymbolId> {
                if let Some(exports) = symbol.exports.as_ref()
                    && let Some(sym_id) = exports.get(member_name)
                {
                    return Some(sym_id);
                }
                if let Some(members) = symbol.members.as_ref()
                    && let Some(sym_id) = members.get(member_name)
                {
                    return Some(sym_id);
                }
                None
            };

        let mut member_sym_id = symbol_named_member(base_symbol, member_name)?;
        let mut member_symbol = source_binder.get_symbol_with_libs(member_sym_id, &lib_binders)?;
        if member_symbol.flags
            & (symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS | symbol_flags::CLASS)
            == 0
        {
            // Namespace-merge fallback: for `export default C.B`, the merged class symbol
            // can surface the static VALUE member (`C.B: number`) first, while the type
            // meaning lives on the split namespace symbol (`namespace C { export interface B }`).
            // Mirror the `export =` namespace-merge fallback and search sibling symbols
            // with the same base name for a TYPE-meaning member.
            for &candidate_sym_id in source_binder.get_symbols().find_all_by_name(base_name) {
                if candidate_sym_id == base_sym_id {
                    continue;
                }
                let Some(candidate_symbol) =
                    source_binder.get_symbol_with_libs(candidate_sym_id, &lib_binders)
                else {
                    continue;
                };
                if (candidate_symbol.flags
                    & (symbol_flags::MODULE
                        | symbol_flags::NAMESPACE_MODULE
                        | symbol_flags::VALUE_MODULE))
                    == 0
                {
                    continue;
                }
                let Some(candidate_member_id) = symbol_named_member(candidate_symbol, member_name)
                else {
                    continue;
                };
                let Some(candidate_member_symbol) =
                    source_binder.get_symbol_with_libs(candidate_member_id, &lib_binders)
                else {
                    continue;
                };
                if candidate_member_symbol.flags
                    & (symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS | symbol_flags::CLASS)
                    == 0
                {
                    continue;
                }
                member_sym_id = candidate_member_id;
                member_symbol = candidate_member_symbol;
                break;
            }
            if member_symbol.flags
                & (symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS | symbol_flags::CLASS)
                == 0
            {
                return None;
            }
        }

        // Record cross-file symbol tracking if necessary.
        if let Some(file_idx) = file_idx {
            self.ctx
                .register_symbol_file_target(member_sym_id, file_idx);
        }

        // For cross-file symbols, use delegation to compute the type in the
        // correct arena context. Calling type_reference_symbol_type directly
        // would use the current file's arena, causing NodeIndex collisions and,
        // for class members, can lose the instance-side type and fall back to
        // the constructor object type.
        if file_idx.is_some() {
            if member_symbol.flags & symbol_flags::CLASS != 0
                && let Some((instance_type, params)) =
                    self.delegate_cross_arena_class_instance_type(member_sym_id)
            {
                let def_id = self.ctx.get_or_create_def_id(member_sym_id);
                if !params.is_empty() && self.ctx.get_def_type_params(def_id).is_none() {
                    self.ctx.insert_def_type_params(def_id, params);
                }
                self.ctx
                    .definition_store
                    .register_type_to_def(instance_type, def_id);
                self.ctx
                    .register_class_instance_in_envs(def_id, instance_type);
                return Some(instance_type);
            }
            if let Some(delegate_type) = self.delegate_cross_arena_interface_type(member_sym_id) {
                return Some(delegate_type);
            }
        }

        Some(self.type_reference_symbol_type(member_sym_id))
    }

    /// Resolve an import alias to its target symbol using the cross-file resolution
    /// infrastructure.
    ///
    /// This is used as a fallback when `resolve_alias_symbol` (which relies on the
    /// binder's `module_exports`) fails for cross-file imports. Uses the checker's
    /// `resolve_import_alias_and_register` which resolves relative module specifiers
    /// from the declaring file's perspective.
    fn resolve_import_alias_cross_file(&self, sym_id: SymbolId) -> Option<SymbolId> {
        let lib_binders: Vec<_> = self
            .ctx
            .lib_contexts
            .iter()
            .map(|lc| std::sync::Arc::clone(&lc.binder))
            .collect();
        let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)?;
        if symbol.flags & symbol_flags::ALIAS == 0 {
            return None;
        }
        let module_specifier = symbol.import_module.as_ref()?;
        let import_name = symbol
            .import_name
            .as_deref()
            .unwrap_or(&symbol.escaped_name);

        // Use current_file_idx as the source for resolving relative specifiers,
        // since locally-declared import symbols may not have decl_file_idx set.
        let source_file_idx = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .unwrap_or(self.ctx.current_file_idx);

        let target_idx = self
            .ctx
            .resolve_import_target_from_file(source_file_idx, module_specifier)?;
        let target_binder = self.ctx.get_binder_for_file(target_idx)?;
        let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
        let file_name = &target_arena.source_files.first()?.file_name;

        // Try module_exports first (keyed by filename), then file_locals.
        let target_sym_id = target_binder
            .module_exports
            .get(file_name)
            .and_then(|exports| exports.get(import_name))
            .or_else(|| target_binder.file_locals.get(import_name))?;

        self.ctx
            .register_symbol_file_target(target_sym_id, target_idx);
        Some(target_sym_id)
    }

    /// Compute the interface structural type from declarations, bypassing `get_type_of_symbol`.
    ///
    /// For merged interface+namespace symbols, `get_type_of_symbol` returns the namespace
    /// type (via the MODULE branch in `compute_type_of_symbol`). This helper computes the
    /// interface type directly from the interface declarations, which is needed when the
    /// symbol is used in type position (e.g., `var f: Foo` where Foo is interface+namespace).
    pub(crate) fn compute_interface_type_from_declarations(&mut self, sym_id: SymbolId) -> TypeId {
        use tsz_lowering::TypeLowering;

        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return TypeId::ERROR;
        };
        let declarations = symbol.declarations.clone();

        if declarations.is_empty() {
            return TypeId::ERROR;
        }

        let local_interface_decls: Vec<_> = declarations
            .iter()
            .copied()
            .filter(|&decl_idx| {
                self.ctx
                    .arena
                    .get(decl_idx)
                    .and_then(|node| self.ctx.arena.get_interface(node))
                    .is_some()
                    && !self
                        .ctx
                        .binder
                        .declaration_arenas
                        .get(&(sym_id, decl_idx))
                        .is_some_and(|arenas| {
                            arenas
                                .iter()
                                .any(|arena| !std::ptr::eq(arena.as_ref(), self.ctx.arena))
                        })
            })
            .collect();
        if local_interface_decls.len() == declarations.len() {
            let mut merged = TypeId::ERROR;
            for decl_idx in local_interface_decls {
                let interface_type = self.get_type_of_interface(decl_idx);
                merged = if merged == TypeId::ERROR {
                    interface_type
                } else {
                    self.merge_interface_types(merged, interface_type)
                };
            }
            return merged;
        }

        // Pre-compute computed property names that the lowering can't resolve from AST alone.
        // This handles cases like `[k]` where k is a `const` unique symbol variable.
        let computed_names = self.precompute_computed_property_names(&declarations);
        let prewarmed_type_params = self.prewarm_member_type_reference_params(&declarations);

        // Get type parameters from the first interface declaration
        let first_decl = declarations.first().copied().unwrap_or(NodeIndex::NONE);
        let mut params = Vec::new();
        let mut updates = Vec::new();
        if first_decl.is_some()
            && let Some(node) = self.ctx.arena.get(first_decl)
            && let Some(interface) = self.ctx.arena.get_interface(node)
        {
            (params, updates) = self.push_type_parameters(&interface.type_parameters);
        }

        let type_param_bindings = self.get_type_param_bindings();
        let type_resolver = |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
        // Stable-identity helper: prefer Lazy(DefId) over Ref(SymbolRef)
        let def_id_resolver = |node_idx: NodeIndex| self.resolve_def_id_for_lowering(node_idx);
        let value_resolver = |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
        let computed_name_resolver = |expr_idx: NodeIndex| -> Option<tsz_common::Atom> {
            computed_names.get(&expr_idx).copied()
        };
        let lazy_type_params_resolver = |def_id: tsz_solver::def::DefId| {
            prewarmed_type_params
                .get(&def_id)
                .cloned()
                .or_else(|| self.ctx.get_def_type_params(def_id))
        };
        let lowering = TypeLowering::with_hybrid_resolver(
            self.ctx.arena,
            self.ctx.types,
            &type_resolver,
            &def_id_resolver,
            &value_resolver,
        )
        .with_type_param_bindings(type_param_bindings)
        .with_computed_name_resolver(&computed_name_resolver)
        .with_lazy_type_params_resolver(&lazy_type_params_resolver);
        let interface_type =
            lowering.lower_interface_declarations_with_symbol(&declarations, sym_id);

        self.pop_type_parameters(updates);
        let _ = params; // params are not needed for this path

        self.merge_interface_heritage_types(&declarations, interface_type)
    }

    pub(crate) fn prewarm_member_type_reference_params(
        &mut self,
        declarations: &[NodeIndex],
    ) -> rustc_hash::FxHashMap<tsz_solver::def::DefId, Vec<tsz_solver::TypeParamInfo>> {
        let mut stack = Vec::new();
        let mut params_by_def = rustc_hash::FxHashMap::default();

        for &decl_idx in declarations {
            stack.push(decl_idx);

            while let Some(node_idx) = stack.pop() {
                let Some(node) = self.ctx.arena.get(node_idx) else {
                    continue;
                };

                if node.kind == syntax_kind_ext::TYPE_REFERENCE
                    && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
                {
                    let has_type_args = type_ref
                        .type_arguments
                        .as_ref()
                        .is_some_and(|args| !args.nodes.is_empty());
                    if !has_type_args
                        && let Some(sym_id_raw) =
                            self.resolve_type_symbol_for_lowering(type_ref.type_name)
                    {
                        let sym_id = tsz_binder::SymbolId(sym_id_raw);
                        let def_id = self.ctx.get_or_create_def_id(sym_id);
                        let params = self.get_type_params_for_symbol(sym_id);
                        if !params.is_empty() {
                            params_by_def.insert(def_id, params);
                        }
                    }
                }

                stack.extend(self.ctx.arena.get_children(node_idx));
            }
        }

        params_by_def
    }

    /// Pre-compute property names for computed property name expressions in interface members.
    /// Iterates over all members of all declarations, finds `COMPUTED_PROPERTY_NAME` nodes,
    /// evaluates the expression type, and builds a map from expression `NodeIndex` to Atom.
    pub(crate) fn precompute_computed_property_names(
        &mut self,
        declarations: &[NodeIndex],
    ) -> rustc_hash::FxHashMap<NodeIndex, tsz_common::Atom> {
        use tsz_parser::parser::syntax_kind_ext;
        let mut map = rustc_hash::FxHashMap::default();
        for &decl_idx in declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = self.ctx.arena.get_interface(node) else {
                continue;
            };
            for &member_idx in &interface.members.nodes {
                let Some(member) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                // Get the name node from signature or accessor
                let name_idx = if let Some(sig) = self.ctx.arena.get_signature(member) {
                    sig.name
                } else if let Some(acc) = self.ctx.arena.get_accessor(member) {
                    acc.name
                } else {
                    continue;
                };
                let Some(name_node) = self.ctx.arena.get(name_idx) else {
                    continue;
                };
                if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    continue;
                }
                let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
                    continue;
                };
                // Set checking_computed_property_name so that TS2467 (type parameter
                // reference in computed property name) is properly emitted.
                let prev = self.ctx.checking_computed_property_name;
                self.ctx.checking_computed_property_name = Some(name_idx);
                // Preserve literal types so that string literal expressions like
                // ["computed"] resolve to the literal type "computed" rather than
                // widening to `string`. Without this, get_literal_property_name
                // cannot extract the property name from the widened type.
                let prev_preserve = self.ctx.preserve_literal_types;
                self.ctx.preserve_literal_types = true;
                // Evaluate the expression type and get the property name
                let expr_type = self.get_type_of_node(computed.expression);
                self.ctx.preserve_literal_types = prev_preserve;
                self.ctx.checking_computed_property_name = prev;
                if let Some(name) =
                    tsz_solver::type_queries::get_literal_property_name(self.ctx.types, expr_type)
                {
                    map.insert(computed.expression, name);
                }
            }
        }
        map
    }

    /// Resolve a symbol to its structural type and return a `Lazy(DefId)` reference.
    ///
    /// This is the canonical stable-identity helper that consolidates the common
    /// two-step pattern:
    ///   1. `type_reference_symbol_type(sym_id)` — ensures the symbol's body is
    ///      materialized in `type_env`
    ///   2. `ctx.create_lazy_type_ref(sym_id)` — creates `TypeData::Lazy(DefId)`
    ///
    /// Use this in type literal and type reference resolution paths instead of
    /// manually calling both steps.
    pub(crate) fn resolve_symbol_as_lazy_type(&mut self, sym_id: SymbolId) -> TypeId {
        let _ = self.type_reference_symbol_type(sym_id);
        self.ctx.create_lazy_type_ref(sym_id)
    }

    /// Like `type_reference_symbol_type` but also returns the type parameters used.
    ///
    /// This is critical for Application type evaluation: when instantiating a generic
    /// type, we need the body type AND the type parameters to be built from the SAME
    /// call to `push_type_parameters`, so the `TypeIds` in the body match those in the
    /// substitution. Otherwise, substitution fails because the `TypeIds` don't match.
    pub(crate) fn type_reference_symbol_type_with_params(
        &mut self,
        sym_id: SymbolId,
    ) -> (TypeId, Vec<tsz_solver::TypeParamInfo>) {
        use tsz_lowering::TypeLowering;

        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
            tracing::debug!(
                sym_id = sym_id.0,
                name = %symbol.escaped_name,
                flags = symbol.flags,
                num_decls = symbol.declarations.len(),
                has_value_decl = symbol.value_declaration.is_some(),
                "type_reference_symbol_type_with_params: ENTRY"
            );
        }

        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
            // For classes, use class_instance_type_with_params_from_symbol which
            // returns both the instance type AND the type params used to build it
            let prefer_interface_type_position = symbol.flags & symbol_flags::CLASS != 0
                && symbol.flags & symbol_flags::INTERFACE != 0;

            if symbol.flags & symbol_flags::CLASS != 0
                && !prefer_interface_type_position
                && let Some((instance_type, params)) =
                    self.class_instance_type_with_params_from_symbol(sym_id)
            {
                // Store type parameters for DefId-based resolution
                if let Some(def_id) = self.ctx.get_existing_def_id(sym_id) {
                    self.ctx.insert_def_type_params(def_id, params.clone());
                }
                return (instance_type, params);
            }

            // When a symbol has both TYPE_ALIAS and INTERFACE flags (e.g., local
            // `type Request<T> = ...` merged with lib's `interface Request`), the
            // local type alias should take precedence. Check whether the TYPE_ALIAS
            // declaration lives in the current arena and skip the INTERFACE path if so.
            let prefer_type_alias_over_interface = symbol.flags & symbol_flags::TYPE_ALIAS != 0
                && symbol.flags & symbol_flags::INTERFACE != 0
                && symbol.declarations.iter().any(|&d| {
                    self.ctx
                        .arena
                        .get(d)
                        .and_then(|n| {
                            if n.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                                let type_alias = self.ctx.arena.get_type_alias(n)?;
                                let name = self.ctx.arena.get_identifier_text(type_alias.name)?;
                                Some(name == symbol.escaped_name.as_str())
                            } else {
                                Some(false)
                            }
                        })
                        .unwrap_or(false)
                });

            // For interfaces, lower with type parameters and return both
            if symbol.flags & symbol_flags::INTERFACE != 0
                && !symbol.declarations.is_empty()
                && !prefer_type_alias_over_interface
            {
                // Build per-declaration arena pairs for multi-arena support
                // (e.g. Promise has declarations in lib.es5.d.ts, lib.es2018.promise.d.ts, etc.)
                let fallback_arena: &NodeArena = self
                    .ctx
                    .binder
                    .symbol_arenas
                    .get(&sym_id)
                    .map_or(self.ctx.arena, |arena| arena.as_ref());

                let has_declaration_arenas = symbol.declarations.iter().any(|&decl_idx| {
                    self.ctx
                        .binder
                        .declaration_arenas
                        .contains_key(&(sym_id, decl_idx))
                });
                let needs_text_based_resolution =
                    has_declaration_arenas || !std::ptr::eq(fallback_arena, self.ctx.arena);

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
                        } else if has_declaration_arenas {
                            // This symbol has lib declarations (with declaration_arenas
                            // entries) but THIS declaration has no entry — it was added
                            // during user-file binding and lives in the user arena.
                            vec![(decl_idx, self.ctx.arena)]
                        } else {
                            vec![(decl_idx, fallback_arena)]
                        }
                    })
                    .collect();

                // Get type parameters from first declaration that has them,
                // along with the arena they came from (needed for lib interfaces).
                let type_params_with_arena: Option<(tsz_parser::parser::NodeList, &NodeArena)> =
                    decls_with_arenas.iter().find_map(|(decl_idx, arena)| {
                        arena
                            .get(*decl_idx)
                            .and_then(|node| arena.get_interface(node))
                            .and_then(|iface| {
                                iface.type_parameters.clone().map(|tpl| (tpl, *arena))
                            })
                    });
                let type_params_list = type_params_with_arena.as_ref().map(|(tpl, _)| tpl.clone());
                let namespace_prefix = decls_with_arenas.iter().find_map(|(decl_idx, arena)| {
                    let node = arena.get(*decl_idx)?;
                    arena.get_interface(node)?;

                    let mut parent = arena
                        .get_extended(*decl_idx)
                        .map_or(NodeIndex::NONE, |info| info.parent);
                    let mut prefixes = Vec::new();
                    while !parent.is_none() {
                        let parent_node = arena.get(parent)?;
                        if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                            && let Some(module) = arena.get_module(parent_node)
                            && let Some(name_node) = arena.get(module.name)
                            && name_node.kind == SyntaxKind::Identifier as u16
                            && let Some(name_ident) = arena.get_identifier(name_node)
                        {
                            prefixes.push(name_ident.escaped_text.clone());
                        }
                        parent = arena
                            .get_extended(parent)
                            .map_or(NodeIndex::NONE, |info| info.parent);
                    }

                    (!prefixes.is_empty())
                        .then(|| prefixes.into_iter().rev().collect::<Vec<_>>().join("."))
                });

                // Pre-compute computed property names for declarations in the
                // current arena. This handles cases like `[FOO_SYMBOL]?: number`
                // inside `declare global { interface Promise<T> { ... } }`, where
                // TypeLowering alone can't resolve the computed expression.
                let computed_names = self.precompute_computed_property_names(&symbol.declarations);

                // Push type params, lower interface, pop type params.
                // push_type_parameters uses self.ctx.arena (user arena) to read
                // type param nodes. For lib interfaces the nodes are in a lib arena,
                // so push_type_parameters may return empty params. In that case,
                // extract params directly from the lib arena.
                let (mut params, updates) = self.push_type_parameters(&type_params_list);
                if params.is_empty() {
                    // For lib/multi-arena interfaces, local push_type_parameters may fail
                    // to read type parameter nodes from self.ctx.arena. Reuse canonical
                    // type-parameter extraction so defaults/constraints are preserved.
                    let canonical_params = self.get_type_params_for_symbol(sym_id);
                    if !canonical_params.is_empty() {
                        params = canonical_params;
                    }
                }

                let type_param_bindings = self.get_type_param_bindings();

                let mut prewarmed_lazy_type_params = rustc_hash::FxHashMap::default();
                for (decl_idx, decl_arena) in &decls_with_arenas {
                    let mut stack = vec![*decl_idx];
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
                                && name_node.kind == SyntaxKind::Identifier as u16
                                && let Some(name) =
                                    decl_arena.get_identifier_text(type_ref.type_name)
                            {
                                self.prime_lib_type_params(name);
                                if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
                                    let def_id = self.ctx.get_or_create_def_id(sym_id);
                                    if let Some(params) = self.ctx.get_def_type_params(def_id)
                                        && !params.is_empty()
                                    {
                                        prewarmed_lazy_type_params.insert(def_id, params);
                                    }
                                }
                            }
                        }
                        stack.extend(decl_arena.get_children(node_idx));
                    }
                }
                let binder = &self.ctx.binder;
                let lib_binders = self.get_lib_binders();
                // For multi-arena interfaces (e.g. PromiseConstructor declared in
                // lib.es2015.promise.d.ts AND lib.es2015.iterable.d.ts), the resolver
                // must look up identifier text from ALL declaration arenas, not just
                // self.ctx.arena. NodeIndices from different arenas may collide, so
                // using self.ctx.arena alone could resolve to the wrong node.
                let multi_arena_resolve = |node_idx: NodeIndex| -> Option<SymbolId> {
                    // Use checker-accessible compiler-managed type detection helper.

                    // Try each declaration arena to find the identifier text
                    let ident_name = decls_with_arenas
                        .iter()
                        .find_map(|(_, arena)| arena.get_identifier_text(node_idx))
                        .or_else(|| fallback_arena.get_identifier_text(node_idx))?;
                    if is_compiler_managed_type(ident_name) {
                        return None;
                    }
                    let sym_id = binder.file_locals.get(ident_name)?;
                    let symbol = binder.get_symbol_with_libs(sym_id, &lib_binders)?;
                    ((symbol.flags & symbol_flags::TYPE) != 0).then_some(sym_id)
                };

                let type_resolver = |node_idx: NodeIndex| -> Option<u32> {
                    if needs_text_based_resolution {
                        multi_arena_resolve(node_idx).map(|s| s.0)
                    } else {
                        self.resolve_type_symbol_for_lowering(node_idx)
                    }
                };
                let value_resolver =
                    |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);

                // Stable-identity helper for DefId-based resolution
                let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> {
                    if needs_text_based_resolution {
                        multi_arena_resolve(node_idx)
                            .map(|sym_id| self.ctx.get_or_create_def_id(sym_id))
                    } else {
                        self.resolve_def_id_for_lowering(node_idx)
                    }
                };
                let name_resolver = |type_name: &str| -> Option<tsz_solver::def::DefId> {
                    namespace_prefix
                        .as_ref()
                        .and_then(|prefix| {
                            let mut scoped =
                                String::with_capacity(prefix.len() + 1 + type_name.len());
                            scoped.push_str(prefix);
                            scoped.push('.');
                            scoped.push_str(type_name);
                            self.resolve_entity_name_text_to_def_id_for_lowering(&scoped)
                        })
                        .or_else(|| self.resolve_entity_name_text_to_def_id_for_lowering(type_name))
                };

                let computed_name_resolver = |expr_idx: NodeIndex| -> Option<tsz_common::Atom> {
                    computed_names.get(&expr_idx).copied()
                };
                let lazy_type_params_resolver = |def_id: tsz_solver::def::DefId| {
                    prewarmed_lazy_type_params
                        .get(&def_id)
                        .cloned()
                        .or_else(|| self.ctx.get_def_type_params(def_id))
                };
                let lowering = TypeLowering::with_hybrid_resolver(
                    fallback_arena,
                    self.ctx.types,
                    &type_resolver,
                    &def_id_resolver,
                    &value_resolver,
                )
                .with_type_param_bindings(type_param_bindings)
                .with_lazy_type_params_resolver(&lazy_type_params_resolver)
                .with_name_def_id_resolver(&name_resolver)
                .with_computed_name_resolver(&computed_name_resolver)
                .with_preferred_self_reference(
                    symbol.escaped_name.clone(),
                    self.ctx.get_or_create_def_id(sym_id),
                );

                // Use merged interface lowering for multi-arena declarations
                let has_multi_arenas = has_declaration_arenas;
                let interface_type = if has_multi_arenas {
                    let (ty, _merged_params) = lowering
                        .lower_merged_interface_declarations_with_symbol(
                            &decls_with_arenas,
                            Some(sym_id),
                        );
                    ty
                } else {
                    lowering.lower_interface_declarations_with_symbol(&symbol.declarations, sym_id)
                };
                // First try the standard heritage merge (works for user-arena interfaces).
                let mut merged =
                    self.merge_interface_heritage_types(&symbol.declarations, interface_type);
                // If standard merge didn't propagate heritage (common for lib interfaces
                // whose declarations live in lib arenas invisible to self.ctx.arena),
                // fall back to the lib-aware heritage merge.
                if merged == interface_type {
                    let name = symbol.escaped_name.clone();
                    merged = self.merge_lib_interface_heritage(merged, &name);
                }

                self.pop_type_parameters(updates);
                if let Some(def_id) = self.ctx.get_existing_def_id(sym_id) {
                    let canonical_params = self.get_type_params_for_symbol(sym_id);
                    if !canonical_params.is_empty() {
                        self.ctx.insert_def_type_params(def_id, canonical_params);
                    } else {
                        self.ctx.insert_def_type_params(def_id, params.clone());
                    }
                }
                return (merged, params);
            }

            // For type aliases, get body type and params together
            if symbol.flags & symbol_flags::TYPE_ALIAS != 0 {
                // When a type alias name collides with a global value declaration
                // (e.g., user-defined `type Proxy<T>` vs global `declare var Proxy`),
                // the merged symbol's value_declaration points to the var decl, not the
                // type alias. We must search declarations[] to find the actual type alias.
                let decl_idx = symbol
                    .declarations
                    .iter()
                    .copied()
                    .find(|&d| {
                        self.ctx
                            .arena
                            .get(d)
                            .and_then(|n| {
                                if n.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                                    // Verify name matches to prevent NodeIndex collisions
                                    let type_alias = self.ctx.arena.get_type_alias(n)?;
                                    let name =
                                        self.ctx.arena.get_identifier_text(type_alias.name)?;
                                    Some(name == symbol.escaped_name.as_str())
                                } else {
                                    Some(false)
                                }
                            })
                            .unwrap_or(false)
                    })
                    .unwrap_or_else(|| {
                        if symbol.value_declaration.is_some() {
                            symbol.value_declaration
                        } else {
                            symbol
                                .declarations
                                .first()
                                .copied()
                                .unwrap_or(NodeIndex::NONE)
                        }
                    });

                if decl_idx.is_some() {
                    // Try user arena first (fast path for user-defined type aliases)
                    if let Some(node) = self.ctx.arena.get(decl_idx)
                        && let Some(type_alias) = self.ctx.arena.get_type_alias(node)
                    {
                        let (params, updates) =
                            self.push_type_parameters(&type_alias.type_parameters);
                        self.prime_type_reference_params_in_alias_body(
                            self.ctx.arena,
                            type_alias.type_node,
                        );
                        let alias_type = self.get_type_from_type_node(type_alias.type_node);
                        self.pop_type_parameters(updates);
                        if let Some(def_id) = self.ctx.get_existing_def_id(sym_id) {
                            self.ctx.insert_def_type_params(def_id, params.clone());
                        }
                        return (alias_type, params);
                    }

                    // For lib type aliases (e.g. Awaited<T>), use TypeLowering with the
                    // correct lib arena. get_type_from_type_node uses self.ctx.arena which
                    // doesn't have lib nodes, so we must use TypeLowering directly.
                    let lib_arena = self
                        .ctx
                        .binder
                        .declaration_arenas
                        .get(&(sym_id, decl_idx))
                        .and_then(|v| v.first())
                        .map(std::convert::AsRef::as_ref)
                        .or_else(|| {
                            self.ctx
                                .binder
                                .symbol_arenas
                                .get(&sym_id)
                                .map(std::convert::AsRef::as_ref)
                        });

                    if let Some(lib_arena) = lib_arena
                        && let Some(node) = lib_arena.get(decl_idx)
                        && let Some(type_alias) = lib_arena.get_type_alias(node)
                    {
                        let type_param_bindings = self.get_type_param_bindings();
                        let binder = &self.ctx.binder;
                        let lib_binders = self.get_lib_binders();
                        let namespace_prefix = {
                            let mut parent = lib_arena
                                .get_extended(decl_idx)
                                .map_or(NodeIndex::NONE, |info| info.parent);
                            let mut prefixes = Vec::new();
                            while !parent.is_none() {
                                let Some(parent_node) = lib_arena.get(parent) else {
                                    break;
                                };
                                if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                                    && let Some(module) = lib_arena.get_module(parent_node)
                                    && let Some(name_node) = lib_arena.get(module.name)
                                    && name_node.kind == SyntaxKind::Identifier as u16
                                    && let Some(name_ident) = lib_arena.get_identifier(name_node)
                                {
                                    prefixes.push(name_ident.escaped_text.clone());
                                }
                                parent = lib_arena
                                    .get_extended(parent)
                                    .map_or(NodeIndex::NONE, |info| info.parent);
                            }
                            (!prefixes.is_empty())
                                .then(|| prefixes.into_iter().rev().collect::<Vec<_>>().join("."))
                        };
                        let resolve_type_name = |name: &str| -> Option<SymbolId> {
                            namespace_prefix
                                .as_ref()
                                .and_then(|prefix| {
                                    let mut scoped =
                                        String::with_capacity(prefix.len() + 1 + name.len());
                                    scoped.push_str(prefix);
                                    scoped.push('.');
                                    scoped.push_str(name);
                                    self.resolve_entity_name_text_to_def_id_for_lowering(&scoped)
                                        .and_then(|def_id| {
                                            self.ctx.def_to_symbol_id_with_fallback(def_id)
                                        })
                                })
                                .or_else(|| {
                                    self.resolve_entity_name_text_to_def_id_for_lowering(name)
                                        .and_then(|def_id| {
                                            self.ctx.def_to_symbol_id_with_fallback(def_id)
                                        })
                                })
                        };

                        let type_resolver = |node_idx: NodeIndex| -> Option<u32> {
                            let ident_name = lib_arena.get_identifier_text(node_idx)?;
                            if is_compiler_managed_type(ident_name) {
                                return None;
                            }
                            let sym_id = resolve_type_name(ident_name)?;
                            let symbol = binder.get_symbol_with_libs(sym_id, &lib_binders)?;
                            ((symbol.flags & symbol_flags::TYPE) != 0).then_some(sym_id.0)
                        };
                        let value_resolver = |node_idx: NodeIndex| -> Option<u32> {
                            self.resolve_value_symbol_for_lowering(node_idx)
                        };
                        let def_id_resolver =
                            |node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> {
                                let ident_name = lib_arena.get_identifier_text(node_idx)?;
                                if is_compiler_managed_type(ident_name) {
                                    return None;
                                }
                                let sym_id = resolve_type_name(ident_name)?;
                                let symbol = binder.get_symbol_with_libs(sym_id, &lib_binders)?;
                                ((symbol.flags & symbol_flags::TYPE) != 0)
                                    .then(|| self.ctx.get_or_create_def_id(sym_id))
                            };
                        let name_resolver = |type_name: &str| -> Option<tsz_solver::def::DefId> {
                            namespace_prefix
                                .as_ref()
                                .and_then(|prefix| {
                                    let mut scoped =
                                        String::with_capacity(prefix.len() + 1 + type_name.len());
                                    scoped.push_str(prefix);
                                    scoped.push('.');
                                    scoped.push_str(type_name);
                                    self.resolve_entity_name_text_to_def_id_for_lowering(&scoped)
                                })
                                .or_else(|| {
                                    self.resolve_entity_name_text_to_def_id_for_lowering(type_name)
                                })
                        };

                        let lazy_type_params_resolver =
                            |def_id: tsz_solver::def::DefId| self.ctx.get_def_type_params(def_id);
                        let lowering = TypeLowering::with_hybrid_resolver(
                            lib_arena,
                            self.ctx.types,
                            &type_resolver,
                            &def_id_resolver,
                            &value_resolver,
                        )
                        .with_type_param_bindings(type_param_bindings)
                        .with_lazy_type_params_resolver(&lazy_type_params_resolver)
                        .with_name_def_id_resolver(&name_resolver);
                        let (alias_type, params) =
                            lowering.lower_type_alias_declaration(type_alias);
                        if let Some(def_id) = self.ctx.get_existing_def_id(sym_id) {
                            self.ctx.insert_def_type_params(def_id, params.clone());
                        }
                        return (alias_type, params);
                    }
                }
            }
        }

        // Fallback: get type of symbol and params separately
        let body_type = self.get_type_of_symbol(sym_id);
        let type_params = self.get_type_params_for_symbol(sym_id);
        (body_type, type_params)
    }
}
