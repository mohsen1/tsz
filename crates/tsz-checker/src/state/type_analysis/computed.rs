//! Computed symbol type analysis: `compute_type_of_symbol`, contextual literal types,
//! and private property access checking.

use crate::query_boundaries::common::{
    array_element_type, contains_infer_types, contains_type_parameters, is_generic_type,
};
use crate::query_boundaries::flow as flow_boundary;
use crate::query_boundaries::state::type_environment;
use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{TypeId, Visibility};
impl<'a> CheckerState<'a> {
    pub(crate) fn type_has_unresolved_inference_holes(&self, type_id: TypeId) -> bool {
        contains_type_parameters(self.ctx.types, type_id)
            || contains_infer_types(self.ctx.types, type_id)
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
                None => {
                    // Also try the cross-file symbol
                    match self.get_cross_file_symbol(sym_id) {
                        Some(symbol) => (
                            symbol.flags,
                            symbol.value_declaration,
                            symbol.declarations.clone(),
                            symbol.import_module.clone(),
                            symbol.import_name.clone(),
                            symbol.escaped_name.clone(),
                        ),
                        None => return (TypeId::UNKNOWN, Vec::new()),
                    }
                }
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
        if flags & symbol_flags::EXPORT_VALUE != 0
            && flags & symbol_flags::ALIAS != 0
            && import_module.is_none()
            && let Some(wrapped_type) =
                self.compute_local_export_value_wrapper_type(sym_id, value_decl, &escaped_name)
        {
            return (wrapped_type, Vec::new());
        }

        // Import alias targeting a cross-file class+namespace merge.
        //
        // When `import { X } from "./m"` imports a symbol that has both CLASS and
        // NAMESPACE_MODULE flags, the local import alias only carries
        // ALIAS | NAMESPACE_MODULE (not CLASS).  Without this guard the
        // NAMESPACE_MODULE branch below would return Lazy(DefId) — a type that
        // only exposes namespace exports and misses class constructor properties
        // like `prototype`.
        //
        // Resolve the import target to its original symbol and delegate to
        // `get_type_of_symbol`, which sees the full CLASS | NAMESPACE_MODULE flags
        // and produces the class constructor type merged with namespace exports.
        if flags & symbol_flags::ALIAS != 0
            && flags & symbol_flags::CLASS == 0
            && flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0
            && let Some(ref module_spec) = import_module
        {
            let target_name = import_name.as_deref().unwrap_or(&escaped_name);
            let target_sym_id = self
                .ctx
                .binder
                .resolve_import_with_reexports_type_only(module_spec, target_name)
                .map(|(sym_id, _is_type_only)| sym_id);

            if let Some(target_sym_id) = target_sym_id
                && target_sym_id != sym_id
                && let Some(target_symbol) = self.get_symbol_globally(target_sym_id)
                && (target_symbol.flags & symbol_flags::CLASS) != 0
            {
                let target_type = self.get_type_of_symbol(target_sym_id);
                // Also cache the instance type so type-position references
                // (`let x: Observable<number>`) continue to work.
                if let Some(&inst) = self.ctx.symbol_instance_types.get(&target_sym_id) {
                    self.ctx.symbol_instance_types.insert(sym_id, inst);
                }
                return (target_type, Vec::new());
            }
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

            // Collect all enum declaration nodes. Merged enums (multiple
            // `const enum E { ... }` blocks) contribute members from every
            // declaration, so we must iterate all of them.
            let enum_decl_indices: Vec<NodeIndex> = {
                let mut indices = Vec::new();
                for &decl in &declarations {
                    if decl.is_some() && self.ctx.arena.get_enum_at(decl).is_some() {
                        indices.push(decl);
                    }
                }
                // Fallback: if no declaration matched as enum, try value_decl
                if indices.is_empty()
                    && value_decl.is_some()
                    && self.ctx.arena.get_enum_at(value_decl).is_some()
                {
                    indices.push(value_decl);
                }
                indices
            };

            // Compute the union type of all enum member types.
            // Also pre-cache each member symbol type so `E.Member` property access
            // can hit `ctx.symbol_types` directly instead of running full symbol
            // resolution for each distinct member.
            let mut member_types = Vec::new();
            // Track auto-increment counter for numeric enum members.
            // TypeScript auto-increments from 0 for the first member, and from
            // previous_value + 1 for subsequent members without initializers.
            // When a member has an explicit numeric initializer, the counter
            // resets to initializer_value + 1. String initializers break auto-increment.
            // The counter resets at the start of each declaration block.
            //
            // We collect (member_type, member_name, member_idx) tuples first,
            // then do env updates in a separate pass to avoid borrow conflicts
            // with `self.enum_member_type_from_decl` / `self.evaluate_constant_expression`.
            let mut member_entries: Vec<(TypeId, Option<String>, NodeIndex)> = Vec::new();
            for &decl_idx in &enum_decl_indices {
                let Some(enum_decl) = self.ctx.arena.get_enum_at(decl_idx) else {
                    continue;
                };
                member_types.reserve(enum_decl.members.nodes.len());
                let mut auto_value: Option<f64> = Some(0.0);
                for &member_idx in &enum_decl.members.nodes {
                    if let Some(member) = self.ctx.arena.get_enum_member_at(member_idx) {
                        let has_initializer = member.initializer.is_some();
                        let mut member_type = self.enum_member_type_from_decl(member_idx);

                        if has_initializer {
                            // Member has explicit initializer. Evaluate it to determine
                            // the next auto-increment value.
                            if let Some(val) = self.evaluate_constant_expression(member.initializer)
                            {
                                auto_value = Some(val + 1.0);
                            } else {
                                // String literal or unevaluable — auto-increment is broken
                                auto_value = None;
                            }
                        } else if member_type == TypeId::NUMBER {
                            // No explicit initializer — use auto-increment if available.
                            // This fixes mapped types over numeric enums: { [k in E]?: string }
                            // needs individual property keys ("0", "1", "2"), not `number`.
                            if let Some(val) = auto_value {
                                member_type = factory.literal_number(val);
                                auto_value = Some(val + 1.0);
                            }
                        }

                        if member_type != TypeId::ERROR {
                            member_types.push(member_type);
                        }

                        // Collect member info for env caching below.
                        let member_name = self.get_property_name(member.name);
                        member_entries.push((member_type, member_name, member_idx));
                    }
                }
            }

            // Pre-cache member symbol types (separate pass to avoid borrow conflicts).
            // This avoids per-member `get_type_of_symbol` overhead in
            // hot paths such as large enum property-access switches.
            //
            // Collect (member_def_id, member_enum_type) pairs so we can mirror
            // them into type_environment after releasing the type_env borrow.
            let mut member_def_entries: Vec<(tsz_solver::DefId, TypeId)> = Vec::new();
            {
                let mut maybe_env = self.ctx.type_env.try_borrow_mut().ok();
                for &(member_type, ref member_name, _member_idx) in &member_entries {
                    if let Some(name) = member_name
                        && let Some(member_sym_id) = self
                            .ctx
                            .binder
                            .get_symbol(sym_id)
                            .and_then(|enum_symbol| enum_symbol.exports.as_ref())
                            .and_then(|exports| exports.get(name))
                    {
                        let member_def_id = self.ctx.get_or_create_def_id(member_sym_id);
                        let member_enum_type = factory.enum_type(member_def_id, member_type);
                        self.ctx
                            .symbol_types
                            .insert(member_sym_id, member_enum_type);
                        if let Some(env) = maybe_env.as_mut() {
                            env.insert(tsz_solver::SymbolRef(member_sym_id.0), member_enum_type);
                            if member_def_id != tsz_solver::DefId::INVALID {
                                env.insert_def(member_def_id, member_enum_type);
                                // Register parent-child relationship for enum member widening
                                env.register_enum_parent(member_def_id, def_id);
                                member_def_entries.push((member_def_id, member_enum_type));
                            }
                        }
                    }
                }
            }
            // Mirror enum member DefId entries into type_environment for consistency
            if !member_def_entries.is_empty()
                && let Ok(mut env) = self.ctx.type_environment.try_borrow_mut()
            {
                for &(member_def_id, member_enum_type) in &member_def_entries {
                    env.insert_def(member_def_id, member_enum_type);
                    env.register_enum_parent(member_def_id, def_id);
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

            // Cache the structural type in both environments for compatibility.
            // Note: Enum types now use TypeData::Enum(def_id, member_type) directly.
            self.ctx.register_def_in_envs(def_id, structural_type);

            // CRITICAL: Return TypeData::Enum(def_id, structural_type) NOT Lazy(def_id)
            // - Lazy(def_id) creates infinite recursion in ensure_refs_resolved
            // - structural_type alone loses nominal identity (E1 becomes 0 | 1)
            // - Enum(def_id, structural_type) preserves both:
            //   1. DefId for nominal identity (E1 != E2)
            //   2. structural_type for assignability to primitives (E1 <: number)
            let enum_type = factory.enum_type(def_id, structural_type);

            // Compute and cache the enum namespace object type for `typeof Enum` / `keyof typeof Enum`.
            // This object has member names as properties (e.g., { Up: Direction.Up, Down: Direction.Down }).
            // Always compute this — both plain enums and enum+namespace merges need it.
            let ns_type = self.merge_namespace_exports_into_object(sym_id, enum_type);
            self.ctx.enum_namespace_types.insert(sym_id, ns_type);
            // Register in both TypeEnvironment instances so the solver's evaluator
            // and the flow analyzer can both access enum namespace types.
            if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
                env.register_enum_namespace_type(def_id, ns_type);
            }
            if let Ok(mut env) = self.ctx.type_environment.try_borrow_mut() {
                env.register_enum_namespace_type(def_id, ns_type);
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
            && flags & symbol_flags::VARIABLE == 0
        {
            return self.compute_namespace_symbol_type(sym_id, flags);
        }

        // Enum member - determine type from parent enum
        if flags & symbol_flags::ENUM_MEMBER != 0 {
            return self.compute_enum_member_symbol_type(sym_id, value_decl);
        }

        // Get/Set accessors - resolve type from the accessor declaration's type annotation.
        // For get accessors, the type is the return type annotation (or inferred from body).
        // For set accessors, the type is the first parameter's type annotation.
        if flags & (symbol_flags::GET_ACCESSOR | symbol_flags::SET_ACCESSOR) != 0 {
            for &decl_idx in &declarations {
                let Some(node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                let Some(accessor) = self.ctx.arena.get_accessor(node) else {
                    continue;
                };
                if node.kind == syntax_kind_ext::GET_ACCESSOR {
                    // Get accessor: return type is the type annotation
                    if accessor.type_annotation.is_some() {
                        let return_type = self.get_type_from_type_node(accessor.type_annotation);
                        return (return_type, Vec::new());
                    }
                    // No type annotation - try to infer from body return type
                    // Fall through to use get_type_of_node if body exists
                    if accessor.body.is_some() {
                        let body_type = self.get_type_of_node(accessor.body);
                        if body_type != TypeId::ERROR && body_type != TypeId::UNKNOWN {
                            return (body_type, Vec::new());
                        }
                    }
                } else if node.kind == syntax_kind_ext::SET_ACCESSOR {
                    // Set accessor: type is the first parameter's type annotation
                    if let Some(&param_idx) = accessor.parameters.nodes.first()
                        && let Some(param_node) = self.ctx.arena.get(param_idx)
                        && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        && param.type_annotation.is_some()
                    {
                        let param_type = self.get_type_from_type_node(param.type_annotation);
                        return (param_type, Vec::new());
                    }
                }
            }
        }

        // Methods merged across lib/interface declarations should preserve overloads from
        // every declaration arena, not just the first value declaration.
        if flags & symbol_flags::METHOD != 0 {
            let mut merged_method_type = None;

            for &decl_idx in &declarations {
                let decl_type = self.type_of_declaration_node_for_symbol(sym_id, decl_idx);
                if matches!(decl_type, TypeId::ERROR | TypeId::UNKNOWN) {
                    continue;
                }

                merged_method_type = Some(if let Some(current) = merged_method_type {
                    self.merge_interface_types(decl_type, current)
                } else {
                    decl_type
                });
            }

            if let Some(method_type) = merged_method_type {
                return (method_type, Vec::new());
            }
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
                    is_abstract: false,
                };
                factory.callable(shape)
            } else if value_decl.is_some() {
                self.get_type_of_function(value_decl)
            } else if implementation_decl.is_some() {
                self.get_type_of_function(implementation_decl)
            } else {
                TypeId::UNKNOWN
            };

            let function_type =
                self.augment_callable_type_with_expandos(&escaped_name, sym_id, function_type);

            // If function is merged with namespace, merge namespace exports into function type
            // This allows accessing namespace members through the function name: Model.Options
            if flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0 {
                // Pre-cache the function type before merging namespace exports.
                // This breaks circularity when the namespace body references the function
                // itself (e.g., `namespace point { export var origin = point(0, 0); }`).
                // Without this, the placeholder is Lazy(DefId) with no call signatures,
                // causing false TS2349 "not callable" errors.
                self.ctx.symbol_types.insert(sym_id, function_type);
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
            // Detect cross-file declarations sharing the same NodeIndex as
            // a local declaration. The binder merge skips duplicate NodeIndex
            // values, so `declarations` has only one entry but
            // `declaration_arenas` stores multiple arenas for it.
            let has_cross_file_same_index = declarations.iter().any(|&decl_idx| {
                self.ctx
                    .binder
                    .declaration_arenas
                    .get(&(sym_id, decl_idx))
                    .is_some_and(|arenas| {
                        arenas.len() > 1
                            && arenas
                                .iter()
                                .any(|a| !std::ptr::eq(a.as_ref(), self.ctx.arena))
                    })
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
            // When all declarations are from lib arenas (no local interface
            // declarations), resolve via the lib type directly. But when the
            // user has local interface declarations that augment/extend the lib
            // type (e.g., `interface Node { forEachChild(...) }`), we must fall
            // through to the full merge path so user-declared members are included.
            let has_local_interface_decl = declarations.iter().any(|&decl_idx| {
                self.ctx
                    .arena
                    .get(decl_idx)
                    .and_then(|node| self.ctx.arena.get_interface(node))
                    .is_some()
            });
            if (has_out_of_arena_decl || is_lib_symbol)
                && !has_local_interface_decl
                && !self.ctx.lib_contexts.is_empty()
                && let Some(lib_type) = self.resolve_lib_type_by_name(&escaped_name)
            {
                // Preserve diagnostic formatting for canonical lib interfaces
                // by recording the resolved object shape on this symbol's DefId.
                let def_id = self.ctx.get_or_create_def_id(sym_id);
                if let Some(shape) = type_environment::object_shape(self.ctx.types, lib_type) {
                    self.ctx.definition_store.set_instance_shape(def_id, shape);
                }

                return (lib_type, Vec::new());
            }

            if !declarations.is_empty() {
                // Get type parameters from the first interface declaration.
                // When cross-file declarations exist, the first declaration may be
                // from another arena. Try all local declarations to find type params.
                let mut params = Vec::new();
                let mut updates = Vec::new();

                if has_out_of_arena_decl {
                    for &decl_idx in declarations.iter() {
                        if let Some(node) = self.ctx.arena.get(decl_idx)
                            && let Some(interface) = self.ctx.arena.get_interface(node)
                        {
                            (params, updates) =
                                self.push_type_parameters(&interface.type_parameters);
                            break;
                        }
                    }
                } else {
                    let first_decl = declarations.first().copied().unwrap_or(NodeIndex::NONE);
                    if let Some(node) = self.ctx.arena.get(first_decl)
                        && let Some(interface) = self.ctx.arena.get_interface(node)
                    {
                        (params, updates) = self.push_type_parameters(&interface.type_parameters);
                    }
                }

                // Pre-compute computed property names that the lowering can't resolve from AST alone.
                let computed_names = self.precompute_computed_property_names(&declarations);
                let prewarmed_type_params =
                    self.prewarm_member_type_reference_params(&declarations);
                let namespace_prefix = declarations.iter().copied().find_map(|decl_idx| {
                    self.ctx
                        .arena
                        .get(decl_idx)
                        .and_then(|node| self.ctx.arena.get_interface(node))
                        .and_then(|_| self.declaration_namespace_prefix(self.ctx.arena, decl_idx))
                });

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
                let computed_name_resolver = |expr_idx: NodeIndex| -> Option<tsz_common::Atom> {
                    computed_names.get(&expr_idx).copied()
                };
                let lazy_type_params_resolver = |def_id: tsz_solver::def::DefId| {
                    prewarmed_type_params
                        .get(&def_id)
                        .cloned()
                        .or_else(|| self.ctx.get_def_type_params(def_id))
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
                let lowering = TypeLowering::with_hybrid_resolver(
                    self.ctx.arena,
                    self.ctx.types,
                    &type_resolver,
                    &def_id_resolver,
                    &value_resolver,
                )
                .with_type_param_bindings(type_param_bindings)
                .with_computed_name_resolver(&computed_name_resolver)
                .with_lazy_type_params_resolver(&lazy_type_params_resolver)
                .with_name_def_id_resolver(&name_resolver);
                let mut interface_type =
                    lowering.lower_interface_declarations_with_symbol(&declarations, sym_id);

                // Cross-file interface declaration merging: when declarations from
                // other arenas exist, lower each with a TypeLowering bound to its
                // source arena and merge the members structurally.
                // Handles both cases:
                //  - Different NodeIndex (has_out_of_arena_decl): decl not in local arena
                //  - Same NodeIndex collision (has_cross_file_same_index): decl IS in
                //    local arena, but declaration_arenas has additional non-local arenas
                if has_out_of_arena_decl || has_cross_file_same_index {
                    for &decl_idx in declarations.iter() {
                        let Some(arenas) =
                            self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx))
                        else {
                            continue;
                        };
                        for arena in arenas.iter() {
                            // Skip the local arena — already lowered above
                            if std::ptr::eq(arena.as_ref(), self.ctx.arena) {
                                continue;
                            }
                            if let Some(node) = arena.get(decl_idx)
                                && arena.get_interface(node).is_some()
                            {
                                let cross_type =
                                    self.lower_cross_file_interface_decl(arena, decl_idx, sym_id);
                                if cross_type != TypeId::ERROR {
                                    interface_type =
                                        self.merge_interface_types(interface_type, cross_type);
                                }
                            }
                        }
                    }
                }

                let mut interface_type =
                    self.merge_interface_heritage_types(&declarations, interface_type);

                // Merge heritage types from cross-file declarations that
                // merge_interface_heritage_types couldn't process (it uses
                // self.ctx.arena which doesn't contain cross-file nodes).
                if has_out_of_arena_decl || has_cross_file_same_index {
                    interface_type =
                        self.merge_cross_file_heritage(&declarations, sym_id, interface_type);
                }

                if let Some(shape) = type_environment::object_shape(self.ctx.types, interface_type)
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
            if decl_idx.is_some() {
                // When the type alias declaration was found in the user arena
                // (the `find` closure above checks `self.ctx.arena.get(d)`),
                // we MUST use the user arena for node lookup.  The `symbol_arenas`
                // fallback may point to a lib arena (e.g., when a user-defined
                // `type Proxy<T>` merges with the global `declare var Proxy`),
                // causing the user-arena NodeIndex to fail lookup in the lib arena
                // and incorrectly returning TypeId::UNKNOWN.
                let found_in_user_arena = self
                    .ctx
                    .arena
                    .get(decl_idx)
                    .and_then(|n| self.ctx.arena.get_type_alias(n))
                    .is_some();
                let decl_arena = if found_in_user_arena {
                    self.ctx.arena
                } else {
                    self.ctx
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
                        })
                        .unwrap_or(self.ctx.arena)
                };

                let Some(node) = decl_arena.get(decl_idx) else {
                    return (TypeId::UNKNOWN, Vec::new());
                };
                let Some(type_alias) = decl_arena.get_type_alias(node) else {
                    return (TypeId::UNKNOWN, Vec::new());
                };

                let decl_binder = self
                    .ctx
                    .get_binder_for_arena(decl_arena)
                    .unwrap_or(self.ctx.binder);
                let has_cross_arena_metadata = !std::ptr::eq(decl_arena, self.ctx.arena)
                    || decl_binder.symbol_arenas.contains_key(&sym_id)
                    || decl_binder
                        .declaration_arenas
                        .contains_key(&(sym_id, decl_idx));

                // When a local type alias has no own type parameters but is
                // inside a generic function (e.g., `function foo<T>() { type X = T extends ... }`),
                // the enclosing function's type parameters must be in scope during lowering.
                // Push them before lowering and pop after.
                let enclosing_tp_updates = if type_alias.type_parameters.is_none() {
                    self.push_enclosing_type_params_for_node(decl_arena, decl_idx)
                } else {
                    Vec::new()
                };

                let (mut alias_type, params) = if has_cross_arena_metadata {
                    let result = self.lower_cross_arena_type_alias_declaration(
                        sym_id, decl_idx, decl_arena, type_alias,
                    );
                    // When a same-file type alias has cross-arena metadata but the
                    // declaration is in the current arena, resolve TypeQuery references
                    // with flow narrowing. Push type parameters into scope first so
                    // that type args in typeof expressions (e.g. `typeof Foo<U>`)
                    // can resolve them instead of emitting false TS2304.
                    if std::ptr::eq(decl_arena, self.ctx.arena) {
                        let (mut at, params) = result;
                        let (_, tp_updates) =
                            self.push_type_parameters(&type_alias.type_parameters);
                        at = self.resolve_type_queries_with_flow(at, type_alias.type_node);
                        self.pop_type_parameters(tp_updates);
                        (at, params)
                    } else {
                        result
                    }
                } else {
                    let (params, updates) = self.push_type_parameters(&type_alias.type_parameters);
                    let mut alias_type = self.get_type_from_type_node(type_alias.type_node);
                    // Resolve TypeQuery references with flow narrowing while type
                    // parameters are still in scope. This prevents false TS2304
                    // errors for type params used as type arguments in typeof
                    // expressions (e.g. `type Alias<U> = typeof Foo<U>`).
                    if std::ptr::eq(decl_arena, self.ctx.arena) {
                        alias_type =
                            self.resolve_type_queries_with_flow(alias_type, type_alias.type_node);
                    }
                    self.pop_type_parameters(updates);
                    (alias_type, params)
                };

                // Pop enclosing type parameters that were pushed for local type aliases.
                self.pop_type_parameters(enclosing_tp_updates);

                // Eagerly evaluate non-generic type aliases whose body is a concrete
                // conditional type.  tsc resolves `type U = [any] extends [number] ? 1 : 0`
                // to `1` during alias resolution so that diagnostics print the resolved
                // type.  We do the same here: if the alias has no type parameters AND
                // the body is evaluable (Conditional, IndexAccess, Mapped, etc.) AND it
                // does not contain deferred type parameters, evaluate it now.
                if params.is_empty()
                    && tsz_solver::is_conditional_type(self.ctx.types, alias_type)
                    && !tsz_solver::contains_type_parameters(self.ctx.types, alias_type)
                {
                    alias_type = self.evaluate_type_with_env(alias_type);
                }

                // Check for invalid circular reference (TS2456)
                // A type alias circularly references itself if it resolves to itself
                // without structural wrapping (e.g., `type A = B; type B = A;`)
                //
                // Three detection paths:
                // 1. is_direct_circular_reference: same-file cycles via symbol_resolution_set
                // 2. circular_type_aliases: marked by a previous cycle member
                // 3. is_cross_file_circular_alias: cross-file cycles by following
                //    Lazy → body → Lazy chain through shared DefinitionStore
                // Suppress circularity checking when the type alias declaration
                // contains parse errors. tsc skips TS2456 for malformed
                // declarations (e.g. `type T1<in in> = T1`) where syntax errors
                // take priority over semantic circularity detection.
                // Check two signals:
                // 1. node_contains_any_parse_error (from parser error positions)
                // 2. Empty type parameter names (parser recovery creates empty
                //    identifiers when reserved words like `in` appear as names)
                let has_empty_tp_name =
                    type_alias.type_parameters.as_ref().is_some_and(|tp_list| {
                        tp_list.nodes.iter().any(|&tp_idx| {
                            self.ctx
                                .arena
                                .get(tp_idx)
                                .and_then(|tp_node| self.ctx.arena.get_type_parameter(tp_node))
                                .and_then(|tp| {
                                    self.ctx
                                        .arena
                                        .get(tp.name)
                                        .and_then(|n| self.ctx.arena.get_identifier(n))
                                })
                                .is_some_and(|ident| {
                                    self.ctx.arena.resolve_identifier_text(ident).is_empty()
                                })
                        })
                    });
                let decl_has_parse_error =
                    self.node_contains_any_parse_error(decl_idx) || has_empty_tp_name;
                let circularity_eligible = flags & (symbol_flags::ALIAS | symbol_flags::NAMESPACE)
                    == 0
                    && !decl_has_parse_error;
                // When the type alias has type parameters and the body is a bare
                // self-reference (simple type reference without type arguments),
                // TSC does NOT consider it circular.  The bare `T` in
                // `type T<out out> = T` refers to the generic type constructor and
                // goes through instantiation, so it is structurally wrapped.
                let generic_self_ref =
                    !params.is_empty() && self.is_simple_type_reference(type_alias.type_node);
                let is_circular = circularity_eligible
                    && !generic_self_ref
                    && (self.is_direct_circular_reference(
                        sym_id,
                        alias_type,
                        type_alias.type_node,
                        false,
                    ) || self.ctx.circular_type_aliases.contains(&sym_id)
                        || (self.is_simple_type_reference(type_alias.type_node)
                            && self.is_cross_file_circular_alias(sym_id, alias_type))
                        || (params.is_empty()
                            && self.is_non_generic_mapped_type_circular(
                                sym_id,
                                type_alias.type_node,
                            )));
                if is_circular && !self.has_parse_errors() {
                    use crate::diagnostics::{
                        diagnostic_codes, diagnostic_messages, format_message,
                    };

                    // Mark this alias as circular so downstream checks (TS2313)
                    // can detect constraints referencing it.
                    self.ctx.circular_type_aliases.insert(sym_id);

                    // Suppress TS2456 when the file has parse errors — the
                    // circularity may be an artifact of parser recovery (e.g.,
                    // malformed type parameter lists).  tsc does the same:
                    // it skips semantic circularity errors when syntax errors
                    // are present.
                    // Suppress TS2456 when:
                    // 1. The file has parse errors (syntax errors take priority)
                    // 2. The type alias has an import alias partner — the apparent
                    //    circularity is caused by the name conflict (TS2440 will
                    //    be emitted instead during statement checking).
                    let has_import_partner = self
                        .ctx
                        .binder
                        .alias_partners
                        .get(&sym_id)
                        .and_then(|&partner_id| self.ctx.binder.get_symbol(partner_id))
                        .is_some_and(|partner| partner.flags & symbol_flags::ALIAS != 0);
                    // tsc's hasParseDiagnostics() checks ALL parse diagnostics
                    // (including grammar checks like TS1359) to suppress TS2456.
                    // Our has_parse_errors only tracks "real" syntax errors, so
                    // we also check all_parse_error_positions which includes
                    // non-suppressing parse errors like TS1359.
                    let file_has_any_parse_diag =
                        self.has_parse_errors() || !self.ctx.all_parse_error_positions.is_empty();
                    if !file_has_any_parse_diag && !has_import_partner {
                        let name = escaped_name;
                        let message = format_message(
                            diagnostic_messages::TYPE_ALIAS_CIRCULARLY_REFERENCES_ITSELF,
                            &[&name],
                        );
                        // Point at the type alias name, not the entire declaration
                        self.error_at_node(
                            type_alias.name,
                            &message,
                            diagnostic_codes::TYPE_ALIAS_CIRCULARLY_REFERENCES_ITSELF,
                        );
                    }
                    let def_id = self.ctx.get_or_create_def_id(sym_id);
                    if !params.is_empty() {
                        self.ctx.insert_def_type_params(def_id, params.clone());
                    }
                    self.ctx
                        .definition_store
                        .register_type_to_def(alias_type, def_id);
                    self.ctx.definition_store.set_body(def_id, alias_type);
                    // Preserve the raw alias body so downstream consumers can
                    // continue to reason about the surrounding type graph (for
                    // example, recursive base-type diagnostics) without
                    // collapsing the branch to ERROR or hiding the original
                    // meta-type structure behind a self-lazy placeholder.
                    return (alias_type, params);
                }

                // CRITICAL FIX: Always create DefId for type aliases, not just when they have type parameters
                // This enables Lazy type resolution via TypeResolver during narrowing operations
                let def_id = self.ctx.get_or_create_def_id(sym_id);

                // Cache type parameters for Application expansion (Priority 1 fix)
                // This enables ExtractState<NumberReducer> to expand correctly
                if !params.is_empty() {
                    self.ctx.insert_def_type_params(def_id, params.clone());
                }

                // Register the object shape so diagnostics can display the type alias
                // name (e.g., "Square") instead of the structural type (e.g.,
                // "{ size: number; kind: \"sq\" }"). Mirrors the interface path above.
                if let Some(shape) = type_environment::object_shape(self.ctx.types, alias_type) {
                    self.ctx.definition_store.set_instance_shape(def_id, shape);
                }

                // Return the params that were used during lowering - this ensures
                // type_env gets the same TypeIds as the type body
                return (alias_type, params);
            }
            return (TypeId::UNKNOWN, Vec::new());
        }

        // Class property declarations: resolve type from annotation or initializer.
        if flags & symbol_flags::PROPERTY != 0
            && let Some(node) = self.ctx.arena.get(value_decl)
            && node.kind == syntax_kind_ext::PROPERTY_DECLARATION
            && let Some(prop_decl) = self.ctx.arena.get_property_decl(node)
        {
            if prop_decl.type_annotation.is_some() {
                let annotation_type = self.get_type_from_type_node(prop_decl.type_annotation);
                return (annotation_type, Vec::new());
            }
            if let Some(jsdoc_type) = self.jsdoc_type_annotation_for_node(value_decl) {
                return (jsdoc_type, Vec::new());
            }
            if prop_decl.initializer.is_some() {
                let init_type = self.get_type_of_node(prop_decl.initializer);
                return (init_type, Vec::new());
            }
        }

        // Variable - get type from annotation or infer from initializer
        if flags & (symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::BLOCK_SCOPED_VARIABLE)
            != 0
        {
            let mut resolved_value_decl = value_decl;

            // When a function-scoped `var` redeclares a parameter (`function f(x: A) { var x: B; }`),
            // TypeScript keeps the parameter's original value surface for later identifier reads
            // and reports the mismatch through TS2403 instead of mutating the live symbol type.
            // Preserve that by preferring the merged parameter declaration for symbol-type reads.
            if let Some(param_decl) = declarations.iter().copied().find(|&decl_idx| {
                self.ctx
                    .arena
                    .get(decl_idx)
                    .is_some_and(|node| node.kind == syntax_kind_ext::PARAMETER)
            }) {
                resolved_value_decl = param_decl;
            }

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
                        let annotation_type =
                            self.get_type_from_type_node(var_decl.type_annotation);
                        // `const k: unique symbol = Symbol()` — create a proper UniqueSymbol
                        // type using the variable's binder symbol as identity.
                        if annotation_type == TypeId::SYMBOL
                            && self.is_const_variable_declaration(resolved_value_decl)
                            && self.is_unique_symbol_type_annotation(var_decl.type_annotation)
                        {
                            return (
                                self.ctx
                                    .types
                                    .unique_symbol(tsz_solver::SymbolRef(sym_id.0)),
                                Vec::new(),
                            );
                        }
                        return (annotation_type, Vec::new());
                    }
                    if let Some(jsdoc_type) =
                        self.jsdoc_type_annotation_for_node(resolved_value_decl)
                    {
                        return (jsdoc_type, Vec::new());
                    }
                    if var_decl.initializer.is_some()
                        && self.is_const_variable_declaration(resolved_value_decl)
                    {
                        // In JS files, parenthesized expressions may carry JSDoc
                        // type casts (e.g., `/** @type {*} */(null)` → any).
                        // The cast type overrides the inner literal, so compute
                        // the full initializer type first and use it when it's
                        // `any` or `unknown` (assertion results).
                        if self.ctx.is_js_file()
                            && self.ctx.arena.get(var_decl.initializer).is_some_and(|n| {
                                n.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                            })
                        {
                            let init_type = self.get_type_of_node(var_decl.initializer);
                            if init_type == TypeId::ANY || init_type == TypeId::UNKNOWN {
                                return (init_type, Vec::new());
                            }
                        }
                        if let Some(literal_type) =
                            self.literal_type_from_initializer(var_decl.initializer)
                        {
                            let literal_type = if self.ctx.is_js_file() {
                                self.augment_object_type_with_define_properties(
                                    &escaped_name,
                                    literal_type,
                                )
                            } else {
                                literal_type
                            };
                            return (literal_type, Vec::new());
                        }
                    }
                    // `const k = Symbol()` — infer unique symbol type.
                    // In TypeScript, const declarations initialized with Symbol() get
                    // a unique symbol type (typeof k), not the general `symbol` type.
                    if var_decl.initializer.is_some()
                        && self.is_const_variable_declaration(resolved_value_decl)
                        && self.is_symbol_call_initializer(var_decl.initializer)
                    {
                        return (
                            self.ctx
                                .types
                                .unique_symbol(tsz_solver::SymbolRef(sym_id.0)),
                            Vec::new(),
                        );
                    }
                    // Fall back to inferring from initializer
                    if var_decl.initializer.is_some() {
                        let mut inferred_type = self.get_type_of_node(var_decl.initializer);
                        // Eagerly evaluate Application types (e.g., merge<A, B>)
                        // to concrete types. Without this, long chains like
                        //   const o50 = merge(merge(merge(...)))
                        // store deeply-nested Application trees that cause O(2^N)
                        // traversal work in subsequent type operations (inference,
                        // contains_type_parameters, ensure_application_symbols_resolved).
                        if is_generic_type(self.ctx.types, inferred_type) {
                            inferred_type = self.evaluate_application_type(inferred_type);
                        }
                        inferred_type = self.augment_callable_type_with_expandos(
                            &escaped_name,
                            sym_id,
                            inferred_type,
                        );
                        if self.ctx.is_js_file() {
                            inferred_type = self.augment_object_type_with_define_properties(
                                &escaped_name,
                                inferred_type,
                            );
                        }
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
                            && array_element_type(self.ctx.types, inferred_type)
                                == Some(TypeId::NEVER)
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
                            // Route null/undefined widening through the flow observation boundary.
                            let final_type = flow_boundary::widen_null_undefined_to_any(
                                self.ctx.types,
                                widened_type,
                                self.ctx.strict_null_checks(),
                            );
                            return (final_type, Vec::new());
                        }
                        return (inferred_type, Vec::new());
                    }

                    // For-of/for-in loop variable: no syntactic initializer, but the
                    // type comes from the iterable expression.  Eagerly compute the
                    // element type so that definite-assignment analysis (TS2454) sees
                    // the real type instead of the fallback `any`.
                    if let Some(for_element_type) =
                        self.compute_for_in_of_variable_type(resolved_value_decl)
                    {
                        let widened = if !self.ctx.compiler_options.sound_mode {
                            crate::query_boundaries::common::widen_freshness(
                                self.ctx.types,
                                for_element_type,
                            )
                        } else {
                            for_element_type
                        };
                        return (widened, Vec::new());
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
                    // Check for inline JSDoc @type on the parameter itself
                    if let Some(jsdoc_type) =
                        self.jsdoc_type_annotation_for_node(resolved_value_decl)
                    {
                        return (jsdoc_type, Vec::new());
                    }
                    // In JS files, check the parent function's JSDoc for @param {Type} name.
                    // The @param tag lives on the function declaration, not on the parameter.
                    if self.is_js_file() {
                        let pname = self.parameter_name_for_error(param.name);
                        // Walk up the parent chain to find the enclosing function node
                        // AST structure: Parameter -> SyntaxList -> FunctionDeclaration
                        let mut current = resolved_value_decl;
                        for _ in 0..4 {
                            if let Some(ext) = self.ctx.arena.get_extended(current)
                                && ext.parent.is_some()
                            {
                                current = ext.parent;
                                if let Some(comment_start) =
                                    self.get_jsdoc_comment_pos_for_function(current)
                                    && let Some(func_jsdoc) = self.get_jsdoc_for_function(current)
                                    && let Some(jsdoc_type) = self
                                        .resolve_jsdoc_param_type_with_pos(
                                            &func_jsdoc,
                                            &pname,
                                            Some(comment_start),
                                        )
                                {
                                    return (jsdoc_type, Vec::new());
                                }
                            } else {
                                break;
                            }
                        }
                    }
                    if let Some(contextual_type) =
                        self.contextual_parameter_type_from_enclosing_function(resolved_value_decl)
                    {
                        return (contextual_type, Vec::new());
                    }
                    // Fall back to inferring from initializer (default value)
                    if param.initializer.is_some() {
                        return (self.get_type_of_node(param.initializer), Vec::new());
                    }
                }
            }
            // Binding element from variable declaration destructuring:
            // `let { a, ...rest } = expr` — resolve element type from initializer.
            if resolved_value_decl.is_some()
                && let Some(t) = self.resolve_binding_element_from_variable_initializer(
                    resolved_value_decl,
                    &escaped_name,
                )
            {
                return (t, Vec::new());
            }
            if resolved_value_decl.is_some()
                && let Some(t) = self.resolve_binding_element_from_annotated_param(
                    resolved_value_decl,
                    &escaped_name,
                )
            {
                return (t, Vec::new());
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
                    if let Some(module_type) = self.commonjs_module_value_type(
                        &module_specifier,
                        Some(self.ctx.current_file_idx),
                    ) {
                        return (module_type, Vec::new());
                    }

                    // Resolve the canonical export surface (module-specifier variants,
                    // cross-file tables, and export= member merging).
                    let exports_table = self.resolve_effective_module_exports(&module_specifier);

                    if let Some(exports_table) = exports_table {
                        // Create an object type with all the module's exports
                        use tsz_solver::PropertyInfo;
                        let module_is_non_module_entity = self
                            .ctx
                            .module_resolves_to_non_module_entity(&module_specifier);
                        // Record cross-file symbol targets so delegate_cross_arena_symbol_resolution
                        // can find the correct arena for symbols from ambient modules.
                        for (name, &sym_id) in exports_table.iter() {
                            self.record_cross_file_symbol_if_needed(
                                sym_id,
                                name,
                                &module_specifier,
                            );
                        }
                        let exports_table_target =
                            exports_table.iter().find_map(|(_, &export_sym_id)| {
                                self.ctx.resolve_symbol_file_index(export_sym_id)
                            });
                        let mut export_equals_type = exports_table
                            .get("export=")
                            .map(|export_equals_sym| self.get_type_of_symbol(export_equals_sym));
                        let surface = exports_table_target
                            .map(|target_idx| self.resolve_js_export_surface(target_idx))
                            .or_else(|| {
                                self.resolve_js_export_surface_for_module(
                                    &module_specifier,
                                    Some(self.ctx.current_file_idx),
                                )
                            });
                        let mut props: Vec<PropertyInfo> = if surface
                            .as_ref()
                            .is_some_and(|s| s.has_commonjs_exports)
                        {
                            let mut named_exports = surface
                                .as_ref()
                                .map(|s| s.named_exports.clone())
                                .unwrap_or_default();
                            for (order, prop) in named_exports.iter_mut().enumerate() {
                                prop.declaration_order = order as u32;
                            }
                            if let Some(surface_direct_type) =
                                surface.as_ref().and_then(|s| s.direct_export_type)
                            {
                                export_equals_type = Some(surface_direct_type);
                            }
                            named_exports
                        } else {
                            let mut props: Vec<PropertyInfo> = Vec::new();
                            for (name, &sym_id) in exports_table.iter() {
                                if name == "export=" {
                                    continue;
                                }
                                // Skip type-only, wildcard-type-only, value-less, and
                                // transitively type-only exports (e.g., re-exported from
                                // a module that uses `export type { X }`).
                                if self.is_type_only_export_symbol(sym_id)
                                    || self
                                        .is_export_from_type_only_wildcard(&module_specifier, name)
                                    || self.export_symbol_has_no_value(sym_id)
                                    || self.is_export_type_only_from_file(
                                        &module_specifier,
                                        name,
                                        None,
                                    )
                                {
                                    continue;
                                }
                                let mut prop_type = self.get_type_of_symbol(sym_id);
                                prop_type = self.apply_module_augmentations(
                                    &module_specifier,
                                    name,
                                    prop_type,
                                );
                                let name_atom = self.ctx.types.intern_string(name);
                                props.push(PropertyInfo {
                                    name: name_atom,
                                    type_id: prop_type,
                                    write_type: prop_type,
                                    optional: false,
                                    readonly: false,
                                    is_method: false,
                                    is_class_prototype: false,
                                    visibility: Visibility::Public,
                                    parent_id: None,
                                    declaration_order: 0,
                                });
                            }
                            props
                        };

                        if !module_is_non_module_entity {
                            for aug_name in
                                self.collect_module_augmentation_names(&module_specifier)
                            {
                                let name_atom = self.ctx.types.intern_string(&aug_name);
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
                                    is_class_prototype: false,
                                    visibility: Visibility::Public,
                                    parent_id: None,
                                    declaration_order: 0,
                                });
                            }
                        }
                        let namespace_type = factory.object(props);
                        // Store display name for error messages: TSC shows namespace
                        // types as `typeof import("module")` in diagnostics.
                        let display_module_name = self.resolve_namespace_display_module_name(
                            &exports_table,
                            &module_specifier,
                        );
                        self.ctx
                            .namespace_module_names
                            .insert(namespace_type, display_module_name);
                        if let Some(export_equals_type) = export_equals_type {
                            if module_is_non_module_entity {
                                return (export_equals_type, Vec::new());
                            }
                            return (
                                factory.intersection2(export_equals_type, namespace_type),
                                Vec::new(),
                            );
                        }

                        return (namespace_type, Vec::new());
                    }
                    // Use unified JS export surface for CommonJS fallback
                    if let Some(surface) = self.resolve_js_export_surface_for_module(
                        &module_specifier,
                        Some(self.ctx.current_file_idx),
                    ) && surface.has_commonjs_exports
                    {
                        let display_name =
                            self.imported_namespace_display_module_name(&module_specifier);
                        if let Some(type_id) =
                            surface.to_type_id_with_display_name(self, Some(display_name))
                        {
                            return (type_id, Vec::new());
                        }
                    }
                    self.emit_module_not_found_error(&module_specifier, value_decl);
                    return (TypeId::ANY, Vec::new());
                }
                // Not a require() call — try qualified symbol resolution
                // for `import x = ns.member` patterns.
                if let Some(target_sym) = self.resolve_qualified_symbol(import.module_specifier) {
                    // When the target has both TYPE_ALIAS and VALUE flags
                    // (e.g., `import X = NS.Foo` where NS has both `type Foo = ...` and
                    // `const Foo = ...`), cache the type alias body for type contexts and
                    // return the value type. This mirrors the TYPE_ALIAS + ALIAS merge
                    // logic used for ES6 imports.
                    let target_info = self
                        .get_symbol_globally(target_sym)
                        .map(|s| (s.flags, s.value_declaration));
                    if let Some((tflags, vd)) = target_info
                        && tflags & symbol_flags::TYPE_ALIAS != 0
                        && tflags & symbol_flags::VALUE != 0
                    {
                        let ta_type = self.get_type_of_symbol(target_sym);
                        self.ctx.import_type_alias_types.insert(sym_id, ta_type);
                        let val_type = self.type_of_value_declaration_for_symbol(target_sym, vd);
                        return (val_type, Vec::new());
                    }
                    return (self.get_type_of_symbol(target_sym), Vec::new());
                }
                // Namespace import failed to resolve
                // Check for TS2694 (Namespace has no exported member) or TS2304 (Cannot find name)
                // This happens when: import Alias = NS.NotExported (where NotExported is not exported)

