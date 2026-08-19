//! Heritage and interface merging plus final-type construction for class
//! instance type resolution.
//!
//! These phases run after the member-collection phases in
//! [`super::instance`]: they merge base-class members and class/interface
//! declarations into the shared [`ClassInstanceBuilder`], then build and
//! register the final instance type. Pure code motion out of the original
//! `get_class_instance_type_inner`; the early-return/cleanup semantics of the
//! base-member merge are preserved exactly.

use super::helpers::{can_skip_base_instantiation, declaration_is_module_augmentation};
use super::instance::ClassInstanceBuilder;
use super::walk_state::ClassInstanceWalkState;
use crate::query_boundaries::class_type::{
    callable_shape_for_type, final_class_instance_type, object_shape_for_type,
};
use crate::query_boundaries::common::{TypeSubstitution, instantiate_type};
use crate::state::CheckerState;
use tsz_lowering::TypeLowering;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_solver::{PropertyInfo, TypeId};

impl CheckerState<'_> {
    /// Merge base class instance properties (derived members take precedence).
    ///
    /// Returns `Some(type)` when a cycle/forward-reference is detected and the
    /// whole `get_class_instance_type_inner` call must early-return that type
    /// (the `did_insert_into_global_set` cleanup is performed inline before
    /// returning, exactly as in the original function).
    pub(super) fn class_instance_merge_base_members<'b>(
        &mut self,
        class: &'b tsz_parser::parser::node::ClassData,
        walk_state: &mut ClassInstanceWalkState,
        b: &mut ClassInstanceBuilder<'b>,
    ) -> Option<TypeId> {
        let current_sym = b.current_sym;
        let did_insert_into_global_set = b.did_insert_into_global_set();
        // Merge base class instance properties (derived members take precedence).
        // A malformed empty `@augments`/`@extends` tag reports TS8023+TS1003
        // but does NOT override a syntactic `extends` clause — tsc still
        // resolves the base class, so inherited JS this-properties stay
        // visible on the derived class.
        if let Some(ref heritage_clauses) = class.heritage_clauses {
            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }
                let Some(&type_idx) = heritage.types.nodes.first() else {
                    break;
                };
                let Some(type_node) = self.ctx.arena.get(type_idx) else {
                    break;
                };

                let (expr_idx, type_arguments) =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        (
                            expr_type_args.expression,
                            expr_type_args.type_arguments.as_ref(),
                        )
                    } else {
                        (type_idx, None)
                    };

                let base_sym_id = match self.resolve_heritage_symbol(expr_idx) {
                    Some(base_sym_id) => base_sym_id,
                    None => {
                        // Can't resolve symbol (e.g., anonymous class expression like
                        // `class extends class { a = 1 }`), try expression-based resolution
                        if let Some(base_instance_type) =
                            self.base_instance_type_from_expression(expr_idx, type_arguments)
                        {
                            self.record_heritage_extends(current_sym, expr_idx, base_instance_type);
                            tracing::debug!(
                                ?base_instance_type,
                                "heritage: resolved base instance type from expression"
                            );
                            self.merge_base_instance_properties(
                                base_instance_type,
                                &mut b.properties,
                                &mut b.string_index,
                                &mut b.number_index,
                                &mut b.symbol_index,
                            );
                        } else {
                            tracing::debug!(
                                ?expr_idx,
                                "heritage: base_instance_type_from_expression returned None"
                            );
                        }
                        break;
                    }
                };
                let base_class_decl = self.get_class_declaration_from_symbol(base_sym_id);

                // Canonicalize class symbol for cycle guards. Some paths can observe
                // alias/default-export symbols while the active resolution set tracks
                // the declaration symbol; check both to avoid recursion leaks.
                let canonical_base_sym =
                    base_class_decl.and_then(|decl_idx| self.class_declaration_symbol(decl_idx));
                let base_in_resolution_set = self
                    .ctx
                    .class_instance_resolution_set
                    .contains(&base_sym_id)
                    || canonical_base_sym
                        .is_some_and(|sym| self.ctx.class_instance_resolution_set.contains(&sym));
                let base_visited = walk_state.contains_base_symbol(base_sym_id, canonical_base_sym);

                // CRITICAL: Check for self-referential class BEFORE processing
                // This catches class C extends C, class D<T> extends D<T>, etc.
                if let Some(current_sym) = current_sym {
                    if base_sym_id == current_sym || canonical_base_sym == Some(current_sym) {
                        // Self-referential inheritance - stop processing.
                        // TS2506 is emitted by the dedicated cycle detection in
                        // class_inheritance.rs, which anchors at the class name (matching tsc).
                        break;
                    }

                    // CRITICAL: Check global resolution set to prevent infinite recursion.
                    // If the base class is currently being resolved, use its cached
                    // partial type (if available) instead of recursing. This handles
                    // nested class expressions that extend their enclosing class:
                    //   class F { Inner = class extends F { p2 = this.p1 }; p1 = 0 }
                    // F is in the resolution set, but its partial type (from prescan
                    // or phase-2 caching) may be in class_instance_type_cache.
                    // If no partial type is cached, build one from the base class's
                    // declared members (annotated properties and constructor params).
                    if base_in_resolution_set {
                        if let Some(base_class_idx) = base_class_decl {
                            // Copy the cached value out and release the borrow before
                            // the body, which re-enters the checker (and can re-borrow
                            // this same `RefCell`).
                            let cached_partial = self
                                .ctx
                                .class_instance_type_cache
                                .borrow()
                                .get(&base_class_idx)
                                .copied();
                            // The base class is mid-resolution (base<->derived cycle):
                            // we use a partial/prescan type for it instead of recursing.
                            // That partial still carries the base class's own type
                            // parameters (e.g. `_def: Def`), so it must be instantiated
                            // with the heritage type arguments (`Base<DerivedDef>`)
                            // before merging — mirroring the normal non-cycle path
                            // below — or inherited members keep the bare type param
                            // (its constraint) instead of the supplied argument.
                            let partial = if let Some(cached_partial) = cached_partial {
                                Some(cached_partial)
                            } else if let Some(base_node) = self.ctx.arena.get(base_class_idx)
                                && let Some(base_class) = self.ctx.arena.get_class(base_node)
                            {
                                // No cached partial type yet — build a quick prescan
                                // from the base class's declared property types and
                                // constructor parameter properties.
                                let prescan =
                                    self.quick_prescan_class_members(base_class_idx, base_class);
                                (prescan != TypeId::ERROR).then_some(prescan)
                            } else {
                                None
                            };
                            if let Some(partial) = partial {
                                let base_type_parameters = self
                                    .ctx
                                    .arena
                                    .get(base_class_idx)
                                    .and_then(|node| self.ctx.arena.get_class(node))
                                    .and_then(|cls| cls.type_parameters.clone());
                                let instantiated = self
                                    .instantiate_partial_base_with_heritage_args(
                                        partial,
                                        base_type_parameters.as_ref(),
                                        type_arguments,
                                    );
                                self.merge_base_instance_properties(
                                    instantiated,
                                    &mut b.properties,
                                    &mut b.string_index,
                                    &mut b.number_index,
                                    &mut b.symbol_index,
                                );
                            }
                        }
                        break;
                    }
                }

                // Check for circular inheritance using symbol tracking
                if base_visited {
                    break;
                }

                let Some(base_class_idx) = base_class_decl else {
                    // Base class node not found in current arena (cross-file case).
                    // Try to resolve the base class type through the symbol system.
                    // If base class is being resolved, skip to prevent infinite loop
                    if base_in_resolution_set {
                        break;
                    }

                    if let Some(base_instance_type) =
                        self.base_instance_type_from_expression(expr_idx, type_arguments)
                    {
                        self.record_heritage_extends(current_sym, expr_idx, base_instance_type);
                        self.merge_base_instance_properties(
                            base_instance_type,
                            &mut b.properties,
                            &mut b.string_index,
                            &mut b.number_index,
                            &mut b.symbol_index,
                        );
                    }
                    break;
                };

                // Check for circular inheritance using node index tracking (for cross-file cycles)
                // CRITICAL: Return immediately to prevent infinite recursion, not just break
                if walk_state.contains_node(base_class_idx) {
                    if did_insert_into_global_set && let Some(sym_id) = current_sym {
                        self.ctx.class_instance_resolution_set.remove(&sym_id);
                    }
                    return Some(TypeId::ANY); // Cycle detected - break recursion
                }
                let Some(base_node) = self.ctx.arena.get(base_class_idx) else {
                    break;
                };
                let Some(base_class) = self.ctx.arena.get_class(base_node) else {
                    break;
                };

                // CRITICAL: Check global resolution set BEFORE recursing into base class
                // This prevents infinite recursion when we have forward references in cycles
                if let Some(base_class_sym) = self.class_declaration_symbol(base_class_idx) {
                    if self
                        .ctx
                        .class_instance_resolution_set
                        .contains(&base_class_sym)
                    {
                        // Base class is already being resolved up the call stack
                        // Return ANY to break the cycle and stop recursion
                        if did_insert_into_global_set && let Some(sym_id) = current_sym {
                            self.ctx.class_instance_resolution_set.remove(&sym_id);
                        }
                        return Some(TypeId::ANY);
                    }
                } else {
                    // CRITICAL: Forward reference detected (symbol not bound yet)
                    // If we've seen this node before in the current resolution path, it's a cycle
                    // This handles cases like: class C extends E {} where E doesn't exist yet
                    // but will be declared later with extends D, and D extends C
                    if walk_state.contains_node(base_class_idx) {
                        if did_insert_into_global_set && let Some(sym_id) = current_sym {
                            self.ctx.class_instance_resolution_set.remove(&sym_id);
                        }
                        return Some(TypeId::ANY); // Forward reference cycle - break recursion
                    }
                    // Otherwise, continue - the forward reference might resolve later
                }

                let mut type_args = Vec::with_capacity(type_arguments.map_or(0, |a| a.nodes.len()));
                if let Some(args) = type_arguments {
                    for &arg_idx in &args.nodes {
                        type_args.push(self.get_type_from_type_node(arg_idx));
                    }
                }

                // Get the base class instance type.
                // We already resolved a concrete class declaration (`base_class_idx`) above, so
                // we can read through the declaration cache directly and avoid an extra symbol
                // resolution round trip on this hot inheritance path.
                // Copy the cached value out and drop the cache borrow before the
                // fallback closure runs: `get_class_instance_type` re-borrows the
                // same `RefCell`, so holding the read guard across it would panic.
                let cached_base_instance_type = self
                    .ctx
                    .class_instance_type_cache
                    .borrow()
                    .get(&base_class_idx)
                    .copied();
                let base_instance_type = cached_base_instance_type
                    .unwrap_or_else(|| self.get_class_instance_type(base_class_idx, base_class));
                let base_instance_type = self.resolve_lazy_type(base_instance_type);
                let mut base_type_params = Vec::new();
                let base_instance_type = if can_skip_base_instantiation(
                    base_class
                        .type_parameters
                        .as_ref()
                        .map_or(0, |params| params.nodes.len()),
                    type_args.len(),
                ) {
                    base_instance_type
                } else {
                    let (resolved_base_type_params, base_type_param_updates) =
                        self.push_type_parameters(&base_class.type_parameters);
                    base_type_params = resolved_base_type_params;

                    if type_args.len() < base_type_params.len() {
                        for (param_index, param) in
                            base_type_params.iter().enumerate().skip(type_args.len())
                        {
                            let fallback = param
                                .default
                                .or(param.constraint)
                                .unwrap_or(TypeId::UNKNOWN);
                            let substitution = TypeSubstitution::from_args(
                                self.ctx.types,
                                &base_type_params[..param_index],
                                &type_args,
                            );
                            type_args.push(
                                crate::query_boundaries::common::instantiate_type_preserving_meta(
                                    self.ctx.types,
                                    fallback,
                                    &substitution,
                                ),
                            );
                        }
                    }
                    if type_args.len() > base_type_params.len() {
                        type_args.truncate(base_type_params.len());
                    }

                    let substitution =
                        TypeSubstitution::from_args(self.ctx.types, &base_type_params, &type_args);
                    let instantiated =
                        instantiate_type(self.ctx.types, base_instance_type, &substitution);
                    self.pop_type_parameters(base_type_param_updates);
                    instantiated
                };

                let has_structural_self_arg = current_sym.is_some_and(|current_sym| {
                    type_args.iter().copied().any(|arg| {
                        self.type_requires_structure_of_symbol_for_base_type(arg, current_sym)
                    })
                });

                if let Some(current_sym) = current_sym
                    && (has_structural_self_arg
                        || self.type_requires_structure_of_symbol_for_base_type(
                            base_instance_type,
                            current_sym,
                        ))
                {
                    self.report_recursive_base_type_for_symbol(current_sym);
                    self.report_instantiated_type_alias_mapped_constraint_cycles(
                        base_sym_id,
                        &base_type_params,
                        &type_args,
                        current_sym,
                    );
                    if let Some(base_shape) =
                        object_shape_for_type(self.ctx.types, base_instance_type)
                    {
                        for base_prop in &base_shape.properties {
                            b.properties
                                .entry(base_prop.name)
                                .or_insert_with(|| base_prop.clone());
                        }
                        if let Some(idx) = base_shape.string_index_signature().copied() {
                            Self::merge_index_signature(&mut b.string_index, idx);
                        }
                        if let Some(ref idx) = base_shape.number_index {
                            Self::merge_index_signature(&mut b.number_index, *idx);
                        }
                        if let Some(idx) = base_shape.symbol_index_signature().copied() {
                            Self::merge_index_signature(&mut b.symbol_index, idx);
                        }
                    }
                    break;
                }

                if let Some(base_shape) = object_shape_for_type(self.ctx.types, base_instance_type)
                {
                    for base_prop in &base_shape.properties {
                        b.properties
                            .entry(base_prop.name)
                            .or_insert_with(|| base_prop.clone());
                    }
                    if let Some(idx) = base_shape.string_index_signature().copied() {
                        Self::merge_index_signature(&mut b.string_index, idx);
                    }
                    if let Some(ref idx) = base_shape.number_index {
                        Self::merge_index_signature(&mut b.number_index, *idx);
                    }
                    if let Some(idx) = base_shape.symbol_index_signature().copied() {
                        Self::merge_index_signature(&mut b.symbol_index, idx);
                    }
                }

                break;
            }
        }
        None
    }

    /// Instantiate a base class's partial/prescan instance type with the
    /// heritage type arguments from an `extends Base<Args>` clause.
    ///
    /// Used by the base<->derived cycle fallback in
    /// [`Self::class_instance_merge_base_members`], where the base class is
    /// mid-resolution and a partial type is used instead of recursing into its
    /// full instance type. That partial still carries the base class's own type
    /// parameters, so its members (e.g. `_def: Def`) must be substituted with
    /// the supplied arguments (`Base<DerivedDef>` => `Def := DerivedDef`) before
    /// being merged into the derived class, exactly as the normal non-cycle
    /// path does. Without a heritage clause that has fewer/more args than the
    /// base has parameters, defaults/constraints fill or excess args truncate,
    /// mirroring the normal path. When the base is non-generic or no arguments
    /// can apply, the partial is returned unchanged.
    fn instantiate_partial_base_with_heritage_args(
        &mut self,
        partial: TypeId,
        base_type_parameters: Option<&tsz_parser::parser::NodeList>,
        type_arguments: Option<&tsz_parser::parser::NodeList>,
    ) -> TypeId {
        let base_param_count = base_type_parameters.map_or(0, |params| params.nodes.len());
        if base_param_count == 0 {
            // Non-generic base: nothing to substitute.
            return partial;
        }

        let mut type_args = Vec::with_capacity(type_arguments.map_or(0, |a| a.nodes.len()));
        if let Some(args) = type_arguments {
            for &arg_idx in &args.nodes {
                type_args.push(self.get_type_from_type_node(arg_idx));
            }
        }

        if can_skip_base_instantiation(base_param_count, type_args.len()) {
            return partial;
        }

        let base_type_parameters = base_type_parameters.cloned();
        let (base_type_params, base_type_param_updates) =
            self.push_type_parameters(&base_type_parameters);

        // Fill unsupplied trailing parameters from their default/constraint,
        // instantiated against the substitution built so far — mirroring the
        // normal heritage path.
        if type_args.len() < base_type_params.len() {
            for (param_index, param) in base_type_params.iter().enumerate().skip(type_args.len()) {
                let fallback = param
                    .default
                    .or(param.constraint)
                    .unwrap_or(TypeId::UNKNOWN);
                let substitution = TypeSubstitution::from_args(
                    self.ctx.types,
                    &base_type_params[..param_index],
                    &type_args,
                );
                type_args.push(
                    crate::query_boundaries::common::instantiate_type_preserving_meta(
                        self.ctx.types,
                        fallback,
                        &substitution,
                    ),
                );
            }
        }
        if type_args.len() > base_type_params.len() {
            type_args.truncate(base_type_params.len());
        }

        let substitution =
            TypeSubstitution::from_args(self.ctx.types, &base_type_params, &type_args);
        let instantiated = instantiate_type(self.ctx.types, partial, &substitution);
        self.pop_type_parameters(base_type_param_updates);
        instantiated
    }

    /// Merge interface declarations for class/interface merging (class members
    /// take precedence), including cross-arena/lib interface declarations.
    pub(super) fn class_instance_merge_interface_decls(
        &mut self,
        apply_module_augmentations: bool,
        b: &mut ClassInstanceBuilder<'_>,
    ) {
        let current_sym = b.current_sym;
        // Merge interface declarations for class/interface merging (class members take precedence)
        if let Some(sym_id) = current_sym
            && let Some((symbol_flags, symbol_declarations, symbol_name)) =
                self.get_cross_file_symbol(sym_id).map(|symbol| {
                    (
                        symbol.flags,
                        symbol.declarations.clone(),
                        symbol.escaped_name.clone(),
                    )
                })
        {
            let mut merged_symbol_flags = symbol_flags;
            let mut merged_symbol_declarations = symbol_declarations;
            let owner_file_idx = self.ctx.resolve_symbol_file_index(sym_id);

            for &candidate_id in self.ctx.binder.get_symbols().find_all_by_name(&symbol_name) {
                if candidate_id == sym_id
                    || self.ctx.resolve_symbol_file_index(candidate_id) != owner_file_idx
                {
                    continue;
                }
                let Some(candidate_symbol) = self.get_cross_file_symbol(candidate_id) else {
                    continue;
                };
                if !candidate_symbol.has_any_flags(tsz_binder::symbol_flags::INTERFACE) {
                    continue;
                }

                merged_symbol_flags |= candidate_symbol.flags;
                for &decl_idx in &candidate_symbol.declarations {
                    if !merged_symbol_declarations.contains(&decl_idx) {
                        merged_symbol_declarations.push(decl_idx);
                    }
                }
            }

            let interface_decls: Vec<NodeIndex> = merged_symbol_declarations
                .iter()
                .copied()
                .filter(|&decl_idx| {
                    if !apply_module_augmentations
                        && declaration_is_module_augmentation(self.ctx.arena, decl_idx)
                    {
                        return false;
                    }
                    self.ctx
                        .arena
                        .get(decl_idx)
                        .and_then(|node| self.ctx.arena.get_interface(node))
                        .is_some()
                })
                .collect();

            if !interface_decls.is_empty() {
                let type_param_bindings = self.get_type_param_bindings();
                let type_resolver =
                    |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
                let value_resolver =
                    |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
                let lowering = TypeLowering::with_resolvers(
                    self.ctx.arena,
                    self.ctx.types,
                    &type_resolver,
                    &value_resolver,
                )
                .with_type_param_bindings(type_param_bindings);
                let interface_type = lowering.lower_interface_declarations(&interface_decls);
                let interface_type =
                    self.merge_interface_heritage_types(&interface_decls, interface_type);
                b.merged_interface_type_for_class = Some(interface_type);

                if let Some(shape) = object_shape_for_type(self.ctx.types, interface_type) {
                    for prop in &shape.properties {
                        b.properties
                            .entry(prop.name)
                            .or_insert_with(|| prop.clone());
                    }
                    if let Some(idx) = shape.string_index_signature().copied() {
                        Self::merge_index_signature(&mut b.string_index, idx);
                    }
                    if let Some(ref idx) = shape.number_index {
                        Self::merge_index_signature(&mut b.number_index, *idx);
                    }
                    if let Some(idx) = shape.symbol_index_signature().copied() {
                        Self::merge_index_signature(&mut b.symbol_index, idx);
                    }
                } else if let Some(shape) = callable_shape_for_type(self.ctx.types, interface_type)
                {
                    for prop in &shape.properties {
                        b.properties
                            .entry(prop.name)
                            .or_insert_with(|| prop.clone());
                    }
                }
            }

            // When the symbol has INTERFACE flags (class merged with interface) but no
            // local interface declarations were found, the interface declarations live
            // in a lib arena (e.g., user `class TemplateStringsArray {}` merged with
            // built-in `interface TemplateStringsArray extends ReadonlyArray<string>`).
            // Check cross-arena declarations and resolve the lib interface type.
            if interface_decls.is_empty()
                && (merged_symbol_flags & tsz_binder::symbol_flags::INTERFACE) != 0
                && !self.ctx.lib_contexts.is_empty()
            {
                // Check for cross-arena interface declarations
                let mut cross_arena_interface_type: Option<TypeId> = None;
                for &decl_idx in &merged_symbol_declarations {
                    if let Some(arenas) =
                        self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx))
                    {
                        for arena in arenas.iter() {
                            if std::ptr::eq(arena.as_ref(), self.ctx.arena) {
                                continue;
                            }
                            let is_module_augmentation_decl =
                                declaration_is_module_augmentation(arena.as_ref(), decl_idx);
                            if let Some(node) = arena.get(decl_idx)
                                && arena.get_interface(node).is_some()
                                && (apply_module_augmentations || !is_module_augmentation_decl)
                            {
                                let cross_type =
                                    self.lower_cross_file_interface_decl(arena, decl_idx, sym_id);
                                if cross_type != TypeId::ERROR {
                                    cross_arena_interface_type =
                                        Some(if let Some(existing) = cross_arena_interface_type {
                                            self.merge_interface_types(existing, cross_type)
                                        } else {
                                            cross_type
                                        });
                                }
                            }
                        }
                    }
                }

                // Fall back to resolve_lib_type_by_name if no cross-arena decls found
                let lib_interface_type = cross_arena_interface_type
                    .or_else(|| self.resolve_lib_type_by_name(&symbol_name));

                if let Some(interface_type) = lib_interface_type {
                    // Merge heritage types for the lib interface
                    let interface_type = self.merge_cross_file_heritage(
                        &merged_symbol_declarations,
                        sym_id,
                        interface_type,
                    );
                    b.merged_interface_type_for_class = Some(interface_type);

                    if let Some(shape) = object_shape_for_type(self.ctx.types, interface_type) {
                        for prop in &shape.properties {
                            b.properties
                                .entry(prop.name)
                                .or_insert_with(|| prop.clone());
                        }
                        if let Some(idx) = shape.string_index_signature().copied() {
                            Self::merge_index_signature(&mut b.string_index, idx);
                        }
                        if let Some(ref idx) = shape.number_index {
                            Self::merge_index_signature(&mut b.number_index, *idx);
                        }
                        if let Some(idx) = shape.symbol_index_signature().copied() {
                            Self::merge_index_signature(&mut b.symbol_index, idx);
                        }
                    } else if let Some(shape) =
                        callable_shape_for_type(self.ctx.types, interface_type)
                    {
                        for prop in &shape.properties {
                            b.properties
                                .entry(prop.name)
                                .or_insert_with(|| prop.clone());
                        }
                    }
                }
            }
        }
    }

    /// Build the final instance type from the accumulated members, run the
    /// final interface-merge / module-augmentation pass, perform the
    /// resolution-set cleanup, and register the result.
    pub(super) fn class_instance_build_final_type(
        &mut self,
        class_idx: NodeIndex,
        apply_module_augmentations: bool,
        walk_state: &mut ClassInstanceWalkState,
        b: ClassInstanceBuilder<'_>,
    ) -> TypeId {
        let current_sym = b.current_sym;
        let did_insert_into_global_set = b.did_insert_into_global_set();
        // Capture before `b.properties` is moved out via `into_values()` below.
        let has_late_bound_members = b.has_late_bound_members();

        // NOTE: Object prototype members (toString, hasOwnProperty, etc.) are NOT
        // merged into the class instance type. The solver handles these via its own
        // Object prototype fallback (resolve_object_member) during property access.
        // Including them as explicit properties would cause false TS2322 errors when
        // assigning plain objects to class-typed variables, since the plain objects
        // wouldn't have these as own properties.

        // Build the final instance type. `properties` is an FxHashMap whose
        // iteration order is non-deterministic, so sort by `declaration_order`
        // so downstream diagnostics like TS2739 ("missing the following
        // properties: a, b, c") see properties in source-declaration order.
        // Synthesized members carry `declaration_order == 0` and stay first
        // via stable sort.
        let mut props: Vec<PropertyInfo> = b.properties.into_values().collect();
        props.sort_by_key(|p| p.declaration_order);
        let mut instance_type = final_class_instance_type(
            self.ctx.types,
            props,
            b.string_index,
            b.number_index,
            b.symbol_index,
            current_sym,
            has_late_bound_members,
            !apply_module_augmentations,
        );

        // Final interface merging pass
        if let Some(sym_id) = current_sym {
            if let Some(interface_type) = b.merged_interface_type_for_class {
                instance_type =
                    self.merge_class_instance_with_interface(instance_type, interface_type);
            }

            // Apply module augmentations targeting this class's interface name.
            // When another file has `declare module './thisFile' { interface ClassName { ... } }`,
            // those augmented members must be merged into the class instance type so that
            // `ClassName.prototype` and value-position usage see the full merged type.
            if apply_module_augmentations
                && let Some(symbol) = self
                    .ctx
                    .binder
                    .get_symbol(sym_id)
                    .or_else(|| self.get_cross_file_symbol(sym_id))
            {
                let class_name = symbol.escaped_name.clone();
                if let Some(sf) = self.ctx.arena.source_files.first() {
                    let file_name = sf.file_name.clone();
                    instance_type =
                        self.apply_module_augmentations(&file_name, &class_name, instance_type);
                }
            }

            walk_state.leave_class(sym_id, class_idx);
            // Only remove from global set if we inserted it ourselves
            if did_insert_into_global_set {
                self.ctx.class_instance_resolution_set.remove(&sym_id);
                // The instance build window is closed: drop any provisional
                // class-instance registrations (#16055), including the corner
                // where a fields-only class interns its completed instance to
                // the same `TypeId` as the snapshot (publication-keyed
                // deregistration alone would keep that window open forever).
                self.ctx
                    .types
                    .unregister_provisional_class_instances_for_def(
                        self.ctx.get_or_create_def_id(sym_id),
                    );
            }
        }
        // Keep class lookup working for structurally unbranded derived instances.
        self.ctx
            .class_decl_miss_cache
            .borrow_mut()
            .remove(&instance_type);
        self.ctx
            .class_instance_type_to_decl
            .insert(instance_type, class_idx);

        if let Some(sym_id) = current_sym {
            self.register_final_class_instance_type(sym_id, instance_type, &b.class_type_params);
            self.refresh_constructor_instance_return_if_stale(class_idx, sym_id, instance_type);
        }

        self.pop_type_parameters(b.class_type_param_updates);

        instance_type
    }
}
