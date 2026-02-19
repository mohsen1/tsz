//! Computed symbol type analysis: `compute_type_of_symbol`, contextual literal types,
//! and private property access checking.

use crate::query_boundaries::state_type_environment;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;
use tsz_solver::Visibility;
use tsz_solver::type_queries_extended::{
    ContextualLiteralAllowKind, classify_for_contextual_literal,
};

impl<'a> CheckerState<'a> {
    /// Compute the type of a class symbol.
    ///
    /// Returns the class constructor type, merging with namespace exports
    /// when the class is merged with a namespace. Also caches the instance
    /// type for TYPE position resolution.
    fn compute_class_symbol_type(
        &mut self,
        sym_id: SymbolId,
        flags: u32,
        value_decl: NodeIndex,
        declarations: &[NodeIndex],
    ) -> (TypeId, Vec<tsz_solver::TypeParamInfo>) {
        // Find the class declaration. When a symbol has both CLASS and FUNCTION flags
        // (function-class merging), value_decl may point to the function, not the class.
        // Search all declarations to find the class node.
        let decl_idx = if value_decl.is_some()
            && self
                .ctx
                .arena
                .get(value_decl)
                .and_then(|n| self.ctx.arena.get_class(n))
                .is_some()
        {
            value_decl
        } else {
            // Search declarations for a class node
            declarations
                .iter()
                .find(|&&d| {
                    d.is_some()
                        && self
                            .ctx
                            .arena
                            .get(d)
                            .and_then(|n| self.ctx.arena.get_class(n))
                            .is_some()
                })
                .copied()
                .unwrap_or(NodeIndex::NONE)
        };

        if decl_idx.is_some()
            && let Some(node) = self.ctx.arena.get(decl_idx)
            && let Some(class) = self.ctx.arena.get_class(node)
        {
            // Compute both constructor and instance types
            let ctor_type = self.get_class_constructor_type(decl_idx, class);
            let instance_type = self.get_class_instance_type(decl_idx, class);

            // Cache instance type for TYPE position resolution
            self.ctx.symbol_instance_types.insert(sym_id, instance_type);

            // When a symbol has both CLASS and FUNCTION flags (function-class merging),
            // merge the function's call signatures into the class constructor type.
            // This allows both `new Foo(...)` (construct) and `Foo(...)` (call) to work.
            let ctor_type = if flags & symbol_flags::FUNCTION != 0 {
                self.merge_function_call_signatures_into_class(ctor_type, declarations)
            } else {
                ctor_type
            };

            if flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0 {
                let merged = self.merge_namespace_exports_into_constructor(sym_id, ctor_type);
                return (merged, Vec::new());
            }
            return (ctor_type, Vec::new());
        }
        (TypeId::UNKNOWN, Vec::new())
    }

    /// Merge function call signatures into a class constructor type.
    ///
    /// When a class and function declaration share the same name (function-class merging),
    /// the resulting type should have both construct signatures (from the class) and
    /// call signatures (from the function).
    fn merge_function_call_signatures_into_class(
        &mut self,
        ctor_type: TypeId,
        declarations: &[NodeIndex],
    ) -> TypeId {
        use crate::query_boundaries::state_type_analysis::{
            call_signatures_for_type, callable_shape_for_type,
        };

        // Collect call signatures from function declarations
        let mut call_signatures = Vec::new();
        for &decl_idx in declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(func) = self.ctx.arena.get_function(node) else {
                continue;
            };
            // Only use overload signatures (no body) as call signatures
            if func.body.is_none() {
                call_signatures.push(self.call_signature_from_function(func, decl_idx));
            }
        }

        if call_signatures.is_empty() {
            // No function overload signatures to merge, check for implementation
            for &decl_idx in declarations {
                let Some(node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                if self.ctx.arena.get_function(node).is_some() {
                    // Has a function implementation - get its type and extract call sig
                    let func_type = self.get_type_of_function(decl_idx);
                    if let Some(signatures) = call_signatures_for_type(self.ctx.types, func_type) {
                        call_signatures = signatures;
                    }
                    break;
                }
            }
        }

        if call_signatures.is_empty() {
            return ctor_type;
        }

        // Merge call signatures into the constructor type's callable shape
        let Some(shape) = callable_shape_for_type(self.ctx.types, ctor_type) else {
            return ctor_type;
        };

        let factory = self.ctx.types.factory();
        factory.callable(tsz_solver::CallableShape {
            call_signatures,
            construct_signatures: shape.construct_signatures.clone(),
            properties: shape.properties.clone(),
            string_index: shape.string_index.clone(),
            number_index: shape.number_index.clone(),
            symbol: None,
        })
    }