                // 1. Check for TS2694 (Namespace has no exported member)
                // But suppress when the left part resolves to a pure interface that
                // shadows an outer namespace which has the member (tsc uses namespace
                // meaning for import-equals entity name resolution).
                let suppress_ts2694 =
                    self.check_import_qualified_shadows_namespace(import.module_specifier);
                if !suppress_ts2694
                    && self.report_type_query_missing_member(import.module_specifier)
                {
                    return (TypeId::ERROR, Vec::new());
                }

                // 2. Check for TS2304 (Cannot find name) for the left-most part
                if let Some(missing_idx) = self.missing_type_query_left(import.module_specifier) {
                    // Suppress if it's an unresolved import (TS2307 already emitted)
                    if !self.is_unresolved_import_symbol(missing_idx)
                        && let Some(name) = self.entity_name_text(missing_idx)
                    {
                        // Route through boundary for TS2304/TS2552 with suggestion collection
                        self.report_not_found_at_boundary(
                            &name,
                            missing_idx,
                            crate::query_boundaries::name_resolution::NameLookupKind::Value,
                        );
                    }
                    return (TypeId::ERROR, Vec::new());
                }

                // Return ERROR for other cases to prevent cascading errors
                return (TypeId::ERROR, Vec::new());
            }

            // Synthetic `export default ...` aliases point directly at the exported
            // declaration node (often an anonymous class/function). They are real
            // value symbols, not unresolved imports, so compute the declaration type
            // instead of falling through to the generic alias-any path.
            if import_module.is_none()
                && value_decl.is_some()
                && let Some(node) = self.ctx.arena.get(value_decl)
                && node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                return (
                    self.type_of_value_declaration_for_symbol(sym_id, value_decl),
                    Vec::new(),
                );
            }

            // Handle ES6 named imports (import { X } from './module')
            // Use the import_module field to resolve to the actual export
            // Check if this symbol has import tracking metadata

            // For ES6 imports with import_module set, resolve using module_exports
            if let Some(ref module_name) = import_module {
                let import_source_file_idx = self
                    .ctx
                    .binder
                    .get_symbol(sym_id)
                    .and_then(|symbol| {
                        (symbol.decl_file_idx != u32::MAX).then_some(symbol.decl_file_idx as usize)
                    })
                    .or(Some(self.ctx.current_file_idx));

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

                // Check if this is a namespace import (import * as ns) or
                // namespace re-export (export * as ns from 'mod').
                // Namespace imports have import_name = None, namespace
                // re-exports have import_name = Some("*").
                if import_name.is_none() || import_name.as_deref() == Some("*") {
                    if let Some(json_namespace_type) = self
                        .json_module_namespace_type_for_module(module_name, import_source_file_idx)
                    {
                        return (json_namespace_type, Vec::new());
                    }

                    // This is a namespace import: import * as ns from 'module'
                    // Create an object type containing all module exports

                    // Guard: if we're already computing this module's namespace type,
                    // we've hit a circular module import (e.g. prop-types <-> react).
                    // Return `any` to break the cycle, matching tsc's behavior for
                    // circular module references.
                    if self
                        .ctx
                        .module_namespace_resolution_set
                        .contains(module_name)
                    {
                        return (TypeId::ANY, Vec::new());
                    }
                    self.ctx
                        .module_namespace_resolution_set
                        .insert(module_name.to_string());

                    // For cross-file symbols (e.g., `export * as ns from './b'` in
                    // another file), the module_name is relative to the declaring file,
                    // not the current file. Use the symbol's declaring file for resolution.
                    let declaring_file_idx = self.ctx.resolve_symbol_file_index(sym_id);
                    let exports_table = self.resolve_effective_module_exports_from_file(
                        module_name,
                        declaring_file_idx,
                    );
                    if let Some(exports_table) = exports_table {
                        // Record cross-file symbol targets for all symbols in the table
                        for (name, &sym_id) in exports_table.iter() {
                            self.record_cross_file_symbol_if_needed(sym_id, name, module_name);
                        }

                        use tsz_solver::PropertyInfo;
                        let module_is_non_module_entity =
                            self.ctx.module_resolves_to_non_module_entity(module_name);
                        let exports_table_target =
                            exports_table.iter().find_map(|(_, &export_sym_id)| {
                                self.ctx.resolve_symbol_file_index(export_sym_id)
                            });
                        let mut export_equals_type = exports_table
                            .get("export=")
                            .map(|export_equals_sym| self.get_type_of_symbol(export_equals_sym));
                        let surface = exports_table_target
                            .map(|target_idx| self.resolve_js_export_surface(target_idx))
                            .or_else(|| {
                                self.resolve_js_export_surface_for_module(
                                    module_name,
                                    declaring_file_idx,
                                )
                            });
                        let mut props: Vec<PropertyInfo> = if surface
                            .as_ref()
                            .is_some_and(|s| s.has_commonjs_exports)
                        {
                            let mut named_exports = surface
                                .as_ref()
                                .map(|s| s.named_exports.clone())
                                .unwrap_or_default();
                            for (order, prop) in named_exports.iter_mut().enumerate() {
                                prop.declaration_order = order as u32;
                            }
                            if export_equals_type.is_none() {
                                export_equals_type =
                                    surface.as_ref().and_then(|s| s.direct_export_type);
                            }
                            named_exports
                        } else {
                            let mut props: Vec<PropertyInfo> = Vec::new();
                            for (name, &export_sym_id) in exports_table.iter() {
                                if name == "export=" {
                                    continue;
                                }
                                // Skip type-only exports (`export type { A }`), exports
                                // reached through `export type *` wildcards, exports
                                // that are intrinsically type-only (type aliases, interfaces
                                // without merged values), and transitively type-only
                                // exports (re-exported from a `export type` chain).
                                if self.is_type_only_export_symbol(export_sym_id)
                                    || self.is_export_from_type_only_wildcard(module_name, name)
                                    || self.export_symbol_has_no_value(export_sym_id)
                                    || self.is_export_type_only_from_file(
                                        module_name,
                                        name,
                                        declaring_file_idx,
                                    )
                                {
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
                                    is_class_prototype: false,
                                    visibility: Visibility::Public,
                                    parent_id: None,
                                    declaration_order: 0,
                                });
                            }
                            props
                        };

                        // Add augmentation declarations that introduce entirely new names.
                        // If the target resolves to a non-module export= value, these names
                        // are invalid and should not be surfaced on the namespace.
                        if !module_is_non_module_entity {
                            for aug_name in self.collect_module_augmentation_names(module_name) {
                                let name_atom = self.ctx.types.intern_string(&aug_name);
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
                                    is_class_prototype: false,
                                    visibility: Visibility::Public,
                                    parent_id: None,
                                    declaration_order: 0,
                                });
                            }
                        }

                        if let Some(source_idx) = declaring_file_idx.or(import_source_file_idx)
                            && let Some(umd_name) =
                                self.resolve_umd_namespace_name_for_module(module_name, source_idx)
                        {
                            for (name, member_sym_id) in
                                self.collect_namespace_exports_across_binders(&umd_name)
                            {
                                let name_atom = self.ctx.types.intern_string(&name);
                                if props.iter().any(|p| p.name == name_atom) {
                                    continue;
                                }

                                let prop_type = self.get_type_of_symbol(member_sym_id);
                                props.push(PropertyInfo {
                                    name: name_atom,
                                    type_id: prop_type,
                                    write_type: prop_type,
                                    optional: false,
                                    readonly: false,
                                    is_method: false,
                                    is_class_prototype: false,
                                    visibility: Visibility::Public,
                                    parent_id: None,
                                    declaration_order: 0,
                                });
                            }
                        }

                        // When esModuleInterop / allowSyntheticDefaultImports is
                        // enabled and the module uses `export =`, synthesize a
                        // "default" property on the namespace so that
                        // `ns.default` resolves to the export= value.
                        if let Some(eq_type) = export_equals_type
                            && self.ctx.allow_synthetic_default_imports()
                        {
                            let default_atom = self.ctx.types.intern_string("default");
                            if !props.iter().any(|p| p.name == default_atom) {
                                props.push(PropertyInfo {
                                    name: default_atom,
                                    type_id: eq_type,
                                    write_type: eq_type,
                                    optional: false,
                                    readonly: false,
                                    is_method: false,
                                    is_class_prototype: false,
                                    visibility: Visibility::Public,
                                    parent_id: None,
                                    declaration_order: 0,
                                });
                            }
                        }

                        let namespace_type = factory.object(props);
                        // Store display name for error messages: TSC shows namespace
                        // types as `typeof import("module")` in diagnostics.
                        let preserve_namespace_display =
                            !(module_is_non_module_entity
                                && self.ctx.allow_synthetic_default_imports());
                        if preserve_namespace_display {
                            self.ctx.namespace_module_names.insert(
                                namespace_type,
                                self.imported_namespace_display_module_name(module_name),
                            );
                        }
                        self.ctx.module_namespace_resolution_set.remove(module_name);
                        if let Some(export_equals_type) = export_equals_type {
                            if module_is_non_module_entity {
                                // For namespace imports of `export =` values under
                                // esModuleInterop/allowSyntheticDefaultImports, tsc
                                // exposes the namespace object shape (with synthetic
                                // `default`) instead of the callable export target.
                                if self.ctx.allow_synthetic_default_imports() {
                                    return (namespace_type, Vec::new());
                                }
                                return (export_equals_type, Vec::new());
                            }
                            return (
                                factory.intersection2(export_equals_type, namespace_type),
                                Vec::new(),
                            );
                        }

                        return (namespace_type, Vec::new());
                    }
                    // Module not found - emit TS2307 error and return ANY
                    // TypeScript treats unresolved imports as `any` to avoid cascading errors
                    self.ctx.module_namespace_resolution_set.remove(module_name);
                    self.emit_module_not_found_error(module_name, value_decl);
                    return (TypeId::ANY, Vec::new());
                }

                // This is a named import: import { X } from 'module'
                // Use import_name if set (for renamed imports), otherwise use escaped_name
                let export_name = import_name.as_ref().unwrap_or(&escaped_name);

                if export_name == "default"
                    && let Some(json_type) =
                        self.json_module_type_for_module(module_name, import_source_file_idx)
                {
                    return (json_type, Vec::new());
                }

                // Check if the module exists first (for proper error differentiation)
                let module_exists = self.ctx.binder.module_exports.contains_key(module_name)
                    || self.module_exists_cross_file(module_name);
                if self.is_ambient_module_match(module_name) && !module_exists {
                    return (TypeId::ANY, Vec::new());
                }

                // First, try local binder's module_exports
                let cross_file_result = self.resolve_cross_file_export(module_name, export_name);
                let export_sym_id = cross_file_result
                    .or_else(|| {
                        self.ctx
                            .binder
                            .module_exports
                            .get(module_name)
                            .and_then(|exports_table| exports_table.get(export_name))
                    })
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

                    // TYPE_ALIAS + ALIAS merge: cache type alias body for type contexts,
                    // return namespace type for value contexts.
                    if let Some(&alias_id) = self.ctx.binder.alias_partners.get(&export_sym_id) {
                        let ta_type = self.get_type_of_symbol(export_sym_id);
                        self.ctx.import_type_alias_types.insert(sym_id, ta_type);
                        self.record_cross_file_symbol_if_needed(alias_id, export_name, module_name);
                        let mut result = self.get_type_of_symbol(alias_id);
                        result = self.apply_module_augmentations(module_name, export_name, result);
                        self.ctx.symbol_types.insert(export_sym_id, result);
                        return (result, Vec::new());
                    }

                    // When the export symbol has both INTERFACE and VARIABLE
                    // flags (e.g., `interface MyFunction` + `export const
                    // MyFunction`), `get_type_of_symbol` returns the interface
                    // type because INTERFACE is checked first. For import
                    // aliases (value position), we need the variable type so
                    // the imported binding is callable/constructable.
                    let mut result = if let Some(sym) = self.get_symbol_globally(export_sym_id) {
                        let has_interface = sym.flags & symbol_flags::INTERFACE != 0;
                        let has_variable = sym.flags
                            & (symbol_flags::FUNCTION_SCOPED_VARIABLE
                                | symbol_flags::BLOCK_SCOPED_VARIABLE)
                            != 0;
                        if has_interface && has_variable && !sym.value_declaration.is_none() {
                            let vd = sym.value_declaration;
                            let vd_type =
                                self.type_of_value_declaration_for_symbol(export_sym_id, vd);
                            if vd_type != TypeId::UNKNOWN && vd_type != TypeId::ERROR {
                                vd_type
                            } else {
                                self.get_type_of_symbol(export_sym_id)
                            }
                        } else {
                            self.get_type_of_symbol(export_sym_id)
                        }
                    } else {
                        self.get_type_of_symbol(export_sym_id)
                    };
                    result = self.apply_module_augmentations(module_name, export_name, result);
                    self.ctx.symbol_types.insert(export_sym_id, result);
                    return (result, Vec::new());
                }

                // Use the unified JS export surface for CommonJS named export lookup.
                if let Some(result) = self.resolve_js_export_named_type(
                    module_name,
                    export_name,
                    Some(self.ctx.current_file_idx),
                ) {
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
                            self.emit_no_default_export_error(module_name, value_decl, false);
                            return (TypeId::ERROR, Vec::new());
                        }

                        // For missing default exports, check_imported_members already
                        // emits TS1192 for positional default imports (`import X from "mod"`).
                        // Don't emit a duplicate TS2305 here — just return ERROR and let
                        // the import checker handle the diagnostic.
                        if !self.ctx.allow_synthetic_default_imports() {
                            tracing::debug!(
                                "default export missing and allowSyntheticDefaultImports is false, returning ERROR (TS1192 handled by import checker)"
                            );
                            return (TypeId::ERROR, Vec::new());
                        }

                        // For default imports without a default export, only
                        // synthesize a namespace fallback for CommonJS-shaped
                        // modules. Pure ESM modules must still report TS1192.
                        if self.module_can_use_synthetic_default_import(module_name) {
                            // Same circular module guard as namespace imports above
                            if self
                                .ctx
                                .module_namespace_resolution_set
                                .contains(module_name)
                            {
                                return (TypeId::ANY, Vec::new());
                            }
                            self.ctx
                                .module_namespace_resolution_set
                                .insert(module_name.to_string());

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
                                        is_class_prototype: false,
                                        visibility: Visibility::Public,
                                        parent_id: None,
                                        declaration_order: 0,
                                    });
                                }
                                let module_type = factory.object(props);
                                self.ctx.namespace_module_names.insert(
                                    module_type,
                                    self.imported_namespace_display_module_name(module_name),
                                );
                                self.ctx.module_namespace_resolution_set.remove(module_name);
                                return (module_type, Vec::new());
                            }
                            self.ctx.module_namespace_resolution_set.remove(module_name);
                        }
                        return (TypeId::ERROR, Vec::new());
                    } else {
                        // TS2305/TS2614: Module has no exported member.
                        // Before emitting, try a type-level resolution for `export =`
                        // modules where the member may be a key of a mapped type.
                        if export_name != "*" {
                            let found_via_export_equals_type = self
                                .try_resolve_named_export_via_export_equals_type(
                                    module_name,
                                    export_name,
                                );
                            if let Some(prop_type) = found_via_export_equals_type {
                                return (prop_type, Vec::new());
                            }
                            let import_specifier_decl = declarations.iter().copied().find(|&decl| {
                                self.ctx.arena.get(decl).is_some_and(|node| {
                                    node.kind == tsz_parser::parser::syntax_kind_ext::IMPORT_SPECIFIER
                                })
                            });
                            if self.ctx.arena.get(value_decl).is_some_and(|node| {
                                node.kind == tsz_parser::parser::syntax_kind_ext::IMPORT_SPECIFIER
                            }) {
                                return (TypeId::ERROR, Vec::new());
                            }
                            self.emit_no_exported_member_error(
                                module_name,
                                export_name,
                                import_specifier_decl.unwrap_or(value_decl),
                            );
                        }
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
}