    /// Compute the type of an enum member symbol.
    ///
    /// Returns a `TypeData::Enum` type with the member's literal type and `DefId`
    /// for nominal identity (ensures E.A is not assignable to E.B).
    fn compute_enum_member_symbol_type(
        &mut self,
        sym_id: SymbolId,
        value_decl: NodeIndex,
    ) -> (TypeId, Vec<tsz_solver::TypeParamInfo>) {
        // Get the member's DefId for nominal typing
        let member_def_id = self.ctx.get_or_create_def_id(sym_id);

        // CRITICAL: Also ensure the parent enum has a DefId
        // This is needed for get_enum_parent_def_id to work when checking
        // member-to-parent assignability (e.g., E.A -> E)
        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
            let parent_sym_id = symbol.parent;
            // Call get_or_create_def_id for the parent to populate symbol_to_def mapping
            let parent_def_id = self.ctx.get_or_create_def_id(parent_sym_id);
            // Register the parent-child relationship in TypeEnvironment
            // This enables enum member widening for mutable bindings
            if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
                env.register_enum_parent(member_def_id, parent_def_id);
            }
        }

        // Get the literal type from the initializer
        let literal_type = self.enum_member_type_from_decl(value_decl);

        // Wrap in nominal enum type for identity
        // This ensures E.A is not assignable to E.B (different DefIds)
        let factory = self.ctx.types.factory();
        let enum_type = factory.enum_type(member_def_id, literal_type);
        (enum_type, Vec::new())
    }

    /// Compute the type of a namespace or module symbol.
    ///
    /// Returns a Lazy type with the `DefId` for deferred resolution.
    /// This is skipped for functions (handled separately) and enums (must come first).
    fn compute_namespace_symbol_type(
        &mut self,
        sym_id: SymbolId,
    ) -> (TypeId, Vec<tsz_solver::TypeParamInfo>) {
        // Create DefId and use Lazy type
        let def_id = self.ctx.get_or_create_def_id(sym_id);
        let factory = self.ctx.types.factory();
        (factory.lazy(def_id), Vec::new())
    }

    fn resolve_export_value_wrapper_target_symbol(
        &self,
        value_decl: NodeIndex,
        escaped_name: &str,
    ) -> Option<SymbolId> {
        if value_decl.is_none() {
            return None;
        }
        let node = self.ctx.arena.get(value_decl)?;
        if node.kind != syntax_kind_ext::EXPORT_DECLARATION {
            return None;
        }
        let export_decl = self.ctx.arena.get_export_decl(node)?;
        if export_decl.export_clause.is_none() {
            return None;
        }

        let clause_idx = export_decl.export_clause;
        let clause_node = self.ctx.arena.get(clause_idx)?;

        if clause_node.kind == syntax_kind_ext::VARIABLE_STATEMENT
            && let Some(var_stmt) = self.ctx.arena.get_variable(clause_node)
        {
            for &list_idx in &var_stmt.declarations.nodes {
                let Some(list_node) = self.ctx.arena.get(list_idx) else {
                    continue;
                };
                let Some(decl_list) = self.ctx.arena.get_variable(list_node) else {
                    continue;
                };
                for &decl_idx in &decl_list.declarations.nodes {
                    let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                        continue;
                    };
                    let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                        continue;
                    };
                    let Some(name_node) = self.ctx.arena.get(var_decl.name) else {
                        continue;
                    };
                    let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                        continue;
                    };
                    if ident.escaped_text == escaped_name
                        && let Some(&sym_id) = self.ctx.binder.node_symbols.get(&decl_idx.0)
                    {
                        return Some(sym_id);
                    }
                }
            }
        }

        self.ctx.binder.node_symbols.get(&clause_idx.0).copied()
    }

    /// Compute type of a symbol (internal, not cached).
    ///
    /// Uses `TypeLowering` to bridge symbol declarations to solver types.
    /// Returns the computed type and the type parameters used (if any).
    /// IMPORTANT: The type params returned must be the same ones used when lowering
    /// the type body, so that instantiation works correctly.
    pub(crate) fn compute_type_of_symbol(
        &mut self,
        sym_id: SymbolId,
    ) -> (TypeId, Vec<tsz_solver::TypeParamInfo>) {
        let factory = self.ctx.types.factory();
        use tsz_lowering::TypeLowering;

        // Handle cross-file symbol resolution via delegation
        if let Some(result) = self.delegate_cross_arena_symbol_resolution(sym_id) {
            tracing::trace!(
                sym_id = sym_id.0,
                result_type = result.0.0,
                file = self.ctx.file_name.as_str(),
                "compute_type_of_symbol: delegated to cross-arena"
            );
            return result;
        }

        // Use get_symbol_globally to find symbols in lib files and other files
        // Extract needed data to avoid holding borrow across mutable operations
        let (flags, value_decl, declarations, import_module, import_name, escaped_name) =
            match self.get_symbol_globally(sym_id) {
                Some(symbol) => (
                    symbol.flags,
                    symbol.value_declaration,
                    symbol.declarations.clone(),
                    symbol.import_module.clone(),
                    symbol.import_name.clone(),
                    symbol.escaped_name.clone(),
                ),
                None => return (TypeId::UNKNOWN, Vec::new()),
            };

        tracing::trace!(
        sym_id = sym_id.0,
        flags = format!("{flags:#x}").as_str(),
        name = escaped_name.as_str(),
        import_module = ?import_module,
        import_name = ?import_name,
        value_decl = value_decl.0,
        file = self.ctx.file_name.as_str(),
        "compute_type_of_symbol: resolved symbol"
        );

        // Export-value wrapper symbols should delegate to their wrapped declaration symbol.
        // This preserves the actual value type for `export var` / `export function` members
        // instead of falling back to implicit `any`.
        if flags & symbol_flags::EXPORT_VALUE != 0
            && let Some(target_sym_id) =
                self.resolve_export_value_wrapper_target_symbol(value_decl, &escaped_name)
            && target_sym_id != sym_id
        {
            return (self.get_type_of_symbol(target_sym_id), Vec::new());
        }

        // Class - return class constructor type (merging namespace exports when present)
        // Also compute and cache instance type for TYPE position resolution
        if flags & symbol_flags::CLASS != 0 {
            return self.compute_class_symbol_type(sym_id, flags, value_decl, &declarations);
        }

        // Enum - return TypeData::Enum with DefId for nominal identity checking.
        // The Enum type provides proper enum subtype checking via DefId-based
        // symbol resolution and type equality.
        //
        // CRITICAL: We must compute and cache a structural type (union of member types)
        // before returning TypeData::Enum to prevent infinite recursion in ensure_refs_resolved.
        //
        // IMPORTANT: This check must come BEFORE the NAMESPACE_MODULE check below because
        // enum-namespace merges have both ENUM and NAMESPACE_MODULE flags. We want to
        // handle them as enums (returning TypeData::Enum) rather than as namespaces (returning Lazy).
        if flags & symbol_flags::ENUM != 0 {
            // Create DefId first
            let def_id = self.ctx.get_or_create_def_id(sym_id);

            // Find the enum declaration node
            let decl_idx = if value_decl.is_some() {
                value_decl
            } else {
                declarations.first().copied().unwrap_or(NodeIndex::NONE)
            };

            // Compute the union type of all enum member types.
            // Also pre-cache each member symbol type so `E.Member` property access
            // can hit `ctx.symbol_types` directly instead of running full symbol
            // resolution for each distinct member.
            let mut member_types = Vec::new();
            if decl_idx.is_some()
                && let Some(enum_decl) = self.ctx.arena.get_enum_at(decl_idx)
            {
                let mut maybe_env = self.ctx.type_env.try_borrow_mut().ok();
                member_types.reserve(enum_decl.members.nodes.len());
                for &member_idx in &enum_decl.members.nodes {
                    if let Some(member) = self.ctx.arena.get_enum_member_at(member_idx) {
                        let member_type = self.enum_member_type_from_decl(member_idx);
                        if member_type != TypeId::ERROR {
                            member_types.push(member_type);
                        }

                        // Pre-cache member symbol types.
                        // This avoids per-member `get_type_of_symbol` overhead in
                        // hot paths such as large enum property-access switches.
                        if let Some(member_name) = self.get_property_name(member.name)
                            && let Some(member_sym_id) = self
                                .ctx
                                .binder
                                .get_symbol(sym_id)
                                .and_then(|enum_symbol| enum_symbol.exports.as_ref())
                                .and_then(|exports| exports.get(&member_name))
                        {
                            let member_def_id = self.ctx.get_or_create_def_id(member_sym_id);
                            let member_enum_type = factory.enum_type(member_def_id, member_type);
                            self.ctx
                                .symbol_types
                                .insert(member_sym_id, member_enum_type);
                            if let Some(env) = maybe_env.as_mut() {
                                env.insert(
                                    tsz_solver::SymbolRef(member_sym_id.0),
                                    member_enum_type,
                                );
                                if member_def_id != tsz_solver::DefId::INVALID {
                                    env.insert_def(member_def_id, member_enum_type);
                                    // Register parent-child relationship for enum member widening
                                    env.register_enum_parent(member_def_id, def_id);
                                }
                            }
                        }
                    }
                }
            }

            // Create the structural type (union of member types, or NUMBER/STRING for homogeneous enums)
            let structural_type = if member_types.is_empty() {
                // Empty enum - default to NUMBER
                TypeId::NUMBER
            } else if member_types.len() == 1 {
                // Single member - use that type
                member_types[0]
            } else {
                // Multiple members - create a union
                factory.union(member_types)
            };

            // Cache the structural type in type_env for compatibility
            // Note: Enum types now use TypeData::Enum(def_id, member_type) directly
            if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
                env.insert_def(def_id, structural_type);
            }

            // CRITICAL: Return TypeData::Enum(def_id, structural_type) NOT Lazy(def_id)
            // - Lazy(def_id) creates infinite recursion in ensure_refs_resolved
            // - structural_type alone loses nominal identity (E1 becomes 0 | 1)
            // - Enum(def_id, structural_type) preserves both:
            //   1. DefId for nominal identity (E1 != E2)
            //   2. structural_type for assignability to primitives (E1 <: number)
            let enum_type = factory.enum_type(def_id, structural_type);

            // CRITICAL: Merge namespace exports for enum+namespace merging
            // When an enum and namespace with the same name are merged, the namespace's
            // exports become accessible as properties on the enum object.
            //
            // FIX: We still return the enum_type here (not the merged object type) because:
            // 1. `Direction` (the enum) should have type `Direction`, not the merged object type
            // 2. `Direction.isVertical(Direction.Up)` should work because:
            //    - `Direction` has type `Direction` (the enum type)
            //    - `Direction.isVertical` is accessed via property access, which resolves to the function
            //    - `Direction.Up` has type `Direction` (my earlier fix)
            // The merged object type is only used internally for property access resolution.
            if flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0 {
                // Create the merged type for internal property access resolution
                let _merged_type = self.merge_namespace_exports_into_object(sym_id, enum_type);
                // Store the merged type in a separate cache for property access lookup
                // But return the enum_type as the type of the enum itself
            }
            // Register DefId <-> SymbolId mapping for enum type resolution
            self.ctx
                .register_resolved_type(sym_id, enum_type, Vec::new());

            return (enum_type, Vec::new());
        }

        // Namespace / Module
        // Return a Ref type AND register DefId mapping for gradual migration.
        // The Ref type is needed because resolve_qualified_name and other code
        // extracts SymbolRef from the type to look up the symbol's exports map.
        // Skip this when the symbol is also a FUNCTION — the FUNCTION branch below
        // handles merging namespace exports into the function's callable type.
        //
        // IMPORTANT: This check must come AFTER the ENUM check above because
        // enum-namespace merges have both ENUM and NAMESPACE_MODULE flags. We want to
        // handle them as enums (returning TypeData::Enum) rather than as namespaces.
        if flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0
            && flags & symbol_flags::FUNCTION == 0
        {
            return self.compute_namespace_symbol_type(sym_id);
        }

        // Enum member - determine type from parent enum
        if flags & symbol_flags::ENUM_MEMBER != 0 {
            return self.compute_enum_member_symbol_type(sym_id, value_decl);
        }

        // Function - build function type or callable overload set.
        // For symbols merged as interface+function, prefer the interface path below
        // when computing the symbol's semantic type (type-position behavior).
        if flags & symbol_flags::FUNCTION != 0 && flags & symbol_flags::INTERFACE == 0 {
            use tsz_solver::CallableShape;

            let mut overloads = Vec::new();
            let mut implementation_decl = NodeIndex::NONE;

            for &decl_idx in &declarations {
                let Some(node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                let Some(func) = self.ctx.arena.get_function(node) else {
                    continue;
                };

                if func.body.is_none() {
                    overloads.push(self.call_signature_from_function(func, decl_idx));
                } else {
                    implementation_decl = decl_idx;
                }
            }

            let function_type = if !overloads.is_empty() {
                let shape = CallableShape {
                    call_signatures: overloads,
                    construct_signatures: Vec::new(),
                    properties: Vec::new(),
                    string_index: None,
                    number_index: None,
                    symbol: None,
                };
                factory.callable(shape)
            } else if value_decl.is_some() {
                self.get_type_of_function(value_decl)
            } else if implementation_decl.is_some() {
                self.get_type_of_function(implementation_decl)
            } else {
                TypeId::UNKNOWN
            };

            // If function is merged with namespace, merge namespace exports into function type
            // This allows accessing namespace members through the function name: Model.Options
            if flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0 {
                return self.merge_namespace_exports_into_function(sym_id, function_type);
            }

            return (function_type, Vec::new());
        }

        // NOTE: When a symbol is BOTH an interface AND a variable (e.g., `interface Error` +
        // `declare var Error: ErrorConstructor`), we fall through to the INTERFACE block below.
        // The interface type is the correct type for TYPE position (e.g., `var e: Error`).
        // VALUE position (e.g., `new Error()`) is handled separately by `get_type_of_identifier`
        // which has its own merged-symbol resolution via `type_of_value_declaration_for_symbol`.

        // Interface - return interface type with call signatures
        if flags & symbol_flags::INTERFACE != 0 {
            // Merged lib symbols can live in the main binder but still carry
            // declaration nodes from other arenas. Lowering those declarations
            // against the current arena produces incomplete interface shapes
            // (e.g. Date without getTime, PromiseConstructor without resolve/race/new).
            //
            // We check two conditions (either triggers the lib path):
            // 1. Per-declaration check: NodeIndex is out of range OR declaration_arenas
            //    has an entry pointing to a different arena
            // 2. Fallback: symbol_arenas has an entry for this symbol, meaning it was
            //    merged from a lib file. This catches cross-arena NodeIndex collisions
            //    where the index is valid in the main arena but maps to a different node
            let has_out_of_arena_decl = declarations.iter().any(|&decl_idx| {
                if self.ctx.arena.get(decl_idx).is_none() {
                    return true;
                }
                if let Some(decl_arena) = self
                    .ctx
                    .binder
                    .declaration_arenas
                    .get(&(sym_id, decl_idx))
                    .and_then(|v| v.first())
                {
                    return !std::ptr::eq(decl_arena.as_ref(), self.ctx.arena);
                }
                false
            });
            // Only use the is_lib_symbol fallback when the per-declaration check
            // couldn't determine the arena origin (i.e. no declaration_arenas entry
            // AND the declaration exists in the current arena). The is_lib_symbol
            // flag is set for ALL symbols that were merged during multi-file
            // compilation, including user-defined interfaces. Using it unconditionally
            // causes user interfaces to skip merge_interface_heritage_types, which
            // loses inherited call/construct signatures (TS2345 false positives).
            let is_lib_symbol = if has_out_of_arena_decl {
                false // Already determined cross-arena by per-decl check
            } else {
                // When all declarations are in the current arena, check if any
                // actually maps to an InterfaceDeclaration node. User-defined
                // interfaces will have real interface nodes; cross-arena collisions
                // will have NodeIndexes that point to unrelated nodes. Only fall
                // back to lib resolution when there's no real interface decl.
                let has_real_interface_decl = declarations.iter().any(|&decl_idx| {
                    self.ctx
                        .arena
                        .get(decl_idx)
                        .and_then(|node| self.ctx.arena.get_interface(node))
                        .is_some()
                });
                !has_real_interface_decl && self.ctx.binder.symbol_arenas.contains_key(&sym_id)
            };
            if (has_out_of_arena_decl || is_lib_symbol)
                && !self.ctx.lib_contexts.is_empty()
                && let Some(lib_type) = self.resolve_lib_type_by_name(&escaped_name)
            {
                // Preserve diagnostic formatting for canonical lib interfaces
                // by recording the resolved object shape on this symbol's DefId.
                let def_id = self.ctx.get_or_create_def_id(sym_id);
                if let Some(shape) = state_type_environment::object_shape(self.ctx.types, lib_type)
                {
                    self.ctx.definition_store.set_instance_shape(def_id, shape);
                }

                return (lib_type, Vec::new());
            }

            if !declarations.is_empty() {
                // Get type parameters from the first interface declaration
                let mut params = Vec::new();
                let mut updates = Vec::new();

                // Try to get type parameters from the interface declaration
                let first_decl = declarations.first().copied().unwrap_or(NodeIndex::NONE);
                if let Some(node) = self.ctx.arena.get(first_decl) {
                    if let Some(interface) = self.ctx.arena.get_interface(node) {
                        (params, updates) = self.push_type_parameters(&interface.type_parameters);
                    }
                }

                let type_param_bindings = self.get_type_param_bindings();
                let type_resolver =
                    |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
                // Use DefId resolver so interface member types like `inner: Inner`
                // produce Lazy(DefId) instead of TypeId::ERROR. Without this, any
                // type reference to another interface/type alias in an interface body
                // fails to resolve.
                let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> {
                    self.resolve_type_symbol_for_lowering(node_idx)
                        .map(|sym_id_raw| {
                            self.ctx
                                .get_or_create_def_id(tsz_binder::SymbolId(sym_id_raw))
                        })
                };
                let value_resolver =
                    |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
                let lowering = TypeLowering::with_hybrid_resolver(
                    self.ctx.arena,
                    self.ctx.types,
                    &type_resolver,
                    &def_id_resolver,
                    &value_resolver,
                )
                .with_type_param_bindings(type_param_bindings);
                let interface_type =
                    lowering.lower_interface_declarations_with_symbol(&declarations, sym_id);
                let interface_type =
                    self.merge_interface_heritage_types(&declarations, interface_type);
                if let Some(shape) =
                    state_type_environment::object_shape(self.ctx.types, interface_type)
                {
                    self.ctx
                        .definition_store
                        .set_instance_shape(self.ctx.get_or_create_def_id(sym_id), shape);
                }

                // Restore the type parameter scope
                self.pop_type_parameters(updates);

                // Return the interface type along with the type parameters that were used
                return (interface_type, params);
            }
            if value_decl.is_some() {
                return (self.get_type_of_interface(value_decl), Vec::new());
            }
            return (TypeId::UNKNOWN, Vec::new());
        }

        // Type alias - resolve using checker's get_type_from_type_node to properly resolve symbols
        if flags & symbol_flags::TYPE_ALIAS != 0 {
            // When a type alias name collides with a global value declaration
            // (e.g., user-defined `type Proxy<T>` vs global `declare var Proxy`),
            // the merged symbol's value_declaration points to the var decl, not the
            // type alias. We must search declarations[] to find the actual type alias.
            let decl_idx = declarations
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
                                let name_node = self.ctx.arena.get(type_alias.name)?;
                                let ident = self.ctx.arena.get_identifier(name_node)?;
                                let name = self.ctx.arena.resolve_identifier_text(ident);
                                Some(name == escaped_name)
                            } else {
                                Some(false)
                            }
                        })
                        .unwrap_or(false)
                })
                .unwrap_or_else(|| {
                    if value_decl.is_some() {
                        value_decl
                    } else {
                        declarations.first().copied().unwrap_or(NodeIndex::NONE)
                    }
                });
            if decl_idx.is_some()
                && let Some(node) = self.ctx.arena.get(decl_idx)
                && let Some(type_alias) = self.ctx.arena.get_type_alias(node)
            {
                let (params, updates) = self.push_type_parameters(&type_alias.type_parameters);
                let alias_type = self.get_type_from_type_node(type_alias.type_node);
                self.pop_type_parameters(updates);

                // Check for invalid circular reference (TS2456)
                // A type alias circularly references itself if it resolves to itself
                // without structural wrapping (e.g., `type A = B; type B = A;`)
                if self.is_direct_circular_reference(sym_id, alias_type, type_alias.type_node) {
                    use crate::diagnostics::{
                        diagnostic_codes, diagnostic_messages, format_message,
                    };

                    let name = escaped_name;
                    let message = format_message(
                        diagnostic_messages::TYPE_ALIAS_CIRCULARLY_REFERENCES_ITSELF,
                        &[&name],
                    );
                    self.error_at_node(
                        decl_idx,
                        &message,
                        diagnostic_codes::TYPE_ALIAS_CIRCULARLY_REFERENCES_ITSELF,
                    );
                    // Return ERROR to prevent downstream issues
                    return (TypeId::ERROR, params);
                }

                // CRITICAL FIX: Always create DefId for type aliases, not just when they have type parameters
                // This enables Lazy type resolution via TypeResolver during narrowing operations
                let def_id = self.ctx.get_or_create_def_id(sym_id);

                // Cache type parameters for Application expansion (Priority 1 fix)
                // This enables ExtractState<NumberReducer> to expand correctly
                if !params.is_empty() {
                    self.ctx.insert_def_type_params(def_id, params.clone());
                }

                // Return the params that were used during lowering - this ensures
                // type_env gets the same TypeIds as the type body
                return (alias_type, params);
            }
            return (TypeId::UNKNOWN, Vec::new());
        }

        // Variable - get type from annotation or infer from initializer
        if flags & (symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::BLOCK_SCOPED_VARIABLE)
            != 0
        {
            let mut resolved_value_decl = value_decl;

            // Symbols can point at wrappers (export declarations, variable statements, or
            // declaration lists). Normalize to the concrete VariableDeclaration node.
            if resolved_value_decl.is_some() {
                if let Some(node) = self.ctx.arena.get(resolved_value_decl)
                    && node.kind == syntax_kind_ext::EXPORT_DECLARATION
                    && let Some(export_decl) = self.ctx.arena.get_export_decl(node)
                    && export_decl.export_clause.is_some()
                {
                    resolved_value_decl = export_decl.export_clause;
                }

                if let Some(node) = self.ctx.arena.get(resolved_value_decl) {
                    if node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                        if let Some(var_stmt) = self.ctx.arena.get_variable(node) {
                            'find_decl_in_stmt: for &list_idx in &var_stmt.declarations.nodes {
                                let Some(list_node) = self.ctx.arena.get(list_idx) else {
                                    continue;
                                };
                                let Some(decl_list) = self.ctx.arena.get_variable(list_node) else {
                                    continue;
                                };
                                for &decl_idx in &decl_list.declarations.nodes {
                                    let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                                        continue;
                                    };
                                    let Some(var_decl) =
                                        self.ctx.arena.get_variable_declaration(decl_node)
                                    else {
                                        continue;
                                    };
                                    let Some(name_node) = self.ctx.arena.get(var_decl.name) else {
                                        continue;
                                    };
                                    let Some(ident) = self.ctx.arena.get_identifier(name_node)
                                    else {
                                        continue;
                                    };
                                    if ident.escaped_text == escaped_name {
                                        resolved_value_decl = decl_idx;
                                        break 'find_decl_in_stmt;
                                    }
                                }
                            }
                        }
                    } else if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                        && let Some(decl_list) = self.ctx.arena.get_variable(node)
                    {
                        for &decl_idx in &decl_list.declarations.nodes {
                            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                                continue;
                            };
                            let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node)
                            else {
                                continue;
                            };
                            let Some(name_node) = self.ctx.arena.get(var_decl.name) else {
                                continue;
                            };
                            let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                                continue;
                            };
                            if ident.escaped_text == escaped_name {
                                resolved_value_decl = decl_idx;
                                break;
                            }
                        }
                    }
                }
            }

            if resolved_value_decl.is_some()
                && let Some(node) = self.ctx.arena.get(resolved_value_decl)
            {
                // Check if this is a variable declaration
                if let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) {
                    // First try type annotation using type-node lowering (resolves through binder).
                    if var_decl.type_annotation.is_some() {
                        return (
                            self.get_type_from_type_node(var_decl.type_annotation),
                            Vec::new(),
                        );
                    }
                    if let Some(jsdoc_type) =
                        self.jsdoc_type_annotation_for_node(resolved_value_decl)
                    {
                        return (jsdoc_type, Vec::new());
                    }
                    if var_decl.initializer.is_some()
                        && self.is_const_variable_declaration(resolved_value_decl)
                        && let Some(literal_type) =
                            self.literal_type_from_initializer(var_decl.initializer)
                    {
                        return (literal_type, Vec::new());
                    }
                    // Fall back to inferring from initializer
                    if var_decl.initializer.is_some() {
                        let mut inferred_type = self.get_type_of_node(var_decl.initializer);
                        let init_is_direct_empty_array = self
                            .ctx
                            .arena
                            .get(var_decl.initializer)
                            .is_some_and(|init_node| {
                                init_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                                    && self
                                        .ctx
                                        .arena
                                        .get_literal_expr(init_node)
                                        .is_some_and(|lit| lit.elements.nodes.is_empty())
                            });
                        if init_is_direct_empty_array
                            && tsz_solver::type_queries::get_array_element_type(
                                self.ctx.types,
                                inferred_type,
                            ) == Some(TypeId::NEVER)
                        {
                            inferred_type = self.ctx.types.factory().array(TypeId::ANY);
                        }
                        // Literal Widening for mutable bindings (let/var):
                        // Only widen when the initializer is a "fresh" literal expression
                        // (direct literal in source code). Types from variable references,
                        // narrowing, or computed expressions are "non-fresh" and NOT widened.
                        // `let x = "div" as const` should have type "div", not string.
                        if !self.is_const_variable_declaration(resolved_value_decl)
                            && !self.is_const_assertion_initializer(var_decl.initializer)
                        {
                            let widened_type =
                                if self.is_fresh_literal_expression(var_decl.initializer) {
                                    self.widen_initializer_type_for_mutable_binding(inferred_type)
                                } else {
                                    inferred_type
                                };
                            // When strictNullChecks is off, undefined and null widen to any
                            // (always, regardless of freshness)
                            if !self.ctx.strict_null_checks()
                                && (widened_type == TypeId::UNDEFINED
                                    || widened_type == TypeId::NULL)
                            {
                                return (TypeId::ANY, Vec::new());
                            }
                            return (widened_type, Vec::new());
                        }
                        return (inferred_type, Vec::new());
                    }
                }
                // Check if this is a function parameter
                else if let Some(param) = self.ctx.arena.get_parameter(node) {
                    // Get type from annotation
                    if param.type_annotation.is_some() {
                        let mut type_id = self.get_type_from_type_node(param.type_annotation);
                        // Under strictNullChecks, optional parameters (?) include undefined
                        // in their type. E.g., `n?: number` has type `number | undefined`.
                        if param.question_token
                            && self.ctx.strict_null_checks()
                            && type_id != TypeId::ANY
                            && type_id != TypeId::UNKNOWN
                            && type_id != TypeId::ERROR
                        {
                            type_id = factory.union(vec![type_id, TypeId::UNDEFINED]);
                        }
                        return (type_id, Vec::new());
                    }
                    // Check for JSDoc type
                    if let Some(jsdoc_type) =
                        self.jsdoc_type_annotation_for_node(resolved_value_decl)
                    {
                        return (jsdoc_type, Vec::new());
                    }
                    // Fall back to inferring from initializer (default value)
                    if param.initializer.is_some() {
                        return (self.get_type_of_node(param.initializer), Vec::new());
                    }
                }
            }
            // Variable without type annotation or initializer gets implicit 'any'
            // This prevents cascading TS2571 errors
            return (TypeId::ANY, Vec::new());
        }

        // Alias - resolve the aliased type (import x = ns.member or ES6 imports)
        if flags & symbol_flags::ALIAS != 0 {
            if value_decl.is_some()
                && let Some(node) = self.ctx.arena.get(value_decl)
                && node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                && let Some(import) = self.ctx.arena.get_import_decl(node)
            {
                // Check for require() call FIRST — `import A = require('M')` must
                // resolve through module_exports, not qualified symbol resolution.
                // The qualified symbol path is for `import x = ns.member` patterns.
                if let Some(module_specifier) =
                    self.get_require_module_specifier(import.module_specifier)
                {
                    // Resolve the canonical export surface (module-specifier variants,
                    // cross-file tables, and export= member merging).
                    let exports_table = self.resolve_effective_module_exports(&module_specifier);

                    if let Some(exports_table) = exports_table {
                        // Create an object type with all the module's exports
                        use tsz_solver::PropertyInfo;
                        let module_is_non_module_entity = self
                            .ctx
                            .module_resolves_to_non_module_entity(&module_specifier);
                        let export_equals_type = exports_table
                            .get("export=")
                            .map(|export_equals_sym| self.get_type_of_symbol(export_equals_sym));
                        let mut props: Vec<PropertyInfo> = Vec::new();
                        for (name, &sym_id) in exports_table.iter() {
                            if name == "export=" {
                                continue;
                            }
                            let mut prop_type = self.get_type_of_symbol(sym_id);
                            prop_type =
                                self.apply_module_augmentations(&module_specifier, name, prop_type);
                            let name_atom = self.ctx.types.intern_string(name);
                            props.push(PropertyInfo {
                                name: name_atom,
                                type_id: prop_type,
                                write_type: prop_type,
                                optional: false,
                                readonly: false,
                                is_method: false,
                                visibility: Visibility::Public,
                                parent_id: None,
                            });
                        }

                        if !module_is_non_module_entity
                            && let Some(augmentations) =
                                self.ctx.binder.module_augmentations.get(&module_specifier)
                        {
                            for aug in augmentations {
                                let name_atom = self.ctx.types.intern_string(&aug.name);
                                if props.iter().any(|p| p.name == name_atom) {
                                    continue;
                                }
                                props.push(PropertyInfo {
                                    name: name_atom,
                                    type_id: TypeId::ANY,
                                    write_type: TypeId::ANY,
                                    optional: false,
                                    readonly: false,
                                    is_method: false,
                                    visibility: Visibility::Public,
                                    parent_id: None,
                                });
                            }
                        }

                        let namespace_type = factory.object(props);
                        if let Some(export_equals_type) = export_equals_type {
                            if module_is_non_module_entity {
                                return (export_equals_type, Vec::new());
                            }
                            return (
                                factory.intersection(vec![export_equals_type, namespace_type]),
                                Vec::new(),
                            );
                        }

                        return (namespace_type, Vec::new());
                    }
                    // Module not found - emit TS2307 error and return ANY
                    // TypeScript treats unresolved imports as `any` to avoid cascading errors
                    self.emit_module_not_found_error(&module_specifier, value_decl);
                    return (TypeId::ANY, Vec::new());
                }
                // Not a require() call — try qualified symbol resolution
                // for `import x = ns.member` patterns.
                if let Some(target_sym) = self.resolve_qualified_symbol(import.module_specifier) {
                    return (self.get_type_of_symbol(target_sym), Vec::new());
                }
                // Namespace import failed to resolve
                // Check for TS2694 (Namespace has no exported member) or TS2304 (Cannot find name)
                // This happens when: import Alias = NS.NotExported (where NotExported is not exported)

                // 1. Check for TS2694 (Namespace has no exported member)
                if self.report_type_query_missing_member(import.module_specifier) {
                    return (TypeId::ERROR, Vec::new());
                }

                // 2. Check for TS2304 (Cannot find name) for the left-most part
                if let Some(missing_idx) = self.missing_type_query_left(import.module_specifier) {
                    // Suppress if it's an unresolved import (TS2307 already emitted)
                    if !self.is_unresolved_import_symbol(missing_idx)
                        && let Some(name) = self.entity_name_text(missing_idx)
                    {
                        self.error_cannot_find_name_at(&name, missing_idx);
                    }
                    return (TypeId::ERROR, Vec::new());
                }

                // Return ERROR for other cases to prevent cascading errors
                return (TypeId::ERROR, Vec::new());
            }
            // Handle ES6 named imports (import { X } from './module')
            // Use the import_module field to resolve to the actual export
            // Check if this symbol has import tracking metadata

            // For ES6 imports with import_module set, resolve using module_exports
            if let Some(ref module_name) = import_module {
                // Check if this is a shorthand ambient module (declare module "foo" without body)
                // Imports from shorthand ambient modules are typed as `any`
                if self
                    .ctx
                    .binder
                    .shorthand_ambient_modules
                    .contains(module_name)
                {
                    return (TypeId::ANY, Vec::new());
                }

                // Check if this is a namespace import (import * as ns)
                // Namespace imports have import_name set to None and should return all exports as an object
                if import_name.is_none() {
                    // This is a namespace import: import * as ns from 'module'
                    // Create an object type containing all module exports

                    let exports_table = self.resolve_effective_module_exports(module_name);

                    if let Some(exports_table) = exports_table {
                        // Record cross-file symbol targets for all symbols in the table
                        for (name, &sym_id) in exports_table.iter() {
                            self.record_cross_file_symbol_if_needed(sym_id, name, module_name);
                        }

                        use tsz_solver::PropertyInfo;
                        let module_is_non_module_entity =
                            self.ctx.module_resolves_to_non_module_entity(module_name);
                        let export_equals_type = exports_table
                            .get("export=")
                            .map(|export_equals_sym| self.get_type_of_symbol(export_equals_sym));
                        let mut props: Vec<PropertyInfo> = Vec::new();
                        for (name, &export_sym_id) in exports_table.iter() {
                            if name == "export=" {
                                continue;
                            }
                            let mut prop_type = self.get_type_of_symbol(export_sym_id);

                            // Rule #44: Apply module augmentations to each exported type
                            prop_type =
                                self.apply_module_augmentations(module_name, name, prop_type);

                            let name_atom = self.ctx.types.intern_string(name);
                            props.push(PropertyInfo {
                                name: name_atom,
                                type_id: prop_type,
                                write_type: prop_type,
                                optional: false,
                                readonly: false,
                                is_method: false,
                                visibility: Visibility::Public,
                                parent_id: None,
                            });
                        }

                        // Add augmentation declarations that introduce entirely new names.
                        // If the target resolves to a non-module export= value, these names
                        // are invalid and should not be surfaced on the namespace.
                        if !module_is_non_module_entity
                            && let Some(augmentations) =
                                self.ctx.binder.module_augmentations.get(module_name)
                        {
                            for aug in augmentations {
                                let name_atom = self.ctx.types.intern_string(&aug.name);
                                if props.iter().any(|p| p.name == name_atom) {
                                    continue;
                                }
                                props.push(PropertyInfo {
                                    name: name_atom,
                                    // Cross-file augmentation declarations may live in a different
                                    // arena; use `any` here to preserve namespace member visibility.
                                    type_id: TypeId::ANY,
                                    write_type: TypeId::ANY,
                                    optional: false,
                                    readonly: false,
                                    is_method: false,
                                    visibility: Visibility::Public,
                                    parent_id: None,
                                });
                            }
                        }

                        let namespace_type = factory.object(props);
                        if let Some(export_equals_type) = export_equals_type {
                            if module_is_non_module_entity {
                                return (export_equals_type, Vec::new());
                            }
                            return (
                                factory.intersection(vec![export_equals_type, namespace_type]),
                                Vec::new(),
                            );
                        }

                        return (namespace_type, Vec::new());
                    }
                    // Module not found - emit TS2307 error and return ANY
                    // TypeScript treats unresolved imports as `any` to avoid cascading errors
                    self.emit_module_not_found_error(module_name, value_decl);
                    return (TypeId::ANY, Vec::new());
                }

                // This is a named import: import { X } from 'module'
                // Use import_name if set (for renamed imports), otherwise use escaped_name
                let export_name = import_name.as_ref().unwrap_or(&escaped_name);

                // Check if the module exists first (for proper error differentiation)
                let module_exists = self.ctx.binder.module_exports.contains_key(module_name)
                    || self.module_exists_cross_file(module_name);
                if self.is_ambient_module_match(module_name) && !module_exists {
                    return (TypeId::ANY, Vec::new());
                }

                // First, try local binder's module_exports
                let export_sym_id = self
                    .ctx
                    .binder
                    .module_exports
                    .get(module_name)
                    .and_then(|exports_table| exports_table.get(export_name))
                    // Fall back to cross-file resolution if local lookup fails
                    .or_else(|| self.resolve_cross_file_export(module_name, export_name))
                    .or_else(|| {
                        self.resolve_named_export_via_export_equals(module_name, export_name)
                    })
                    .or_else(|| {
                        let mut visited_aliases = Vec::new();
                        self.resolve_reexported_member_symbol(
                            module_name,
                            export_name,
                            &mut visited_aliases,
                        )
                    });

                if let Some(export_sym_id) = export_sym_id {
                    // Detect cross-file SymbolIds: the driver copies target file's
                    // module_exports into the local binder, so SymbolIds may be from
                    // another binder. Check if the SymbolId maps to the expected name
                    // in the current binder — if not, it's from another file.
                    self.record_cross_file_symbol_if_needed(
                        export_sym_id,
                        export_name,
                        module_name,
                    );

                    let mut result = self.get_type_of_symbol(export_sym_id);

                    // Rule #44: Apply module augmentations to the imported type
                    // If there are augmentations for this module+interface, merge them in
                    result = self.apply_module_augmentations(module_name, export_name, result);

                    // CRITICAL: Update the symbol type cache with the augmented type
                    // This ensures that when the type annotation uses the symbol (e.g., `let x: Observable<number>`),
                    // it gets the augmented type with all merged members
                    self.ctx.symbol_types.insert(export_sym_id, result);

                    return (result, Vec::new());
                }

                // Module augmentations can introduce named exports that don't appear
                // in the base module export table. Treat those names as resolvable.
                if self
                    .ctx
                    .binder
                    .module_augmentations
                    .get(module_name)
                    .is_some_and(|augs| augs.iter().any(|aug| aug.name == *export_name))
                {
                    let mut result = TypeId::ANY;
                    result = self.apply_module_augmentations(module_name, export_name, result);
                    return (result, Vec::new());
                }

                // If the module resolved externally but isn't part of the program,
                // skip export member validation (treat as `any`).
                let has_exports_table = self.ctx.binder.module_exports.contains_key(module_name)
                    || self.resolve_effective_module_exports(module_name).is_some();
                if module_exists
                    && !has_exports_table
                    && self.ctx.resolve_import_target(module_name).is_none()
                {
                    return (TypeId::ANY, Vec::new());
                }

                // Export not found - emit appropriate error based on what's missing
                if module_exists {
                    // Module exists but export not found
                    if export_name == "default" {
                        let has_export_equals = self.module_has_export_equals(module_name);

                        if has_export_equals {
                            self.emit_no_default_export_error(module_name, value_decl);
                            return (TypeId::ERROR, Vec::new());
                        }

                        // `import { default as X } from 'mod'` is a named import lookup,
                        // so missing `default` should be TS2305 (not TS1192).
                        // Use declarations list as fallback when value_decl is NONE
                        // (ES6 named imports don't set value_declaration).
                        let binding_node = if value_decl.is_some() {
                            tracing::debug!(value_decl = %value_decl.0, "using value_decl as binding_node");
                            value_decl
                        } else {
                            let node = declarations.first().copied().unwrap_or(NodeIndex::NONE);
                            tracing::debug!(from_declarations = %node.0, decls_len = declarations.len(), "using first declaration as binding_node");
                            node
                        };

                        tracing::debug!(binding_node = %binding_node.0, module_name, export_name, "checking if this is a true default import");
                        let is_true_default = self.is_true_default_import_binding(binding_node);
                        tracing::debug!(is_true_default, allow_synthetic = %self.ctx.allow_synthetic_default_imports(), "result of is_true_default_import_binding");

                        if !is_true_default {
                            // With allowSyntheticDefaultImports, `{ default as X }` resolves
                            // to the module namespace (same as `import X from "mod"`).
                            if !self.ctx.allow_synthetic_default_imports() {
                                tracing::debug!(
                                    "NOT a true default import and allowSyntheticDefaultImports is false, emitting TS2305"
                                );
                                self.emit_no_exported_member_error(
                                    module_name,
                                    export_name,
                                    value_decl,
                                );
                                return (TypeId::ERROR, Vec::new());
                            }
                            tracing::debug!(
                                "NOT a true default import but allowSyntheticDefaultImports is true, allowing it"
                            );
                        } else {
                            tracing::debug!(
                                "IS a true default import, will emit TS1192 later if needed"
                            );
                        }

                        // For default imports without a default export:
                        // If allowSyntheticDefaultImports is enabled, return namespace type
                        if self.ctx.allow_synthetic_default_imports() {
                            // Create a namespace type from all module exports
                            let exports_table = self.resolve_effective_module_exports(module_name);

                            if let Some(exports_table) = exports_table {
                                use tsz_solver::PropertyInfo;
                                let mut props: Vec<PropertyInfo> = Vec::new();
                                for (name, &export_sym_id) in exports_table.iter() {
                                    let prop_type = self.get_type_of_symbol(export_sym_id);
                                    let name_atom = self.ctx.types.intern_string(name);
                                    props.push(PropertyInfo {
                                        name: name_atom,
                                        type_id: prop_type,
                                        write_type: prop_type,
                                        optional: false,
                                        readonly: false,
                                        is_method: false,
                                        visibility: Visibility::Public,
                                        parent_id: None,
                                    });
                                }
                                let module_type = factory.object(props);
                                return (module_type, Vec::new());
                            }
                        }
                        // TS1192: Module '{0}' has no default export.
                        self.emit_no_default_export_error(module_name, value_decl);
                    } else {
                        // TS2305: Module '{0}' has no exported member '{1}'.
                        self.emit_no_exported_member_error(module_name, export_name, value_decl);
                    }
                } else {
                    // Module not found at all - emit TS2307
                    self.emit_module_not_found_error(module_name, value_decl);
                }
                return (TypeId::ERROR, Vec::new());
            }

            // Unresolved alias - return ANY to prevent cascading TS2571 errors
            return (TypeId::ANY, Vec::new());
        }

        // Fallback: return ANY for unresolved symbols to prevent cascading errors
        // The actual "cannot find" error should already be emitted elsewhere
        (TypeId::ANY, Vec::new())
    }

    #[tracing::instrument(level = "debug", skip(self), fields(decl_idx = %decl_idx.0))]
    fn is_true_default_import_binding(&self, decl_idx: NodeIndex) -> bool {
        if decl_idx.is_none() {
            tracing::debug!("decl_idx is none, returning true (treat as default import)");
            return true;
        }

        // If this declaration is (or is nested under) `import { default as X }`,
        // it's a named import lookup, not a default import binding.
        let mut probe = decl_idx;
        for i in 0..8 {
            let Some(node) = self.ctx.arena.get(probe) else {
                tracing::trace!(iteration = i, "no node found, breaking");
                break;
            };
            tracing::trace!(iteration = i, probe = %probe.0, kind = ?node.kind, "checking node");
            if node.kind == syntax_kind_ext::IMPORT_SPECIFIER
                && let Some(specifier) = self.ctx.arena.get_specifier(node)
            {
                tracing::debug!("found IMPORT_SPECIFIER node");
                let imported_name_idx = if specifier.property_name.is_none() {
                    specifier.name
                } else {
                    specifier.property_name
                };
                if let Some(imported_name_node) = self.ctx.arena.get(imported_name_idx)
                    && let Some(imported_ident) = self.ctx.arena.get_identifier(imported_name_node)
                    && imported_ident.escaped_text.as_str() == "default"
                {
                    tracing::debug!(imported_name = %imported_ident.escaped_text, "found 'default' specifier, this is a NAMED import, returning false");
                    return false;
                }
            }
            let Some(ext) = self.ctx.arena.get_extended(probe) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            probe = ext.parent;
        }

        let mut current = decl_idx;
        let mut import_decl_idx = NodeIndex::NONE;
        for _ in 0..8 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                break;
            };
            if parent_node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                import_decl_idx = parent;
                break;
            }
            current = parent;
        }

        if import_decl_idx.is_none() {
            // Unknown shape: prefer default-import semantics to avoid false TS2305.
            tracing::debug!("import_decl_idx is none, returning true (treat as default import)");
            return true;
        }

        let Some(import_decl_node) = self.ctx.arena.get(import_decl_idx) else {
            tracing::debug!("no import_decl_node found, returning false");
            return false;
        };
        let Some(import_decl) = self.ctx.arena.get_import_decl(import_decl_node) else {
            tracing::debug!("no import_decl found, returning false");
            return false;
        };
        let Some(clause_node) = self.ctx.arena.get(import_decl.import_clause) else {
            tracing::debug!("no clause_node found, returning false");
            return false;
        };
        let Some(clause) = self.ctx.arena.get_import_clause(clause_node) else {
            tracing::debug!("no clause found, returning false");
            return false;
        };

        let result = clause.name.is_some();
        tracing::debug!(
            has_clause_name = clause.name.is_some(),
            result,
            "checked clause.name, returning"
        );
        result
    }

    pub(crate) fn contextual_literal_type(&mut self, literal_type: TypeId) -> Option<TypeId> {
        let ctx_type = self.ctx.contextual_type?;
        self.contextual_type_allows_literal(ctx_type, literal_type)
            .then_some(literal_type)
    }

    pub(crate) fn contextual_type_allows_literal(
        &mut self,
        ctx_type: TypeId,
        literal_type: TypeId,
    ) -> bool {
        let mut visited = FxHashSet::default();
        self.contextual_type_allows_literal_inner(ctx_type, literal_type, &mut visited)
    }

    pub(crate) fn contextual_type_allows_literal_inner(
        &mut self,
        ctx_type: TypeId,
        literal_type: TypeId,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        if ctx_type == literal_type {
            return true;
        }
        if !visited.insert(ctx_type) {
            return false;
        }

        // Resolve Lazy(DefId) types before classification. Type aliases like
        // `type Direction = "north" | "south"` are Lazy until resolved.
        if let Some(def_id) = tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, ctx_type) {
            // Try type_env first
            let resolved = {
                let env = self.ctx.type_env.borrow();
                env.get_def(def_id)
            };
            if let Some(resolved) = resolved
                && resolved != ctx_type
            {
                return self.contextual_type_allows_literal_inner(resolved, literal_type, visited);
            }
            // If not resolved, use centralized relation precondition setup to populate type_env.
            self.ensure_relation_input_ready(ctx_type);
            let resolved = {
                let env = self.ctx.type_env.borrow();
                env.get_def(def_id)
            };
            if let Some(resolved) = resolved
                && resolved != ctx_type
            {
                return self.contextual_type_allows_literal_inner(resolved, literal_type, visited);
            }
            return false;
        }

        // Evaluate KeyOf and IndexAccess types to their concrete form before
        // classification. E.g., keyof Person → "name" | "age".
        if tsz_solver::type_queries::is_keyof_type(self.ctx.types, ctx_type)
            || tsz_solver::type_queries::is_index_access_type(self.ctx.types, ctx_type)
        {
            let evaluated = self.evaluate_type_with_env(ctx_type);
            if evaluated != ctx_type && evaluated != TypeId::ERROR {
                return self.contextual_type_allows_literal_inner(evaluated, literal_type, visited);
            }
        }

        match classify_for_contextual_literal(self.ctx.types, ctx_type) {
            ContextualLiteralAllowKind::Members(members) => members.iter().any(|&member| {
                self.contextual_type_allows_literal_inner(member, literal_type, visited)
            }),
            // Type parameters always allow literal types. In TypeScript, when the
            // expected type is a type parameter (e.g., K extends keyof T), the literal
            // is preserved and the constraint is checked later during generic inference.
            ContextualLiteralAllowKind::TypeParameter { .. }
            | ContextualLiteralAllowKind::TemplateLiteral => true,
            ContextualLiteralAllowKind::Application => {
                let expanded = self.evaluate_application_type(ctx_type);
                if expanded != ctx_type {
                    return self.contextual_type_allows_literal_inner(
                        expanded,
                        literal_type,
                        visited,
                    );
                }
                false
            }
            ContextualLiteralAllowKind::Mapped => {
                let expanded = self.evaluate_mapped_type_with_resolution(ctx_type);
                if expanded != ctx_type {
                    return self.contextual_type_allows_literal_inner(
                        expanded,
                        literal_type,
                        visited,
                    );
                }
                false
            }
            ContextualLiteralAllowKind::NotAllowed => false,
        }
    }

    /// Check if a type node is a simple type reference without structural wrapping.
    ///
    /// Returns true for bare type references like `type A = B`, false for wrapped
    /// references like `type A = { x: B }` or `type A = B | null`.
    fn is_simple_type_reference(&self, type_node: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(type_node) else {
            return false;
        };

        // Type reference or identifier without structural wrapping
        matches!(
            node.kind,
            k if k == syntax_kind_ext::TYPE_REFERENCE || k == SyntaxKind::Identifier as u16
        )
    }

    /// Check if a type alias directly circularly references itself.
    ///
    /// Returns true when a type alias resolves to itself without structural wrapping,
    /// which is invalid: `type A = B; type B = A;`
    ///
    /// Returns false for valid recursive types that use structural wrapping:
    /// `type List = { value: number; next: List | null };`
    fn is_direct_circular_reference(
        &self,
        sym_id: SymbolId,
        resolved_type: TypeId,
        type_node: NodeIndex,
    ) -> bool {
        // Check if resolved_type is Lazy(DefId) pointing back to sym_id
        if let Some(def_id) =
            tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, resolved_type)
        {
            // Map DefId back to SymbolId
            if let Some(&target_sym_id) = self.ctx.def_to_symbol.borrow().get(&def_id)
                && target_sym_id == sym_id
            {
                // It's a self-reference - check if it's direct (no structural wrapping)
                return self.is_simple_type_reference(type_node);
            }
        }

        // Also check union/intersection members for circular references.
        if let Some(members) =
            tsz_solver::type_queries::get_union_members(self.ctx.types, resolved_type)
        {
            for &member in &members {
                if self.is_direct_circular_reference(sym_id, member, type_node) {
                    return true;
                }
            }
        }
        if let Some(members) =
            tsz_solver::type_queries::get_intersection_members(self.ctx.types, resolved_type)
        {
            for &member in &members {
                if self.is_direct_circular_reference(sym_id, member, type_node) {
                    return true;
                }
            }
        }

        false
    }

    fn report_private_identifier_outside_class(
        &mut self,
        name_idx: NodeIndex,
        property_name: &str,
        object_type: TypeId,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        let class_name = self
            .get_class_name_from_type(object_type)
            .unwrap_or_else(|| "the class".to_string());
        let message = format_message(
            diagnostic_messages::PROPERTY_IS_NOT_ACCESSIBLE_OUTSIDE_CLASS_BECAUSE_IT_HAS_A_PRIVATE_IDENTIFIER,
            &[property_name, &class_name],
        );
        self.error_at_node(
            name_idx,
            &message,
            diagnostic_codes::PROPERTY_IS_NOT_ACCESSIBLE_OUTSIDE_CLASS_BECAUSE_IT_HAS_A_PRIVATE_IDENTIFIER,
        );
    }

    fn report_private_identifier_shadowed(
        &mut self,
        name_idx: NodeIndex,
        property_name: &str,
        object_type: TypeId,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        let type_string = self
            .get_class_name_from_type(object_type)
            .unwrap_or_else(|| "the type".to_string());
        let message = format_message(
            diagnostic_messages::THE_PROPERTY_CANNOT_BE_ACCESSED_ON_TYPE_WITHIN_THIS_CLASS_BECAUSE_IT_IS_SHADOWED,
            &[property_name, &type_string],
        );
        self.error_at_node(
            name_idx,
            &message,
            diagnostic_codes::THE_PROPERTY_CANNOT_BE_ACCESSED_ON_TYPE_WITHIN_THIS_CLASS_BECAUSE_IT_IS_SHADOWED,
        );
    }

    // Resolve a typeof type reference to its structural type.
    //
    // This function resolves `typeof X` type queries to the actual type of `X`.
    // This is useful for type operations where we need the structural type rather
    // than the type query itself.
    // **TypeQuery Resolution:**
    // - **TypeQuery**: `typeof X` → get the type of symbol X
    // - **Other types**: Return unchanged (not a typeof query)
    //
    // **Use Cases:**
    // - Assignability checking (need actual type, not typeof reference)
    // - Type comparison (typeof X should be compared to X's type)
    // - Generic constraint evaluation
    // NOTE: refine_mixin_call_return_type, mixin_base_param_index, instance_type_from_constructor_type,
    // instance_type_from_constructor_type_inner, merge_base_instance_into_constructor_return,
    // merge_base_constructor_properties_into_constructor_return moved to constructor_checker.rs

    pub(crate) fn get_type_of_private_property_access(
        &mut self,
        idx: NodeIndex,
        access: &tsz_parser::parser::node::AccessExprData,
        name_idx: NodeIndex,
        object_type: TypeId,
    ) -> TypeId {
        let factory = self.ctx.types.factory();
        use tsz_solver::operations_property::PropertyAccessResult;

        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return TypeId::ERROR; // Missing identifier data - propagate error
        };
        let property_name = ident.escaped_text.clone();

        let (symbols, saw_class_scope) = self.resolve_private_identifier_symbols(name_idx);

        // NOTE: Do NOT emit TS18016 here for property access expressions.
        // `obj.#prop` is always valid syntax — the private identifier in a property
        // access position is grammatically correct. TSC only emits TS18016 for truly
        // invalid positions (object literals, standalone expressions). For property
        // access, the error is always semantic (TS18013: can't access private member),
        // which is handled below based on the object's type.

        // Evaluate for type checking but preserve original for error messages
        // This preserves nominal identity (e.g., D<string>) in error messages
        let original_object_type = object_type;
        let object_type = self.evaluate_application_type(object_type);

        // Property access on `never` returns `never` (bottom type propagation).
        // TSC does not emit TS18050 for property access on `never` — the result is
        // simply `never`, which allows exhaustive narrowing patterns to work correctly.
        if object_type == TypeId::NEVER {
            return TypeId::NEVER;
        }

        let (object_type_for_check, nullish_cause) = self.split_nullish_type(object_type);
        let Some(object_type_for_check) = object_type_for_check else {
            if access.question_dot_token {
                return TypeId::UNDEFINED;
            }
            if let Some(cause) = nullish_cause {
                // Type is entirely nullish - emit TS18050 "The value X cannot be used here"
                self.report_nullish_object(access.expression, cause, true);
            }
            return TypeId::ERROR;
        };

        // When symbols are empty but we're inside a class scope, check if the object type
        // itself has private properties matching the name. This handles cases like:
        //   let a: A2 = this;
        //   a.#prop;  // Should work if A2 has #prop
        if symbols.is_empty() {
            // Resolve type references (Ref, TypeQuery, etc.) before property access lookup
            let resolved_type = self.resolve_type_for_property_access(object_type_for_check);

            // Try to find the property directly in the resolved object type
            use tsz_solver::operations_property::PropertyAccessResult;
            match self
                .ctx
                .types
                .property_access_type(resolved_type, &property_name)
            {
                PropertyAccessResult::Success { .. } => {
                    // Property exists in the type, but if we're outside a class, it's TS18013
                    if !saw_class_scope {
                        self.report_private_identifier_outside_class(
                            name_idx,
                            &property_name,
                            original_object_type,
                        );
                        return TypeId::ERROR;
                    }
                    // Property exists in the type and we're in a class scope, proceed with the access
                    return self.get_type_of_property_access_by_name(
                        idx,
                        access,
                        resolved_type,
                        &property_name,
                    );
                }
                _ => {
                    // FALLBACK: Manually check if the property exists in the callable type
                    // This fixes cases where property_access_type fails due to atom comparison issues
                    // The property IS in the type (as shown by error messages), but the lookup fails
                    if let Some(shape) =
                        crate::query_boundaries::state_type_analysis::callable_shape_for_type(
                            self.ctx.types,
                            resolved_type,
                        )
                    {
                        let prop_atom = self.ctx.types.intern_string(&property_name);
                        for prop in &shape.properties {
                            if prop.name == prop_atom {
                                // Property found in the callable's properties list!
                                // But if we're outside a class, it's TS18013
                                if !saw_class_scope {
                                    self.report_private_identifier_outside_class(
                                        name_idx,
                                        &property_name,
                                        original_object_type,
                                    );
                                    return TypeId::ERROR;
                                }
                                // Return the property type (handle optional and write_type)
                                let prop_type = if prop.optional {
                                    factory.union(vec![prop.type_id, TypeId::UNDEFINED])
                                } else {
                                    prop.type_id
                                };
                                return self.apply_flow_narrowing(idx, prop_type);
                            }
                        }
                    }

                    // Property not found, emit error if appropriate
                    if saw_class_scope {
                        // Use original_object_type to preserve nominal identity (e.g., D<string>)
                        self.error_property_not_exist_at(
                            &property_name,
                            original_object_type,
                            name_idx,
                        );
                    } else {
                        self.report_private_identifier_outside_class(
                            name_idx,
                            &property_name,
                            original_object_type,
                        );
                    }
                    return TypeId::ERROR;
                }
            }
        }

        let declaring_type = match self.private_member_declaring_type(symbols[0]) {
            Some(ty) => ty,
            None => {
                if saw_class_scope {
                    // Use original_object_type to preserve nominal identity (e.g., D<string>)
                    self.error_property_not_exist_at(
                        &property_name,
                        original_object_type,
                        name_idx,
                    );
                } else {
                    self.report_private_identifier_outside_class(
                        name_idx,
                        &property_name,
                        original_object_type,
                    );
                }
                return TypeId::ERROR;
            }
        };

        if object_type_for_check == TypeId::ANY {
            return TypeId::ANY;
        }
        if object_type_for_check == TypeId::ERROR {
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        }
        if object_type_for_check == TypeId::UNKNOWN {
            return TypeId::ANY; // UNKNOWN remains ANY for now (could be stricter)
        }

        // For private member access, use nominal typing based on private brand.
        // If both types have the same private brand, they're from the same class
        // declaration and the access should be allowed.
        let types_compatible =
            if self.types_have_same_private_brand(object_type_for_check, declaring_type) {
                true
            } else {
                self.is_assignable_to(object_type_for_check, declaring_type)
            };

        if !types_compatible {
            let shadowed = symbols.iter().skip(1).any(|sym_id| {
                self.private_member_declaring_type(*sym_id)
                    .is_some_and(|ty| {
                        if self.types_have_same_private_brand(object_type_for_check, ty) {
                            true
                        } else {
                            self.is_assignable_to(object_type_for_check, ty)
                        }
                    })
            });
            if shadowed {
                self.report_private_identifier_shadowed(
                    name_idx,
                    &property_name,
                    original_object_type,
                );
                return TypeId::ERROR;
            }

            // Use original_object_type to preserve nominal identity (e.g., D<string>)
            self.error_property_not_exist_at(&property_name, original_object_type, name_idx);
            return TypeId::ERROR;
        }

        let declaring_type = self.resolve_type_for_property_access(declaring_type);
        let mut result_type = match self
            .ctx
            .types
            .property_access_type(declaring_type, &property_name)
        {
            PropertyAccessResult::Success {
                type_id,
                from_index_signature,
                ..
            } => {
                if from_index_signature {
                    // Private fields can't come from index signatures
                    // Use original_object_type to preserve nominal identity (e.g., D<string>)
                    self.error_property_not_exist_at(
                        &property_name,
                        original_object_type,
                        name_idx,
                    );
                    return TypeId::ERROR;
                }
                type_id
            }
            PropertyAccessResult::PropertyNotFound { .. } => {
                // If we got here, we already resolved the symbol, so the private field exists.
                // The solver might not find it due to type encoding issues.
                // FALLBACK: Try to manually find the property in the callable type
                if let Some(shape) =
                    crate::query_boundaries::state_type_analysis::callable_shape_for_type(
                        self.ctx.types,
                        declaring_type,
                    )
                {
                    let prop_atom = self.ctx.types.intern_string(&property_name);
                    for prop in &shape.properties {
                        if prop.name == prop_atom {
                            // Property found! Return its type
                            return if prop.optional {
                                factory.union(vec![prop.type_id, TypeId::UNDEFINED])
                            } else {
                                prop.type_id
                            };
                        }
                    }
                }
                // Property not found even in fallback, return ANY for type recovery
                TypeId::ANY
            }
            PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                property_type.unwrap_or(TypeId::UNKNOWN)
            }
            PropertyAccessResult::IsUnknown => {
                // TS2339: Property does not exist on type 'unknown'
                // Use the same error as TypeScript for property access on unknown
                // Use original_object_type to preserve nominal identity (e.g., D<string>)
                self.error_property_not_exist_at(&property_name, original_object_type, name_idx);
                TypeId::ERROR
            }
        };

        if let Some(cause) = nullish_cause {
            if access.question_dot_token {
                result_type = factory.union(vec![result_type, TypeId::UNDEFINED]);
            } else {
                self.report_possibly_nullish_object(access.expression, cause);
            }
        }

        self.apply_flow_narrowing(idx, result_type)
    }
}
